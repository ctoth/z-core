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

DMA is a Phase 6 feature. Its v0.1 timing granularity is one transferred byte:
each byte will consume the UM0050 transfer cost plus the applicable DCNTL
waits. Cycle-steal and burst modes will order those byte units according to
their documented mode, interleaving them with the instruction-level CPU
timeline rather than arbitrating individual T states. This preserves transfer
order and elapsed-cycle effects while keeping `step()` atomic and the hot path
free of bus-waveform machinery.

## Clock variants

All accounting remains in phi cycles, including Z8S180 operation. Clock-mode
registers may affect peripheral scheduling when their owning phases land, but
they do not change the unit returned by `step()`, `run()`, or `cycle_count()`.
