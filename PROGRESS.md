# z-core execution progress

## Phase 0 — Scaffold and CI

### Gate G0 — PASS (2026-07-20)

```text
> cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s
     Running unittests src\main.rs (target\debug\deps\z180_cli-a990f9faf351a214.exe)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src\lib.rs (target\debug\deps\z180_core-760d8d6d22545700.exe)

running 1 test
test tests::it_works ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests z180_core

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.09s
```

The final `cargo fmt --check` command completed successfully with no output.

```text
> Get-Item -LiteralPath .\docs\vendor\um0050.pdf | Select-Object FullName,Length

FullName                                       Length
--------                                       ------
C:\Users\Q\code\z-core\docs\vendor\um0050.pdf 2922169
```

## Phase 1 — Conformance apparatus

### P1.1 — Z80 SingleStepTests submodule

- Repository: https://github.com/SingleStepTests/z80
- Path: `tests/sst`
- Pinned commit: `ebe1875d48f374bcfd4b505d8eb8ee751568b5f7`

### Phase 1 task-order correction (2026-07-20)

The original P1.2 runner depended on the machine, register file, memory, and
`step()` API created by the original P1.3 stub task. Q authorized reordering
the two tasks. `PLAN.md` now defines P1.2 as the stub CPU subset and P1.3 as
the SST runner; the runner-code cross-reference in P1.7 now points to P1.3.

### P1.5 oracle-binding blocker — RESOLVED (2026-07-20)

The named existing qns CFFI surface at `C:\Users\Q\code\qns` is present, but
`qns.cpu.Z180` and its CFFI declarations expose only `qns_z180_get_reg`, IRQ
control, PC/halt, MMU getters, and unrelated diagnostics. They expose no CPU
register setter and no complete snapshot access for the alternate register
bank, I/R, IFF1/IFF2, IM, or ITC.

P1.5 requires seeded randomized full initial states and recording the complete
post-instruction SST state, including TRAP ITC. That cannot be done through the
exact named binding. A bootstrap/epilogue reconstruction would substitute a
different workflow and would not provide an actual complete one-instruction
snapshot. Required from Q: authorize and provide the exact missing qns oracle
state-load/state-capture mechanism, or revise P1.5 to name another exact
mechanism.

Q explicitly authorized replacing the unavailable incumbent-oracle dependency
with an independent first-party reference model derived only from verified
UM0050 facts. `PLAN.md` now defines `tools/reference/`, deterministic
reference-generated Z180 instruction/TRAP/MMU cases, generator determinism and
schema gates, and optional non-gating incumbent comparisons only if a complete
black-box state API later exists. No emulator source access is authorized.

### P1.5 flag-schema correction (2026-07-20)

Direct reading of UM0050 Table 46 established that OTIM/OTDM define Z from the
post-decrement B value and N from the output byte, but mark S, H, P/V, and C as
affected without defining their resulting values. The corpus schema now carries
`flags_mask`: `0xD7` for normally defined documented flags and `0x42` for
OTIM/OTDM. This prevents a first-party reference function from inventing values
that the manufacturer did not specify. The Appendix C reset-state ITC value was
also corrected from `0x00` to the verified `0x01` reset value.

### P1.5 — UM0050 reference corpus (2026-07-20)

The exact generation command completed with exit code 0 and no stdout/stderr:

```text
> uv run --project tools/reference python tools/reference/generate.py --out tests/z180-sst
```

The parsed corpus census is:

```text
{
  "files": 36,
  "counts": [
    {
      "cases": 50,
      "files": 1
    },
    {
      "cases": 200,
      "files": 35
    }
  ]
}
```

The 35 200-case files are 34 Z180-added opcode forms plus the MMU family. The
50-case TRAP file covers all verified representative second-opcode forms:

```text
CB30 CB31 CB32 CB33 CB34 CB35 CB36 CB37 DD24 ED31 FD24
```

The reference source imports neither z-core nor an incumbent emulator. All
instruction, flag, TRAP, ITC, and MMU constants cite verified manufacturer
facts in `docs/verification-log.md`.

### P1.5b — Reference self-consistency and schema (2026-07-20)

```text
> uv run --project tools/reference pytest tools/reference
============================= test session starts =============================
platform win32 -- Python 3.13.5, pytest-8.4.2, pluggy-1.6.0
rootdir: C:\Users\Q\code\z-core\tools\reference
configfile: pyproject.toml
plugins: hypothesis-6.157.2
collected 3 items

tools\reference\test_reference.py ...                                    [100%]

============================= 3 passed in 20.55s ==============================
```

The second exact command completed with exit code 0 and no stdout/stderr:

```text
> uv run --project tools/reference python tools/reference/generate.py --check tests/z180-sst
```

The tests apply identical reference transitions twice for 1,000 Hypothesis
examples, validate every checked-in case and count, and generate two complete
temporary corpora whose relative trees and bytes must match. `--check` performs
the same validation and byte comparison against the checked-in corpus.

### P1.6 — ZEX assets and harness skeleton (2026-07-20)

`zexdoc.com` is pinned from the dedicated `agn453/ZEXALL` repository at commit
`8f71d418bae69a476a5a0e5c6e122c8801b8d9f4`. The upstream identifies it as
Frank D. Cringle's documented-flags exerciser extracted from YAZE-AG 2.51.3
and licensed GPL-2.0. The 8,704-byte binary's SHA-256 is
`34923a7ed82285d3038b2d54bd64899e12173eebb61f9d07b4fc72e78af2ae8f`.
`tests/vendor/zex/NOTICES.md` records the immutable source and license URLs.

The real vendored artifact loads and stops cleanly on the Phase 1 stub:

```text
> cargo run -p z180-cli -- zex tests/vendor/zex/zexdoc.com
   Compiling z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.35s
     Running `target\debug\z180-cli.exe zex tests/vendor/zex/zexdoc.com`
unimplemented opcode at PC=0100: c3
```

Final workspace authority:

```text
> cargo test --workspace

running 9 tests
test sst::policy::tests::appendix_a_policy_excludes_known_undefined_families ... ok
test sst::policy::tests::opcode_parser_skips_displacement_placeholder ... ok
test sst::policy::tests::only_filter_expands_inclusive_main_opcode_ranges ... ok
test sst::tests::sabotage_reverses_only_ld_operands ... ok
test sst::tests::comparison_reports_the_first_differing_field ... ok
test sst::tests::comparison_masks_xy_flags_and_r_high_bit ... ok
test zex::tests::bdos_console_output_returns_to_the_stacked_address ... ok
test zex::tests::bdos_string_output_stops_at_dollar ... ok
test zex::tests::unimplemented_opcode_is_reported_cleanly ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 8 tests
test tests::implemented_set_is_only_the_phase_one_stub ... ok
test tests::halt_enters_halted_state_and_leaves_flags_unchanged ... ok
test tests::ld_reads_and_writes_through_hl ... ok
test tests::nop_advances_pc_and_r_without_changing_registers ... ok
test tests::ld_register_to_register_preserves_flags ... ok
test tests::opcode_76_is_halt_not_ld_hl_hl ... ok
test tests::reset_preserves_memory_and_clears_r_and_halt ... ok
test tests::unimplemented_opcode_does_not_execute ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.68s

> cargo fmt --all --check
```

The final format check completed successfully with no output.

### P1.7 state-load blocker — RESOLVED (2026-07-20)

P1.7 requires the shared SST runner to load Appendix C's
`itc/cbr/bbr/cbar/sleeping` initial state and compare the resulting Z180 state.
The plan's immutable core API names only read-only `io_reg_peek` and
`mmu_translate` visibility; neither is implemented yet, and the API names no
setter or complete conformance-state load/capture mechanism. The current core
also has no separate ITC/MMU/sleep fields to load. MMU register implementation
is ordered later in Phase 5.

Deterministic port reads/writes can be implemented CLI-locally through a shared
`HostBus`, but that does not solve the missing Z180 control-state mechanism.
Deserializing the fields while ignoring them, relying on today's reset-valued
initial corpus, adding an unplanned `io_reg_poke`/test setter/snapshot adapter,
or deferring the requirement to Phase 5 would each substitute for the literal
P1.7 instruction.

Resolution: the controlling plan now defers injection and comparison of the
Appendix C `z180` state to the owning implementation phases. P1.7 validates
the complete schema, dispatches all three families, provides deterministic
port scripting, and reports the census while every generated case remains
UNIMPLEMENTED. Phase 3 activates instruction/TRAP cases from reset state and
compares ITC/SLP through their owning public interfaces. Phase 5 activates MMU
cases by programming CBR/BBR/CBAR through the real internal-I/O instruction
path. No privileged SST-only setter or adapter is authorized.

### P1.7 — z180-sst runner dispatch and census (2026-07-20)

The shared SST runner now deserializes and validates every Appendix C field:
generated metadata, complete base/Z180 initial and final states, canonical RAM,
typed port events, and 16 ordered 20-bit MMU probes. It dispatches all three
generated case kinds to Phase 1 UNIMPLEMENTED reports without injecting or
comparing Z180 control state. Its local scripted `HostBus` supplies ordered
read values, records all reads/writes, and reports the first event mismatch
when a case executes. F comparison uses each generated case's `flags_mask`.
`--census` emits every opcode/special-family count and an aggregate total.

Exact generated-corpus command:

```text
> cargo run -p z180-cli -- sst --dir tests/z180-sst --census
UNIMPLEMENTED ed00: pass=0 fail=0 unimplemented=200
... 32 additional ED opcode families, each unimplemented=200 ...
UNIMPLEMENTED ed9b: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED mmu: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED trap: pass=0 fail=0 unimplemented=50
CENSUS ed00=200
... 32 additional ED opcode families, each cases=200 ...
CENSUS ed9b=200
CENSUS mmu=200
CENSUS trap=50
CENSUS total=7050
SUMMARY pass=0 fail=0 unimplemented=7050 excluded=0
```

The shared standard path was rerun with the first G1 command and remained
exact: all 65 selected opcode files reported 1,000 PASS each, with
`SUMMARY pass=65000 fail=0 unimplemented=0 excluded=0`.

Final quality authority:

```text
> cargo test --workspace
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.52s

> cargo fmt --all --check
```

The format check completed successfully with no output. Unit regressions cover
generated instruction/MMU schema dispatch, all-page MMU validation, scripted
read/write ordering, first-difference reporting, and per-case flag masks.

### Gate G1 — PASS (2026-07-20)

The exact normal SST gate produced:

```text
> cargo run -p z180-cli -- sst --dir tests/sst/v1 --only 00,76,40..7f
PASS 00: pass=1000 fail=0 unimplemented=0
PASS 40: pass=1000 fail=0 unimplemented=0
PASS 41: pass=1000 fail=0 unimplemented=0
PASS 42: pass=1000 fail=0 unimplemented=0
PASS 43: pass=1000 fail=0 unimplemented=0
PASS 44: pass=1000 fail=0 unimplemented=0
PASS 45: pass=1000 fail=0 unimplemented=0
PASS 46: pass=1000 fail=0 unimplemented=0
PASS 47: pass=1000 fail=0 unimplemented=0
PASS 48: pass=1000 fail=0 unimplemented=0
PASS 49: pass=1000 fail=0 unimplemented=0
PASS 4a: pass=1000 fail=0 unimplemented=0
PASS 4b: pass=1000 fail=0 unimplemented=0
PASS 4c: pass=1000 fail=0 unimplemented=0
PASS 4d: pass=1000 fail=0 unimplemented=0
PASS 4e: pass=1000 fail=0 unimplemented=0
PASS 4f: pass=1000 fail=0 unimplemented=0
PASS 50: pass=1000 fail=0 unimplemented=0
PASS 51: pass=1000 fail=0 unimplemented=0
PASS 52: pass=1000 fail=0 unimplemented=0
PASS 53: pass=1000 fail=0 unimplemented=0
PASS 54: pass=1000 fail=0 unimplemented=0
PASS 55: pass=1000 fail=0 unimplemented=0
PASS 56: pass=1000 fail=0 unimplemented=0
PASS 57: pass=1000 fail=0 unimplemented=0
PASS 58: pass=1000 fail=0 unimplemented=0
PASS 59: pass=1000 fail=0 unimplemented=0
PASS 5a: pass=1000 fail=0 unimplemented=0
PASS 5b: pass=1000 fail=0 unimplemented=0
PASS 5c: pass=1000 fail=0 unimplemented=0
PASS 5d: pass=1000 fail=0 unimplemented=0
PASS 5e: pass=1000 fail=0 unimplemented=0
PASS 5f: pass=1000 fail=0 unimplemented=0
PASS 60: pass=1000 fail=0 unimplemented=0
PASS 61: pass=1000 fail=0 unimplemented=0
PASS 62: pass=1000 fail=0 unimplemented=0
PASS 63: pass=1000 fail=0 unimplemented=0
PASS 64: pass=1000 fail=0 unimplemented=0
PASS 65: pass=1000 fail=0 unimplemented=0
PASS 66: pass=1000 fail=0 unimplemented=0
PASS 67: pass=1000 fail=0 unimplemented=0
PASS 68: pass=1000 fail=0 unimplemented=0
PASS 69: pass=1000 fail=0 unimplemented=0
PASS 6a: pass=1000 fail=0 unimplemented=0
PASS 6b: pass=1000 fail=0 unimplemented=0
PASS 6c: pass=1000 fail=0 unimplemented=0
PASS 6d: pass=1000 fail=0 unimplemented=0
PASS 6e: pass=1000 fail=0 unimplemented=0
PASS 6f: pass=1000 fail=0 unimplemented=0
PASS 70: pass=1000 fail=0 unimplemented=0
PASS 71: pass=1000 fail=0 unimplemented=0
PASS 72: pass=1000 fail=0 unimplemented=0
PASS 73: pass=1000 fail=0 unimplemented=0
PASS 74: pass=1000 fail=0 unimplemented=0
PASS 75: pass=1000 fail=0 unimplemented=0
PASS 76: pass=1000 fail=0 unimplemented=0
PASS 77: pass=1000 fail=0 unimplemented=0
PASS 78: pass=1000 fail=0 unimplemented=0
PASS 79: pass=1000 fail=0 unimplemented=0
PASS 7a: pass=1000 fail=0 unimplemented=0
PASS 7b: pass=1000 fail=0 unimplemented=0
PASS 7c: pass=1000 fail=0 unimplemented=0
PASS 7d: pass=1000 fail=0 unimplemented=0
PASS 7e: pass=1000 fail=0 unimplemented=0
PASS 7f: pass=1000 fail=0 unimplemented=0
SUMMARY pass=65000 fail=0 unimplemented=0 excluded=0
```

The exact negative control exited 1, reported failures on every non-identity
LD family, and retained passes only where reversing operands is behaviorally
unchanged. It emitted 55,869 lines because each mismatch is printed; the
first failure and complete aggregate were:

```text
> cargo run -p z180-cli -- sst --dir tests/sst/v1 --only 00,76,40..7f --sabotage-ld
PASS 00: pass=1000 fail=0 unimplemented=0
PASS 40: pass=1000 fail=0 unimplemented=0
FAIL 41: pass=9 fail=991 unimplemented=0
  41 0000: b expected=65 actual=97
...
PASS 7f: pass=1000 fail=0 unimplemented=0
SUMMARY pass=9201 fail=55799 unimplemented=0 excluded=0
Error: 55799 single-step test(s) failed
error: process didn't exit successfully: `target\debug\z180-cli.exe sst --dir tests/sst/v1 --only 00,76,40..7f --sabotage-ld` (exit code: 1)
```

The generated Z180 corpus gate produced:

```text
> cargo run -p z180-cli -- sst --dir tests/z180-sst --census
UNIMPLEMENTED ed00: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed01: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed04: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed08: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed09: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed0c: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed10: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed11: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed14: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed18: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed19: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed1c: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed20: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed21: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed24: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed28: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed29: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed2c: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed30: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed34: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed38: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed39: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed3c: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed4c: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed5c: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed64: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed6c: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed74: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed76: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed7c: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed83: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed8b: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed93: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED ed9b: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED mmu: pass=0 fail=0 unimplemented=200
UNIMPLEMENTED trap: pass=0 fail=0 unimplemented=50
CENSUS ed00=200
CENSUS ed01=200
CENSUS ed04=200
CENSUS ed08=200
CENSUS ed09=200
CENSUS ed0c=200
CENSUS ed10=200
CENSUS ed11=200
CENSUS ed14=200
CENSUS ed18=200
CENSUS ed19=200
CENSUS ed1c=200
CENSUS ed20=200
CENSUS ed21=200
CENSUS ed24=200
CENSUS ed28=200
CENSUS ed29=200
CENSUS ed2c=200
CENSUS ed30=200
CENSUS ed34=200
CENSUS ed38=200
CENSUS ed39=200
CENSUS ed3c=200
CENSUS ed4c=200
CENSUS ed5c=200
CENSUS ed64=200
CENSUS ed6c=200
CENSUS ed74=200
CENSUS ed76=200
CENSUS ed7c=200
CENSUS ed83=200
CENSUS ed8b=200
CENSUS ed93=200
CENSUS ed9b=200
CENSUS mmu=200
CENSUS trap=50
CENSUS total=7050
SUMMARY pass=0 fail=0 unimplemented=7050 excluded=0
```

The ZEX harness gate produced:

```text
> cargo run -p z180-cli -- zex tests/vendor/zex/zexdoc.com
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running `target\debug\z180-cli.exe zex tests/vendor/zex/zexdoc.com`
unimplemented opcode at PC=0100: c3
```

The CI checkout now explicitly requests submodules. The pinned checkout and
the workflow's exact commands were:

```text
> git submodule status
 ebe1875d48f374bcfd4b505d8eb8ee751568b5f7 tests/sst (v1.0-beta.2)

> cargo fmt --check

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.13s

> cargo test --workspace
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored
Doc-tests z180_core: 0 passed; 0 failed
```

Q authorized repository creation and push. Public repository
`https://github.com/ctoth/z-core` now tracks committed `master`. Q then
required the checkout action update; official upstream release `v7.0.1` was
verified and committed as `13e9d51` while retaining `submodules: true`.

GitHub Actions run `29803089336` on that exact commit is green:

```text
✓ master CI · 29803089336
✓ ubuntu-latest in 39s
  ✓ Run actions/checkout@v7.0.1
  ✓ Install Rust components
  ✓ Check formatting
  ✓ Run Clippy
  ✓ Run tests
✓ windows-latest in 1m13s
  ✓ Run actions/checkout@v7.0.1
  ✓ Install Rust components
  ✓ Check formatting
  ✓ Run Clippy
  ✓ Run tests
```

Run: https://github.com/ctoth/z-core/actions/runs/29803089336

All five G1 requirements are green. Phase 1 is complete; no CPU behavior
beyond the authorized stub subset was added.

## Phase 2 — Full unprefixed opcode page

### P2.1 — Main-page optable migration (2026-07-20)

`crates/z180-core/src/optable.rs` now owns the 256-entry main opcode table.
Each descriptor carries its mnemonic template, operand kinds, byte length,
optional verified cycle count, and a monomorphized handler function pointer.
Cycle entries deliberately remain `None` until Phase 4 verifies and
transcribes UM0050 timing; the existing non-hardware progress unit remains the
temporary runtime fallback. Operand metadata is retained for the planned
table-driven disassembler/docs consumers rather than read artificially on the
hot path.

Only the already-authorized Phase 1 stub entries are populated: NOP, HALT,
and the `40h`–`7Fh` LD matrix with `76h` correctly owned by HALT. `step()` now
gets implementation status, byte length, provisional timing state, and
dispatch from this table. The duplicated free implementation-policy match was
removed; the CLI's query is table-backed. No new opcode behavior was added.

Table-driven dispatch preserved the exact SST authority:

```text
> cargo run -p z180-cli -- sst --dir tests/sst/v1 --only 00,76,40..7f
PASS 00: pass=1000 fail=0 unimplemented=0
PASS 40..7f: 64 files, each pass=1000 fail=0 unimplemented=0
SUMMARY pass=65000 fail=0 unimplemented=0 excluded=0
```

Final quality authority:

```text
> cargo test --workspace
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.62s

> cargo fmt --all --check
```

The format check completed successfully with no output. Optable regressions
prove the exact populated set and the NOP/HALT/LD metadata.

### P2.2 — Documented unprefixed opcodes (2026-07-20)

All 252 documented main-page instructions are implemented from UM0050 Tables
38-47. `CB`, `DD`, `ED`, and `FD` remain unimplemented prefix bytes for Phase
3. The table remains the single owner of mnemonic, operands, byte length,
provisional timing state, and handler. Immediate `IN A,(n)` / `OUT (n),A`
forms use the documented `A:n` 16-bit port and route directly to `HostBus`;
the internal-I/O window remains owned by Phase 5.

The core now covers loads, ALU and documented flags, 8/16-bit INC/DEC,
accumulator rotates, DAA, CPL/SCF/CCF, control flow, stack/exchange, HALT,
and DI/EI. EI shadow is private state: EI establishes it, the next executed
instruction consumes it before dispatch, consecutive EI refreshes it, and DI
clears it. No privileged SST-only state API was added.

DAA has an exhaustive core regression over all 2,048 combinations of A and
N/H/C. The external DAA corpus independently passes all 1,000 cases:

```text
> cargo run -p z180-cli -- sst --dir tests/sst/v1 --only 27
PASS 27: pass=1000 fail=0 unimplemented=0
SUMMARY pass=1000 fail=0 unimplemented=0 excluded=0
```

The complete main-page SST slice is green:

```text
> cargo run -p z180-cli -- sst --dir tests/sst/v1 --only 00..ff
PASS 00..ff: 252 documented files, each pass=1000 fail=0 unimplemented=0
SUMMARY pass=252000 fail=0 unimplemented=0 excluded=0
```

Final workspace authority:

```text
> cargo test --workspace
running 12 tests (z180-cli)
test result: ok. 12 passed; 0 failed; 0 ignored
running 12 tests (z180-core)
test result: ok. 12 passed; 0 failed; 0 ignored
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.82s

> cargo fmt --all --check
```

The format check completed successfully with no output. The ZEX harness's
deliberately unimplemented test byte moved from newly implemented `C3` to the
still-unimplemented `CB` prefix; harness behavior is unchanged.

P2.3 and Gate G2 remain unchecked.

### P2.3 — Interrupt-check point (2026-07-20)

`step()` now enters a private interrupt service point before the HALT hold and
before fetching the next opcode. The Phase 2 implementation returns no pending
source unconditionally: interrupt pins, source priority, acknowledge behavior,
and HALT wake-up remain owned by Phase 5. EI shadow remains observable at the
check point and is consumed only when the following instruction proceeds to
dispatch.

Focused regressions prove the check point has no Phase 2 source and does not
consume EI shadow itself. Final quality authority:

```text
> cargo test --workspace
running 12 tests (z180-cli)
test result: ok. 12 passed; 0 failed; 0 ignored
running 13 tests (z180-core)
test result: ok. 13 passed; 0 failed; 0 ignored
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.64s

> cargo fmt --all --check
```

The format check completed successfully with no output. Gate G2 remains the
next unchecked plan item.

### Gate G2 — PASS (2026-07-20)

P2.3 commit `d51187b` was pushed before the gate. GitHub Actions run
`29805137326` passed Ubuntu in 40 seconds and Windows in 1 minute 12 seconds,
including checkout with submodules, rustfmt, warnings-denied Clippy, and all
workspace tests.

Exact full SST gate output:

```text
> cargo run -p z180-cli -- sst --dir tests/sst/v1
SUMMARY pass=252000 fail=0 unimplemented=453000 excluded=899
```

| Surface | Files/cases | Result |
|---|---:|---|
| Documented unprefixed page | 252 files / 252,000 cases | 100% PASS |
| Documented CB/DD/ED/FD-prefixed pages | 453 files / 453,000 cases | UNIMPLEMENTED for Phase 3 |
| UM-defined undefined prefixed forms | 899 files | EXCLUDED by Appendix A policy |
| Failures | 0 cases | PASS |

The only unimplemented reports are prefixed-page stems (`cb`, `dd`, `ed`, or
`fd`); all 252 documented single-byte files report 1,000 passes each.

The required negative control still detects reversed LD operands and exits
nonzero:

```text
> cargo run -p z180-cli -- sst --dir tests/sst/v1 --only 40..7f --sabotage-ld
SUMMARY pass=8201 fail=55799 unimplemented=0 excluded=0
Error: 55799 single-step test(s) failed
error: process didn't exit successfully (exit code: 1)
```

Phase 2 is complete. Phase 3 task 1 is the next unchecked plan item.

## Phase 3 — Prefixed pages, Z180 instructions, TRAP

### P3.1 — Documented CB page (2026-07-20)

The core now decodes the CB page as a two-byte instruction and increments R
for both M1 fetches. A dedicated opcode table owns all documented CB rotates,
shifts, BIT, RES, and SET forms. CB `30..37` SLL remains undefined and has no
handler; TRAP behavior remains owned by Phase 3 task 5.

Direct UM0050 page-image inspection established the documented flag effects.
The verification log records the manual-defined behavior and the deterministic
free choice for BIT's UM-undefined S and P/V outputs. The SST runner now asks
the core whether a complete opcode sequence is implemented, activating CB
files without adding a runner-side execution adapter.

External corpus results:

```text
> cargo run -p z180-cli -- sst --dir tests/sst/v1 --only "cb 00,cb 08,cb 10,cb 18,cb 20,cb 28,cb 38,cb 40,cb 7f,cb 80,cb bf,cb c0,cb ff"
SUMMARY pass=13000 fail=0 unimplemented=0 excluded=0

> cargo run -p z180-cli -- sst --dir tests/sst/v1
SUMMARY pass=500000 fail=0 unimplemented=205000 excluded=899
```

All 248 documented CB files pass all 248,000 cases. The eight SLL files are
explicitly excluded under the Appendix A policy. The 205 remaining
UNIMPLEMENTED files are only the later Phase 3 DD/FD, DDCB/FDCB, and ED
surfaces.

The workspace quality authorities passed after the final diff audit:

```text
> cargo test --workspace
z180-cli: 12 passed; 0 failed
z180-core: 14 passed; 0 failed
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.54s

> cargo fmt --check
```

The format check completed successfully with no output. The ZEX harness's
deliberately unimplemented fixture moved from newly implemented `CB` to the
still-unimplemented `DD` prefix; its behavior is unchanged. Phase 3 task 2 is
the next unchecked plan item.

### P3.2 — Documented DD/FD pages (2026-07-20)

Direct UM0050 inspection resolved the plan's DD/FD undefined-operation
question. Table 48 defines DD/FD only when IX/IY replaces an HL or (HL)
operand, explicitly substitutes JP (HL), and explicitly declares prefixed
EX DE,HL illegal. The TRAP chapter defines any undefined second opcode fetch
as a TRAP. UM0050 therefore documents no prefix-ignored DD/FD case: every
other second byte is undefined and remains without a handler until P3.5.

Separate DD and FD metadata tables each contain exactly the 39 documented
Table 48 substitutions. Decode executes them as two-M1 instructions against
IX or IY, including signed-displacement loads, INC/DEC, and ALU forms plus the
documented 16-bit arithmetic, load, stack, exchange, jump, and SP forms.
DDCB/FDCB remains unimplemented for P3.3.

External corpus results:

```text
> cargo run -p z180-cli -- sst --dir tests/sst/v1 --only "dd 09,fd 29,dd 21,fd 22,dd 2a,fd 23,dd 2b,dd 34,fd 35,dd 36,dd 46,fd 70,dd 86,fd be,dd e1,fd e3,dd e5,fd e9,dd f9"
SUMMARY pass=19000 fail=0 unimplemented=0 excluded=0

> cargo run -p z180-cli -- sst --dir tests/sst/v1
SUMMARY pass=578000 fail=0 unimplemented=127000 excluded=899
```

All 78 documented DD/FD files pass all 78,000 cases. The 127 remaining
UNIMPLEMENTED files are only the later P3.3 DDCB/FDCB and P3.4 ED surfaces.

The workspace quality authorities passed:

```text
> cargo test --workspace
z180-cli: 12 passed; 0 failed
z180-core: 15 passed; 0 failed
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.69s

> cargo fmt --check
```

The format check completed successfully with no output. Phase 3 task 3 is the
next unchecked plan item.

### P3.3 — Documented DDCB/FDCB pages (2026-07-20)

Direct UM0050 inspection links Table 48 Note 3's `(HL)` to `(IX/IY+d)`
substitution with the `g=(HL)` cells marked in Table 49. The documented
DDCB/FDCB final bytes therefore have low bits `110` and operate only on
indexed memory: seven rotate/shift forms excluding SLL, eight BIT, eight RES,
and eight SET per prefix. Register-result forms and SLL are undefined and
remain without handlers for P3.5 TRAP.

Dedicated DDCB/FDCB metadata tables contain exactly those 31 forms per prefix.
Four-byte decode reads DD/FD, CB, the signed displacement, and the final
opcode, advances PC by four, increments R for the two opcode-fetch M1 cycles,
and executes against `(IX/IY+d)`.

The first focused run reported all nine files UNIMPLEMENTED because the core's
implementation query expected the displacement as an opcode byte. The SST
parser's established representation correctly omits operand placeholders.
Changing the query identity from four bytes to `[DD/FD, CB, final]` activated
the existing execution path; no instruction semantics changed in that
correction.

External corpus results after the correction:

```text
> cargo run -p z180-cli -- sst --dir tests/sst/v1 --only "dd cb __ 06,fd cb __ 1e,dd cb __ 3e,fd cb __ 46,dd cb __ 7e,fd cb __ 86,dd cb __ be,fd cb __ c6,dd cb __ fe"
SUMMARY pass=9000 fail=0 unimplemented=0 excluded=0

> cargo run -p z180-cli -- sst --dir tests/sst/v1
SUMMARY pass=640000 fail=0 unimplemented=65000 excluded=899
```

All 62 documented DDCB/FDCB files pass all 62,000 cases. The 65 remaining
UNIMPLEMENTED files are the P3.4 ED surface only.

The workspace quality authorities passed:

```text
> cargo test --workspace
z180-cli: 12 passed; 0 failed
z180-core: 16 passed; 0 failed
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.62s

> cargo fmt --check
```

The format check completed successfully with no output. Phase 3 task 4 is the
next unchecked plan item.

### P3.4 — standard SST conflict with defined Z180 ED opcodes — RESOLVED (2026-07-21)

Direct UM0050 Table 50 authority and the first focused ED execution run prove
that the current Gate G3 corpus rule is internally inconsistent:

- UM0050 defines ED4C/5C/6C/7C as MLT, ED64 as TST immediate, ED74 as TSTIO,
  and ED76 as SLP.
- The standard `tests/sst/v1` files at those bytes encode undocumented Z80
  aliases instead: NEG for the MLT/TST/TSTIO bytes and IM1 for ED76.
- Appendix A currently permits SST exclusion if and only if an opcode is
  undefined on Z180, so these defined-but-semantically-different files cannot
  be excluded under the controlling policy.
- Gate G3 simultaneously requires every documented v1 file to PASS and the
  Z180 additions to use their UM0050 semantics. No implementation can satisfy
  both expectations for the same opcode bytes.

The focused command exited nonzero with
`SUMMARY pass=14819 fail=3181 unimplemented=0 excluded=0`. ED4C demonstrates
the semantic conflict directly: the corpus expects NEG accumulator results
while the UM-defined implementation multiplies BC. ED76 expects IM1 while the
UM-defined implementation enters sleep. Separate ED42/ED4A flag mismatches
also remain to be corrected within P3.4 after the authority conflict is
resolved.

Q authorized the narrow consistent corpus-policy correction on 2026-07-21.
Appendix A and Gate G3 now exclude standard v1 files whose undocumented Z80
aliases occupy bytes repurposed as defined Z180 operations: ED4C/5C/6C/7C,
ED64, ED74, and ED76. Their Z180 semantics remain authoritative through the
reference-generated `tests/z180-sst` instruction cases required by P3.7. This
policy change does not alter opcode execution or rewrite either corpus.

### P3.4 — standard RETI IFF semantics conflict with Z180 — RESOLVED (2026-07-21)

After applying the authorized seven-file alias exclusion and fixing the
independent 16-bit ADC/SBC zero-flag defect, the exact focused ED run produced
`SUMMARY pass=14489 fail=511 unimplemented=0 excluded=3`. All remaining
failures are the standard `ed 4d` file comparing IFF1 after RETI.

Direct inspection of the UM0050 Table 45 page image, printed p. 230, states
that RETI pops PC without changing IEF1 or IEF2. RETN separately pops PC and
copies IEF2 into IEF1. The current core implements that exact distinction.
The standard Z80 corpus expects RETI to copy IFF2 into IFF1, so changing core
execution to pass it would violate the manual.

The subsequent block was invalid: Q's existing authorization covered standard
v1 files whose tested meaning conflicts with the defined Z180 meaning; it was
not limited to renamed undocumented aliases. Appendix A and Gate G3 now state
that authorized rule without the invented narrowing. Standard `ed 4d` is
excluded, the verification log names its exact contradiction, and a pinned
core regression proves that RETI preserves both IEFs while RETN alone restores
IEF1 from IEF2. The UM0050 behavior remains unchanged.

### P3.4 — ED page (2026-07-21)

The ED metadata table now contains exactly the 92 populated UM0050 Table 50
cells. Decode performs the second M1 fetch and dispatches all documented
standard families plus IN0/OUT0, TST/TSTIO, MLT, OTIM/OTDM, and SLP. Blank
cells remain without handlers for P3.5 TRAP. SLP owns real private sleep state
with a public read-only accessor for the P3.7 runner.

Direct manual verification corrected three execution details found during
focused SST work: 16-bit ADC/SBC derives Z from the complete result; RETI
leaves IEF1/IEF2 unchanged while RETN restores IEF1 from IEF2; and block input
uses initial BC while block output uses decremented B with C. A pinned core
regression distinguishes RETI from RETN. The authorized Appendix A policy
excludes eight standard ED files whose transitions contradict defined Z180
semantics: 4C, 4D, 5C, 64, 6C, 74, 76, and 7C.

The focused non-P3.6 ED selection passed every applicable standard case:

```text
SUMMARY pass=38000 fail=0 unimplemented=0 excluded=23
```

The rebuilt complete-corpus diagnostic now reports:

```text
SUMMARY pass=689986 fail=7014 unimplemented=0 excluded=907
```

Only ED A2/A3/AA/AB/B2/B3/BA/BB fail, and every reported first difference is
`f`. Cases whose flags already match pass completely; direct Table 46 and raw
A2/A3 case audits establish the shared branch's input/output port ordering.
P3.6 explicitly owns the remaining block-operation flag subtleties, so P3.4
does not alter those flags ahead of their numbered task.

Workspace tests passed 13 CLI tests, 18 core tests, and doc tests. The final
warnings-denied Clippy and format check passed:

```text
> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.66s

> cargo fmt --check
```

Phase 3 task 5 is the next unchecked plan item.

### P3.5 — undefined-opcode TRAP (2026-07-21)

Undefined opcode fetches now take the synchronous Z180 TRAP path instead of
returning an unimplemented sentinel. Reset initializes ITC to `0x01`; a TRAP
preserves the three enable bits, sets TRAP, sets UFO only for an undefined
third opcode byte, preserves IEF1/IEF2, pushes the post-fetch PC, and vectors
to logical `0x0000`. Decode carries indexed-bit classification forward rather
than rereading an opcode byte, so an external-memory mapping does not observe
an extra read.

The core emits the plan's `Event::Trap` with the instruction address, opcode
bytes, and opcode-byte count. `drain_events()` exposes those events for P3.7;
the complete configurable ring and the other event variants remain owned by
Phase 7. Phase 5 retains ownership of guest ITC dispatch and write masks.

Pinned tests cover undefined second-byte CB/DD/ED/FD forms and undefined
third-byte DDCB/FDCB forms. They verify the stacked address, R increments,
ITC TRAP/UFO state, unchanged IEFs, logical-zero vector, cycle/event data, and
event draining. The exact P3.5 targeted gate and workspace regression pass:

```text
> cargo test -p z180-core trap
2 passed; 0 failed

> cargo test --workspace
z180-cli: 13 passed; 0 failed
z180-core: 19 passed; 0 failed
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.85s

> cargo fmt --check
```

The format check completed successfully with no output. Phase 3 task 6 is the
next unchecked plan item.

### P3.6 — ED flag subtleties (2026-07-21)

Direct page-image reading of UM0050 Tables 38, 39, 43, and 46 confirmed that
the existing NEG, RLD/RRD, block-transfer, and block-search documented flags
were already correct. Table 46 defines standard block-I/O Z from decremented B
and N from the transferred byte's most-significant bit, but marks S/H/P/V/C
undefined. The verification log now labels z-core's deterministic free choice
for those flags: the conventional standard-corpus formula for single transfers
plus the pinned H/P/V correction during a nonterminal repeat.

The first formula made all four nonrepeat files pass but exposed the distinct
repeat correction:

```text
SUMMARY pass=5037 fail=2963 unimplemented=0 excluded=0
```

After applying the repeat correction in the existing ED block-I/O branch, the
same eight-file selection passed:

```text
PASS ed a2: pass=1000 fail=0 unimplemented=0
PASS ed a3: pass=1000 fail=0 unimplemented=0
PASS ed aa: pass=1000 fail=0 unimplemented=0
PASS ed ab: pass=1000 fail=0 unimplemented=0
PASS ed b2: pass=1000 fail=0 unimplemented=0
PASS ed b3: pass=1000 fail=0 unimplemented=0
PASS ed ba: pass=1000 fail=0 unimplemented=0
PASS ed bb: pass=1000 fail=0 unimplemented=0
SUMMARY pass=8000 fail=0 unimplemented=0 excluded=0
```

The broader selection covering every P3.6 standard family passed:

```text
SUMMARY pass=19000 fail=0 unimplemented=0 excluded=0
```

The full standard corpus now has no failures or unimplemented cases:

```text
SUMMARY pass=697000 fail=0 unimplemented=0 excluded=907
```

Table 46 separately requires terminal OTIMR/OTDMR to set P/V. The core now
sets it on the final transfer, and a two-direction regression pins Z/P/V/N set
with S/H/C reset:

```text
> cargo test -p z180-core z180_repeat_block_output
1 passed; 0 failed

> cargo test --workspace
z180-cli: 13 passed; 0 failed
z180-core: 20 passed; 0 failed
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.60s

> cargo fmt --check
```

The format check completed successfully with no output. Phase 3 task 7 is the
next unchecked plan item.

### P3.7 — generated instruction and TRAP suites (2026-07-21)

The generated SST runner now executes `instruction` and `trap` cases through
the ordinary single-step path. It requires their initial Z180 fields to equal
reset state instead of injecting them, supplies and verifies their scripted
port events, compares base CPU state with the case-specific F mask, and
compares final ITC and SLP state through `Z180::itc()` and
`Z180::sleeping()`. The `mmu` family remains explicitly UNIMPLEMENTED for
Phase 5.

Gate G3 passed in full:

```text
> z180-cli sst --dir tests/sst/v1
SUMMARY pass=697000 fail=0 unimplemented=0 excluded=907

> z180-cli sst --dir tests/z180-sst
SUMMARY pass=6850 fail=0 unimplemented=200 excluded=0

> cargo test -p z180-core trap
2 passed; 0 failed
```

The 200 generated UNIMPLEMENTED cases are exactly the deferred MMU suite.
Repository-wide regression and quality gates also pass:

```text
> cargo test --workspace
z180-cli: 15 passed; 0 failed
z180-core: 20 passed; 0 failed
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.46s

> cargo fmt --all -- --check
```

The format check completed successfully with no output. Gate G3 is complete;
Phase 4 task 1 is the next unchecked plan item.

## Phase 4 — Timing and ZEXDOC

### P4.1 — UM0050 cycle-count transcription (2026-07-21)

Every implemented main, CB, ED, DD, FD, DDCB, and FDCB opcode now carries
its Z180 cycle metadata in `optable.rs`, the sole production owner of cycle
numbers. The metadata represents fixed, conditional taken/untaken,
terminal/repeating block-operation, and Z80180/Z8S180 variant timing. Runtime
dispatch resolves the executed path after each instruction. HALT idle and
second-/third-opcode TRAP totals also use table-owned constants.

The values were transcribed by direct page-image reading of UM0050 Tables
38–47. The verification log records those tables plus the interrupt timing
figures: NMI 11 states, INT0 Mode 0 with the shown RST response 13, INT0 Mode
1 11, and vectored acknowledgement 18. It also records the complete 17- and
23-state second-/third-opcode TRAP totals and the baseline Z80180 RETI
22-state refetch sequence versus the later Z8S180 12-state form.

Coverage and repository gates pass:

```text
> cargo test -p z180-core timing
4 passed; 0 failed

> cargo test --workspace
z180-cli: 15 passed; 0 failed
z180-core: 25 passed; 0 failed
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo]

> cargo fmt --all -- --check
```

The format check completed successfully with no output. Gate G4 remains in
progress; Phase 4 task 2, DCNTL memory/I-O wait-state insertion, is the next
unchecked plan item.

### P4.2 — DCNTL wait-state insertion (2026-07-21)

CPU execution now counts every memory and external-I/O access made during a
step and adds the waits selected by DCNTL to the base timing from `optable.rs`.
Bits 7–6 provide 0–3 memory waits and bits 5–4 provide 1–4 external-I/O
waits. DCNTL resets to `F0h`, selecting the verified maximum of three memory
and four external-I/O waits; the verification log records the manual's
contradictory register-table reset row and the controlling prose evidence.

Conditional DJNZ/JR/JP/CALL paths now fetch their operand bytes even when the
condition is false, so their memory waits reflect the actual bus cycles.
Third-opcode TRAP performs its documented displacement and effective-address
reads before stacking. HALT-mode idle performs the Figure 20 memory read at
the address following HALT, so its three-state base cycle also receives the
programmed memory waits. Existing external-I/O instructions all route through
the counted private path; no public DCNTL configuration surface was added
ahead of the Phase 5 internal-I/O register table.

Repository gates pass:

```text
> cargo test -p z180-core timing
5 passed; 0 failed

> cargo test --workspace
z180-cli: 15 passed; 0 failed
z180-core: 26 passed; 0 failed
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo]

> cargo fmt --all -- --check
```

The format check completed successfully with no output. Gate G4 remains in
progress; Phase 4 task 3, `docs/timing-notes.md`, is the next unchecked plan
item.

### P4.3 — timing-model notes (2026-07-21)

`docs/timing-notes.md` now defines the timing contract. It distinguishes the
implemented instruction base states and DCNTL access waits from the deliberate
v0.1 approximations: no T-state bus waveform, zero cycle cost for dynamic RAM
refresh, and one-byte DMA timing units interleaved with the instruction-level
CPU timeline. It records why those approximations preserve the observable
cycle/peripheral contract without adding bus-phase machinery to the hot path.

The document also fixes the public accounting unit as system-clock phi cycles,
defines `step()`, `run()`, and `cycle_count()` behavior, records that reset does
not rewind elapsed time, and marks internal-I/O timing and DMA/refresh runtime
behavior as owned by their later plan phases rather than claiming they already
exist.

Repository gates pass:

```text
> cargo test --workspace
z180-cli: 15 passed; 0 failed
z180-core: 26 passed; 0 failed
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo]

> cargo fmt --all -- --check
```

The format check completed successfully with no output. Gate G4 remains in
progress; Phase 4 task 4, the hand-computed timing spot-check suite, is the
next unchecked plan item.

### P4.4 — hand-computed timing spot checks (2026-07-21)

The core timing tests now contain 26 explicit short programs whose totals are
hand-computed from instruction base states plus reset-state DCNTL waits and
asserted through `cycle_count()`. Coverage includes fixed register and memory
forms; JR, DJNZ, JP, CALL, and RET taken/untaken paths; EX DE,HL, EXX, and
EX (SP),HL; MLT; LDI; terminal and repeating LDIR; OTIM and internally
repeating OTIMR; and external input/output.

The exact spot-check test and the complete timing gate pass:

```text
> cargo test -p z180-core timing_spot_checks_hand_computed_program_totals
1 passed; 0 failed

> cargo test -p z180-core timing
6 passed; 0 failed

> cargo test --workspace
z180-cli: 15 passed; 0 failed
z180-core: 27 passed; 0 failed
Doc-tests z180_core: 0 passed; 0 failed

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo]

> cargo fmt --all -- --check
```

The format check completed successfully with no output. Gate G4 remains in
progress; Phase 4 task 5, the full ZEXDOC run, is the next unchecked plan
item.

### P4.5 — ZEXDOC hard stop (2026-07-21)

The exact required command was run against the pinned vendored ZEXDOC binary:

```text
> cargo run -p z180-cli -- zex tests/vendor/zex/zexdoc.com
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.18s
     Running `target\debug\z180-cli.exe zex tests/vendor/zex/zexdoc.com`
Z80 instruction exerciser
<adc,sbc> hl,<bc,de,hl,sp>....  OK
add hl,<bc,de,hl,sp>..........  OK
add ix,<bc,de,ix,sp>..........  OK
add iy,<bc,de,iy,sp>..........  OK
aluop a,nn....................  OK
aluop a,<b,c,d,e,h,l,(hl),a>..  OK
aluop a,<ixh,ixl,iyh,iyl>.....
```

The process exited with code 0 at that point. It did not print `OK` for the
active group and did not print the required final `Tests complete` line, so
Gate G4 failed.

**RESOLVED BLOCKER:** Stock ZEXDOC requires the undocumented Z80
IXH/IXL/IYH/IYL operations exercised by the active group. PLAN.md section 3.1
and Appendix A
require those operations to be undefined on Z180 and to trigger TRAP. The
implemented DD/FD tables follow that requirement, and the ZEX harness had
mistaken the resulting TRAP vector to logical 0000h for a successful CP/M warm
boot. Consequently, the original stock-ZEXDOC transcript and the required Z180
undefined-opcode semantics could not both be true. Under Q's existing
authorization, P4.5/G4 now names the pinned `zexdoc-z180.com` derivative. Its
fixed-size pointer table omits exactly the nine descriptors containing
mandatory-TRAP opcodes; the stock binary remains as provenance. The harness now
reports `Event::Trap` as an error before accepting a subsequent PC of 0000h.

### P4.5 — Z180-compatible ZEXDOC completion (2026-07-21)

The stock and derived binaries have SHA-256 values
`34923a7ed82285d3038b2d54bd64899e12173eebb61f9d07b4fc72e78af2ae8f`
and `349f67340953ed359692ccda23bae7dca9ea64fa766427ae0a4f2de2301ea588`,
respectively. The derived binary is byte-identical before the test-pointer
table and from file offset `0xC2` onward. Its 58 retained descriptor pointers
are followed by ten zero words in the original 136-byte table.

Gate G4 timing output:

```text
> cargo test -p z180-core timing
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.04s
     Running unittests src\lib.rs (target\debug\deps\z180_core-760d8d6d22545700.exe)

running 6 tests
test optable::tests::timing_metadata_covers_fixed_conditional_repeat_and_variant_rows ... ok
test optable::tests::every_implemented_opcode_has_um0050_timing ... ok
test optable::tests::timing_resolution_selects_the_executed_path ... ok
test tests::timing_applies_dcntl_memory_and_external_io_waits ... ok
test tests::timing_selects_conditional_and_repeat_paths ... ok
test tests::timing_spot_checks_hand_computed_program_totals ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out; finished in 0.01s
```

Full Z180-compatible ZEXDOC transcript:

```text
> cargo run -p z180-cli -- zex tests/vendor/zex/zexdoc-z180.com
   Compiling z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.83s
     Running `target\debug\z180-cli.exe zex tests/vendor/zex/zexdoc-z180.com`
Z80 instruction exerciser
<adc,sbc> hl,<bc,de,hl,sp>....  OK
add hl,<bc,de,hl,sp>..........  OK
add ix,<bc,de,ix,sp>..........  OK
add iy,<bc,de,iy,sp>..........  OK
aluop a,nn....................  OK
aluop a,<b,c,d,e,h,l,(hl),a>..  OK
aluop a,(<ix,iy>+1)...........  OK
bit n,(<ix,iy>+1).............  OK
bit n,<b,c,d,e,h,l,(hl),a>....  OK
cpd<r>........................  OK
cpi<r>........................  OK
<daa,cpl,scf,ccf>.............  OK
<inc,dec> a...................  OK
<inc,dec> b...................  OK
<inc,dec> bc..................  OK
<inc,dec> c...................  OK
<inc,dec> d...................  OK
<inc,dec> de..................  OK
<inc,dec> e...................  OK
<inc,dec> h...................  OK
<inc,dec> hl..................  OK
<inc,dec> ix..................  OK
<inc,dec> iy..................  OK
<inc,dec> l...................  OK
<inc,dec> (hl)................  OK
<inc,dec> sp..................  OK
<inc,dec> (<ix,iy>+1).........  OK
ld <bc,de>,(nnnn).............  OK
ld hl,(nnnn)..................  OK
ld sp,(nnnn)..................  OK
ld <ix,iy>,(nnnn).............  OK
ld (nnnn),<bc,de>.............  OK
ld (nnnn),hl..................  OK
ld (nnnn),sp..................  OK
ld (nnnn),<ix,iy>.............  OK
ld <bc,de,hl,sp>,nnnn.........  OK
ld <ix,iy>,nnnn...............  OK
ld a,<(bc),(de)>..............  OK
ld <b,c,d,e,h,l,(hl),a>,nn....  OK
ld (<ix,iy>+1),nn.............  OK
ld <b,c,d,e>,(<ix,iy>+1)......  OK
ld <h,l>,(<ix,iy>+1)..........  OK
ld a,(<ix,iy>+1)..............  OK
ld <bcdehla>,<bcdehla>........  OK
ld a,(nnnn) / ld (nnnn),a.....  OK
ldd<r> (1)....................  OK
ldd<r> (2)....................  OK
ldi<r> (1)....................  OK
ldi<r> (2)....................  OK
neg...........................  OK
<rrd,rld>.....................  OK
<rlca,rrca,rla,rra>...........  OK
<set,res> n,<bcdehl(hl)a>.....  OK
<set,res> n,(<ix,iy>+1).......  OK
ld (<ix,iy>+1),<b,c,d,e>......  OK
ld (<ix,iy>+1),<h,l>..........  OK
ld (<ix,iy>+1),a..............  OK
ld (<bc,de>),a................  OK
Tests complete
```

Every retained line reports `OK`, the final `Tests complete` line is present,
and the process completed without an `Event::Trap` diagnostic. Gate G4 is
complete. Final P4.5 regression validation also passed: `cargo test
--workspace` ran 16 z180-cli tests and 27 z180-core tests with no failures;
`cargo fmt --all -- --check` completed with no output; and `cargo clippy
--workspace --all-targets -- -D warnings` completed cleanly. Phase 5 task 1 is
the next unchecked plan item.

## Phase 5 — Interrupts, MMU, internal I/O window

### P5.1 — Internal I/O register file and dispatch (2026-07-21)

`ioregs.rs` is the single 64-entry source of truth for internal-register reset
values, read masks, write masks, Z80180/Z8S180 availability, and side-effect
selectors. `Z180` owns one register file; the former standalone DCNTL and ITC
fields have been removed. The public side-effect-free `io_reg_peek` view is
now implemented.

All existing instruction forms continue through the existing `read_io` and
`write_io` path. That path now applies ICR relocation, requires a zero high
byte for internal decode, duplicates every internal cycle on `HostBus`, ignores
duplicate external read data, and excludes internal cycles from DCNTL's
external-I/O waits. Waits are accumulated when each access occurs so a DCNTL
write cannot retroactively alter its own opcode/operand fetches.

The plan's register mnemonic at 0Bh was corrected from `TRDR` to UM0050's
`TRD`; its Phase 5 MMU typo `BAR` was corrected to `CBAR`; and the `HostBus`
contract now records the manufacturer's duplicate-cycle behavior. The
verification log records the complete map/mask/reset audit and the explicit
deterministic choices for manufacturer-unspecified values.

Focused P5.1 authority:

```text
> cargo test -p z180-core ioregs -- --nocapture
   Compiling z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.16s
     Running unittests src\lib.rs (target\debug\deps\z180_core-760d8d6d22545700.exe)

running 5 tests
test tests::ioregs_decode_relocation_and_duplicate_bus_cycles_match_um0050 ... ok
test tests::ioregs_reset_masks_and_variants_match_um0050 ... ok
test tests::ioregs_write_masks_and_special_effects_match_um0050 ... ok
test tests::ioregs_in0_tstio_and_otim_use_internal_data_and_duplicate_the_bus ... ok
test tests::ioregs_internal_cycles_do_not_receive_external_io_waits ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 27 filtered out; finished in 0.00s
```

The generated instruction families that exercise every P5.1-routed Z180 I/O
form also pass through the real SST runner:

```text
> cargo run -p z180-cli -- sst --dir tests/z180-sst --only ed00,ed01,ed74,ed83,ed8b,ed93,ed9b
PASS ed00: pass=200 fail=0 unimplemented=0
PASS ed01: pass=200 fail=0 unimplemented=0
PASS ed74: pass=200 fail=0 unimplemented=0
PASS ed83: pass=200 fail=0 unimplemented=0
PASS ed8b: pass=200 fail=0 unimplemented=0
PASS ed93: pass=200 fail=0 unimplemented=0
PASS ed9b: pass=200 fail=0 unimplemented=0
SUMMARY pass=1400 fail=0 unimplemented=0 excluded=0
```

Final workspace authority:

```text
> cargo test --workspace
running 16 tests
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 32 tests
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.72s

> cargo fmt --all -- --check
```

The format check completed with no output. P5.2 is the next unchecked task;
Gate G5 remains pending until all four Phase 5 tasks are complete.

### P5.2 — MMU translation and generated MMU execution (2026-07-21)

`Z180` now owns a 16-entry logical-page to physical-base table computed from
CBR, BBR, and CBAR at construction, reset, and every write to those three
internal registers. `mmu_translate` is one indexed base lookup plus the
12-bit page offset. Every instruction fetch and memory operand continues
through the existing `read_logical`/`write_logical` ownership path and is now
translated; physical `mem_peek`/`mem_poke` remain host debugging APIs.

The generated MMU runner uses a 1 MiB RAM map and programs CBR, CBAR, then BBR
with ordinary `OUT0 (n),B` instructions. It observes the required duplicate
external writes for all three internal-I/O cycles. Each of the 16 probes is
checked against `mmu_translate` and then read through a real translated
`LD A,(HL)` instruction whose fetch is itself translated. The runner restores
the generated ordinary CPU state afterward, leaves the programmed MMU state
intact, and compares CBR/BBR/CBAR with the remaining `z180` state.

The fixed core tests cover reset identity/restoration, immediate write
effects, real internal-I/O programming, and translated fetch/read/write paths.
The Phase 5 proptest covers arbitrary CBR/BBR/CBAR/logical inputs against an
independent closed-form formula.

Focused core authority:

```text
> cargo test -p z180-core mmu -- --nocapture
   Compiling z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.05s
     Running unittests src\lib.rs (target\debug\deps\z180_core-5f3557b25cb550c6.exe)

running 3 tests
test tests::mmu_reset_and_internal_io_writes_recompute_all_pages ... ok
test tests::mmu_translates_instruction_fetches_reads_and_writes ... ok
test tests::mmu_translation_array_matches_closed_form ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 32 filtered out; finished in 0.70s
```

Generated 200-case MMU authority after the final runner hardening:

```text
> cargo run -p z180-cli -- sst --dir tests/z180-sst --only mmu
   Compiling z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.42s
     Running `target\debug\z180-cli.exe sst --dir tests/z180-sst --only mmu`
PASS mmu: pass=200 fail=0 unimplemented=0
SUMMARY pass=200 fail=0 unimplemented=0 excluded=0
```

Final workspace authority:

```text
> cargo test --workspace
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 35 tests
test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.71s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.54s

> cargo fmt --all -- --check
```

The format check completed with no output. P5.3, interrupt machinery, is the
next unchecked task; Gate G5 remains pending until P5.3 and P5.4 complete.

### P5.3 — Interrupt machinery (2026-07-21)

The core now exposes the plan-owned logical-assertion APIs for INT0/INT1/INT2
and NMI. NMI assertion edges latch until sampled; external maskable inputs are
level-sampled and individually gated by ITC before IFF1 is applied. Qualified
internal peripheral requests enter the same fixed-priority selector; their
individual device enable and request conditions remain owned by the Phase 6
peripherals.

The selector implements the UM0050 order TRAP, NMI, INT0, INT1, INT2, PRT0,
PRT1, DMA0, DMA1, CSI/O, ASCI0, ASCI1. The former plan ordering of DMA0 before
PRT1 was corrected against Figure 31 and Table 9. INT1, INT2, and internal
sources vector through I, IL bits 7–5, and their fixed low codes. INT0 uses the
documented Phase 5 fixed `FFh` acknowledge choice: RST 38h in Mode 0, 0038h in
Mode 1, and I:FFh vector-table lookup in Mode 2. NMI, maskable IFF changes, EI
shadow, stack/vector waits, and distinct HALT/SLP wake behavior follow the
directly verified manual behavior recorded in `docs/verification-log.md` and
`docs/timing-notes.md`.

Focused P5.3 authority:

```text
> cargo test -p z180-core interrupts_ -- --nocapture
   Compiling z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.23s
     Running unittests src\lib.rs (target\debug\deps\z180_core-5f3557b25cb550c6.exe)

running 5 tests
test tests::interrupts_nmi_is_edge_latched_and_preserves_iff1_in_iff2 ... ok
test tests::interrupts_ei_shadow_defers_maskable_service_for_one_instruction ... ok
test tests::interrupts_vector_through_i_il_in_um0050_priority_order ... ok
test tests::interrupts_int0_modes_use_fixed_ff_acknowledge_data ... ok
test tests::interrupts_halt_and_sleep_follow_distinct_wake_rules ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 34 filtered out; finished in 0.00s
```

Final workspace authority:

```text
> cargo test --workspace
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

running 39 tests
test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.77s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.77s

> cargo fmt --all -- --check
```

The format check completed with no output. P5.4, the complete unit-test matrix
and MMU/ICR boundary regressions, is the next unchecked task; Gate G5 remains
pending until P5.4 is complete.

### P5.4 — Phase 5 boundary test matrix (2026-07-21)

The interrupt dispatch matrix exercises all 11 `IrqSource` variants across
enabled and disabled source conditions and both IFF1 states, for 44 explicit
combinations. It verifies service/no-service decisions, vectors, cycle totals,
stacked PCs, and IFF results. External lines prove ITC gating while asserted;
internal cases represent the presence or absence of a qualified Phase 6
peripheral request.

The fixed MMU cases cover BA=CA, BA=0, CA=F, and 20-bit wrap at 1 MiB for both
CBR and BBR relocation. The ICR round-trip uses ordinary OUT0 instructions to
move the internal window from 00h–3Fh to 40h–7Fh and back, then proves the
active/inactive addresses and exact duplicate bus writes.

Focused P5.4 authorities:

```text
> cargo test -p z180-core interrupts_vector_dispatch_matrix -- --nocapture
running 1 test
test tests::interrupts_vector_dispatch_matrix_covers_every_source_gate_and_iff_state ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 41 filtered out; finished in 0.01s

> cargo test -p z180-core mmu_boundary_cases -- --nocapture
running 1 test
test tests::mmu_boundary_cases_cover_empty_regions_and_one_mibibyte_wrap ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 41 filtered out; finished in 0.00s

> cargo test -p z180-core ioregs_icr_relocation_round_trip -- --nocapture
running 1 test
test tests::ioregs_icr_relocation_round_trip_uses_each_active_window ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 41 filtered out; finished in 0.00s
```

Final workspace authority:

```text
> cargo test --workspace
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 42 tests
test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.86s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.72s

> cargo fmt --all -- --check
```

The format check completed with no output. All four Phase 5 tasks are now
implemented; Gate G5 is the next unchecked plan item.

### Gate G5 — PASS (2026-07-21)

Gate preflight proved that the original combined Cargo filter was not
executable (`unexpected argument 'mmu'`) and that no bare `z180-cli` executable
was installed in this checkout. `PLAN.md` now names three separate Cargo test
commands and the established `cargo run -p z180-cli -- ...` checkout form.

The first standard-SST run exposed 12 retained failures after 696,988 passes.
All were IN/INI/IND/INIR/INDR fixtures with B=00h and expected external reads
inside the reset ICR window at 0000h–003Fh. Direct fixture inspection proved
the exact ports. The core correctly returned internal-register data and
ignored the duplicate external read value, as UM0050 requires.

The runner correction does not exclude or special-case results. For each
standard case, it selects an ICR window unused by that case's expected external
ports and programs ICR through a real `OUT0 (3Fh),B` before loading the fixture
state; only the setup duplicate-bus observation is cleared. Generated Z180
cases retain reset ICR and continue exercising internal I/O. A pinned real
`ED 48 0041` fixture regression proves the setup, and all nine affected
families pass 9,000/9,000.

Final workspace/static authority after the correction:

```text
> cargo test --workspace
running 18 tests
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

running 42 tests
test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.73s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.55s

> cargo fmt --all -- --check
```

The exact Gate G5 commands then produced:

```text
> cargo run -p z180-cli -- sst --dir tests/z180-sst
SUMMARY pass=7050 fail=0 unimplemented=0 excluded=0

> cargo test -p z180-core interrupts
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 36 filtered out; finished in 0.01s

> cargo test -p z180-core mmu
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 38 filtered out; finished in 0.84s

> cargo test -p z180-core ioregs
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 36 filtered out; finished in 0.00s

> cargo run -p z180-cli -- sst --dir tests/sst/v1
SUMMARY pass=697000 fail=0 unimplemented=0 excluded=907
```

Every retained standard case passed; the 907 file-level Appendix A exclusions
are unchanged. Phase 5 and Gate G5 are complete. Phase 6 task 1, PRT0/PRT1,
is the next unchecked plan item.

## Phase 6 — On-chip peripherals

### P6.1 — PRT0/PRT1 (2026-07-21)

PRT0 and PRT1 now use the common phi/20 timer phase documented by UM0050.
Each enabled 16-bit TMDR decrements to zero, sets its read-only TIF at zero,
keeps zero observable for one timer interval, and reloads from RLDR on the
following tick. TMDR and RLDR reset to FFFFh; TMDR writes are accepted only
while the corresponding channel is stopped.

A low-byte TMDR read captures the simultaneous high byte for the following
high-byte read. TIF clearing requires the documented sequence of reading TCR
with the flag set and then reading either byte of that channel's TMDR. Each
qualified TIF/TIE request drives only its existing Phase 5 internal-interrupt
bit; IFF gating, PRT0-before-PRT1 priority, vector construction, and interrupt
acknowledge remain owned by the interrupt controller.

The first focused run passed four tests and failed only the coherent-read test:
that test had changed TMDR through a software write, whose stopped-register
write path invalidates the prior latch. The test was corrected to cross a real
phi/20 counter tick from 1300h to 12FFh, directly exercising the documented
coherent-read purpose. No production behavior changed for that correction.

Focused and final authorities:

```text
> cargo test -p z180-core prt_ -- --nocapture
running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 42 filtered out; finished in 0.00s

> cargo test --workspace
running 18 tests
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

running 47 tests
test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.71s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.99s

> cargo fmt --all -- --check
```

The formatting check completed with no output. P6.1 is complete; P6.2 FRC is
the next unchecked Phase 6 task. Gate G6 remains open until all seven Phase 6
tasks are complete.

### P6.2 — FRC (2026-07-21)

The 8-bit FRC now decrements once every ten elapsed phi clocks, wraps from
00h to FFh, and advances independently of reads. It preserves the existing
read-only register contract, continues while ICR selects I/O STOP, and RESET
restores both FFh and the divide-by-ten phase.

UM0050 printed p. 172 confirms the register, reset, read-only, and I/O STOP
behavior but omits the rate. The original Hitachi HD64180 User's Manual
§2.15, printed p. 96, explicitly states one decrement per ten phi clocks and
that reads do not affect counting; this directly verifies the rate marked
`verify` in the controlling plan.

Focused and final authorities:

```text
> cargo test -p z180-core frc_ -- --nocapture
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 47 filtered out; finished in 0.00s

> cargo test --workspace
running 18 tests
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

running 50 tests
test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.97s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.97s

> cargo fmt --all -- --check
```

The formatting check completed with no output. P6.2 is complete; P6.3
ASCI0/ASCI1 is the next unchecked Phase 6 task. Gate G6 remains open.

### P6.3 — ASCI0/ASCI1 (2026-07-21)

Both asynchronous channels now model the documented TDR/TSR and RSR/RDR
stages at byte granularity. Host receive bytes become visible only after a
complete configured frame; baseline channels retain one completed byte while
Z8S180 channels expose the four-byte receive FIFO. Guest transmit bytes move
through TDR and the inaccessible shift stage, and become host-visible through
`asci_tx_pop` only after a complete frame.

Frame timing covers 7/8 data bits, optional parity or multiprocessor bit, one
or two stop bits, the `/10` or `/30` prescaler, `/16` or `/64` sampling, and
the standard `2^SS` divisors. Z8S180 X1 bit-clock and 16-bit ASTC BRG modes
use their separate documented equation. The fixed host APIs are now present:
`asci_rx_push`, `asci_tx_pop`, `set_asci_cts`, and `set_asci_dcd`.

STAT now implements RDRF, OVRN/error clearing, TDRE/TIE/RIE qualification,
RDR read side effects, the DCD0 transition latch, CTS0/CTS1 gating, and the
Z8S180 modem-advisory and RDRF-interrupt-inhibit controls. CTS suppresses
observable TDRE without stopping an active TSR. RESET and I/O STOP stop both
channels and reset their status/control state while preserving TDR/RDR data;
I/O STOP continues to hold ASCI stopped until the mode is cleared.

The first focused run passed eight tests and failed only the vector test. That
test enabled TIE before trying to execute HALT, so the CPU correctly serviced
the request at the pre-instruction checkpoint. The test was corrected to enter
HALT first and assert the request afterward; no production behavior changed.

Focused and final authorities:

```text
> cargo test -p z180-core asci_ -- --nocapture
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 50 filtered out; finished in 0.00s

> cargo test --workspace
running 18 tests
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

running 59 tests
test result: ok. 59 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.82s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.92s

> cargo fmt --all -- --check
```

The formatting check completed with no output. P6.3 is complete; P6.4 CSI/O
is the next unchecked Phase 6 task. Gate G6 remains open.

### P6.4 — CSI/O (2026-07-21)

The clocked serial port now models one unbuffered, half-duplex 8-bit transfer.
Setting TE starts transmission from TRD; setting RE makes the receive shift
stage ready for `csio_rx_push`. Completion clears the active enable, sets EF,
and either makes the byte available through `csio_tx_pop` or places it in TRD.
TRD reads and writes clear EF, and software clearing RE or TE aborts the active
operation immediately.

All seven internal speed selections use the UM0050 Table 22 divisors: one bit
consumes `20 << SS` phi cycles and one byte consumes `160 << SS` cycles for
SS=0 through 6. SS=7 selects external CKS; because the fixed public API has no
CKS edge source, such a transfer remains active until software aborts it. The
RXS/CTS1 pin multiplex requires CTS1E clear before receive can start.

EF and EIE directly qualify the existing CSI/O internal interrupt request and
vector. RESET restores CNTR to 07h while preserving TRD. I/O STOP clears
EF/RE/TE and aborts transfer for the duration of the mode, preserves EIE and
TRD, and prevents later CNTR writes from restarting the peripheral until the
mode is cleared.

Focused and final authorities:

```text
> cargo test -p z180-core csio_ -- --nocapture
running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 59 filtered out; finished in 0.00s

> cargo test --workspace
running 18 tests
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

running 64 tests
test result: ok. 64 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.84s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.74s

> cargo fmt --all -- --check
```

The formatting check completed with no output. P6.4 is complete; P6.5
DMA0/DMA1 is the next unchecked Phase 6 task. Gate G6 remains open.

### P6.5 — DMA0/DMA1 (2026-07-21)

Both DMA channels now execute from the existing internal-register state.
DMA0 performs 20-bit physical memory-to-memory and memory-to/from-I/O
transfers using SAR0, DAR0, BCR0, and DMODE; increment, decrement, fixed
memory-mapped-I/O, fixed true-I/O, burst, and cycle-steal behavior follow the
direct UM0050 mode tables. DMA1 performs memory-to/from-I/O using 20-bit MAR1,
fixed 16-bit IAR1, BCR1, and the four DCNTL DIM combinations.

DSTAT implements the active-low DWE write protocol, automatic DME enable,
completion clearing of DE, and level `!DE && DIE` requests. The fixed
`set_dreq(ch, level)` API uses logical assertion: level sense continues while
asserted and edge sense permits one byte per assertion edge. DMA0 wins
simultaneous channel requests, and automatic DMA0 memory-to-memory excludes
DMA1 until termination. NMI clears DME while preserving restart state; RESET
stops DMA, resets control state, and preserves the manual-named address/count
registers.

Each byte consumes the UM's two three-clock bus cycles plus DCNTL waits:
`6 + 2*MWI` for memory-to-memory and `6 + MWI + IWI` for memory-to-I/O, where
IWI includes the mandatory true-I/O wait. Zero-wait A15/A16 address crossings
receive the internal Ti state. The documented v0.1 byte/instruction boundary
drains burst transfers before the next instruction, performs one cycle-steal
or edge-request byte per instruction boundary, and advances all other
peripherals over DMA elapsed time before CPU interrupt sampling and fetch.

Focused and final authorities:

```text
> cargo test -p z180-core dma -- --nocapture
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 64 filtered out; finished in 0.00s

> cargo test --workspace
running 18 tests
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

running 70 tests
test result: ok. 70 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.85s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.11s

> cargo fmt --all -- --check
```

The formatting check completed with no output. P6.5 is complete; P6.6
peripheral-to-interrupt integration and pairwise priority tests are the next
unchecked Phase 6 task. Gate G6 remains open.

### P6.6 — peripheral interrupt integration (2026-07-21)

The seven internal peripheral sources are now covered as real adjacent
priority pairs: PRT0 > PRT1 > DMA0 > DMA1 > CSI/O > ASCI0 > ASCI1. The
integration authority creates both requests through each device's documented
register/state owner, verifies both qualified request bits, acknowledges and
checks the higher source's I:IL vector, clears that source through its real
guest-visible protocol, then acknowledges and checks the lower vector.

This closes the gap left by the earlier selector unit tests, which directly
assigned `internal_irq_pending` to isolate vector dispatch. No production
change was required: the implemented peripheral qualification and fixed
priority selector compose correctly under all six adjacent competitions.

Focused and final authorities:

```text
> cargo test -p z180-core peripheral_interrupts_vector_in_every_adjacent_priority_pair -- --nocapture
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 70 filtered out; finished in 0.00s

> cargo test --workspace
running 18 tests
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

running 71 tests
test result: ok. 71 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.71s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.88s

> cargo fmt --all -- --check
```

The formatting check completed with no output. P6.6 is complete. The next
task originally consumed `save_state()` before the plan created that method;
the existing state-feature task has therefore been moved ahead of its
determinism consumer without changing either task's scope. P6.7 state support
is next, followed by P6.8's 10-million-cycle determinism test. Gate G6 remains
open.

### P6.7 — versioned save state (2026-07-21)

The optional, default-off `state` feature now provides the plan's exact
`save_state()` and `load_state()` surface using a raw version byte followed by
a postcard payload. The payload includes every core-owned state field:
registers, memory mapping and contents, variant, internal registers, elapsed
and in-progress timing, CPU control and pin latches, DMA requests, all
peripheral pipelines and queues, and pending events. The generic HostBus is
host-owned and remains outside the snapshot; the derived MMU page cache is
recomputed after an atomic successful load.

The error authority covers missing, unsupported, truncated, and structurally
invalid payloads without mutation. The 32-case property authority snapshots
machines with active PRT, ASCI transmit/receive, CSI/O, DMA, memory, and
register state, restores into a fresh machine, then proves equal consumed
cycles, serial output, events, and final state after both machines run the
same generated cycle budget. Repeated state serialization is byte-identical.
The default no-feature dependency tree still contains only `z180-core`.

Focused and final authorities:

```text
> cargo test -p z180-core --features state save_state_ -- --nocapture
running 2 tests
test tests::save_state_version_and_decode_errors_are_atomic ... ok
test tests::save_state_round_trip_resume_matches_uninterrupted_execution ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 71 filtered out; finished in 0.35s

> cargo test -p z180-core --features state
running 73 tests
test result: ok. 73 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.74s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo test --workspace
running 18 tests
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

running 71 tests
test result: ok. 71 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.76s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.41s

> cargo clippy -p z180-core --all-targets --features state -- -D warnings
    Checking windows-sys v0.61.2
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking tempfile v3.27.0
    Checking rusty-fork v0.3.1
    Checking proptest v1.11.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.24s

> cargo tree -p z180-core --no-default-features --edges normal
z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)

> cargo fmt --all -- --check
```

The formatting check completed with no output. P6.7 is complete pending its
task commit, push, and CI. P6.8's exact 10-million-cycle determinism authority
is next only after P6.7 lands. Gate G6 remains open.

### P6.8 — 10-million-cycle determinism (2026-07-21)

The determinism authority constructs two independent one-MiB machines and
applies the same real input sequence to both: a repeating PRT0 timer, ASCI0
transmit and receive traffic, and a 256-byte DMA0 cycle-steal copy between
physical pages outside the CPU's identity-mapped logical 64 KiB. An undefined
DD 76 instruction spanning FFFFh/0000h emits one real TRAP event before 76h
executes as HALT. With two memory waits, every remaining CPU/DMA boundary is a
five-cycle multiple, so both `run(10_000_000)` calls consume exactly—not at
least—10,000,000 phi cycles.

The test directly requires both cycle counters to equal 10,000,000, the event
stream to be nonempty, the versioned `save_state()` byte vectors to match, and
the drained event vectors to match. DMA data is never executed as code, so the
script measures deterministic peripheral scheduling rather than accidental
opcode behavior.

The Gate G6 combined SST line still named a bare `z180-cli` executable even
though Gate G5 had already proven none is installed in the checkout. Its
command spelling was corrected to the established pair of ordered
`cargo run -p z180-cli -- ...` invocations; no gate scope or outcome changed.

Exact P6.8 and Gate G6 authorities:

```text
> cargo test -p z180-core --features state determinism_timer_asci_dma_matches_after_ten_million_cycles -- --nocapture
running 1 test
test tests::determinism_timer_asci_dma_matches_after_ten_million_cycles ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 73 filtered out; finished in 0.63s

> cargo test -p z180-core
running 71 tests
test result: ok. 71 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.76s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo run -p z180-cli -- sst --dir tests/sst/v1 && cargo run -p z180-cli -- sst --dir tests/z180-sst
SUMMARY pass=697000 fail=0 unimplemented=0 excluded=907
SUMMARY pass=7050 fail=0 unimplemented=0 excluded=0
```

P6.8 and Gate G6 have passing functional evidence. Phase 6 is complete pending
the task's final static gates, commit, push, and CI; Phase 7 must not begin
before that landing completes.

Final integrated and static authorities:

```text
> cargo test -p z180-core --features state
running 74 tests
test result: ok. 74 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.79s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo test --workspace
running 18 tests
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s

running 71 tests
test result: ok. 71 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.76s

Doc-tests z180_core
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.83s

> cargo clippy -p z180-core --all-targets --features state -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.94s

> cargo fmt --all -- --check
```

The formatting check completed with no output. Phase 6 and Gate G6 are
complete pending only the P6.8 commit, push, and exact CI completion.

## Phase 7 — Debug, trace, save-state, disassembler

### P7.1 — Event ring, watches, and I/O/IRQ trace

Q explicitly authorized completing the missing section 2.2 event API design.
The fixed API now gives `MachineConfig` an `event_capacity` (default 4096),
uses opaque `WatchId` values with `WatchKind::{Read, Write, Both}`, and exposes
the sticky loss state through `events_lost()` and `clear_events_lost()`.
`PLAN.md` records the exact ring, reservation, memory-watch, I/O-trace,
PC-watch, and reset semantics so later bindings have one durable contract.

The core retains the newest configured number of events in chronological
order. Storage is reserved when a producer is enabled or the first
unconditional event occurs and is reused across drains. Overflow, capacity
zero, or a failed reservation drops the affected observation and sets the
sticky loss flag without panicking. CPU and DMA physical memory accesses feed
the same watch path; host `mem_peek`/`mem_poke` access does not. CPU and DMA
I/O each emit exactly one event per guest-visible access, including internal
I/O accesses with their required duplicate external bus cycle. IRQ
acknowledgements, undefined-opcode traps, and attempted ROM writes emit their
corresponding event variants.

The PC watch counts once at instruction entry, excludes HALT idle cycles, and
resets its count when changed. Hardware reset preserves watch/trace producer
configuration while clearing queued observations, loss state, and PC hits.
The optional `state` payload persists the complete debug configuration and
ring, so its version byte advanced from 1 to 2.

Targeted P7.1 authority:

```text
> cargo test -p z180-core --features state event_ -- --nocapture
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.18s
     Running unittests src\lib.rs (target\debug\deps\z180_core-5fc747acf9de75a9.exe)

running 6 tests
test tests::event_io_trace_records_cpu_dma_and_internal_duplicate_accesses_once ... ok
test tests::event_ring_retains_newest_entries_and_loss_is_sticky ... ok
test tests::event_memory_watch_fires_exactly_on_its_physical_half_open_range ... ok
test tests::event_memory_watches_cover_dma_and_rom_write_attempts ... ok
test tests::event_irq_trace_and_pc_watch_use_acknowledge_and_instruction_boundaries ... ok
test tests::event_debug_configuration_and_ring_round_trip_in_save_state ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 74 filtered out; finished in 0.01s
```

The default and state-feature core suites passed 76/76 and 80/80 respectively.
The complete workspace regression also passed:

```text
> cargo test --workspace
running 18 tests
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

running 76 tests
test result: ok. 76 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.76s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Static authorities:

```text
> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.23s

> cargo clippy -p z180-core --all-targets --features state -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.77s

> cargo fmt --all -- --check
```

The formatting check and `git diff --check` completed with no output.

### P7.2 — Instruction trace ring

The public instruction trace uses `TraceEntry { cycle, pc, phys_pc, bytes,
len }`, `set_insn_trace(Option<usize>)`, and `drain_insn_trace()`. Enabling it
preallocates a newest-retained ring; resizing preserves the newest queued
entries that fit, `Some(0)` records nothing, and `None` disables tracing,
clears the queue, and releases storage. Draining preserves enabled storage for
reuse, while reset preserves the configured capacity and clears observations.

Capture is attached to the existing logical-read path after DMA, interrupt,
HALT, and SLP checks. It therefore records the bytes the guest actually
fetched without issuing extra memory or HostBus reads. Bytes are stored in
logical-address order even for DDCB/FDCB's `0, 1, 3, 2` fetch order. Each
normal instruction and undefined-opcode TRAP produces one entry; interrupt
acknowledgements, DMA transfers, and idle calls do not. The optional state
payload persists the trace setting and ring, advancing its version from 2 to
3.

The focused test covers MMU-translated `phys_pc`, immediate and DDCB byte
capture, an undefined `ED 31` TRAP, and the exact watched-read sequence proving
that tracing adds no bus reads. Separate tests cover overflow/resize,
allocation reuse, HALT idle exclusion, reset, zero capacity, disable, and
save/load continuation.

```text
> cargo test -p z180-core --features state insn_trace -- --nocapture
running 3 tests
test tests::insn_trace_ring_resizes_drains_resets_and_disables_exactly ... ok
test tests::insn_trace_records_fetched_bytes_physical_pc_and_traps_without_extra_reads ... ok
test tests::insn_trace_configuration_and_ring_round_trip_in_save_state ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 80 filtered out; finished in 0.01s

> cargo test -p z180-core --features state
running 83 tests
test result: ok. 83 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.74s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo test --workspace
running 18 tests
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

running 78 tests
test result: ok. 78 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.74s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy -p z180-core --all-targets --features state -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.53s

> cargo fmt --all -- --check
```

The formatting check and `git diff --check` completed with no output.

### P7.3 — Optable-driven disassembler

`z180-core::disassemble_one` now decodes all seven opcode pages directly from
the private optables and returns address, encoded bytes, length, and formatted
text. Standard and Z180 ED mnemonics are concrete optable metadata rather than
a non-renderable placeholder. Immediate values, relative targets, register
fields, restart vectors, and signed IX/IY displacements are formatted from the
descriptor operands and encoded bytes; undefined or truncated input produces
bounded `DB` records so every nonempty byte stream advances by one through four
bytes without panicking.

`z180-cli dis file.bin --org 0x0000` reads a raw binary and prints stable
address, byte, and instruction columns. The checked-in crafted binary contains
exactly one instance of each of the 77 mnemonic names. Its unit authority
checks exact coverage and the golden listing; the process integration test
launches the built CLI on that same `.bin` and compares stdout byte-for-byte.

```text
> cargo test -p z180-core disassembler -- --nocapture
running 3 tests
test disassembler::tests::disassembler_formats_immediates_indexes_relative_targets_and_unknowns ... ok
test disassembler::tests::every_implemented_optable_entry_formats_without_placeholders ... ok
test disassembler::tests::disassembly_is_total_and_lengths_tile_the_input ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 78 filtered out; finished in 0.46s

> cargo test -p z180-cli disassembler_golden_covers_every_mnemonic_once -- --nocapture
running 1 test
test dis::tests::disassembler_golden_covers_every_mnemonic_once ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 0.00s

> cargo test -p z180-cli dis_command_matches_the_every_mnemonic_golden_file -- --nocapture
running 1 test
test dis_command_matches_the_every_mnemonic_golden_file ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.40s
```

The literal user-facing command also completed successfully and its listing
matched the checked-in golden:

```text
> cargo run -p z180-cli -- dis crates/z180-cli/tests/fixtures/dis_every_mnemonic.bin --org 0x4000
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.08s
     Running `target\debug\z180-cli.exe dis crates/z180-cli/tests/fixtures/dis_every_mnemonic.bin --org 0x4000`
4000  88          ADC A,B
...
4082  A8          XOR B
```

Full regression and static authorities:

```text
> cargo test --workspace
running 20 tests
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s

running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.41s

running 81 tests
test result: ok. 81 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.74s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo test -p z180-core --features state
running 86 tests
test result: ok. 86 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.74s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

> cargo clippy --workspace --all-targets -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Checking z180-cli v0.1.0 (C:\Users\Q\code\z-core\crates\z180-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.05s

> cargo clippy -p z180-core --all-targets --features state -- -D warnings
    Checking z180-core v0.1.0 (C:\Users\Q\code\z-core\crates\z180-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.16s

> cargo fmt --all -- --check
```

The formatting check completed with no output.

The first pushed P7.3 CI run (`29873100117`) passed every Ubuntu step but its
Windows default-test step found that Git had checked the text golden out with
CRLF while the deterministic renderer emits LF. The instruction listing was
otherwise byte-identical. Repository `.gitattributes` now marks the crafted
`.bin` as binary and pins `.golden` fixtures to `text eol=lf`; the exact golden
and process tests pass locally with those attributes. Corrected CI run
`29873366711` passed Ubuntu in 1m06s and Windows in 2m02s, including both
default and state-feature test authorities.

### P7.4 — Bare-ROM runner

Q approved adding the `toml` crate after the plan's closed dependency set
blocked the required `machine.toml` parser. `z180-cli` uses `toml 1.1.3` with
only its parsing and Serde features enabled.

`z180-cli run rom.bin --cycles N --trace --config machine.toml` now constructs
the existing core `MachineConfig` directly. The strict TOML schema requires a
nonzero `clock_hz`, a `z80180` or `z8s180` `variant`, and a `regions` array.
Each region names its `kind` (`rom`, `ram`, or `external`), physical `base`, and
`size`. Exactly one ROM region receives the positional image and its size must
match exactly; the core remains the authority for alignment, overlap, and
physical-address validation. `run --help` prints a complete configuration
example.

Execution steps the core until it consumes at least the requested cycle
budget. `--trace` drains the existing one-entry instruction trace ring after
each step and prints the entry cycle, logical PC, physical PC, actual fetched
bytes, and optable-driven disassembly. Undefined instructions report the core
TRAP and a sleeping CPU reports that it cannot reach the requested budget
rather than silently succeeding early. The bare external bus deterministically
reads `ff` and ignores writes.

Targeted authority:

```text
> cargo test -p z180-cli run::tests -- --nocapture
running 5 tests
test run::tests::config_maps_clock_variant_and_all_region_kinds ... ok
test run::tests::config_rejects_unknown_fields_and_invalid_rom_bindings ... ok
test run::tests::sleeping_cpu_reports_that_the_cycle_budget_cannot_be_reached ... ok
test run::tests::trace_reports_cycle_logical_and_physical_addresses_and_instruction ... ok
test run::tests::trap_stops_the_runner_with_the_faulting_instruction ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 20 filtered out

> cargo test -p z180-cli --test run_cli -- --nocapture
running 1 test
test run_command_executes_a_configured_rom_with_trace ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

The process test invokes the literal command shape and receives this trace:

```text
000000000000  0000  000000  00           NOP
000000000006  0001  000001  00           NOP
```

Full regression and static authorities:

```text
> cargo test --workspace
running 25 tests
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 81 tests
test result: ok. 81 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

> cargo test -p z180-core --features state
running 86 tests
test result: ok. 86 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

> cargo clippy --workspace --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.10s

> cargo clippy -p z180-core --all-targets --features state -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.10s

> cargo fmt --all -- --check
```

The formatting check completed with no output.

### Gate G7 — PASS (2026-07-21)

P7.4 commit `fd30135` was pushed before the gate. GitHub Actions run
`29874375518` passed formatting, default/state Clippy, and default/state tests
on Ubuntu in 1m08s and Windows in 2m00s.

The exact workspace gate produced:

```text
> cargo test --workspace
running 25 tests
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 1 test
test dis_command_matches_the_every_mnemonic_golden_file ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 1 test
test run_command_executes_a_configured_rom_with_trace ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 81 tests
test result: ok. 81 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

The separately named disassembler golden authority produced:

```text
> cargo test -p z180-cli disassembler_golden_covers_every_mnemonic_once -- --nocapture
running 1 test
test dis::tests::disassembler_golden_covers_every_mnemonic_once ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 24 filtered out
```

The deterministic save/load/resume demonstration saves a machine after two
NOPs, creates an observably divergent machine, loads the snapshot, resumes the
saved and uninterrupted paths for the same budget, and requires their final
versioned state bytes to be identical. Its actual transcript was:

```text
> cargo test -p z180-core --features state save_load_resume_demonstration_transcript -- --nocapture
running 1 test
SAVE cycle=6 pc=0002 af=1234 state_bytes=65995
DIVERGE cycle=3 pc=0001 af=A55A
LOAD cycle=6 pc=0002 af=1234
UNINTERRUPTED cycle=15 pc=0005 af=1234
RESUMED cycle=15 pc=0005 af=1234
MATCH state_bytes=true
test tests::save_load_resume_demonstration_transcript ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 86 filtered out
```

Final gate-record regressions and static authorities:

```text
> cargo test -p z180-core --features state
running 87 tests
test result: ok. 87 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

> cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.28s

> cargo clippy -p z180-core --all-targets --features state -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.99s

> cargo fmt --all -- --check
```

The formatting check completed with no output.

All three named G7 authorities are green. Phase 7 and Gate G7 are complete
pending the gate-record commit, push, and CI; Phase 8 must not begin before
that landing completes.

## Phase 8 — Python binding, qns migration, reference differential

### P8.1 — Python binding

AUTHORIZED: Q approved preserving the plan-literal Rust method unchanged and
adding
`set_ext_map_table(&mut self, table: Option<Vec<u32>>) -> Result<(),
ConfigError>` for language bindings. It accepts exactly 1,048,576 entries,
rejects every other length atomically, and uses the same private mapper path as
`set_ext_mapper`. Python `Machine.set_ext_mapper(callable)` samples all 20-bit
inputs before mutably borrowing the machine, then installs this table; `None`
clears it. The callable is never invoked by the execution hot path.

Implementation is complete as the single P8.1 source slice. The public
package is `z180`; its private native module is `z180._native`, preserving
the package namespace required by P8.2's `z180.compat`. The extension uses
PyO3 0.29 with `abi3-py311` and the wheel is built by maturin.

`Machine(config_dict)` rejects unknown fields and maps all section 2.2
lifecycle, register, interrupt, MMU, serial, physical-memory, debug/event,
instruction-trace, and save-state APIs. It additionally exposes the core's
public pin/state controls. Events and trace entries are typed-by-`kind`
Python dictionaries; `Reg`, `IrqLine`, and `WatchKind` are Python enum-like
classes and `WatchId` stays opaque.

RAM was changed from one concatenated allocation to stable per-region stores
so remapping one page range cannot invalidate an unrelated exported region.
`Machine.ram_regions()` discovers exact contiguous RAM regions and
`Machine.ram(base)` returns a writable, zero-copy `memoryview`. The exporter
retains its owning machine. Active views reject `remap` and `load_state` with
`BufferError`; ordinary execution and debugger writes remain visible through
the view. Save-state format version advanced from 3 to 4 for the memory
layout, and state loading validates every decoded page/store reference before
commit.

The Rust core authorities for remapping and both mapper forms pass. The
default core suite passed 83/83 and the state-enabled suite passed 89/89.
The Python authority exercises the entire exposed surface, strict config,
bidirectional zero-copy behavior, storage guards, complete mapper sampling
and failure atomicity, events/traces, and save/load:

```text
> uv run --project crates/z180-py pytest crates/z180-py/tests -q
.......                                                                  [100%]
7 passed in 0.13s
```

The release wheel is genuinely abi3 with a Python 3.11 floor and contains
only the package source, native extension, and distribution metadata:

```text
> uv run --with maturin maturin build --release --out ../../target/wheels
Found pyo3 bindings with abi3-py3.11 support
Built wheel for abi3 Python >= 3.11 to
../../target/wheels/z180-0.1.0-cp311-abi3-win_amd64.whl

> uv run --isolated --python 3.11 \
    --with ./target/wheels/z180-0.1.0-cp311-abi3-win_amd64.whl \
    --with pytest pytest crates/z180-py/tests -q
.......                                                                  [100%]
7 passed in 0.13s
```

Static and full-workspace authorities:

```text
> cargo fmt --all -- --check

> cargo clippy --workspace --all-targets --all-features -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.43s

> cargo test --workspace
running 25 tests
test result: ok. 25 passed; 0 failed; 0 ignored
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored
running 89 tests
test result: ok. 89 passed; 0 failed; 0 ignored
```

P8.1 landed as `138220f`. CI run `29877242122` passed on Ubuntu in 1m13s
and Windows in 2m15s.

### P8.2 — qns compatibility layer

The current `C:\Users\Q\code\qns\qns\cpu.py` was read as the signature
authority without modifying its dirty checkout. `z180.compat.Z180` preserves
its constructor, constants, lifecycle, register, IRQ, MMU, watch, and ASCI
diagnostic surface, including the current `reset_asci_debug`,
`pc_watch_cycle`, and `pc_watch_cbar` additions.

A private `_compat_machine` native factory maps the full 20-bit physical
space as one External region. Its HostBus invokes Python memory and I/O
callbacks, masks read results to one byte, and raises the first callback
exception after the active instruction. Public `Machine(config_dict)` is
unchanged. The compat run loop pumps one instruction at a time: ASCI/CSI input
bytes remain pending until the core queue accepts them, and completed output
bytes drain to the incumbent callbacks.

The qns-visible cycle position is Python-owned so callbacks can read
`cpu.cycle_count` reentrantly while the native machine is executing. It
credits exactly the requested budget and resets to zero as qns requires;
`step`/`run` still return actual core cycles. Core lifetime-cycle semantics
remain unchanged. The core instruction trace supplies exact watch hits and
the compatibility timeline records their qns-visible cycle positions.

`asci_debug_state` returns direct core values for `status`, `cntla`, and
`tx_data_register`. The core has no public equivalent for the incumbent's
shift-stage, FIFO-depth, derived-timing, IRQ-internal, and transition-history
diagnostics; every such field is explicitly documented and returned as
zero/false, as required by P8.2.

Python compatibility authority:

```text
> uv run --project crates/z180-py pytest crates/z180-py/tests -q
.............                                                            [100%]
13 passed in 0.37s
```

The test set covers signature parity, real callback-backed instruction
execution, callback masking/error propagation, reentrant cycle reads,
reset/budget behavior, register/IRQ/MMU/watch/debug surfaces, and ASCI/CSI
queue retry and drain behavior. The rebuilt wheel contains `z180/compat.py`
and passes from an isolated Python 3.11 environment:

```text
> uv run --refresh-package z180 --isolated --python 3.11 \
    --with ./target/wheels/z180-0.1.0-cp311-abi3-win_amd64.whl \
    --with pytest pytest crates/z180-py/tests -q
.............                                                            [100%]
13 passed in 0.18s
```

Static and Rust regression authorities remained green:

```text
> cargo fmt --all -- --check

> cargo clippy --workspace --all-targets --all-features -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.24s

> cargo test --workspace
25 CLI unit + 2 CLI integration + 89 core tests passed
```

P8.2 landed as `afd0bdc`. CI run `29878130219` passed on Ubuntu in 1m14s
and Windows in 2m21s.

### P8.3 — qns internal-memory migration

RESOLVED: P8.3 was blocked because public `Machine(config_dict)` could not
combine internal RAM with qns's external I/O and flash callbacks. Q authorized
the proposed compatible constructor extension:

`Machine(config_dict=None, *, mem_read=None, mem_write=None, io_read=None,
io_write=None)`.

The public constructor now reuses the P8.2 `PythonBus`. Memory callbacks run
only for configured External pages; I/O callbacks run for external I/O and
required duplicate internal-I/O cycles. Omitted callbacks retain the P8.1
unmapped-value behavior. The first Python callback exception propagates after
the active native operation and is then cleared. No new class, adapter,
dependency, or core API was added.

The Python authorities execute internal RAM without either memory callback,
program CBAR/BBR so a guest read and write reach qns's physical
80000h-FFFFFh flash aperture, exercise external and duplicate internal I/O
cycles, retain omitted-callback defaults, reject positional callbacks, and
prove callback errors do not remain pending:

```text
> uv run --project crates/z180-py pytest crates/z180-py/tests -q
.................                                                        [100%]
17 passed in 0.36s
```

`docs/qns-migration.md` is written from the current read-only qns checkout.
It gives the exact per-profile region layout, ROM-to-core-RAM shadow
initialization, `qns/memory.py` ownership changes, V1/V2 and state-directory
conversion, memory-observer/event relocation, callback reentrancy rule,
serial/CSI queue ordering, and direct `Machine` API substitutions. It records
the internal-mode target as at least 50M cycles/sec without claiming a P8.3
measurement; the three measured rates belong to P8.4.

The release abi3 wheel contains the authorized constructor and passes the same
authority in an isolated Python 3.11 environment:

```text
> uv run --with maturin maturin build --release --out ../../target/wheels
Found pyo3 bindings with abi3-py3.11 support
Finished `release` profile [optimized] target(s) in 2.98s
Built wheel for abi3 Python >= 3.11 to
../../target/wheels/z180-0.1.0-cp311-abi3-win_amd64.whl

> uv run --refresh-package z180 --isolated --python 3.11 \
    --with ./target/wheels/z180-0.1.0-cp311-abi3-win_amd64.whl \
    --with pytest pytest crates/z180-py/tests -q
.................                                                        [100%]
17 passed in 0.16s
```

Static and workspace authorities are green. On this machine PyO3 must be
pointed at the verified 64-bit uv project interpreter; without that variable,
the PATH-selected 32-bit interpreter fails before project code is checked.

```text
> cargo fmt --all -- --check

> $env:PYO3_PYTHON =
    'C:\Users\Q\code\z-core\crates\z180-py\.venv\Scripts\python.exe'
> cargo clippy --workspace --all-targets --all-features -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.27s

> cargo test --workspace
25 CLI unit + 2 CLI integration + 89 core tests passed
```

P8.3 landed as `0e78551`. CI run `29879478732` passed on Ubuntu in 1m15s
and Windows in 2m12s.

### P8.4 — Python execution-mode benchmark

The root `bench.py` runs the same all-NOP instruction loop in the three
required modes. Each mode calibrates to a sample of at least 250ms, then
reports the median of five samples. The benchmark used the P8.3 release abi3
wheel and qns's current compiled old-CFFI binding; `CFFI_AVAILABLE` was
verified true before the run.

```text
> uv run --project C:\Users\Q\code\qns \
    --with ./target/wheels/z180-0.1.0-cp311-abi3-win_amd64.whl \
    python bench.py
mode                 budget       median seconds       cycles/sec
-------------------  -----------  -----------------  ---------------
compat callback        1,600,000           0.281288        5,688,119
internal memory      102,400,000           0.437195      234,220,593
old CFFI               3,200,000           0.266097       12,025,694
```

| Mode | Cycles/sec | Python target |
|---|---:|---:|
| z-core compat callbacks | 5,688,119 | informational |
| z-core internal memory | 234,220,593 | >= 50,000,000 PASS |
| old qns CFFI | 12,025,694 | informational |

P8.4 landed as `12350be`. CI run `29879817022` passed on Ubuntu in 1m11s
and Windows in 2m9s.

### P8.5 — optional incumbent lockstep

NOT RUN: no authorized full-state black-box API

The current public `qns.cpu.Z180` wrapper can capture the six named registers,
but it exposes no register setter and no complete CPU state load/save API. ROM
and callback wiring alone cannot establish identical starting CPU state: a
diagnostic first `DI` instruction left all other compared fields equal while
the incumbent reported reset AF=0040h and z-core reported AF=0000h. Since DI
does not initialize AF, that is an initial-state difference, not an
instruction divergence. No incumbent source was read, no adjudication claim
was made, and the non-qualifying diagnostic harness was discarded.

P8.5 is complete pending its record commit, push, and CI. P8.6 is next after
that landing completes.

## Phase 9 — WASM and TypeScript

## Phase 10 — Documentation and v0.1.0
