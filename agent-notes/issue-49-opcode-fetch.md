# Issue 49: synchronous opcode-fetch classification

## Goal

Deliver https://github.com/ctoth/z-core/issues/49 as its own independent PR
against master. Preserve ABI-1 and the order and count of normal memory reads.
No merge was requested.

## State

Implementation complete; publishing its independent PR. Worktree:
`C:/Users/Q/code/z-core-issue-49`, branch `issue-49-opcode-fetch`, from origin/master.
Q authorized implementation/publication with "Make it so!".

## Problem and source

`crates/z180-core/src/lib.rs:488-516` reads opcode/prefix bytes and indexed
displacements through the same read_logical path. `access.rs:4-12` translates
addresses and sends reads through the same memory dispatch; C HostBus callbacks
in `crates/z180-c/src/bus.rs` lose fetch identity. After-execution instruction
traces cannot support synchronous fetch-address tracking.

## Decisions and why

1. Add a default no-op safe HostBus fetch observation operation invoked by the
   core's decoder at an actual opcode read, with the final physical address.
   It observes the existing access; it does not perform a second memory read.
2. Expose it to C through an additive observer registration function and separate
   observer type. Do not change existing z180_bus, z180_create or callbacks.
   The observer may record host state but may not reenter the machine.
3. Classify actual first/prefix/subsequent opcode reads at their source. Indexed
   displacement and operand/data reads are not instruction opcode fetches.
   Do not infer classification from PC proximity, R increments, or a second
   decoder. The final indexed opcode must be handled explicitly.
4. Document HALT dummy fetches separately from executed instruction fetches;
   SLP idle has no opcode fetch. Interrupt stack/vector accesses and DMA are
   not instruction opcode fetches. No synthetic callback for an interrupt
   acknowledge byte that the existing core does not read from memory.
5. Keep this PR independent of #48; no copying of context-interface code.
6. Parnas (1972), all six images read through paper-reader: decoder owns
   classification and CBus owns foreign callback conversion.

## Manufacturer evidence read as page images

Zilog UM005004-0918 (`docs/vendor/um0050.pdf`), scoped pages 31-33 and 70-72,
rendered with paper-reader's render_pages.py. Local page images are outside
the worktrees at C:/Users/Q/code/z-core-manual-read.

- p.32 calls HALT bus activity repeated dummy fetches of the following opcode;
  p.33 Figure 20 distinguishes HALT instruction fetch from HALT mode.
- p.33 says SLEEP stops the CPU clock and deactivates bus control outputs.
- pp.70-72 and Figures 32-33 distinguish second/third opcode fetch, data read
  at IX/IY+d, and stack writes. Classification must not equate fetch with the
  existing core's R-increment count.
- During implementation cite the applicable opcode-map and interrupt-timing
  pages after direct image inspection; no MAME source or oracle as authority.

## Implementation and verification plan

- Factor the existing logical-read path only enough to mark opcode accesses
  while preserving translation, memory reads, waits, capture and failures.
- Add observer conversion/registration, checked-in C header declarations and
  README classification/lifetime/non-reentry documentation.
- Synthetic Rust and C tests: ordinary immediate and data reads; CB, ED, DD,
  FD, DDCB/FDCB including displacement; physical address after translation;
  HALT, SLP, NMI and vectored entry; DMA; callback order/count; legacy hosts.
- Record directly verified manual references in docs/verification-log.md.
- Inspect the final diff; run only authored test files locally; commit this note
  with source, push, open one PR closing #49, monitor and repair existing CI.

## Surprises

The final DDCB/FDCB opcode is read separately from its displacement, while the
current R increment count remains two. That count cannot classify fetches.

## Next step

Publish and monitor PR CI. Authored checks passed:
`cargo test -p z180-core --lib tests::fetch` and
`python crates/z180-c/tests/consumer.py` (C/C++, static/shared).
Also inspected manufacturer page images pp.73-75, 90-91, 247-250. DMA transfers
are data movement without CPU intervention (pp.90-91), not decoded opcodes.
The C consumer caught observer initialization in MachineConfig instead of CBus;
it was moved to its owning CBus initializer and the consumer then passed.
