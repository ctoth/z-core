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
    type Error = std::convert::Infallible;

    fn mem_read(&mut self, _phys: u32) -> Result<u8, Self::Error> { Ok(0xff) }
    fn mem_write(&mut self, _phys: u32, _value: u8) -> Result<(), Self::Error> { Ok(()) }
    fn io_read(&mut self, _port: u16) -> Result<u8, Self::Error> { Ok(0xff) }
    fn io_write(&mut self, _port: u16, _value: u8) -> Result<(), Self::Error> { Ok(()) }
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

An infallible bus uses `step()` and `run()` as above. A bus that can fail uses
`try_step()` and `try_run()`; the core returns its error without charging the
failed instruction or performing later memory or I/O effects. Debugger
`mem_poke()` is identical for both kinds of bus because it writes core-owned
RAM only and never calls the bus.

## Features

Enable versioned save states explicitly:

```toml
z180-core = { path = "../z-core/crates/z180-core", features = ["state"] }
```

The state payload restores emulated state but deliberately does not serialize
the host bus or external address mapper.

## Embedded memory use

Construction transfers the ROM buffers in `MachineConfig` into the machine
without copying their bytes. RAM is allocated and zeroed for each configured
RAM region; ROM remains heap-owned. A 20-bit physical address space allocates
only its page table, rather than a full 1 MiB guest image.

Use `event_capacity: 0` when event capture is unnecessary. The default is 4096
events, allocated lazily when tracing, watches, or an event producer first
needs the ring. Instruction tracing is disabled by default.

With `state` enabled, `save_state()` serializes borrowed machine data directly
into the returned byte buffer. It does not clone guest RAM, ROM, or queues.
Save-state version 4 and its byte encoding remain unchanged.
Use `save_state_into(&mut Vec<u8>)` to reuse a buffer, or
`save_state_to_slice(&mut [u8])` to write without allocating. Both include the
version byte. The Vec is cleared before writing and retains its capacity; the
slice returns its written prefix. A short slice returns
`SaveStateError::BufferTooSmall` and may contain a partial payload.

`drain_events_iter()` and `drain_insn_trace_iter()` consume entries without
allocating, in chronological order. Dropping a partial drain discards its
remaining entries while keeping ring storage and the sticky event-loss flag.
The existing Vec-returning drain methods remain available.

Opcode execution tables omit mnemonic and operand formatting data. Both
execution and disassembly tables are derived at compile time from the same
definitions; disassembly retains no CPU handler pointers.

## Verify

From the z-core repository root:

```powershell
cargo test -p z180-core
cargo test -p z180-core --features state
```

See [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md) for the execution,
memory, and event flows.
