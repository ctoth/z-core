# Rust module refactoring plan authoring notes

## Current state

- The requested execution-ready plan exists at
  `docs/rust-module-refactoring-plan.md`.
- No Rust source file, manifest, historical plan, progress record, or existing
  untracked note has been modified.
- The new plan is intentionally uncommitted; the user asked for the plan, not
  a commit or execution.

## Findings and observations

- `crates/z180-core/src/lib.rs` is 6,942 lines and combines crate-root API and
  state, execution, interrupts, state serialization, debugging, access,
  peripherals, and 3,366 lines of inline unit tests.
- Other primary ownership candidates are `z180-replay/src/lib.rs` (1,468
  lines), `z180-cli/src/sst.rs` (1,291), `z180-py/src/lib.rs` (871), and
  `z180-wasm/src/lib.rs` (823).
- `optable.rs` and `ioregs.rs` are cohesive declarative authorities and are
  explicitly excluded from splitting.
- rust-analyzer provides semantic module assists through LSP, but its
  component is missing for the pinned 1.93.1 toolchain. `ast-grep 0.41.0` is
  installed but is not a semantic cross-module mover.
- The first Python on PATH is 32-bit. Workspace Clippy initially failed
  because PyO3 targets 64-bit Rust. The plan now pins
  `PYO3_PYTHON` to the verified uv-managed 64-bit CPython 3.13.5 executable.
- The plain CI-form Python build normalized stale requirement metadata in
  tracked `crates/z180-py/uv.lock`. That validation-owned diff was inspected
  and restored immediately. The plan now uses verified `uv run --frozen`
  forms so execution cannot absorb this unrelated pre-existing lock drift.

## Validation completed

- All named current source items and planned test filter families exist.
- Every planned destination path is currently absent.
- `cargo fmt --check` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed with the
  pinned 64-bit `PYO3_PYTHON`.
- `cargo clippy -p z180-core --all-targets --features state -- -D warnings`
  passed.
- `cargo test --workspace` passed: CLI 27 unit tests plus two integration
  tests, core 94 tests plus two state-compat tests, replay seven tests.
- The DAA gate passed: opcode 27 reported 1,000 passes and zero failures.
- `cargo test -p z180-core --features state` passed 94 unit tests and both
  released-state compatibility tests.
- The frozen Python binding build passed and all 52 Python tests passed.
- The Node-target WebAssembly build and smoke test passed at 170,717,517
  emulated cycles/second against the 25,000,000 target.
- The differential gate passed all three properties at the gate profile.
- The tracked worktree is clean after validation; only the requested plan,
  this checkpoint, and the five pre-existing notes are untracked.

## Blocker

- None currently. Remaining work is validation only.

## Next action

- Perform the final document/status review and hand off the plan without
  committing or executing it.
