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
    let path = lock_path(vault);
    fsutil::prepare_parent(&path)?;
    let file = open_lock_file(&path)?;

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
    File::open(path).map_err(|err| RsnugError::LockFileUnopenable(path.to_path_buf(), err))
}

fn lock_path(vault: &Path) -> PathBuf {
    let mut name = vault.as_os_str().to_owned();
    name.push(".lock");
    PathBuf::from(name)
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

    #[cfg(unix)]
    #[test]
    fn a_lock_file_that_will_not_open_names_itself() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("vault.age");
        let path = lock_path(&vault);
        std::fs::write(&path, b"").expect("write lock file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        if File::open(&path).is_ok() {
            return;
        }

        let Err(err) = acquire_within(&vault, BRIEF) else {
            panic!("a lock file rsnug cannot open must not yield a lock");
        };

        assert!(
            matches!(&err, RsnugError::LockFileUnopenable(reported, _) if reported == &path),
            "the error must name the lock file, got {err:?}"
        );
        assert!(err.to_string().contains("vault.age.lock"), "{err}");
    }

    #[test]
    fn the_lock_file_does_not_take_the_place_of_the_vault() {
        let vault = Path::new("/tmp/rsnug/vault.age");

        assert_eq!(lock_path(vault), Path::new("/tmp/rsnug/vault.age.lock"));
    }
}
