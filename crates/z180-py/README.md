# z180 Python binding

This crate builds the abi3 Python package `z180`. `z180.Machine` exposes the
shared core directly; `z180.compat.Z180` preserves qns's historical callback
surface for staged migration.

## Build in this checkout

From the repository root, enter the crate and install its build frontend into
the uv command environment:

```powershell
Set-Location crates/z180-py
uv run --with maturin maturin develop --release
uv run pytest tests
```

Python 3.11 or newer is required.

## Run a core-owned RAM machine

Save this as `basic.py` after running `maturin develop`:

```python
from z180 import Machine, Reg

machine = Machine({
    "regions": [{"base": 0, "size": 0x1000, "kind": "ram"}],
})
ram = machine.ram(0)
ram[:5] = b"\x3e\x5a\x32\x08\x00"  # LD A,5Ah; LD (0008h),A

machine.step()
machine.step()

assert ram[8] == 0x5A
assert machine.reg(Reg.AF) >> 8 == 0x5A
print(f"cycles={machine.cycle_count()} pc={machine.reg(Reg.PC):04X}")
```

Run it in the crate environment:

```powershell
uv run python basic.py
```

`ram(base)` is a writable zero-copy view. Release all live views before
calling `remap()` or `load_state()`, because either operation could replace
their backing storage.

## qns integration

The direct qns path keeps 512 KiB of RAM inside z-core and leaves the banked
flash aperture plus external I/O in qns:

```python
from z180 import Machine

regions = [
    {"base": 0x00000, "size": self.profile.ram_size, "kind": "ram"},
]
if self.profile.flash_size:
    regions.append({"base": 0x80000, "size": 0x80000, "kind": "external"})

self.cpu = Machine(
    config_dict={
        "clock_hz": clock,
        "phys_addr_bits": 20,
        "unmapped_read": 0xFF,
        "variant": "Z80180",
        "regions": regions,
        "event_capacity": 4096,
    },
    mem_read=self._mem_read,
    mem_write=self._mem_write,
    io_read=self._io_read,
    io_write=self._io_write,
)
self.memory.ram = self.cpu.ram(0x00000)
```

For profiles without flash, leave `80000h-FFFFFh` unmapped. Do not configure
an external region merely to preserve callbacks. ASCI and CSI/O use
`asci_rx_push`/`asci_tx_pop` and `csio_rx_push`/`csio_tx_pop`, not bus
callbacks. A Python bus callback must not call back into the same `Machine`
while `step()` or `run()` holds it.

The complete [qns migration guide](../../docs/qns-migration.md) specifies ROM
initialization, event-backed write observation, instruction-boundary reads,
queue pumping, and legacy state conversion. For the initial callback-backed
rollout only:

```python
from z180.compat import Z180
```

Compat mode is a rollout fallback; it is not the internal-memory performance
path.
