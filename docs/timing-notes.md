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
