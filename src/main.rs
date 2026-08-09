mod cli;
mod exit;

use clap::Parser;
use cli::Cli;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();

    eprintln!("rsnug: `{}` is not implemented yet", cli.command.name());
    ExitCode::from(exit::GENERAL_ERROR)
}
