# z-core: Z180 Emulator Core — Work Plan

This document is the complete, authoritative work plan for building `z-core`: a
from-scratch Z180 CPU/SoC emulator core in Rust, with bindings for Python,
TypeScript (WASM), and later Swift/iOS. It is written to be executed by an AI
coding agent working sequentially through the phases. Follow it exactly.

**The customer** is the `qns` project (`C:\Users\Q\code\qns`), an emulator for
the Blazie Engineering Braille 'N Speak family: Z180 (HD64180-compatible) at
12.288 MHz, heavy MMU banking, ASCI serial, INT2 keyboard interrupt, external
peripherals on I/O ports (SSI-263 speech, display, RTC, etc.). z-core replaces
`z180emu` (GPLv2, C) so the stack can be owned outright and shipped anywhere,
including iOS.

---

## 0. Ground rules for the executing agent

These rules override your defaults. Violating any of them is a failed task.

1. **Execute phases in order.** Do not reorder, skip, parallelize across
   phases, or start a phase before the previous phase's gate is green.
2. **Gates are hard.** Each phase ends with a gate: exact commands and expected
   outcomes. A gate passes only if you ran the commands and the output shows it.
   Paste the raw command output into `PROGRESS.md` under the phase heading.
   A claim of success without pasted output is a failure.
3. **Clean room.** You must NEVER open, read, or copy from the source code of
   any existing emulator: MAME, z180emu (`C:\Users\Q\src\z180emu`), redcode/Z80,
   Ares, FUSE, or any other. Not "for reference", not "just the tables".
   Existing-emulator oracles are used strictly as black boxes (run them,
   observe outputs). First-party reference functions authored solely from
   UM0050 are allowed and must not call or copy any emulator implementation.
   Exception: the `qns` Python codebase is NOT an emulator core and may be read
   freely. Violation of this rule poisons the licensing story of the whole
   project.
4. **Facts come from the manufacturer's documentation.** The primary reference
   is the **Zilog Z8018x Family MPU User Manual** (Zilog document UM0050 /
   "Z80180/Z8S180/Z8L180 MPU User Manual"), freely downloadable from
   zilog.com literature. Download it once into `docs/vendor/` (PDF). Every
   register address, bit assignment, cycle count, vector offset, and trap
   behavior you implement must be traceable to it. Numbers appearing in this
   plan (register maps, opcode encodings, priorities) are believed correct but
   MUST be verified against the manual before use; where the manual and this
   plan disagree, the manual wins — record the discrepancy in
   `docs/verification-log.md`.
5. **Verification log.** Maintain `docs/verification-log.md`: one line per
   verified fact — the fact, the UM section/table number, the date. Cheap to
   write, and it is the audit trail proving the clean-room claim.
6. **When blocked, stop.** If a gate will not pass, a needed asset is missing,
   or the manual contradicts the plan in a way that changes an API, write a
   `BLOCKED:` entry in `PROGRESS.md` describing exactly what is needed, and
   stop. Do not improvise around a blocker. Do not weaken a gate to pass it.
7. **No scope invention.** Implement what this plan says. If you believe
   something extra is needed, write a `PROPOSAL:` note in `PROGRESS.md` and
   continue with the planned work if possible.
8. **Rust quality bar** (enforced by CI from Phase 0):
   - Stable toolchain, pinned in `rust-toolchain.toml`.
   - `z180-core` is `#![no_std]` + `alloc`, zero `unsafe`, zero required
     dependencies (serde optional behind a feature).
   - `cargo clippy --workspace --all-targets -- -D warnings` clean.
   - `cargo fmt --check` clean.
   - No `unwrap()`/`expect()`/`panic!()` in non-test code of `z180-core`.
     Fallible APIs return `Result`. Emulation itself must be total: any guest
     behavior (any opcode byte sequence, any register write) must be handled
     without panicking.
9. **Commit discipline.** One commit per task (tasks are numbered below).
   Message format: `P<phase>.<task>: <imperative summary>`. Commit only
   compiling, test-passing states except where a task is explicitly
   "scaffolding only".
10. **Dependencies are a closed set.** Allowed: `z180-core`: none required
    (`serde` + `postcard` behind `state` feature); `z180-cli`: `clap`,
    `serde`, `serde_json`, `anyhow`; `z180-py`: `pyo3`; `z180-wasm`:
    `wasm-bindgen`, `js-sys`, `serde-wasm-bindgen`. Dev-deps for tests:
    `proptest` (Rust), `hypothesis` + `pytest` (Python tooling in
    `tools/reference/` and z180-py tests). Anything else requires a
    `PROPOSAL:` note and Q's approval.
11. **Determinism is a feature.** Same config + same inputs + same call
    sequence must produce bit-identical state and event streams on every
    platform. No wall-clock time, no randomness, no host-dependent behavior
    anywhere in `z180-core`.

---

## 1. Product definition

### 1.1 What z-core is

- A cycle-counting, interrupt-accurate, MMU-accurate emulator of the Z8018x
  CPU family (Z80180 baseline; Z8S180 extras behind a variant flag), including
  the on-chip peripherals: MMU, 2× PRT timers, 2× ASCI serial, CSI/O, 2× DMA,
  interrupt controller, and the undefined-opcode TRAP mechanism.
- One Rust core, consumed as: a Rust crate; a Python native module (PyO3);
  an npm package (WASM + TypeScript types); later a Swift XCFramework.
- Guest memory lives INSIDE the core (this is the performance model — no
  per-access host callbacks on the hot path). Hosts interact through
  configuration, I/O port handlers, queues, watchpoints, and event rings.
- First-class debug/tooling API: the qns project "lives and dies on tooling",
  so traces, watches, and save-states are core features, not afterthoughts.

### 1.2 What z-core is not (non-goals for v0.1)

- Not a full-system BNS emulator (qns remains that; z-core is the CPU/SoC).
- Not a binary translator / JIT. Straight interpreter. Performance target is
  easily met (see 1.3).
- Not cycle-exact at the bus-waveform level. We count cycles per instruction
  and schedule peripherals on that clock; we do not model T-state bus phases.
- No Z80182/Z8L180-specific peripherals (ESCC, PIA ports) in v0.1.

### 1.3 Performance targets

- Native (release build): ≥ 100 million emulated cycles/second on the dev
  machine. (12.288 MHz target hardware ⇒ ≥ 8× real-time is the floor; expect
  far more.)
- Python binding, internal-memory mode: ≥ 50M cycles/sec.
- WASM in Node: ≥ 25M cycles/sec.
- These are gate numbers for the benchmark tasks; if missed, profile and fix
  before proceeding. Correctness always lands first; optimization only happens
  inside the phase that benchmarks it.

### 1.4 Licensing

- Dual license **MIT OR Apache-2.0**. `LICENSE-MIT` and `LICENSE-APACHE` at
  repo root from Phase 0.
- The clean-room rule (§0.3) exists to keep this claim true.
- GPL'd test *programs* (e.g. ZEXDOC binary) may be run by tools but are
  vendored under `tests/vendor/` with a `NOTICES.md` explaining they are test
  data/programs executed by the emulator, not linked code.

---

## 2. Architecture

### 2.1 Workspace layout

```
z-core/
├── Cargo.toml            # [workspace]
├── rust-toolchain.toml
├── LICENSE-MIT
├── LICENSE-APACHE
├── README.md
├── PLAN.md               # controlling plan; edit only with Q's explicit authorization
├── PROGRESS.md           # per-phase log, gate outputs, BLOCKED/PROPOSAL notes
├── docs/
│   ├── vendor/           # UM0050 PDF and any other datasheets (gitignored if large)
│   ├── verification-log.md
│   ├── timing-notes.md   # written in P3
│   ├── qns-migration.md  # written in P7
│   └── ARCHITECTURE.md   # written in P10
├── crates/
│   ├── z180-core/        # the emulator. no_std + alloc, no unsafe, no deps
│   ├── z180-cli/         # disassembler, ROM runner, SST runner, ZEX harness
│   ├── z180-py/          # PyO3 bindings (maturin project)
│   └── z180-wasm/        # wasm-bindgen bindings + TS types
├── tests/
│   ├── sst/              # git submodule: SingleStepTests/z80 (JSON tests)
│   ├── z180-sst/         # OUR generated Z180 single-step JSON tests (P1)
│   └── vendor/           # zexdoc.com etc. + NOTICES.md
└── tools/
    └── reference/        # independent UM0050-derived Python specification,
                          # corpus generator, and Hypothesis strategies
```

### 2.2 Core data model (crate `z180-core`)

Implement these types with exactly these semantics (names may not drift):

```rust
pub struct MachineConfig {
    pub clock_hz: u32,        // informational; default 12_288_000
    pub phys_addr_bits: u8,   // 20..=24; default 20. Page table covers 2^bits.
    pub unmapped_read: u8,    // value returned for unmapped phys reads; default 0xFF
    pub variant: Variant,     // Z80180 | Z8S180
    pub regions: Vec<RegionDef>,
}

pub enum Variant { Z80180, Z8S180 }

pub struct RegionDef { pub base: u32, pub size: u32, pub kind: RegionKind }

pub enum RegionKind {
    Ram,                // core-owned, zero-initialized
    Rom(Vec<u8>),       // core-owned, writes ignored (and recorded as Event::RomWrite)
    External,           // reads/writes go to the HostBus callback (slow path)
}
```

- **Physical memory** is managed as a page table over `2^phys_addr_bits`
  bytes, page size 4 KiB. Each page entry: `Ram{offset}`, `Rom{offset}`,
  `External`, `Unmapped`. Base/size of regions must be 4 KiB aligned
  (constructor returns `Err` otherwise).
- **Why `phys_addr_bits` is configurable:** the Z180 MMU itself emits 20-bit
  physical addresses (hardware fact — do not "extend" the MMU). Larger
  physical spaces exist to support board-level banking hardware: the host
  models a bank latch by observing an I/O port write and calling
  `remap(base, size, kind)` / `set_ext_mapper` (below) to repoint pages.
- **External address mapper (optional):**
  `set_ext_mapper(Option<fn(u32) -> u32>)` — a pure function applied to the
  MMU's 20-bit output before page lookup, for boards whose banking sits
  between CPU and memory. Default `None` (identity).

```rust
pub trait HostBus {
    fn mem_read(&mut self, phys: u32) -> u8;         // External regions only
    fn mem_write(&mut self, phys: u32, val: u8);
    fn io_read(&mut self, port: u16) -> u8;          // ports NOT claimed by on-chip I/O
    fn io_write(&mut self, port: u16, val: u8);
}
```

Core object and lifecycle:

```rust
pub struct Z180<B: HostBus> { /* private */ }

impl<B: HostBus> Z180<B> {
    pub fn new(config: MachineConfig, bus: B) -> Result<Self, ConfigError>;
    pub fn reset(&mut self);                     // hardware reset per UM
    pub fn step(&mut self) -> u32;               // one instruction; returns cycles consumed
    pub fn run(&mut self, cycles: u32) -> u32;   // run until >= budget; returns actual
    pub fn cycle_count(&self) -> u64;            // total since construction
    pub fn halted(&self) -> bool;                // in HALT/SLP state

    // Registers
    pub fn reg(&self, r: Reg) -> u16;            // Reg::{PC,SP,AF,BC,DE,HL,IX,IY,AF2,BC2,DE2,HL2,IR}
    pub fn set_reg(&mut self, r: Reg, v: u16);
    pub fn instruction_pc(&self) -> u16;         // start address of current/last instruction

    // Interrupt pins
    pub fn set_irq(&mut self, line: IrqLine, level: bool);  // IrqLine::{Int0,Int1,Int2}
    pub fn set_nmi(&mut self, level: bool);

    // MMU / internal I/O visibility (side-effect-free)
    pub fn io_reg_peek(&self, internal_addr: u8) -> u8;     // 0x00..=0x3F internal file
    pub fn mmu_translate(&self, logical: u16) -> u32;       // current CBR/BBR/CBAR mapping

    // Serial queues (host side of the pins)
    pub fn asci_rx_push(&mut self, ch: usize, byte: u8) -> bool; // false if rx not ready
    pub fn asci_tx_pop(&mut self, ch: usize) -> Option<u8>;
    pub fn csio_rx_push(&mut self, byte: u8) -> bool;
    pub fn csio_tx_pop(&mut self) -> Option<u8>;

    // Memory access (host debugging; physical addresses)
    pub fn mem_peek(&self, phys: u32) -> u8;
    pub fn mem_poke(&mut self, phys: u32, val: u8);
    pub fn remap(&mut self, base: u32, size: u32, kind: RegionKind) -> Result<(), ConfigError>;
}
```

Debug/trace/events API (all in core, all deterministic):

```rust
pub enum Event {
    IoRead   { cycle: u64, pc: u16, port: u16, val: u8 },
    IoWrite  { cycle: u64, pc: u16, port: u16, val: u8 },
    MemWrite { cycle: u64, pc: u16, phys: u32, val: u8 },  // only for watched ranges
    MemRead  { cycle: u64, pc: u16, phys: u32, val: u8 },  // only for watched ranges
    IrqAck   { cycle: u64, source: IrqSource, vector: u16 },
    Trap     { cycle: u64, pc: u16, opcode: [u8; 3], len: u8 },
    RomWrite { cycle: u64, pc: u16, phys: u32, val: u8 },
}

pub fn add_mem_watch(&mut self, base: u32, size: u32, kind: WatchKind) -> WatchId; // Read|Write|Both
pub fn remove_mem_watch(&mut self, id: WatchId);
pub fn set_io_trace(&mut self, enabled: bool);            // emit all Io* events
pub fn set_irq_trace(&mut self, enabled: bool);
pub fn set_pc_watch(&mut self, addr: Option<u16>);        // logical address
pub fn pc_watch_hits(&self) -> u64;
pub fn drain_events(&mut self) -> Vec<Event>;             // ring buffer, capacity configurable,
                                                          // overflow sets a sticky `events_lost` flag

// Instruction trace (heavier; off by default)
pub struct TraceEntry { pub cycle: u64, pub pc: u16, pub phys_pc: u32, pub bytes: [u8;4], pub len: u8 }
pub fn set_insn_trace(&mut self, capacity: Option<usize>);
pub fn drain_insn_trace(&mut self) -> Vec<TraceEntry>;

// Save states (feature = "state")
pub fn save_state(&self) -> Vec<u8>;                      // versioned, postcard-encoded
pub fn load_state(&mut self, data: &[u8]) -> Result<(), StateError>;
```

Design rule for the hot path: `step()` reads instruction bytes via an inlined
page-table lookup; no trait-object dispatch for Ram/Rom pages; `HostBus` is
only reached for `External` pages and unclaimed I/O ports. Events are pushed
to a preallocated ring, never allocated per event after warm-up.

### 2.3 Single source of truth for the instruction set

One module, `crates/z180-core/src/optable.rs`, defines a static table per
opcode page (main, CB, ED, DD, FD, DDCB, FDCB): for every opcode —
mnemonic template, operand kinds, byte length, Z180 cycle count, and a
handler function pointer. The interpreter dispatch, the disassembler
(`z180-cli dis`), and the docs generator all consume THIS table. No second
copy of any opcode fact may exist anywhere in the workspace. Cycle counts
live only here.

---

## 3. Z180 technical reference summary

Everything in this section is the implementation checklist of *facts*. Each
fact must be verified against UM0050 before implementation (rules 0.4, 0.5).
Where this plan turns out to be wrong, the manual wins.

### 3.1 Z180 vs Z80 — the differences that matter

1. **No undocumented opcodes.** Every Z80 "undocumented" instruction
   (IXH/IXL/IYH/IYL 8-bit ops, SLL, ED holes, DDCB/FDCB result-copy variants)
   is an *undefined* opcode on Z180 and triggers the TRAP mechanism (3.4).
   The BNS firmware investigation in qns has already hit a real illegal-opcode
   trap, so this must be exactly right.
2. **New instructions** (see 3.5): MLT, TST, TSTIO, IN0, OUT0, OTIM, OTDM,
   OTIMR, OTDMR, SLP.
3. **Different instruction timing.** Z180 cycle counts are generally shorter
   than Z80. Never copy Z80 timing; transcribe the Z180 table from UM0050
   (Phase 4). Memory and I/O wait-state insertion is configurable by the
   guest via DCNTL — model it.
4. **Undocumented flag bits (XF/YF, bits 5 and 3 of F)** are not guaranteed to
   match Z80 behavior. Policy: implement the documented six flags rigorously;
   set XF/YF from the conventional Z80 rule where cheap (result bits 5/3),
   and MASK bits 3 and 5 in all external conformance comparisons. Where
   UM0050 marks a documented flag as affected but supplies no resulting-value
   rule (OTIM/OTDM S, H, P/V, and C), the generated case's `flags_mask` also
   excludes that flag rather than inventing a value.
5. **On-chip peripherals** occupy a 64-byte internal I/O window (3.6),
   relocatable via ICR.
6. **MMU** translates every logical 16-bit address to 20-bit physical (3.3).
7. **R register** exists and increments per opcode fetch as on Z80 (verify
   exact increment rules in UM); `LD A,R` semantics as documented.
8. **I/O address decode:** Z80-style `OUT (n),A` places A on A15..A8;
   `IN r,(C)`/`OUT (C),r` use full BC; `IN0`/`OUT0` force high byte 0x00.
   Internal I/O only matches when the 16-bit port falls in the internal
   window per the UM's decode rule — verify it and encode it exactly once in
   the I/O dispatch.

### 3.2 CPU core state

- Registers: AF, BC, DE, HL, AF', BC', DE', HL', IX, IY, SP, PC, I, R.
- IFF1/IFF2, interrupt modes IM0/IM1/IM2, EI shadow (interrupts enabled only
  after the instruction following EI).
- HALT state; SLP (sleep) state — both exit per UM.
- `instruction_pc`: address of the first byte of the currently executing
  instruction (the debugger and TRAP logic both need it).

### 3.3 MMU

- Registers (internal I/O): CBR (0x38), BBR (0x39), CBAR (0x3A).
- CBAR low nibble = BA (Bank Area base, logical bits 15..12); high nibble =
  CA (Common Area 1 base).
- Translation of logical address L, with LA = L >> 12:
  - LA < BA        → Common Area 0: phys = L (no relocation)
  - BA ≤ LA < CA   → Bank Area:     phys = L + (BBR << 12)
  - LA ≥ CA        → Common Area 1: phys = L + (CBR << 12)
  (20-bit result, wraps modulo 2^20.) Verify the boundary conventions
  (≤ vs <) against the UM figures — off-by-one here is a classic bug.
- Reset: CBR = BBR = 0x00, CBAR = 0xF0. Verify.
- Implementation: recompute a 16-entry logical-page → physical-base array on
  every CBR/BBR/CBAR write; hot-path translation is one index + add.

### 3.4 TRAP (undefined opcode)

Per UM0050 "TRAP interrupt":
- Fetching an undefined opcode sets ITC bit 7 (TRAP). ITC bit 6 (UFO) records
  whether the undefined byte was the 2nd or 3rd byte of the instruction.
- The CPU pushes the PC of the undefined instruction (adjusted per UFO as the
  manual specifies) and restarts at logical 0x0000. IFF handling per UM.
- Guest clears TRAP by writing 0 to ITC bit 7; writes cannot set it. Encode
  ITC read/write masks exactly.
- Also emit `Event::Trap` so hosts can debug traps without guest cooperation.

### 3.5 Z180-added instructions (encodings to verify, then implement)

All on the ED page:

| Mnemonic     | Encoding (believed)       | Notes |
|--------------|---------------------------|-------|
| IN0 r,(n)    | ED 00/08/10/18/20/28/30/38 n | r = B,C,D,E,H,L,A; ED30 changes flags only; port 0x00nn |
| OUT0 (n),r   | ED 01/09/11/19/21/29/39 n | port 0x00nn |
| TST r        | ED 04/0C/14/1C/24/2C/3C   | A AND r → flags only |
| TST (HL)     | ED 34                     | |
| TST n        | ED 64 n                   | |
| TSTIO n      | ED 74 n                   | (C) AND n → flags only |
| MLT ss       | ED 4C/5C/6C/7C            | BC/DE/HL/SP: 8×8 → 16 unsigned |
| OTIM / OTIMR | ED 83 / ED 93             | block out, incrementing |
| OTDM / OTDMR | ED 8B / ED 9B             | block out, decrementing |
| SLP          | ED 76                     | sleep until interrupt/reset |

Flag effects: take from the UM0050 instruction descriptions, not from memory
and not from this table. The external Z80 suites cannot cover these — their
tests come from the reference-generated suite (Phase 1) plus hand-written unit
tests per family.

### 3.6 Internal I/O register map (base 0x00; relocatable via ICR)

ICR (0x3F) bits 7..6 relocate the 64-byte window to 0x00/0x40/0x80/0xC0.
Believed map — VERIFY EVERY ROW against the UM0050 internal I/O register
table; rows marked (S) are Z8S180-only:

```
0x00 CNTLA0    0x01 CNTLA1    0x02 CNTLB0    0x03 CNTLB1
0x04 STAT0     0x05 STAT1     0x06 TDR0      0x07 TDR1
0x08 RDR0      0x09 RDR1      0x0A CNTR      0x0B TRDR
0x0C TMDR0L    0x0D TMDR0H    0x0E RLDR0L    0x0F RLDR0H
0x10 TCR       0x12 ASEXT0(S) 0x13 ASEXT1(S)
0x14 TMDR1L    0x15 TMDR1H    0x16 RLDR1L    0x17 RLDR1H
0x18 FRC       0x1A ASTC0L(S) 0x1B ASTC0H(S) 0x1C ASTC1L(S)
0x1D ASTC1H(S) 0x1E CMR(S)    0x1F CCR(S)
0x20 SAR0L     0x21 SAR0H     0x22 SAR0B     0x23 DAR0L
0x24 DAR0H     0x25 DAR0B     0x26 BCR0L     0x27 BCR0H
0x28 MAR1L     0x29 MAR1H     0x2A MAR1B     0x2B IAR1L
0x2C IAR1H     0x2D IAR1B(S)  0x2E BCR1L     0x2F BCR1H
0x30 DSTAT     0x31 DMODE     0x32 DCNTL     0x33 IL
0x34 ITC       0x36 RCR       0x38 CBR       0x39 BBR
0x3A CBAR      0x3E OMCR      0x3F ICR
```

Unlisted addresses are reserved: define read value and write behavior from
the UM and test it. Every register gets: reset value, read mask, write mask,
side-effect handler — encoded in ONE table in `ioregs.rs` (same
single-source-of-truth discipline as the optable).

### 3.7 Interrupts

- External pins: /INT0 (honors IM0/IM1/IM2), /INT1, /INT2 (always vectored),
  /NMI. Edge/level behavior per UM — verify.
- Internal sources: PRT0, PRT1, DMA0, DMA1, CSI/O, ASCI0, ASCI1.
- Priority (highest first), believed: TRAP, NMI, INT0, INT1, INT2, PRT0,
  DMA0, PRT1, DMA1, CSI/O, ASCI0, ASCI1 — VERIFY against the UM's priority
  table; do not trust this list.
- INT1/INT2/internal vectors: high byte = I register; low byte = IL's
  programmable bits | fixed per-source offset. Believed offsets: INT1=0x00,
  INT2=0x02, PRT0=0x04, PRT1=0x06, DMA0=0x08, DMA1=0x0A, CSI/O=0x0C,
  ASCI0=0x0E, ASCI1=0x10 — VERIFY.
- ITC bits ITE0/ITE1/ITE2 enable external pins (reset: only ITE0 — verify).
  IFF1 gates all maskable sources; NMI and TRAP are not maskable.
- Acknowledge cycle counts and stacking per UM.
- qns depends on INT2 (keyboard) and ASCI interrupts being exactly right.

### 3.8 On-chip peripherals — behavioral requirements

**PRT (2× programmable reload timers):** 16-bit down-counters TMDR0/1 with
reload RLDR0/1 and control TCR. Clock = system clock / 20. TIF flags on
underflow; flag-clear protocol on the documented read sequence (verify).
Interrupts when TIE set.

**ASCI (2× async serial):** registers CNTLA/CNTLB/STAT/TDR/RDR per channel
(+ ASEXT/ASTC on S180). Model at byte granularity with real timing: bit time
from CNTLB (clock source, prescale, divide ratio) and frame length from
CNTLA/CNTLB (start + 7/8 data + optional parity + 1/2 stop); a byte occupies
frame_bits × bit_time cycles each direction. RX: host pushes via
`asci_rx_push`; core raises RDRF when the modeled frame time elapses; OVRN
and RIE per UM. TX: guest writes TDR; TDRE behavior per UM; completed bytes
appear via `asci_tx_pop`. STAT read/write side effects must follow the UM
exactly (the old z180emu wrapper grew debug counters because this area kept
biting qns — treat it as high-risk). Expose `set_asci_cts(ch, level)` and
`set_asci_dcd(ch, level)` with UM semantics.

**CSI/O:** CNTR + TRDR, half-duplex clocked serial; EF flag, EIE interrupt;
byte-level timing from CNTR speed bits; host side via the two queues.

**DMA (2 channels):** DMA0 memory↔memory and memory↔I/O with 20-bit
SAR0/DAR0 (+B upper bits) and BCR0; DMA1 memory↔I/O via MAR1/IAR1. DSTAT
enable protocol (DE0/DE1 with DWE write-enable — tricky, guests depend on
it), DMODE burst vs cycle-steal, DCNTL wait states + DREQ sense, completion
interrupts DIE0/DIE1, NMI-stops-DMA (verify). Transfers consume bus cycles
interleaved with the CPU per mode, at "cycles per byte" fidelity from the
UM. Expose `set_dreq(ch, level)`.

**FRC (0x18):** free-running 8-bit down-counter, one tick per 10 system
clocks (verify rate); read-only; guests use it for delays.

**RCR (refresh):** implement the register; the timing effect of refresh
cycles may be approximated as zero for v0.1 — record the choice in
`docs/timing-notes.md` and verify BNS firmware doesn't depend on it.

**OMCR / CCR+CMR (S180):** implement storage plus any bits with observable
behavior. v0.1 accounts time in system-clock (φ) cycles only; document this
in timing-notes.md.

### 3.9 Reset state

Collect every reset value from the UM into the `ioregs.rs` table and ONE
test (`tests/reset_state.rs`) asserting all of them via `io_reg_peek`. CPU
registers: use only the UM's stated reset values; where the UM says
"undefined", choose 0x0000 and record the free choice in
verification-log.md.

---

## 4. Execution phases

Phases run strictly in order (rule 0.1). Every phase ends with a **GATE**
block: run those exact commands, paste raw output into `PROGRESS.md`.

> **THE CONFORMANCE-FIRST RULE (absolute):** Phase 1 builds the entire
> conformance apparatus — test suites vendored/generated, runners built,
> negative controls proven — before any CPU behavior beyond the Phase-1 stub
> subset exists. No instruction, flag, MMU, or peripheral implementation work
> may begin until Gate G1 is green. If you find yourself writing opcode
> handlers and G1 is not green in PROGRESS.md, you are off-plan: stop, revert
> to the last green state, and do Phase 1.

### Phase 0 — Scaffold and CI

Tasks:
1. `cargo new` workspace per layout 2.1: crates `z180-core` (lib, no_std),
   `z180-cli` (bin), placeholders NOT yet for py/wasm (created in their
   phases). Pin stable toolchain in `rust-toolchain.toml`.
2. Add LICENSE-MIT, LICENSE-APACHE, README.md (one paragraph: what, why,
   status), .gitignore (target/, docs/vendor/*.pdf, *.pyd, node_modules/),
   PROGRESS.md (empty phase headings), docs/verification-log.md (header row).
3. CI: GitHub Actions workflow `ci.yml` — ubuntu + windows jobs running
   `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D
   warnings`, `cargo test --workspace`. Add `#![forbid(unsafe_code)]` and
   `#![no_std]` to z180-core now.
4. Download UM0050 PDF into docs/vendor/ and record its exact title, source
   URL, and revision in verification-log.md. If it cannot be fetched,
   BLOCKED per rule 0.6.

**GATE G0:** `cargo test --workspace && cargo clippy --workspace
--all-targets -- -D warnings && cargo fmt --check` all green locally;
UM0050 present. Paste output.

### Phase 1 — Conformance apparatus (before the emulator exists)

Tasks:
1. **Vendor the Z80 single-step suite.** Add
   `https://github.com/SingleStepTests/z80` as git submodule at `tests/sst`.
   Record its commit hash in PROGRESS.md.
2. **Stub CPU subset.** Implement in z180-core ONLY: fetch loop skeleton,
   register file, and the opcodes 0x00 NOP, 0x76 HALT, and the 0x40..0x7F
   LD r,r' block. Nothing else. This exists purely to prove the harness
   end-to-end.
3. **SST runner** in z180-cli: `z180-cli sst --dir tests/sst/v1 [--only
   XX,YY] [--report json|text]`. For each JSON test: build a 64K flat-RAM
   machine (MMU reset state = identity for the logical space in play),
   load `initial` state (registers incl. AF', I, R, IFFs; memory bytes), run
   ONE instruction, compare against `final`: registers, memory. Comparison
   policy: mask F bits 3 and 5 everywhere; ignore the suite's `cycles` bus
   arrays entirely (Z180 timing differs from Z80 — our timing gate is Phase
   4); compare R masked to 7 bits and allow it to be excluded via
   `--ignore-r` (document why if used). Undocumented-opcode test files are
   excluded by the opcode policy list (Appendix A). Distinguish three
   outcomes per test: PASS / FAIL (with first differing field printed) /
   UNIMPLEMENTED (opcode has no handler). UNIMPLEMENTED is never a pass.
4. **Negative control.** Add a hidden debug flag `--sabotage-ld` to the
   runner that deliberately swaps the operands of LD r,r' before comparison.
   Prove: without the flag, the LD/NOP/HALT SST files pass; with the flag,
   they fail. This demonstrates the harness detects wrongness. Keep the flag
   (it is rerun at every phase gate as a self-check).
5. **UM0050 reference corpus.** In `tools/reference/` (Python, run with
   `uv`), implement small first-party state-transition functions derived
   directly from the verified UM0050 rules and use them to GENERATE our
   Z180-specific single-step suite into `tests/z180-sst/` (Appendix C):
   - For every Z180-added opcode (3.5): ≥ 200 cases each of randomized
     (seeded, seed recorded in the file) register/memory initial states →
     apply exactly one independent reference transition → record final
     state. I/O instructions also record deterministic port reads/writes in
     the SST `ports` format so their externally visible behavior is tested.
     Each case records the documented F-bit comparison mask derived from
     3.1.4; it is `0xD7` except OTIM/OTDM, whose UM-defined mask is `0x42`.
     Phase 1 I/O strategies select ports outside the reset-state 64-byte
     internal window; internal-register behavior belongs to Phase 5.
   - TRAP cases: a representative sample (≥ 50) of undefined opcodes across
     pages (Appendix A list), with reference-computed post-trap PC, stack
     contents, and ITC.
   - MMU cases: ≥ 200 randomized CBR/BBR/CBAR programs, each followed by
     reads through all 16 logical 4K pages, with effective physical
     addresses computed from the verified closed-form rule in 3.3
     (observable via which RAM byte is returned).
   Reference code must not import, call, or copy z-core or any existing
   emulator. Every transition rule and numeric constant must cite a row in
   `docs/verification-log.md`; UM0050 wins over the plan and reference code.
   If a later optional black-box comparison disagrees, annotate the affected
   JSON case with `"disputed": true` plus a `dispute_note` citing the UM
   section and record the adjudication in `verification-log.md`.
   Implementation detail: define the Phase 1 instruction encodings and pure
   transitions once in `tools/reference/spec.py`; the Rust optable does not
   exist until Phase 2 and therefore cannot be a Phase 1 input. State
   generation uses **Hypothesis strategies** defined once in
   `tools/reference/strategies.py` (register values, flag bytes, MMU register
   triples, memory patterns, and instruction encodings from `spec.py`).
   Create `tools/reference/pyproject.toml`,
   `tools/reference/.python-version`, and `tools/reference/uv.lock` with only
   the allowed `hypothesis` and `pytest` dependencies, plus
   `generate.py` as the re-runnable corpus entrypoint. Generate the checked-in
   corpus with exactly:

   ```text
   uv run --project tools/reference python tools/reference/generate.py --out tests/z180-sst
   ```

   Corpus files are emitted via `@settings(derandomize=True)` plus explicit
   seeds with Python and dependency versions pinned by `uv.lock`. The same
   strategies are reused by the Phase 8 reference differential properties —
   one vocabulary of "interesting states" for the whole project.
5b. **Reference-generator self-consistency and schema check** (Hypothesis,
   runs now, no z-core needed). Create
   `tools/reference/test_reference.py` for these checks:
   - Property: applying the same reference transition twice to identical
     initial state and instruction inputs yields identical final state and
     I/O events for ≥ 1000 examples.
   - Generate the complete corpus twice into separate temporary directories
     and require byte-identical directory trees.
   - Validate every case against Appendix C, every opcode file at ≥ 200
     cases, TRAP at ≥ 50 cases, and MMU at ≥ 200 cases.
   Run exactly:

   ```text
   uv run --project tools/reference pytest tools/reference
   uv run --project tools/reference python tools/reference/generate.py --check tests/z180-sst
   ```

   The latter regenerates into a temporary directory, validates schema/counts,
   and compares every relative path byte-for-byte.
   Any nondeterminism, schema failure, or count failure: BLOCKED, report to Q
   with the shrunk example or exact differing path.
6. **ZEX assets + harness skeleton.** Vendor `zexdoc.com` (CP/M binary) into
   `tests/vendor/zex/` with NOTICES.md (source URL + GPL notice). If no
   trustworthy source is found, BLOCKED — ask Q. Build `z180-cli zex
   <file.com>`: 64K RAM machine, binary loaded at 0x0100, PC=0x0100, minimal
   CP/M shim (BDOS entry 0x0005 handling C=2 console-out and C=9 string-out
   to stdout; jump-to-0x0000 warm boot ends the run). It will not pass yet —
   the skeleton just has to load, run, and terminate on the stub CPU (it
   will hit UNIMPLEMENTED opcodes; the harness must report that cleanly, not
   crash).
7. **z180-sst runner mode**: `z180-cli sst --dir tests/z180-sst` shares all
   runner code with task 3, deserializes and validates the complete Appendix C
   schema (including `z180`, `ports`, and all 16 MMU probes), dispatches the
   `instruction`/`trap`/`mmu` case kinds, provides a scripted `HostBus` for
   deterministic port reads and recorded writes, compares F through the
   case's `flags_mask` when a case executes, and adds a `--census` report of
   case counts per opcode and special family. In Phase 1 every z180-sst case
   remains UNIMPLEMENTED: the runner does not inject or compare Appendix C
   `z180` state yet. Do not add a privileged SST-only setter or adapter.
   Phase 3 activates instruction/TRAP execution from reset state through the
   owning CPU/TRAP interfaces; Phase 5 activates MMU execution by programming
   CBR/BBR/CBAR through the real internal-I/O instruction path.

**GATE G1** (all pasted):
- `z180-cli sst --dir tests/sst/v1 --only 00,76,40..7f` → PASS for all
  implemented-opcode files, UNIMPLEMENTED (not FAIL) elsewhere.
- Same command with `--sabotage-ld` → FAILs reported on LD files.
- `tests/z180-sst/` populated; case counts per opcode plus TRAP/MMU family
  printed by `z180-cli sst --dir tests/z180-sst --census`; all currently
  UNIMPLEMENTED.
- `z180-cli zex tests/vendor/zex/zexdoc.com` terminates with a clean
  "unimplemented opcode at PC=xxxx" report.
- CI green on the submodule checkout.

### Phase 2 — Full unprefixed opcode page

Tasks:
1. Implement `optable.rs` structure (2.3) and migrate the stub opcodes into
   it.
2. Implement all documented unprefixed opcodes (0x00..0xFF): loads, ALU,
   INC/DEC, rotates (RLCA/RRCA/RLA/RRA), DAA (bit-exact — test heavily),
   CPL/SCF/CCF, jumps/calls/returns/RST, EX/EXX, PUSH/POP, IN A,(n) /
   OUT (n),A (route to I/O dispatch; internal window per 3.6 once it exists —
   for now all I/O goes to HostBus), DJNZ, HALT, DI/EI (IFF + EI shadow),
   flag semantics for every op from UM0050 (its instruction summary tables).
3. Interrupt-check point in the fetch loop exists but no sources fire yet.

**GATE G2:** `z180-cli sst --dir tests/sst/v1` → every documented
unprefixed-opcode file 100% PASS; zero FAIL anywhere; remaining
UNIMPLEMENTED only for prefixed pages. `--sabotage-ld` still detects. Paste
the summary table.

### Phase 3 — Prefixed pages, Z180 instructions, TRAP

Tasks:
1. CB page (rotates/shifts/BIT/RES/SET — documented only; SLL is undefined
   → TRAP).
2. DD/FD pages: documented IX/IY forms only; every undocumented DD/FD
   combination follows the UM's rule for undefined operation (verify: some
   act as if the prefix were absent vs trap — encode exactly what the UM
   says, and cover the verified result in the reference-generated corpus).
3. DDCB/FDCB: documented displacement forms only; undocumented → per UM.
4. ED page: Z80 documented ED ops (LDIR family, IN/OUT (C), ADC/SBC HL,
   RETI/RETN, IM x, LD I/A etc.) + all Z180 additions from 3.5. ED holes →
   TRAP.
5. TRAP mechanism per 3.4, including UFO adjustment and Event::Trap.
6. NEG, RRD/RLD, and the block ops' flag subtleties from the UM tables.
7. Activate the z180-sst `instruction` and `trap` families. Require their
   initial `z180` fields to equal reset state rather than injecting them.
   Supply and verify their scripted port events, compare masked F and base CPU
   state, and compare resulting ITC and SLP state through the owning public
   interfaces implemented in this phase. Leave the `mmu` family
   UNIMPLEMENTED.

**GATE G3:**
- `z180-cli sst --dir tests/sst/v1` → all documented files PASS, zero FAIL,
  zero UNIMPLEMENTED (excluded undocumented files listed by name in the
  report, count matching Appendix A policy).
- `z180-cli sst --dir tests/z180-sst` → 100% PASS on reference-generated
  Z180-op and TRAP
  suites (MMU suite may remain UNIMPLEMENTED until Phase 5; the report must
  show it as such).
- Unit tests: `cargo test -p z180-core trap` green.

### Phase 4 — Timing + ZEXDOC

Tasks:
1. Transcribe Z180 cycle counts for every implemented opcode from UM0050's
   instruction summary into `optable.rs` (the ONLY place cycle numbers may
   live). Record the UM table number in verification-log.md. Include: extra
   cycles for taken vs untaken branches, block-op repeat iterations,
   interrupt acknowledge costs.
2. Implement DCNTL memory/I-O wait-state insertion into cycle accounting
   (reset value of DCNTL = maximum waits — verify).
3. Write `docs/timing-notes.md`: what is counted, what is approximated
   (refresh, DMA interleave granularity), and why.
4. Timing spot-check tests: hand-computed total cycles for ≥ 20 short
   straight-line programs (covering branches taken/untaken, block ops, EX,
   MLT) asserted via `cycle_count()`.
5. Run ZEXDOC to completion under `z180-cli zex`.

**GATE G4:** paste (a) `cargo test -p z180-core timing` green; (b) full
ZEXDOC console transcript showing every test line reporting OK and the
final "Tests complete" line. Any ZEXDOC failure is a hard stop — fix before
proceeding. (ZEXDOC exercises documented flags only; XF/YF-sensitive CRC
mismatches should not occur — if one does, investigate rather than mask.)

### Phase 5 — Interrupts, MMU, internal I/O window

Tasks:
1. Internal I/O register file + dispatch (3.6): `ioregs.rs` table (reset
   value, read mask, write mask, side-effect hooks), ICR relocation, IN0/
   OUT0/TSTIO/OTIM-family routing, 16-bit port decode rule per UM.
2. MMU (3.3): translation array, CBR/BBR/BAR writes, `mmu_translate`
   accessor. All instruction fetches and memory operands now translate.
   Activate the z180-sst `mmu` family by executing ordinary internal-I/O
   instructions to program CBR/BBR/CBAR, then run and compare all 16 probes
   and resulting `z180` state. Do not bypass the internal-I/O instruction
   path with a runner-only setter.
3. Interrupt machinery (3.7): IM0 (treat bus vector as RST 38h unless a
   mode-0 vector API is added — document choice), IM1, IM2, NMI, TRAP
   priority, INT1/INT2 + internal vectoring via I:IL, ITC enables, EI
   shadow, HALT/SLP wake. Acknowledge cycle costs from UM.
4. Unit tests: vector dispatch matrix (each source × enabled/disabled ×
   IFF states), MMU boundary cases (BA=CA, BA=0, CA=0xF, wrap at 1MB), ICR
   relocation round-trip.

**GATE G5:**
- `z180-cli sst --dir tests/z180-sst` → 100% PASS including the MMU suite.
- `cargo test -p z180-core interrupts mmu ioregs` green.
- `z180-cli sst --dir tests/sst/v1` still 100% (regression check — SST
  machines run with reset-state MMU, which must still be identity for the
  covered logical range).

### Phase 6 — On-chip peripherals

Tasks (each peripheral = implement + unit tests derived from UM register
descriptions; one task per peripheral, in this order):
1. PRT0/PRT1 (+ TCR flag-clear protocol tests; interrupt delivery test).
2. FRC.
3. ASCI0/ASCI1 (frame timing math tested against hand-computed bit times
   for the standard divisor settings; RDRF/OVRN/TDRE/interrupt protocol
   tests; CTS/DCD gating tests).
4. CSI/O.
5. DMA0/DMA1 (DE/DWE enable protocol test; mem→mem copy test with cycle
   cost assertions; DMA1 mem↔I/O with a scripted HostBus; NMI-stop test).
6. Peripheral↔interrupt integration: each source raises its vector with
   correct priority ordering (pairwise priority tests).
7. Determinism test: run a scripted machine (timers + ASCI traffic + DMA)
   for 10M cycles twice; assert identical `save_state()` bytes and event
   streams.

**GATE G6:** `cargo test -p z180-core` full suite green (paste count);
determinism test output pasted; all earlier suites re-run green (single
command: `z180-cli sst --dir tests/sst/v1 && z180-cli sst --dir
tests/z180-sst`).

### Phase 7 — Debug, trace, save-state, disassembler

Tasks:
1. Event ring + watches + pc-watch + io/irq trace (2.2 API) with tests
   (watch fires exactly on watched range; ring overflow sets events_lost).
2. Instruction trace ring.
3. `state` feature: save_state/load_state with version byte; round-trip
   property test (save → load → run N cycles ≡ run N cycles from original).
4. Disassembler in z180-cli (`z180-cli dis file.bin --org 0x0000`), driven
   from optable metadata; golden-file test over a crafted binary covering
   every mnemonic once.
5. `z180-cli run rom.bin --cycles N --trace --config machine.toml`:
   config file mapping (regions, variant, clock) so Q can run bare ROMs.

**GATE G7:** `cargo test --workspace` green; disassembler golden test
green; a save/load/resume demonstration transcript pasted.

### Phase 8 — Python binding + qns migration + reference differential

Tasks:
1. `crates/z180-py`: PyO3 + maturin, abi3 wheel, module name `z180`.
   Surface: `Machine(config_dict)` exposing the full 2.2 API pythonically;
   zero-copy `memoryview` over guest RAM regions for fast inspection.
2. Compat layer `z180.compat.Z180` mirroring the constructor/method
   signatures of qns's current `qns.cpu.Z180` (see that file): mem/io
   callbacks become an External-region + HostBus adapter (slow but
   compatible); `serial_rx/serial_tx/csio_rx/csio_tx` adapt to the queue
   API; `get_reg`/`pc`/`sp`/`halted`/`set_irq`/`cbr`/`bbr`/`cbar`/
   `watch_pc`/`pc_watch_count` preserved. `asci_debug_state` returns the
   new core's equivalent fields where meaningful and zeros elsewhere
   (document each).
3. `docs/qns-migration.md`: exact steps to switch qns to internal-memory
   mode (regions from `qns/profiles.py`, I/O stays callbacks), expected
   perf, and what changes in `qns/memory.py`.
4. Benchmark: `python bench.py` — cycles/sec for (a) compat callback mode,
   (b) internal-memory mode; and the same loop on the old CFFI binding for
   comparison. Record all three numbers.
5. **Optional incumbent lockstep:** only if the old qns binding exposes a
   genuine black-box API that can load and capture the complete compared
   state, `tools/reference/incumbent_lockstep.py` may run the BNS ROM boot on
   BOTH bindings with identical qns-derived wiring. Compare
   `(instruction_pc, AF, BC, DE, HL, SP)` for the first 10 million
   instructions. On divergence: dump both states + disassembly around PC and
   adjudicate with UM citations in `verification-log.md`; z-core-wrong ⇒ fix
   and rerun from zero. If that full-state API is absent, record
   `NOT RUN: no authorized full-state black-box API` in PROGRESS.md. This
   optional task is not a gate and never authorizes reading emulator source.
6. **Hypothesis reference differential fuzzer** —
   `tools/reference/test_differential.py` (pytest + Hypothesis, reusing
   `strategies.py` from Phase 1):
   - Property A (single instruction): for any Z180-added opcode and generated
     initial state, z-core and the independent UM0050 transition produce
     identical final state, touched memory, and I/O events (F masked per the
     generated case's `flags_mask` from 3.1.4). Shared Z80 instructions remain
     covered by SST.
   - Property B (short sequences): for any generated sequence of up to 32
     reference-modeled Z180 instructions in a 4K arena (HALT/SLP excluded),
     final states match instruction-by-instruction.
   - Property C (TRAP/MMU): undefined-opcode transitions and randomized
     CBR/BBR/CBAR accesses across all 16 logical pages match the independent
     reference; MMU effects also match the closed-form 3.3 formula.
   - If the optional incumbent API is available, add it as a third comparison
     leg. A disagreement is adjudicated against UM0050; it never changes the
     mandatory two-way reference gate or licenses emulator-source access.
   - Every failure Hypothesis finds is SHRUNK automatically; the worker pins
     the shrunk case as a permanent `@example(...)` regression AND exports it
     as a `disputed`-or-fixed JSON case into `tests/z180-sst/` before fixing.
     UM wins over both first-party implementations.
   - The Hypothesis example database (`.hypothesis/`) is committed under
     `tools/reference/` so CI reuses discovered edge cases forever.
   - Budget: `--hypothesis-profile=gate` = 2,000 examples per property for
     the G8 gate; a `nightly` profile with 50,000 examples is defined for
     ongoing CI use.

**GATE G8:** paste — `uv run pytest` for z180-py tests on Windows green;
benchmark table (must meet 1.3 targets); reference differential fuzzer: all
three properties pass at the `gate` profile with zero surviving
counterexamples (pinned regressions listed by name). Record the optional
incumbent status as either its completed/adjudicated summary or exactly
`NOT RUN: no authorized full-state black-box API`; that status is
informational and does not weaken or replace the mandatory reference gate.

### Phase 9 — WASM + TypeScript

Tasks:
1. `crates/z180-wasm`: wasm-bindgen wrapper over the same API; `wasm-pack
   build --target web` and `--target nodejs`; package name decided by Q
   (BLOCKED note if unset — default placeholder `@zcore/z180`).
2. Hand-written `.d.ts` refinements if wasm-bindgen's generated types are
   too loose; TS consumers must get typed Event objects.
3. Node smoke test: load a small test ROM (assembled fibonacci loop),
   run 1M cycles, assert register results and cycles/sec ≥ 1.3 target.
4. Browser demo page (minimal, no framework): load ROM file, run, show
   registers + serial output as text. Static files only.

**GATE G9:** paste `wasm-pack build` output + node smoke test output with
perf number.

### Phase 10 — Documentation and v0.1.0

Tasks:
1. `docs/ARCHITECTURE.md`: the 2.x content as-built, plus a data-flow
   diagram (mermaid) of fetch→MMU→page table→bus and the event system.
2. Per-crate READMEs with runnable examples.
3. `CHANGELOG.md`; version 0.1.0 across the workspace; git tag `v0.1.0`.
4. Final full-matrix run: every gate command from G1..G9 re-run in one
   session, outputs pasted into PROGRESS.md under "v0.1.0 evidence".
5. Sweep for TODO/FIXME/dead code; clippy pedantic pass (fix or explicitly
   allow with a comment).

**GATE G10:** the v0.1.0 evidence block complete; `git tag` shows v0.1.0.

### Phase 11 (deferred, not in v0.1) — Swift/iOS

Not to be executed without a separate instruction from Q. Sketch for
planning only: `crates/z180-ffi` exposing a C ABI (cbindgen header) →
XCFramework build for ios/ios-sim/macos → Swift package wrapping it. The
core is no_std+alloc and dependency-free precisely so this phase is
mechanical.

---

## Appendix A — Undefined-opcode policy (drives SST exclusions and TRAP tests)

Undefined on Z180 (all must TRAP; verify each category against UM0050's
instruction set chapter before encoding):

- All Z80 "undocumented" main-page holes: none exist on the main page (every
  main-page byte is defined) — verify.
- CB page: SLL (0x30..0x37) — undefined → TRAP.
- ED page: every byte not listed as a documented Z80 ED op or a Z180 addition
  (3.5) → TRAP. Enumerate the defined set in `optable.rs`; everything else is
  a single shared trap handler.
- DD/FD pages: only the documented IX/IY instruction forms are defined. For
  every other following byte, implement exactly what UM0050 specifies for
  undefined prefix sequences (trap vs prefix-ignored — DO NOT GUESS; verify,
  and cover the verified behavior in the reference-generated z180-sst cases).
- DDCB/FDCB: only the documented displacement forms; others per UM.

SST exclusion rule (Phase 1 runner): exclude a test file if and only if its
opcode is undefined-on-Z180 per this appendix. The runner prints the excluded
list with the reason; the count must be stable across runs and recorded in
PROGRESS.md at each gate.

## Appendix B — Test asset acquisition

| Asset | Source | Placement | Notes |
|---|---|---|---|
| UM0050 (Z80180/Z8S180/Z8L180 MPU User Manual) | zilog.com literature search | docs/vendor/ | Record revision + URL in verification-log.md |
| SingleStepTests Z80 JSON | github.com/SingleStepTests/z80 | tests/sst (submodule) | Pin commit hash |
| zexdoc.com | ask Q if no trustworthy mirror; commonly redistributed with Z80 emulator projects | tests/vendor/zex/ | GPL notice in NOTICES.md; executed, never linked |
| BNS ROMs | C:\Users\Q\code\qns\roms\ | not copied here | used only via qns in Phase 8 |
| Independent reference model | verified UM0050 facts | tools/reference/ | first-party specification code; never imports z-core or emulator code |
| Optional incumbent comparison | qns's built `_z180_cffi` extension | not copied here | non-gating; black box only and only when a complete state API exists |

## Appendix C — z180-sst JSON format

Instruction and TRAP cases use the SingleStepTests Z80 v1 shape: `name`,
`initial`, and `final`, with
`pc,sp,a,b,c,d,e,f,h,l,i,r,ix,iy,af_,bc_,de_,hl_,iff1,iff2,im,ram`.
They omit `cycles` but retain the standard ordered `ports` events
`[address, value, "r"|"w"]`; read events provide deterministic HostBus
inputs and write events are expected outputs. Each case additionally has:

```json
{
  "kind": "instruction",
  "seed": 12345,
  "flags_mask": 215,
  "disputed": false,
  "dispute_note": ""
}
```

`kind` is `instruction` for opcode cases and `trap` for undefined-opcode
cases. `flags_mask` selects the F bits whose resulting values UM0050 defines:
215 (`0xD7`, the documented six flags) for every case except OTIM/OTDM, which
use 66 (`0x42`, Z and N). Every generated initial state adds the reset-state
defaults `"z180": {"itc": 1, "cbr": 0, "bbr": 0, "cbar": 240,
"sleeping": false}`. Its final state contains the reference transition's
resulting `z180` values; this makes SLP state and TRAP/MMU control state
observable.

MMU cases use `kind: "mmu"`, the same common metadata and initial/final state,
`z180` in both states, and exactly 16 ordered probes:

```json
{
  "kind": "mmu",
  "mmu_probes": [
    {"logical": 0, "expected_physical": 0, "value": 17}
  ]
}
```

Each MMU probe's logical address belongs to a distinct logical 4K page; the
test harness places `value` at `expected_physical`, performs the logical read,
and requires that value. Generator scripts live in `tools/reference/` and
must be re-runnable with pinned explicit seeds so the suite can be regenerated
and extended deterministically.

## Appendix D — qns compat surface (Phase 8 checklist)

From `C:\Users\Q\code\qns\qns\cpu.py`, the shim must provide: constructor
kwargs (`clock, mem_read, mem_write, io_read, io_write, serial_rx,
serial_tx, csio_rx, csio_tx`); `reset() / step() / run(cycles) /
cycle_count / get_reg / pc / instruction_pc / sp / halted /
set_irq(line, state)` with the same `PC..IY` index constants and
`IRQ0/1/2`, `CLEAR/ASSERT`; `cbr / bbr / cbar`; `watch_pc(addr|None) /
pc_watch_count / pc_watch_cycle / pc_watch_cbar`; `asci_debug_state(ch)`
returning the documented dict keys (equivalents where meaningful, zeros
where the old debug counters have no analogue — list each in
qns-migration.md); `reset_asci_debug()` as a no-op or event-ring reset.

## Appendix E — Review protocol (for Q and the reviewing model)

After each phase the reviewer checks, in order:
1. PROGRESS.md gate block: are the exact gate commands present with raw
   output? (No output ⇒ phase not done, regardless of code.)
2. `git log --oneline`: task-per-commit discipline, messages match plan
   numbering.
3. verification-log.md: new facts logged for everything numeric added this
   phase (spot-check 3 against the UM).
4. Re-run the gate commands locally; outputs must match PROGRESS.md.
5. Negative control (`--sabotage-ld`) still detects at every gate.
6. Clean-room spot check: `git log --stat` shows no files copied from other
   emulators; no suspicious verbatim tables (cycle tables must cite UM
   table numbers).

## Appendix F — Property-based testing catalog

Two engines, two layers. Rust `proptest` tests live inside `z180-core` and
run at every `cargo test` from the phase that introduces them. Python
`hypothesis` tests live in `tools/reference/` + z180-py and come alive in
Phase 8 (strategies authored in Phase 1). Fixed-example unit tests remain
mandatory; properties are in ADDITION, not instead.

Rust proptest properties (add in the phase shown):
- P2: ALU algebra — for random a,b: ADD/SUB/ADC/SBC results and C/H/V
  flags match a widened-arithmetic reference computed independently in the
  test (u16/i16 math, not the implementation's own path). DAA: for random
  A/F, DAA output equals a from-first-principles BCD-correction reference
  function written in the test file.
- P2: PUSH then POP round-trips any register pair; EX/EXX are involutions.
- P3: prefix decode totality — for ANY 4-byte sequence, `step()` returns
  without panic and consumes 1..=4 bytes (traps count as consumed per UM).
- P4: cycle monotonicity — for any instruction, `step()` returns > 0
  cycles and `cycle_count` strictly increases.
- P5: MMU — for any CBR/BBR/CBAR triple and any logical address,
  `mmu_translate` equals the closed-form 3.3 formula; translation array
  and formula never disagree.
- P7: save/load — for any reachable machine state S (generated by running
  a random program for a random cycle count): load(save(S)) then run(N) ≡
  run(N) directly, for random N. Disassembler totality: `dis` never
  panics on any byte sequence and its reported lengths tile the buffer.

Hypothesis stateful testing (Phase 8, after peripherals exist):
- `RuleBasedStateMachine` per peripheral driving the z180-py binding with
  random interleavings of: internal-register reads/writes, `run(n)` for
  random n, rx pushes / tx pops, and pin changes. Invariants asserted
  after every rule, derived from UM0050 (NOT from z-core or an incumbent —
  the incumbent's ASCI is known-shaky, which is partly why z-core exists):
  - ASCI: RDRF never set before one full modeled frame time has elapsed
    since the push; OVRN implies RDRF was set at push time; TDRE cannot
    be observed 0 for longer than one frame time of `run`; STAT reserved
    bits read as documented constants.
  - PRT: TIF sets only at computed underflow cycles (± the documented
    read-sequence semantics); reload occurs on the documented edge;
    disabled timer never moves.
  - DMA: DE0/DE1 can only become 1 through the documented DWE protocol;
    BCR reaching 0 with DIE set raises exactly one interrupt; byte counts
    conserved (source bytes read == dest bytes written == initial BCR).
  These machines are the hardening layer for exactly the register-protocol
  subtleties that bit qns historically.

Shrinking discipline (both engines): every counterexample gets (1) pinned
as a permanent regression (`@example` / `proptest-regressions` file,
committed), (2) adjudicated against UM0050 in verification-log.md before
any code changes, (3) fixed. Never delete a pinned regression.

## Appendix G — Glossary

- **SST**: SingleStepTests JSON suite (per-opcode state-transition tests).
- **UM0050**: Zilog Z8018x family MPU User Manual (the authority).
- **Reference model**: first-party pure Python transitions independently
  authored from verified UM0050 facts; never imports z-core or emulator code.
- **Optional incumbent**: an existing emulator accessed only as a black box,
  never a required authority or gate.
- **Internal I/O window**: the 64-byte on-chip register file (3.6).
- **TRAP**: Z180 undefined-opcode interrupt (3.4).
- **Gate**: a phase's exit criteria — commands + pasted output.

---
End of plan.
