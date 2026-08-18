use crate::cli::Format;
use crate::commands::{GetOutcome, InitOutcome, MigrateOutcome};
use age::secrecy::ExposeSecret;
use serde_json::json;

pub fn init(outcome: InitOutcome, format: Format) -> String {
    let path = outcome.path.display().to_string();
    let key_file = outcome.key_file.display().to_string();
    match format {
        Format::Text => format!("Initialized vault at {path} (key: {key_file})\n"),
        Format::Json => format!("{}\n", json!({ "path": path, "key_file": key_file })),
    }
}

pub fn migrate(outcome: MigrateOutcome, format: Format) -> String {
    let path = outcome.path.display().to_string();
    let key_file = outcome.key_file.display().to_string();
    let backup = outcome.backup.display().to_string();
    match format {
        Format::Text => {
            format!("Migrated vault at {path} (key: {key_file}, backup: {backup})\n")
        }
        Format::Json => format!(
            "{}\n",
            json!({ "path": path, "key_file": key_file, "backup": backup })
        ),
    }
}

pub fn set(key: String, format: Format) -> String {
    match format {
        Format::Text => format!("Set {key}\n"),
        Format::Json => format!("{}\n", json!({ "key": key })),
    }
}

pub fn unset(key: String, format: Format) -> String {
    match format {
        Format::Text => format!("Unset {key}\n"),
        Format::Json => format!("{}\n", json!({ "key": key })),
    }
}

pub fn get(outcome: GetOutcome, format: Format) -> String {
    match (outcome, format) {
        (GetOutcome::Metadata { key }, Format::Text) => format!("{key}: exists\n"),
        (GetOutcome::Metadata { key }, Format::Json) => format!("{}\n", json!({ "key": key })),
        (GetOutcome::Revealed { value, .. }, Format::Text) => {
            format!("{}\n", value.expose_secret())
        }
        (GetOutcome::Revealed { key, value }, Format::Json) => {
            format!(
                "{}\n",
                json!({ "key": key, "value": value.expose_secret() })
            )
        }
    }
}

pub fn list(keys: Vec<String>, format: Format) -> String {
    match format {
        Format::Text => {
            if keys.is_empty() {
                String::new()
            } else {
                format!("{}\n", keys.join("\n"))
            }
        }
        Format::Json => format!("{}\n", json!({ "keys": keys })),
    }
}
