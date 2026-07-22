# z-core deep implementation review — 2026-07-22

Scope: full workspace at commit `ca3af18` (`P10.3: prepare v0.1.0 release`).
Reviewed: `z180-core` (CPU, MMU, memory, interrupts, PRT/FRC/ASCI/CSIO, DMA,
save-state, tracing, disassembler, optable), `z180-cli` (dis/run/zex/sst),
`z180-py`, `z180-wasm`. Method: manual read of the ALU/flags/decode/interrupt/
memory core plus four focused subsystem passes; every load-bearing finding was
re-verified against the source and, where relevant, against the project's own
`docs/verification-log.md` UM0050 citations.

**Overall:** a genuinely high-quality, careful emulator. `no_std` + `alloc`,
`#![forbid(unsafe_code)]` in the core, exhaustive in-file tests, and clean-room
UM0050 citations throughout. Flag computation (including the hard undocumented
INI/IND/OUTI/OUTD H/C/P-V behavior), the MMU closed-form translation,
save/load atomicity, and the interrupt priority/vectoring model are all correct
and well-tested. The issues below are concentrated in less-travelled paths
(TRAP stacking, fixed-address DMA, degenerate timer reload) and in test-harness
leniency. One is Critical.

---

## Critical

### C1. TRAP stacks the wrong return PC (contradicts the project's own UM0050 citation)
**`crates/z180-core/src/lib.rs:538` and `:540`** (in `step()`); wrong values
enshrined by tests at `:5512-5513` and `:5551-5552`.

`docs/verification-log.md:68` records the intended spec: *"For a second-opcode
undefined fetch, UFO is zero and the invalid instruction begins at stacked PC
minus one; for a third-opcode undefined fetch, UFO is one and it begins at
stacked PC minus two."* With the trapped instruction starting at
`S = instruction_pc`, that requires:

* UFO=0 (2-byte undefined, e.g. `ED xx`, `DD xx`, `CB xx`): stacked PC = `S+1`
* UFO=1 (`DDCB`/`FDCB` undefined 3rd opcode): stacked PC = `S+2`

The code instead passes:

* line 540: `pc.wrapping_add(2)` → `S+2` for UFO=0 — **off by +1**
* line 538: `pc.wrapping_add(4)` → `S+4` for UFO=1 — **off by +2**

It stacks `S + instruction_length` rather than the address that reflects the
undefined opcode.

**Failing scenario:** `ED 31` at `0x1234` (UFO=0). UM0050 (and the verification
log) require stacked PC `0x1235` so a handler computes `0x1235 - 1 = 0x1234`.
The emulator stacks `0x1236`; the handler computes `0x1235`, pointing into the
middle of the trapped instruction, and any `RET`/`RETI` resumes one byte too
high. For `DD CB 05 40` at `0x1234` (UFO=1): required `0x1236`, emulated
`0x1238` — two bytes high.

**Fix:** `pc.wrapping_add(2)` → `pc.wrapping_add(1)` (line 540) and
`pc.wrapping_add(4)` → `pc.wrapping_add(2)` (line 538). The R-register /
`m1_fetches` accounting is already consistent with the *correct* stacked PC, so
only the pushed value changes. Update the two tests (`0x36→0x35` at 5512;
`0x38→0x36` at 5551) — they currently lock in the defect.

---

## High

### H1. DMA0 fixed-address memory mode (SM/DM = 2) is misclassified as I/O
**`crates/z180-core/src/lib.rs:3256-3257`** (`service_dma`).

DMODE SM/DM encode `0`=incr-mem, `1`=decr-mem, `2`=**fixed-mem**, `3`=I/O.
`dma0_transfer_byte` correctly treats only mode `3` as I/O (`:3096`, `:3121`),
but `service_dma` classifies with `< 2` / `>= 2` thresholds, lumping
fixed-memory (mode 2) together with I/O:

```rust
let memory_to_memory = source_mode < 2 && destination_mode < 2;
let valid = !(source_mode >= 2 && destination_mode >= 2);
```

Consequences (DE0 set, count > 0):

* **SM=2 (fixed mem) → DM=0 (incr mem):** a legitimate mem→mem transfer, but
  `memory_to_memory=false`, `valid=true`, so it enters the DREQ-gated branch and
  **stalls forever** unless a spurious DREQ0 is asserted. (Reading a
  memory-mapped FIFO at a fixed source into a RAM buffer hangs.)
* **SM=2 ↔ DM=2:** `valid=false` → treated as illegal, **zero transfers**.
* **SM=2 (fixed mem) ↔ DM=3 (I/O)** and **SM=3 ↔ DM=2:** legal
  fixed-memory↔I/O, but `valid=false` → **never runs**.

Correct predicate: `memory_to_memory = source_mode != 3 && destination_mode != 3`;
the only illegal combination is `source_mode == 3 && destination_mode == 3`
(I/O→I/O). Existing tests only exercise SM/DM ∈ {0,1} and mode 3, so mode 2 is
entirely unverified.

---

## Medium

### M1. `ram_region`/`ram_region_mut` return `None` for any RAM region not based at physical 0
**`crates/z180-core/src/memory.rs:397-405`** (`ram_segment`).

```rust
if first_page != 0
    && self.pages[first_page - 1]
        == (Page::Ram { store, offset: offset.checked_sub(PAGE_SIZE as usize)? })
{ return None; }
```

A freshly-mapped region's first page always has store `offset == 0` (`:199`,
`:209-216`). When `offset == 0`, `checked_sub(PAGE_SIZE)` is `None` and the `?`
propagates that `None` out of the whole function — aborting the lookup instead
of merely making the boundary check false. So a single
`RegionDef { base: 0x4000, size: 0x1000, kind: Ram }` makes `ram_region(0x4000)`
return `None`, even though `0x4000` is exactly the region start. **Every RAM
region based at a non-zero physical address is inaccessible through this API** —
i.e. the standard "ROM low, RAM high" layout can't be loaded/inspected by the
host. The existing test only passes because it lands on a split store with
`offset == 0x2000`, never `offset == 0`. Fix: guard the check with
`if let Some(prev) = offset.checked_sub(PAGE_SIZE as usize)` instead of `?`.

### M2. SST comparator never detects spurious/extra memory writes
**`crates/z180-cli/src/sst.rs:831-841`** (`compare`).

The final-state RAM check only iterates `expected.final_state.ram` and verifies
each listed address matches; it never confirms memory *outside* that list is
unchanged. A CPU bug that writes to an address the reference model didn't touch
(wrong effective address, block-op off-by-one, stray push) passes silently. This
is exactly the class of leniency that can hide a real CPU defect. Fix: snapshot
all initial RAM and assert nothing changed except addresses in `final.ram`.

### M3. PRT stops interrupting when `RLDR = 0000h`
**`crates/z180-core/src/lib.rs:3066-3074`** (`advance_prt`).

TIF is set only on the `1 → 0` decrement transition; the reload path
(`count == 0`) never sets it. For `RLDR ≥ 1` this yields the correct
`RLDR+1` period, but with `RLDR = 0000h` the counter reloads `0` every tick and
takes the `count == 0` branch forever, so TIF asserts **at most once** then goes
permanently silent. UM0050 makes `RLDR=0` the fastest periodic rate. Fix: set
TIF in the reload branch (when reaching 0), which keeps `RLDR≥1` timing intact.
Untested.

### M4. Host-callback exceptions do not stop `run()`
**`crates/z180-py/src/lib.rs:50-56, 423-431`**;
**`crates/z180-wasm/src/lib.rs:56-61, 395-403`**; core loop
**`crates/z180-core/src/lib.rs:590-600`**.

`record_error` stores only the first error and the bus returns `unmapped_read`,
so the core keeps executing. Because `Z180::run` loops `step()` with no
callback-error awareness, a Python/JS exception on the first access of a large
`run(cycles)` call does **not** abort — the emulator runs the entire remaining
budget and fires every subsequent side-effecting `mem_write`/`io_write`
callback; the exception surfaces only after `run` returns, with machine state
far past the fault. A host callback cannot signal "stop." Consider checking the
error latch between steps in `run`.

### M5. `cargo test --workspace` fails on a fresh clone
**`crates/z180-cli/src/sst.rs:1030-1035`**.

The test `standard_sst_relocates_internal_window_away_from_expected_external_port`
hard-`expect()`s `../../tests/sst/v1/ed 48.json`, which lives in the
uninitialized `tests/sst` git submodule (`SingleStepTests/z80`). On a fresh
clone the workspace test suite fails with a panic — yet `README.md` instructs
`cargo test --workspace` without mentioning `git submodule update --init`. CI
passes because `ci.yml` sets `submodules: true`. Two fixes: (a) document the
submodule requirement in the README; (b) make this test skip gracefully (return
early / `#[ignore]`-with-note) when the fixture is absent, rather than panicking.

### M6. DMA physical accesses bypass the external address mapper that CPU accesses use
**`crates/z180-core/src/lib.rs:3099/3104, 3171/3175`** vs **`:2416/2426`**.

`read_logical`/`write_logical` route the post-MMU physical address through
`map_external_address` (ext-mapper function/table), but `dma0_transfer_byte`/
`dma1_transfer_byte` call `emulation_mem_read`/`emulation_mem_write` on the raw
20-bit address. If the ext-mapper models physical-bus banking (as its test
"apply after MMU translation" frames it), CPU and DMA see different physical
memory for the same address — inconsistent with a shared external bus. Related
to L5 (the ext-mapper is also excluded from save-state); the feature is
half-wired. Scoped as a design-consistency defect (not a UM0050 register).

---

## Low / informational

* **L1 — DDCB/FDCB operand fetch order reversed.**
  `lib.rs:499/512` read the sub-opcode at `pc+3` to index the table before the
  handler reads the displacement at `pc+2` (`:1822`), so the observable read
  sequence is `S, S+1, S+3, S+2` — opcode before displacement, the reverse of
  hardware. Register results are identical; only bus/watchpoint ordering and
  insn-trace byte order differ.

* **L2 — DAA test is circular.** `lib.rs:5981` `expected_daa` re-implements the
  exact algorithm of `execute_daa` (same correction table, same
  `(A ^ result) & 0x10` half-carry trick), so the "every combination" test
  (`:5572`) validates the code against itself, not against hardware. (The
  implementation itself appears correct — the correction table matches the
  standard DAA table for both N=0/N=1 and the XOR-based H is the accepted
  accurate method — but the test provides no independent oracle.)

* **L3 — Undocumented XY flags (bits 3/5) are not hardware-accurate and are
  masked in SST.** LDI/LDD (`lib.rs:2063`) preserve the previous XY bits instead
  of deriving them from `A + transferred_byte`; CPI/CPD (`:2085`) take XY from
  `A - value` without the `-H` adjustment. `FLAG_COMPARE_MASK = !0x28`
  (`sst.rs:16`) masks bits 3/5 out of every comparison, and generated cases are
  forbidden from setting them (`:393`). This is defensible — the SST corpus is
  NMOS-Z80-derived and the Z180 CMOS part's undocumented flags may differ — but
  it means "passes SingleStepTests" explicitly excludes XF/YF, and that scope
  limit should be stated in the docs.

* **L4 — PyO3 panicking borrows.** `z180-py/src/lib.rs:337, 562, 572-578` use
  `borrow_mut`/`borrow` (panic on conflict) rather than `try_borrow*`. Reentrant
  access (e.g. `ram()` from inside a `step()` callback, or a `memoryview` of an
  existing ram view) panics; PyO3's `catch_unwind` turns it into
  `PanicException` rather than a clean `PyBorrowError`. Not UB.

* **L5 — `ext_mapper` is not serialized in save-state.** `SavedState`
  (`lib.rs:252-308`) omits the external mapper; `load_state` leaves the target
  machine's existing mapper in place. `save_state()==save_state()` still holds,
  but restoring a snapshot onto a fresh machine loses external-bus wiring.
  Defensible as host config, but undocumented as excluded.

* **L6 — `save_state` swallows serialization failure.** `lib.rs:947-949`
  `if let Ok(payload) = postcard::to_allocvec(...)` returns just the version byte
  on error (which then fails `load_state`). postcard can't fail for these types
  in practice, but the failure is silent (the fn returns `Vec<u8>`, not
  `Result`).

* **L7 — `load_state` accepts out-of-range `interrupt_mode`.** `lib.rs:1007`
  assigns `im` from untrusted bytes with no `<= 2` check. No panic path was
  found (no array indexing by mode), but a malformed save installs an invalid IM.

* **L8 — Cross-binding `ram()` inconsistency.** WASM `ram()`
  (`z180-wasm/src/lib.rs:575-580`) returns a **copy** (`Uint8Array::from`), while
  Python `ram()` (`z180-py/src/lib.rs:572-591`) returns a **live, writable**
  memoryview. Writes to the WASM result silently don't reach the machine — a
  porting trap.

* **L9 — Additional SST maskings.** R-register bit 7 is masked
  (`sst.rs:783`, hides `LD R,A` bit-7 bugs), and `mask_pair_flags` applies the
  fixed XY mask to AF′ regardless of a case's `flags_mask` (`:790, 904`), a minor
  inconsistency with the main-AF path.

* **L10 — `zex` trap detection is fragile.** `z180-cli/src/zex.rs:76-84` uses
  `drain_events().next()` with a refutable `if let Some(Event::Trap …)`; correct
  only because no other events are emitted in that config. `run.rs:176-188` uses
  the robust `find_map` pattern — `zex` should match it.

* **L11 — FRC marked available on both variants.** `ioregs.rs:336-344` gates the
  free-running counter (18h) as `BOTH`, but it is likely a Z8S180-only
  enhancement (adjacent enhancement registers ASTC/CMR/CCR/ASEXT are correctly
  gated `S180`). Verify against the UM0050 reset/availability table; if 18h is
  reserved on the base Z80180, this should be `S180`.

* **L12 — ASCI auto-`/CTS` is flag-only.** `start_asci_transmit` (`lib.rs:2821`)
  never consults `/CTS`; `/CTS` only masks the TDRE surface. A byte already
  latched in TDR still transmits after `/CTS` rises. The test at `:5040`
  deliberately enshrines this stance ("CTS does not stop TSR"); UM0050's wording
  ("does not transmit" while `/CTS` High) is arguably stricter. Verify.

---

## Verified correct (spot-checked, no defect)

* **Interrupt core** — NMI edge-latch + `iff2←iff1`/`iff1←false`/vector 0x0066;
  EI one-instruction shadow (cleared post-fetch so trailing `EI` re-arms);
  maskable accept clears both IFFs; INT0 modes 0/1/2; I:IL vectoring with the
  correct fixed source codes (INT1=0x00 … ASCI1=0x10); ITC ITE0/1/2 gating;
  exact UM0050 priority order; HALT-vs-SLEEP distinct wake rules. RETI/RETN
  matches *documented* UM0050 semantics (only RETN restores IFF1) — the NMOS
  "RETI also restores" behavior is an undocumented silicon quirk, so this is
  correct for a UM0050 target, **not** a bug.
* **Redundant DD/FD prefix → TRAP** is a deliberate, cited decision
  (`verification-log.md:33`), not an over-trap.
* **MMU** — CA0/Bank/CA1 boundary selection and 1 MiB wrap are exact (matches the
  boundary tests and the closed-form proptest); recompute triggers on
  CBR/BBR/CBAR writes, reset, and load.
* **Memory** — alignment/range/overlap/ROM-size checks are atomic (validated
  before mutation); ROM writes suppressed and reported; store reclamation
  correct; `load_state` is atomic (all fallible steps before the first mutation)
  and `Memory::is_valid` fully bounds-checks decoded page stores;
  `try_reserve_exact` avoids OOM-abort on hostile capacities.
* **ALU/flags** — 8/16-bit ADD/ADC/SUB/SBC/CP overflow+H+carry, INC/DEC, ADD HL,
  ADC/SBC HL, NEG, MLT, TST, RRD/RLD, CB rotates/shifts/bit/res/set, and the full
  undocumented INI/IND/OUTI/OUTD H/C/P-V behavior all match. R-register bit-7
  preservation is correct.
* **Bindings** — core `#![forbid(unsafe_code)]`; PyO3 buffer `unsafe`
  (`__getbuffer__`/`__releasebuffer__`) reference and `CString` ownership are
  balanced and gated by `active_views`; WASM u64 fields use `BigInt` (no f64
  precision loss) and the TS declarations match the exported surface.
* **Peripherals** — PRT phi/20 tick, TDE gating, TMDR-write-requires-stopped,
  low-byte-latches-high, TCR masks; FRC /10 divide + reset 0xFF + IO_STOP; ASCI
  double-buffering, overrun, RIE/TIE + S180 RDRF-inhibit, DCD latch, baud/frame
  math; CSIO speed table, half-duplex unbuffered receive, EF/EIE protocol — all
  match UM0050 and their hand-computed tests.

---

## Recommended priority

1. **C1** — fix the TRAP stacked-PC (+ correct the two tests). This breaks any
   UM0050-conformant trap handler and directly contradicts the project's own
   verification log.
2. **H1** — fix the fixed-address DMA classification; add mode-2 tests.
3. **M1** — fix `ram_segment`'s `?`; the RAM-access API is broken for the common
   ROM-low/RAM-high layout.
4. **M2 / M5** — close the SST spurious-write leniency and the fresh-clone test
   failure (both undermine the test suite as a correctness oracle).
5. Remaining Medium/Low items as capacity allows; several (L3, L5, L11, L12) are
   documentation/scoping clarifications rather than code fixes.
