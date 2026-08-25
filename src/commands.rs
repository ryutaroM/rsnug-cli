use crate::error::RsnugError;
use crate::key;
use crate::lock;
use crate::vault::{self, VaultData};
use age::secrecy::{ExposeSecret, SecretString};
use std::path::Path;

pub struct InitOutcome {
    pub path: std::path::PathBuf,
    pub key_file: std::path::PathBuf,
}

pub struct MigrateOutcome {
    pub path: std::path::PathBuf,
    pub key_file: std::path::PathBuf,
    pub backup: std::path::PathBuf,
}

pub enum GetOutcome {
    Metadata { key: String },
    Revealed { key: String, value: SecretString },
}

fn require_vault(path: &Path) -> Result<(), RsnugError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(RsnugError::VaultNotAFile(path.to_path_buf())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Err(RsnugError::VaultNotFound(path.to_path_buf()))
        }
        Err(err) => Err(RsnugError::Io(err)),
    }
}

pub fn init(
    path: &Path,
    key_file: &Path,
    force: bool,
    new_key: bool,
) -> Result<InitOutcome, RsnugError> {
    let _lock = lock::acquire(path)?;
    let recipient = if path.exists() {
        if !force {
            return Err(RsnugError::VaultAlreadyExists(path.to_path_buf()));
        }
        let identities = key::load(key_file)?;
        if !vault::is_decryptable(path, &identities)? {
            return Err(RsnugError::VaultNotOverwritable(path.to_path_buf()));
        }
        recipient_for(key_file, new_key, identities)?
    } else {
        recipient_from_key_file(key_file, new_key)?
    };

    vault::save(path, &VaultData::empty(), &recipient)?;
    Ok(InitOutcome {
        path: path.to_path_buf(),
        key_file: key_file.to_path_buf(),
    })
}

fn recipient_from_key_file(
    key_file: &Path,
    new_key: bool,
) -> Result<age::x25519::Recipient, RsnugError> {
    match key::load(key_file) {
        Ok(identities) => recipient_for(key_file, new_key, identities),
        Err(RsnugError::KeyFileNotFound(_)) => match key::generate(key_file) {
            Ok(identity) => Ok(identity.to_public()),
            Err(RsnugError::KeyFileAlreadyExists(_)) => {
                recipient_for(key_file, new_key, key::load(key_file)?)
            }
            Err(err) => Err(err),
        },
        Err(err) => Err(err),
    }
}

fn recipient_for(
    key_file: &Path,
    new_key: bool,
    identities: Vec<age::x25519::Identity>,
) -> Result<age::x25519::Recipient, RsnugError> {
    if new_key {
        return Ok(key::append(key_file)?.to_public());
    }
    identities
        .first()
        .map(age::x25519::Identity::to_public)
        .ok_or_else(|| RsnugError::KeyFileInvalid(key_file.to_path_buf()))
}

pub fn migrate(
    path: &Path,
    key_file: &Path,
    passphrase: &SecretString,
) -> Result<MigrateOutcome, RsnugError> {
    require_vault(path)?;
    let _lock = lock::acquire(path)?;
    if !vault::is_legacy(path)? {
        return Err(RsnugError::VaultAlreadyMigrated(path.to_path_buf()));
    }

    let data = vault::load_legacy(path, passphrase)?;

    let backup = backup_path(path);
    if backup.exists() && std::fs::read(&backup)? != std::fs::read(path)? {
        return Err(RsnugError::BackupAlreadyExists(backup));
    }

    let recipient = recipient_from_key_file(key_file, false)?;

    std::fs::copy(path, &backup)?;
    vault::save(path, &data, &recipient)?;

    Ok(MigrateOutcome {
        path: path.to_path_buf(),
        key_file: key_file.to_path_buf(),
        backup,
    })
}

fn backup_path(path: &Path) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".bak");
    std::path::PathBuf::from(name)
}

pub fn set(
    path: &Path,
    identities: &[age::x25519::Identity],
    key: String,
    value: SecretString,
) -> Result<(), RsnugError> {
    require_vault(path)?;
    let _lock = lock::acquire(path)?;
    let (mut data, recipient) = vault::load(path, identities)?;
    data.insert(key, value.expose_secret().to_owned());
    vault::save(path, &data, &recipient)
}

pub fn get(
    path: &Path,
    identities: &[age::x25519::Identity],
    key: &str,
    reveal: bool,
) -> Result<GetOutcome, RsnugError> {
    let (data, _) = vault::load(path, identities)?;
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

pub fn unset(
    path: &Path,
    identities: &[age::x25519::Identity],
    key: &str,
) -> Result<(), RsnugError> {
    require_vault(path)?;
    let _lock = lock::acquire(path)?;
    let (mut data, recipient) = vault::load(path, identities)?;
    if data.remove(key).is_none() {
        return Err(RsnugError::KeyNotFound(key.to_owned()));
    }
    vault::save(path, &data, &recipient)
}

pub fn list(path: &Path, identities: &[age::x25519::Identity]) -> Result<Vec<String>, RsnugError> {
    let (data, _) = vault::load(path, identities)?;
    Ok(data.keys().map(str::to_owned).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_vault_is_reported_as_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("vault.age");

        match require_vault(&vault) {
            Err(RsnugError::VaultNotFound(reported)) => assert_eq!(reported, vault),
            other => panic!("expected VaultNotFound, got {other:?}"),
        }
    }

    #[test]
    fn an_existing_vault_file_is_accepted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("vault.age");
        std::fs::write(&vault, b"ciphertext").expect("write");

        require_vault(&vault).expect("an existing vault file is a vault");
    }

    #[test]
    fn a_directory_is_not_a_vault() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("vault.age");
        std::fs::create_dir(&vault).expect("create dir");

        match require_vault(&vault) {
            Err(RsnugError::VaultNotAFile(reported)) => assert_eq!(reported, vault),
            other => panic!("expected VaultNotAFile, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_vault_rsnug_cannot_look_at_is_reported_as_an_io_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let blocked = dir.path().join("blocked");
        std::fs::create_dir(&blocked).expect("create dir");
        let vault = blocked.join("vault.age");
        std::fs::write(&vault, b"ciphertext").expect("write");
        crate::fsutil::set_private_permissions(&blocked, 0o000).expect("chmod");

        let result = require_vault(&vault);
        let denied = matches!(
            std::fs::metadata(&vault),
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied
        );
        crate::fsutil::set_private_permissions(&blocked, 0o700).expect("chmod");

        if !denied {
            return;
        }
        match result {
            Err(RsnugError::Io(err)) => {
                assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
            }
            other => panic!("expected Io, got {other:?}"),
        }
    }
}
