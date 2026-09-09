# Verification scope

PR CI runs workspace/state tests, Python contracts, Node WASM contracts, the
strict TypeScript consumer, the browser debugger workflow, independent DAA
SST cases, the first-party Z180 SST corpus, and reference differential properties.

Nightly CI additionally runs the full pinned shared-Z80 SST corpus and
`tests/vendor/zex/zexdoc-z180.com`, with JSON SST reports and the exerciser's
complete output attached to the exact commit. SST reports include excluded
cases and their reasons; neither missing implementations nor unexpected
failures are converted to exclusions. AF/AF' bits 3 and 5 are not compared
because their behavior is undocumented on Z80180; all eight R bits are checked.

The stock ZEXDOC binary includes encodings undefined on Z180. The documented
Z180 exerciser is the scheduled gate; this is not a claim of undocumented Z80
compatibility. `docs/verification-log.md` and the vendored artifact documentation
retain the implementation and fixture provenance.

Performance is reported separately from correctness. `cargo run --release -p
z180-cli -- bench` reports individual samples, a median, OS/architecture, cycle
budget, warmup count, and retained replay storage for instruction mixes, MMU,
DMA, external callbacks, tracing, and recording. Compare the same workload,
build profile, cycle budget, and machine; shared CI timing is not a release
threshold. Increase `--cycles` for stable measurements on faster hosts.
