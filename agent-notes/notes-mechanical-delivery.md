Checkpoint 2026-10-01: PR #45 merged normally at faf16b9 after all exact-head Linux/Windows checks passed.
Follow-up isolated worktree z-core-memory now uses branch mechanical-sympathy from that merge.
Compact execution/disassembly views and one-pass formatter pass core tests, v4 compatibility, and CLI golden output.
New descriptor-size and formatter-allocation regressions failed before edits and now pass (NOP 14 calls to 1).
Next: reusable save buffers, allocation-free drains, replay borrowing; no blocker.

Checkpoint: reusable Vec and fixed-slice save APIs now emit unchanged v4 bytes, with explicit short-buffer errors.
Allocation tests confirm both save paths and iterator drains allocate zero buffers when storage is prepared.
Partial event/trace drains discard the tail, preserve ring storage, and keep the sticky loss flag.
Python and WASM now convert directly from drain iterators; all 100 core tests and historical state fixtures pass.
Next: red allocation regressions for replay export/seek, then borrowing changes and final cross-consumer validation.

Checkpoint: all three replay allocation regressions failed before edits and now pass, with replay behavior tests green.
64 KiB seek peak falls 137,779 to 71,768 bytes; JSON export peak falls 328,606 to 262,144 bytes.
Playback compares borrowed expected batches against ring drains without allocating either batch.
Docs now explain additive save/drain APIs and const-derived opcode views; original consumers and wire versions are preserved.
Next: matched ARM builds, baseline/follow-up throughput samples, full checks and follow-up PR; no blocker.

Checkpoint: matched Cortex-M4 execution image is 63,252 -> 40,392 bytes; execution plus disassembly 118,932 -> 66,572.
Python 52 tests, Node smoke, TypeScript and browser workflow pass; fresh isolated target passes workspace/state Clippy and tests.
New allocation coverage checks every opcode page and both replay event/trace drains; no-default-feature tests pass.
Quiet native benchmark pairs show roughly flat to 3 percent lower throughput; report the memory tradeoff without a speedup claim.
Next: finish conformance/reference gates, commit exact paths, publish follow-up PR and monitor exact-head CI. Set 64-bit PYO3_PYTHON for all workspace commands.

Checkpoint: all five follow-ups committed at 2f01cd20478152f4f6bacd66b74c7b60408fba57; PR #46 is open and mergeable.
Local DAA 1,000 cases, Z180 7,050 cases, and 3 reference properties (2,000 examples each) pass.
All PR CI checks passed; matching push CI has passed tests and is finishing Windows checkout cleanup.
PR #45 remains merged at faf16b9; original master is fast-forwarded there and all untracked notes are preserved.
Next: verify final exact-head CI status and report the completed follow-up PR and measured tradeoff; follow-up merge was not explicitly requested.

Final: every exact-head PR/push Linux/Windows check and Pages build passed for PR #46 at 2f01cd2.
PR #46 remains open, mergeable and ready for review; isolated worktree is clean.
PR #45 was normally merged, and its master CI and Pages deployment also passed.
All five recommended follow-ups are complete; lower-priority packed pages and remap scans were outside the recommended batch.
Firmware checkout was untouched; actual Flipper device execution remains untested.
