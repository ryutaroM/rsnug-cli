use crate::error::RsnugError;
use crate::fsutil;
use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const WAIT: Duration = Duration::from_secs(5);
const RETRY: Duration = Duration::from_millis(25);

pub struct VaultLock(File);

impl Drop for VaultLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

pub fn acquire(vault: &Path) -> Result<VaultLock, RsnugError> {
    acquire_within(vault, WAIT)
}

fn acquire_within(vault: &Path, wait: Duration) -> Result<VaultLock, RsnugError> {
    fsutil::prepare_parent(vault)?;
    let file = open_lock_file(&lock_path(vault))?;

    let deadline = Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(VaultLock(file)),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(err)) => return Err(RsnugError::Io(err)),
        }
        if Instant::now() >= deadline {
            return Err(RsnugError::VaultLocked(vault.to_path_buf()));
        }
        std::thread::sleep(RETRY);
    }
}

fn open_lock_file(path: &Path) -> Result<File, RsnugError> {
    match fsutil::private_options().create_new(true).open(path) {
        Ok(_) => fsutil::set_private_permissions(path, 0o600)?,
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(err) => return Err(RsnugError::Io(err)),
    }
    Ok(File::open(path)?)
}

fn lock_path(vault: &Path) -> PathBuf {
    let mut name = resolve(vault).into_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

// Two names for one vault must land on one lock file. A vault that does not
// exist yet is resolved through its directory instead.
fn resolve(vault: &Path) -> PathBuf {
    if let Ok(resolved) = vault.canonicalize() {
        return resolved;
    }
    let Some(name) = vault.file_name() else {
        return vault.to_path_buf();
    };
    let parent = match vault.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    parent
        .canonicalize()
        .map(|parent| parent.join(name))
        .unwrap_or_else(|_| vault.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRIEF: Duration = Duration::from_millis(50);

    #[test]
    fn the_lock_file_sits_next_to_the_vault() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("vault.age");

        let _guard = acquire_within(&vault, BRIEF).expect("acquire");

        assert!(dir.path().join("vault.age.lock").exists());
    }

    #[test]
    fn a_second_acquire_gives_up_while_the_first_is_held() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("vault.age");
        let _guard = acquire_within(&vault, BRIEF).expect("acquire");

        let result = acquire_within(&vault, BRIEF);

        assert!(
            matches!(result, Err(RsnugError::VaultLocked(reported)) if reported == vault),
            "a held lock must stop the second writer, and name the vault it guards"
        );
    }

    #[test]
    fn dropping_the_guard_releases_the_lock() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("vault.age");

        drop(acquire_within(&vault, BRIEF).expect("acquire"));

        acquire_within(&vault, BRIEF).expect("the lock must be free again");
    }

    #[test]
    fn a_vault_in_a_missing_directory_can_still_be_locked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("fresh").join("vault.age");

        let _guard = acquire_within(&vault, BRIEF).expect("acquire");

        assert!(vault.parent().expect("parent").exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_lock_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("vault.age");

        let _guard = acquire_within(&vault, BRIEF).expect("acquire");

        let mode = std::fs::metadata(lock_path(&vault))
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn the_lock_file_does_not_take_the_place_of_the_vault() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("vault.age");

        let expected = dir
            .path()
            .canonicalize()
            .expect("canonicalize")
            .join("vault.age.lock");
        assert_eq!(lock_path(&vault), expected);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_the_vault_takes_the_same_lock_as_the_vault() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("vault.age");
        std::fs::write(&vault, b"vault").expect("write");
        let link = dir.path().join("link.age");
        std::os::unix::fs::symlink(&vault, &link).expect("symlink");

        let _guard = acquire_within(&vault, BRIEF).expect("acquire");

        assert!(
            matches!(
                acquire_within(&link, BRIEF),
                Err(RsnugError::VaultLocked(_))
            ),
            "two names for one vault must contend for one lock"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_directory_takes_the_same_lock_as_the_real_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let real = dir.path().join("real");
        std::fs::create_dir(&real).expect("create dir");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");

        assert_eq!(
            lock_path(&link.join("vault.age")),
            lock_path(&real.join("vault.age")),
            "an uninitialized vault is named by its directory, which init must resolve too"
        );
    }

    #[test]
    fn a_bare_name_takes_the_same_lock_as_its_full_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cwd = std::env::current_dir().expect("cwd");

        std::env::set_current_dir(dir.path()).expect("chdir");
        let bare = lock_path(Path::new("vault.age"));
        std::env::set_current_dir(&cwd).expect("chdir back");

        assert_eq!(bare, lock_path(&dir.path().join("vault.age")));
    }
}
