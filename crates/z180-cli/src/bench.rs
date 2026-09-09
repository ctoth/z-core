use anyhow::{Result, ensure};
use clap::Args;
use serde::Serialize;
use std::{convert::Infallible, hint::black_box, time::Instant};
use z180_core::{HostBus, MachineConfig, RegionDef, RegionKind, Z180};
use z180_replay::{Options, Timeline};

#[derive(Debug, Args)]
pub struct BenchArgs {
    #[arg(long, default_value_t = 7)]
    samples: usize,
    #[arg(long, default_value_t = 1_000_000)]
    cycles: u32,
}
struct Bus;
impl HostBus for Bus {
    type Error = Infallible;
    fn mem_read(&mut self, address: u32) -> Result<u8, Infallible> {
        Ok(black_box(address as u8))
    }
    fn mem_write(&mut self, address: u32, value: u8) -> Result<(), Infallible> {
        black_box((address, value));
        Ok(())
    }
    fn io_read(&mut self, port: u16) -> Result<u8, Infallible> {
        Ok(black_box(port as u8))
    }
    fn io_write(&mut self, port: u16, value: u8) -> Result<(), Infallible> {
        black_box((port, value));
        Ok(())
    }
}
#[derive(Serialize)]
struct Measurement {
    workload: &'static str,
    cycles_per_second: Vec<f64>,
    median_cycles_per_second: f64,
    retained_replay_bytes: usize,
}
fn config(workload: &str) -> MachineConfig {
    let mut program = match workload {
        "callbacks" => vec![
            0x3a, 0x00, 0x80, 0x32, 0x01, 0x80, 0xdb, 0x80, 0xd3, 0x80, 0xc3, 0, 0,
        ],
        "mmu" => vec![
            0x3e, 0xf1, 0xed, 0x39, 0x3a, 0x3e, 1, 0xed, 0x39, 0x39, 0x3a, 0, 0x10, 0x32, 1, 0x10,
            0xc3, 10, 0,
        ],
        "dma" => {
            let mut bytes = Vec::new();
            // Program the same memory-to-memory mode covered by core DMA tests.
            for (register, value) in [
                (0x20, 0),
                (0x21, 0x20),
                (0x22, 0),
                (0x23, 0),
                (0x24, 0x30),
                (0x25, 0),
                (0x26, 0),
                (0x27, 1),
                (0x31, 2),
                (0x32, 0x80),
                (0x30, 0x64),
            ] {
                bytes.extend_from_slice(&[0x3e, value, 0xed, 0x39, register]);
            }
            bytes.extend_from_slice(&[0xc3, 0, 0]);
            bytes
        }
        _ => vec![
            0x21, 0, 0x20, 0x34, 0x7e, 0x87, 0x0c, 0x80, 0x77, 0xc3, 3, 0,
        ],
    };
    program.resize(4096, 0);
    let mut regions = vec![
        RegionDef {
            base: 0,
            size: 4096,
            kind: RegionKind::Rom(program),
        },
        RegionDef {
            base: 4096,
            size: 0xf000,
            kind: RegionKind::Ram,
        },
    ];
    if workload == "callbacks" {
        regions[1].size = 0x7000;
        regions.push(RegionDef {
            base: 0x8000,
            size: 0x8000,
            kind: RegionKind::External,
        });
    }
    MachineConfig {
        regions,
        ..MachineConfig::default()
    }
}
pub fn run(args: BenchArgs) -> Result<()> {
    ensure!(
        (3..=101).contains(&args.samples) && args.cycles > 0,
        "use 3 through 101 samples and a positive cycle budget"
    );
    let mut measurements = Vec::new();
    for workload in [
        "instruction-mix",
        "mmu",
        "dma",
        "callbacks",
        "tracing",
        "replay",
    ] {
        let mut rates = Vec::new();
        let mut retained = 0;
        for sample in 0..=args.samples {
            let (elapsed, cycles) = if workload == "replay" {
                let mut timeline = Timeline::new(config(workload), Bus, Options::default())?;
                timeline.start()?;
                let start = Instant::now();
                let cycles = timeline.try_run(args.cycles)?;
                let elapsed = start.elapsed();
                retained = timeline.retained_bytes();
                (elapsed, cycles)
            } else {
                let mut cpu = Z180::new(config(workload), Bus)?;
                if workload == "tracing" {
                    cpu.set_insn_trace(Some(4096));
                }
                let start = Instant::now();
                let cycles = cpu.run(args.cycles);
                (start.elapsed(), cycles)
            };
            if sample != 0 {
                rates.push(f64::from(cycles) / elapsed.as_secs_f64());
            }
        }
        let mut sorted = rates.clone();
        sorted.sort_by(f64::total_cmp);
        measurements.push(Measurement {
            workload,
            median_cycles_per_second: sorted[sorted.len() / 2],
            cycles_per_second: rates,
            retained_replay_bytes: retained,
        });
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({ "schema": 1, "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "cycle_budget": args.cycles, "samples": args.samples, "warmup_samples": 1, "measurements": measurements })
        )?
    );
    Ok(())
}
