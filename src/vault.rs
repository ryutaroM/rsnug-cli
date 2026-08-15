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

    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.entries.remove(key)
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

pub fn load(path: &Path, identity: &age::x25519::Identity) -> Result<VaultData, RsnugError> {
    let ciphertext = read_vault(path)?;
    let plaintext = decrypt(&ciphertext, identity, path)?;
    parse(&plaintext)
}

#[allow(dead_code)]
pub fn load_legacy(path: &Path, passphrase: &SecretString) -> Result<VaultData, RsnugError> {
    let ciphertext = read_vault(path)?;
    let plaintext = decrypt_legacy(&ciphertext, passphrase)?;
    parse(&plaintext)
}

#[allow(dead_code)]
pub fn is_legacy(path: &Path) -> Result<bool, RsnugError> {
    let ciphertext = std::fs::read(path)?;
    let decryptor =
        age::Decryptor::new(&ciphertext[..]).map_err(|_| RsnugError::DecryptionFailed)?;
    Ok(decryptor.is_scrypt())
}

pub fn is_decryptable(path: &Path, identity: &age::x25519::Identity) -> Result<bool, RsnugError> {
    let ciphertext = std::fs::read(path)?;
    Ok(decrypt(&ciphertext, identity, path).is_ok())
}

fn read_vault(path: &Path) -> Result<Vec<u8>, RsnugError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Err(RsnugError::VaultNotFound(path.to_path_buf()))
        }
        Err(err) => Err(RsnugError::Io(err)),
    }
}

fn parse(plaintext: &[u8]) -> Result<VaultData, RsnugError> {
    let data: VaultData = serde_json::from_slice(plaintext)?;

    if data.version != CURRENT_VERSION {
        return Err(RsnugError::UnsupportedVaultVersion {
            found: data.version,
            expected: CURRENT_VERSION,
        });
    }

    Ok(data)
}

pub fn save(
    path: &Path,
    data: &VaultData,
    recipient: &age::x25519::Recipient,
) -> Result<(), RsnugError> {
    let plaintext = serde_json::to_vec(data)?;
    let ciphertext = encrypt(&plaintext, recipient)?;

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

fn encrypt(plaintext: &[u8], recipient: &age::x25519::Recipient) -> Result<Vec<u8>, RsnugError> {
    let encryptor = age::Encryptor::with_recipients(std::iter::once(recipient as _))
        .map_err(|err| RsnugError::Io(std::io::Error::other(err.to_string())))?;
    let mut ciphertext = vec![];
    let mut writer = encryptor
        .wrap_output(&mut ciphertext)
        .map_err(RsnugError::Io)?;
    writer.write_all(plaintext).map_err(RsnugError::Io)?;
    writer.finish().map_err(RsnugError::Io)?;
    Ok(ciphertext)
}

fn decrypt(
    ciphertext: &[u8],
    identity: &age::x25519::Identity,
    path: &Path,
) -> Result<Vec<u8>, RsnugError> {
    let decryptor = age::Decryptor::new(ciphertext).map_err(|_| RsnugError::DecryptionFailed)?;

    if decryptor.is_scrypt() {
        return Err(RsnugError::LegacyVault(path.to_path_buf()));
    }

    let reader = decryptor
        .decrypt(std::iter::once(identity as _))
        .map_err(|_| RsnugError::DecryptionFailed)?;
    read_plaintext(reader)
}

#[allow(dead_code)]
fn decrypt_legacy(ciphertext: &[u8], passphrase: &SecretString) -> Result<Vec<u8>, RsnugError> {
    let decryptor = age::Decryptor::new(ciphertext).map_err(|_| RsnugError::DecryptionFailed)?;
    let mut identity = age::scrypt::Identity::new(passphrase.clone());
    identity.set_max_work_factor(22);
    let reader = decryptor
        .decrypt(std::iter::once(&identity as _))
        .map_err(|_| RsnugError::DecryptionFailed)?;
    read_plaintext(reader)
}

fn read_plaintext(mut reader: impl Read) -> Result<Vec<u8>, RsnugError> {
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

    fn legacy_ciphertext(plaintext: &[u8], passphrase: &SecretString, log_n: u8) -> Vec<u8> {
        let mut recipient = age::scrypt::Recipient::new(passphrase.clone());
        recipient.set_work_factor(log_n);
        let encryptor =
            age::Encryptor::with_recipients(std::iter::once(&recipient as _)).expect("encryptor");
        let mut ciphertext = vec![];
        let mut writer = encryptor.wrap_output(&mut ciphertext).expect("wrap");
        writer.write_all(plaintext).expect("write");
        writer.finish().expect("finish");
        ciphertext
    }

    #[test]
    fn encrypt_then_decrypt_round_trips() {
        let identity = age::x25519::Identity::generate();
        let plaintext = b"hello vault";
        let ciphertext = encrypt(plaintext, &identity.to_public()).expect("encrypt");
        let decrypted = decrypt(&ciphertext, &identity, Path::new("vault.age")).expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn a_different_identity_cannot_open_the_vault() {
        let identity = age::x25519::Identity::generate();
        let other = age::x25519::Identity::generate();
        let ciphertext = encrypt(b"hello vault", &identity.to_public()).expect("encrypt");

        let result = decrypt(&ciphertext, &other, Path::new("vault.age"));

        assert!(matches!(result, Err(RsnugError::DecryptionFailed)));
    }

    #[test]
    fn is_legacy_separates_scrypt_vaults_from_identity_vaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let identity = age::x25519::Identity::generate();
        let legacy = dir.path().join("legacy.age");
        let current = dir.path().join("current.age");
        std::fs::write(&legacy, legacy_ciphertext(b"{}", &passphrase("pw"), 12)).expect("write");
        std::fs::write(
            &current,
            encrypt(b"{}", &identity.to_public()).expect("encrypt"),
        )
        .expect("write");

        assert_eq!(is_legacy(&legacy).ok(), Some(true));
        assert_eq!(is_legacy(&current).ok(), Some(false));
    }

    #[test]
    fn loading_a_scrypt_vault_reports_it_as_legacy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let v1 = br#"{"version":1,"entries":{"KEY":"VALUE"}}"#;
        std::fs::write(&path, legacy_ciphertext(v1, &passphrase("pw"), 12)).expect("write");

        let result = load(&path, &age::x25519::Identity::generate());

        assert!(matches!(result, Err(RsnugError::LegacyVault(reported)) if reported == path));
    }

    #[test]
    fn load_legacy_opens_a_vault_written_with_a_high_work_factor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let v1 = br#"{"version":1,"entries":{"KEY":"VALUE"}}"#;
        std::fs::write(&path, legacy_ciphertext(v1, &passphrase("pw"), 20)).expect("write");

        let data = load_legacy(&path, &passphrase("pw")).expect("load legacy");

        assert_eq!(data.get("KEY"), Some("VALUE"));
    }

    #[test]
    fn a_v1_vault_loads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let identity = age::x25519::Identity::generate();
        let v1 = br#"{"version":1,"entries":{"KEY":"VALUE"}}"#;
        std::fs::write(&path, encrypt(v1, &identity.to_public()).expect("encrypt")).expect("write");

        let data = load(&path, &identity).expect("load");

        assert_eq!(data.get("KEY"), Some("VALUE"));
    }

    #[test]
    fn is_decryptable_accepts_only_the_matching_identity() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let identity = age::x25519::Identity::generate();
        save(&path, &VaultData::empty(), &identity.to_public()).expect("save");

        assert_eq!(is_decryptable(&path, &identity).ok(), Some(true));
        assert_eq!(
            is_decryptable(&path, &age::x25519::Identity::generate()).ok(),
            Some(false)
        );
    }

    #[test]
    fn is_decryptable_rejects_garbage() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        std::fs::write(&path, b"not an age file").expect("write");

        assert_eq!(
            is_decryptable(&path, &age::x25519::Identity::generate()).ok(),
            Some(false)
        );
    }

    #[test]
    fn is_decryptable_reports_an_unreadable_file_as_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let identity = age::x25519::Identity::generate();

        assert!(is_decryptable(&dir.path().join("missing.age"), &identity).is_err());
        assert!(is_decryptable(dir.path(), &identity).is_err());
    }

    #[test]
    fn a_future_vault_version_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let identity = age::x25519::Identity::generate();
        let future = br#"{"version":99,"entries":{}}"#;
        std::fs::write(
            &path,
            encrypt(future, &identity.to_public()).expect("encrypt"),
        )
        .expect("write");

        assert!(load(&path, &identity).is_err());
    }

    #[test]
    fn a_saved_vault_is_written_as_v1() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let identity = age::x25519::Identity::generate();
        let mut data = VaultData::empty();
        data.insert("KEY".to_owned(), "VALUE".to_owned());
        save(&path, &data, &identity.to_public()).expect("save");

        assert_eq!(on_disk_version(&path, &identity), 1);
    }

    #[test]
    fn a_removed_entry_leaves_no_trace_in_the_saved_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let identity = age::x25519::Identity::generate();
        let mut data = VaultData::empty();
        data.insert("KEY".to_owned(), "SUPERSECRET".to_owned());
        save(&path, &data, &identity.to_public()).expect("save");
        data.remove("KEY").expect("entry was present");
        save(&path, &data, &identity.to_public()).expect("save");

        let ciphertext = std::fs::read(&path).expect("read");
        let plaintext = decrypt(&ciphertext, &identity, &path).expect("decrypt");

        assert!(!String::from_utf8_lossy(&plaintext).contains("SUPERSECRET"));
    }

    fn on_disk_version(path: &Path, identity: &age::x25519::Identity) -> u64 {
        let ciphertext = std::fs::read(path).expect("read");
        let plaintext = decrypt(&ciphertext, identity, path).expect("decrypt");
        let value: serde_json::Value = serde_json::from_slice(&plaintext).expect("json");
        value["version"].as_u64().expect("version is a number")
    }

    #[test]
    fn remove_takes_the_entry_out_of_entries() {
        let mut data = VaultData::empty();
        data.insert("KEY".to_owned(), "VALUE".to_owned());

        assert_eq!(data.remove("KEY").as_deref(), Some("VALUE"));
        assert_eq!(data.get("KEY"), None);
        assert_eq!(data.remove("KEY"), None);
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
