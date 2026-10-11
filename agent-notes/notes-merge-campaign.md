# Merge campaign checkpoint

## Current state

- User authorized merging all issue-campaign pull requests when green.
- PR #28 was verified at head `d70a9684276303840f7d48d28591430123020e76` as ready to merge: all four Ubuntu/Windows CI checks passed, GitHub reported `CLEAN` and `MERGEABLE`, and the PR was marked ready.
- PR #28 was merged using the repository's established merge-commit method.
- The remaining PR head names were retrieved directly from GitHub after an initial inferred branch name failed before changing any branch.
- The following remaining branches were successfully rebased onto the post-#28 `origin/master` and force-pushed with lease:
  - PR #29: `fix/issue-8-sst-comparison-contract`
  - PR #30: `docs/issue-13-frc-availability`
  - PR #31: `test/issue-7-independent-daa-oracle`
  - PR #32: `fix/issue-10-state-interrupt-mode`
  - PR #33: `fix/issue-11-wasm-ram-copy`
  - PR #34: `fix/issues-19-26-debug-memory`

## Observations

- The established method from merged PR #27 is a two-parent merge commit.
- The first attempted rebase loop stopped before mutation because the inferred PR #29 branch name did not exist; the exact GitHub head names were then used.
- A repository checkpoint interrupted the second rebase loop after PR #34 was reported successfully rebased and pushed.

## Blocker

- No known code or CI blocker yet.
- The interrupted loop may have stopped while beginning PR #35, so the worktree/rebase state must be inspected before resuming.

## Next action

- Inspect Git/worktree state and exact remote heads to determine whether PR #35 was touched.
- Resume rebasing at the first untouched branch.
- Wait for fresh CI, then merge PRs sequentially only when their current heads are green and mergeable.

This file is an uncommitted operational record and must not be staged, committed, or pushed.

## Checkpoint after PR #40 rebase conflict

- PR #35 was confirmed to have completed its rebase and force-push before the earlier loop was interrupted.
- PRs #36, #37, #38, and #39 were successfully rebased onto post-#28 `origin/master` and force-pushed with lease.
- PR #40 conflicted in `crates/z180-core/src/lib.rs` between:
  - #28 resetting `indexed_displacement` at every instruction entry.
  - #40 replacing the fallible entry point with `try_step` and checking the bus-error invariant.
- The two behaviors are independent and both required. The resolution keeps `try_step`, checks that no prior bus error is pending, then resets `indexed_displacement` before instruction execution.
- The rebase completed as commit `ad05d17`.
- `cargo fmt --check` passed.
- The first workspace Clippy attempt did not compile code because PyO3 selected a 32-bit Python for a 64-bit Rust target.
- `uv python find 3.13` identified the 64-bit interpreter at `C:\Users\Q\AppData\Roaming\uv\python\cpython-3.13.5-windows-x86_64-none\python.exe`.
- With `PYO3_PYTHON` pinned to that interpreter:
  - workspace Clippy passed;
  - state-feature Clippy passed.

Current blocker: no code blocker. The checkpoint interrupted validation before the two Rust test commands ran.

Next action: run workspace and state-feature tests for the #40 resolution, then push #40 if both pass; rebase #41 afterward.

## Checkpoint after merging PR #29

- The #40 resolution passed workspace tests and state-feature tests.
- PR #40 was force-pushed with lease at rebased head `ad05d17`.
- PR #41 was rebased onto post-#28 `origin/master` and force-pushed at `32dd4a9`.
- PR #29 was verified at rebased head `8811bb2bc32a3fa63ebbc357a3d5c45835917db3`:
  - GitHub reported `CLEAN` and `MERGEABLE`;
  - both Ubuntu CI runs passed;
  - both Windows CI runs passed.
- PR #29 was marked ready and merged with the established merge-commit method.
- The post-#29 rebase loop has completed and pushed PRs #30 through #35.

Current blocker: no known code or CI blocker. The checkpoint interrupted the rebase loop after #35; the worktree must be checked for a partially started #36 operation.

Next action: inspect the worktree and exact #36 remote head, then resume the post-#29 rebase at the first untouched branch. Wait for fresh #30 CI before its merge.

## Checkpoint while waiting for PR #30

- The interrupted post-#29 rebase loop had fully rebased and pushed PR #36.
- PRs #37 through #41 were then rebased onto post-#29 `origin/master` and force-pushed with lease.
- PR #30 current head is `071b4b15f82327aa8db480ebc6f169eb2e387700`.
- GitHub reports #30 as mergeable, but its two Ubuntu and two Windows jobs remain queued, so it is not yet green.
- Runner inspection showed a CI storm from rebasing every remaining PR at once. Several current campaign heads are running, while #30 is waiting for capacity.
- To retain the exact green-current-base guarantee without repeatedly invalidating every later head, subsequent merges will use just-in-time rebasing: after each merge, rebase only the next numbered PR onto current `master`, wait for its fresh CI, then merge it.
- Five pre-#29 CI runs for superseded #39-#41 heads were identified. Four were cancelled:
  - `30220938500`
  - `30220937282`
  - `30220933520`
  - `30220849635`
- Cancellation of `30220848025` was rejected because that run completed before the request reached it; no current-head run was cancelled.

Current blocker: #30's current-head CI is queued, not failed.

Next action: continue watching #30's four required jobs. If all pass and GitHub still reports clean/mergeable, mark #30 ready and merge it. Then rebase only #31 onto the new master.

## Checkpoint after merging PR #31

- To free the only decision-relevant gate, 15 active CI runs for PRs #31-#41 were cancelled before #30 completed. Those later heads were guaranteed to be rebased again before merge; #30 and `master` runs were explicitly excluded.
- PR #30 at `071b4b15f82327aa8db480ebc6f169eb2e387700` passed both Ubuntu and both Windows checks, remained `CLEAN` and `MERGEABLE`, was marked ready, and was merged with a merge commit.
- PR #31 was rebased alone onto post-#30 `origin/master` and force-pushed at `805be16394722bfcebf1c15dff286778e832a56f`.
- PR #31 then passed both Ubuntu and both Windows checks, remained `CLEAN` and `MERGEABLE`, was marked ready, and was merged with a merge commit.
- The just-in-time rebase strategy is working: only the next PR's CI runs, preventing later heads from consuming runner capacity before their final base exists.

Current blocker: none known.

Next action: fetch post-#31 `master`, rebase PR #32 (`fix/issue-10-state-interrupt-mode`) onto it, force-push with lease, and require fresh green CI before merge.

## Checkpoint during PR #37 conflict resolution

- PR #32 at `96c506413f38dbcfaf97ab1e6625a0431536b25f` passed all four CI checks, remained clean/mergeable, and was merged.
- PR #33 at `87bf91f262c9ae6a9a4bafc99028f767668ea7a8` passed all four CI checks, remained clean/mergeable, and was merged.
- PR #34 at `ac4edd747a2c8e263a55847852f5871785ed3aec` passed all four CI checks, remained clean/mergeable, and was merged. Its merge closes issues #19 and #26.
- PR #35 at `2dfe8512a6bc5b6b75ef889e74b5387bc2e57c2d` passed all four CI checks, remained clean/mergeable, and was merged.
- PR #36 at `b450eb5b41187c2398dc6b5e022ac5588f64db23` passed all four CI checks, remained clean/mergeable, and was merged.
- PR #37 conflicted only in `crates/z180-py/python/z180/compat.py`:
  - #36 moved deferred IRQ flushing into the native-step `finally` block so callback IRQ changes apply before `run` returns.
  - #37 moved `_pump_inputs()` from before the loop to each instruction boundary.
- The combined resolution retains #36's `finally` flush, removes #37's obsolete pre-step deferred-IRQ loop, and calls `_pump_inputs()` at the top of every execution-loop iteration.
- The resolved #37 rebase completed as commit `9006991`.

Current blocker: no known semantic blocker. The resolved #37 branch has not yet been locally validated or pushed.

Next action: run formatting, Rust tests, and the relevant Python compatibility tests for #37. If they pass, force-push with lease and require fresh GitHub CI before merge.

## Checkpoint during PR #38 conflict resolution

- PR #37's combined resolution passed:
  - formatting;
  - workspace Rust tests;
  - state-feature Rust tests;
  - release Python binding build;
  - the Python compatibility test file.
- PR #37 at `9006991ec3e8ebefef884c45d934a75fbd44d25c` then passed all four GitHub CI checks, remained clean/mergeable, and was merged.
- PR #38 conflicted in `compat.py` and `test_compat.py` against the merged #36/#37 behavior.
- The combined `run` state model now retains:
  - #38's outer `_in_run` reentrancy guard;
  - #38's conditional callback snapshots and PC-watch capture;
  - #37's `_pump_inputs()` call at every instruction boundary;
  - #36's post-step deferred IRQ flush in the `finally` block;
  - output draining inside the protected outer run.
- Both #36's deferred-IRQ regression and #38's callback-state regressions were retained.
- The resolved #38 rebase completed as commit `926a388`.

Current blocker: no known semantic blocker. The resolved #38 branch has not yet been locally validated or pushed.

Next action: run formatting, workspace/state Rust tests, release Python binding build, and compatibility tests. If green, force-push #38 with lease and require fresh GitHub CI before merge.

## Checkpoint during PR #39 conflict resolution

- PR #38's combined resolution passed formatting, both Rust test gates, release Python binding build, and compatibility tests locally.
- PR #38 at `926a388cc5525726c838739d11ef4044e0654f5f` then passed all four GitHub CI checks, remained clean/mergeable, and was merged.
- PR #39's production files merged automatically onto post-#38 master.
- Conflicts were limited to adjacent/additive tests:
  - `test_compat.py`: retained #37's renamed one-run ASCI test and inserted #39's invalid serial/CSIO callback-value matrix before it.
  - `node-smoke.cjs`: retained #34's debugger-poke regression and added #39's invalid/valid callback integer matrices.
- The resolved #39 rebase completed as commit `9dace2c`.

Current blocker: no known semantic blocker. The resolved #39 branch has not yet been locally validated or pushed.

Next action: run formatting, Rust tests, the full Python binding/tests, and Node/WASM build/smoke. If green, force-push #39 with lease and require fresh GitHub CI before merge.

## Checkpoint during PR #40 convergence

- PR #39's resolved branch passed formatting, both Rust test gates, full Python binding/tests, and Node/WASM build/smoke locally.
- PR #39 at `9dace2ca356cbf8a204488b7a50bcca491d48dc9` then passed all four GitHub CI checks, remained clean/mergeable, and was merged.
- PR #40 conflicts with merged #34/#39 in five files, as expected.
- Controlling combined semantics:
  - #34's debugger `mem_poke` is universally infallible, writes core-owned RAM only, and never calls `HostBus`.
  - Therefore #40's old `try_mem_poke` split is superseded and must be deleted everywhere.
  - #40's `try_step`/`try_run` remain the fallible boundary for emulated bus access.
  - #39's named safe-integer callback validation remains the value contract.
  - #40's Python/WASM buses return native `PyErr`/`JsValue` directly rather than latching out-of-band errors.
- Resolutions already applied:
  - core `mem_poke` and `Memory::poke` retain #34 ownership;
  - the infallible core bus implementation records emulated memory writes using #40's `Result` signature;
  - Python direct bus propagation calls #39's named validator;
  - `PyRuntimeError` remains for #35's reentrant-borrow error;
  - Python and Node/WASM test conflicts retain debugger, validation, and abort-on-error regressions together.

Current blocker: no known semantic blocker, but the #40 rebase is not complete. Stale `try_mem_poke` references remain in README and binding methods, and all conflict markers/references must be audited before `rebase --continue`.

Next action: remove every remaining `try_mem_poke` reference, restore Python/WASM debugger `mem_poke` to direct core calls, audit for conflict markers/out-of-band error remnants, continue the rebase, then run full Rust/Python/WASM validation.

## Checkpoint after local PR #40 validation

- All remaining `try_mem_poke`, out-of-band callback-error, and conflict-marker remnants were removed.
- Python and WASM debugger `mem_poke` now call the universally infallible core-owned `mem_poke` directly.
- Core README now documents that only emulated `step`/`run` split into fallible and infallible methods; debugger `mem_poke` never calls the bus.
- The converged #40 rebase completed and was formatted.
- Full local gates passed:
  - `cargo fmt --check`;
  - workspace Clippy with warnings denied;
  - state-feature Clippy with warnings denied;
  - workspace tests;
  - state-feature tests;
  - release Python binding build;
  - full Python tests;
  - Node/WASM build;
  - Node/WASM smoke;
  - Hypothesis differential gate.
- The final amended #40 commit is `cea52f66c42d15d169a144c79d4ab6606ecd8bfd`.

Current blocker: none known. The validated #40 branch has not yet been pushed or run through fresh GitHub CI.

Next action: force-push #40 with lease, require all four fresh CI checks and clean/mergeable status, then merge. Afterward rebase #41 onto that final base and resolve any `try_run`/GIL overlap.

## Checkpoint after local PR #41 validation

- PR #40 at `cea52f66c42d15d169a144c79d4ab6606ecd8bfd` passed all four GitHub CI checks, remained clean/mergeable, and was merged.
- PR #41 conflicted only in Python binding run logic and adjacent tests.
- The final combined run contract:
  - uses one-million-cycle responsiveness chunks;
  - calls core `try_run` for every chunk;
  - detaches from the GIL only for callback-free machines;
  - keeps callback-bearing machines attached;
  - propagates #40's native `PyErr` directly;
  - checks Python signals after each successful chunk;
  - does not restore `callback_error`, `Mutex`, or old result-latch plumbing.
- The accumulated validation/error tests and #41's GIL-release regression were retained.
- The resolved #41 rebase completed as `c83c651`.
- Full local gates passed:
  - formatting;
  - both Clippy modes;
  - workspace and state-feature tests;
  - release Python binding build and full Python tests;
  - Node/WASM build and smoke;
  - Hypothesis differential gate.

Current blocker: none known. The validated #41 branch has not yet been pushed or run through fresh GitHub CI.

Next action: force-push #41 with lease, require all four fresh CI checks and clean/mergeable status, then merge. Finally verify `master`, campaign issue closure, remaining open PRs/issues, and the uncommitted-notes-only worktree.

## Final campaign state

- PR #41 at `c83c65134918e6168f4f30f6db8df9d9ddaa8aff` passed all four GitHub CI checks, remained clean/mergeable, and was merged.
- PRs #28 through #41 are all confirmed `MERGED`.
- Final `master` merge commit is `0d087fede26db26249b1c93ea7fef05380d9e5fc`.
- Final landed-code CI run `30223668703` passed on Ubuntu and Windows.
- Ubuntu passed formatting, both Clippy modes, workspace/state tests, independent DAA, Python build/tests, Node/WASM build/smoke, and the reference differential gate.
- Windows passed formatting, both Clippy modes, workspace/state tests, independent DAA, Python build/tests, and the reference differential gate; Node/WASM steps are intentionally skipped by the workflow on Windows.
- GitHub reports no open issues.
- The only remaining open pull request is pre-existing Dependabot PR #1, which is outside the issue campaign and was not merged.
- Local `master` was fast-forwarded to the final remote head.
- The worktree has no tracked changes; only the four required uncommitted `notes-*.md` operational records remain.

Current blocker: none.

Next action: none for this campaign. Keep all notes files uncommitted.
