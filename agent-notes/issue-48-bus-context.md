# Issue 48: immutable callback context

## Goal

Deliver https://github.com/ctoth/z-core/issues/48 as its own PR against master.
Preserve ABI-1 layouts, callback signatures, error behavior and non-reentry.
No merge was requested.

## State

Implementation complete; publishing the independent PR. Worktree:
`C:/Users/Q/code/z-core-issue-48`, branch `issue-48-bus-context`, from origin/master.
Q authorized implementation and publication with "Make it so!".

## Problem and source

`crates/z180-c/src/types.rs:74` defines callbacks without cycle or PC context;
`crates/z180-c/src/bus.rs:15` forwards them without context. Same-machine
reentry is prohibited by the C contract. `crates/z180-core/src/lib.rs:450-455`
services and charges DMA before CPU accesses in that same step. A snapshot
before stepping therefore cannot supply the callback's current cycle count.
`access.rs` owns CPU access dispatch; `peripherals/dma.rs` separately owns DMA
I/O dispatch; `memory.rs:289,316` dispatches External memory callbacks.

## Decisions and why

1. Add a small immutable core context value and defaulted context-aware HostBus
   operations which delegate to existing operations for existing Rust hosts.
   Generate context in the safe core at the access site, including DMA I/O;
   thread memory context through the existing memory dispatch.
2. Add a separately named C callback interface and constructor. Leave `z180_bus`
   and `z180_create` intact. Both constructors use the same core and existing
   validation/error machinery. No instruction or board logic in the binding.
3. Cycles mean the core's accumulated, already charged cycles before the access;
   accesses within an instruction or DMA service share that existing timing
   granularity. CPU PC means instruction entry; interrupt entry and DMA use the
   interrupted/pending logical PC. Do not reuse stale instruction_pc for those
   phases. Reset preserves elapsed cycles and returns the PC context to reset PC.
4. Keep this PR independent of issue 49; no fetch-classification dependency.
5. Parnas (1972), all six page images read through paper-reader: keep execution
   decisions in the core and C representation/conversion in the binding. Existing
   notes are under papers/Parnas_1972_Decomposing_Systems in the main checkout.

## Implementation and verification plan

- Edit core HostBus declarations, access dispatch, memory dispatch, DMA dispatch
  and only the execution-context bookkeeping required to distinguish phases.
- Edit C types, CBus and creation, regenerate the checked-in header, document
  context semantics and lifetime in the C README.
- Add synthetic Rust and C regression cases for ordinary memory/I/O access,
  DMA then CPU in one step, interrupt entry after an earlier instruction,
  reset preserving cycles, callback failure, and legacy ABI behavior.
- Inspect changes against the issue; run only authored test files locally.
- Commit this note with the source, push, open one PR closing #48, and monitor
  its existing CI on both platforms. Repair red checks within scope.

## Surprises

DMA I/O bypasses CPU read_io/write_io; covering only access.rs would miss it.
The checkout supplied by Q has unrelated untracked review and paper artifacts;
they remain untouched in the original checkout.

## Next step

Publish and monitor PR CI. Local authored checks passed:
`cargo test -p z180-core --lib tests::bus_context` and
`python crates/z180-c/tests/consumer.py` (C/C++, static/shared).
The internal callback_pc is transient, refreshed before every step, and does not
change the existing instruction_pc getter or serialized state format.
