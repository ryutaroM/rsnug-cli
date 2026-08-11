use crate::error::RsnugError;
use crate::vault::{self, TrashEntry, VaultData};
use age::secrecy::{ExposeSecret, SecretString};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct InitOutcome {
    pub path: std::path::PathBuf,
}

pub struct TrashedKey {
    pub key: String,
    pub deleted_at: u64,
}

pub enum GetOutcome {
    Metadata { key: String },
    Revealed { key: String, value: SecretString },
}

pub fn init(
    path: &Path,
    passphrase: &SecretString,
    force: bool,
) -> Result<InitOutcome, RsnugError> {
    if path.exists() {
        if !force {
            return Err(RsnugError::VaultAlreadyExists(path.to_path_buf()));
        }
        if !vault::is_decryptable(path, passphrase) {
            return Err(RsnugError::VaultNotOverwritable(path.to_path_buf()));
        }
    }
    vault::save(path, &VaultData::empty(), passphrase)?;
    Ok(InitOutcome {
        path: path.to_path_buf(),
    })
}

pub fn set(
    path: &Path,
    passphrase: &SecretString,
    key: String,
    value: SecretString,
) -> Result<(), RsnugError> {
    let mut data = vault::load(path, passphrase)?;
    let value = value.expose_secret().to_owned();
    if let Some(previous) = data.remove(&key)
        && previous != value
    {
        data.trash_push(key.clone(), TrashEntry::new(previous, now()));
    }
    data.insert(key, value);
    vault::save(path, &data, passphrase)
}

pub fn get(
    path: &Path,
    passphrase: &SecretString,
    key: &str,
    reveal: bool,
) -> Result<GetOutcome, RsnugError> {
    let data = vault::load(path, passphrase)?;
    let value = data
        .get(key)
        .ok_or_else(|| RsnugError::KeyNotFound(key.to_owned()))?;

    Ok(if reveal {
        GetOutcome::Revealed {
            key: key.to_owned(),
            value: SecretString::from(value.to_owned()),
        }
    } else {
        GetOutcome::Metadata {
            key: key.to_owned(),
        }
    })
}

pub fn unset(path: &Path, passphrase: &SecretString, key: &str) -> Result<(), RsnugError> {
    let mut data = vault::load(path, passphrase)?;
    let value = data
        .remove(key)
        .ok_or_else(|| RsnugError::KeyNotFound(key.to_owned()))?;
    data.trash_push(key.to_owned(), TrashEntry::new(value, now()));
    vault::save(path, &data, passphrase)
}

pub fn restore(
    path: &Path,
    passphrase: &SecretString,
    key: &str,
    at: Option<u64>,
) -> Result<(), RsnugError> {
    let mut data = vault::load(path, passphrase)?;
    let entry = match at {
        Some(at) => {
            data.trash_take_at(key, at)
                .ok_or_else(|| RsnugError::TrashGenerationNotFound {
                    key: key.to_owned(),
                    at,
                })?
        }
        None => data
            .trash_pop(key)
            .ok_or_else(|| RsnugError::KeyNotFound(key.to_owned()))?,
    };
    if data.get(key).is_some() {
        return Err(RsnugError::KeyAlreadyExists(key.to_owned()));
    }
    data.insert(key.to_owned(), entry.into_value());
    vault::save(path, &data, passphrase)
}

pub fn list(path: &Path, passphrase: &SecretString) -> Result<Vec<String>, RsnugError> {
    let data = vault::load(path, passphrase)?;
    Ok(data.keys().map(str::to_owned).collect())
}

pub fn list_trash(path: &Path, passphrase: &SecretString) -> Result<Vec<TrashedKey>, RsnugError> {
    let data = vault::load(path, passphrase)?;
    Ok(data
        .trash_entries()
        .map(|(key, entry)| TrashedKey {
            key: key.to_owned(),
            deleted_at: entry.deleted_at(),
        })
        .collect())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}
