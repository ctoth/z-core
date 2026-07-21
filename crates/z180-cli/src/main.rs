mod dis;
mod run;
mod sst;
mod zex;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "z180-cli")]
#[command(about = "Z180 emulator command-line tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Disassemble a raw Z180 binary.
    Dis(dis::DisArgs),
    /// Run a bare ROM using a TOML machine configuration.
    Run(run::RunArgs),
    /// Run SingleStepTests JSON conformance cases.
    Sst(sst::SstArgs),
    /// Run a CP/M ZEX instruction exerciser image.
    Zex(zex::ZexArgs),
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Dis(args) => dis::run(args),
        Command::Run(args) => run::run(args),
        Command::Sst(args) => sst::run(args),
        Command::Zex(args) => zex::run(args),
    }
}
