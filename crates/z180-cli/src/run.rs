use std::{fmt::Write as _, fs, io, io::Write, path::PathBuf};

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use serde::Deserialize;
use z180_core::{
    Event, HostBus, MachineConfig, RegionDef, RegionKind, Variant, Z180, disassemble_one,
};

#[derive(Debug, Args)]
#[command(after_long_help = r#"machine.toml format:
  clock_hz = 18_432_000
  variant = "z80180"  # or "z8s180"

  [[regions]]
  kind = "rom"        # exactly one; receives the ROM image
  base = 0x00000
  size = 0x10000       # must equal the ROM image size

  [[regions]]
  kind = "ram"        # "external" is also supported
  base = 0x10000
  size = 0x10000"#)]
pub struct RunArgs {
    /// Raw ROM image to execute.
    pub rom: PathBuf,
    /// Stop after consuming at least this many CPU cycles.
    #[arg(long)]
    pub cycles: u64,
    /// Print each executed instruction.
    #[arg(long)]
    pub trace: bool,
    /// TOML machine configuration.
    #[arg(long)]
    pub config: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    clock_hz: u32,
    variant: FileVariant,
    regions: Vec<FileRegion>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum FileVariant {
    Z80180,
    Z8S180,
}

impl From<FileVariant> for Variant {
    fn from(variant: FileVariant) -> Self {
        match variant {
            FileVariant::Z80180 => Self::Z80180,
            FileVariant::Z8S180 => Self::Z8S180,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum FileRegion {
    Rom { base: u32, size: u32 },
    Ram { base: u32, size: u32 },
    External { base: u32, size: u32 },
}

impl FileRegion {
    fn is_rom(&self) -> bool {
        matches!(self, Self::Rom { .. })
    }
}

#[derive(Default)]
struct BareBus;

impl HostBus for BareBus {
    fn mem_read(&mut self, _phys: u32) -> u8 {
        0xff
    }

    fn mem_write(&mut self, _phys: u32, _value: u8) {}

    fn io_read(&mut self, _port: u16) -> u8 {
        0xff
    }

    fn io_write(&mut self, _port: u16, _value: u8) {}
}

pub fn run(args: RunArgs) -> Result<()> {
    let config_text = fs::read_to_string(&args.config)
        .with_context(|| format!("failed to read machine config {}", args.config.display()))?;
    let file_config: FileConfig = toml::from_str(&config_text)
        .with_context(|| format!("failed to parse machine config {}", args.config.display()))?;
    let rom = fs::read(&args.rom)
        .with_context(|| format!("failed to read ROM image {}", args.rom.display()))?;
    let machine_config = build_machine_config(file_config, rom)?;

    let stdout = io::stdout();
    let mut output = stdout.lock();
    run_machine(machine_config, args.cycles, args.trace, &mut output)
}

fn build_machine_config(config: FileConfig, rom: Vec<u8>) -> Result<MachineConfig> {
    ensure!(config.clock_hz != 0, "clock_hz must be greater than zero");
    let rom_regions = config
        .regions
        .iter()
        .filter(|region| region.is_rom())
        .count();
    ensure!(
        rom_regions == 1,
        "machine config must contain exactly one ROM region, found {rom_regions}"
    );

    let mut rom = Some(rom);
    let mut regions = Vec::with_capacity(config.regions.len());
    for region in config.regions {
        let definition = match region {
            FileRegion::Rom { base, size } => {
                let data = rom.take().expect("exactly one ROM region was validated");
                ensure!(
                    data.len() == size as usize,
                    "ROM image size {:#x} does not match configured ROM region size {size:#x}",
                    data.len()
                );
                RegionDef {
                    base,
                    size,
                    kind: RegionKind::Rom(data),
                }
            }
            FileRegion::Ram { base, size } => RegionDef {
                base,
                size,
                kind: RegionKind::Ram,
            },
            FileRegion::External { base, size } => RegionDef {
                base,
                size,
                kind: RegionKind::External,
            },
        };
        regions.push(definition);
    }

    Ok(MachineConfig {
        clock_hz: config.clock_hz,
        variant: config.variant.into(),
        regions,
        ..MachineConfig::default()
    })
}

fn run_machine(
    config: MachineConfig,
    cycles: u64,
    trace: bool,
    output: &mut impl Write,
) -> Result<()> {
    let mut cpu = Z180::new(config, BareBus).context("machine configuration is invalid")?;
    if trace {
        cpu.set_insn_trace(Some(1));
    }

    while cpu.cycle_count() < cycles {
        let step_cycles = cpu.step();
        if trace {
            for entry in cpu.drain_insn_trace() {
                writeln!(output, "{}", render_trace(&entry))?;
            }
        }
        if let Some((cycle, pc, opcode, len)) = cpu.drain_events().into_iter().find_map(|event| {
            if let Event::Trap {
                cycle,
                pc,
                opcode,
                len,
            } = event
            {
                Some((cycle, pc, opcode, len))
            } else {
                None
            }
        }) {
            bail!("Z180 TRAP at cycle {cycle}, PC={pc:04x}: opcode={opcode:02x?}, len={len}");
        }
        if step_cycles == 0 {
            bail!(
                "CPU made no cycle progress at cycle {} before reaching requested cycle {cycles}",
                cpu.cycle_count()
            );
        }
    }

    Ok(())
}

fn render_trace(entry: &z180_core::TraceEntry) -> String {
    let len = usize::from(entry.len);
    let mut encoded = String::new();
    for (index, byte) in entry.bytes[..len].iter().enumerate() {
        if index != 0 {
            encoded.push(' ');
        }
        write!(encoded, "{byte:02X}").expect("writing to a String cannot fail");
    }
    let instruction = disassemble_one(&entry.bytes[..len], entry.pc)
        .expect("an executed trace entry always contains one instruction");
    format!(
        "{:012}  {:04X}  {:06X}  {encoded:<11}  {}",
        entry.cycle, entry.pc, entry.phys_pc, instruction.text
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
clock_hz = 18_432_000
variant = "z8s180"

[[regions]]
kind = "rom"
base = 0x00000
size = 0x01000

[[regions]]
kind = "ram"
base = 0x01000
size = 0x01000

[[regions]]
kind = "external"
base = 0x02000
size = 0x01000
"#;

    fn parse_config(text: &str, rom: Vec<u8>) -> Result<MachineConfig> {
        build_machine_config(toml::from_str(text)?, rom)
    }

    #[test]
    fn config_maps_clock_variant_and_all_region_kinds() {
        let rom = vec![0; 0x1000];
        let config = parse_config(CONFIG, rom.clone()).expect("config must be valid");

        assert_eq!(config.clock_hz, 18_432_000);
        assert_eq!(config.variant, Variant::Z8S180);
        assert_eq!(
            config.regions,
            vec![
                RegionDef {
                    base: 0,
                    size: 0x1000,
                    kind: RegionKind::Rom(rom),
                },
                RegionDef {
                    base: 0x1000,
                    size: 0x1000,
                    kind: RegionKind::Ram,
                },
                RegionDef {
                    base: 0x2000,
                    size: 0x1000,
                    kind: RegionKind::External,
                },
            ]
        );
    }

    #[test]
    fn config_rejects_unknown_fields_and_invalid_rom_bindings() {
        let unknown = CONFIG.replace("clock_hz = 18_432_000", "clock_hz = 18_432_000\nclock = 1");
        assert!(toml::from_str::<FileConfig>(&unknown).is_err());

        let no_rom = CONFIG.replace("kind = \"rom\"", "kind = \"ram\"");
        let error = parse_config(&no_rom, vec![0; 0x1000]).expect_err("a ROM mapping is required");
        assert_eq!(
            error.to_string(),
            "machine config must contain exactly one ROM region, found 0"
        );

        let error = parse_config(CONFIG, vec![0; 1]).expect_err("ROM sizes must match");
        assert_eq!(
            error.to_string(),
            "ROM image size 0x1 does not match configured ROM region size 0x1000"
        );
    }

    #[test]
    fn trace_reports_cycle_logical_and_physical_addresses_and_instruction() {
        let config = parse_config(CONFIG, vec![0; 0x1000]).expect("config must be valid");
        let mut output = Vec::new();

        run_machine(config, 12, true, &mut output).expect("ROM must execute");

        assert_eq!(
            String::from_utf8(output).expect("trace must be UTF-8"),
            concat!(
                "000000000000  0000  000000  00           NOP\n",
                "000000000006  0001  000001  00           NOP\n",
            )
        );
    }

    #[test]
    fn trap_stops_the_runner_with_the_faulting_instruction() {
        let mut rom = vec![0; 0x1000];
        rom[0] = 0xdd;
        let config = parse_config(CONFIG, rom).expect("config must be valid");
        let mut output = Vec::new();

        let error = run_machine(config, 100, false, &mut output)
            .expect_err("an undefined opcode must stop execution");

        assert_eq!(
            error.to_string(),
            "Z180 TRAP at cycle 0, PC=0000: opcode=[dd, 00, 00], len=2"
        );
        assert!(output.is_empty());
    }

    #[test]
    fn sleeping_cpu_reports_that_the_cycle_budget_cannot_be_reached() {
        let mut rom = vec![0; 0x1000];
        rom[..2].copy_from_slice(&[0xed, 0x76]);
        let config = parse_config(CONFIG, rom).expect("config must be valid");
        let mut output = Vec::new();

        let error = run_machine(config, 100, false, &mut output)
            .expect_err("a sleeping CPU cannot complete the cycle budget");

        assert_eq!(
            error.to_string(),
            "CPU made no cycle progress at cycle 14 before reaching requested cycle 100"
        );
        assert!(output.is_empty());
    }
}
