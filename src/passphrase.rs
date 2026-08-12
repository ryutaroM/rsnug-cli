use crate::error::RsnugError;
use age::secrecy::SecretString;

pub const ENV_VAR: &str = "RSNUG_PASSPHRASE";

pub fn resolve() -> Result<SecretString, RsnugError> {
    match std::env::var(ENV_VAR) {
        Ok(value) if !value.is_empty() => Ok(SecretString::from(value)),
        _ => Err(RsnugError::LegacyPassphraseMissing),
    }
}
