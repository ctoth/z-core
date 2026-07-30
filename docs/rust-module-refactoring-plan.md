# Rust module ownership refactoring plan

## Status and authority

This is an execution plan for a post-v0.1 structural refactor of the Rust
workspace. It does not reopen or replace the completed build plan in
`PLAN.md`, and it does not add work to `PROGRESS.md`.

Do not execute this plan until Q explicitly authorizes execution. Once
execution starts, this document is the controlling workflow for the refactor.
Execute its slices in order. Do not begin a later slice until the current slice
has either been committed as a kept change or fully reverted.

The plan itself is the required durable record. Before source work begins,
commit only this file:

```text
Add Rust module refactoring plan
```

## Goal

Make module ownership visible in the file layout without changing emulator
behavior, public APIs, save-state bytes, host bindings, dependencies, or
conformance authority.

The refactor is complete when:

1. `z180-core/src/lib.rs` owns crate-level types, the `Z180` state layout,
   construction, lifecycle coordination, and public machine accessors rather
   than every implementation concern.
2. Instruction execution, state serialization, debugging, interrupts,
   bus/internal-I/O access, and each peripheral family have named owner
   modules.
3. `z180-replay`, the SST runner, and the Python and WebAssembly bindings have
   owner modules for their already-distinct responsibilities.
4. Large declarative authorities remain intact: `optable.rs` continues to be
   the single opcode/timing authority and `ioregs.rs` continues to be the
   internal-register authority.
5. Every existing CI command passes from the final source state.

File length is an observation, not the acceptance criterion. A move is kept
only when it creates a truthful ownership boundary.

## Current evidence

The live checkout measured these largest Rust files before plan authoring:

| File | Lines | Decision |
| --- | ---: | --- |
| `crates/z180-core/src/lib.rs` | 6,942 | Split production responsibilities and inline tests |
| `crates/z180-replay/src/lib.rs` | 1,468 | Split replay-bus and timeline implementations |
| `crates/z180-core/src/optable.rs` | 1,303 | Keep intact as the opcode/timing authority |
| `crates/z180-cli/src/sst.rs` | 1,291 | Split input, execution, comparison, and reporting |
| `crates/z180-py/src/lib.rs` | 871 | Split bus, config, machine implementation, and event conversion |
| `crates/z180-wasm/src/lib.rs` | 823 | Split bus, config, machine implementation, and event conversion |
| `crates/z180-core/src/ioregs.rs` | 696 | Keep intact as the internal-register authority |

At plan authoring, `master` matched `origin/master` and had no tracked changes.
The five existing untracked `notes-*.md` files are user-owned and must remain
unmodified and uncommitted. The required task checkpoint
`notes-rust-module-refactoring-plan.md` must also remain unmodified during
execution and uncommitted.

This plan was authored against source commit
`c2b9188ff0851cec2162ab52310b8f4a32a93f0a`. Its default-feature
`cargo test -p z180-core -- --list` inventory is 87 tests. If production source
has changed from that baseline before execution, stop and refresh this plan
against the new source rather than applying stale item moves.

## Tooling decision

No installed standalone CLI can safely infer these cross-module ownership
moves and repair Rust visibility semantically.

- rust-analyzer has the relevant semantic assists (`extract_module` and
  `move_module_to_file`), but the pinned `1.93.1` rust-analyzer component is
  not installed and the assists are exposed through LSP/editor code actions,
  not a stable batch command.
- The first `python.exe` on this Windows host is 32-bit. Before any workspace
  Cargo command, PyO3 must be pinned to the verified uv-managed 64-bit
  CPython 3.13.5 interpreter shown in the execution protocol below.
- The checked-in Python lock metadata currently retains the older
  `pytest>=8,<10` requirement while `pyproject.toml` names
  `pytest>=9.0.3,<10`. A plain `uv run` normalizes that unrelated metadata and
  dirties `uv.lock`. Python gates in this plan therefore use `uv run --frozen`
  so the structural campaign cannot absorb the pre-existing lock drift.
- `ast-grep 0.41.0` is installed. It may be used interactively for a
  narrowly specified, reviewed AST rewrite inside the active slice. It must
  not be used to infer module ownership, move a selected region between
  files, or promote visibility across a module boundary.
- `cargo fmt`, `cargo clippy`, and the compiler/test suite are the semantic
  validation authorities.

Perform source moves directly and review the raw diff. Do not add a custom
refactoring script, helper crate, code generator, adapter, facade, or
compatibility layer for this campaign.

## Non-negotiable boundaries

- Preserve every existing public Rust, Python, JavaScript, and TypeScript name
  and signature.
- Keep `Z180<B>` as one type. Do not introduce peripheral component structs,
  helper traits, sender objects, adapters, or wrapper machines.
- Keep public enums and structs at their current crate-root paths. Move
  inherent `impl` blocks and private implementation items instead of creating
  compatibility re-exports.
- Do not change the field order or encoded meaning of `SavedState`; released
  state compatibility remains authoritative.
- Do not change opcode descriptors, instruction timing, register metadata,
  callback validation, or event payloads.
- Do not add dependencies or edit any `Cargo.toml`.
- Do not edit `PLAN.md`, `PROGRESS.md`, `docs/verification-log.md`, the five
  existing untracked notes, or `notes-rust-module-refactoring-plan.md`.
- Do not mix cleanup, renaming, behavior changes, or performance work into a
  move.
- A visibility change is allowed only when the compiler proves that the moved
  existing item must cross the new private module boundary. Use the narrowest
  visibility that compiles, never add a new public item, and include the
  visibility change in the active slice's diff review.

## Git and slice protocol

Before the first slice, run:

```text
git status --short --branch
git diff --quiet
git diff --cached --quiet
git rev-parse HEAD
```

In the same PowerShell session that will run Cargo gates, set:

```powershell
$env:PYO3_PYTHON = 'C:\Users\Q\AppData\Roaming\uv\python\cpython-3.13.5-windows-x86_64-none\python.exe'
```

Verify the literal path first with:

```powershell
Test-Path -LiteralPath 'C:\Users\Q\AppData\Roaming\uv\python\cpython-3.13.5-windows-x86_64-none\python.exe'
```

The result must be `True`. If the interpreter is unavailable, stop; do not
substitute the 32-bit `python.exe` on PATH or select a different Python
version.

Record the baseline commit in the execution report. Untracked `notes-*.md`
files do not block execution; tracked or staged changes do.

For every source slice:

1. Confirm there is no tracked or staged diff from another slice.
2. Change only the paths named by that slice.
3. Run `cargo fmt --all`.
4. Inspect `git diff --check` and the complete raw diff.
5. Run the slice's exact gates.
6. If the slice is kept, mark its checkbox in this plan, stage only its named
   paths plus this plan, and commit with the exact message.
7. If the slice is rejected, restore every changed tracked path and remove
   only the exact new paths named by the slice. Confirm the tracked diff is
   empty before continuing.

Do not carry a failed or partial move into another slice. A failing gate is
evidence about the active slice; repair or revert that same slice.

## Ordered implementation slices

### 1. Split `z180-core` unit tests by responsibility

- [ ] Complete

Owned paths:

- `crates/z180-core/src/lib.rs`
- `crates/z180-core/src/tests/mod.rs`
- `crates/z180-core/src/tests/access.rs`
- `crates/z180-core/src/tests/io_dma.rs`
- `crates/z180-core/src/tests/state.rs`
- `crates/z180-core/src/tests/debug.rs`
- `crates/z180-core/src/tests/timers.rs`
- `crates/z180-core/src/tests/serial.rs`
- `crates/z180-core/src/tests/instructions.rs`
- `crates/z180-core/src/tests/interrupts.rs`
- `crates/z180-core/src/tests/timing.rs`

Replace the inline `#[cfg(test)] mod tests { ... }` with
`#[cfg(test)] mod tests;`. Keep the existing test-only buses, fixtures, and
machine constructors in `tests/mod.rs`; do not invent replacement helpers.
Move each existing test function exactly once using this exhaustive ownership
map:

- `access.rs`: `fallible_bus_*`, `mmu_*`, `remap_*`,
  `debugger_memory_*`, and `external_mapper_*`.
- `io_dma.rs`: `ioregs_*`, `dma*`, and `nmi_stops_dma_*`.
- `state.rs`: `save_state_*`, `load_state_*`, `save_load_*`, and
  `determinism_*`.
- `debug.rs`: `event_*` and `insn_trace_*`.
- `timers.rs`: `prt_*` and `frc_*`.
- `serial.rs`: `asci_*`, `both_asci_*`, `s180_asci_*`, and `csio_*`.
- `instructions.rs`: `stack_push_*`, `indexed_bit_*`, `implemented_set_*`,
  `nop_*`, `halt_*`, `ld_*`, `opcode_76_*`, `reset_*`, and
  `z180_repeat_*`.
- `interrupts.rs`: `ei_shadow_*`, `interrupts_*`,
  `peripheral_interrupts_*`, `undefined_*`, and `reti_*`.
- `timing.rs`: `timing_*`.

Preserve test names and bodies.

Gates:

```text
cargo test -p z180-core -- --list
cargo test -p z180-core
cargo test -p z180-core --features state
cargo test --workspace
```

The list command must still report 87 default-feature tests. Every
pre-existing test name must appear once, allowing only the new owner-module
prefix to differ.

Commit:

```text
Refactor z180-core tests into owner modules
```

### 2. Extract save-state implementation

- [ ] Complete

Owned paths:

- `crates/z180-core/src/lib.rs`
- `crates/z180-core/src/state.rs`

Move `STATE_VERSION`, private `SavedState`, and the existing `save_state` and
`load_state` implementations into `state.rs`. Leave public `StateError` at the
crate root. Keep the `SavedState` fields in exactly their existing order and
move method bodies without rewriting them.

Gates:

```text
cargo test -p z180-core --features state
cargo test -p z180-core --features state --test state_compat
cargo test --workspace
```

Commit:

```text
Extract z180-core save-state implementation
```

### 3. Extract debugging, watches, events, and instruction tracing

- [ ] Complete

Owned paths:

- `crates/z180-core/src/lib.rs`
- `crates/z180-core/src/debug.rs`

Keep the public `WatchId`, `WatchKind`, `Event`, and `TraceEntry` definitions at
the crate root. Move `MemWatch`, `TraceCapture`, and the existing watch, event,
PC-watch, and instruction-trace method implementations into `debug.rs`.
Other modules may call the moved event-emission operation only through the
narrowest crate-internal visibility required by the compiler.

Gates:

```text
cargo test -p z180-core event_
cargo test -p z180-core insn_trace
cargo test -p z180-core --features state
cargo test --workspace
```

Commit:

```text
Extract z180-core debugging implementation
```

### 4. Extract interrupt-controller implementation

- [ ] Complete

Owned paths:

- `crates/z180-core/src/lib.rs`
- `crates/z180-core/src/interrupts.rs`

Keep `IrqLine` and `IrqSource` at the crate root. Move interrupt pin setters,
interrupt-state accessors, priority selection, acknowledge, NMI, maskable
interrupt, and TRAP-taking implementations into `interrupts.rs`. Do not change
priority, vector, stack, timing, or event behavior.

Gates:

```text
cargo test -p z180-core interrupts_
cargo test -p z180-core peripheral_interrupts_
cargo test -p z180-core undefined_
cargo test --workspace
```

Commit:

```text
Extract z180-core interrupt implementation
```

### 5. Extract instruction execution by opcode family

- [ ] Complete

Owned paths:

- `crates/z180-core/src/lib.rs`
- `crates/z180-core/src/instructions.rs`
- `crates/z180-core/src/instructions/indexed.rs`
- `crates/z180-core/src/instructions/extended.rs`

Move register/operand/stack/condition/ALU helpers and unprefixed and CB handler
implementations into `instructions.rs`. Move the existing `execute_index` and
`execute_index_cb` implementations into `instructions/indexed.rs`. Move the
existing `execute_ed` implementation into `instructions/extended.rs`. Keep
handler names and the `pub(crate)` handler surface consumed by `optable.rs`
unchanged. Do not edit `optable.rs`.

Gates:

```text
cargo test -p z180-core implemented_set_
cargo test -p z180-core timing_
cargo run -p z180-cli --release -- sst --dir tests/sst/v1 --only 27
cargo test --workspace
```

Commit:

```text
Extract z180-core instruction implementations
```

### 6. Extract memory, bus, and internal-I/O access

- [ ] Complete

Owned paths:

- `crates/z180-core/src/lib.rs`
- `crates/z180-core/src/access.rs`

Move logical memory access, external mapping, emulation memory access,
memory-watch matching, internal-I/O decode, CPU I/O access, and internal
register read/write effect implementations into `access.rs`. Keep the
core-owned `Memory` representation in `memory.rs` and the register metadata in
`ioregs.rs`; do not edit either authority in this slice. If the move appears to
require changing either authority, stop and report that mismatch instead.

Gates:

```text
cargo test -p z180-core mmu_
cargo test -p z180-core ioregs_
cargo test -p z180-core event_io_
cargo test --workspace
```

Commit:

```text
Extract z180-core access implementation
```

### 7. Extract peripheral-family implementations

- [ ] Complete

Owned paths:

- `crates/z180-core/src/lib.rs`
- `crates/z180-core/src/peripherals.rs`
- `crates/z180-core/src/peripherals/asci.rs`
- `crates/z180-core/src/peripherals/csio.rs`
- `crates/z180-core/src/peripherals/prt.rs`
- `crates/z180-core/src/peripherals/dma.rs`

Use `peripherals.rs` only for private submodule declarations. Move existing
ASCI, CSI/O, PRT/FRC, and DMA implementations into the matching owner files.
Keep `finish_step` in `lib.rs` as the lifecycle coordinator that advances the
peripherals in the existing order. Do not introduce component structs or
change the `Z180` field layout.

Gates:

```text
cargo test -p z180-core prt_
cargo test -p z180-core frc_
cargo test -p z180-core asci_
cargo test -p z180-core csio_
cargo test -p z180-core dma
cargo test -p z180-core --features state
cargo test --workspace
```

Commit:

```text
Extract z180-core peripheral implementations
```

### 8. Split replay-bus and timeline implementations

- [ ] Complete

Owned paths:

- `crates/z180-replay/src/lib.rs`
- `crates/z180-replay/src/bus.rs`
- `crates/z180-replay/src/timeline.rs`
- `crates/z180-replay/src/tests.rs`

Keep all currently public replay types at the crate root. Move `ReplayBus`
implementation blocks and `HostBus` implementation into `bus.rs`; move
`Timeline` implementation blocks and timeline-only execution/error helpers
into `timeline.rs`; move the inline tests intact into `tests.rs`. Do not add a
branching facade or change journal/checkpoint semantics.

Gates:

```text
cargo test -p z180-replay
cargo test --workspace
```

Commit:

```text
Split replay bus and timeline implementations
```

### 9. Split the SST runner by pipeline stage

- [ ] Complete

Owned paths:

- `crates/z180-cli/src/sst.rs`
- `crates/z180-cli/src/sst/model.rs`
- `crates/z180-cli/src/sst/input.rs`
- `crates/z180-cli/src/sst/execute.rs`
- `crates/z180-cli/src/sst/compare.rs`
- `crates/z180-cli/src/sst/report.rs`
- `crates/z180-cli/src/sst/tests.rs`
- `crates/z180-cli/src/sst/policy.rs`

Keep `SstArgs` and the top-level `run` coordinator in `sst.rs`. Move existing
deserialized/report models, corpus discovery and validation, machine/case
execution, result comparison, report formatting, and inline tests into the
matching owner modules. Preserve `policy.rs` as-is except for import paths
required by the move. Do not change filters, exclusions, sabotage behavior,
comparison masks, output text, or JSON shape.

Gates:

```text
cargo test -p z180-cli
cargo run -p z180-cli --release -- sst --dir tests/sst/v1 --only 27
cargo test --workspace
```

Commit:

```text
Split the SST runner by pipeline stage
```

### 10. Split the Python binding implementation

- [ ] Complete

Owned paths:

- `crates/z180-py/src/lib.rs`
- `crates/z180-py/src/bus.rs`
- `crates/z180-py/src/config.rs`
- `crates/z180-py/src/machine.rs`
- `crates/z180-py/src/events.rs`

Keep Python-exposed class and enum declarations and `_native` module
registration in `lib.rs`. Move `PythonBus` and its `HostBus` implementation,
configuration/region parsing, existing `Machine` and `RamView` implementation
blocks, and event/trace dictionary conversion into the matching modules. Move
PyO3-annotated blocks intact. Do not change Python names, callback validation,
compatibility registration, or buffer behavior.

Gates, each run with working directory `crates/z180-py`:

```text
uv run --frozen --with maturin maturin develop --release
uv run --frozen pytest tests
```

Then, from the repository root:

```text
cargo test --workspace
```

Commit:

```text
Split the Python binding implementation
```

### 11. Split the WebAssembly binding implementation

- [ ] Complete

Owned paths:

- `crates/z180-wasm/src/lib.rs`
- `crates/z180-wasm/src/bus.rs`
- `crates/z180-wasm/src/config.rs`
- `crates/z180-wasm/src/machine.rs`
- `crates/z180-wasm/src/events.rs`

Keep WebAssembly-exposed type declarations, the TypeScript refinement
constant, and exported names in `lib.rs`. Move `JsBus` and its `HostBus`
implementation, configuration/region/callback parsing, the existing `Machine`
implementation block, and event/trace JavaScript conversion into the matching
modules. Move wasm-bindgen-annotated blocks intact. Do not change JavaScript
names, TypeScript declarations, callback validation, BigInt handling, or
event objects.

Gates, each run with working directory `crates/z180-wasm`:

```text
wasm-pack build --target nodejs --scope zcore
node tests/node-smoke.cjs
```

Then, from the repository root:

```text
cargo test --workspace
```

Commit:

```text
Split the WebAssembly binding implementation
```

### 12. Document final module ownership and run the complete gate

- [ ] Complete

Owned paths:

- `docs/ARCHITECTURE.md`
- `docs/rust-module-refactoring-plan.md`

Update only the workspace/module ownership description in
`docs/ARCHITECTURE.md` so it matches the resulting files. Do not rewrite
behavioral architecture or verification history.

Run these commands separately from the repository root, in this order:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p z180-core --all-targets --features state -- -D warnings
cargo test --workspace
cargo run -p z180-cli --release -- sst --dir tests/sst/v1 --only 27
cargo test -p z180-core --features state
```

Run these commands separately with working directory `crates/z180-py`:

```text
uv run --frozen --with maturin maturin develop --release
uv run --frozen pytest tests
```

Run these commands separately with working directory `crates/z180-wasm`:

```text
wasm-pack build --target nodejs --scope zcore
node tests/node-smoke.cjs
```

Run this final command from the repository root:

```text
uv run --project tools/reference pytest tools/reference/test_differential.py -q --hypothesis-profile=gate
```

Inspect:

```text
git status --short --branch
git diff --check
git log --oneline --decorate -15
```

The final status may contain only the five pre-existing user-owned untracked
notes and `notes-rust-module-refactoring-plan.md`. All twelve plan checkboxes
must be checked, every source slice must have its own named commit, and no old
production implementation may coexist with its new owner module.

Commit:

```text
Document Rust module ownership
```

## Stop conditions

Stop before changing more code and report the exact active slice if:

- a planned destination path already exists with different ownership;
- a move requires changing a public signature, serialized state, event shape,
  opcode metadata, register metadata, or callback behavior;
- a move appears to require a new trait, adapter, facade, wrapper, sender,
  component state object, or dependency;
- the current slice cannot pass its exact gates without a behavior change;
- tracked changes from another slice or another actor are present;
- an existing test is invalid or the supposed ownership boundary is disproven
  by current code.

Do not preserve momentum by switching to another file or performing adjacent
cleanup. Resume only after Q supplies the missing decision or changes the
plan.
