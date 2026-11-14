# C ABI

## Goal

Design and implement a public C ABI for z180-core: useful to a board emulator,
straightforward to bind from other languages, and independent of Rust layout.

## State

Design prepared on 2026-11-14; Q authorized implementation with "Make it so!".
Working on feat/c-abi. The separate z180-c crate, all planned API families,
generated C header, and focused Rust boundary tests are implemented locally.
Core additions are queue counts and safe watch-ID conversion. Implementation,
generated header, C example, documentation, and existing Windows/Linux CI
integration are implemented and published in
[PR #47](https://github.com/ctoth/z-core/pull/47). Remote CI is running.

Verified locally with the explicitly authored test tasks:
- `cargo test -p z180-c --lib tests::`
- `python crates/z180-c/tests/consumer.py`

The consumer passed on Windows as C99/C++11 with static and shared linking,
including actual callbacks, prior effects on host failure, ROM copying, independent
machines, debug drain sizing, serial RX/TX, event loss, state buffer sizing,
state-restored watches, callback/mapper preservation, and disassembly. The board
example ran with PC=1 and cycles=6. Header regeneration matched with cbindgen
`--verify`; `git diff --check` was clean. No Linux execution or full PR CI has
completed yet; do not claim those checks passed.

Read: workspace Cargo.toml and README; core README and Cargo.toml;
core lib.rs, access.rs, debug.rs, memory.rs, registers.rs, state.rs;
disassembler.rs entry point and public interrupt, DMA, ASCI and CSI/O methods;
Python bus.rs; replay lib.rs;
existing .github/workflows/ci.yml. No tracked repository AGENTS.md or CLAUDE.md
was found. Worktree was clean on master at inspection.

Research: Parnas, *On the Criteria To Be Used in Decomposing Systems into
Modules*, all six page images, via paper-reader. See
[reading notes](../papers/Parnas_1972_Decomposing_Systems/notes.md).
Read the [Rustonomicon FFI documentation](https://doc.rust-lang.org/nomicon/ffi.html),
particularly representation, nullable callbacks, opaque types and unwinding.

## Decisions and why

### Boundary and deliverables

Add `crates/z180-c`, depending directly on z180-core with `state` enabled.
Build `cdylib`, `staticlib`, and `rlib` (the latter for binding tests).
The core remains no_std and unsafe-free. All pointer conversion and callback
invocation belongs in the binding crate; CPU behavior remains in the core.
Deliver a C99 header usable from C++, a README, and a runnable C board example.

Scope is the emulator core, including peripherals, memory mapping, debugging,
disassembly, and save states. Replay is a separate crate and API; this change
does not expose its recording/timeline system or rewire Python/Wasm through C.
That boundary keeps one emulator implementation and avoids coupling the native
ABI to replay's recording schema.

### ABI representation and evolution

Use the `z180_` symbol prefix and the platform C calling convention everywhere,
including callbacks. Provide `z180_abi_version()` returning major version 1.
Use an opaque `z180_machine` pointer. Public records use repr(C), natural
alignment, fixed-width integer fields, and pointer-plus-size_t buffer pairs.
Pass records by pointer, not by value. Avoid packed structs, bitfields, C enum
parameters, Rust bool, Rust enum layouts, strings requiring the Rust allocator,
and pointers into internal RAM or queues.

Use uint32_t identifiers with named constants for registers, variants, region
kinds, event kinds, watch kinds, and interrupt lines. Assign values explicitly;
never derive them from Rust discriminants. Boolean inputs are uint8_t, accepting
only 0 or 1. Unknown identifiers are errors before touching the machine.

Freeze v1 record layouts and function signatures. Add new functions for later
capabilities; incompatible records get new type/function names or an ABI-major
change. Do not append fields silently, reserve speculative padding, or introduce
a function-table negotiation protocol. Compatibility is within a platform ABI,
not between architectures. Save-state versioning remains separate.

Keep ABI type/function declarations in one Rust source owner and generate the
distributed header with cbindgen. Generated output is not hand-maintained code.
Pin the generator after reading its current primary documentation; CI checks
that regeneration matches the distributed header. Include export/import macros
for Windows shared builds, a static-link option, and C++ extern-C declarations.

### Ownership and creation

`z180_create(config, regions, region_count, bus, out_machine, error)` returns a
status and clears a valid out_machine before doing work. Configuration fields
match MachineConfig: clock_hz, phys_addr_bits, unmapped_read, variant,
event_capacity. `z180_config_init(out_config)` supplies core defaults.
Regions contain base, size, kind, and ROM data pointer/length. Copy the input
configuration, descriptors, ROM bytes, and callback table during creation.
RAM is zeroed and owned by the core. No caller allocation changes ownership.

The optional bus contains user_data and the four HostBus callbacks. The caller
keeps user_data and callback code alive until destroy returns. No user-data
destructor callback: the host owns its context and releases it after destroy.
Null bus/callback entries mean reads return configured unmapped_read and writes
do nothing, matching the existing Python bus. `z180_destroy(NULL)` is harmless;
destroy invalidates the handle and all copies of it.

ROM length must equal the region size; non-ROM descriptors require zero length.
Use the core's configuration validation for alignment, overlap, and range.
Validate pointer/count multiplication, isize::MAX bounds, and nullability before
forming Rust slices. A null buffer is valid only with zero length unless a
function explicitly documents otherwise. Non-null pointers must designate
properly aligned live allocations of the stated size; arbitrary addresses and
stale handles cannot be validated by an ordinary C ABI.

### Calls, errors, callbacks, and panic behavior

Fallible operations return int32_t status. Proposed fixed values: OK=0,
INVALID_ARGUMENT=1, INVALID_CONFIG=2, HOST_ERROR=3, BUFFER_TOO_SMALL=4,
INVALID_STATE=5, INTERNAL_ERROR=6, POISONED=7. Empty serial queues and rejected
serial input are ordinary successful results with a present/accepted byte.

An optional caller-owned error record has status, host_code (int32_t), and a
fixed 256-byte NUL-terminated UTF-8 message. Truncate on a UTF-8 boundary.
Clear it on success; numeric codes are authoritative. No global or thread-local
last-error state. Creation failures can therefore explain themselves without
a machine handle. Ordinary outputs remain untouched on failure, except the
explicit creation, execution-progress, and save-buffer contracts below.

Each bus callback returns int32_t: zero is success, every other value is a host
error preserved in host_code. Read callbacks receive an out-byte pointer and
must set it on success. The wrapper initializes that byte defensively. Callback
execution is synchronous on the invoking thread. Internal I/O still produces
the existing external bus cycle; do not filter it out.

One caller at a time per handle, including getters and destroy. Host callbacks
must not call back into that same handle, destroy it, throw, or longjmp through
Rust. Distinct machines may be used independently; the host synchronizes any
shared callback context. This is a documented caller contract, not a global
registry, lock, background worker, or dispatch queue.

Catch unwinding Rust panics inside exported fallible entry points. A panic
after entering a machine operation poisons that handle; subsequent operations
return POISONED, with destroy still permitted. Do not claim an interrupted
mutation is safe to resume. Failed construction publishes no handle. Allocation
abort, invalid foreign pointers, foreign unwinding, and panic=abort cannot be
converted into recoverable statuses; document those limits. Normal callback
errors do not poison a handle.

### Public operation families

All names below have the z180_ prefix. Scalars use fixed-width types; buffer
lengths use size_t. Machine operations take the handle first and optional error
last. Output arguments precede error.

| Family | Operations and result |
| --- | --- |
| Lifecycle | abi_version, config_init, create, destroy, reset |
| Execution | step(out_cycles), run(uint32_t budget, out_cycles), get_status(out_record) |
| Registers | reg_get(id, out_u16), reg_set(id, u16); all 13 Reg values |
| Interrupt state | get/set IFF1 and IFF2; get/set interrupt mode 0..2 |
| Board signals | set_irq(line, level), set_nmi(level), set_dreq(channel, level), set_asci_cts(channel, level), set_asci_dcd(level) |
| Serial | asci_rx_push(channel, byte, out_accepted), asci_tx_pop(channel, out_present, out_byte); equivalent CSI/O calls without channel |
| Memory | mem_peek(physical, out_byte), mem_poke(physical, byte), RAM bulk read/write by region base and offset, RAM region enumeration, remap(region) |
| Mapping/I/O inspection | mmu_translate(logical, out_physical), io_reg_peek(index, out_byte), set_ext_map_table(data, count) |
| Watches | mem_watch_add(base, size, kind, out_id), mem_watch_remove(id), set_pc_watch(enabled, address), pc_watch_hits(out_u64) |
| Debug capture | set_io_trace, set_irq_trace, set_insn_trace(enabled, capacity), drain_events, drain_insn_trace, events_lost, clear_events_lost |
| State | save_state_size(out_size), save_state(buffer, capacity, out_written), load_state(bytes, length) |
| Decode | disassemble(bytes, length, logical_address, out_record, text, capacity, out_required), is_instruction_implemented(bytes, length, out_bool) |

get_status returns total cycles, instruction PC, halted, sleeping, IFF1, IFF2,
interrupt mode, and ITC in a frozen record. ASCI and DREQ channels are 0..1;
DCD has no channel parameter because only channel zero is modeled. Internal I/O
indices are 0..63. Inspection/poke retains existing side-effect-free behavior:
poke only writes RAM; peek does not call the host.

RAM bulk operations copy within one region, validate the complete range before
writing, and return no internal pointers. External mapping takes the existing
complete 2^20-entry table and copies it; null plus zero clears it. Do not expose
the Rust fn-pointer mapper, which has no per-instance context; the table already
represents every mapping over this finite domain.

step and run use the core's fallible execution. run keeps its uint32_t budget
and instruction-boundary overshoot behavior. Use the core cycle-count delta for
out_cycles on both success and HOST_ERROR, reporting charged progress, including
DMA progress. At u64 saturation the delta cannot represent further cycles;
document that it is the change in the saturating counter. Do not promise rollback
or safe automatic retry after a host error: prior bus effects can persist.

Event records use an explicit kind and a fixed flat payload containing cycle,
PC, physical address, port, value, interrupt source/vector, and trap opcode/length;
zero unused fields. Trace records contain cycle, PC, physical PC, bytes, length.
Drain APIs accept caller storage and write count. If capacity is insufficient,
report required count and drain nothing. Querying with null/zero never consumes.
Implement count accessors in the existing core debug owner before calling its
full-drain iterator. Do not add a second queue in the binding.

Watch IDs need explicit core accessors for their opaque numeric representation,
with zero rejected on input; do not transmute WatchId or keep a second ID map.
The core remains the ID owner, including IDs restored in save states.

save_state_size uses the existing fallible serialization into a temporary Vec;
it reports the exact current size, without caching a second state. save_state
delegates to save_state_to_slice, writes out_written=0 on failure, and permits a
partial buffer on BUFFER_TOO_SMALL, as the core does. Callers serialize between
size and write, so state cannot change unexpectedly. load_state retains the host
bus and mapper. Decode failure leaves emulated state unchanged.

Disassembly text is UTF-8 plus NUL; required length includes the terminator.
Insufficient space returns BUFFER_TOO_SMALL and the required size without a
partial instruction result. Empty input is INVALID_ARGUMENT. Truncated or
unsupported encodings retain the core's data-byte disassembly fallback
(disassembler.rs:49). Delegate formatting and opcode coverage to the existing core.

### Implementation order and validation

1. Establish ABI declarations, generated header, create/destroy, config conversion,
   callback adapter, status/error conversion, and execution. Add the C example
   that maps ROM/RAM, steps an instruction, observes PC/cycles, and destroys it.
2. Complete the operation families above; make only the narrow core additions
   for queue counts and watch-ID conversion. Document each ownership/lifetime,
   failure-output, and side-effect contract in the public declarations.
3. Add focused Rust boundary tests and an independently compiled C consumer;
   compile the header from C++ too. Exercise real C callbacks and outputs, not
   just Rust calls to exported functions. Check invalid identifiers, NULL/zero
   buffers, ROM copying, caller-context lifetime, two independent machines,
   callback failure after prior effects, panic poisoning, non-consuming short
   drains, serial empty/rejected input, exact/short save buffers, invalid state,
   restored watches, and disassembly sizing. Test both static and dynamic linking.
4. Extend the existing Windows/Linux PR CI jobs with header generation checking,
   C/C++ compilation/linking/execution, and the new crate's tests. Preserve all
   existing checks. CI runs on the PR; only a test file being authored may run
   locally under the supplied instructions. No broad local CI run.

Implementation paths: workspace Cargo.toml/Cargo.lock; crates/z180-c/Cargo.toml,
src/lib.rs, src/types.rs, src/bus.rs, include/z180.h, cbindgen.toml, README.md,
tests and examples; core debug.rs/lib.rs for the two narrow access requirements;
root README; existing .github/workflows/ci.yml; this note. Split further only
when actual implementation size calls for it.

## Surprises

- First PR CI run found surplus blank lines at EOF in binding bus.rs, lib.rs,
  and types.rs. The formatter-output application had trimmed its comparison
  range and left the old trailing blank line outside the replaced range.
  Removed those exact trailing lines; retained the existing formatting check.

- lib.rs:579 restores registers after a handler bus failure, not every side effect.
- access.rs:103 and :193 contain the internal-I/O duplicate host accesses.
- debug.rs:58 and :95 drain the entire queue; dropping early loses the suffix.
- state.rs:134 deliberately allows partial output on a short save buffer.
- lib.rs:346 reset does not reset cycle_count; keep that behavior in C.
- WatchId's payload is private (lib.rs:95); safe conversion needs a core accessor.
- One read command used unsupported shell brace expansion; it failed at parsing
  before execution and was repeated using real paths. No source was changed.

## Next step

Q explicitly authorized direct Codex commit/push/PR publication with "I do"
in response to the permission question. Commit the substantive changes with
this note, push and open the PR in the same turn, then watch and repair remote CI.
Do not infer merge authority.
Do not stage paper PDFs or images. This note replaces the original c-abi.md so
there remains one living note for the task.

Paper PDF and six page images were identity-checked and retained locally under
papers/Parnas_1972_Decomposing_Systems. They are untracked reading artifacts and
must not be staged with the eventual implementation. Reconciliation found no
other collected papers. Final source check corrected the disassembly description
to preserve the core's fallback for nonempty truncated input.
