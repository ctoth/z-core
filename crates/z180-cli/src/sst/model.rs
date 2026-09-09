use super::*;

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub(super) enum ReportFormat {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum CaseKind {
    Instruction,
    Trap,
    Mmu,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum PortDirection {
    R,
    W,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct PortEvent(pub(super) u16, pub(super) u8, pub(super) PortDirection);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MmuProbe {
    pub(super) logical: u16,
    pub(super) expected_physical: u32,
    pub(super) value: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Z180State {
    pub(super) itc: u8,
    pub(super) cbr: u8,
    pub(super) bbr: u8,
    pub(super) cbar: u8,
    pub(super) sleeping: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct TestCase {
    pub(super) name: String,
    pub(super) kind: Option<CaseKind>,
    pub(super) seed: Option<u64>,
    pub(super) flags_mask: Option<u8>,
    pub(super) disputed: Option<bool>,
    pub(super) dispute_note: Option<String>,
    pub(super) ports: Option<Vec<PortEvent>>,
    pub(super) mmu_probes: Option<Vec<MmuProbe>>,
    pub(super) initial: TestState,
    #[serde(rename = "final")]
    pub(super) final_state: TestState,
    #[serde(flatten)]
    pub(super) extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TestState {
    pub(super) pc: u16,
    pub(super) sp: u16,
    pub(super) a: u8,
    pub(super) b: u8,
    pub(super) c: u8,
    pub(super) d: u8,
    pub(super) e: u8,
    pub(super) f: u8,
    pub(super) h: u8,
    pub(super) l: u8,
    pub(super) i: u8,
    pub(super) r: u8,
    pub(super) ix: u16,
    pub(super) iy: u16,
    #[serde(rename = "af_")]
    pub(super) af2: u16,
    #[serde(rename = "bc_")]
    pub(super) bc2: u16,
    #[serde(rename = "de_")]
    pub(super) de2: u16,
    #[serde(rename = "hl_")]
    pub(super) hl2: u16,
    pub(super) iff1: u8,
    pub(super) iff2: u8,
    pub(super) im: u8,
    pub(super) ram: Vec<[u16; 2]>,
    pub(super) z180: Option<Z180State>,
    #[serde(flatten)]
    pub(super) extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct RunReport {
    pub(super) files: Vec<FileReport>,
    pub(super) excluded: Vec<ExcludedFile>,
    pub(super) pass: usize,
    pub(super) fail: usize,
    pub(super) unimplemented: usize,
    pub(super) census: Vec<CensusEntry>,
}

impl RunReport {
    pub(super) fn new() -> Self {
        Self {
            files: Vec::new(),
            excluded: Vec::new(),
            pass: 0,
            fail: 0,
            unimplemented: 0,
            census: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, file: FileReport) {
        self.pass += file.pass;
        self.fail += file.fail;
        self.unimplemented += file.unimplemented;
        self.files.push(file);
    }
}

#[derive(Debug, Serialize)]
pub(super) struct FileReport {
    pub(super) file: String,
    pub(super) pass: usize,
    pub(super) fail: usize,
    pub(super) unimplemented: usize,
    pub(super) failures: Vec<Failure>,
}

#[derive(Debug, Serialize)]
pub(super) struct Failure {
    pub(super) test: String,
    pub(super) field: String,
    pub(super) expected: String,
    pub(super) actual: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ExcludedFile {
    pub(super) file: String,
    pub(super) reason: String,
}

#[derive(Debug, Serialize)]
pub(super) struct CensusEntry {
    pub(super) family: String,
    pub(super) cases: usize,
}
