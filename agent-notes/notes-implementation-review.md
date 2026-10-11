# Notes — implementation review session (2026-07-25)

## Task
Q asked: review the implementation, find issues, open GitHub issues as found. Tracker was empty at start.

## State
- Prior deep review: `reviews/2026-07-22/opus-4.8.md` (C1, H1, M1–M6, L1–L12). Recent 8 commits (ed4db2e..2455668) fixed C1, M1, M2, M4, M5 plus compat reentrancy/input pumping.
- Three analyst subagents dispatched, still running:
  1. `prior-findings` — status of every 07-22 finding in the current tree.
  2. `recent-commits` — deep review of the 8 new commits, runs test suites.
  3. `compat-review` — fresh adversarial review of `compat.py` + py/wasm binding surfaces vs `docs/qns-migration.md`.

## Issues filed (each verified by my own read of current source before filing)
- #2 — DMA SM/DM=2 (fixed-memory) misclassified as I/O in `service_dma` (`lib.rs:3322-3323`); stalls or rejects legal transfers. Prior H1, still present.
- #3 — PRT RLDR=0000h: reload branch (`lib.rs:3129-3137`) never sets TIF; interrupts stop after at most one. Prior M3, still present.
- #4 — DMA reads/writes raw physical addresses, bypassing `map_external_address` that CPU paths use (`lib.rs:2476/2486` vs `3162/3167`). Prior M6, still present.
- #5 — CI (`.github/workflows/ci.yml`) never runs `crates/z180-py/tests` or `crates/z180-wasm/tests/node-smoke.cjs`; only Rust tests + reference gate. New finding, confirmed by grep of `.github/` (only pytest is tools/reference).

## Other observations (not filed)
- `test_differential.py:240` compares TRAP memory events with `sorted(...)` — order-insensitive leniency, related to prior L1 (DDCB fetch order). Minor; may fold into an L1 issue if the agent confirms L1 still present.
- `bench.py` looks fine as a tool.

## What worked / didn't
- gh issue create with --body-file from scratchpad: worked (issues #2–#5).
- Reading current code before filing caught nothing wrong in the prior claims — all three carried-over findings reproduced exactly.

## Next step
- On agent completion: personally verify each reported finding against source (quote code), then file surviving findings as issues. Check especially: TRAP fix exactness, SST comparator gaps, ram_segment boundaries, run() abort semantics py vs wasm, compat.py reentrancy, remaining L-items (L1, L4, L6–L12).
- Then final summary to Q listing all filed issues.

## Agent results (prior-findings, received 07-26)
- FIXED: C1, M1, M2, M4, M5. STILL PRESENT: H1, M3, M6 (filed as #2-#4), L1-L12.
- My own re-verification of the L-items so far (direct reads):
  - L1 confirmed: lib.rs:529/:542 read sub-opcode at pc+3 before displacement at pc+2.
  - L2 confirmed: expected_daa (lib.rs ~6047) mirrors execute_daa exactly — circular test.
  - L5 confirmed: SavedState (lib.rs:266-310) has no ext_mapper field (asci_cts IS saved).
  - L6 confirmed: lib.rs:977 `if let Ok(payload)` with no error path.
  - L7 confirmed: load_state validation block checks events/watch/trace/memory, not interrupt_mode; assigned unchecked.
  - L11 confirmed: ioregs.rs FRC entry availability: BOTH.
  - L12 confirmed: start_asci_transmit gates on shift/tdr/ICR/TE only, never asci_cts.
- All L-items verified by direct read (L3 doc absence via grep; L4 borrows at py lib.rs:337/346/579/591; L8 wasm ram copy :596-601; L9 sst.rs:818/939; L10 zex.rs:76-84).
- Filed #6 (L1), #7 (L2), #8 (L3+L9), #9 (L4), #10 (L5+L6+L7), #11 (L8), #12 (L10), #13 (L11), #14 (L12).
- Issues #2-#14 now open. Awaiting recent-commits + compat-review agents (compat-review is actively probing — probe*.py files appearing in scratchpad).

## compat-review agent results (received 07-26)
- Report with 17 findings + test-gap list; probe scripts in scratchpad. I re-ran ALL probes myself — outputs reproduce: compat cycle_count drift (step credits 1 cycle vs real 6; run() clamps), deferred-IRQ resurrection, unguarded accessors raise from callbacks, rx pumped 2x per run() vs drain per instruction, big-run starves serial (probe_pump2: 1x run(200000) transmits nothing, 200x run(1000) transmits all), tx pop-discard without serial_tx, mem_peek/poke asymmetry, run(0)/run(-5) still pump, PanicException from ram() in callback, float/None → AttributeError '__and__', reentrant run() accepted with counter clobber.
- Finding 2 (failed mem_read substitutes 0xFF → instruction commits write to wrong address) NOT in current probe files; verified mechanism in source (py lib.rs:69-78 record_error + return unmapped_read; write_callback never checks latch, so write fires). Writing my own reproduction before filing.
- Plan: file grouped issues: cycle accounting (1+8), corrupted-execution-after-callback-error (2, both bindings + mutex-poison drop), deferred IRQ (3), pump asymmetry (5+14), wasm memPoke unchecked (7), compat reentrancy guards (9+15+10), GIL/signals (11), tx discard (12), return-type divergence (13), peek/poke asymmetry (16), FFI overhead (17). Add probe-evidence comments to #9 (ram panic) and #11 (wasm copy).
- recent-commits agent report received: corroborates compat findings (pump throttle+stranding from 9142185, deferred-IRQ persistence from d431073, incomplete reentrancy guards, ~2.2x no-callback snapshot overhead). New: SST spurious-write watch only in run_file (add_mem_watch appears once, sst.rs:537; run_mmu_file has none) and logical-vs-physical comparison; run() discards partial cycles on error; bindings duplicate core run loop. Clean: 12a3a6f TRAP fix (matches verification-log; caveat: corpus regenerated by same commit, independent anchor is UM0050 citation + hand-computed reference tests), 60d50c7 ram_segment, ae9cc19 submodule independence. Tests: 83 core+cli pass, 21 py tests pass, SST SUMMARY pass=7050 fail=0. cargo test --workspace fails locally only due to 32-bit python on PATH (env issue, not code).
- My verifications: reran ALL probe scripts (outputs reproduce); wrote+ran my own probe_wrongwrite.py (LD (0800h),A with failing operand read → host write at 0x8ff, then exception); confirmed wasm memPoke lacks return_callback_result vs py; confirmed callback_byte divergence; grep allow_threads=0; add_mem_watch count=1.
- Issue bodies written for 12 final issues: cycle-count (crit), callback-error corruption, deferred-irq, pump-throttle, wasm-mempoke, compat-reentrancy, gil, sst-mmu-watch, snapshot-overhead, tx-discard, callback-types, peek-poke. Next: file them + evidence comments on #9/#11, then final summary.

## DONE (2026-07-26)
- All 25 issues filed: #2-#5 (first batch), #6-#14 (prior-review L-items), #15-#26 (compat/binding/SST findings from agent reports, each re-verified by my own probe runs or reads). Evidence comments added to #9 and #11.
- Review complete. Everything published was verified by my own command output or direct source read; agent-only claims were labeled as such inside issue bodies (pre-d431073 timing comparison in #23, wasm-by-inspection in #19).

## Blockers
None.

## Issue-resolution campaign continuation (2026-07-26)

### User direction
- Work holistically: issues may share one root-cause fix or supersede one another.
- Prefer the actual coherent design over mechanically producing one patch per issue.
- Preserve TDD for valid implementation defects.

### Current repository and GitHub state
- `master` matches `origin/master` (`git rev-list --left-right --count` returned `0 0`).
- No tracked changes. Existing untracked notes remain untouched and must remain uncommitted.
- The only open PR is Dependabot PR #1.
- Open issues are #2 through #26.
- No issue branch has been created and no source or test file has been changed in this campaign.

### Holistic issue clusters observed
- Compat execution boundary: #15, #17, #18, #20, #23, #24 all arise from the orchestration in `crates/z180-py/python/z180/compat.py`.
- External-memory/debug API contract: #19 and #26 both depend on the intended `mem_peek`/`mem_poke` semantics.
- DMA addressing/classification: #2 and #4 initially appeared related, but current architecture evidence contradicts both review premises and requires manual verification for #2.
- Binding callback/runtime contract: #9, #16, #21, #25.
- Cross-binding RAM/API contract: #11 and parts of #25.
- SST authority: #8 and #22.
- Isolated or mostly isolated surfaces: #3, #5, #6, #7, #10, #12, #13, #14.

### Invalidity evidence found
- Issue #4's premise is wrong under the repository's authoritative boundary. `docs/verification-log.md` states that the external mapper applies only to CPU logical accesses after MMU translation; DMA and host physical APIs intentionally remain raw. Do not patch, close, or comment on #4 without explicit user instruction.
- Issue #2 is invalid. Direct inspection of UM0050 page images 99-100 shows mode 2 is fixed memory-mapped I/O (`Memory*`), DREQ-controlled; mode 2 paired with mode 2 or true I/O mode 3 is reserved. Current `service_dma` predicates implement those exact rules. The issue's proposed `!= 3` predicates would ungate memory-mapped-I/O DMA and enable reserved combinations. Do not patch, close, or comment on #2 without explicit user instruction.

### Current blocker / interrupted action
- None requiring user input.
- The first `pdftoppm` attempt used unsupported `-png` and created no images. The corrected PPM render succeeded; page images were inspected directly, not through text extraction.

### Exact next action
- Continue the validity/overlap map for the remaining issues, then choose the first coherent valid PR boundary. DMA issues #2/#4 are excluded.

### Campaign progress after DMA adjudication
- User explicitly authorized closing and commenting on invalid issues.
- Closed #2 as invalid with a comment citing UM0050 Tables 13-14 and DCNTL.
- Closed #4 as invalid with a comment citing the recorded CPU-only external-mapper boundary.
- Direct UM0050 Table 6 page-image review confirms FRC at 18h on the base Z80180. Issue #13's suspected S180-only code change is wrong; `BOTH` is correct. The issue remains actionable only because it explicitly asks for a durable verification-log citation when `BOTH` is confirmed.
- Direct ASCI page-image review confirms `/CTS` suppresses TDRE; it does not specify aborting a byte already in TSR. The current test loads TDR/TSR before raising `/CTS`, and `docs/verification-log.md` already records this exact boundary with citations.
- Closed #14 as already completed/invalid with an evidence comment.

### Issue #5 validation
- `.github/workflows/ci.yml` has no Python-binding or WASM-binding test step.
- The repository-documented Python commands are executable as written:
  - `uv run --with maturin maturin develop --release` succeeded.
  - `uv run pytest tests` passed all 21 tests.
- The repository-documented WASM commands are executable as written:
  - `wasm-pack build --target nodejs --scope zcore` succeeded.
  - `node tests/node-smoke.cjs` passed at about 202 million cycles/second against the 25 million threshold.
- This makes #5 a valid enabling issue and a strong first PR candidate before changing the binding/compat clusters.

### Current repository state
- No branch or tracked source/test/documentation change has been made yet.
- Build commands generated only expected ignored build/package artifacts.
- `notes-implementation-review.md` remains an uncommitted control record.

### Next action
- Verify Git status after the binding builds, then create a focused issue-#5 branch if tracked state remains clean.
- Add the two exact documented binding gates to CI, validate the workflow and local gates, then commit/push/open the PR with `Closes #5`.

### Issue #5 implementation state
- Branch: `fix/issue-5-binding-ci`.
- Only tracked change: `.github/workflows/ci.yml`.
- Added Python binding build/tests to both existing OS matrix entries.
- Added pinned `taiki-e/install-action@v2.85.1` installation of `wasm-pack@0.15.0` on Linux, followed by the documented Node.js WASM build and smoke.
- Did not add TypeScript, browser, benchmark, or binding behavior changes.

### Issue #5 verification
- `go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 .github/workflows/ci.yml` passed.
- `cargo fmt --check` passed.
- Initial bare workspace Clippy found the known local environment mismatch: 64-bit Rust versus a 32-bit default Python.
- Verified `crates/z180-py/.venv/Scripts/python.exe` exists; maturin identifies its build as `win_amd64`.
- With `PYO3_PYTHON` set to that exact crate-venv interpreter:
  - `cargo clippy --workspace --all-targets -- -D warnings` passed.
  - `cargo clippy -p z180-core --all-targets --features state -- -D warnings` passed.
  - `cargo test --workspace` passed (89 core tests, 26 CLI tests, 2 CLI integration tests, plus zero-test binding/doc targets).
- `cargo test -p z180-core --features state` passed all 89 tests.
- Python gate rerun passed: build succeeded and all 21 pytest tests passed.
- WASM gate rerun passed: build succeeded and Node smoke ran about 208 million cycles/second against the 25 million threshold.
- Reference differential gate passed all 3 tests.
- `git diff --check` passed.
- Diff review confirms exactly 19 workflow lines added and no other tracked file changed.

### Exact next action after checkpoint
- Stage only `.github/workflows/ci.yml`.
- Commit the kept issue-#5 slice.
- Read the publish-workflow skill before push/PR creation, then push, open a PR containing `Closes #5`, and watch CI to terminal green.

### Issue #5 published
- Commit: `30b9628` (`Run binding tests in CI`).
- Remote branch: `fix/issue-5-binding-ci`.
- Draft PR: #27, `Run Python and WASM binding tests in CI`.
- PR URL: https://github.com/ctoth/z-core/pull/27
- PR contains one commit, one changed file, 19 additions, and `Closes #5`.
- GitHub Actions terminal result:
  - push Ubuntu: passed in 2m34s
  - pull-request Ubuntu: passed in 2m11s
  - push Windows: passed in 3m49s
  - pull-request Windows: passed in 3m36s
- The PR remains draft and unmerged; do not assume its CI additions exist on `master`.

### Next campaign action
- Return to `master` without touching the untracked notes.
- Fetch/reverify current remote state.
- Select the next valid issue that is independent of unmerged PR #27; do not begin a binding/compat change that relies on #27 being merged.

### Post-PR #27 adjudication
- Returned to current `master` and fetched `origin`.
- Issue #3 is invalid. Direct UM0050 page-image review and Zilog's current Z80180 product specification both define TIF as setting when TMDR decrements to zero; neither defines RLDR=0000h as a one-tick periodic source.
- Closed #3 with an evidence comment. No timer code or test was changed.

### Issue #6 TDD state
- Branch: `fix/issue-6-indexed-fetch-order`.
- Current tracked change is test-only: `tools/reference/test_differential.py`.
- Replaced order-insensitive sorted event comparison with exact chronological comparison.
- Correct expected third-opcode sequence is prefix fetch, CB fetch, displacement read, sub-opcode fetch, effective-address read, then stack writes.
- The first red run showed the old sorted comparison was masking another real observable-order defect: `push_word` writes PCL at SP-2 before PCH at SP-1.
- Direct UM0050 Figures 32-33 confirm PC stacking writes PCH at SP-1 before PCL at SP-2; the third-opcode timing also places indexed displacement acquisition before the sub-opcode/effective read sequence.
- Updated the regression so all fetch/read events precede the high-then-low PC stack writes.
- Focused differential gate remains red: 2 passed, 1 failed; the first exact mismatch is current low-before-high TRAP stacking.

### Issue #6 coherent PR boundary
- Treat this as one observable memory-bus-order fix rather than forcing the test around the newly exposed stack defect.
- Fix stack push temporal order for PUSH/CALL/RST/interrupt/TRAP through the existing `push_word` owner.
- Fix DDCB/FDCB displacement/sub-opcode fetch order for both defined and trapped forms without adding a duplicate read.
- Keep the exact differential comparison as the regression authority.

### Exact next action
- Change `push_word` to issue high-byte/SP-1 before low-byte/SP-2 while preserving final memory and SP.
- Rerun the focused differential gate to expose the next expected mismatch before changing indexed fetch order.

### Issue #6 production changes in progress
- `push_word` now writes the high byte to SP-1 before the low byte to SP-2, then commits SP-2. Final stack memory and SP are unchanged; observable bus/watch order now matches UM0050.
- The first differential reruns appeared unchanged because `tools/reference` was loading its own cached path wheel, not the crate-local editable extension.
- `uv run --project tools/reference --refresh-package z180 ...` did not rebuild the transitive Rust workspace dependency.
- `uv run --project tools/reference --with maturin maturin develop ...` from the repository root failed before build because maturin searched the root for a nonexistent `pyproject.toml`.
- The reliable current-source rebuild command is:
  - `uv sync --project tools/reference --reinstall-package z180 --refresh-package z180`
- After that exact reinstall, the focused differential test advanced past stack order and failed at the intended indexed fetch mismatch: current read of PC+3 precedes expected read of PC+2.
- Added transient internal `indexed_displacement: Option<i8>` decode state to `Z180`.
- The slot is initialized and cleared on construction, reset, load, and each step.
- DD/FD CB decode now reads PC+2 once into that slot before reading PC+3.
- Both the defined indexed-bit handler and undefined third-opcode TRAP path consume the prefetched displacement, preventing duplicate reads.
- No public interface, alternate decoder, or handler ABI was introduced.

### Current tracked files
- `crates/z180-core/src/lib.rs`
- `tools/reference/test_differential.py`

### Exact next action after checkpoint
- Rebuild the `tools/reference` path wheel with the reliable `uv sync --reinstall-package` command.
- Run the focused differential gate; it must pass exact chronological event comparison.
- Then add/confirm direct Rust coverage for defined DDCB/FDCB fetch order before full gates.

### Issue #6 green implementation state
- Reinstalled the reference path wheel and the exact differential gate passed all 3 properties.
- Added direct core regression `indexed_bit_fetches_displacement_before_sub_opcode_without_duplicate_reads`; it covers both DD and FD prefixes and exact read addresses `[0, 1, 2, 3, effective]`.
- Added direct core regression `stack_push_writes_high_byte_before_low_byte`; it verifies PCH/SP-1 then PCL/SP-2 through the shared `push_word` owner.
- The first `cargo test --workspace` run found one stale expected read vector in `insn_trace_records_fetched_bytes_physical_pc_and_traps_without_extra_reads`; it encoded the old PC+3/PC+2 order.
- Updated only that expected vector to PC+2 then PC+3. Its named rerun passed.

### Issue #6 gates so far
- `cargo fmt --check` passed.
- Workspace Clippy with the 64-bit `PYO3_PYTHON` passed.
- State-feature Clippy passed.
- `cargo test --workspace` passed after the stale expectation update:
  - 91 core tests
  - 26 CLI unit tests
  - 2 CLI integration tests
  - binding/doc targets
- `cargo test -p z180-core --features state` passed all 91 tests.
- Python binding rebuilt from current source; all 21 Python tests passed.
- WASM Node target rebuilt from current source; smoke passed at about 192 million cycles/second.

### Exact next action after checkpoint
- Reinstall the current reference path wheel once more after the final Rust expectation edit and rerun the differential gate.
- Run `git diff --check`, inspect the complete two-file diff and worktree status.
- If scope is exact, stage only `crates/z180-core/src/lib.rs` and `tools/reference/test_differential.py`, then commit the kept slice.
## 2026-07-26 issue campaign checkpoint: PR #28

- Issue #6 was validated against the chronological memory-access oracle.
- Draft PR #28: https://github.com/ctoth/z-core/pull/28
- Commit: `f057f12` (`Preserve observable memory access order`)
- The shared correction preserves observable order at the owning surfaces:
  - stack pushes write the high byte at `SP-1` before the low byte at `SP-2`;
  - indexed CB decoding reads the displacement at `PC+2` before the sub-opcode at `PC+3`, without duplicate reads;
  - the differential oracle compares chronological events instead of sorting them.
- Local Rust, state-feature, Python-binding, WASM, and differential gates passed.
- GitHub Actions passed for push and pull-request events on Ubuntu and Windows.
- PR #28 remains draft and unmerged.
## 2026-07-26 issue campaign checkpoint: issues #22 and #8

- Issue #22 was closed as invalid.
  - `run_mmu_file` is not an instruction-case runner.
  - It programs the MMU, validates sixteen translation/read probes, reloads
    fixture state, and compares it; it does not execute a fixture target
    instruction whose extra writes could escape detection.
  - Adding the standard runner's write watch would therefore watch no target
    execution and would conflate instruction final-state RAM with the MMU probe
    contract.
- Issue #8 was validated and fixed with TDD.
  - Red tests proved R bit 7 was ignored and AF' used the fixed default flag
    mask instead of the case's mask.
  - The correction compares all eight R bits and applies the case flag mask to
    both AF and AF'.
  - README documents the intentional Z80180 XY-flag exclusion and the
    diagnostic-only `--ignore-r` option.
- Kept commit: `f34ac50` (`Tighten SST comparison contract`)
- Draft PR #29: https://github.com/ctoth/z-core/pull/29
- GitHub Actions passed for both push and pull-request events on Ubuntu and
  Windows.
- Local verification:
  - pinned SingleStepTests: 697,000 passed, 0 failed, 0 unimplemented;
  - first-party Z180 SST: 7,050 passed, 0 failed, 0 unimplemented;
  - formatting, workspace/state Clippy, workspace/state tests, and the
    differential gate all passed.
- Next action: inspect PR #29 CI to terminal status, then return to master and
  continue holistic issue triage.
## 2026-07-26 issue campaign checkpoint: issue #13

- Direct inspection of UM0050 Tables 6 and 7 proves FRC at internal I/O address
  18h exists on both the base Z80180 and enhanced Z8S180 maps.
- The runtime `availability: BOTH` setting is correct and was not changed.
- Updated the existing FRC verification-log row to record the variant fact and
  cite Tables 6 and 7 explicitly.
- Kept commit: `2374c56` (`Document FRC variant availability`)
- Draft PR #30: https://github.com/ctoth/z-core/pull/30
- GitHub Actions passed for both push and pull-request events on Ubuntu and
  Windows.
- Next action: wait for PR #30 CI to reach terminal status, then resume issue
  triage from master.
## 2026-07-26 issue campaign checkpoint: issue #12

- Issue #12 was closed as invalid.
- The ZEX harness installs no event-producing watches, traces, or interrupt
  source; in its current configuration an undefined-instruction step emits
  only the TRAP event.
- The reported failure requires a hypothetical future event source, so adding
  a helper or alternate scan now would be speculative rather than a current
  bug fix.
- No branch, tracked change, commit, or PR was created.
## 2026-07-26 issue campaign checkpoint: issue #7

- Issue #7 was validated: the exhaustive in-crate DAA test duplicated the
  production correction thresholds and flag derivation, so it was not an
  independent correctness oracle.
- The pinned `SingleStepTests/z80` submodule at commit `ebe1875` supplies an
  independently generated DAA corpus.
- The exact narrow gate
  `cargo run -p z180-cli --release -- sst --dir tests/sst/v1 --only 27`
  passes 1,000 cases with zero failures and is practical for CI.
- Deleted the 49-line circular unit test/helper and added the two-line
  independent CI gate. No replacement formula or adapter was introduced.
- Kept commit: `087c312` (`Use independent DAA conformance oracle`)
- Draft PR #31: https://github.com/ctoth/z-core/pull/31
- GitHub Actions passed for both push and pull-request events on Ubuntu and
  Windows, including the new independent DAA conformance step.
- Local formatting, workspace/state Clippy, workspace/state tests, and
  differential tests passed.
- Next action: wait for PR #31 CI to reach terminal status, including the new
  DAA step, then resume holistic triage from master.
## 2026-07-26 issue campaign checkpoint: issue #10

- Issue #10 was mixed:
  - the external mapper boundary is already explicitly documented in
    `docs/ARCHITECTURE.md` and `docs/timing-notes.md`;
  - no serialization error is producible by the current derived `SavedState`
    schema, so changing the public API or adding a panic would be speculative;
  - malformed `interrupt_mode` input was a real validation defect.
- TDD red proved a version-correct state with `interrupt_mode = 3` returned
  `Ok(())` and installed impossible state.
- Added `interrupt_mode <= 2` to the existing pre-mutation load-state invariant
  block and proved rejection is atomic.
- Kept commit: `de74ab8` (`Reject invalid saved interrupt modes`)
- Draft PR #32: https://github.com/ctoth/z-core/pull/32
- GitHub Actions passed for both push and pull-request events on Ubuntu and
  Windows.
- Local formatting, workspace/state Clippy, workspace/state tests, and
  differential tests passed.
- Next action: wait for PR #32 CI to reach terminal status, then resume
  holistic triage from master.
## 2026-07-26 issue campaign checkpoint: issue #11

- Issue #11 was validated as a cross-binding naming/contract bug.
- Python's writable zero-copy RAM view is intentional for qns and guards
  storage replacement while exported.
- WASM safely returns detached copies; exposing that as `ram()` made mutation
  silently ineffective and implied false parity.
- TDD red rebuilt the package and proved the new Node consumer contract failed
  because `ramCopy()` did not exist.
- Removed the ambiguous WASM `ram()` export, added `ramCopy()`, retained
  `loadRam()` as the exact-size write path, and documented/tested both
  semantics. No compatibility alias was retained.
- Kept commit: `a5dac07` (`Make WASM RAM copy semantics explicit`)
- Draft PR #33: https://github.com/ctoth/z-core/pull/33
- GitHub Actions passed for both push and pull-request events on Ubuntu and
  Windows. The binding-specific Node/TypeScript proof remains local until
  draft PR #27's CI expansion lands.
- Local WASM build, Node smoke, TypeScript, formatting, workspace/state
  Clippy, workspace/state tests, and differential tests passed.
- Next action: wait for PR #33 CI to reach terminal status, then resume
  holistic triage from master.
## 2026-07-26 issue campaign checkpoint: issues #19 and #26

- Read-only triage found one owning defect across both issues.
- `mem_peek` is debugger-side physical inspection and bypasses `HostBus`;
  External reads therefore return the configured unmapped/fill byte.
- `mem_poke` currently delegates to emulation `write`, so External writes call
  `HostBus::mem_write`. This violates the debugger-side boundary and creates
  issue #19's deferred WASM callback error.
- The coherent queued fix is at the core memory owner:
  - debugger poke mutates core-owned RAM only;
  - ROM, External, and Unmapped debugger pokes are no-ops;
  - no HostBus callback or emulation event is produced;
  - document the exact External read/write behavior;
  - cover the core contract and the WASM no-deferred-error behavior.
- This one root correction should resolve/supersede both #19 and #26; adding
  WASM-only callback-latch plumbing would preserve the wrong core contract.
- No branch or tracked change was created because draft PR #27 should land
  first and the user has not yet explicitly authorized merging it.
## 2026-07-26 issue campaign checkpoint: callback and compat adjudication

- Draft PR #27 remains the required CI foundation. All checks are green, but
  merging it and rebasing draft PRs #28-#33 remain blocked on explicit user
  authorization; the question "should you merge 27 and rebase?" established
  that the answer is yes but did not authorize the mutation.
- Read-only callback triage:
  - #9 is valid and narrow: PyO3 `borrow`/`borrow_mut` calls in `ram()`, the
    buffer exporter, and `set_ext_mapper()` can raise `PanicException` during
    reentrant callback access. The fix belongs at those borrow sites using
    fallible borrows and one consistent named runtime error.
  - #25 is valid but needs the exact shared Python/WASM integer contract fixed
    before implementation. Python currently applies `__and__(0xff)`, while
    WASM accepts only finite safe integer-valued numbers and masks them.
  - #16 is valid and architectural: `HostBus` reads return `u8`, so binding
    callback failures substitute `unmapped_read` and allow the active
    instruction to complete. Binding-only latch cleanup cannot undo observable
    writes already emitted by that instruction.
  - #21 is likely valid but requires a separately proved GIL/interruptibility
    design; it is not the same root as #16.
- The compat issues #15/#17/#18/#20/#23/#24 all touch the Python run loop, but
  their premises must be adjudicated against the Phase 8 compatibility
  authority before combining them.
- Durable/current evidence already weakens two review premises:
  - `PROGRESS.md` records the compat counter as a Python-owned qns-visible
    timeline that intentionally credits the requested budget and resets,
    rather than mirroring native lifetime cycles. The plan-named incumbent qns
    tests also require `run(100)` then `run(25)` to report exactly 100 then
    125. #15's proposed switch to real instruction cycles would break that
    contract, though its exception-path cleanup claim still needs separate
    adjudication.
  - The compat public surface has callback-based serial output and does not
    expose native `asci_tx_pop`. Therefore #24's claim that a compat caller can
    poll the queue is false; draining unobserved output may intentionally model
    a disconnected wire and prevent guest transmit backpressure. The original
    qns callback behavior remains the final authority to check.
- Current worktree is `master` aligned with `origin/master`; only the three
  user-owned `notes-*.md` files are untracked. No source branch or edit was
  created during this read-only adjudication.
- Next allowed action without merge authority: finish read-only validation of
  the incumbent qns run/cycle and serial callback behavior, then close/comment
  only issues whose current premise is proved invalid. Do not merge, rebase,
  create a source branch, or start a new source slice until explicitly
  authorized.

### Compat adjudication result

- Closed #24 as invalid. The incumbent qns wrapper always installs output
  thunks that discard when no user callback exists, and the compat surface has
  no public queue-pop API. Current unconditional draining preserves that
  callback contract.
- Closed #15 as invalid/superseded. The incumbent implementation and tests
  require requested-budget accounting, so the claimed actual-cycle correction
  would be a regression. Callback-failure accounting belongs to #16's
  instruction-abort decision and must not be fixed independently.
- #17 remains valid as a distinct IRQ-ordering issue: callback-deferred IRQ
  changes must be applied after the active step, and a later host write must
  not be overwritten by stale deferred state.
- #18 remains valid as a distinct serial-acceptance issue: a byte refused
  before firmware enables the receiver must be retried during the active run;
  once-per-run pumping makes behavior depend on chunk size.
- #20 remains valid with narrowed scope: preserve the specifically supported
  pre-instruction callback snapshots, provide explicit behavior for the
  remaining bus-callback methods, and reject recursive execution instead of
  letting a serial-output callback corrupt the outer run. It is not authority
  to make every mutator safely reentrant.
- #23 remains valid as a distinct no-callback fast-path issue. It should avoid
  the Python per-instruction snapshot/queue/trace loop only when none of those
  facilities is active, without changing compat requested-budget accounting.
- These four issues share a file but not one contract. Deliver them
  sequentially as separate TDD PRs rather than one run-loop mega-refactor.
- Fresh GitHub check: PR #27 remains draft, CLEAN, MERGEABLE at commit
  `30b9628`, with both Ubuntu and Windows checks green for push and PR runs.
  Explicit merge/rebase authorization is still the next required input.

## 2026-07-26 issue campaign checkpoint: CI foundation merged

- The user explicitly authorized merging PR #27 and rebasing PRs #28-#33.
- PR #27 was marked ready and merged with a merge commit, preserving its
  reviewed one-commit issue slice.
- Merge commit: `f6550740cdb4d4e556bcccb79c212ebcddcbe57c`.
- Issue #5 closed automatically as completed.
- Local `master` was fast-forwarded to `origin/master` at `f655074`; the only
  worktree additions remained the three untracked `notes-*.md` files.
- PR #28 branch `fix/issue-6-indexed-fetch-order` has been successfully rebased
  onto the merged `origin/master`. It has not yet been force-pushed.
- Next action: force-push PR #28 with lease, then rebase and force-push PRs
  #29-#33 in order. After all pushes, verify the expanded terminal CI on every
  PR and return to clean local `master`.

### Rebased PR heads

- #28 `fix/issue-6-indexed-fetch-order`: `f057f12` -> `d70a968`
- #29 `fix/issue-8-sst-comparison-contract`: `f34ac50` -> `4942480`
- #30 `docs/issue-13-frc-availability`: `2374c56` -> `7746be7`
- #31 `test/issue-7-independent-daa-oracle`: `087c312` -> `4c9c765`
- #32 `fix/issue-10-state-interrupt-mode`: `de74ab8` -> `191c766`
- #33 `fix/issue-11-wasm-ram-copy`: `a5dac07` -> `0c29bf5`
- Every branch rebased without conflicts and was force-pushed with lease.
- `git range-diff` reports exact patch identity for #28, #29, #30, #32, and
  #33. #31 reports only workflow context movement because PR #27 inserted the
  binding steps beside its DAA gate; direct old-base/new-base diffs have the
  same two-line DAA addition and the same 49-line circular-test deletion.
- No tracked local edits were introduced. Next action: verify each PR points to
  the listed head, wait for the expanded GitHub CI to reach terminal green,
  then switch back to `master` and record final state.

### Merge/rebase completion

- PR #27 is merged and issue #5 is closed.
- PRs #28-#33 each point to the expected rebased head listed above.
- Every PR has four terminal-success CI jobs from the post-#27 workflow:
  Ubuntu and Windows for both push and pull-request events.
- Every rebased PR reports `CLEAN` and `MERGEABLE`; all six remain drafts.
- The checkout is back on `master` at `f655074`, exactly aligned with
  `origin/master`.
- Final worktree state contains no tracked changes. The only untracked files
  are `notes-implementation-review.md`, `notes-plan-authoring.md`, and
  `notes-plan-execution.md`; they remain unstaged and uncommitted as required.
- Next issue-campaign action: start the single TDD core-memory slice that
  resolves #19 and #26 together, unless the user first directs which already
  green draft PRs should be made ready and merged.

## 2026-07-26 issue campaign checkpoint: issues #19 and #26 implementation

- Branch: `fix/issues-19-26-debug-memory`, created from merged `master`
  `f655074`.
- TDD red proofs:
  - focused core test failed because External `mem_poke` populated the
    recording bus's memory-write list;
  - rebuilt WASM Node smoke failed with `debuggerWrites == 1`, proving
    `memPoke` invoked `memWrite`.
- The implementation removes the bus parameter from `Memory::poke` and writes
  directly to a matched RAM store. ROM, External, Unmapped, and out-of-range
  addresses return without action.
- Public Rust documentation and `docs/ARCHITECTURE.md` now state the exact
  debugger contract: physical RAM/ROM reads, fill for External/Unmapped reads,
  RAM-only writes, and no HostBus or watch events.
- TDD green proofs:
  - focused core contract test passes;
  - rebuilt WASM Node smoke passes, including zero `memWrite` calls, fill-byte
    `memPeek`, and a successful following `step()` with no deferred error.
- Current intended tracked slice is exactly four files:
  `crates/z180-core/src/lib.rs`, `crates/z180-core/src/memory.rs`,
  `crates/z180-wasm/tests/node-smoke.cjs`, and `docs/ARCHITECTURE.md`.
- `git diff --check` passes. The only other worktree files are the three
  required untracked notes files.
- Next action: run formatting, Clippy, workspace/state tests, Python binding
  tests, rebuilt WASM smoke, and the differential gate; then commit the
  four-file slice if all gates pass.

### Issues #19 and #26 published

- Full local gates passed:
  - formatting;
  - workspace and state-feature Clippy under 64-bit `uv` Python 3.13;
  - workspace and state-feature tests;
  - Python binding build and 21 tests;
  - rebuilt WASM Node smoke;
  - 3-case Hypothesis differential gate.
- The first bare-shell workspace Clippy invocation stopped before project
  linting because the shell exposed a 32-bit Python to 64-bit Rust. The exact
  same gate passed unchanged inside the repository's 64-bit `uv` environment.
- Kept commit: `e50bca0` (`Keep debugger memory access inside core`).
- Draft PR #34: https://github.com/ctoth/z-core/pull/34
- The PR body closes both #19 and #26 because they are one core defect and its
  binding consequence.
- GitHub Actions is terminal green: Ubuntu and Windows both passed for push and
  pull-request events, including the new Python and WASM CI steps from merged
  PR #27.
- Branch worktree contains no tracked changes; only the three required
  untracked notes files remain.
- Next action: return to `master` and choose the next valid independent issue.

## 2026-07-26 issue campaign checkpoint: issue #9 implementation

- Branch: `fix/issue-9-pyo3-borrow-errors`, created from merged `master`
  `f655074`.
- Current issue validation confirms two public reentrant-callback failures:
  calling either `Machine.ram()` or `Machine.set_ext_mapper()` from a bus
  callback escapes as `pyo3_runtime.PanicException`.
- TDD red proof: the two-case focused regression test failed at the reported
  `slf.borrow(py)` and `slf.borrow_mut(py)` sites with `PyBorrowError` and
  `PyBorrowMutError`.
- The narrow implementation replaces all four reported panicking PyO3 borrow
  sites, including both internal `RamView::__getbuffer__` borrows, with
  `try_borrow` or `try_borrow_mut`. Every conflict maps directly to
  `RuntimeError("machine is busy in a callback")`; no helper or adapter was
  introduced.
- TDD green proof: the rebuilt binding passes both focused reentrant cases.
- Validation passed so far:
  - `cargo fmt --check`;
  - workspace and state-feature Clippy under 64-bit `uv` Python 3.13;
  - workspace tests under that same environment: 89 core tests, 26 CLI unit
    tests, and 2 CLI integration tests;
  - state-feature tests: 89 tests;
  - rebuilt Python binding and all 23 Python tests.
- The initial bare-shell workspace test invocation stopped before project
  tests because PATH exposed a 32-bit Python to 64-bit Rust. The exact
  `cargo test --workspace` gate passed unchanged inside the repository's
  64-bit `uv` environment.
- Current intended tracked slice is exactly
  `crates/z180-py/src/lib.rs` and `crates/z180-py/tests/test_api.py`.
  The three notes files remain untracked and must not be staged.
- Remaining local gates passed:
  - rebuilt Node.js WASM smoke at about 207 million cycles/second against the
    25 million threshold;
  - 3-case Hypothesis differential gate.
- No code blocker is known. Next action: inspect the final two-file diff,
  commit only those tracked files, push the branch, open a draft PR closing
  #9, and watch every CI job to terminal state.

### Issue #9 published

- Kept commit: `92e478e` (`Report reentrant Python borrows cleanly`).
- Draft PR #35: https://github.com/ctoth/z-core/pull/35
- The PR body closes #9.
- GitHub Actions is terminal green: Ubuntu and Windows both passed for push
  and pull-request events.
- PR #35 reports `CLEAN` and `MERGEABLE` and remains a draft.
- Branch worktree contains no tracked changes; only the three required
  untracked notes files remain.
- Next issue-campaign action: return to `master`, refresh the open issue/PR
  map, and select the next valid independent issue without overlapping an
  existing draft PR.

## 2026-07-26 issue campaign checkpoint: issue #17 implementation

- The refreshed open issue/PR map left seven unassigned issues: #16, #17,
  #18, #20, #21, #23, and #25. Current `compat.py` directly confirms #17's
  stale deferred-IRQ lifetime; it is independent of the other remaining
  callback, serial-pumping, and performance concerns.
- Branch: `fix/issue-17-deferred-irq`, created from merged `master`
  `f655074`.
- TDD red proof: a callback queued INT2 assertion during a zero-cycle native
  step, but `compat.run()` returned without applying it. The focused test saw
  no native IRQ change because the request remained in
  `_deferred_irq_states`.
- The narrow implementation removes next-iteration application and drains the
  captured deferred states in the existing `step()` cleanup, after the native
  mutable borrow is gone. A direct host `set_irq` also removes any queued
  value for that same line before applying the host's current state.
- TDD green proof: the public sequence now applies the callback assertion
  before `run()` returns, accepts a later direct clear, and a following run
  does not resurrect the stale assertion.
- Current intended tracked slice is exactly
  `crates/z180-py/python/z180/compat.py` and
  `crates/z180-py/tests/test_compat.py`. `git diff --check` passes.
- Validation passed so far:
  - focused callback-IRQ regression;
  - `cargo fmt --check`;
  - workspace and state-feature Clippy under 64-bit `uv` Python 3.13;
  - workspace tests: 89 core tests, 26 CLI unit tests, and 2 CLI integration
    tests;
  - state-feature tests: 89 tests;
  - rebuilt release Python binding.
- No blocker is known. Next action: run the full Python suite, rebuilt WASM
  smoke, and 3-case differential gate; then inspect, commit, and publish the
  two-file issue #17 slice if all remaining gates pass.

### Issue #17 published

- Remaining local gates passed:
  - all 22 Python tests;
  - rebuilt Node.js WASM smoke at about 198 million cycles/second against the
    25 million threshold;
  - 3-case Hypothesis differential gate.
- Kept commit: `4b587c4` (`Flush deferred compatibility IRQ changes`).
- Draft PR #36: https://github.com/ctoth/z-core/pull/36
- The PR body closes #17.
- GitHub Actions is terminal green: Ubuntu and Windows both passed for push
  and pull-request events.
- PR #36 reports `CLEAN` and `MERGEABLE` and remains a draft.
- Branch worktree contains no tracked changes; only the three required
  untracked notes files remain.
- Next issue-campaign action: return to `master` and adjudicate the next
  unassigned issue against current code, keeping the existing draft PRs
  independent.

## 2026-07-26 issue campaign checkpoint: issue #18 implementation

- Branch: `fix/issue-18-input-pump`, created from merged `master` `f655074`.
- Current code confirmed both issue claims:
  - `run(0)` called both host input callbacks despite executing no
    instruction;
  - one `run(200_000)` with two queued serial bytes transmitted neither byte.
- Initial TDD red proof: both focused regressions failed. The zero-cycle call
  polled serial channels 0 and 1 plus CSIO, and the large run left
  `transmitted` empty.
- Moving the existing `_pump_inputs()` call from run entry to the top of the
  existing instruction loop made the zero-cycle regression pass, but the
  real-machine large-run regression remained red.
- Live inspection showed why: the first host byte had left the host queue and
  the second sat in `_serial_pending`, while the first was stranded in the
  core receiver. Firmware enabled receive before changing CNTLB from its reset
  external-clock value; `asci_rx_push` accepted the byte with no clock source,
  and the later internal-clock selection never started it. Even two million
  additional cycles produced no receive or transmit. Shipping only the pump
  move would therefore relocate, not solve, the stranding.
- A new exact core regression proves that an accepted receive waiting on the
  external clock must begin when firmware selects an internal CNTLB divisor.
  The first attempted test command selected zero tests because it omitted the
  `tests::` module prefix and was discarded as evidence. The corrected exact
  command ran one test and failed with RDRF clear.
- The core now starts a waiting, unclocked ASCI receive when an ASCI CNTLB
  write supplies a valid internal frame timing. The corrected exact core test
  passes.
- Current intended tracked slice is three files:
  `crates/z180-core/src/lib.rs`,
  `crates/z180-py/python/z180/compat.py`, and
  `crates/z180-py/tests/test_compat.py`.
- No blocker is known. Next action: rebuild the Python binding, rerun both
  focused compatibility regressions, and continue to the full gate only if
  the single large run now transmits both queued bytes.

### Issue #18 published

- Rebuilt focused compatibility tests passed: `run(0)` did not poll, and one
  `run(200_000)` retransmitted both queued bytes.
- Full local gates passed:
  - formatting;
  - workspace and state-feature Clippy;
  - workspace and state-feature tests, both with 90 core tests;
  - rebuilt Python binding and all 21 Python tests;
  - rebuilt Node.js WASM smoke at about 191 million cycles/second;
  - 3-case Hypothesis differential gate.
- Kept commit: `9b86157` (`Pump compatibility input at instruction boundaries`).
- Draft PR #37: https://github.com/ctoth/z-core/pull/37
- The PR body closes #18 and records both the compatibility pump defect and
  the required core external-to-internal clock transition.
- GitHub Actions is terminal green: Ubuntu and Windows both passed for push
  and pull-request events.
- PR #37 reports `CLEAN` and `MERGEABLE` and remains a draft.
- Branch worktree contains no tracked changes; only the three required
  untracked notes files remain.
- Next issue-campaign action: return to `master` and adjudicate #20, #23, and
  their shared compatibility callback/snapshot boundary before deciding
  whether one root change supersedes part of either issue.

## 2026-07-26 issue campaign checkpoint: issues #20 and #23 adjudication

- Branch: `fix/issues-20-23-compat-callback-state`, created from merged
  `master` `f655074`.
- Both issues remain valid in current code and share one state-model defect:
  `compat.run()` captures a full pre-instruction callback snapshot
  unconditionally, while that snapshot is incomplete for actual callbacks and
  the run-active window ends before serial output callbacks execute.
- The selected root slice will:
  - skip pre-instruction snapshot work when no native memory or I/O callback
    can run;
  - extend the existing snapshot contract to the remaining read-only
    diagnostic accessors;
  - reject mutating compat operations during a native bus callback with a
    named compatibility error;
  - keep the outer run marked active through output draining so serial
    callbacks cannot re-enter and corrupt its accounting.
- This is one shared TDD PR intended to close both #20 and #23; it does not
  introduce callback wrappers, adapters, or a parallel execution path.
- Current tracked worktree is clean. The only files are the three required
  untracked notes files.
- Next action: add focused failing authorities for zero snapshot calls in the
  no-bus-callback path, remaining diagnostic access during a bus callback,
  named mutation rejection, and serial-output run reentry rejection before
  editing production code.

### Issues #20 and #23 TDD implementation evidence

- Five focused authorities failed on unchanged production code:
  - a no-bus-callback step performed eight register snapshot reads;
  - `asci_debug_state()` from a memory callback raised
    `RuntimeError("Already mutably borrowed")`;
  - callback `reset()` and `watch_pc()` raised opaque
    `RuntimeError("Already borrowed")`;
  - a serial-output callback re-entered `run()` without error.
- One direct compatibility state model now:
  - records whether native memory or I/O callbacks exist and only captures
    their pre-instruction snapshot when one can run;
  - snapshots ASCI diagnostic registers and PC-watch count for safe read-only
    callback access;
  - rejects callback `reset()` and `watch_pc()` before either local or native
    mutation;
  - keeps `_in_run` active through input, native steps, watch capture, and
    output draining so any callback run reentry gets a named error;
  - skips CBAR/watch-trace work when no PC watch is armed.
- All five focused authorities pass after the change.
- A temporary reusable five-sample benchmark calibrated each sample to at
  least 250 ms and measured the exact `Z180()` no-bus-callback path on the
  same branch and machine. The two tracked slice files were stashed only for
  the baseline and immediately restored:
  - unchanged `master`: 11,558,121 cycles/second median;
  - current branch: 35,646,851 cycles/second median;
  - measured improvement: about 3.08 times.
- The temporary benchmark script is diagnostic-only and must be removed
  before commit.
- Current intended durable slice is
  `crates/z180-py/python/z180/compat.py`,
  `crates/z180-py/tests/test_compat.py`, and the compatibility callback
  contract documentation still to be updated.
- No blocker is known. Next action: remove the temporary script, document the
  unified callback contract, inspect the exact diff, then run full gates.

### Issues #20 and #23 local validation complete

- `docs/qns-migration.md` now documents the exact compatibility boundary:
  instruction-entry snapshots for bus callbacks, deferred `set_irq`, named
  errors for callback mutators, non-reentrant run across every callback, and
  no snapshot construction when no native bus callback exists.
- The final implementation also avoids PC-watch counter reads when no watch
  is armed.
- Full local gates passed:
  - formatting;
  - workspace and state-feature Clippy;
  - workspace and state-feature tests, both with 89 core tests;
  - rebuilt release Python binding and all 26 Python tests;
  - rebuilt Node.js WASM smoke at about 203 million cycles/second;
  - 3-case Hypothesis differential gate.
- After the final pure-Python no-watch refinement, all 26 Python tests passed
  again.
- The final intended tracked slice is exactly
  `crates/z180-py/python/z180/compat.py`,
  `crates/z180-py/tests/test_compat.py`, and
  `docs/qns-migration.md`. The temporary benchmark script is removed and the
  three required notes files remain untracked.
- No blocker is known. Next action: perform the final diff/ledger check,
  commit the three-file slice, push it, open a draft PR closing #20 and #23,
  and watch all CI jobs to terminal state.

### Issues #20 and #23 published

- Kept commit: `ef23090` (`Unify compatibility callback state`).
- Draft PR #38: https://github.com/ctoth/z-core/pull/38
- The PR body closes both #20 and #23 because one compatibility callback-state
  model caused both the missing reentrancy contract and unconditional snapshot
  overhead.
- GitHub Actions is terminal green: Ubuntu and Windows both passed for push
  and pull-request events.
- PR #38 reports `CLEAN` and `MERGEABLE` and remains a draft.
- Branch worktree contains no tracked changes; only the three required
  untracked notes files remain.
- Next issue-campaign action: return to `master` and adjudicate the remaining
  unassigned issues #16, #21, and #25 against current code before selecting
  the next TDD slice.

## 2026-07-26 issue campaign checkpoint: issue #25 implementation

- Current code confirms all three remaining unassigned issues are valid.
  Issue #25 is the active independent slice; #16's instruction-abort boundary
  and #21's GIL/signal loop remain separate.
- Branch: `fix/issue-25-callback-values`, created from merged `master`
  `f655074`.
- The selected cross-binding contract is:
  - booleans are rejected;
  - values must be integers in JavaScript's exact safe range,
    `-(2^53 - 1)` through `2^53 - 1`;
  - valid integers are masked to their low byte;
  - invalid values raise an error naming the callback.
- TDD red proof:
  - Python accepted booleans and out-of-range integers, while floats, `None`,
    and strings leaked `AttributeError` from `__and__`;
  - Python `io_read` accepted a boolean;
  - compat serial/CSI input accepted booleans and out-of-range integers or
    leaked raw comparison/bitwise `TypeError` messages.
  - 17 invalid cases failed and 3 valid masking cases passed.
- The expanded WASM Node smoke already passed the same invalid/valid matrix,
  proving its existing callback validator is the contract anchor.
- Python `read_callback` now validates `PyInt` but excludes `PyBool`, enforces
  the safe range, names `memRead` or `ioRead`, and masks directly without
  invoking user `__and__`.
- Compat `serial_rx` and `csio_rx` now apply the same type/range validation
  before their existing negative-sentinel and low-byte logic.
- After rebuilding the binding, all 20 focused Python/compat cases pass.
- `docs/ARCHITECTURE.md` now records the shared binding and compatibility
  queue-input contract.
- Current intended tracked slice is exactly
  `crates/z180-py/src/lib.rs`,
  `crates/z180-py/python/z180/compat.py`,
  `crates/z180-py/tests/test_api.py`,
  `crates/z180-py/tests/test_compat.py`,
  `crates/z180-wasm/tests/node-smoke.cjs`, and
  `docs/ARCHITECTURE.md`.
- Full local gates passed:
  - `cargo fmt --check`;
  - workspace and state-feature Clippy;
  - workspace tests, including 89 core tests and 26 CLI tests;
  - state-feature tests, including 89 core tests;
  - rebuilt release Python binding and all 41 Python tests;
  - rebuilt Node.js WASM smoke at about 205 million cycles/second;
  - 3-case Hypothesis differential gate.
- No blocker is known. Next action: perform the final diff/ledger check, commit
  only the six-file slice, push it, open a draft PR closing #25, and watch all
  CI jobs to terminal state.

### Issue #25 published

- Kept commit: `032a614` (`Converge callback integer validation`).
- Draft PR #39: https://github.com/ctoth/z-core/pull/39
- GitHub Actions is terminal green: Ubuntu and Windows both passed for push
  and pull-request events.
- PR #39 reports `CLEAN` and `MERGEABLE` and remains a draft.
- Branch worktree contains no tracked changes; only the three required
  untracked notes files remain.
- Next issue-campaign action: return to `master` and begin the separate #16
  TDD slice for aborting an instruction after a host callback error.

## 2026-07-26 issue campaign checkpoint: issue #16 TDD red

- Branch: `fix/issue-16-abort-callback-error`, created from merged `master`
  `f655074`.
- Current code confirms issue #16 is valid: the core `HostBus` methods cannot
  return failure, so Python and WASM record callback errors out of band,
  substitute `unmapped_read`, and surface the error only after `step()`.
- New Python and Node/WASM regressions require a failed callback access to
  leave the instruction unconsumed: no later host write, PC at the instruction
  start, and no cycles charged.
- TDD red proof:
  - Python's failed low operand read still called `mem_write(0x08ff, 0x5a)`;
  - Python and WASM still advanced a failed opcode-fetch step to PC `0x0038`
    and charged 20 cycles.
- A binding-only write buffer or documentation would leave fabricated
  execution in place. The active implementation direction is the core owner:
  make host bus accesses fallible and propagate the original binding error
  through instruction execution so control returns at the failed access.
- The core must preserve the existing infallible Rust ergonomics where
  possible, while Python and WASM use the fallible step/run path.
- No blocker is known. Next action: implement the fallible core bus boundary,
  directly return `PyErr`/`JsValue` from the binding buses, and drive the
  compiler and focused regressions to green before full gates.

### Issue #16 implementation in progress

- `HostBus` now has an associated error and returns `Result` from every host
  memory and I/O access.
- `Z180::try_step` and `try_run` expose the fallible execution path, while
  `step` and `run` remain available for buses whose error is `Infallible`.
- The core records the first failed access during an instruction, suppresses
  subsequent memory/I/O reads and writes, restores the pre-instruction
  registers and EI/watch bookkeeping, abandons partial instruction tracing,
  and returns the original bus error without charging instruction cycles.
- Memory and DMA access paths now participate in the same failure boundary;
  DMA stops its transfer loop on the first bus error.
- The first workspace compile intentionally failed because existing bus
  implementations and binding adapters still use the old infallible method
  signatures. This is expected implementation work, not a blocker.
- Next action: adapt every in-repo `HostBus` implementation, replace Python
  and WASM's out-of-band callback-error stores with direct errors, then rerun
  workspace compilation and the focused red regressions.

### Issue #16 focused implementation green

- Every in-repo `HostBus` implementation now uses the explicit `Result`
  contract. Infallible core/CLI buses retain plain `step` and `run`.
- Python and WASM removed their callback-error mutex/refcell channels and
  duplicated run loops. Their buses return `PyErr` or `JsValue` directly and
  their public execution methods delegate to core `try_step`/`try_run`.
- Workspace `cargo check` passes.
- Rebuilt Python focused regressions pass: 3 tests, including the exact
  corrupt-write reproduction.
- Rebuilt Node/WASM smoke passes, including the same abort boundary, at about
  180 million cycles/second.
- A core-level fallible-bus regression now pins the owner contract directly:
  the operand read error must prevent the later external write and leave PC
  and cycle count at the instruction boundary.
- No blocker is known. Next action: run the new core regression, document the
  public fallible bus contract, inspect Clippy/API fallout, then run all
  required full gates.

### Issue #16 local validation complete

- The public contract is documented in `crates/z180-core/README.md`,
  `docs/ARCHITECTURE.md`, and `docs/timing-notes.md`.
- A second Python regression proves an external `io_read` failure during
  `INI` cannot reach the later external memory write and restores PC, BC, HL,
  and cycle count.
- Full local gates passed:
  - formatting;
  - workspace and state-feature Clippy;
  - workspace tests, including 90 core tests and 26 CLI tests;
  - state-feature tests, including 90 core tests;
  - rebuilt release Python binding and all 23 Python tests;
  - rebuilt Node.js WASM smoke at about 174 million cycles/second;
  - 3-case Hypothesis differential gate.
- No blocker is known. Next action: inspect the exact tracked diff and ledger,
  resolve any audit finding, then commit the single #16 slice, push it, open a
  draft PR closing #16, and watch all CI jobs to terminal state.

### Issue #16 final audit

- Diff review found and removed an unnecessary API regression: generic buses
  now use `try_mem_poke`, while `Infallible` buses retain the original
  `mem_poke` surface just as they retain `step` and `run`.
- After that correction, every full local gate passed again:
  - formatting;
  - workspace and state-feature Clippy;
  - workspace and state-feature tests, both with 90 core tests;
  - rebuilt release Python binding and all 23 Python tests;
  - rebuilt Node.js WASM smoke at about 153 million cycles/second;
  - 3-case Hypothesis differential gate.
- Next action: perform the final diff/ledger check, stage only the fourteen
  intended issue files, commit the slice, push it, open a draft PR closing
  #16, and watch all CI jobs to terminal state.

### Issue #16 published

- Kept commit: `d2bf29e` (`Abort execution on host bus errors`).
- Draft PR #40: https://github.com/ctoth/z-core/pull/40
- GitHub Actions is terminal green: Ubuntu and Windows both passed for push
  and pull-request events.
- PR #40 reports `CLEAN` and `MERGEABLE` and remains a draft.
- Branch worktree contains no tracked changes; only the three required
  untracked notes files remain.
- Next issue-campaign action: return to `master` and begin the separate #21
  TDD slice for GIL release and signal responsiveness.

## 2026-07-26 issue campaign checkpoint: issue #21 implementation

- Branch: `fix/issue-21-python-run-gil`, created from merged `master`
  `f655074`.
- Current code confirms issue #21 is valid: Python `Machine.run()` contains no
  `detach` or signal check and holds the GIL for its whole step loop.
- TDD red proof: while a worker ran 50 million internal-RAM cycles, the main
  thread could not reacquire before the worker finished.
- The implementation records whether any host callback was supplied. It runs
  callback-free machines in one-million-cycle detached chunks and checks
  Python signals after every chunk. Callback machines stay attached, preserve
  per-step callback error handling, and check signals at the same cycle
  boundary.
- The strengthened 100-million-cycle concurrency regression passes, and the
  complete 22-test Python suite passes.
- A temporary reusable five-sample benchmark measures the exact internal-RAM
  `JR -2` run path. Current implementation median: about 508 million
  cycles/second.
- Exactly the two tracked slice files are stashed as
  `issue-21 benchmark baseline`; the untracked benchmark script and required
  notes remain in place.
- No blocker is known. Next action: rebuild the unchanged baseline, measure it
  with the same script, immediately restore the stash, and use the comparison
  to decide whether the one-million-cycle boundary is acceptable.

### Issue #21 benchmark and contract

- Same-machine five-sample medians with the exact reusable benchmark:
  - unchanged `master`: about 466 million cycles/second;
  - implementation after immediate stash restore: about 498 million
    cycles/second (first implementation pass: about 508 million).
- The one-million-cycle detach/signal boundary therefore produced no measured
  throughput regression and remains the kept design.
- The exact two-file stash was restored and dropped. The temporary benchmark
  script was removed.
- `crates/z180-py/README.md` now documents callback-free GIL release,
  periodic signal processing, and why callback machines remain attached.
- Current intended tracked slice is exactly
  `crates/z180-py/src/lib.rs`,
  `crates/z180-py/tests/test_api.py`, and
  `crates/z180-py/README.md`.
- No blocker is known. Next action: run formatting, full Clippy/Rust gates,
  rebuild the Python binding, run all Python tests, rebuild/test WASM, and run
  the differential gate before the final diff/ledger audit.

### Issue #21 local validation complete

- Full local gates passed:
  - formatting;
  - workspace and state-feature Clippy;
  - workspace tests, including 89 core tests and 26 CLI tests;
  - state-feature tests, including 89 core tests;
  - rebuilt release Python binding and all 22 Python tests;
  - rebuilt Node.js WASM smoke at about 209 million cycles/second;
  - 3-case Hypothesis differential gate.
- The regression proves another Python thread runs before a
  100-million-cycle callback-free native run completes. The implementation's
  explicit `py.check_signals()` boundary provides Ctrl-C/custom-handler
  processing after each one-million-cycle chunk in both callback modes.
- No blocker is known. Next action: perform the final three-file diff and
  ledger check, commit the slice, push it, open a draft PR closing #21, and
  watch all CI jobs to terminal state.

### Issue #21 published

- Kept commit: `c1b9dfa` (`Keep Python runs responsive`).
- Draft PR #41: https://github.com/ctoth/z-core/pull/41
- GitHub Actions is terminal green: Ubuntu and Windows both passed for push
  and pull-request events.
- PR #41 reports `CLEAN` and `MERGEABLE` and remains a draft.
- Branch worktree contains no tracked changes; only the three required
  untracked notes files remain.
- Next issue-campaign action: perform the final live issue/PR audit and verify
  that every remaining open issue is owned by one green draft PR.

## 2026-07-26 issue campaign final audit

- Merged PR #27 closed valid issue #5.
- Eight issues were closed after current-code adjudication showed them invalid
  or already fixed: #2, #3, #4, #12, #14, #15, #22, and #24.
- Every remaining open issue maps exactly once to a draft issue PR:
  - #6 -> #28
  - #8 -> #29
  - #13 -> #30
  - #7 -> #31
  - #10 -> #32
  - #11 -> #33
  - #19 and #26 -> #34
  - #9 -> #35
  - #17 -> #36
  - #18 -> #37
  - #20 and #23 -> #38
  - #25 -> #39
  - #16 -> #40
  - #21 -> #41
- Live GitHub audit reports draft PRs #28 through #41 all `CLEAN`,
  `MERGEABLE`, and with zero incomplete or non-success checks.
- The only other open PR is the independent non-draft Dependabot PR #1; it
  is outside this issue campaign.
- No open issue is unowned, duplicated across PRs, or left without terminal
  green CI. The issue campaign is complete at the requested draft-PR boundary.
