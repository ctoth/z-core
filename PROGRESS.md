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

## Phase 4 — Timing and ZEXDOC

## Phase 5 — Interrupts, MMU, internal I/O window

## Phase 6 — On-chip peripherals

## Phase 7 — Debug, trace, save-state, disassembler

## Phase 8 — Python binding, qns migration, reference differential

## Phase 9 — WASM and TypeScript

## Phase 10 — Documentation and v0.1.0
