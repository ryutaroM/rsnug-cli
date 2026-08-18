use crate::exit;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum RsnugError {
    KeyFileNotFound(PathBuf),
    KeyFilePermissions(PathBuf, u32),
    KeyFileInvalid(PathBuf),
    KeyFileAlreadyExists(PathBuf),
    LegacyVault(PathBuf),
    LegacyPassphraseMissing,
    ExcessiveWork { required: u8, max: u8 },
    VaultAlreadyMigrated(PathBuf),
    BackupAlreadyExists(PathBuf),
    VaultNotFound(PathBuf),
    VaultAlreadyExists(PathBuf),
    VaultNotOverwritable(PathBuf),
    DecryptionFailed,
    KeyNotFound(String),
    UnsupportedVaultVersion { found: u32, expected: u32 },
    HomeDirectoryUnavailable,
    Io(std::io::Error),
    Serialization(serde_json::Error),
}

impl RsnugError {
    pub fn exit_code(&self) -> u8 {
        match self {
            RsnugError::KeyFileNotFound(_) => exit::VAULT_UNAVAILABLE,
            RsnugError::KeyFilePermissions(_, _) => exit::VAULT_UNAVAILABLE,
            RsnugError::KeyFileInvalid(_) => exit::VAULT_UNAVAILABLE,
            RsnugError::LegacyVault(_) => exit::VAULT_UNAVAILABLE,
            RsnugError::LegacyPassphraseMissing => exit::VAULT_UNAVAILABLE,
            RsnugError::ExcessiveWork { .. } => exit::VAULT_UNAVAILABLE,
            RsnugError::VaultNotFound(_) => exit::VAULT_UNAVAILABLE,
            RsnugError::VaultNotOverwritable(_) => exit::VAULT_UNAVAILABLE,
            RsnugError::DecryptionFailed => exit::VAULT_UNAVAILABLE,
            RsnugError::KeyFileAlreadyExists(_) => exit::GENERAL_ERROR,
            RsnugError::VaultAlreadyMigrated(_) => exit::GENERAL_ERROR,
            RsnugError::BackupAlreadyExists(_) => exit::GENERAL_ERROR,
            RsnugError::VaultAlreadyExists(_) => exit::GENERAL_ERROR,
            RsnugError::UnsupportedVaultVersion { .. } => exit::GENERAL_ERROR,
            RsnugError::HomeDirectoryUnavailable => exit::GENERAL_ERROR,
            RsnugError::Io(_) => exit::GENERAL_ERROR,
            RsnugError::Serialization(_) => exit::GENERAL_ERROR,
            RsnugError::KeyNotFound(_) => exit::KEY_NOT_FOUND,
        }
    }
}

impl fmt::Display for RsnugError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RsnugError::KeyFileNotFound(path) => {
                write!(
                    f,
                    "key file not found at {} (run `rsnug init` to create one)",
                    path.display()
                )
            }
            RsnugError::KeyFilePermissions(path, mode) => {
                write!(
                    f,
                    "key file {} has mode {mode:o}, expected 600 (run `chmod 600 {}`)",
                    path.display(),
                    path.display()
                )
            }
            RsnugError::KeyFileInvalid(path) => {
                write!(
                    f,
                    "key file {} does not contain an age secret key",
                    path.display()
                )
            }
            RsnugError::KeyFileAlreadyExists(path) => {
                write!(
                    f,
                    "key file already exists at {} (rsnug never overwrites a key file)",
                    path.display()
                )
            }
            RsnugError::LegacyVault(path) => {
                write!(
                    f,
                    "vault at {} is passphrase-encrypted (run `rsnug migrate`)",
                    path.display()
                )
            }
            RsnugError::LegacyPassphraseMissing => {
                write!(f, "RSNUG_PASSPHRASE is not set")
            }
            RsnugError::ExcessiveWork { required, max } => {
                write!(
                    f,
                    "vault needs scrypt work factor {required}, above the {max} rsnug accepts"
                )
            }
            RsnugError::VaultAlreadyMigrated(path) => {
                write!(f, "vault at {} already uses a key file", path.display())
            }
            RsnugError::BackupAlreadyExists(path) => {
                write!(
                    f,
                    "a backup already exists at {} (move it aside so migrate does not overwrite it)",
                    path.display()
                )
            }
            RsnugError::VaultNotFound(path) => {
                write!(f, "vault not found at {}", path.display())
            }
            RsnugError::VaultAlreadyExists(path) => {
                write!(
                    f,
                    "vault already exists at {} (use --force, which requires the key file to open it)",
                    path.display()
                )
            }
            RsnugError::VaultNotOverwritable(path) => {
                write!(
                    f,
                    "refusing to overwrite {} because the key file does not open it (delete the file yourself to start over)",
                    path.display()
                )
            }
            RsnugError::DecryptionFailed => {
                write!(f, "failed to decrypt vault (wrong key or corrupt file)")
            }
            RsnugError::KeyNotFound(key) => write!(f, "key `{key}` not found"),
            RsnugError::UnsupportedVaultVersion { found, expected } => {
                write!(f, "unsupported vault version {found} (expected {expected})")
            }
            RsnugError::HomeDirectoryUnavailable => {
                write!(
                    f,
                    "could not determine a default path (set --vault and --key-file explicitly)"
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
