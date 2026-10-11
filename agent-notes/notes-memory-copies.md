2026-10-01 checkpoint
- Flipper probe cross-compiles: code/constants 63,256 bytes; small guests plausible, current BNS memory does not fit.
- Authorized allocation cleanup is isolated in C:/Users/Q/code/z-core-memory on reduce-memory-copies; original notes preserved.
- New allocation regressions are red: ROM construction adds 71,776 bytes; saving an 80 KiB guest peaks at 301,658 bytes.
- Next: move initial ROM buffers, borrow save-state data and serialize into one output, preserve v4 bytes and check consumers.

2026-10-01 verification checkpoint
- ROM construction now moves buffers; save_state borrows machine data and writes version plus payload into one Vec; loading moves decoded rings.
- Allocation cases improved 71,776 -> 6,240 bytes (64 KiB ROM construction), 301,658 -> 131,072 bytes (80 KiB guest save).
- Released v4 fixture remains byte-identical; workspace/default-core tests, Clippy, ARM state build, Python 52 tests, Node/TypeScript/browser checks pass.
- Updated Flipper probe code/constants total 63,100 bytes (156 bytes smaller); changes stay on reduce-memory-copies in z-core-memory.
