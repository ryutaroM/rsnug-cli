use crate::error::RsnugError;
use crate::vault::{self, VaultData};
use age::secrecy::{ExposeSecret, SecretString};
use std::path::Path;

pub struct InitOutcome {
    pub path: std::path::PathBuf,
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
    data.insert(key, value.expose_secret().to_owned());
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
    if data.remove(key).is_none() {
        return Err(RsnugError::KeyNotFound(key.to_owned()));
    }
    vault::save(path, &data, passphrase)
}

pub fn list(path: &Path, passphrase: &SecretString) -> Result<Vec<String>, RsnugError> {
    let data = vault::load(path, passphrase)?;
    Ok(data.keys().map(str::to_owned).collect())
}
