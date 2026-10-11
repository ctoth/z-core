# Issue 42 execution notes

## Current state

- User request: resolve GitHub issue 42 with TDD, open a PR whose body contains a closing phrase, wait for green checks, and merge.
- Live issue: `ctoth/z-core#42`, "SLP consumes no cycles and never ticks the PRT; sleeping must advance time and wake on timer interrupts".
- Current branch: `agent/fix-slp-time`, created from `origin/master`.
- The prior local `master` commit `9d593cd` ("Add Rust module refactoring plan") is unrelated and is not on this branch.
- The existing untracked `crates/z180-core/src/tests/` directory and other `notes-*.md` files are unrelated user work and must not be edited or staged.
- This notes file is an operational record and must remain uncommitted.

## Findings

- The issue is valid on `origin/master`.
- `Z180::try_step()` returns only DMA cycles when `self.sleeping`, so it returns zero in the normal sleeping case.
- `Z180::try_run()` stops when `try_step()` returns zero. As a result, sleeping consumes no budget.
- `finish_step()` is the existing owner for advancing the PRT, ASCI, CSIO, FRC, and `cycle_count`.
- The existing HALT path uses `HALT_IDLE_CYCLES` with `finish_step()`.
- `sleeping()` is already exposed on the core.
- Existing timer/interrupt test helpers and setup live in the inline `tests` module in `crates/z180-core/src/lib.rs`.
- TDD red: `cargo test -p z180-core sleep_run_ -- --nocapture` failed both new tests because sleeping `run()` returned 0 instead of consuming 40/96 cycles.
- Production change: the existing sleeping branch now passes `HALT_IDLE_CYCLES` through `finish_step()`, preserving the existing owner for PRT/peripheral/cycle advancement.
- TDD green: both new `sleep_run_` regressions pass.
- Full core suite: 89 tests pass after changing the old disabled-INT1 sleep assertion from zero cycles to three advancing idle cycles.
- The workspace test gate exposed one additional stale contract in `crates/z180-cli/src/run.rs`: its test expected a sleeping CPU to fail to reach the requested cycle budget. The CLI production loop now progresses without modification, so only that obsolete test expectation was changed to require successful budget completion.
- Local Clippy initially selected a 32-bit Python. The CI-matching uv interpreter is `C:\Users\Q\AppData\Roaming\uv\python\cpython-3.13.5-windows-x86_64-none\python.exe`; setting `PYO3_PYTHON` to it makes workspace Clippy pass.
- GitHub issue 42 explicitly assigns deletion of the QNS workaround and the QNS integration test to the separately tracked `ctoth/qns` follow-up. This branch will not modify the QNS repository.
- The regression uses RLDR0=1, whose reload-derived steady period is 40 phi cycles, and calls `run(2 * period)` as the issue requires.
- Full local CI gates passed: formatting, workspace/all-target Clippy, state-feature Clippy, workspace tests, 1,000-case DAA conformance, state-feature tests, Python binding build, 52 Python tests, and the three-test reference differential gate.
- The Linux-only WASM build and Node smoke test are not locally applicable on Windows and must be verified by PR CI.
- The Python build temporarily rewrote `crates/z180-py/uv.lock`; that unrelated generated change was inspected and fully restored before staging.
- Kept slice committed as `3648e29` (`Advance time while sleeping`) with only `crates/z180-core/src/lib.rs` and `crates/z180-cli/src/run.rs`.
- Branch `agent/fix-slp-time` was pushed and ready PR `ctoth/z-core#43` was opened against `master`.
- PR #43 contained the exact closing phrase `Closes #42`.
- Both push-triggered and pull-request-triggered CI runs passed on Ubuntu and Windows. The Ubuntu jobs included the Linux-only WASM build and Node smoke test.
- PR #43 was merged at `2026-07-30T07:02:12Z` as `f144ef8bb7c91a648c5a0a96a79dfb6b286941f9`.
- Issue #42 closed at `2026-07-30T07:02:13Z`.
- `refs/heads/master` on `origin` was directly verified at `f144ef8bb7c91a648c5a0a96a79dfb6b286941f9`.

## Blocker

- None.

## Next action

- None. The requested issue resolution, TDD cycle, PR publication, green CI watch, merge, and issue closure are complete.
