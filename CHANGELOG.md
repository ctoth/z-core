# Changelog

All notable changes to z-core are documented in this file.

## 0.1.0 - 2026-07-21

Initial release.

### Core

- Implemented the documented Z80180 instruction set, Z180-added instructions,
  undefined-opcode TRAP behavior, instruction timing, DCNTL wait states, and
  Z80180/Z8S180 variants.
- Added the Z180 MMU, configurable 20- through 24-bit physical page table,
  core-owned RAM and ROM, external bus regions, remapping, and optional
  board-level address mapping.
- Implemented the internal-I/O register file, interrupt controller, PRT, FRC,
  ASCI, CSI/O, and both DMA channels with deterministic cycle advancement.
- Added memory watches, I/O and interrupt events, PC watches, instruction
  traces, versioned save states, and table-driven disassembly.

### Host APIs

- Added `z180-cli` ROM execution, disassembly, SST conformance, and CP/M ZEX
  commands.
- Added the abi3 `z180` Python package with core-owned zero-copy RAM and the
  callback-backed `z180.compat.Z180` migration surface for qns.
- Added WebAssembly packages for browsers and Node.js, strict TypeScript
  declarations, a Node Fibonacci/performance smoke, and a framework-free
  browser demo.

### Verification

- Added pinned shared-Z80 SST and ZEX assets plus a first-party,
  UM0050-derived Z180 corpus and Hypothesis differential properties.
- Added deterministic peripheral, interrupt, timing, MMU, event, trace,
  disassembler, and save-state tests across Windows and Linux CI.
- Documented the clean-room fact trail, as-built architecture, and the direct
  qns internal-memory migration.

### Packaging note

- The WebAssembly package remains unpublished under the build placeholder
  `@zcore/z180` until Q selects its final package name.
