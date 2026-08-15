use crate::error::RsnugError;
use age::secrecy::ExposeSecret;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub const ENV_VAR: &str = "RSNUG_KEY_FILE";

pub fn default_path() -> Result<PathBuf, RsnugError> {
    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME")
        && !config_home.is_empty()
    {
        return Ok(PathBuf::from(config_home).join("rsnug").join("key"));
    }
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => Ok(PathBuf::from(home)
            .join(".config")
            .join("rsnug")
            .join("key")),
        _ => Err(RsnugError::HomeDirectoryUnavailable),
    }
}

pub fn resolve_path(explicit: Option<PathBuf>) -> Result<PathBuf, RsnugError> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Ok(value) = std::env::var(ENV_VAR)
        && !value.is_empty()
    {
        return Ok(PathBuf::from(value));
    }
    default_path()
}

pub fn load(path: &Path) -> Result<Vec<age::x25519::Identity>, RsnugError> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(RsnugError::KeyFileNotFound(path.to_path_buf()));
        }
        Err(err) => return Err(RsnugError::Io(err)),
    };

    check_permissions(path, &metadata)?;

    let contents = std::fs::read_to_string(path)?;
    let identities = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(age::x25519::Identity::from_str)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| RsnugError::KeyFileInvalid(path.to_path_buf()))?;

    if identities.is_empty() {
        return Err(RsnugError::KeyFileInvalid(path.to_path_buf()));
    }

    Ok(identities)
}

pub fn generate(path: &Path) -> Result<age::x25519::Identity, RsnugError> {
    if path.exists() {
        return Err(RsnugError::KeyFileAlreadyExists(path.to_path_buf()));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        set_private_permissions(parent, 0o700)?;
    }

    let identity = age::x25519::Identity::generate();
    let temp_path = temp_path(path);
    std::fs::write(&temp_path, entry(&identity).as_bytes())?;
    set_private_permissions(&temp_path, 0o600)?;
    std::fs::rename(&temp_path, path)?;

    Ok(identity)
}

pub fn append(path: &Path) -> Result<age::x25519::Identity, RsnugError> {
    use std::io::Write;

    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(RsnugError::KeyFileNotFound(path.to_path_buf()));
        }
        Err(err) => return Err(RsnugError::Io(err)),
    };
    check_permissions(path, &metadata)?;

    let identity = age::x25519::Identity::generate();
    let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
    file.write_all(entry(&identity).as_bytes())?;
    file.sync_all()?;

    Ok(identity)
}

fn entry(identity: &age::x25519::Identity) -> String {
    format!(
        "# public key: {}\n{}\n",
        identity.to_public(),
        identity.to_string().expose_secret()
    )
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

#[cfg(unix)]
fn check_permissions(path: &Path, metadata: &std::fs::Metadata) -> Result<(), RsnugError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = metadata.permissions().mode();
    if mode & 0o077 != 0 {
        return Err(RsnugError::KeyFilePermissions(
            path.to_path_buf(),
            mode & 0o777,
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_permissions(_path: &Path, _metadata: &std::fs::Metadata) -> Result<(), RsnugError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_honors_xdg_config_home() {
        temp_env(
            &[("XDG_CONFIG_HOME", Some("/tmp/xdg-test")), ("HOME", None)],
            || {
                let path = default_path().expect("path");
                assert_eq!(path, PathBuf::from("/tmp/xdg-test/rsnug/key"));
            },
        );
    }

    #[test]
    fn default_path_falls_back_to_home() {
        temp_env(
            &[("XDG_CONFIG_HOME", None), ("HOME", Some("/tmp/home-test"))],
            || {
                let path = default_path().expect("path");
                assert_eq!(path, PathBuf::from("/tmp/home-test/.config/rsnug/key"));
            },
        );
    }

    #[test]
    fn default_path_without_a_home_is_an_error() {
        temp_env(&[("XDG_CONFIG_HOME", None), ("HOME", None)], || {
            assert!(matches!(
                default_path(),
                Err(RsnugError::HomeDirectoryUnavailable)
            ));
        });
    }

    #[test]
    fn an_explicit_path_beats_the_environment() {
        temp_env(&[(ENV_VAR, Some("/tmp/from-env"))], || {
            let path = resolve_path(Some(PathBuf::from("/tmp/explicit"))).expect("path");
            assert_eq!(path, PathBuf::from("/tmp/explicit"));
        });
    }

    #[test]
    fn the_environment_beats_the_default_path() {
        temp_env(
            &[
                (ENV_VAR, Some("/tmp/from-env")),
                ("XDG_CONFIG_HOME", Some("/tmp/xdg-test")),
            ],
            || {
                let path = resolve_path(None).expect("path");
                assert_eq!(path, PathBuf::from("/tmp/from-env"));
            },
        );
    }

    #[test]
    fn an_empty_environment_variable_falls_through_to_the_default_path() {
        temp_env(
            &[
                (ENV_VAR, Some("")),
                ("XDG_CONFIG_HOME", Some("/tmp/xdg-test")),
                ("HOME", None),
            ],
            || {
                let path = resolve_path(None).expect("path");
                assert_eq!(path, PathBuf::from("/tmp/xdg-test/rsnug/key"));
            },
        );
    }

    #[test]
    fn a_generated_key_file_round_trips_through_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key");

        let generated = generate(&path).expect("generate");
        let loaded = load(&path).expect("load");

        assert_eq!(
            loaded[0].to_public().to_string(),
            generated.to_public().to_string()
        );
    }

    #[test]
    fn load_skips_comment_and_blank_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key");
        let identity = age::x25519::Identity::generate();
        write_key_file(
            &path,
            &format!(
                "# created by age-keygen\n\n   # indented comment\n{}\n",
                identity.to_string().expose_secret()
            ),
        );

        let loaded = load(&path).expect("load");

        assert_eq!(
            loaded[0].to_public().to_string(),
            identity.to_public().to_string()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_key_file_that_is_group_readable_is_rejected() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key");
        let identity = age::x25519::Identity::generate();
        write_key_file(&path, identity.to_string().expose_secret());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");

        match load(&path).err() {
            Some(RsnugError::KeyFilePermissions(reported, mode)) => {
                assert_eq!(reported, path);
                assert_eq!(mode, 0o644);
            }
            other => panic!("expected KeyFilePermissions, got {other:?}"),
        }
    }

    #[test]
    fn a_key_file_without_an_age_secret_key_is_invalid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key");
        write_key_file(&path, "# public key: age1nothinghere\n\n");

        assert!(matches!(load(&path), Err(RsnugError::KeyFileInvalid(_))));
    }

    #[test]
    fn a_key_file_whose_first_line_is_not_a_key_is_invalid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key");
        write_key_file(&path, "not-a-key\n");

        assert!(matches!(load(&path), Err(RsnugError::KeyFileInvalid(_))));
    }

    #[test]
    fn a_missing_key_file_is_reported_as_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("missing");

        match load(&path).err() {
            Some(RsnugError::KeyFileNotFound(reported)) => assert_eq!(reported, path),
            other => panic!("expected KeyFileNotFound, got {other:?}"),
        }
    }

    #[test]
    fn generate_never_overwrites_an_existing_key_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key");
        let original = b"AGE-SECRET-KEY-ORIGINAL\n";
        write_key_file(&path, "AGE-SECRET-KEY-ORIGINAL\n");

        match generate(&path).err() {
            Some(RsnugError::KeyFileAlreadyExists(reported)) => assert_eq!(reported, path),
            other => panic!("expected KeyFileAlreadyExists, got {other:?}"),
        }
        assert_eq!(std::fs::read(&path).expect("read"), original);
    }

    #[cfg(unix)]
    #[test]
    fn a_generated_key_file_is_only_readable_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("key");
        generate(&path).expect("generate");

        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        let parent = std::fs::metadata(path.parent().expect("parent"))
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(parent, 0o700);
    }

    #[test]
    fn a_generated_key_file_carries_its_public_key_as_a_comment() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key");

        let identity = generate(&path).expect("generate");

        let contents = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            contents,
            format!(
                "# public key: {}\n{}\n",
                identity.to_public(),
                identity.to_string().expose_secret()
            )
        );
    }

    #[test]
    fn generate_leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key");
        generate(&path).expect("generate");

        let entries: Vec<PathBuf> = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .map(|entry| entry.expect("entry").path())
            .collect();
        assert_eq!(entries, vec![path]);
    }

    #[test]
    fn load_returns_every_identity_in_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key");
        let first = age::x25519::Identity::generate();
        let second = age::x25519::Identity::generate();
        write_key_file(&path, &format!("{}{}", entry(&first), entry(&second)));

        let loaded = load(&path).expect("load");

        assert_eq!(
            loaded
                .iter()
                .map(|identity| identity.to_public().to_string())
                .collect::<Vec<_>>(),
            vec![
                first.to_public().to_string(),
                second.to_public().to_string()
            ]
        );
    }

    #[test]
    fn a_single_unparsable_line_invalidates_the_whole_key_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key");
        let good = age::x25519::Identity::generate();
        write_key_file(&path, &format!("{}AGE-SECRET-KEY-1NOTREAL\n", entry(&good)));

        let result = load(&path);

        assert!(
            matches!(result, Err(RsnugError::KeyFileInvalid(reported)) if reported == path),
            "a broken line is a lost key, so it must not be skipped silently"
        );
    }

    #[test]
    fn append_adds_an_identity_and_keeps_the_existing_ones() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key");
        let original = generate(&path).expect("generate");

        let added = append(&path).expect("append");
        let loaded = load(&path).expect("load");

        assert_eq!(loaded.len(), 2);
        assert_eq!(
            loaded[0].to_public().to_string(),
            original.to_public().to_string()
        );
        assert_eq!(
            loaded[1].to_public().to_string(),
            added.to_public().to_string()
        );
    }

    #[test]
    fn append_on_a_missing_key_file_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");

        let result = append(&dir.path().join("key"));

        assert!(matches!(result, Err(RsnugError::KeyFileNotFound(_))));
    }

    fn write_key_file(path: &Path, contents: &str) {
        std::fs::write(path, contents).expect("write");
        set_private_permissions(path, 0o600).expect("chmod");
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
