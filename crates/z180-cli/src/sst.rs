mod policy;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};
use z180_core::{HostBus, MachineConfig, Reg, RegionDef, RegionKind, Z180, is_opcode_implemented};

use policy::{OnlyFilter, opcode_bytes, undefined_reason};

const FLAG_COMPARE_MASK: u8 = !0x28;

#[derive(Debug, Args)]
pub(crate) struct SstArgs {
    /// Directory containing SingleStepTests v1 JSON files.
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
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum ReportFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Deserialize)]
struct TestCase {
    name: String,
    initial: TestState,
    #[serde(rename = "final")]
    final_state: TestState,
}

#[derive(Debug, Deserialize)]
struct TestState {
    pc: u16,
    sp: u16,
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8,
    h: u8,
    l: u8,
    i: u8,
    r: u8,
    ix: u16,
    iy: u16,
    #[serde(rename = "af_")]
    af2: u16,
    #[serde(rename = "bc_")]
    bc2: u16,
    #[serde(rename = "de_")]
    de2: u16,
    #[serde(rename = "hl_")]
    hl2: u16,
    iff1: u8,
    iff2: u8,
    im: u8,
    ram: Vec<[u16; 2]>,
}

#[derive(Debug, Serialize)]
struct RunReport {
    files: Vec<FileReport>,
    excluded: Vec<ExcludedFile>,
    pass: usize,
    fail: usize,
    unimplemented: usize,
}

impl RunReport {
    fn new() -> Self {
        Self {
            files: Vec::new(),
            excluded: Vec::new(),
            pass: 0,
            fail: 0,
            unimplemented: 0,
        }
    }

    fn push(&mut self, file: FileReport) {
        self.pass += file.pass;
        self.fail += file.fail;
        self.unimplemented += file.unimplemented;
        self.files.push(file);
    }
}

#[derive(Debug, Serialize)]
struct FileReport {
    file: String,
    pass: usize,
    fail: usize,
    unimplemented: usize,
    failures: Vec<Failure>,
}

#[derive(Debug, Serialize)]
struct Failure {
    test: String,
    field: String,
    expected: String,
    actual: String,
}

#[derive(Debug, Serialize)]
struct ExcludedFile {
    file: String,
    reason: String,
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

        let opcodes = opcode_bytes(&stem)?;
        if let Some(reason) = undefined_reason(&opcodes) {
            report.excluded.push(ExcludedFile {
                file: stem,
                reason: reason.to_owned(),
            });
            continue;
        }

        let cases = load_cases(&path)?;
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

        report.push(run_file(stem, cases, args.ignore_r)?);
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

fn json_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(directory)
        .with_context(|| format!("failed to read SST directory {}", directory.display()))?;
    let mut files = Vec::new();
    for entry in entries {
        let path = entry
            .with_context(|| format!("failed to read an entry in {}", directory.display()))?
            .path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn file_stem(path: &Path) -> Result<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_ascii_lowercase)
        .with_context(|| format!("SST filename is not valid UTF-8: {}", path.display()))
}

fn load_cases(path: &Path) -> Result<Vec<TestCase>> {
    let data =
        fs::read(path).with_context(|| format!("failed to read SST file {}", path.display()))?;
    serde_json::from_slice(&data)
        .with_context(|| format!("failed to parse SST file {}", path.display()))
}

fn implemented(opcodes: &[u8]) -> bool {
    matches!(opcodes, [opcode] if is_opcode_implemented(*opcode))
}

fn run_file(file: String, cases: Vec<TestCase>, ignore_r: bool) -> Result<FileReport> {
    let mut file_report = FileReport {
        file,
        pass: 0,
        fail: 0,
        unimplemented: 0,
        failures: Vec::new(),
    };

    for case in cases {
        let mut cpu = machine()?;
        load_state(&mut cpu, &case.initial)
            .with_context(|| format!("failed to load initial state for {}", case.name))?;
        let cycles = cpu.step();
        if cycles == 0 {
            file_report.unimplemented += 1;
            continue;
        }

        if let Some(failure) = compare(&cpu, &case, ignore_r) {
            file_report.fail += 1;
            file_report.failures.push(failure);
        } else {
            file_report.pass += 1;
        }
    }

    Ok(file_report)
}

fn machine() -> Result<Z180<NullBus>> {
    let config = MachineConfig {
        regions: vec![RegionDef {
            base: 0,
            size: 0x1_0000,
            kind: RegionKind::Ram,
        }],
        ..MachineConfig::default()
    };
    Z180::new(config, NullBus).context("flat 64K SST machine configuration is invalid")
}

fn load_state(cpu: &mut Z180<NullBus>, state: &TestState) -> Result<()> {
    for [address, value] in &state.ram {
        let value = u8::try_from(*value)
            .with_context(|| format!("RAM value {value} at {address:04x} exceeds one byte"))?;
        cpu.mem_poke(u32::from(*address), value);
    }

    cpu.set_reg(Reg::PC, state.pc);
    cpu.set_reg(Reg::SP, state.sp);
    cpu.set_reg(Reg::AF, pair(state.a, state.f));
    cpu.set_reg(Reg::BC, pair(state.b, state.c));
    cpu.set_reg(Reg::DE, pair(state.d, state.e));
    cpu.set_reg(Reg::HL, pair(state.h, state.l));
    cpu.set_reg(Reg::IX, state.ix);
    cpu.set_reg(Reg::IY, state.iy);
    cpu.set_reg(Reg::AF2, state.af2);
    cpu.set_reg(Reg::BC2, state.bc2);
    cpu.set_reg(Reg::DE2, state.de2);
    cpu.set_reg(Reg::HL2, state.hl2);
    cpu.set_reg(Reg::IR, pair(state.i, state.r));
    cpu.set_iff1(state.iff1 != 0);
    cpu.set_iff2(state.iff2 != 0);
    cpu.set_interrupt_mode(state.im);
    Ok(())
}

fn compare(cpu: &Z180<NullBus>, case: &TestCase, ignore_r: bool) -> Option<Failure> {
    let expected = &case.final_state;
    let [a, f] = cpu.reg(Reg::AF).to_be_bytes();
    let [b, c] = cpu.reg(Reg::BC).to_be_bytes();
    let [d, e] = cpu.reg(Reg::DE).to_be_bytes();
    let [h, l] = cpu.reg(Reg::HL).to_be_bytes();
    let [i, r] = cpu.reg(Reg::IR).to_be_bytes();

    let checks = [
        difference_u16("pc", expected.pc, cpu.reg(Reg::PC)),
        difference_u16("sp", expected.sp, cpu.reg(Reg::SP)),
        difference_u8("a", expected.a, a),
        difference_u8("b", expected.b, b),
        difference_u8("c", expected.c, c),
        difference_u8("d", expected.d, d),
        difference_u8("e", expected.e, e),
        difference_u8("f", expected.f & FLAG_COMPARE_MASK, f & FLAG_COMPARE_MASK),
        difference_u8("h", expected.h, h),
        difference_u8("l", expected.l, l),
        difference_u8("i", expected.i, i),
        (!ignore_r)
            .then(|| difference_u8("r", expected.r & 0x7f, r & 0x7f))
            .flatten(),
        difference_u16("ix", expected.ix, cpu.reg(Reg::IX)),
        difference_u16("iy", expected.iy, cpu.reg(Reg::IY)),
        difference_u16(
            "af_",
            mask_pair_flags(expected.af2),
            mask_pair_flags(cpu.reg(Reg::AF2)),
        ),
        difference_u16("bc_", expected.bc2, cpu.reg(Reg::BC2)),
        difference_u16("de_", expected.de2, cpu.reg(Reg::DE2)),
        difference_u16("hl_", expected.hl2, cpu.reg(Reg::HL2)),
        difference_u8("iff1", expected.iff1, u8::from(cpu.iff1())),
        difference_u8("iff2", expected.iff2, u8::from(cpu.iff2())),
        difference_u8("im", expected.im, cpu.interrupt_mode()),
    ];

    if let Some(difference) = checks.into_iter().flatten().next() {
        return Some(Failure {
            test: case.name.clone(),
            field: difference.field.to_owned(),
            expected: difference.expected,
            actual: difference.actual,
        });
    }

    for [address, expected_value] in &expected.ram {
        let actual = cpu.mem_peek(u32::from(*address));
        if u16::from(actual) != *expected_value {
            return Some(Failure {
                test: case.name.clone(),
                field: format!("ram[{address:04x}]"),
                expected: format!("{expected_value:02x}"),
                actual: format!("{actual:02x}"),
            });
        }
    }

    None
}

struct Difference {
    field: &'static str,
    expected: String,
    actual: String,
}

fn difference_u8(field: &'static str, expected: u8, actual: u8) -> Option<Difference> {
    (expected != actual).then(|| Difference {
        field,
        expected: format!("{expected:02x}"),
        actual: format!("{actual:02x}"),
    })
}

fn difference_u16(field: &'static str, expected: u16, actual: u16) -> Option<Difference> {
    (expected != actual).then(|| Difference {
        field,
        expected: format!("{expected:04x}"),
        actual: format!("{actual:04x}"),
    })
}

const fn pair(high: u8, low: u8) -> u16 {
    u16::from_be_bytes([high, low])
}

const fn mask_pair_flags(value: u16) -> u16 {
    value & u16::from_be_bytes([0xff, FLAG_COMPARE_MASK])
}

fn print_text(report: &RunReport) {
    for excluded in &report.excluded {
        println!("EXCLUDED {}: {}", excluded.file, excluded.reason);
    }
    for file in &report.files {
        let status = if file.fail != 0 {
            "FAIL"
        } else if file.unimplemented != 0 {
            "UNIMPLEMENTED"
        } else {
            "PASS"
        };
        println!(
            "{status} {}: pass={} fail={} unimplemented={}",
            file.file, file.pass, file.fail, file.unimplemented
        );
        for failure in &file.failures {
            println!(
                "  {}: {} expected={} actual={}",
                failure.test, failure.field, failure.expected, failure.actual
            );
        }
    }
    println!(
        "SUMMARY pass={} fail={} unimplemented={} excluded={}",
        report.pass,
        report.fail,
        report.unimplemented,
        report.excluded.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_masks_xy_flags_and_r_high_bit() {
        let mut cpu = machine().expect("valid test machine");
        cpu.set_reg(Reg::AF, pair(0x12, 0x28));
        cpu.set_reg(Reg::AF2, pair(0x34, 0x28));
        cpu.set_reg(Reg::IR, pair(0x56, 0x80));
        let case = TestCase {
            name: "masked fields".to_owned(),
            initial: zero_state(),
            final_state: TestState {
                a: 0x12,
                af2: pair(0x34, 0x00),
                i: 0x56,
                ..zero_state()
            },
        };

        assert!(compare(&cpu, &case, false).is_none());
    }

    #[test]
    fn comparison_reports_the_first_differing_field() {
        let cpu = machine().expect("valid test machine");
        let case = TestCase {
            name: "wrong pc and sp".to_owned(),
            initial: zero_state(),
            final_state: TestState {
                pc: 1,
                sp: 2,
                ..zero_state()
            },
        };

        let failure = compare(&cpu, &case, false).expect("comparison must fail");
        assert_eq!(failure.field, "pc");
        assert_eq!(failure.expected, "0001");
        assert_eq!(failure.actual, "0000");
    }

    fn zero_state() -> TestState {
        TestState {
            pc: 0,
            sp: 0,
            a: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            f: 0,
            h: 0,
            l: 0,
            i: 0,
            r: 0,
            ix: 0,
            iy: 0,
            af2: 0,
            bc2: 0,
            de2: 0,
            hl2: 0,
            iff1: 0,
            iff2: 0,
            im: 0,
            ram: Vec::new(),
        }
    }
}
