use super::*;

pub(super) fn json_files(directory: &Path) -> Result<Vec<PathBuf>> {
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

pub(super) fn file_stem(path: &Path) -> Result<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_ascii_lowercase)
        .with_context(|| format!("SST filename is not valid UTF-8: {}", path.display()))
}

pub(super) fn load_cases(path: &Path) -> Result<Vec<TestCase>> {
    let data =
        fs::read(path).with_context(|| format!("failed to read SST file {}", path.display()))?;
    serde_json::from_slice(&data)
        .with_context(|| format!("failed to parse SST file {}", path.display()))
}

pub(super) fn validate_generated_cases(stem: &str, cases: &[TestCase]) -> Result<Option<CaseKind>> {
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
