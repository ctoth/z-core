mod model;
use model::*;
mod input;
use input::*;
mod execute;
use execute::*;
mod compare;
use compare::*;
mod report;
use report::*;
mod policy;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};
use z180_core::{Event, HostBus, MachineConfig, Reg, RegionDef, RegionKind, WatchKind, Z180};

use policy::{OnlyFilter, exclusion_reason, opcode_bytes};

const FLAG_COMPARE_MASK: u8 = !0x28;

#[derive(Debug, Args)]
pub(crate) struct SstArgs {
    /// Directory containing `SingleStepTests` v1 JSON files.
    #[arg(long)]
    dir: PathBuf,

    /// Comma-separated opcode files or inclusive main-page ranges.
    #[arg(long)]
    only: Option<String>,

    /// Output format for the per-file and aggregate report.
    #[arg(long, value_enum, default_value_t = ReportFormat::Text)]
    report: ReportFormat,

    /// Ignore R only when diagnosing disputed M1 accounting; gates never use this.
    #[arg(long)]
    ignore_r: bool,

    /// Deliberately reverse LD r,r' operands to prove the harness detects errors.
    #[arg(long, hide = true)]
    sabotage_ld: bool,

    /// Print case counts per opcode file and special generated family.
    #[arg(long)]
    census: bool,
}

pub(crate) fn run(args: SstArgs) -> Result<()> {
    let filter = OnlyFilter::parse(args.only.as_deref())?;
    let files = json_files(&args.dir)?;
    let mut selected_files = 0_usize;
    let mut report = RunReport::new();

    for path in files {
        let stem = file_stem(&path)?;
        if !filter.matches(&stem) {
            continue;
        }
        selected_files += 1;

        let cases = load_cases(&path)?;
        let generated_kind = validate_generated_cases(&stem, &cases)?;
        if args.census {
            report.census.push(CensusEntry {
                family: stem.clone(),
                cases: cases.len(),
            });
        }

        if let Some(kind) = generated_kind {
            match kind {
                CaseKind::Instruction | CaseKind::Trap => {
                    report.push(run_file(stem, cases, args.ignore_r, args.sabotage_ld)?);
                }
                CaseKind::Mmu => report.push(run_mmu_file(stem, cases, args.ignore_r)?),
            }
            continue;
        }

        let opcodes = opcode_bytes(&stem)?;
        if let Some(reason) = exclusion_reason(&opcodes) {
            report.excluded.push(ExcludedFile {
                file: stem,
                reason: reason.to_owned(),
            });
            continue;
        }

        if !implemented(&opcodes) {
            report.push(FileReport {
                file: stem,
                pass: 0,
                fail: 0,
                unimplemented: cases.len(),
                failures: Vec::new(),
            });
            continue;
        }

        report.push(run_file(stem, cases, args.ignore_r, args.sabotage_ld)?);
    }

    if selected_files == 0 {
        bail!("no JSON opcode files matched {}", args.dir.display());
    }

    match args.report {
        ReportFormat::Text => print_text(&report),
        ReportFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
    }

    if report.fail != 0 {
        bail!("{} single-step test(s) failed", report.fail);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
