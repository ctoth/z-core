# Prebuilt z180-c static libraries on release

## Goal

Let C hosts (first: blmemu) link z-core without a Rust toolchain. Q chose "prebuilt + source" for blmemu on 2026-11-14 and said "Make it so!": z-core publishes static libraries per release; blmemu downloads the pinned one by default and can still build from source.

## State

- `.github/workflows/release.yml`: on `v*` tags (and on PRs that change it, without publishing) builds `libz180.a` with `cargo rustc --crate-type staticlib --locked` for x86_64-unknown-linux-gnu (ubuntu-22.04), x86_64-pc-windows-gnu (ubuntu-22.04) and aarch64+x86_64-apple-darwin (lipo, macOS 11). Each archive `z180-c-VERSION-NAME.tar.gz` holds lib/libz180.a, include/z180.h, native-static-libs.txt (rustc's report) and both licences. It links tests/consumer.c against the package using only those flags, and runs it where the runner can. The tag job checks tag == z180-c's Cargo version, writes SHA256SUMS and creates the release.
- crates/z180-c/README.md documents the assets.

## Decisions and why

- `--crate-type staticlib` builds only the static library, so the Windows GNU cross build needs no DLL link step.
- Linux built on ubuntu-22.04 to match blmemu's runners' glibc.
- No submodules checked out: the SST corpus is not needed to build the library.
- The package's link check uses only native-static-libs.txt, proving the shipped flags suffice.

## Next step

PR, CI green, merge, then tag v0.1.0 (matches crates/z180-c/Cargo.toml) and pin blmemu to the release's SHA256SUMS.
