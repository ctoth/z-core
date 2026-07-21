# UM0050 verification log

| Verified fact | UM0050 section or table | Date |
|---|---|---|
| Manual title: *Z8018x Family MPU User Manual* | Cover, UM005004-0918 | 2026-07-20 |
| Manual revision: UM005004-0918 (September 2018) | Cover and document footer | 2026-07-20 |
| Official source: https://www.zilog.com/docs/z180/um0050.pdf | Zilog documentation listing for UM0050 | 2026-07-20 |
| CPU register set comprises AF, BC, DE, HL, alternate AF/BC/DE/HL, I, R, IX, IY, SP, and PC | Software Architecture, CPU Registers, Figure 74, pp. 175–177 | 2026-07-20 |
| R bits 0–6 increment on every CPU opcode-fetch (M1) cycle; R resets to 00h | Software Architecture, R Counter (R), p. 177 | 2026-07-20 |
| Register codes 000–101 and 111 select B, C, D, E, H, L, and A respectively | Instruction Set, Table 32, p. 207 | 2026-07-20 |
| The 01dddsss block encodes LD r,r', LD r,(HL), and LD (HL),r; these operations do not alter flags | Data Transfer Instructions, Table 41, pp. 222–223 | 2026-07-20 |
| NOP is opcode 00h and HALT is opcode 76h; neither alters flags | Special Control Instructions, Table 47, p. 235; Op Code Map, Table 48, p. 247 | 2026-07-20 |
| Hardware RESET restarts execution at logical and physical address 00000h | Operation Modes, RESET Timing, Figure 15, p. 25 | 2026-07-20 |
| I resets to 00h | Software Architecture, Interrupt Vector Register (I), p. 177 | 2026-07-20 |
| Free choice: CPU registers without a UM0050 reset value are initialized to 0000h for deterministic emulation | UM0050 specifies PC/I/R reset values but no reset value for the remaining CPU registers; z-core deterministic policy | 2026-07-20 |
| IEF1 and IEF2 reset to 0 | Interrupt Sources During RESET, p. 83 | 2026-07-20 |
| DD/FD are defined only where Table 48 substitutes IX/IY for an HL or (HL) operand; JP (HL) is substituted, while prefixed EX DE,HL is illegal | Op Code Map, Table 48 notes, pp. 247–248 | 2026-07-20 |
| CB opcodes 30h–37h have no defined SLL operation on Z80180 | Op Code Map, Table 49, p. 249 | 2026-07-20 |
| The defined ED opcode set is exactly the populated cells in the Z80180 ED map; blank ED cells are undefined | Op Code Map, Table 50, p. 250; TRAP Interrupt, pp. 70–72 | 2026-07-20 |
| An undefined first, second, or third opcode fetch invokes TRAP; UFO distinguishes second/third-opcode cases | TRAP Interrupt and Figures 32–33, pp. 70–72 | 2026-07-20 |
