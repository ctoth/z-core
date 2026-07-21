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
| IN0 includes ED30 as the `g=110` flags-only form; ED00/08/10/18/20/28/38 load B/C/D/E/H/L/A, while OUT0 has only the seven register forms and ED31 is not documented | I/O Instructions, Table 46, pp. 231–232; ED op-code map, Table 50, p. 250 | 2026-07-20 |
| The flag-table symbols mean not affected (bullet), affected (up arrow), undefined (`X`), set (`S`), reset (`R`), parity (`P`), and overflow (`V`) | Instruction Set, flag notation, p. 209 | 2026-07-20 |
| MLT BC/DE/HL/SP multiplies the unsigned high and low bytes of the selected pair into that 16-bit pair and does not affect flags; encodings are ED4C/5C/6C/7C | CPU Control Instructions, Table 38, p. 213; ED op-code map, Table 50, p. 250 | 2026-07-20 |
| TST forms compute A AND register, (HL), or immediate data without changing either operand; S and Z follow the result, H is set, P/V is result parity, and N and C are reset | Exchange, Block Transfer, Search, and Test Instructions, Table 40, pp. 215–216 | 2026-07-20 |
| IN0 reads port 00m into the selected register, with ED30 changing flags only; S and Z follow the byte, H and N reset, P/V is parity, and C is unaffected | I/O Instructions, Table 46, p. 231 | 2026-07-20 |
| OUT0 writes the selected register to port 00m and does not affect flags | I/O Instructions, Table 46, p. 232 | 2026-07-20 |
| TSTIO reads port 00C and ANDs it with the immediate byte without changing the port; S and Z follow the result, H is set, P/V is parity, and N and C reset | I/O Instructions, Table 46, p. 233 | 2026-07-20 |
| OTIM and OTDM write (HL) to port 00C, adjust HL and C in the named direction, and decrement B; Z is defined by B becoming zero and N by the output byte's most-significant bit, while S, H, P/V, and C are marked affected without a resulting-value rule | I/O Instructions, Table 46, pp. 232–234, notes 5–6 | 2026-07-20 |
| Terminal OTIMR and OTDMR set Z, reset S/H/P/V/C, set N from the final output byte's most-significant bit, and repeat the corresponding transfer until B is zero | I/O Instructions, Table 46, pp. 232–234, notes 5–6 | 2026-07-20 |
| SLP is ED76, enters sleep, and does not affect flags | Special Control Instructions, Table 47, p. 235; ED op-code map, Table 50, p. 250 | 2026-07-20 |
| CBAR bits 7–4 are CA and bits 3–0 are BA; CBR and BBR are 8-bit 4 KiB physical bases; reset values are CBAR=F0h and CBR=BBR=00h | MMU, CBAR/CBR/BBR register descriptions, pp. 60–63 | 2026-07-20 |
| Logical pages below BA are Common Area 0 with physical base zero, pages from BA through the page before CA are Bank Area using BBR, and pages from CA upward are Common Area 1 using CBR; the selected 8-bit base is added to logical address bits 15–12 and bits 11–0 pass through | MMU, Figures 27–30, pp. 60–64 | 2026-07-20 |
| ITC bit 7 is TRAP, set only by an undefined opcode fetch and clearable by writing zero; bit 6 is read-only UFO; bits 2–0 are ITE2–ITE0; reset ITC is 01h | Interrupts, ITC register, pp. 67–68 | 2026-07-20 |
| TRAP is non-maskable, leaves IEF1 and IEF2 unchanged, stacks the current PC high byte at SP-1 and low byte at SP-2, and vectors to logical 0000h | Interrupts, Table 8 and TRAP Interrupt, pp. 69–72 | 2026-07-20 |
| For a second-opcode undefined fetch, UFO is zero and the invalid instruction begins at stacked PC minus one; for a third-opcode undefined fetch, UFO is one and it begins at stacked PC minus two | Interrupts, ITC UFO description and TRAP Interrupt, pp. 68, 70–72 | 2026-07-20 |
