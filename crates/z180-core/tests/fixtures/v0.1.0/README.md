# z180-core v0.1.0 save-state fixture

`state-v4.bin` was produced from the released `v0.1.0` source at commit
`ca3af1886151481118be278d02eeb600199817b7`, not from the current checkout.

| Field | Value |
| --- | --- |
| z180-core release | `v0.1.0` |
| save-state version byte | `4` |
| byte length | `66050` |
| SHA-256 | `29F862CE1E7590C64F0A5628CEC65F2BC79D8BC4E18C49CFE13199BB099CF026` |
| generator | `generate.rs` in this directory |

The generator configures a Z8S180 with 64 KiB RAM, non-default registers,
asserted INT2 and DREQ1 pins, one memory watch, I/O and IRQ trace settings, a
PC watch, and an instruction-trace ring. It executes `LD A,5Ah` followed by
`LD (1234h),A`, leaving both retained debug events and instruction traces in
the serialized payload.

## Reproduction

1. Export the repository at tag `v0.1.0` into an isolated directory.
2. Copy `generate.rs` to
   `crates/z180-core/examples/generate_state_v4.rs` inside that export.
3. From the exported workspace, run:

   ```powershell
   cargo run -p z180-core --features state --example generate_state_v4 -- <output-path>
   ```

4. Verify the byte length and SHA-256 above.

`generate.rs` deliberately targets the released `HostBus` API and is retained
as provenance rather than compiled by the current workspace. The current
integration test loads the binary, checks its observable state, requires a
byte-identical re-save, resumes execution, and rejects an undeclared future
version atomically.
