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

## Phase 2 — Full unprefixed opcode page

## Phase 3 — Prefixed pages, Z180 instructions, TRAP

## Phase 4 — Timing and ZEXDOC

## Phase 5 — Interrupts, MMU, internal I/O window

## Phase 6 — On-chip peripherals

## Phase 7 — Debug, trace, save-state, disassembler

## Phase 8 — Python binding, qns migration, lockstep

## Phase 9 — WASM and TypeScript

## Phase 10 — Documentation and v0.1.0
