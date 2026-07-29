# z180-replay

`z180-replay` is the deterministic history and time-travel layer for
`z180-core`. It owns one `Z180<ReplayBus<B>>`, records every ordered host-bus
access and host stimulus, and periodically stores core save-state checkpoints.

Historical playback never calls the live host bus. Recorded read values are
supplied to the CPU, writes are compared with the journal and suppressed, and
a host failure is reproduced at the same bus record. This matters for
stateful devices such as flash command engines: replaying an old write against
the live device would mutate it twice.

## Contract

- Positions count every attempted `try_step`, including attempts that return a
  host-bus error and sleeping attempts that consume zero cycles.
- Checkpoints contain `z180-core` state plus journal cursors. The host bus and
  external address mapper remain the wiring of the same owned machine.
- `setup()` is mutable only before `start()`. After recording begins,
  `machine()` is read-only; external device inputs go through `apply()` and
  output drains go through `drain()` so their exact instruction boundaries
  are journaled.
- Calling `seek()` enters playback. Playback may advance only through recorded
  history. It cannot branch into a new live future because the live host
  devices have not been rewound.
- `find_first_write()` is a temporary probe. It restores the caller's exact
  state, journal position, and mode before returning. It fails rather than
  returning an incomplete answer if the event ring loses data.
- Journals are currently in-memory and grow with bus traffic and host actions.
  Checkpoints are bounded by `Options::max_checkpoints`; the initial
  checkpoint is retained so all recorded history remains seekable.

## Example

```rust
use core::convert::Infallible;
use z180_core::{HostBus, MachineConfig, RegionDef, RegionKind};
use z180_replay::{Options, Timeline};

struct Board;

impl HostBus for Board {
    type Error = Infallible;

    fn mem_read(&mut self, _address: u32) -> Result<u8, Self::Error> {
        Ok(0xff)
    }

    fn mem_write(&mut self, _address: u32, _value: u8) -> Result<(), Self::Error> {
        Ok(())
    }

    fn io_read(&mut self, _port: u16) -> Result<u8, Self::Error> {
        Ok(0xff)
    }

    fn io_write(&mut self, _port: u16, _value: u8) -> Result<(), Self::Error> {
        Ok(())
    }
}

let config = MachineConfig {
    regions: vec![RegionDef {
        base: 0,
        size: 0x1000,
        kind: RegionKind::Ram,
    }],
    ..MachineConfig::default()
};
let mut timeline = Timeline::new(config, Board, Options::default())?;
timeline.setup()?.mem_poke(0, 0x00); // NOP before history starts
timeline.start()?;

let beginning = timeline.position();
timeline.try_step()?;
let after_nop = timeline.position();

timeline.seek(beginning)?;
timeline.try_step()?;
assert_eq!(timeline.position(), after_nop);

# Ok::<(), Box<dyn std::error::Error>>(())
```

The concrete error enums intentionally preserve the distinction between a
live host error, a recorded historical host failure, a journal divergence,
and an invalid checkpoint or position.
