mod sst;

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
    /// Run SingleStepTests JSON conformance cases.
    Sst(sst::SstArgs),
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Sst(args) => sst::run(args),
    }
}
