# z180-cli

`z180-cli` is the command-line host for the shared `z180-core` implementation.
It disassembles raw binaries, runs configured ROMs, executes the SST
conformance corpora, and hosts CP/M ZEX programs.

## Disassemble a binary

Run this from the repository root against the included every-mnemonic fixture:

```powershell
cargo run -p z180-cli -- dis crates/z180-cli/tests/fixtures/dis_every_mnemonic.bin --org 0x4000
```

Output starts with the logical address, encoded bytes, and decoded Z180
instruction:

```text
4000  88          ADC A,B
4001  80          ADD A,B
4002  A0          AND B
```

## Run a ROM

The runner requires a TOML machine description with exactly one ROM region.
Region bases and sizes are 4 KiB aligned, and the ROM file length must equal
its configured region size.

```toml
clock_hz = 12_288_000
variant = "z80180"

[[regions]]
kind = "rom"
base = 0x00000
size = 0x10000

[[regions]]
kind = "ram"
base = 0x10000
size = 0x10000
```

With that saved as `machine.toml` and a matching 64 KiB `rom.bin`:

```powershell
cargo run -p z180-cli -- run rom.bin --cycles 1000000 --config machine.toml
```

Add `--trace` to print entry cycle, logical PC, physical PC, bytes, and the
decoded instruction. An undefined instruction stops the run with an explicit
Z180 TRAP diagnostic.

## Run conformance tools

Print the checked-in Z180 corpus census:

```powershell
cargo run -p z180-cli -- sst --dir tests/z180-sst --census
```

Run the two instruction corpora:

```powershell
cargo run -p z180-cli -- sst --dir tests/sst/v1
cargo run -p z180-cli -- sst --dir tests/z180-sst
```

The `zex` subcommand runs the repository's pinned CP/M exerciser artifacts.
Use `cargo run -p z180-cli -- --help` or append `--help` to a subcommand for
its exact arguments.
