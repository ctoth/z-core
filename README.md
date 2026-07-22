# z-core

z-core is a from-scratch, deterministic Z80180/Z8S180 CPU and SoC emulator in
Rust. It provides the CPU, MMU, interrupts, timers, ASCI, CSI/O, DMA, tracing,
and save-state machinery used by native Rust, Python, and WebAssembly hosts.
Its first customer is the [qns](https://github.com/ctoth/qns) Braille 'N Speak
emulator.

The core is `no_std` + `alloc`, forbids unsafe code, and keeps guest RAM inside
the emulator. Host callbacks are reserved for board-owned external memory and
I/O, which keeps the normal fetch and operand path fast.

## Start here

Run the workspace tests:

```powershell
cargo test --workspace
```

Disassemble the included every-mnemonic fixture:

```powershell
cargo run -p z180-cli -- dis crates/z180-cli/tests/fixtures/dis_every_mnemonic.bin --org 0x4000
```

Build the Python binding or WebAssembly package by following the README in the
corresponding crate:

- [`z180-core`](crates/z180-core/README.md): native Rust API
- [`z180-cli`](crates/z180-cli/README.md): disassembler, ROM runner, SST, and ZEX
- [`z180-py`](crates/z180-py/README.md): Python API and qns integration
- [`z180-wasm`](crates/z180-wasm/README.md): Node.js, browser, and TypeScript

The [architecture](docs/ARCHITECTURE.md) describes the as-built data flow.
The [qns migration guide](docs/qns-migration.md) gives the exact move from the
callback compatibility path to core-owned RAM. Clean-room implementation facts
and their UM0050 citations are recorded in
[`docs/verification-log.md`](docs/verification-log.md).

## License

Licensed under either Apache-2.0 or MIT, at your option.
