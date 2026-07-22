# z180-core

`z180-core` is the deterministic, cycle-counting Z80180/Z8S180 implementation
used by every z-core host surface. It is `no_std` + `alloc`, forbids unsafe
code, and has no default dependencies.

## Use the core

Add the workspace crate as a dependency while developing in this checkout:

```toml
[dependencies]
z180-core = { path = "../z-core/crates/z180-core" }
```

This complete program maps one 4 KiB ROM page, executes its first NOP, and
prints the consumed and total cycle counts:

```rust
use z180_core::{HostBus, MachineConfig, Reg, RegionDef, RegionKind, Z180};

struct BoardBus;

impl HostBus for BoardBus {
    fn mem_read(&mut self, _phys: u32) -> u8 { 0xff }
    fn mem_write(&mut self, _phys: u32, _value: u8) {}
    fn io_read(&mut self, _port: u16) -> u8 { 0xff }
    fn io_write(&mut self, _port: u16, _value: u8) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut rom = vec![0; 0x1000];
    rom[0] = 0x00; // NOP

    let config = MachineConfig {
        regions: vec![RegionDef {
            base: 0,
            size: rom.len() as u32,
            kind: RegionKind::Rom(rom),
        }],
        ..MachineConfig::default()
    };
    let mut cpu = Z180::new(config, BoardBus)?;

    let consumed = cpu.step();
    assert_eq!(cpu.reg(Reg::PC), 1);
    println!("step={consumed} total={}", cpu.cycle_count());
    Ok(())
}
```

RAM and ROM are core-owned. Configure `RegionKind::External` only where a
board must receive `HostBus::mem_read` and `mem_write` calls. All external I/O
ports use `HostBus::io_read` and `io_write`; internal-I/O accesses also emit
their required duplicate external bus cycle.

## Features

Enable versioned save states explicitly:

```toml
z180-core = { path = "../z-core/crates/z180-core", features = ["state"] }
```

The state payload restores emulated state but deliberately does not serialize
the host bus or external address mapper.

## Verify

From the z-core repository root:

```powershell
cargo test -p z180-core
cargo test -p z180-core --features state
```

See [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md) for the execution,
memory, and event flows.
