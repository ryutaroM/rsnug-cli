use crate::error::RsnugError;
use age::secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const CURRENT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct VaultData {
    version: u32,
    entries: BTreeMap<String, String>,
}

impl VaultData {
    pub fn empty() -> Self {
        Self {
            version: CURRENT_VERSION,
            entries: BTreeMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    pub fn insert(&mut self, key: String, value: String) {
        self.entries.insert(key, value);
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

pub fn default_path() -> Result<PathBuf, RsnugError> {
    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME")
        && !config_home.is_empty()
    {
        return Ok(PathBuf::from(config_home).join("rsnug").join("vault.age"));
    }
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => Ok(PathBuf::from(home)
            .join(".config")
            .join("rsnug")
            .join("vault.age")),
        _ => Err(RsnugError::HomeDirectoryUnavailable),
    }
}

pub fn resolve_path(explicit: Option<PathBuf>) -> Result<PathBuf, RsnugError> {
    match explicit {
        Some(path) => Ok(path),
        None => default_path(),
    }
}

pub fn load(path: &Path, passphrase: &SecretString) -> Result<VaultData, RsnugError> {
    let ciphertext = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(RsnugError::VaultNotFound(path.to_path_buf()));
        }
        Err(err) => return Err(RsnugError::Io(err)),
    };

    let plaintext = decrypt(&ciphertext, passphrase)?;
    let data: VaultData = serde_json::from_slice(&plaintext)?;

    if data.version != CURRENT_VERSION {
        return Err(RsnugError::UnsupportedVaultVersion {
            found: data.version,
            expected: CURRENT_VERSION,
        });
    }

    Ok(data)
}

pub fn save(path: &Path, data: &VaultData, passphrase: &SecretString) -> Result<(), RsnugError> {
    let plaintext = serde_json::to_vec(data)?;
    let ciphertext = encrypt(&plaintext, passphrase)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        set_private_permissions(parent, 0o700)?;
    }

    let temp_path = path.with_extension("age.tmp");
    std::fs::write(&temp_path, &ciphertext)?;
    set_private_permissions(&temp_path, 0o600)?;
    std::fs::rename(&temp_path, path)?;

    Ok(())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path, mode: u32) -> Result<(), RsnugError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path, _mode: u32) -> Result<(), RsnugError> {
    Ok(())
}

fn encrypt(plaintext: &[u8], passphrase: &SecretString) -> Result<Vec<u8>, RsnugError> {
    let encryptor = age::Encryptor::with_user_passphrase(passphrase.clone());
    let mut ciphertext = vec![];
    let mut writer = encryptor
        .wrap_output(&mut ciphertext)
        .map_err(RsnugError::Io)?;
    writer.write_all(plaintext).map_err(RsnugError::Io)?;
    writer.finish().map_err(RsnugError::Io)?;
    Ok(ciphertext)
}

fn decrypt(ciphertext: &[u8], passphrase: &SecretString) -> Result<Vec<u8>, RsnugError> {
    let decryptor = age::Decryptor::new(ciphertext).map_err(|_| RsnugError::DecryptionFailed)?;
    let mut reader = decryptor
        .decrypt(std::iter::once(
            &age::scrypt::Identity::new(passphrase.clone()) as _,
        ))
        .map_err(|_| RsnugError::DecryptionFailed)?;
    let mut plaintext = vec![];
    reader
        .read_to_end(&mut plaintext)
        .map_err(|_| RsnugError::DecryptionFailed)?;
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passphrase(value: &str) -> SecretString {
        SecretString::from(value.to_owned())
    }

    #[test]
    fn encrypt_then_decrypt_round_trips() {
        let plaintext = b"hello vault";
        let ciphertext = encrypt(plaintext, &passphrase("correct")).expect("encrypt");
        let decrypted = decrypt(&ciphertext, &passphrase("correct")).expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_with_wrong_passphrase_fails() {
        let ciphertext = encrypt(b"hello vault", &passphrase("correct")).expect("encrypt");
        let result = decrypt(&ciphertext, &passphrase("wrong"));
        assert!(result.is_err());
    }

    #[test]
    fn default_path_honors_xdg_config_home() {
        temp_env(
            &[("XDG_CONFIG_HOME", Some("/tmp/xdg-test")), ("HOME", None)],
            || {
                let path = default_path().expect("path");
                assert_eq!(path, PathBuf::from("/tmp/xdg-test/rsnug/vault.age"));
            },
        );
    }

    #[test]
    fn default_path_falls_back_to_home() {
        temp_env(
            &[("XDG_CONFIG_HOME", None), ("HOME", Some("/tmp/home-test"))],
            || {
                let path = default_path().expect("path");
                assert_eq!(
                    path,
                    PathBuf::from("/tmp/home-test/.config/rsnug/vault.age")
                );
            },
        );
    }

    fn temp_env(vars: &[(&str, Option<&str>)], body: impl FnOnce()) {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let previous: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|(name, _)| ((*name).to_owned(), std::env::var(name).ok()))
            .collect();

        for (name, value) in vars {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }

        body();

        for (name, value) in previous {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(&name, value),
                    None => std::env::remove_var(&name),
                }
            }
        }
    }
}
