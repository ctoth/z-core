# z180 C ABI

## Immutable callback context

`z180_create_with_context` accepts a `z180_context_bus` instead of `z180_bus`.
Each memory/I/O callback receives a borrowed `const z180_bus_context *`.
Copy its values if needed after the callback returns. The usual callback
lifetime, success/output, error and non-reentry contracts still apply.

`cycles` is the core's accumulated, already charged cycle count before that
access. All accesses within one instruction or DMA service share that timing
granularity; this is not hardware bus-cycle timing. DMA charged before CPU
execution in the same step is included in the later CPU callbacks.
`pc` is the logical instruction-entry PC for CPU accesses, the interrupted PC
for interrupt-entry accesses, and the pending CPU PC during DMA. HALT uses the
pending PC. Reset preserves elapsed cycles and resets the PC to zero. Context
is supplied for the same callbacks as the legacy interface, including the
existing notifications for internal I/O. Missing callbacks retain the usual
unmapped-read/ignored-write behavior. ABI-1 types and entry points are unchanged.

## Opcode-fetch observation

Register a `z180_fetch_observer` with `z180_set_fetch_observer`. NULL unregisters;
registration is copied and survives reset/load. Callback code/user_data must live
until replacement, unregistration or destruction. No same-machine reentry or
unwinding is allowed. The observer cannot fail; normal bus errors are unchanged.

The observer runs immediately before each existing instruction opcode read, with
its physical address after MMU and external mapping, including core-owned RAM/ROM.
It adds no reads. Prefixes and subsequent opcode bytes are reported; immediate
operands, indexed displacement and data reads are excluded. DD/FD CB d op reports
DD/FD, CB and op, but not d. This is instruction classification, not the M1 pin
or R counter. A notification describes an attempted fetch even if its read fails;
no later notification is emitted after that failure in the step.

HALT's instruction byte is reported once; subsequent dummy reads at the next PC
are excluded because they execute no instruction. SLP reports its two opcode
bytes; sleeping emits no reads. Interrupt acknowledge, stack/vector accesses,
NMI's discarded read and DMA are excluded. The handler's first opcode is reported
when it executes in the next step. The core's fixed INT0 mode-0 restart does not
read an acknowledge opcode through memory, so no synthetic fetch is emitted.
These distinctions follow UM0050 pp.32-33, 70-75 and Tables 48-50.
Existing ABI-1 bus layouts, signatures and memory-read order/count are unchanged.

A C99 interface to the Z80180/Z8S180 emulator, also usable from C++11 and
foreign-language bindings. The public header is [include/z180.h](include/z180.h).
ABI major version 1 exposes the CPU, board callbacks, memory mapping, serial
peripherals, interrupts, tracing, watches, disassembly, and versioned save states.

The implementation calls `z180-core` directly. Guest memory stays in the core;
unsafe pointer handling lives in this crate. The replay/timeline API remains in
the separate `z180-replay` crate.

## Build and use

From the repository root:

```text
cargo build --release -p z180-c
```

Distribute `include/z180.h` with the target platform's library:

| Platform | Shared library | Shared import library | Static library |
| --- | --- | --- | --- |
| Linux | libz180.so | none | libz180.a |
| Windows MSVC | z180.dll | z180.dll.lib | z180.lib |

These appear in `target/release`. Shared consumers need the shared library on
the loader's search path. Static consumers define `Z180_STATIC` before including
the header and link the Rust standard library's native dependencies. The
[consumer test](tests/consumer.py) contains working Windows/Linux link commands.
`Z180_BUILD` selects the export declaration when needed by a producer; consumers
normally leave it undefined.

Each `v*` release also publishes prebuilt static libraries, so C hosts need no
Rust toolchain: `z180-c-VERSION-x86_64-linux-gnu.tar.gz`,
`z180-c-VERSION-x86_64-windows-gnu.tar.gz` (MinGW) and
`z180-c-VERSION-universal-apple-darwin.tar.gz` (arm64 and x86_64, macOS 11+),
with `SHA256SUMS`. Each holds `lib/libz180.a`, `include/z180.h`, the licences,
and `native-static-libs.txt`: the flags to link after the archive. Define
`Z180_STATIC`.

[examples/board.c](examples/board.c) is a complete program: configure ROM and
RAM, create a machine, step, inspect PC, and destroy it. On Linux:

```text
cc -std=c99 -Icrates/z180-c/include crates/z180-c/examples/board.c -Ltarget/release -lz180 -o board
LD_LIBRARY_PATH=target/release ./board
```

Always check returned status before using output values. The optional
`z180_error *` argument supplies a numeric status, the original callback error
code, and a NUL-terminated UTF-8 diagnostic. It is cleared on success. Numeric
status constants are stable; diagnostic wording is not a parsing interface.

## Ownership and pointer rules

- `z180_create` returns one owned opaque machine. `z180_destroy` frees it;
  `z180_destroy(NULL)` is valid. A destroyed handle cannot be used again.
- Creation copies configuration, region descriptors, ROM bytes, and the callback
  table. `remap` also copies ROM; `set_ext_map_table` copies its table. Input
  buffers can be released when these calls return.
- RAM, ROM, debug rings, and state buffers never expose internal Rust pointers.
  RAM read/write functions copy between a caller buffer and one RAM region.
- Every non-null pointer must designate a live, aligned allocation of the stated
  size. Output storage may be uninitialized. Inputs, outputs, error records, and
  the machine must not overlap. A null buffer is accepted with length zero;
  required scalar output pointers cannot be null. Shape checks do not make
  dangling, forged, or undersized pointers safe.
- `user_data` remains host-owned. Its allocation and callback code must remain
  valid until machine destruction. Destroy does not call or free user_data.

Memory regions use 4 KiB aligned bases and sizes; physical width is 20..24 bits.
ROM length must equal region size; other region kinds require zero data length.
Creation rejects overlapping regions. `remap` replaces the selected range.
Named integer constants identify variants, registers, region kinds, watch kinds,
interrupts, and event kinds. Unknown identifiers are rejected. Boolean parameters
accept only 0 and 1. ASCI and DREQ channels are 0..1; DCD applies to channel zero.

## Board callbacks and execution

Callbacks run synchronously on the thread calling `step` or `run`. A callback
returns zero for success, or any nonzero `int32_t` host error. Successful read
callbacks must initialize their out-byte. The ABI returns `Z180_HOST_ERROR` and
preserves the callback's nonzero code in `error.host_code`.

Missing read callbacks return the configured unmapped byte; missing writes do
nothing. The normal fetch/operand path uses core-owned RAM/ROM. External memory
regions use memory callbacks. Internal I/O also produces the core's duplicate
external I/O bus cycle; its callback is observable and can fail.

`run` accepts a cycle budget and completes steps at instruction boundaries, so
it can overshoot. Zero budget advances nothing. `step` and `run` report the
change in the core's saturating cycle counter on success and on host failure,
including charged DMA progress. At counter saturation that delta may be zero.
A host failure does **not** roll back prior external effects or all machine
state; do not automatically retry it as though nothing happened. `reset`
preserves the core's cycle counter and memory.

One call at a time is allowed per machine, including getters and destroy.
Callbacks cannot reenter their machine, destroy it, throw, or longjmp through
Rust. Different machines can be used independently; the host synchronizes shared
callback context. No library background thread or global last-error slot exists.

Unwinding Rust panics are caught and return `Z180_INTERNAL_ERROR`. A machine
interrupted by a panic becomes poisoned: only destruction remains supported;
other operations return `Z180_POISONED`. Ordinary callback errors do not poison
it. Invalid foreign pointers, foreign unwinding, allocation aborts, and a build
configured with `panic=abort` are not recoverable through this mechanism.

## Buffers and observability

Ordinary outputs are unchanged on failure, with these explicit exceptions:

| Operation | Failure output |
| --- | --- |
| create | Valid out-machine is set to NULL before construction |
| step/run | Charged cycle delta is written on host error |
| RAM enumeration, event/trace drains | Required count is written on a short buffer |
| save_state | Written length is zero; storage is unchanged |
| disassemble | Required text capacity, including NUL, is written on a short buffer |

The optional error record is always updated when valid. Invalid error-record
alignment returns `Z180_INVALID_ARGUMENT` without touching other outputs.

Event and trace drains are all-or-nothing. Query with NULL/zero capacity, allocate
the reported count, then drain. A short buffer consumes nothing. An empty queue
returns success/count zero. `events_lost` is sticky until explicitly cleared;
draining does not clear it. Event fields unused by their tag are zero. Structure
padding has no defined value and must not be serialized or compared.

`set_insn_trace` preserves the core's allocation behavior: if it cannot reserve
the requested storage, the previous capacity remains. Disabling clears the
ring. Event rings may drop old entries; inspect `events_lost` when completeness
matters.

`mem_peek` inspects core-owned physical memory without callbacks. For external
or unmapped addresses it returns the unmapped byte. `mem_poke` writes RAM only,
without bus callbacks or watch events. Bulk RAM operations validate the full
range before copying. `mmu_translate` returns the MMU address before optional
external mapping. The external table has exactly 2^20 entries; NULL/zero clears
it. Internal-I/O inspection accepts register indices 0..63, without read effects.

Serial RX returning accepted=0 means the receiver cannot accept that byte now;
it is not an ABI error. Empty TX returns present=0 and value=0. Watch IDs are
nonzero and survive save/load; removing an absent nonzero ID is a no-op.

`save_state_size` computes the exact current size using temporary serialization.
Call `save_state` with your buffer while keeping the machine unchanged between
calls. The payload uses the core's existing state format; ABI version and state
version are independent. Invalid state is rejected without replacing machine
state. Loading retains the host callbacks and external mapper, which the payload
does not serialize.

Disassembly returns one instruction or the core's data-byte fallback for a
truncated/unsupported encoding. Empty input is invalid. Its text is UTF-8 with
a trailing NUL; query text capacity with NULL/zero. No partial instruction record
is published on a short text buffer.

## Compatibility and maintenance

`z180_abi_version()` returns 1. Record layouts, constant values, and signatures
are frozen for this ABI major within a platform ABI. New functions can be added;
incompatible records require new type/function names or a major change. Layouts
are naturally aligned, not packed; sizes need not match across architectures.

The Rust ABI declarations are the single source for the distributed header.
Regenerate using pinned cbindgen 0.29.4, and review the resulting header:

```text
cargo install cbindgen --version 0.29.4 --locked
cbindgen --config crates/z180-c/cbindgen.toml --crate z180-c --output crates/z180-c/include/z180.h
```

The existing Windows/Linux PR CI checks header regeneration, Rust tests, and
the independent C/C++ consumer in both static/shared modes. The focused tasks
are `cargo test -p z180-c --lib tests::` and
`python crates/z180-c/tests/consumer.py`. The latter also runs the board example.
