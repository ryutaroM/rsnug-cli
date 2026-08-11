use crate::exit;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum RsnugError {
    PassphraseMissing,
    VaultNotFound(PathBuf),
    VaultAlreadyExists(PathBuf),
    VaultNotOverwritable(PathBuf),
    DecryptionFailed,
    KeyNotFound(String),
    TrashGenerationNotFound { key: String, at: u64 },
    KeyAlreadyExists(String),
    UnsupportedVaultVersion { found: u32, expected: u32 },
    HomeDirectoryUnavailable,
    Io(std::io::Error),
    Serialization(serde_json::Error),
}

impl RsnugError {
    pub fn exit_code(&self) -> u8 {
        match self {
            RsnugError::PassphraseMissing => exit::VAULT_UNAVAILABLE,
            RsnugError::VaultNotFound(_) => exit::VAULT_UNAVAILABLE,
            RsnugError::VaultNotOverwritable(_) => exit::VAULT_UNAVAILABLE,
            RsnugError::DecryptionFailed => exit::VAULT_UNAVAILABLE,
            RsnugError::VaultAlreadyExists(_) => exit::GENERAL_ERROR,
            RsnugError::KeyAlreadyExists(_) => exit::GENERAL_ERROR,
            RsnugError::UnsupportedVaultVersion { .. } => exit::GENERAL_ERROR,
            RsnugError::HomeDirectoryUnavailable => exit::GENERAL_ERROR,
            RsnugError::Io(_) => exit::GENERAL_ERROR,
            RsnugError::Serialization(_) => exit::GENERAL_ERROR,
            RsnugError::KeyNotFound(_) => exit::KEY_NOT_FOUND,
            RsnugError::TrashGenerationNotFound { .. } => exit::KEY_NOT_FOUND,
        }
    }
}

impl fmt::Display for RsnugError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RsnugError::PassphraseMissing => {
                write!(f, "RSNUG_PASSPHRASE is not set")
            }
            RsnugError::VaultNotFound(path) => {
                write!(f, "vault not found at {}", path.display())
            }
            RsnugError::VaultAlreadyExists(path) => {
                write!(
                    f,
                    "vault already exists at {} (use --force)",
                    path.display()
                )
            }
            RsnugError::VaultNotOverwritable(path) => {
                write!(
                    f,
                    "refusing to overwrite {} because RSNUG_PASSPHRASE does not open it (delete the file yourself to start over)",
                    path.display()
                )
            }
            RsnugError::DecryptionFailed => {
                write!(
                    f,
                    "failed to decrypt vault (wrong passphrase or corrupt file)"
                )
            }
            RsnugError::KeyNotFound(key) => write!(f, "key `{key}` not found"),
            RsnugError::TrashGenerationNotFound { key, at } => {
                write!(
                    f,
                    "no trashed generation of `{key}` at {}",
                    crate::timestamp::format(*at)
                )
            }
            RsnugError::KeyAlreadyExists(key) => {
                write!(
                    f,
                    "key `{key}` is live; restoring it would overwrite the current value"
                )
            }
            RsnugError::UnsupportedVaultVersion { found, expected } => {
                write!(f, "unsupported vault version {found} (expected {expected})")
            }
            RsnugError::HomeDirectoryUnavailable => {
                write!(
                    f,
                    "could not determine a default vault path (set --vault explicitly)"
                )
            }
            RsnugError::Io(err) => write!(f, "{err}"),
            RsnugError::Serialization(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for RsnugError {}

impl From<std::io::Error> for RsnugError {
    fn from(err: std::io::Error) -> Self {
        RsnugError::Io(err)
    }
}

impl From<serde_json::Error> for RsnugError {
    fn from(err: serde_json::Error) -> Self {
        RsnugError::Serialization(err)
    }
}
