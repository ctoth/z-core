# Rust refactoring tooling setup

## User authority

- Scope is Rust refactoring tooling for
  `docs/rust-module-refactoring-plan.md`, not general build/test tooling.
- Rust source must not be changed with Codex generic edit/write tools,
  shell-generated rewrites, or text-level substitution.
- Rust source moves must be produced by genuine Rust-aware refactoring
  operations.

## Current findings

- Repository toolchain is pinned by `rust-toolchain.toml` to Rust `1.93.1`.
- The pinned toolchain initially lacked `rust-analyzer` and `rust-src`.
- Installed both with:
  `rustup component add rust-analyzer rust-src --toolchain 1.93.1`.
- Verified pinned server:
  `rust-analyzer 1.93.1 (01f6ddf7 2026-02-11)`.
- Verified the pinned binary contains the `extract_module`,
  `move_module_to_file`, and `move_item` operations.
- Installed official VS Code extension
  `rust-lang.rust-analyzer@0.3.2989`.
- Installed Helix `25.07.1` as a terminal LSP client because it provides an
  executable code-action channel for Codex without raw source editing.
- `hx --health rust` finds the rust-analyzer rustup proxy and reports the Rust
  parser, queries, and indentation support healthy.
- `rust-analyzer analysis-stats . --run-all-ide-things` loaded all eight
  workspace crates and exited successfully. It printed repeated internal
  `inference diagnostic in desugared expr` log messages while exercising IDE
  diagnostics, so the semantic engine loads the workspace but this noisy
  diagnostic path should not be represented as a clean diagnostic gate.
- Cargo generated an isolated smoke-test library at
  `C:\Users\Q\AppData\Local\Temp\z-core-ra-refactor-smoke`; no generic writer
  created its Rust source.
- Helix invoked the pinned rust-analyzer and applied real LSP code actions in
  that scratch crate: Extract Module, LSP Rename, Move Module to File, Import,
  and Remove all unused imports.
- The first compiler gate failed because structural extraction left a nested
  test module unable to resolve `add`. The fix was made only through
  rust-analyzer semantic actions, after which `cargo test` passed.
- The exact documented install/repair commands have now been rerun:
  rustup reports both pinned components current, Scoop reports Helix
  `25.07.1` installed, and VS Code reports
  `rust-lang.rust-analyzer@0.3.2989` installed.
- The documented verification commands currently report rust-analyzer
  `1.93.1`, Helix `25.07.1`, and a healthy Helix Rust integration.
- Helix reports no code action for `move_item`; rust-analyzer exposes this
  experimental operation through client-specific integration rather than the
  ordinary code-action menu used for module extraction.
- The installed VS Code extension package was directly inspected and registers
  `rust-analyzer.moveItemUp`, `rust-analyzer.moveItemDown`, and the
  `experimental/moveItem` server request.
- An initial VS Code command-URI attempt did not produce a saved file change
  and was not accepted as verification.
- The command-URI approach was rejected because it was not an LLM-operable
  end-to-end mutation path.
- A second isolated VS Code instance was launched with Chromium DevTools on
  port `9333`. The LLM connected to that instance through CDP and dispatched
  trusted command-palette and save key events; VS Code and rust-analyzer
  remained the only Rust-source mutators.
- `rust-analyzer: Move Item Down` moved the complete `add` item below
  `mod smokemodname;`. The saved file hash became
  `85509627757F7669711F322731B16DB6892CC35A56861843D1870CCAEF8C66E4`,
  and `cargo test` passed.
- `rust-analyzer: Move Item Up` restored the original order and exact SHA-256
  `3447ACCF369783A043C54F49CB0E7C4D237A6913B392BFDE2472E43B53837ECA`;
  `cargo test` passed again.
- No user input or generic Rust-source edit/write operation was used for
  either item move.
- The dedicated VS Code instance was closed through CDP after verification,
  port `9333` was confirmed no longer listening, and the temporary browser
  endpoint tab was closed.

## Tooling decision so far

- rust-analyzer is the refactoring engine.
- The intended move sequence is:
  1. use rust-analyzer Move Item Up/Down when complete Rust items must be made
     contiguous;
  2. select complete items and apply rust-analyzer Extract Module;
  3. rename the generated module using LSP Rename;
  4. apply rust-analyzer Move Module to File;
  5. review the generated diff and run compiler/tests as validation.
- `ast-grep` is not an acceptable module-move mechanism.
- RustRover is not currently selected: its documented Move refactor supports
  files and top-level items, while rust-analyzer directly documents extraction
  of methods selected from inside an `impl` block.

## Current repository state observed

- `master` is ahead of `origin/master` by one commit.
- Tracked and staged diffs were clean at the start of this tooling task.
- Existing untracked paths include user notes and
  `crates/z180-core/src/tests/`; none have been modified.
- `docs/rust-module-refactoring-plan.md` now records the exact installed
  versions, Rust-aware operation boundary, source-mutation protocol, and live
  smoke-test result.
- No Rust source in the repository has been changed by this task.

## Next action

Validate every documented command, inspect the Markdown-only diff and Git
status, and report the completed end-to-end LLM tooling. Keep this task note
uncommitted and do not stage it.
