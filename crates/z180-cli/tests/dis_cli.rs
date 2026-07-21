use std::{path::PathBuf, process::Command};

#[test]
fn dis_command_matches_the_every_mnemonic_golden_file() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dis_every_mnemonic.bin");
    let output = Command::new(env!("CARGO_BIN_EXE_z180-cli"))
        .arg("dis")
        .arg(fixture)
        .arg("--org")
        .arg("0x4000")
        .output()
        .expect("z180-cli must launch");

    assert!(
        output.status.success(),
        "z180-cli dis failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("disassembly output must be UTF-8"),
        include_str!("fixtures/dis_every_mnemonic.golden")
    );
}
