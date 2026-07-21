use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use z180_core::{Event, HostBus, MachineConfig, Reg, RegionDef, RegionKind, Z180};

const PROGRAM_BASE: u16 = 0x0100;
const BDOS_ENTRY: u16 = 0x0005;

#[derive(Debug, Args)]
pub struct ZexArgs {
    /// CP/M .COM image to execute.
    pub file: PathBuf,
}

#[derive(Default)]
struct NullBus;

impl HostBus for NullBus {
    fn mem_read(&mut self, _phys: u32) -> u8 {
        0xff
    }

    fn mem_write(&mut self, _phys: u32, _value: u8) {}

    fn io_read(&mut self, _port: u16) -> u8 {
        0xff
    }

    fn io_write(&mut self, _port: u16, _value: u8) {}
}

pub fn run(args: ZexArgs) -> Result<()> {
    let program = fs::read(&args.file)
        .with_context(|| format!("failed to read CP/M image {}", args.file.display()))?;
    let stdout = io::stdout();
    let mut output = stdout.lock();
    run_program(&program, &mut output)
}

fn run_program(program: &[u8], output: &mut impl Write) -> Result<()> {
    ensure!(
        program.len() <= usize::from(u16::MAX - PROGRAM_BASE) + 1,
        "CP/M image is too large for the 0x0100 load address"
    );

    let config = MachineConfig {
        regions: vec![RegionDef {
            base: 0,
            size: 0x1_0000,
            kind: RegionKind::Ram,
        }],
        ..MachineConfig::default()
    };
    let mut cpu =
        Z180::new(config, NullBus).context("flat 64K ZEX machine configuration is invalid")?;
    for (offset, value) in program.iter().copied().enumerate() {
        cpu.mem_poke(u32::from(PROGRAM_BASE) + offset as u32, value);
    }
    cpu.set_reg(Reg::PC, PROGRAM_BASE);

    loop {
        let pc = cpu.reg(Reg::PC);
        if pc == 0 {
            return Ok(());
        }
        if pc == BDOS_ENTRY {
            service_bdos(&mut cpu, output)?;
            continue;
        }

        let opcode = cpu.mem_peek(u32::from(pc));
        let step_cycles = cpu.step();
        if let Some(Event::Trap {
            cycle,
            pc,
            opcode,
            len,
        }) = cpu.drain_events().into_iter().next()
        {
            bail!("Z180 TRAP at cycle {cycle}, PC={pc:04x}: opcode={opcode:02x?}, len={len}");
        }
        if step_cycles == 0 {
            writeln!(output, "unimplemented opcode at PC={pc:04x}: {opcode:02x}")?;
            return Ok(());
        }
    }
}

fn service_bdos(cpu: &mut Z180<NullBus>, output: &mut impl Write) -> Result<()> {
    let function = cpu.reg(Reg::BC).to_le_bytes()[0];
    match function {
        2 => output.write_all(&[cpu.reg(Reg::DE).to_le_bytes()[0]])?,
        9 => {
            let mut address = cpu.reg(Reg::DE);
            let mut terminated = false;
            for _ in 0..=u16::MAX {
                let value = cpu.mem_peek(u32::from(address));
                if value == b'$' {
                    terminated = true;
                    break;
                }
                output.write_all(&[value])?;
                address = address.wrapping_add(1);
            }
            ensure!(terminated, "BDOS function 9 string has no '$' terminator");
        }
        _ => bail!("unsupported BDOS function C={function}"),
    }

    let sp = cpu.reg(Reg::SP);
    let low = cpu.mem_peek(u32::from(sp));
    let high = cpu.mem_peek(u32::from(sp.wrapping_add(1)));
    cpu.set_reg(Reg::SP, sp.wrapping_add(2));
    cpu.set_reg(Reg::PC, u16::from_le_bytes([low, high]));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undefined_opcode_trap_is_an_error() {
        let mut output = Vec::new();
        let error = run_program(&[0xdd], &mut output).expect_err("undefined opcode must fail");

        assert_eq!(
            error.to_string(),
            "Z180 TRAP at cycle 0, PC=0100: opcode=[dd, 00, 00], len=2"
        );
        assert!(output.is_empty());
    }

    #[test]
    fn clean_warm_boot_is_success() {
        let mut output = Vec::new();

        run_program(&[0xc3, 0x00, 0x00], &mut output).expect("JP 0000h is a clean warm boot");

        assert!(output.is_empty());
    }

    #[test]
    fn bdos_console_output_returns_to_the_stacked_address() {
        let config = MachineConfig {
            regions: vec![RegionDef {
                base: 0,
                size: 0x1_0000,
                kind: RegionKind::Ram,
            }],
            ..MachineConfig::default()
        };
        let mut cpu = Z180::new(config, NullBus).expect("valid flat machine");
        cpu.set_reg(Reg::BC, 2);
        cpu.set_reg(Reg::DE, u16::from(b'Z'));
        cpu.set_reg(Reg::SP, 0x8000);
        cpu.mem_poke(0x8000, 0x34);
        cpu.mem_poke(0x8001, 0x12);
        let mut output = Vec::new();

        service_bdos(&mut cpu, &mut output).expect("BDOS function 2 succeeds");

        assert_eq!(output, b"Z");
        assert_eq!(cpu.reg(Reg::PC), 0x1234);
        assert_eq!(cpu.reg(Reg::SP), 0x8002);
    }

    #[test]
    fn bdos_string_output_stops_at_dollar() {
        let config = MachineConfig {
            regions: vec![RegionDef {
                base: 0,
                size: 0x1_0000,
                kind: RegionKind::Ram,
            }],
            ..MachineConfig::default()
        };
        let mut cpu = Z180::new(config, NullBus).expect("valid flat machine");
        cpu.set_reg(Reg::BC, 9);
        cpu.set_reg(Reg::DE, 0x4000);
        cpu.set_reg(Reg::SP, 0x8000);
        cpu.mem_poke(0x4000, b'O');
        cpu.mem_poke(0x4001, b'K');
        cpu.mem_poke(0x4002, b'$');
        cpu.mem_poke(0x8000, 0x00);
        cpu.mem_poke(0x8001, 0x01);
        let mut output = Vec::new();

        service_bdos(&mut cpu, &mut output).expect("BDOS function 9 succeeds");

        assert_eq!(output, b"OK");
        assert_eq!(cpu.reg(Reg::PC), 0x0100);
        assert_eq!(cpu.reg(Reg::SP), 0x8002);
    }
}
