# Notes — PLAN.md authoring session (2026-07-20)

## State
- `PLAN.md` (~962 lines) complete and current in the repo root. Uncommitted — awaiting Q's word before it becomes the first commit and goes to codex.
- No code exists yet; the repo is intentionally empty besides the plan and this note.

## What was decided and why (condensed)
- One Rust core (`no_std`+alloc, no unsafe, MIT OR Apache-2.0) compiled to every host: PyO3 for Python, wasm-bindgen for TS, C-ABI/XCFramework for Swift later. No Z180→WASM binary translation (BNS bank-switching + RAM code defeats it; interpreter speed is already ~10× more than needed; conformance is far easier on an interpreter).
- Replacing z180emu is a shipping requirement, not vanity: it is GPLv2 (verified from its COPYING) and GPLv2 is generally incompatible with App Store distribution.
- Guest memory lives inside the core (4K page table, phys space configurable 20–24 bits; MMU stays honest 20-bit, board-level banking = host-driven page remapping). This removes the per-access Python callback tax that makes qns slow today.
- Conformance-first is a hard gate (Q's explicit requirement): Phase 1 builds ALL test apparatus (SingleStepTests/z80 runner, oracle-generated Z180/TRAP/MMU corpus via qns's existing z180emu binding as a black box, ZEXDOC harness, `--sabotage-ld` negative control) before any real CPU implementation is allowed.
- Hypothesis hardening (Q asked; applied): shared strategies module from Phase 1; oracle self-consistency property before the corpus is trusted; Phase 8 differential fuzzer (single-instr, 32-instr lockstep, MMU three-way vs closed formula) with shrunk counterexamples pinned forever; Rust proptest catalog + peripheral RuleBasedStateMachine invariants derived from UM0050, deliberately not from the shaky incumbent ASCI. See PLAN.md Appendix F.

## What was tried / what failed
- Bash heredoc append to PLAN.md failed once with a quoting error; switched to Write-scratch-file + `cat >>` per the standing rule about multi-line text through shells. No further failures.
- Three stale "Appendix C" cross-references (opcode policy is Appendix A) found and fixed; appendix lettering normalized (F=property catalog, G=glossary).

## Blockers
- None. Open items owned by Q: npm package name (placeholder `@zcore/z180`), zexdoc.com source if no trustworthy mirror is found (plan marks it BLOCKED-ask-Q).

## Next step
- On Q's word: commit PLAN.md as the first commit; hand to codex for execution; I take the critic role per PLAN.md Appendix E (review protocol: gate outputs pasted in PROGRESS.md, verification-log spot checks, negative-control rerun, clean-room spot check).
