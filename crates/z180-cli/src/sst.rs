mod policy;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};
use z180_core::{HostBus, MachineConfig, Reg, RegionDef, RegionKind, Z180};

use policy::{OnlyFilter, exclusion_reason, opcode_bytes};

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

    /// Deliberately reverse LD r,r' operands to prove the harness detects errors.
    #[arg(long, hide = true)]
    sabotage_ld: bool,

    /// Print case counts per opcode file and special generated family.
    #[arg(long)]
    census: bool,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum ReportFormat {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum CaseKind {
    Instruction,
    Trap,
    Mmu,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum PortDirection {
    R,
    W,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct PortEvent(u16, u8, PortDirection);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MmuProbe {
    logical: u16,
    expected_physical: u32,
    value: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Z180State {
    itc: u8,
    cbr: u8,
    bbr: u8,
    cbar: u8,
    sleeping: bool,
}

#[derive(Debug, Deserialize)]
struct TestCase {
    name: String,
    kind: Option<CaseKind>,
    seed: Option<u64>,
    flags_mask: Option<u8>,
    disputed: Option<bool>,
    dispute_note: Option<String>,
    ports: Option<Vec<PortEvent>>,
    mmu_probes: Option<Vec<MmuProbe>>,
    initial: TestState,
    #[serde(rename = "final")]
    final_state: TestState,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
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
    z180: Option<Z180State>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct RunReport {
    files: Vec<FileReport>,
    excluded: Vec<ExcludedFile>,
    pass: usize,
    fail: usize,
    unimplemented: usize,
    census: Vec<CensusEntry>,
}

impl RunReport {
    fn new() -> Self {
        Self {
            files: Vec::new(),
            excluded: Vec::new(),
            pass: 0,
            fail: 0,
            unimplemented: 0,
            census: Vec::new(),
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

#[derive(Debug, Serialize)]
struct CensusEntry {
    family: String,
    cases: usize,
}

#[derive(Default)]
struct PortScript {
    expected: Vec<PortEvent>,
    observed: Vec<PortEvent>,
}

#[derive(Clone, Default)]
struct ScriptedBus {
    script: Rc<RefCell<PortScript>>,
}

impl HostBus for ScriptedBus {
    fn mem_read(&mut self, _phys: u32) -> u8 {
        0xff
    }

    fn mem_write(&mut self, _phys: u32, _value: u8) {}

    fn io_read(&mut self, port: u16) -> u8 {
        let mut script = self.script.borrow_mut();
        let value = script
            .expected
            .get(script.observed.len())
            .filter(|event| event.0 == port && event.2 == PortDirection::R)
            .map_or(0xff, |event| event.1);
        script
            .observed
            .push(PortEvent(port, value, PortDirection::R));
        value
    }

    fn io_write(&mut self, port: u16, value: u8) {
        self.script
            .borrow_mut()
            .observed
            .push(PortEvent(port, value, PortDirection::W));
    }
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
                CaseKind::Mmu => report.push(FileReport {
                    file: stem,
                    pass: 0,
                    fail: 0,
                    unimplemented: cases.len(),
                    failures: Vec::new(),
                }),
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

fn validate_generated_cases(stem: &str, cases: &[TestCase]) -> Result<Option<CaseKind>> {
    let Some(first) = cases.first() else {
        return Ok(None);
    };
    let Some(kind) = first.kind else {
        if cases.iter().any(|case| case.kind.is_some()) {
            bail!("{stem} mixes standard and generated SST cases");
        }
        return Ok(None);
    };

    match kind {
        CaseKind::Instruction => {
            if stem.len() != 4
                || !stem.starts_with("ed")
                || u8::from_str_radix(&stem[2..], 16).is_err()
            {
                bail!("generated instruction family has invalid filename {stem:?}");
            }
        }
        CaseKind::Trap if stem != "trap" => {
            bail!("generated TRAP cases must be in trap.json, not {stem}.json");
        }
        CaseKind::Mmu if stem != "mmu" => {
            bail!("generated MMU cases must be in mmu.json, not {stem}.json");
        }
        CaseKind::Trap | CaseKind::Mmu => {}
    }

    for (index, case) in cases.iter().enumerate() {
        validate_generated_case(stem, index, kind, case)?;
    }
    Ok(Some(kind))
}

fn validate_generated_case(
    stem: &str,
    index: usize,
    kind: CaseKind,
    case: &TestCase,
) -> Result<()> {
    let location = format!("{stem}.json[{index}]");
    if case.kind != Some(kind) {
        bail!("{location} has inconsistent case kind");
    }
    if case.name.is_empty() {
        bail!("{location}.name is empty");
    }
    if case.seed.is_none() {
        bail!("{location}.seed is missing");
    }
    let Some(mask) = case.flags_mask else {
        bail!("{location}.flags_mask is missing");
    };
    if mask & 0x28 != 0 {
        bail!("{location}.flags_mask includes undocumented flag bits");
    }
    let Some(disputed) = case.disputed else {
        bail!("{location}.disputed is missing");
    };
    let Some(note) = &case.dispute_note else {
        bail!("{location}.dispute_note is missing");
    };
    if disputed && note.is_empty() {
        bail!("{location} is disputed without a note");
    }
    if case.ports.is_none() {
        bail!("{location}.ports is missing");
    }
    if !case.extra.is_empty() {
        bail!("{location} has unknown top-level fields");
    }

    validate_generated_state(&location, "initial", &case.initial)?;
    validate_generated_state(&location, "final", &case.final_state)?;

    if matches!(kind, CaseKind::Instruction | CaseKind::Trap) {
        let initial_z180 = case
            .initial
            .z180
            .as_ref()
            .expect("generated state validation requires z180 state");
        if initial_z180.itc != 0x01
            || initial_z180.cbr != 0
            || initial_z180.bbr != 0
            || initial_z180.cbar != 0xf0
            || initial_z180.sleeping
        {
            bail!("{location}.initial.z180 must equal reset state");
        }
    }

    match (kind, &case.mmu_probes) {
        (CaseKind::Mmu, Some(probes)) => validate_mmu_probes(&location, probes)?,
        (CaseKind::Mmu, None) => bail!("{location}.mmu_probes is missing"),
        (_, Some(_)) => bail!("{location} has MMU probes outside the MMU family"),
        (_, None) => {}
    }
    Ok(())
}

fn validate_generated_state(location: &str, side: &str, state: &TestState) -> Result<()> {
    if state.iff1 > 1 || state.iff2 > 1 {
        bail!("{location}.{side} has a non-boolean IFF value");
    }
    if state.im > 2 {
        bail!("{location}.{side}.im is invalid");
    }
    let Some(z180) = &state.z180 else {
        bail!("{location}.{side}.z180 is missing");
    };
    let _ = (z180.itc, z180.cbr, z180.bbr, z180.cbar, z180.sleeping);
    if !state.extra.is_empty() {
        bail!("{location}.{side} has unknown state fields");
    }
    let mut previous = None;
    for [address, value] in &state.ram {
        if *value > 0xff {
            bail!("{location}.{side}.ram value at {address:04x} exceeds one byte");
        }
        if previous.is_some_and(|previous| previous >= *address) {
            bail!("{location}.{side}.ram is not sorted with unique addresses");
        }
        previous = Some(*address);
    }
    Ok(())
}

fn validate_mmu_probes(location: &str, probes: &[MmuProbe]) -> Result<()> {
    if probes.len() != 16 {
        bail!("{location}.mmu_probes must contain all 16 logical pages");
    }
    for (page, probe) in probes.iter().enumerate() {
        if usize::from(probe.logical >> 12) != page {
            bail!("{location}.mmu_probes[{page}] does not probe logical page {page}");
        }
        if probe.expected_physical > 0x0f_ffff {
            bail!("{location}.mmu_probes[{page}].expected_physical exceeds 20 bits");
        }
        let _ = probe.value;
    }
    Ok(())
}

fn implemented(opcodes: &[u8]) -> bool {
    Z180::<ScriptedBus>::is_instruction_implemented(opcodes)
}

fn run_file(
    file: String,
    cases: Vec<TestCase>,
    ignore_r: bool,
    sabotage_ld: bool,
) -> Result<FileReport> {
    let mut file_report = FileReport {
        file,
        pass: 0,
        fail: 0,
        unimplemented: 0,
        failures: Vec::new(),
    };

    for case in cases {
        let expected_ports = case.ports.clone().unwrap_or_default();
        let (mut cpu, port_script) = machine(expected_ports)?;
        load_state(&mut cpu, &case.initial)
            .with_context(|| format!("failed to load initial state for {}", case.name))?;
        let sabotage = sabotage_ld.then(|| inject_reversed_ld(&mut cpu)).flatten();
        let cycles = cpu.step();
        if let Some(sabotage) = sabotage {
            sabotage.restore_fetch_byte(&mut cpu);
        }
        if cycles == 0 {
            file_report.unimplemented += 1;
            continue;
        }

        let failure = compare(&cpu, &case, ignore_r)
            .or_else(|| compare_ports(&port_script.borrow(), &case.name));
        if let Some(failure) = failure {
            file_report.fail += 1;
            file_report.failures.push(failure);
        } else {
            file_report.pass += 1;
        }
    }

    Ok(file_report)
}

struct LdSabotage {
    address: u16,
    opcode: u8,
    writes_fetch_byte: bool,
}

impl LdSabotage {
    fn restore_fetch_byte(self, cpu: &mut Z180<ScriptedBus>) {
        if !self.writes_fetch_byte {
            cpu.mem_poke(u32::from(self.address), self.opcode);
        }
    }
}

fn inject_reversed_ld(cpu: &mut Z180<ScriptedBus>) -> Option<LdSabotage> {
    let address = cpu.reg(Reg::PC);
    let opcode = cpu.mem_peek(u32::from(address));
    let reversed = reversed_ld_opcode(opcode)?;
    let destination = (reversed >> 3) & 0x07;
    let writes_fetch_byte = destination == 0x06 && cpu.reg(Reg::HL) == address;
    cpu.mem_poke(u32::from(address), reversed);
    Some(LdSabotage {
        address,
        opcode,
        writes_fetch_byte,
    })
}

fn reversed_ld_opcode(opcode: u8) -> Option<u8> {
    ((0x40..=0x7f).contains(&opcode) && opcode != 0x76)
        .then_some(0x40 | ((opcode & 0x07) << 3) | ((opcode >> 3) & 0x07))
}

fn machine(expected_ports: Vec<PortEvent>) -> Result<(Z180<ScriptedBus>, Rc<RefCell<PortScript>>)> {
    let config = MachineConfig {
        regions: vec![RegionDef {
            base: 0,
            size: 0x1_0000,
            kind: RegionKind::Ram,
        }],
        ..MachineConfig::default()
    };
    let script = Rc::new(RefCell::new(PortScript {
        expected: expected_ports,
        observed: Vec::new(),
    }));
    let bus = ScriptedBus {
        script: Rc::clone(&script),
    };
    let cpu = Z180::new(config, bus).context("flat 64K SST machine configuration is invalid")?;
    Ok((cpu, script))
}

fn load_state(cpu: &mut Z180<ScriptedBus>, state: &TestState) -> Result<()> {
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

fn compare(cpu: &Z180<ScriptedBus>, case: &TestCase, ignore_r: bool) -> Option<Failure> {
    let expected = &case.final_state;
    let flag_mask = case.flags_mask.unwrap_or(FLAG_COMPARE_MASK);
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
        difference_u8("f", expected.f & flag_mask, f & flag_mask),
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

    if let Some(expected_z180) = &expected.z180 {
        let z180_checks = [
            difference_u8("z180.itc", expected_z180.itc, cpu.itc()),
            difference_u8(
                "z180.sleeping",
                u8::from(expected_z180.sleeping),
                u8::from(cpu.sleeping()),
            ),
        ];
        if let Some(difference) = z180_checks.into_iter().flatten().next() {
            return Some(Failure {
                test: case.name.clone(),
                field: difference.field.to_owned(),
                expected: difference.expected,
                actual: difference.actual,
            });
        }
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

fn compare_ports(script: &PortScript, test: &str) -> Option<Failure> {
    if script.expected == script.observed {
        return None;
    }
    let index = script
        .expected
        .iter()
        .zip(&script.observed)
        .position(|(expected, observed)| expected != observed)
        .unwrap_or_else(|| script.expected.len().min(script.observed.len()));
    Some(Failure {
        test: test.to_owned(),
        field: format!("ports[{index}]"),
        expected: script
            .expected
            .get(index)
            .map_or_else(|| "<end>".to_owned(), format_port_event),
        actual: script
            .observed
            .get(index)
            .map_or_else(|| "<end>".to_owned(), format_port_event),
    })
}

fn format_port_event(event: &PortEvent) -> String {
    let direction = match event.2 {
        PortDirection::R => 'r',
        PortDirection::W => 'w',
    };
    format!("{:04x}:{:02x}:{direction}", event.0, event.1)
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
    if !report.census.is_empty() {
        for entry in &report.census {
            println!("CENSUS {}={}", entry.family, entry.cases);
        }
        println!(
            "CENSUS total={}",
            report.census.iter().map(|entry| entry.cases).sum::<usize>()
        );
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
        let (mut cpu, _) = machine(Vec::new()).expect("valid test machine");
        cpu.set_reg(Reg::AF, pair(0x12, 0x28));
        cpu.set_reg(Reg::AF2, pair(0x34, 0x28));
        cpu.set_reg(Reg::IR, pair(0x56, 0x80));
        let case = standard_case(
            "masked fields",
            TestState {
                a: 0x12,
                af2: pair(0x34, 0x00),
                i: 0x56,
                ..zero_state()
            },
        );

        assert!(compare(&cpu, &case, false).is_none());
    }

    #[test]
    fn comparison_reports_the_first_differing_field() {
        let (cpu, _) = machine(Vec::new()).expect("valid test machine");
        let case = standard_case(
            "wrong pc and sp",
            TestState {
                pc: 1,
                sp: 2,
                ..zero_state()
            },
        );

        let failure = compare(&cpu, &case, false).expect("comparison must fail");
        assert_eq!(failure.field, "pc");
        assert_eq!(failure.expected, "0001");
        assert_eq!(failure.actual, "0000");
    }

    #[test]
    fn comparison_uses_the_generated_case_flag_mask() {
        let (mut cpu, _) = machine(Vec::new()).expect("valid test machine");
        cpu.set_reg(Reg::AF, pair(0, 0x80));
        let mut case = standard_case("masked documented flags", zero_state());
        case.flags_mask = Some(0x42);

        assert!(compare(&cpu, &case, false).is_none());
    }

    #[test]
    fn sabotage_reverses_only_ld_operands() {
        assert_eq!(reversed_ld_opcode(0x41), Some(0x48));
        assert_eq!(reversed_ld_opcode(0x70), Some(0x46));
        assert_eq!(reversed_ld_opcode(0x40), Some(0x40));
        assert_eq!(reversed_ld_opcode(0x76), None);
        assert_eq!(reversed_ld_opcode(0x00), None);
    }

    #[test]
    fn scripted_bus_supplies_reads_and_records_writes_in_order() {
        let expected = vec![
            PortEvent(0x0040, 0x12, PortDirection::R),
            PortEvent(0x0041, 0x34, PortDirection::W),
        ];
        let script = Rc::new(RefCell::new(PortScript {
            expected,
            observed: Vec::new(),
        }));
        let mut bus = ScriptedBus {
            script: Rc::clone(&script),
        };

        assert_eq!(bus.io_read(0x0040), 0x12);
        bus.io_write(0x0041, 0x34);
        assert!(compare_ports(&script.borrow(), "ports").is_none());
    }

    #[test]
    fn generated_schema_dispatches_instruction_and_validates_mmu_pages() {
        let instruction = generated_case(CaseKind::Instruction, None);
        assert_eq!(
            validate_generated_cases("ed00", &[instruction]).expect("valid instruction schema"),
            Some(CaseKind::Instruction)
        );

        let probes = (0_u16..16)
            .map(|page| MmuProbe {
                logical: page << 12,
                expected_physical: u32::from(page) << 12,
                value: page as u8,
            })
            .collect();
        let mmu = generated_case(CaseKind::Mmu, Some(probes));
        assert_eq!(
            validate_generated_cases("mmu", &[mmu]).expect("valid MMU schema"),
            Some(CaseKind::Mmu)
        );
    }

    #[test]
    fn generated_instruction_requires_reset_z180_state() {
        let mut instruction = generated_case(CaseKind::Instruction, None);
        instruction
            .initial
            .z180
            .as_mut()
            .expect("generated z180 state")
            .itc = 0x81;

        let error = validate_generated_cases("ed00", &[instruction])
            .expect_err("non-reset initial z180 state must be rejected");
        assert!(
            error
                .to_string()
                .contains("initial.z180 must equal reset state")
        );
    }

    #[test]
    fn generated_comparison_includes_itc_and_sleeping() {
        let (cpu, _) = machine(Vec::new()).expect("valid test machine");
        let mut case = generated_case(CaseKind::Instruction, None);
        let expected_z180 = case
            .final_state
            .z180
            .as_mut()
            .expect("generated z180 state");
        expected_z180.itc = 0x81;
        expected_z180.sleeping = true;

        let failure = compare(&cpu, &case, false).expect("ITC mismatch must fail");
        assert_eq!(failure.field, "z180.itc");
    }

    fn standard_case(name: &str, final_state: TestState) -> TestCase {
        TestCase {
            name: name.to_owned(),
            kind: None,
            seed: None,
            flags_mask: None,
            disputed: None,
            dispute_note: None,
            ports: None,
            mmu_probes: None,
            initial: zero_state(),
            final_state,
            extra: BTreeMap::new(),
        }
    }

    fn generated_case(kind: CaseKind, mmu_probes: Option<Vec<MmuProbe>>) -> TestCase {
        TestCase {
            name: "generated".to_owned(),
            kind: Some(kind),
            seed: Some(1),
            flags_mask: Some(0xd7),
            disputed: Some(false),
            dispute_note: Some(String::new()),
            ports: Some(Vec::new()),
            mmu_probes,
            initial: generated_state(),
            final_state: generated_state(),
            extra: BTreeMap::new(),
        }
    }

    fn generated_state() -> TestState {
        TestState {
            z180: Some(Z180State {
                itc: 1,
                cbr: 0,
                bbr: 0,
                cbar: 0xf0,
                sleeping: false,
            }),
            ..zero_state()
        }
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
            z180: None,
            extra: BTreeMap::new(),
        }
    }
}
