# qns migration to z-core internal memory

This guide describes the direct `z180.Machine` path. The already shipped
`z180.compat.Z180` path is the drop-in first step, but it leaves all memory on
Python callbacks and is not the internal-memory design.

The source snapshot used for this guide is the current qns layout:

- `qns/profiles.py` selects six models and a 0, 2 MiB, or 4 MiB flash backing
  store.
- `qns/memory.py` defaults to 512 KiB RAM and 256 KiB ROM and exposes the
  banked flash through the physical `80000h-FFFFFh` aperture.
- `qns/bns.py` runs in 1,000-cycle chunks and currently puts observation,
  shadow-RAM behavior, and flash behavior behind the same memory callbacks.

Internal mode changes ownership, not the BNS hardware map: z-core owns the
512 KiB RAM hot path, qns continues to own flash and every external I/O
device.

## 1. Install and imports

Add the `z180` wheel to qns's runtime dependencies and replace the production
import of `qns.cpu.Z180` with:

```python
from z180 import IrqLine, Machine, Reg, WatchKind
```

Keep `qns.cpu.Z180` only as the old-CFFI benchmark subject until P8.4 records
the comparison. Do not put another wrapper or adapter between `BNS` and
`Machine`; internal mode uses `Machine` directly.

## 2. Put region facts in `qns/profiles.py`

Add `ram_size` and `rom_size` to `HardwareProfile` and set them to
`512 * 1024` and `256 * 1024`, respectively, for all six current profiles.
`flash_size` remains exactly as it is now:

| Profile | Core RAM | ROM image | qns flash backing | Physical high aperture |
|---|---:|---:|---:|---|
| `bsp` | 512 KiB | 256 KiB | 0 | unmapped |
| `bs2` | 512 KiB | 256 KiB | 2 MiB | External |
| `bsl` | 512 KiB | 256 KiB | 0 | unmapped |
| `bl2` | 512 KiB | 256 KiB | 2 MiB | External |
| `bl4` | 512 KiB | 256 KiB | 4 MiB | External |
| `tns` | 512 KiB | 256 KiB | 0 | unmapped |

The 2 MiB and 4 MiB values are backing-store sizes, not address-space region
sizes. The CPU always sees one 512 KiB aperture; qns's existing
`high_bank_latch` and `_flash_offset()` choose the backing page.

## 3. Construct the native machine

In `BNS.__init__`, build this list directly from the selected profile:

```python
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

Create `self.memory` before this block as qns does today. Its initial
`bytearray` is replaced immediately by the writable, zero-copy `memoryview`.
Keep that view for the lifetime of the machine. `Machine.remap()` and
`Machine.load_state()` intentionally raise `BufferError` while a RAM view is
alive, so qns must not use those APIs for its nonvolatile-state files.

The memory callbacks are called only for the configured External flash
aperture. Low RAM fetches, reads, and writes never enter Python. I/O callbacks
remain active for external ports and for the Z180's required duplicate bus
cycle on an internal-I/O access.

Profiles without flash deliberately leave `80000h-FFFFFh` unmapped. Do not add
an External region for them: doing so would restore Python callbacks to an
address range that the profile does not provide.

## 4. Replace shadow RAM in `qns/memory.py`

The current `_written_addrs` policy says that an unwritten address below the
ROM size reads ROM, while a written address reads RAM. A core-owned RAM region
implements the same effective bytes if ROM is copied into RAM before
execution. Make these changes:

1. Keep `rom` for firmware discovery, `trace_boot()`, and import of V1/V2
   state. Keep `flash`, `high_bank_latch`, `_flash_command`,
   `_flash_offset()`, and `_write_flash()` unchanged.
2. Allow `ram` to be the `memoryview` assigned by `BNS.__init__`.
3. In `load_rom()`, zero both the RAM view and `rom`, copy the image into
   both at the requested offset, and set `rom_loaded`. Reject an image or
   offset that exceeds either configured size instead of relying on slice
   truncation.
4. In `read()`, retain 20-bit masking and the flash check. Otherwise return
   `ram[addr]` when `addr < len(ram)`, then `0xFF`. There is no runtime
   `_written_addrs` branch.
5. In `write()`, retain the flash command path. Otherwise write directly to
   `ram[addr]` when in range. Do not update `_written_addrs`.
6. Delete the persistent `_written_addrs` set after legacy-state import is
   implemented as described below.

The effective initialization is therefore:

```text
00000h .. end of loaded ROM     ROM bytes copied into core RAM
end of loaded ROM .. 7FFFFh     zero-filled core RAM
80000h .. FFFFFh                qns flash callback, or unmapped
```

`dump_ram()` remains `bytes(self.memory.ram)`; a memoryview supports that
conversion without changing the file format.

## 5. Move callback-side observers off the memory hot path

`BNS._mem_read` and `_mem_write` currently do more than memory access. Simply
leaving those methods in place would silently lose all low-RAM observation,
because internal RAM never calls them.

### Writes

Install one full-RAM write watch when exact write counts, input-boundary
epochs, first-N logging, CSV logging, or a requested write trace is active:

```python
self._ram_write_watch = self.cpu.add_mem_watch(
    0x00000, self.profile.ram_size, WatchKind.Write
)
```

After every execution unit, drain events and feed each `mem_write` event to
the existing observer logic using its `phys`, `value`, `pc`, and `cycle`
fields. Split that observer logic from the storage write: z-core has already
changed RAM, so the event consumer must not call `memory.write()` again.

The External flash path still enters `_mem_write`. It must run the same
observer logic once and then call `memory.write()` for the flash command
state machine. This division prevents both missed counts and double writes.

For a requested single-address or range trace, a narrower write watch is
sufficient only when the global `stats['writes']` count and input epochs are
not required. Convert qns's inclusive `trace_writes_range=(start, end)` to
the core's half-open API with `base=start, size=end-start+1`.

Call `drain_events()` at least once per existing 1,000-cycle chunk. After
every drain, treat `events_lost()` as a fatal diagnostic error; a dropped
write event makes qns's counters and input protocol untrustworthy. Do not
clear the flag and continue.

### Reads at discovered instruction boundaries

The current read callback observes opcode fetches at
`keyboard_wait_pc` and `capture_addr`, then reads registers and RAM at that
instant. A delayed memory event does not contain the needed register snapshot.
When either observer is active, execute one instruction at a time and perform
the boundary check immediately before `step()`:

```python
pc = self.cpu.reg(Reg.PC)
physical_pc = self.cpu.mmu_translate(pc)
```

If `physical_pc` equals `keyboard_wait_pc`, update the ready/consume epochs
from the current RAM view. If it equals the English `capture_addr`, read
`Reg.HL`, `Reg.BC`, CBR (`io_reg_peek(0x38)`), and the message bytes before
calling `step()`. This preserves the old callback's instruction-entry view
without adding a stop-on-watch API.

Do not install an all-RAM read watch. It would create an event for every
opcode and operand fetch and defeat the internal-memory performance model.

When neither callback-time boundary observer nor callback-time device is
enabled, retain qns's 1,000-cycle `Machine.run(chunk)` loop. Write events may
be processed after that chunk because each event already carries the exact
PC, cycle, address, and value.

## 6. Do not re-enter `Machine` from a callback

PyO3 holds the machine's mutable borrow while `step()` or `run()` invokes a
Python bus callback. A qns callback must not call `self.cpu.cycle_count()`,
`reg()`, `io_reg_peek()`, or any other `Machine` method reentrantly.

For the existing gas-gauge and trace users, set a BNS-owned value immediately
before each instruction:

```python
self._callback_cycle = self.cpu.cycle_count()
actual = self.cpu.step()
```

Use `_callback_cycle` inside `_io_read`, `_io_write`, gas-gauge handlers, and
interrupt logging. z-core advances its public cycle total at instruction
completion, so the captured value is the same instruction-entry position
that is available while the callback runs. This is another reason the
correctness-first qns path is instruction-at-a-time when cycle-sensitive I/O
is enabled.

## 7. Adapt serial and CSI/O callbacks to queues

Do not pass `serial_rx`, `serial_tx`, `csio_rx`, or `csio_tx` to `Machine`;
they are not bus callbacks. Preserve one pending byte per ASCI channel and
one pending CSI/O byte, as the compat layer does.

Before each instruction or safe execution chunk:

1. If an ASCI channel has no pending byte, call `_serial_receive(channel)`.
2. Retain a nonnegative result, masked to eight bits, until
   `asci_rx_push(channel, byte)` returns `True`.
3. Obtain CSI/O input from the selected display/clock device and retain it
   until `csio_rx_push(byte)` returns `True`.

After each instruction or chunk, repeatedly call `asci_tx_pop(channel)` for
both channels and deliver every returned byte through `_serial_transmit()`.
Likewise, drain `csio_tx_pop()` and deliver each byte to the selected CSI/O
device. A `None` result ends that queue's drain.

`BNS.step()` must perform the same pump, native step, event processing, and
drain sequence as the run loop. One private execution method is justified
here because it is the single owner of that ordering for both callers; do
not create a CPU facade.

## 8. Direct API substitutions in `qns/bns.py`

Internal mode uses these exact replacements:

| Compat expression | Direct `Machine` expression |
|---|---|
| `cpu.cycle_count` | `cpu.cycle_count()` outside callbacks; `_callback_cycle` inside |
| `cpu.get_reg(Z180.HL)` | `cpu.reg(Reg.HL)` |
| `cpu.get_reg(Z180.BC)` | `cpu.reg(Reg.BC)` |
| `cpu.pc` | `cpu.reg(Reg.PC)` |
| `cpu.instruction_pc` | event `pc`, or `cpu.instruction_pc()` outside callbacks |
| `cpu.halted` | `cpu.halted()` |
| `cpu.cbr` / `bbr` / `cbar` | `cpu.io_reg_peek(0x38 / 0x39 / 0x3A)` |
| `cpu.set_irq(0, state)` | `cpu.set_irq(IrqLine.Int0, bool(state))` |
| `cpu.set_irq(1, state)` | `cpu.set_irq(IrqLine.Int1, bool(state))` |
| `cpu.set_irq(2, state)` | `cpu.set_irq(IrqLine.Int2, bool(state))` |
| `cpu.watch_pc(addr)` | `cpu.set_pc_watch(addr)` |
| `cpu.pc_watch_count` | `cpu.pc_watch_hits()` |

For qns's `pc_watch_cycle` and `pc_watch_cbar` output, record
`cycle_count()` and `io_reg_peek(0x3A)` immediately before stepping an
instruction whose `reg(Reg.PC)` equals the armed logical address. The native
PC watch remains the count authority.

The `Memory.cbr/bbr/cbar` mirrors may remain temporarily for the existing I/O
device registration, because duplicate internal-I/O bus cycles keep them
updated. They are not a translation authority in internal mode; z-core's MMU
is the authority. Status output should read the native registers directly.

## 9. Nonvolatile-state migration

The current V1/V2 state stores raw shadow RAM plus a bitmap. New internal RAM
stores the effective readable bytes, so introduce `_STATE_MAGIC_V3 =
b"QNSRAM\\x00\\x03"` with:

```text
magic | ram_size:u32le | flash_size:u32le | effective_ram | flash
```

New saves contain no shadow bitmap. For state directories, write `ram.bin`
and `flash.bin`; omit `shadow.bin`. Presence of `shadow.bin` identifies a
legacy directory.

Keep V1/V2 and legacy-directory reads. Because qns CLI already calls
`load_rom()` before `load_state()` or `load_state_dir()`, convert old data as
follows:

1. Start the core RAM view with the loaded ROM bytes and zeros beyond ROM.
2. For each address below `ram_size`, copy the legacy RAM byte when the old
   bitmap bit is set.
3. Also copy every legacy RAM byte at addresses at or beyond `rom_size`,
   because the old reader returned RAM there even when its bitmap bit was
   clear.
4. Restore flash exactly as V2 does.

V3 and new directory loads copy `ram.bin` directly into the core RAM view and
restore flash. Validate lengths before changing either buffer so a malformed
state cannot partially load. Once legacy conversion has completed,
`_written_addrs` has no runtime role.

Add migration tests for an unwritten ROM byte, a written byte under ROM, an
unwritten byte beyond ROM, both flash sizes, V1, V2, V3, legacy directories,
and new directories.

## 10. Compat diagnostics

`z180.compat.Z180.asci_debug_state()` preserves the incumbent dictionary
shape. `status`, `cntla`, and `tx_data_register` are native register values.
These fields have no public core equivalent and intentionally return zero or
`False`: `rx_bits_remaining`, `rx_fifo_depth`, `tx_bits_remaining`,
`tx_shift_register`, `irq_pending`, `brg_divisor`, `frame_bits`,
`rie_set_count`, `rie_clear_count`, `rie_last_pc`, `rie_last_cycle`,
`stat_write_count`, `stat_last_write`, `stat_last_write_pc`, and
`stat_last_write_cycle`. `reset_asci_debug()` is therefore a no-op.

Direct internal mode should use `io_reg_peek()` for hardware-visible ASCI
registers and should not copy the synthetic zero-filled diagnostics into a
new wrapper.

## 11. Performance expectation and verification

The Phase 8 target for a release-built Python binding in internal-memory mode
is at least 50 million emulated cycles per second. P8.3 does not claim a
measured result; P8.4 must run `python bench.py` and record all three measured
rates: compat callbacks, internal memory, and the old CFFI binding.

That 50M target applies to the internal `Machine.run()` benchmark with guest
RAM in the core. Fully instrumented qns can be slower when it deliberately
uses per-instruction queue pumping, pre-step boundary observation, or a full
RAM write watch. Report those modes separately rather than presenting a raw
core number as end-to-end qns throughput.

Before making internal mode the qns default, require:

1. Existing qns tests green through the compat path.
2. New region, ROM-shadow, legacy-state, observer, event-overflow, queue, and
   callback-reentrancy regressions green through the direct path.
3. One boot per profile with the same selected ROM and persisted state on
   both paths, comparing externally visible output and input acceptance.
4. The P8.4 benchmark table recorded, with internal mode at or above 50M
   cycles/sec.

Keep compat mode selectable until those checks pass. Compatibility is a
rollout fallback; it is not evidence that internal-memory mode is active.
