use crate::cli::Format;
use crate::commands::{GetOutcome, InitOutcome, TrashedKey};
use age::secrecy::ExposeSecret;
use serde_json::json;

pub fn init(outcome: InitOutcome, format: Format) -> String {
    let path = outcome.path.display().to_string();
    match format {
        Format::Text => format!("Initialized vault at {path}\n"),
        Format::Json => format!("{}\n", json!({ "path": path })),
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

pub fn restore(key: String, format: Format) -> String {
    match format {
        Format::Text => format!("Restored {key}\n"),
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

pub fn list_trash(entries: Vec<TrashedKey>, format: Format) -> String {
    match format {
        Format::Text => entries
            .into_iter()
            .map(|entry| {
                format!(
                    "{}\t{}\n",
                    entry.key,
                    crate::timestamp::format(entry.deleted_at)
                )
            })
            .collect(),
        Format::Json => {
            let trashed: Vec<_> = entries
                .into_iter()
                .map(|entry| json!({ "key": entry.key, "deleted_at": crate::timestamp::format(entry.deleted_at) }))
                .collect();
            format!("{}\n", json!({ "trashed": trashed }))
        }
    }
}
