use crate::error::RsnugError;
use age::secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const LEGACY_VERSION: u32 = 1;
const CURRENT_VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
pub struct VaultData {
    version: u32,
    entries: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    trash: BTreeMap<String, Vec<TrashEntry>>,
}

#[derive(Serialize, Deserialize)]
pub struct TrashEntry {
    value: String,
    deleted_at: u64,
}

impl TrashEntry {
    pub fn new(value: String, deleted_at: u64) -> Self {
        Self { value, deleted_at }
    }

    pub fn deleted_at(&self) -> u64 {
        self.deleted_at
    }

    pub fn into_value(self) -> String {
        self.value
    }
}

impl VaultData {
    pub fn empty() -> Self {
        Self {
            version: LEGACY_VERSION,
            entries: BTreeMap::new(),
            trash: BTreeMap::new(),
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

    pub fn trash_push(&mut self, key: String, entry: TrashEntry) {
        self.trash.entry(key).or_default().push(entry);
        self.sync_version();
    }

    pub fn trash_pop(&mut self, key: &str) -> Option<TrashEntry> {
        let generations = self.trash.get_mut(key)?;
        let entry = generations.pop();
        self.prune(key);
        entry
    }

    pub fn trash_take_at(&mut self, key: &str, deleted_at: u64) -> Option<TrashEntry> {
        let generations = self.trash.get_mut(key)?;
        let index = generations
            .iter()
            .rposition(|entry| entry.deleted_at == deleted_at)?;
        let entry = generations.remove(index);
        self.prune(key);
        Some(entry)
    }

    fn prune(&mut self, key: &str) {
        if self.trash.get(key).is_some_and(Vec::is_empty) {
            self.trash.remove(key);
        }
        self.sync_version();
    }

    pub fn trash_entries(&self) -> impl Iterator<Item = (&str, &TrashEntry)> {
        self.trash.iter().flat_map(|(key, generations)| {
            generations.iter().map(move |entry| (key.as_str(), entry))
        })
    }

    fn sync_version(&mut self) {
        self.version = if self.trash.is_empty() {
            LEGACY_VERSION
        } else {
            CURRENT_VERSION
        };
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
    let mut data: VaultData = serde_json::from_slice(&plaintext)?;

    if data.version == 0 || data.version > CURRENT_VERSION {
        return Err(RsnugError::UnsupportedVaultVersion {
            found: data.version,
            expected: CURRENT_VERSION,
        });
    }
    data.sync_version();

    Ok(data)
}

pub fn is_decryptable(path: &Path, passphrase: &SecretString) -> bool {
    std::fs::read(path)
        .ok()
        .is_some_and(|ciphertext| decrypt(&ciphertext, passphrase).is_ok())
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
    fn a_v1_vault_loads_with_an_empty_trash() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let v1 = br#"{"version":1,"entries":{"KEY":"VALUE"}}"#;
        std::fs::write(&path, encrypt(v1, &passphrase("pw")).expect("encrypt")).expect("write");

        let data = load(&path, &passphrase("pw")).expect("load");

        assert_eq!(data.get("KEY"), Some("VALUE"));
        assert_eq!(data.trash_entries().count(), 0);
    }

    #[test]
    fn is_decryptable_accepts_only_the_matching_passphrase() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        save(&path, &VaultData::empty(), &passphrase("pw")).expect("save");

        assert!(is_decryptable(&path, &passphrase("pw")));
        assert!(!is_decryptable(&path, &passphrase("other")));
    }

    #[test]
    fn is_decryptable_rejects_garbage_and_missing_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");

        assert!(!is_decryptable(&path, &passphrase("pw")));

        std::fs::write(&path, b"not an age file").expect("write");
        assert!(!is_decryptable(&path, &passphrase("pw")));
    }

    #[test]
    fn a_future_vault_version_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let future = br#"{"version":99,"entries":{}}"#;
        std::fs::write(&path, encrypt(future, &passphrase("pw")).expect("encrypt")).expect("write");

        assert!(load(&path, &passphrase("pw")).is_err());
    }

    #[test]
    fn trash_round_trips_through_save_and_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let mut data = VaultData::empty();
        data.insert("KEY".to_owned(), "VALUE".to_owned());
        let value = data.remove("KEY").expect("entry was present");
        data.trash_push("KEY".to_owned(), TrashEntry::new(value, 1_700_000_000));
        save(&path, &data, &passphrase("pw")).expect("save");

        let loaded = load(&path, &passphrase("pw")).expect("load");

        assert_eq!(loaded.get("KEY"), None);
        let (key, entry) = loaded.trash_entries().next().expect("one trashed entry");
        assert_eq!(key, "KEY");
        assert_eq!(entry.deleted_at(), 1_700_000_000);
    }

    #[test]
    fn trashing_the_same_key_twice_keeps_both_generations() {
        let mut data = VaultData::empty();
        data.trash_push("KEY".to_owned(), TrashEntry::new("OLD".to_owned(), 1));
        data.trash_push("KEY".to_owned(), TrashEntry::new("NEW".to_owned(), 2));

        assert_eq!(data.trash_entries().count(), 2);
        assert_eq!(data.trash_pop("KEY").expect("newest").into_value(), "NEW");
        assert_eq!(data.trash_pop("KEY").expect("oldest").into_value(), "OLD");
        assert!(data.trash_pop("KEY").is_none());
    }

    #[test]
    fn trash_take_at_returns_the_generation_with_that_timestamp() {
        let mut data = VaultData::empty();
        data.trash_push("KEY".to_owned(), TrashEntry::new("OLD".to_owned(), 1));
        data.trash_push("KEY".to_owned(), TrashEntry::new("NEW".to_owned(), 2));

        assert_eq!(
            data.trash_take_at("KEY", 1).expect("older").into_value(),
            "OLD"
        );
        assert_eq!(data.trash_entries().count(), 1);
        assert_eq!(data.trash_pop("KEY").expect("newer").into_value(), "NEW");
    }

    #[test]
    fn trash_take_at_returns_none_for_an_unknown_timestamp() {
        let mut data = VaultData::empty();
        data.trash_push("KEY".to_owned(), TrashEntry::new("VALUE".to_owned(), 1));

        assert!(data.trash_take_at("KEY", 99).is_none());
        assert_eq!(data.trash_entries().count(), 1);
    }

    #[test]
    fn trash_take_at_returns_none_for_an_unknown_key() {
        let mut data = VaultData::empty();
        data.trash_push("KEY".to_owned(), TrashEntry::new("VALUE".to_owned(), 1));

        assert!(data.trash_take_at("NOPE", 1).is_none());
        assert_eq!(data.trash_entries().count(), 1);
    }

    #[test]
    fn trash_take_at_prefers_the_newest_generation_when_timestamps_tie() {
        let mut data = VaultData::empty();
        data.trash_push("KEY".to_owned(), TrashEntry::new("A".to_owned(), 5));
        data.trash_push("KEY".to_owned(), TrashEntry::new("B".to_owned(), 5));

        assert_eq!(
            data.trash_take_at("KEY", 5).expect("newest").into_value(),
            "B"
        );
        assert_eq!(
            data.trash_take_at("KEY", 5).expect("oldest").into_value(),
            "A"
        );
        assert!(data.trash_take_at("KEY", 5).is_none());
    }

    #[test]
    fn trash_take_at_drops_back_to_v1_when_it_empties_the_trash() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let mut data = VaultData::empty();
        data.trash_push("KEY".to_owned(), TrashEntry::new("VALUE".to_owned(), 1));
        data.trash_take_at("KEY", 1).expect("entry");
        save(&path, &data, &passphrase("pw")).expect("save");

        assert_eq!(on_disk_version(&path), 1);
    }

    #[test]
    fn a_vault_without_trash_is_written_as_v1() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let mut data = VaultData::empty();
        data.insert("KEY".to_owned(), "VALUE".to_owned());
        save(&path, &data, &passphrase("pw")).expect("save");

        assert_eq!(on_disk_version(&path), 1);
    }

    #[test]
    fn a_vault_with_trash_is_written_as_v2() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let mut data = VaultData::empty();
        data.trash_push("KEY".to_owned(), TrashEntry::new("VALUE".to_owned(), 1));
        save(&path, &data, &passphrase("pw")).expect("save");

        assert_eq!(on_disk_version(&path), 2);
    }

    #[test]
    fn emptying_the_trash_makes_the_vault_v1_again() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("vault.age");
        let mut data = VaultData::empty();
        data.trash_push("KEY".to_owned(), TrashEntry::new("VALUE".to_owned(), 1));
        data.trash_pop("KEY").expect("entry");
        save(&path, &data, &passphrase("pw")).expect("save");

        assert_eq!(on_disk_version(&path), 1);
    }

    fn on_disk_version(path: &Path) -> u64 {
        let ciphertext = std::fs::read(path).expect("read");
        let plaintext = decrypt(&ciphertext, &passphrase("pw")).expect("decrypt");
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
    fn trash_pop_returns_the_stored_value_once() {
        let mut data = VaultData::empty();
        data.trash_push("KEY".to_owned(), TrashEntry::new("VALUE".to_owned(), 1));

        assert_eq!(data.trash_pop("KEY").expect("entry").into_value(), "VALUE");
        assert!(data.trash_pop("KEY").is_none());
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
