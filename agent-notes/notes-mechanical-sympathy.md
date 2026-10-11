2026-10-01 inspection checkpoint
- Inspected reduce-memory-copies at 10016b6; no additional production edits. Experiments live in Temp/z-core-layout-review-da1daab0440b4ef59b1e5f39f13e979f.
- Compact ARM execution descriptors (20 -> 8 bytes), derived from authoritative tables, reduced matched app code/constants 63,228 -> 40,388 bytes; 96 core tests plus allocation/v4 compatibility tests pass.
- NOP disassembly measured 14 allocation/growth calls; save output has 48,662 spare capacity bytes in the 80 KiB guest case.
- Further leads: borrow replay export/checkpoints/actions; additive event/trace drain and save-buffer APIs. Full evidence and scope limits are in the experiment RESULTS.md.
