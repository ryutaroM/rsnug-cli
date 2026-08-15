use age::secrecy::ExposeSecret;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn base(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rsnug"));
    command
        .args(args)
        .env_remove("RSNUG_PASSPHRASE")
        .env_remove("RSNUG_KEY_FILE");
    command
}

fn run(args: &[&str]) -> Output {
    base(args)
        .stdin(Stdio::null())
        .output()
        .expect("failed to run rsnug")
}

fn scoped(args: &[&str], vault: &Path, key: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rsnug"));
    command
        .arg("--vault")
        .arg(vault)
        .arg("--key-file")
        .arg(key)
        .args(args)
        .env_remove("RSNUG_PASSPHRASE")
        .env_remove("RSNUG_KEY_FILE");
    command
}

fn run_with_vault(args: &[&str], vault: &Path, key: &Path) -> Output {
    scoped(args, vault, key)
        .stdin(Stdio::null())
        .output()
        .expect("failed to run rsnug")
}

fn run_with_passphrase(args: &[&str], vault: &Path, key: &Path, passphrase: &str) -> Output {
    scoped(args, vault, key)
        .env("RSNUG_PASSPHRASE", passphrase)
        .stdin(Stdio::null())
        .output()
        .expect("failed to run rsnug")
}

fn run_with_vault_stdin(args: &[&str], vault: &Path, key: &Path, input: &str) -> Output {
    use std::io::Write;

    let mut child = scoped(args, vault, key)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn rsnug");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("failed to wait for rsnug")
}

fn init_vault(vault: &Path, key: &Path) {
    assert_eq!(code(&run_with_vault(&["init"], vault, key)), 0);
}

fn write_key(path: &Path) {
    let identity = age::x25519::Identity::generate();
    std::fs::write(path, format!("{}\n", identity.to_string().expose_secret())).expect("write key");
    set_key_mode(path, 0o600);
}

#[cfg(unix)]
fn set_key_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

#[cfg(not(unix))]
fn set_key_mode(_path: &Path, _mode: u32) {}

fn legacy_vault(path: &Path, passphrase: &str, entries: &str) {
    use std::io::Write;

    let mut recipient =
        age::scrypt::Recipient::new(age::secrecy::SecretString::from(passphrase.to_owned()));
    recipient.set_work_factor(10);
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as _)).expect("encryptor");
    let mut ciphertext = vec![];
    let mut writer = encryptor.wrap_output(&mut ciphertext).expect("wrap");
    writer
        .write_all(format!(r#"{{"version":1,"entries":{entries}}}"#).as_bytes())
        .expect("write");
    writer.finish().expect("finish");
    std::fs::write(path, &ciphertext).expect("write vault");
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is not utf-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is not utf-8")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("terminated by signal")
}

fn run_ok(args: &[&str], vault: &Path, key: &Path) -> String {
    let output = run_with_vault(args, vault, key);
    assert_eq!(code(&output), 0, "{args:?}: {}", stderr(&output));
    stdout(&output)
}

fn assert_lists_commands(text: &str) {
    for command in ["init", "set", "get", "list", "unset"] {
        assert!(
            text.lines()
                .any(|line| line.trim_start().starts_with(command)),
            "help should list `{command}`: {text}"
        );
    }
}

#[test]
fn long_help_succeeds_and_lists_commands() {
    let output = run(&["--help"]);
    assert_eq!(code(&output), 0);
    assert_lists_commands(&stdout(&output));
}

#[test]
fn short_help_succeeds_and_lists_commands() {
    let output = run(&["-h"]);
    assert_eq!(code(&output), 0);
    assert_lists_commands(&stdout(&output));
}

#[test]
fn version_flag_reports_package_version() {
    let output = run(&["--version"]);
    assert_eq!(code(&output), 0);
    assert!(stdout(&output).contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn no_arguments_shows_help() {
    let output = run(&[]);
    assert_lists_commands(&format!("{}{}", stdout(&output), stderr(&output)));
}

#[test]
fn list_on_missing_vault_fails_with_vault_unavailable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");

    let output = run_with_vault(&["list"], &vault, &key);

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
}

#[test]
fn list_on_freshly_initialized_vault_is_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);

    let output = run_with_vault(&["list"], &vault, &key);

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "");
}

#[test]
fn list_returns_all_keys_sorted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    for (name, value) in [("ZEBRA", "1"), ("APPLE", "2"), ("MANGO", "3")] {
        assert_eq!(
            code(&run_with_vault(&["set", name, value], &vault, &key)),
            0
        );
    }

    let output = run_with_vault(&["list"], &vault, &key);

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "APPLE\nMANGO\nZEBRA\n");
}

#[test]
fn set_without_value_or_stdin_is_a_usage_error() {
    assert_eq!(code(&run(&["set", "KEY"])), 2);
}

#[test]
fn set_with_both_value_and_stdin_is_a_usage_error() {
    assert_eq!(code(&run(&["set", "KEY", "VALUE", "--stdin"])), 2);
}

#[test]
fn set_with_stdin_does_not_block_on_closed_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");

    let output = run_with_vault(&["set", "KEY", "--stdin"], &vault, &key);

    assert_ne!(code(&output), 2);
}

#[test]
fn unknown_command_is_a_usage_error() {
    assert_eq!(code(&run(&["bogus"])), 2);
}

#[test]
fn init_creates_a_vault_file_and_prints_its_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");

    let output = run_with_vault(&["init"], &vault, &key);

    assert_eq!(code(&output), 0);
    assert!(vault.exists());
    assert!(stdout(&output).contains(vault.to_str().unwrap()));
    assert_eq!(stderr(&output), "");
}

#[test]
fn init_generates_the_key_file_and_names_it_in_the_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");

    let output = run_with_vault(&["init"], &vault, &key);

    assert_eq!(code(&output), 0);
    assert!(key.exists());
    assert!(
        stdout(&output).contains(key.to_str().expect("utf-8 path")),
        "init must say which key file the vault was bound to: {}",
        stdout(&output)
    );
}

#[cfg(unix)]
#[test]
fn init_creates_the_key_file_private_to_the_owner() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);

    let mode = std::fs::metadata(&key)
        .expect("metadata")
        .permissions()
        .mode();

    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn init_never_overwrites_an_existing_key_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    let original = std::fs::read(&key).expect("read key");

    assert_eq!(code(&run_with_vault(&["init", "--force"], &vault, &key)), 0);

    assert_eq!(
        std::fs::read(&key).expect("read key"),
        original,
        "overwriting the key file would make the vault unopenable forever"
    );
}

#[test]
fn a_command_without_a_key_file_fails_with_vault_unavailable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    std::fs::remove_file(&key).expect("remove key");

    let output = run_with_vault(&["list"], &vault, &key);

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
    assert!(vault.exists());
}

#[test]
fn init_twice_without_force_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");

    assert_eq!(code(&run_with_vault(&["init"], &vault, &key)), 0);
    let second = run_with_vault(&["init"], &vault, &key);

    assert_eq!(code(&second), 1);
    assert_eq!(stdout(&second), "");
}

#[test]
fn init_with_force_overwrites() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");

    assert_eq!(code(&run_with_vault(&["init"], &vault, &key)), 0);
    let second = run_with_vault(&["init", "--force"], &vault, &key);

    assert_eq!(code(&second), 0);
}

#[test]
fn init_with_force_and_the_right_passphrase_empties_the_vault() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &vault, &key)),
        0
    );

    assert_eq!(code(&run_with_vault(&["init", "--force"], &vault, &key)), 0);

    let output = run_with_vault(&["list"], &vault, &key);
    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "");
}

#[test]
fn init_with_force_and_the_wrong_passphrase_keeps_the_vault_intact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &vault, &key)),
        0
    );

    let other = dir.path().join("other-key");
    write_key(&other);

    let output = run_with_vault(&["init", "--force"], &vault, &other);

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
    assert_eq!(
        stdout(&run_with_vault(&["get", "KEY", "--reveal"], &vault, &key)),
        "VALUE\n"
    );
}

#[test]
fn init_with_force_on_an_undecryptable_vault_says_to_delete_the_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    write_key(&key);
    std::fs::write(&vault, b"not an age file").expect("write");

    let output = run_with_vault(&["init", "--force"], &vault, &key);

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("delete"), "{}", stderr(&output));
    assert_eq!(std::fs::read(&vault).expect("read"), b"not an age file");
}

#[test]
fn init_with_force_on_an_unreadable_vault_does_not_blame_the_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    write_key(&key);
    std::fs::create_dir(&vault).expect("create dir");

    let output = run_with_vault(&["init", "--force"], &vault, &key);

    assert_eq!(code(&output), 1);
    assert_eq!(stdout(&output), "");
    assert!(
        !stderr(&output).contains("key file"),
        "an unreadable vault path is not a key problem: {}",
        stderr(&output)
    );
    assert!(vault.is_dir());
}

#[test]
fn set_on_missing_vault_fails_with_vault_unavailable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");

    let output = run_with_vault(&["set", "KEY", "VALUE"], &vault, &key);

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
}

#[test]
fn set_succeeds_after_init() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);

    let output = run_with_vault(&["set", "KEY", "VALUE"], &vault, &key);

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "Set KEY\n");
}

#[test]
fn set_with_stdin_reads_the_piped_value() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);

    let output = run_with_vault_stdin(&["set", "KEY", "--stdin"], &vault, &key, "VALUE\n");

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "Set KEY\n");
}

#[test]
fn set_then_get_reveal_round_trips_the_value() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &vault, &key)),
        0
    );

    let output = run_with_vault(&["get", "KEY", "--reveal"], &vault, &key);

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "VALUE\n");
}

#[test]
fn get_without_reveal_shows_metadata_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &vault, &key)),
        0
    );

    let output = run_with_vault(&["get", "KEY"], &vault, &key);

    assert_eq!(code(&output), 0);
    assert!(!stdout(&output).contains("VALUE"));
}

#[test]
fn get_without_reveal_never_leaks_value_in_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &vault, &key)),
        0
    );

    let output = run_with_vault(&["--format", "json", "get", "KEY"], &vault, &key);

    assert_eq!(code(&output), 0);
    let text = stdout(&output);
    assert!(!text.contains("value"));
    assert!(!text.contains("VALUE"));
}

#[test]
fn get_missing_key_fails_with_key_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);

    let output = run_with_vault(&["get", "NOPE"], &vault, &key);

    assert_eq!(code(&output), 3);
    assert_eq!(stdout(&output), "");
}

#[test]
fn unset_removes_the_key_from_list_and_get() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &vault, &key)),
        0
    );

    let output = run_with_vault(&["unset", "KEY"], &vault, &key);

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "Unset KEY\n");
    assert_eq!(stdout(&run_with_vault(&["list"], &vault, &key)), "");
    assert_eq!(code(&run_with_vault(&["get", "KEY"], &vault, &key)), 3);
}

#[test]
fn unset_leaves_other_keys_alone() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    for (name, value) in [("ONE", "1"), ("TWO", "2")] {
        assert_eq!(
            code(&run_with_vault(&["set", name, value], &vault, &key)),
            0
        );
    }

    assert_eq!(code(&run_with_vault(&["unset", "ONE"], &vault, &key)), 0);

    let output = run_with_vault(&["list"], &vault, &key);
    assert_eq!(stdout(&output), "TWO\n");
}

#[test]
fn unset_twice_fails_the_second_time() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    run_ok(&["set", "KEY", "VALUE"], &vault, &key);
    run_ok(&["unset", "KEY"], &vault, &key);

    let output = run_with_vault(&["unset", "KEY"], &vault, &key);

    assert_eq!(code(&output), 3);
    assert_eq!(stdout(&output), "");
}

#[test]
fn set_over_a_live_key_replaces_the_value() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    run_ok(&["set", "KEY", "OLD"], &vault, &key);

    run_ok(&["set", "KEY", "NEW"], &vault, &key);

    assert_eq!(run_ok(&["get", "KEY", "--reveal"], &vault, &key), "NEW\n");
    assert_eq!(run_ok(&["list"], &vault, &key), "KEY\n");
}

#[test]
fn restore_is_not_a_command() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);

    let output = run_with_vault(&["restore", "KEY"], &vault, &key);

    assert_eq!(code(&output), 2);
    assert_eq!(stdout(&output), "");
}

#[test]
fn list_rejects_a_trash_flag() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);

    let output = run_with_vault(&["list", "--trash"], &vault, &key);

    assert_eq!(code(&output), 2);
    assert_eq!(stdout(&output), "");
}

#[test]
fn unset_missing_key_fails_with_key_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);

    let output = run_with_vault(&["unset", "NOPE"], &vault, &key);

    assert_eq!(code(&output), 3);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
}

#[test]
fn unset_on_missing_vault_fails_with_vault_unavailable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");

    let output = run_with_vault(&["unset", "KEY"], &vault, &key);

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
}

#[test]
fn get_with_wrong_passphrase_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &vault, &key)),
        0
    );

    let other = dir.path().join("other-key");
    write_key(&other);

    let output = run_with_vault(&["get", "KEY", "--reveal"], &vault, &other);

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
}

#[test]
fn a_passphrase_vault_is_reported_as_legacy_rather_than_unopenable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    write_key(&key);
    legacy_vault(&vault, "pw", r#"{"KEY":"VALUE"}"#);

    let output = run_with_vault(&["list"], &vault, &key);

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains("migrate"),
        "a legacy vault must point at migrate, not look like a wrong key: {}",
        stderr(&output)
    );
}

#[test]
fn migrate_moves_a_passphrase_vault_onto_the_key_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    legacy_vault(&vault, "pw", r#"{"KEY":"VALUE"}"#);

    let migrated = run_with_passphrase(&["migrate"], &vault, &key, "pw");

    assert_eq!(code(&migrated), 0, "{}", stderr(&migrated));
    assert!(key.exists());
    assert_eq!(run_ok(&["get", "KEY", "--reveal"], &vault, &key), "VALUE\n");
}

#[test]
fn migrate_leaves_a_backup_of_the_original_vault() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    legacy_vault(&vault, "pw", r#"{"KEY":"VALUE"}"#);
    let original = std::fs::read(&vault).expect("read vault");

    assert_eq!(
        code(&run_with_passphrase(&["migrate"], &vault, &key, "pw")),
        0
    );

    let backup = dir.path().join("vault.age.bak");
    assert_eq!(std::fs::read(&backup).expect("read backup"), original);
}

#[test]
fn migrate_without_the_passphrase_fails_without_touching_the_vault() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    legacy_vault(&vault, "pw", r#"{"KEY":"VALUE"}"#);
    let original = std::fs::read(&vault).expect("read vault");

    let output = run_with_vault(&["migrate"], &vault, &key);

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
    assert_eq!(std::fs::read(&vault).expect("read vault"), original);
    assert!(!key.exists());
}

#[test]
fn migrate_refuses_a_vault_that_already_uses_a_key_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);

    let output = run_with_passphrase(&["migrate"], &vault, &key, "pw");

    assert_eq!(code(&output), 1);
    assert_eq!(stdout(&output), "");
}

#[test]
fn one_key_file_serves_several_vaults_with_separate_identities() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("key");
    let first = dir.path().join("first.age");
    let second = dir.path().join("second.age");
    init_vault(&first, &key);
    assert_eq!(
        code(&run_with_vault(&["init", "--new-key"], &second, &key)),
        0
    );

    assert_eq!(code(&run_with_vault(&["set", "A", "1"], &first, &key)), 0);
    assert_eq!(code(&run_with_vault(&["set", "B", "2"], &second, &key)), 0);

    assert_eq!(run_ok(&["get", "A", "--reveal"], &first, &key), "1\n");
    assert_eq!(run_ok(&["get", "B", "--reveal"], &second, &key), "2\n");
}

#[test]
fn writing_to_a_vault_does_not_rebind_it_to_another_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("key");
    let first = dir.path().join("first.age");
    let second = dir.path().join("second.age");
    init_vault(&first, &key);
    assert_eq!(
        code(&run_with_vault(&["init", "--new-key"], &second, &key)),
        0
    );
    assert_eq!(code(&run_with_vault(&["set", "B", "2"], &second, &key)), 0);

    let contents = std::fs::read_to_string(&key).expect("read key");
    let secrets: Vec<&str> = contents
        .lines()
        .filter(|line| line.starts_with("AGE-SECRET-KEY-1"))
        .collect();
    assert_eq!(secrets.len(), 2, "--new-key must append, not replace");

    let only_first = dir.path().join("only-first");
    std::fs::write(&only_first, format!("{}\n", secrets[0])).expect("write");
    set_key_mode(&only_first, 0o600);

    assert_ne!(
        code(&run_with_vault(&["list"], &second, &only_first)),
        0,
        "the second vault must still be bound to its own identity after a write"
    );
}

#[cfg(unix)]
#[test]
fn a_group_readable_key_file_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    set_key_mode(&key, 0o644);

    let output = run_with_vault(&["list"], &vault, &key);

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("600"), "{}", stderr(&output));
}

#[test]
fn a_legacy_vault_without_a_key_file_still_points_at_migrate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    legacy_vault(&vault, "pw", r#"{"KEY":"VALUE"}"#);

    let output = run_with_vault(&["list"], &vault, &key);

    assert_eq!(code(&output), 4);
    assert!(
        stderr(&output).contains("migrate"),
        "telling the user to run init here sends them the wrong way: {}",
        stderr(&output)
    );
}
