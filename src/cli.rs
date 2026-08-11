use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Secrets manager for use by AI agents
#[derive(Parser)]
#[command(name = "rsnug", version, about, arg_required_else_help = true)]
pub struct Cli {
    /// Path to the vault file
    #[arg(short = 'f', long, value_name = "PATH", global = true)]
    pub vault: Option<PathBuf>,

    /// Output format
    #[arg(long, value_enum, default_value_t = Format::Text, global = true)]
    pub format: Format,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Text,
    Json,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create a new vault
    Init {
        /// Overwrite an existing vault, if the passphrase opens it
        #[arg(long)]
        force: bool,
    },
    /// Set a secret
    Set {
        /// Key of the secret
        key: String,

        /// Value to set. Required unless --stdin is given
        #[arg(required_unless_present = "stdin")]
        value: Option<String>,

        /// Read the value from standard input
        #[arg(long, conflicts_with = "value", required_unless_present = "value")]
        stdin: bool,
    },
    /// Get metadata for a secret
    Get {
        /// Key of the secret
        key: String,

        /// Print the value in plaintext
        #[arg(long)]
        reveal: bool,
    },
    /// Delete a secret
    Unset {
        /// Key of the secret
        key: String,
    },
    /// List the registered keys
    List,
}

impl Command {
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            Command::Init { .. } => "init",
            Command::Set { .. } => "set",
            Command::Get { .. } => "get",
            Command::Unset { .. } => "unset",
            Command::List => "list",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, error::ErrorKind};

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("rsnug").chain(args.iter().copied()))
    }

    fn kind(args: &[&str]) -> ErrorKind {
        match parse(args) {
            Ok(_) => panic!("expected a parse error"),
            Err(err) => err.kind(),
        }
    }

    #[test]
    fn definition_is_internally_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn both_help_flags_are_available() {
        assert_eq!(kind(&["-h"]), ErrorKind::DisplayHelp);
        assert_eq!(kind(&["--help"]), ErrorKind::DisplayHelp);
    }

    #[test]
    fn version_flag_is_available() {
        assert_eq!(kind(&["--version"]), ErrorKind::DisplayVersion);
        assert_eq!(kind(&["-V"]), ErrorKind::DisplayVersion);
    }

    #[test]
    fn missing_subcommand_shows_help() {
        assert_eq!(
            kind(&[]),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn set_requires_a_value_or_stdin() {
        assert_eq!(kind(&["set", "KEY"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn set_rejects_a_value_together_with_stdin() {
        assert_eq!(
            kind(&["set", "KEY", "VALUE", "--stdin"]),
            ErrorKind::ArgumentConflict
        );
    }

    #[test]
    fn set_accepts_stdin_without_a_value() {
        let cli = parse(&["set", "KEY", "--stdin"]).expect("should parse");
        let Command::Set { key, value, stdin } = cli.command else {
            panic!("expected set");
        };
        assert_eq!(key, "KEY");
        assert_eq!(value, None);
        assert!(stdin);
    }

    #[test]
    fn set_accepts_a_positional_value() {
        let cli = parse(&["set", "KEY", "VALUE"]).expect("should parse");
        let Command::Set { key, value, stdin } = cli.command else {
            panic!("expected set");
        };
        assert_eq!(key, "KEY");
        assert_eq!(value.as_deref(), Some("VALUE"));
        assert!(!stdin);
    }

    #[test]
    fn get_hides_the_value_unless_reveal_is_given() {
        let Command::Get { reveal, .. } = parse(&["get", "KEY"]).expect("should parse").command
        else {
            panic!("expected get");
        };
        assert!(!reveal);

        let Command::Get { reveal, .. } = parse(&["get", "KEY", "--reveal"])
            .expect("should parse")
            .command
        else {
            panic!("expected get");
        };
        assert!(reveal);
    }

    #[test]
    fn unset_takes_only_a_key() {
        let Command::Unset { key } = parse(&["unset", "KEY"]).expect("should parse").command else {
            panic!("expected unset");
        };
        assert_eq!(key, "KEY");
    }

    #[test]
    fn format_defaults_to_text_and_accepts_json_only() {
        assert_eq!(parse(&["list"]).expect("should parse").format, Format::Text);
        assert_eq!(
            parse(&["--format", "json", "list"])
                .expect("should parse")
                .format,
            Format::Json
        );
        assert_eq!(kind(&["--format", "yaml", "list"]), ErrorKind::InvalidValue);
    }

    #[test]
    fn global_flags_are_accepted_after_the_subcommand() {
        let cli = parse(&[
            "set", "--format", "json", "--vault", "/tmp/v", "KEY", "VALUE",
        ])
        .expect("should parse");
        assert_eq!(cli.format, Format::Json);
        assert_eq!(cli.vault.as_deref(), Some(std::path::Path::new("/tmp/v")));
    }
}
