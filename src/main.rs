mod cli;
mod commands;
mod error;
mod exit;
mod key;
mod render;
mod vault;

use age::secrecy::SecretString;
use clap::Parser;
use cli::{Cli, Command};
use error::RsnugError;
use std::io::Read;
use std::process::ExitCode;

const LEGACY_ENV_VAR: &str = "RSNUG_PASSPHRASE";

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
    let key_path = key::resolve_path(cli.key_file)?;

    match cli.command {
        Command::Init { force, new_key } => commands::init(&vault_path, &key_path, force, new_key)
            .map(|outcome| render::init(outcome, cli.format)),
        Command::Migrate => commands::migrate(&vault_path, &key_path, &legacy_passphrase()?)
            .map(|outcome| render::migrate(outcome, cli.format)),
        Command::Set { key, value, stdin } => {
            let identities = key::load(&key_path)?;
            let value = SecretString::from(if stdin {
                read_stdin_value()?
            } else {
                value.expect("clap guarantees value xor stdin")
            });
            commands::set(&vault_path, &identities, key.clone(), value)
                .map(|()| render::set(key, cli.format))
        }
        Command::Get { key, reveal } => {
            let identities = key::load(&key_path)?;
            commands::get(&vault_path, &identities, &key, reveal)
                .map(|outcome| render::get(outcome, cli.format))
        }
        Command::Unset { key } => {
            let identities = key::load(&key_path)?;
            commands::unset(&vault_path, &identities, &key).map(|()| render::unset(key, cli.format))
        }
        Command::List => {
            let identities = key::load(&key_path)?;
            commands::list(&vault_path, &identities).map(|keys| render::list(keys, cli.format))
        }
    }
}

fn legacy_passphrase() -> Result<SecretString, RsnugError> {
    match std::env::var(LEGACY_ENV_VAR) {
        Ok(value) if !value.is_empty() => Ok(SecretString::from(value)),
        _ => Err(RsnugError::LegacyPassphraseMissing),
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
