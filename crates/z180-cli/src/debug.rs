use std::{
    convert::Infallible,
    fs,
    io::{self, BufRead, Write},
    path::PathBuf,
};

use anyhow::{Result, anyhow, bail, ensure};
use clap::Args;
use z180_core::{
    Event, HostBus, MachineConfig, Reg, RegionDef, RegionKind, WatchKind, disassemble_one,
};
use z180_replay::{Drained, Mode, Options, Output, Timeline};

#[derive(Debug, Args)]
pub struct DebugArgs {
    /// ROM at address zero; omit to use the included RAM counter sample.
    #[arg(long, conflicts_with = "replay")]
    rom: Option<PathBuf>,
    /// Open an exported recording in playback mode.
    #[arg(long)]
    replay: Option<PathBuf>,
    /// Stop recording at this many retained MiB (one operation may exceed it).
    #[arg(long, default_value_t = 64)]
    memory_mib: usize,
}

struct Board;
impl HostBus for Board {
    type Error = Infallible;
    fn mem_read(&mut self, _: u32) -> Result<u8, Infallible> {
        Ok(0xff)
    }
    fn mem_write(&mut self, _: u32, _: u8) -> Result<(), Infallible> {
        Ok(())
    }
    fn io_read(&mut self, _: u16) -> Result<u8, Infallible> {
        Ok(0xff)
    }
    fn io_write(&mut self, _: u16, _: u8) -> Result<(), Infallible> {
        Ok(())
    }
}

const SAMPLE: &[u8] = &[
    0x31, 0x00, 0xf0, 0x3e, 0x00, 0x3c, 0x32, 0x00, 0x10, 0x18, 0xfa,
];
const HELP: &str = "step [count] | run [max-attempts] | back [count] | seek attempt\nregs | mem physical [length] | dis | mmu | break logical|off | watch physical|off\nfind-write physical [from-attempt] | export path | history | help | quit\nAddresses accept decimal or 0x-prefixed hex. Rewinding enters playback; no live branching.";

fn make_timeline(rom: &[u8]) -> Result<Timeline<Board>> {
    ensure!(
        !rom.is_empty() && rom.len() <= 0xff000,
        "ROM must leave at least one RAM page in the first MiB"
    );
    let size = rom.len().div_ceil(4096) * 4096;
    let mut data = vec![0; size];
    data[..rom.len()].copy_from_slice(rom);
    let mut timeline = Timeline::new(
        MachineConfig {
            event_capacity: 1024,
            regions: vec![
                RegionDef {
                    base: 0,
                    size: size as u32,
                    kind: RegionKind::Rom(data),
                },
                RegionDef {
                    base: size as u32,
                    size: (65536_usize.max(size + 4096) - size) as u32,
                    kind: RegionKind::Ram,
                },
            ],
            ..MachineConfig::default()
        },
        Board,
        Options {
            checkpoint_interval_attempts: 256,
            max_checkpoints: 64,
        },
    )?;
    timeline
        .setup()?
        .add_mem_watch(0, 1 << 24, WatchKind::Write);
    timeline.setup()?.set_reg(Reg::SP, 0xfffe);
    timeline.start()?;
    Ok(timeline)
}

pub fn run(args: DebugArgs) -> Result<()> {
    let limit = args
        .memory_mib
        .checked_mul(1024 * 1024)
        .filter(|&n| n > 0)
        .ok_or_else(|| anyhow!("memory budget must be positive and fit usize"))?;
    let mut timeline = if let Some(path) = args.replay {
        ensure!(
            fs::metadata(&path)?.len() <= 512 * 1024 * 1024,
            "recording exceeds 512 MiB import limit"
        );
        Timeline::import_recording(&fs::read(path)?, Board).map_err(anyhow::Error::msg)?
    } else {
        make_timeline(
            &args
                .rom
                .map(fs::read)
                .transpose()?
                .unwrap_or_else(|| SAMPLE.to_vec()),
        )?
    };
    timeline.set_byte_limit(Some(limit));
    session(&mut timeline, io::stdin().lock(), &mut io::stdout().lock())
}

fn number(value: Option<&str>, default: Option<u64>) -> Result<u64> {
    let Some(value) = value else {
        return default.ok_or_else(|| anyhow!("missing argument"));
    };
    Ok(if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)?
    } else {
        value.parse()?
    })
}

fn session(
    timeline: &mut Timeline<Board>,
    input: impl BufRead,
    output: &mut impl Write,
) -> Result<()> {
    writeln!(output, "Z180 debugger. {HELP}")?;
    let mut breakpoint = None;
    let mut watch = Some(0x1000_u32);
    for line in input.lines() {
        let line = line?;
        if line.trim() == "quit" {
            break;
        }
        if let Err(error) = command(timeline, &line, &mut breakpoint, &mut watch, output) {
            writeln!(output, "error: {error}")?;
        }
        output.flush()?;
    }
    Ok(())
}

fn command(
    t: &mut Timeline<Board>,
    line: &str,
    breakpoint: &mut Option<u16>,
    watch: &mut Option<u32>,
    out: &mut impl Write,
) -> Result<()> {
    let mut words = line.split_whitespace();
    match words.next().unwrap_or("") {
        "" => {}
        "help" => writeln!(out, "{HELP}")?,
        "step" | "run" => {
            let stepping = line.trim_start().starts_with("step");
            let count = number(words.next(), Some(if stepping { 1 } else { 100_000 }))?;
            ensure!(
                count <= 10_000_000,
                "maximum is 10000000 attempts per command"
            );
            for _ in 0..count {
                if !stepping && *breakpoint == Some(t.machine().reg(Reg::PC)) {
                    writeln!(out, "breakpoint")?;
                    break;
                }
                let before = t.position();
                if t.mode() == Mode::Playback && Some(before) == t.recorded_end() {
                    writeln!(out, "end of recording")?;
                    break;
                }
                let cycles = t.try_step()?;
                let mut stopped = false;
                if t.mode() == Mode::Live {
                    if let Drained::Events(events) = t.drain(Output::Events)? {
                        ensure!(!t.machine().events_lost(), "event history lost");
                        for event in events {
                            if matches!(event, Event::MemWrite { phys, .. } if Some(phys) == *watch)
                                || matches!(event, Event::Trap { .. })
                            {
                                writeln!(out, "{event:?}")?;
                                stopped = true;
                            }
                        }
                    }
                } else if let Some(address) = *watch
                    && let Some(hit) = t.find_first_write(before, t.position(), address, 1)?
                {
                    writeln!(out, "{:?}", hit.event)?;
                    stopped = true;
                }
                if stopped || cycles == 0 || t.machine().halted() || t.machine().sleeping() {
                    break;
                }
            }
            show(t, out)?;
        }
        "back" | "seek" => {
            let back = line.trim_start().starts_with("back");
            let value = number(words.next(), if back { Some(1) } else { None })?;
            let target = if back {
                t.position()
                    .attempted_steps
                    .checked_sub(value)
                    .ok_or_else(|| anyhow!("before recording"))?
            } else {
                value
            };
            let position = t
                .position_at_attempt(target)
                .ok_or_else(|| anyhow!("attempt outside recording"))?;
            t.seek(position)?;
            show(t, out)?;
        }
        "regs" | "dis" => show(t, out)?,
        "mem" => {
            let base = u32::try_from(number(words.next(), None)?)?;
            let length = u32::try_from(number(words.next(), Some(16))?)?;
            ensure!(
                length <= 4096 && base.checked_add(length).is_some_and(|end| end <= 1 << 24),
                "memory range outside physical space or exceeds 4096 bytes"
            );
            for address in base..base + length {
                writeln!(out, "{address:06X}: {:02X}", t.machine().mem_peek(address))?;
            }
        }
        "mmu" => {
            for logical in (0..=0xffff_u16).step_by(4096) {
                writeln!(
                    out,
                    "{logical:04X} -> {:06X}",
                    t.machine().mmu_translate(logical)
                )?;
            }
        }
        "break" => {
            let value = words.next();
            *breakpoint = if value == Some("off") {
                None
            } else {
                Some(u16::try_from(number(value, None)?)?)
            };
        }
        "watch" => {
            let value = words.next();
            *watch = if value == Some("off") {
                None
            } else {
                let v = u32::try_from(number(value, None)?)?;
                ensure!(v < 1 << 24, "watch outside physical space");
                Some(v)
            };
        }
        "find-write" => {
            let address = u32::try_from(number(words.next(), None)?)?;
            ensure!(address < 1 << 24, "address outside physical space");
            let from = number(words.next(), Some(0))?;
            let start = t
                .position_at_attempt(from)
                .ok_or_else(|| anyhow!("invalid start"))?;
            let end = t.recorded_end().ok_or_else(|| anyhow!("empty recording"))?;
            match t.find_first_write(start, end, address, 1)? {
                Some(hit) => writeln!(
                    out,
                    "attempt {}: {:?}; seek {} to inspect before it",
                    hit.attempted_step, hit.event, hit.attempted_step
                )?,
                None => writeln!(out, "no matching write")?,
            }
        }
        "export" => {
            let path = line.trim_start().strip_prefix("export").unwrap().trim();
            ensure!(!path.is_empty(), "missing output path");
            fs::write(path, t.export_recording().map_err(anyhow::Error::msg)?)?;
            writeln!(out, "recording exported")?;
        }
        "history" => writeln!(
            out,
            "mode={:?} position={:?} end={:?} retained_bytes={}",
            t.mode(),
            t.position(),
            t.recorded_end(),
            t.retained_bytes()
        )?,
        _ => bail!("unknown command; type help"),
    }
    Ok(())
}

fn show(t: &Timeline<Board>, out: &mut impl Write) -> Result<()> {
    let cpu = t.machine();
    writeln!(
        out,
        "attempt={} cycle={} mode={:?}",
        t.position().attempted_steps,
        cpu.cycle_count(),
        t.mode()
    )?;
    for reg in [
        Reg::PC,
        Reg::SP,
        Reg::AF,
        Reg::BC,
        Reg::DE,
        Reg::HL,
        Reg::IX,
        Reg::IY,
        Reg::AF2,
        Reg::BC2,
        Reg::DE2,
        Reg::HL2,
        Reg::IR,
    ] {
        write!(out, "{reg:?}={:04X} ", cpu.reg(reg))?;
    }
    writeln!(out)?;
    let pc = cpu.reg(Reg::PC);
    let bytes: Vec<_> = (0..4)
        .map(|i| cpu.mem_peek(cpu.mmu_translate(pc.wrapping_add(i))))
        .collect();
    if let Some(insn) = disassemble_one(&bytes, pc) {
        writeln!(out, "{pc:04X}: {}", insn.text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_watch_back_and_find_write_workflow() {
        let mut timeline = make_timeline(SAMPLE).unwrap();
        let mut output = Vec::new();
        session(
            &mut timeline,
            &b"run\nmem 0x1000 1\nback\nregs\nfind-write 0x1000\nmmu\nquit\n"[..],
            &mut output,
        )
        .unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(!text.contains("error:"), "{text}");
        assert!(text.contains("001000: 01"), "{text}");
        assert!(text.contains("PC=0006"), "{text}");
        assert!(text.contains("seek 3 to inspect before it"), "{text}");
        assert_eq!(timeline.machine().mem_peek(0x1000), 0);
    }
}
