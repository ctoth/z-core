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

## Phase 2 — Full unprefixed opcode page

## Phase 3 — Prefixed pages, Z180 instructions, TRAP

## Phase 4 — Timing and ZEXDOC

## Phase 5 — Interrupts, MMU, internal I/O window

## Phase 6 — On-chip peripherals

## Phase 7 — Debug, trace, save-state, disassembler

## Phase 8 — Python binding, qns migration, reference differential

## Phase 9 — WASM and TypeScript

## Phase 10 — Documentation and v0.1.0
