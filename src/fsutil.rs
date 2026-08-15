use crate::error::RsnugError;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn prepare_parent(path: &Path) -> Result<(), RsnugError> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    if parent.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(parent)?;
    set_private_permissions(parent, 0o700)
}

pub fn create_private(path: &Path) -> Result<File, RsnugError> {
    let file = private_options().create_new(true).open(path)?;
    set_private_permissions(path, 0o600)?;
    Ok(file)
}

pub fn write_private_atomic(path: &Path, bytes: &[u8]) -> Result<(), RsnugError> {
    let temp = temp_sibling(path);
    let mut file = private_options().create_new(true).open(&temp)?;
    set_private_permissions(&temp, 0o600)?;
    if let Err(err) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        let _ = std::fs::remove_file(&temp);
        return Err(RsnugError::Io(err));
    }
    drop(file);
    if let Err(err) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(RsnugError::Io(err));
    }
    Ok(())
}

fn temp_sibling(path: &Path) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or_default();
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{}.{nanos}.tmp", std::process::id()));
    PathBuf::from(name)
}

#[cfg(unix)]
fn private_options() -> std::fs::OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).mode(0o600);
    options
}

#[cfg(not(unix))]
fn private_options() -> std::fs::OpenOptions {
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    options
}

#[cfg(unix)]
pub fn set_private_permissions(path: &Path, mode: u32) -> Result<(), RsnugError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
pub fn set_private_permissions(_path: &Path, _mode: u32) -> Result<(), RsnugError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_parent_leaves_an_existing_directory_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let shared = dir.path().join("shared");
        std::fs::create_dir(&shared).expect("create dir");
        set_private_permissions(&shared, 0o755).expect("chmod");

        prepare_parent(&shared.join("key")).expect("prepare");

        assert_eq!(
            mode_of(&shared),
            0o755,
            "a directory rsnug did not create is not rsnug to lock down"
        );
    }

    #[test]
    fn prepare_parent_locks_down_a_directory_it_creates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fresh = dir.path().join("fresh");

        prepare_parent(&fresh.join("key")).expect("prepare");

        assert_eq!(mode_of(&fresh), 0o700);
    }

    #[test]
    fn prepare_parent_accepts_a_bare_relative_name() {
        assert!(prepare_parent(Path::new("mykey")).is_ok());
    }

    #[test]
    fn create_private_refuses_an_existing_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key");
        std::fs::write(&path, b"original").expect("write");

        assert!(create_private(&path).is_err());
        assert_eq!(std::fs::read(&path).expect("read"), b"original");
    }

    #[cfg(unix)]
    #[test]
    fn write_private_atomic_never_exposes_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("secret");

        write_private_atomic(&path, b"payload").expect("write");

        assert_eq!(std::fs::read(&path).expect("read"), b"payload");
        assert_eq!(mode_of(&path), 0o600);
    }

    #[test]
    fn write_private_atomic_replaces_an_existing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("secret");
        write_private_atomic(&path, b"first").expect("write");

        write_private_atomic(&path, b"second").expect("write");

        assert_eq!(std::fs::read(&path).expect("read"), b"second");
    }

    #[test]
    fn write_private_atomic_leaves_no_temp_file_behind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("secret");

        write_private_atomic(&path, b"payload").expect("write");

        let entries: Vec<PathBuf> = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .map(|entry| entry.expect("entry").path())
            .collect();
        assert_eq!(entries, vec![path]);
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    }

    #[cfg(not(unix))]
    fn mode_of(_path: &Path) -> u32 {
        0o700
    }
}
