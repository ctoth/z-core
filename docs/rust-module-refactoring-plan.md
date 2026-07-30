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

The refreshed live checkout measured these largest Rust files before source
execution:

| File | Lines | Decision |
| --- | ---: | --- |
| `crates/z180-core/src/lib.rs` | 6,999 | Split production responsibilities and inline tests |
| `crates/z180-replay/src/lib.rs` | 1,468 | Split replay-bus and timeline implementations |
| `crates/z180-core/src/optable.rs` | 1,303 | Keep intact as the opcode/timing authority |
| `crates/z180-cli/src/sst.rs` | 1,291 | Split input, execution, comparison, and reporting |
| `crates/z180-py/src/lib.rs` | 871 | Split bus, config, machine implementation, and event conversion |
| `crates/z180-wasm/src/lib.rs` | 823 | Split bus, config, machine implementation, and event conversion |
| `crates/z180-core/src/ioregs.rs` | 696 | Keep intact as the internal-register authority |

At the execution refresh, `master` was at
`3084c0b141487fc1d2d1441887b7e3975ae8ef59`, one commit ahead of
`origin/master`, with no staged or source changes. The only tracked change was
this plan's tooling update.

These ten untracked notes predate valid source execution and must remain
uncommitted:

- `notes-implementation-review.md`
- `notes-issue-42.md`
- `notes-merge-campaign.md`
- `notes-plan-authoring.md`
- `notes-plan-execution.md`
- `notes-replay-trust-building.md`
- `notes-rust-module-refactoring-execution.md`
- `notes-rust-module-refactoring-plan.md`
- `notes-rust-refactoring-tooling.md`
- `notes-transcript-admission-search.md`

Only `notes-rust-module-refactoring-execution.md` may be updated for required
execution checkpoints. The other nine notes are user-owned or prior task
records and must remain unmodified.

This plan was refreshed against production source commit
`3084c0b141487fc1d2d1441887b7e3975ae8ef59`. Its default-feature
`cargo test -p z180-core -- --list` inventory is 89 tests. The refresh includes
the sleeping-time production fix and its two `sleep_run_*` tests. If
production source changes from that baseline before execution, stop and
refresh this plan against the new source rather than applying stale item
moves.

## Rust refactoring tooling

Rust source must be changed through Rust-aware refactoring operations, not
through Codex file editing, `apply_patch`, shell-generated source, text
replacement, or a custom move script. Raw diff inspection and compiler/test
commands validate a refactoring; they are not permitted source-mutation
mechanisms.

The installed and end-to-end verified refactoring stack is:

- the rustup `rust-analyzer` and `rust-src` components for the workspace's
  pinned `1.93.1` toolchain;
- `rust-analyzer 1.93.1 (01f6ddf7 2026-02-11)`, used by Helix through the
  rustup proxy;
- Helix `25.07.1`, the terminal LSP client used to apply rust-analyzer code
  actions;
- the official VS Code extension `rust-lang.rust-analyzer@0.3.2989`, including
  bundled server
  `rust-analyzer 0.3.2989-standalone (12c3381f0b 2026-07-26)`. Its package
  registers `rust-analyzer.moveItemUp`, `rust-analyzer.moveItemDown`, and the
  `experimental/moveItem` server request. VS Code is driven through its
  Chromium DevTools Protocol endpoint so the LLM can apply and save these
  Rust-aware operations without human input.

Install or repair that exact local stack with:

```powershell
rustup component add rust-analyzer rust-src --toolchain 1.93.1
scoop install helix@25.07.1
code --install-extension rust-lang.rust-analyzer@0.3.2989 --force
```

Before opening a refactoring client, pin the server selected by the rustup
proxy and verify the installed integration:

```powershell
$env:RUSTUP_TOOLCHAIN = '1.93.1'
rust-analyzer --version
hx --version
hx --health rust
code --list-extensions --show-versions | Select-String -Pattern '^rust-lang\.rust-analyzer@'
& "$env:USERPROFILE\.vscode\extensions\rust-lang.rust-analyzer-0.3.2989-win32-x64\server\rust-analyzer.exe" --version
```

The required results are the rust-analyzer and Helix versions above, a Helix
Rust health report that finds `rust-analyzer` and marks the Rust parser and
queries healthy, the VS Code extension version above, and the exact bundled
server version above. If any result differs, stop before source work and
refresh this tooling record; do not silently use a different refactoring
engine.

The pinned rust-analyzer binary and installed client package were verified to
contain these required operations:

- `extract_module`, shown by clients as `Extract Module`;
- `move_module_to_file`, shown by Helix as `Extract module to file`;
- `move_item`, registered by VS Code as `rust-analyzer: Move item up` and
  `rust-analyzer: Move item down`;
- LSP Rename and semantic import, qualification, unused-import, and visibility
  code actions.

The LLM control path for VS Code is:

1. Launch a dedicated instance, never an existing user window:

   ```powershell
   $env:RUSTUP_TOOLCHAIN = '1.93.1'
   code --user-data-dir 'C:\Users\Q\AppData\Local\Temp\z-core-ra-vscode-cdp-profile' `
     --extensions-dir 'C:\Users\Q\.vscode\extensions' `
     --disable-workspace-trust `
     --remote-debugging-port=9333 `
     '--remote-allow-origins=*' `
     --new-window '<workspace-root>' `
     --goto '<absolute-rust-path>:<line>:<column>'
   ```

   Resolve all four angle-bracket fields to the active slice before running
   the command. The path and caret must identify the complete Rust item being
   moved.
2. Open `http://127.0.0.1:9333/json/list` through the browser DevTools
   connector, select the dedicated VS Code page by its exact title, and
   connect to that page's `webSocketDebuggerUrl`.
3. Send these CDP messages directly from the connector; do not write an
   automation script:
   - `Input.dispatchKeyEvent` key-down and key-up for `P` / `KeyP`, virtual
     key `80`, modifiers `10` (`Ctrl+Shift`) to open the command palette;
   - `Input.insertText` with exactly `rust-analyzer: Move Item Down` or
     `rust-analyzer: Move Item Up`;
   - `Input.dispatchKeyEvent` key-down and key-up for `Enter`, virtual key
     `13`, to invoke rust-analyzer;
   - after the workspace edit settles, `Input.dispatchKeyEvent` key-down and
     key-up for `s` / `KeyS`, virtual key `83`, modifiers `2` (`Ctrl`) to save.
4. Read the saved file and raw Git diff only as evidence. The expected change
   is movement of the selected complete Rust item; any textual rewrite or
   unrelated edit rejects the operation.
5. Run the active compiler/test gate immediately.
6. At the end of the active slice, save and verify every expected Rust-aware
   edit, then send CDP `Browser.close` to the dedicated instance and verify
   that port `9333` is no longer listening. Do not leave a refactoring debug
   endpoint running between slices.

CDP supplies trusted editor input only. VS Code sends the semantic request,
rust-analyzer creates the workspace edit, and VS Code applies and saves it.
The connector must never call a DOM text-edit API, Monaco edit API, filesystem
write API, or generic source-edit tool.

Use the following source-mutation protocol inside the active slice:

1. Open only the active slice's Rust paths in a verified client.
2. Select complete Rust items. If required items are noncontiguous, use only
   rust-analyzer `Move item up` or `Move item down` to make them contiguous,
   preserving their relative order, and inspect that order-only diff before
   continuing.
3. Apply rust-analyzer `Extract Module` to the complete selection.
4. Name the generated module with LSP Rename; typing the destination name is
   input to that refactoring operation, not permission for source editing.
5. Apply rust-analyzer `Extract module to file`.
6. Run the slice's first compiler gate immediately. If the generated move
   needs an import, path qualification, unused-import removal, or visibility
   repair, apply only the matching rust-analyzer semantic code action. Never
   select a code-generation action as a substitute for an existing moved
   item.
7. Run `cargo fmt --all`, inspect the complete raw diff, and continue with the
   slice's remaining gates.

An isolated live smoke test on 2026-07-30 exercised the complete required
stack in `C:\Users\Q\AppData\Local\Temp\z-core-ra-refactor-smoke`:

- Cargo generated the temporary library; rust-analyzer `Extract Module`, LSP
  Rename, and `Extract module to file` created the destination module.
- The first `cargo test` exposed a nested `use super::*` whose meaning changed
  after extraction. rust-analyzer `Import crate::add` and
  `Remove all unused imports` repaired the imports without text editing, and
  the next `cargo test` passed.
- The LLM used the CDP protocol above to invoke
  `rust-analyzer: Move Item Down`. The saved source contained the complete
  `add` item below `mod smokemodname;`, its SHA-256 became
  `85509627757F7669711F322731B16DB6892CC35A56861843D1870CCAEF8C66E4`, and
  `cargo test` passed.
- The LLM then invoked `rust-analyzer: Move Item Up`. The file returned
  byte-for-byte to starting SHA-256
  `3447ACCF369783A043C54F49CB0E7C4D237A6913B392BFDE2472E43B53837ECA`, and
  `cargo test` passed again.

No human input, generic Rust-source write, custom move script, or text
replacement was used. The smoke test also proves that rust-analyzer output
must not be trusted without the immediate compiler gate and Rust-aware repair
loop.

No installed standalone CLI safely performs this workflow. The rust-analyzer
assists are LSP/editor operations, not a stable batch command.

- The first `python.exe` on this Windows host is 32-bit. Before any workspace
  Cargo command, PyO3 must be pinned to the verified uv-managed 64-bit
  CPython 3.13.5 interpreter shown in the execution protocol below.
- The checked-in Python lock metadata currently retains the older
  `pytest>=8,<10` requirement while `pyproject.toml` names
  `pytest>=9.0.3,<10`. A plain `uv run` normalizes that unrelated metadata and
  dirties `uv.lock`. Python gates in this plan therefore use `uv run --frozen`
  so the structural campaign cannot absorb the pre-existing lock drift.
- `ast-grep 0.41.0` is installed but is not an allowed source-mutation
  mechanism in this campaign. It must not infer ownership, move source,
  rewrite imports, or change visibility.
- `cargo fmt`, `cargo clippy`, and the compiler/test suite are the semantic
  validation authorities.

Do not add a custom refactoring script, helper crate, code generator, adapter,
facade, or compatibility layer for this campaign. If the installed
Rust-aware operations cannot produce a planned move, stop and report the
active slice instead of editing the Rust source by another mechanism.

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
- Do not edit `PLAN.md`, `PROGRESS.md`, `docs/verification-log.md`, or any
  protected untracked note identified in Current evidence. Only the designated
  execution checkpoint note may change, and it must remain uncommitted.
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
- `timing.rs`: `timing_*` and `sleep_run_*`.

Preserve test names and bodies.

Gates:

```text
cargo test -p z180-core -- --list
cargo test -p z180-core
cargo test -p z180-core --features state
cargo test --workspace
```

The list command must still report 89 default-feature tests. Every
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

The final status may contain only the ten untracked notes listed in Current
evidence. All twelve plan checkboxes must be checked, every source slice must
have its own named commit, and no old production implementation may coexist
with its new owner module.

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
