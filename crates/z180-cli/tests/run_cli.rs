use std::{
    env, fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn run_command_executes_a_configured_rom_with_trace() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must follow the Unix epoch")
        .as_nanos();
    let directory = env::temp_dir().join(format!("z180-cli-run-{}-{nonce}", std::process::id()));
    fs::create_dir(&directory).expect("temporary fixture directory must be creatable");
    let rom_path = directory.join("rom.bin");
    let config_path = directory.join("machine.toml");
    fs::write(&rom_path, vec![0_u8; 0x1000]).expect("temporary ROM must be writable");
    fs::write(
        &config_path,
        r#"clock_hz = 18_432_000
variant = "z8s180"

[[regions]]
kind = "rom"
base = 0x00000
size = 0x01000
"#,
    )
    .expect("temporary config must be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_z180-cli"))
        .arg("run")
        .arg(&rom_path)
        .arg("--cycles")
        .arg("12")
        .arg("--trace")
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("z180-cli must launch");
    fs::remove_dir_all(&directory).expect("temporary fixture directory must be removable");

    assert!(
        output.status.success(),
        "z180-cli run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("trace output must be UTF-8"),
        concat!(
            "000000000000  0000  000000  00           NOP\n",
            "000000000006  0001  000001  00           NOP\n",
        )
    );
}
