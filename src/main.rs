mod cli;
mod commands;
mod error;
mod exit;
mod passphrase;
mod render;
mod vault;

use age::secrecy::SecretString;
use clap::Parser;
use cli::{Cli, Command};
use error::RsnugError;
use std::io::Read;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(output) => {
            if !output.is_empty() {
                print!("{output}");
            }
            ExitCode::from(exit::OK)
        }
        Err(err) => {
            eprintln!("rsnug: {err}");
            ExitCode::from(err.exit_code())
        }
    }
}

fn run(cli: Cli) -> Result<String, RsnugError> {
    let vault_path = vault::resolve_path(cli.vault)?;
    let passphrase = passphrase::resolve()?;

    match cli.command {
        Command::Init { force } => commands::init(&vault_path, &passphrase, force)
            .map(|outcome| render::init(outcome, cli.format)),
        Command::Set { key, value, stdin } => {
            let value = SecretString::from(if stdin {
                read_stdin_value()?
            } else {
                value.expect("clap guarantees value xor stdin")
            });
            commands::set(&vault_path, &passphrase, key.clone(), value)
                .map(|()| render::set(key, cli.format))
        }
        Command::Get { key, reveal } => commands::get(&vault_path, &passphrase, &key, reveal)
            .map(|outcome| render::get(outcome, cli.format)),
        Command::List => {
            commands::list(&vault_path, &passphrase).map(|keys| render::list(keys, cli.format))
        }
    }
}

fn read_stdin_value() -> Result<String, RsnugError> {
    let mut buf = Vec::new();
    std::io::stdin().read_to_end(&mut buf)?;
    let text = String::from_utf8(buf)
        .map_err(|err| RsnugError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, err)))?;
    let text = text.strip_suffix('\n').unwrap_or(&text);
    let text = text.strip_suffix('\r').unwrap_or(text);
    Ok(text.to_owned())
}
