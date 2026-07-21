# Timing model

z-core counts elapsed time in Z8018x system-clock (phi) cycles. It is a
cycle-counting emulator, not a bus-waveform simulator: instruction and
peripheral scheduling use the same cycle clock, but individual T1/T2/T3/TW
signal phases are not represented.

The UM0050 facts behind the implemented values are recorded in the
[verification log](verification-log.md).

## What is counted now

`optable.rs` is the only production source of instruction base timing. Its
opcode entries represent:

- fixed instruction timing;
- taken and untaken conditional timing;
- terminal and repeating block-operation timing;
- the Z80180/Z8S180 RETI timing difference; and
- HALT-mode, TRAP, and interrupt-acknowledge base costs.

As an instruction executes, each access captures the DCNTL waits programmed at
that access: 0–3 for each CPU memory read or write and 1–4 for each external-I/O
read or write. A DCNTL write therefore affects later accesses, not the earlier
fetches of the instruction performing the write. DCNTL resets to `F0h`, so
reset-state execution adds three waits per memory access and four waits per
external-I/O access.

The access count includes opcode and operand fetches, data reads and writes,
stack accesses, TRAP accesses, and the memory read performed by each HALT-mode
idle cycle. Internal-I/O cycles receive no external-I/O waits. Their required
duplicate external bus cycles use the same internal timing and likewise ignore
WAIT and the programmed external-I/O wait count.

`step()` returns the total base-plus-wait cycles consumed by one instruction or
one HALT-mode idle cycle. A sleeping CPU returns zero because no CPU clock
progress occurs until a wake source exists. `run(budget)` executes complete
steps until it reaches or exceeds the requested budget, and returns the actual
cycles consumed. `cycle_count()` accumulates those cycles from construction;
`reset()` models hardware reset and does not rewind that elapsed-time clock.

### Interrupt acknowledge

The UM0050 base totals are 11 cycles for NMI, 13 for the fixed Mode 0 RST
response, 11 for INT0 Mode 1, and 18 for INT0 Mode 2, INT1, INT2, and internal
vectored responses. The automatic two acknowledge wait states shown in the UM
figures are already part of those base totals. Logical stack writes, the NMI
acknowledge memory read, and vectored restart-address reads independently add
the DCNTL memory waits programmed when each access occurs.

The public `HostBus` contract has no interrupt-acknowledge data callback. As
allowed by Phase 5 task 3, z-core fixes the acknowledge data byte at `FFh`:
INT0 Mode 0 therefore executes RST 38h, and INT0 Mode 2 uses `FFh` as the low
byte of its `I:FFh` vector-table address. No runner-only or mode-specific
vector setter exists.

At each pre-instruction checkpoint, qualified peripheral requests are sampled
in the fixed PRT0, PRT1, DMA0, DMA1, CSI/O, ASCI0, ASCI1 order. The device
owners, rather than the interrupt controller, set and clear those level
requests from their documented flags and enable bits. A selected request
therefore remains eligible after acknowledge until guest-visible device state
clears it. Pairwise integration tests drive and clear every adjacent pair
through those real owners before each 18-cycle internal acknowledge; no test
or runtime path injects a different arbitration timing rule.

### Programmable reload timers

PRT0 and PRT1 share a system-clock-divided-by-20 phase accumulator. At every
20 elapsed phi cycles, each enabled channel advances once. A nonzero TMDR
decrements; the transition from one to zero sets TIF and leaves zero visible
for that interval; the following timer tick reloads TMDR from RLDR. A channel
raises its internal interrupt request only while both its TIF and TIE bits are
set.

Timer time is charged by the same `finish_step` boundary that updates
`cycle_count()`. Consequently, a TIF raised by an instruction or HALT idle
step is eligible at the following instruction-boundary interrupt checkpoint.
This preserves the documented elapsed-cycle behavior without exposing
mid-instruction bus phases.

### Free-running counter

FRC decrements once per ten elapsed phi cycles and wraps from `00h` to `FFh`.
It uses its own divide-by-ten phase, advances at the same `finish_step`
boundary as PRT and `cycle_count()`, and continues while ICR selects I/O STOP.
Reads do not change either its count or divider phase. RESET restores `FFh`
and restarts the divider phase.

### Asynchronous serial channels

ASCI0 and ASCI1 schedule complete byte frames at the common `finish_step`
boundary. For the standard internal baud-rate generator, one bit consumes

```text
(10 + 20 * PS) * 2^SS * clock_mode
```

phi cycles, where `PS` is zero or one and `clock_mode` is 16 or 64. A frame
contains one start bit, seven or eight data bits, a parity/multiprocessor bit
when selected, and one or two stop bits. Receive RDRF and transmit completion
therefore become observable only after `frame_bits * bit_cycles` elapsed phi
cycles. Divider selection is captured when a byte enters its shift stage, so
later register writes affect the next frame rather than retiming a frame in
progress.

In Z8S180 BRG mode, one bit instead consumes
`2 * (ASTC + 2) * clock_mode` phi cycles. X1 selects a one-phi bit clock in
place of `/16` or `/64`. Standard-divisor tests pin 8N1 `/10,/16,SS=0` at
1600 cycles, 8-parity-2-stop `/30,/16,SS=2` at 23040 cycles, and 7N1
`/10,/64,SS=1` at 11520 cycles; an ASTC=3, X1, 8N1 frame is pinned at 100
cycles.

SS=7 selects an external CKA clock. The fixed Phase 1 host API has no CKA
edge input, so such a byte may occupy the hardware shift stage but system-phi
progress cannot complete it. No external frequency is inferred and no
substitute clock API is introduced.

### Clocked serial port

CSI/O uses one unbuffered 8-bit shift operation in either transmit or receive
mode. For internal clock selections SS=0 through 6, one bit consumes
`20 << SS` phi cycles and the byte completes after `160 << SS` cycles. The
seven pinned byte totals are therefore 160, 320, 640, 1280, 2560, 5120, and
10240 phi cycles. Completion, TE/RE clearing, EF setting, and queue/TRD
visibility occur together at the common `finish_step` boundary.

The divider is captured when TE starts transmission or when the host supplies
a byte to an enabled receiver. Later speed writes do not retime that active
byte; the UM requires software to disable both directions before changing
baud rate. SS=7 selects an external CKS input. The fixed Phase 1 API has no
CKS edge input, so an external-clock transfer remains in its shift stage and
does not advance from system phi cycles. No external clock is inferred.

### Save-state boundary

With the optional `state` feature, `save_state()` serializes a version byte
followed by a postcard payload containing all core-owned emulation state. That
includes registers, mapped memory topology and contents, cycle and
instruction-boundary state, pin and request latches, peripheral divider and
shift state, host-facing serial queues, and pending events. `load_state()`
decodes the complete payload and validates the exact internal-register-file
length before mutating the machine, then recomputes the MMU page cache because
it is derived from the restored internal registers.

The generic `HostBus` and any external device or External-region contents it
owns are outside this snapshot boundary. A host that uses those surfaces must
checkpoint and restore them alongside the returned core bytes. Repeated saves
of the same core state are byte-identical, and resuming from a snapshot uses
the same instruction-level scheduling boundaries documented above.

The Phase 6 determinism authority runs two separately constructed machines
for exactly 10,000,000 phi cycles each. Both execute the same PRT0 reload
loop, ASCI0 transmit and receive frames, and a 256-byte DMA0 cycle-steal copy.
A single real undefined DD instruction produces a nonempty TRAP event stream,
then execution enters HALT so the two-wait memory schedule reaches the exact
cycle boundary. The DMA physical pages stay outside the CPU's identity-mapped
logical 64 KiB, preventing transfer data from becoming accidental code. The
test requires byte-identical state payloads and identical drained events.

## Intentional approximations

### Bus phases

The core does not expose T-state bus phases, WAIT sampling edges, refresh bus
waveforms, or mid-machine-cycle arbitration. Modeling those signals would add
hot-path state and callbacks that the target firmware does not require. The
observable contract is elapsed phi cycles, access order, and peripheral state
at instruction boundaries.

### Dynamic RAM refresh

For v0.1, refresh consumes zero modeled cycles. The later RCR implementation
will preserve the documented register behavior and refresh schedule, but it
will not subtract refresh bus cycles from CPU or DMA progress. This keeps the
instruction clock deterministic and avoids bus-phase simulation. Phase 6 must
verify that the BNS firmware does not depend on refresh stealing execution
time before this approximation is accepted as final.

### DMA interleave granularity

DMA timing granularity is one transferred byte. Each byte consists of a
three-clock read bus cycle and a three-clock write bus cycle. A memory access
adds the DCNTL MWI value (zero through three); a true I/O access adds the DCNTL
IWI total (one through four, including the mandatory I/O wait). The resulting
costs are `6 + 2*MWI` for memory-to-memory and `6 + MWI + IWI` for
memory-to-I/O. Memory-mapped I/O uses memory timing. When a DMA address carry
or borrow crosses A15/A16, a zero-wait affected memory cycle receives the
manual's internal Ti state so its minimum length is four clocks.

The v0.1 scheduler orders those byte units on the instruction-level CPU
timeline rather than arbitrating individual T states. DMA0 burst mode drains
the enabled memory-to-memory count before the next CPU instruction. Cycle
steal performs one byte before each instruction. An edge-sensed DREQ permits
one byte; a level-sensed DREQ continues while logically asserted. Simultaneous
ready channels select DMA0, and an active DMA0 memory-to-memory operation keeps
DMA1 from running until DMA0 terminates. DMA elapsed time advances FRC, PRT,
ASCI, and CSI/O through the same `finish_step` boundary before CPU interrupt
sampling and fetch; `step()` returns the combined DMA and CPU/interrupt cycles
while still executing at most one CPU instruction.

The public `set_dreq(ch, true)` value means logical assertion, translating the
UM's electrically active-low DREQ pins in the same way as the external
interrupt APIs. NMI assertion clears DME before the next DMA byte. Because the
model has no bus-phase callback, an NMI cannot suspend between the read and
write halves of a byte; the byte boundary is the documented v0.1 suspension
point. This preserves transfer order and elapsed-cycle effects while keeping
`step()` atomic and the hot path free of bus-waveform machinery.

## Clock variants

All accounting remains in phi cycles, including Z8S180 operation. Clock-mode
registers may affect peripheral scheduling when their owning phases land, but
they do not change the unit returned by `step()`, `run()`, or `cycle_count()`.
