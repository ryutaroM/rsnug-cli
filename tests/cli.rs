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
    set_mode(path, 0o600);
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) {}

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

const VAULT_PATH_COMMANDS: [&[&str]; 6] = [
    &["set", "KEY", "VALUE"],
    &["unset", "KEY"],
    &["get", "KEY"],
    &["list"],
    &["init"],
    &["init", "--force"],
];

fn assert_vault_unavailable(output: &Output, vault: &Path, needle: &str, args: &[&str]) {
    let message = stderr(output);
    assert_eq!(code(output), 4, "{args:?}: {message}");
    assert_eq!(stdout(output), "", "{args:?}");
    assert!(message.contains(needle), "{args:?}: {message}");
    assert!(
        message.contains(vault.to_str().expect("utf-8 path")),
        "{args:?}: {message}"
    );
}

fn assert_every_command_rejects(vault: &Path, key: &Path, needle: &str) {
    for args in VAULT_PATH_COMMANDS {
        let output = run_with_vault(args, vault, key);
        assert_vault_unavailable(&output, vault, needle, args);
    }
}

#[test]
fn every_command_rejects_a_vault_that_is_a_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    write_key(&key);
    std::fs::create_dir(&vault).expect("create dir");

    assert_every_command_rejects(&vault, &key, "is not a file");
}

#[test]
fn every_command_rejects_a_vault_path_no_user_can_inspect() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("key");
    write_key(&key);
    let file = dir.path().join("not-a-directory");
    std::fs::write(&file, b"payload").expect("write");

    assert_every_command_rejects(&file.join("vault.age"), &key, "cannot be read");
}

#[cfg(unix)]
#[test]
fn every_command_rejects_a_vault_behind_a_directory_it_cannot_enter() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("key");
    write_key(&key);
    let blocked = dir.path().join("blocked");
    let vault = blocked.join("inner").join("vault.age");
    std::fs::create_dir_all(vault.parent().expect("parent")).expect("create dir");
    set_mode(&blocked, 0o000);

    let denied = matches!(
        std::fs::metadata(&vault),
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied
    );
    if !denied {
        set_mode(&blocked, 0o700);
        eprintln!(
            "SKIP: this user traverses a mode-000 directory (root?); \
             asserting the same contract on a path no user can inspect instead"
        );
        let file = dir.path().join("not-a-directory");
        std::fs::write(&file, b"payload").expect("write");
        assert_every_command_rejects(&file.join("vault.age"), &key, "cannot be read");
        return;
    }

    let outputs: Vec<Output> = VAULT_PATH_COMMANDS
        .iter()
        .map(|args| run_with_vault(args, &vault, &key))
        .collect();
    set_mode(&blocked, 0o700);

    for (args, output) in VAULT_PATH_COMMANDS.iter().zip(&outputs) {
        assert_vault_unavailable(output, &vault, "cannot be read", args);
    }
}

#[test]
fn every_command_rejects_a_vault_path_that_names_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("key");
    write_key(&key);
    let missing = dir.path().join("missing");

    assert_every_command_rejects(&missing.join(".."), &key, "cannot be read");
    assert_every_command_rejects(
        &missing.join("..").join("vault.age"),
        &key,
        "cannot be read",
    );
}

#[test]
fn every_reading_command_reports_a_missing_vault_and_init_creates_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    write_key(&key);

    for args in [
        &["set", "KEY", "VALUE"][..],
        &["unset", "KEY"][..],
        &["get", "KEY"][..],
        &["list"][..],
    ] {
        let output = run_with_vault(args, &vault, &key);
        assert_vault_unavailable(&output, &vault, "vault not found", args);
    }

    assert_eq!(code(&run_with_vault(&["init"], &vault, &key)), 0);
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

    assert_vault_unavailable(&output, &vault, "is not a file", &["init", "--force"]);
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
    set_mode(&only_first, 0o600);

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
    set_mode(&key, 0o644);

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

#[test]
fn init_force_with_a_mistyped_key_path_does_not_advise_deleting_the_vault() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);

    let output = run_with_vault(&["init", "--force"], &vault, &dir.path().join("typo"));

    assert_eq!(code(&output), 4);
    assert!(
        !stderr(&output).contains("delete the file"),
        "a wrong --key-file is not a reason to tell the user to destroy a good vault: {}",
        stderr(&output)
    );
}

#[test]
fn init_force_on_a_legacy_vault_points_at_migrate_not_deletion() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    write_key(&key);
    legacy_vault(&vault, "pw", r#"{"KEY":"VALUE"}"#);
    let original = std::fs::read(&vault).expect("read vault");

    let output = run_with_vault(&["init", "--force"], &vault, &key);

    assert_eq!(code(&output), 4);
    assert!(
        stderr(&output).contains("migrate"),
        "migrate can rescue this vault, so deletion is the wrong advice: {}",
        stderr(&output)
    );
    assert_eq!(std::fs::read(&vault).expect("read vault"), original);
}

#[test]
fn init_new_key_on_a_fresh_machine_creates_exactly_one_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");

    assert_eq!(
        code(&run_with_vault(&["init", "--new-key"], &vault, &key)),
        0
    );

    let contents = std::fs::read_to_string(&key).expect("read key");
    let secrets = contents
        .lines()
        .filter(|line| line.starts_with("AGE-SECRET-KEY-1"))
        .count();
    assert_eq!(
        secrets, 1,
        "the first identity would otherwise be dead weight"
    );
}

#[test]
fn migrate_refuses_to_clobber_an_existing_backup() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    legacy_vault(&vault, "pw", r#"{"KEY":"VALUE"}"#);
    let backup = dir.path().join("vault.age.bak");
    std::fs::write(&backup, b"someone else's backup").expect("write backup");

    let output = run_with_passphrase(&["migrate"], &vault, &key, "pw");

    assert_ne!(code(&output), 0);
    assert_eq!(
        std::fs::read(&backup).expect("read backup"),
        b"someone else's backup"
    );
}

#[cfg(unix)]
#[test]
fn migrate_that_stops_at_the_key_file_leaves_no_backup_behind() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    legacy_vault(&vault, "pw", r#"{"KEY":"VALUE"}"#);
    write_key(&key);
    set_mode(&key, 0o644);

    let refused = run_with_passphrase(&["migrate"], &vault, &key, "pw");

    assert_ne!(code(&refused), 0);
    let backup = dir.path().join("vault.age.bak");
    assert!(
        !backup.exists(),
        "a migrate that never rewrote the vault must not leave a backup that blocks the retry"
    );

    set_mode(&key, 0o600);
    let retried = run_with_passphrase(&["migrate"], &vault, &key, "pw");

    assert_eq!(code(&retried), 0, "{}", stderr(&retried));
    assert_eq!(run_ok(&["get", "KEY", "--reveal"], &vault, &key), "VALUE\n");
}

#[test]
fn migrate_reuses_the_backup_it_already_made_of_the_same_vault() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    legacy_vault(&vault, "pw", r#"{"KEY":"VALUE"}"#);
    let original = std::fs::read(&vault).expect("read vault");
    let backup = dir.path().join("vault.age.bak");
    std::fs::write(&backup, &original).expect("write backup");

    let output = run_with_passphrase(&["migrate"], &vault, &key, "pw");

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(std::fs::read(&backup).expect("read backup"), original);
    assert_eq!(run_ok(&["get", "KEY", "--reveal"], &vault, &key), "VALUE\n");
}

#[cfg(unix)]
#[test]
fn a_key_file_that_could_not_be_written_is_not_left_behind() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");

    let output = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "ulimit -f 0; exec '{}' --vault '{}' --key-file '{}' init",
            env!("CARGO_BIN_EXE_rsnug"),
            vault.display(),
            key.display()
        ))
        .stdin(Stdio::null())
        .env_remove("RSNUG_PASSPHRASE")
        .env_remove("RSNUG_KEY_FILE")
        .output()
        .expect("failed to run rsnug");

    assert!(!output.status.success());
    assert!(
        !key.exists(),
        "a key file rsnug could not write must not survive to block the next init"
    );

    let retried = run_with_vault(&["init"], &vault, &key);

    assert_eq!(code(&retried), 0, "{}", stderr(&retried));
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &vault, &key)),
        0
    );
    assert_eq!(run_ok(&["get", "KEY", "--reveal"], &vault, &key), "VALUE\n");
}

#[test]
fn a_bare_relative_key_file_name_is_usable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut command = Command::new(env!("CARGO_BIN_EXE_rsnug"));
    command
        .current_dir(dir.path())
        .args(["--vault", "v.age", "--key-file", "mykey", "init"])
        .stdin(Stdio::null())
        .env_remove("RSNUG_PASSPHRASE")
        .env_remove("RSNUG_KEY_FILE");
    let output = command.output().expect("failed to run rsnug");

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(dir.path().join("mykey").exists());
}

fn spawn_concurrent_sets(vault: &Path, key: &Path, count: usize) -> Vec<Output> {
    let children: Vec<_> = (0..count)
        .map(|n| {
            let name = format!("KEY{n}");
            scoped(&["set", &name, "VALUE"], vault, key)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("failed to spawn rsnug")
        })
        .collect();
    children
        .into_iter()
        .map(|child| child.wait_with_output().expect("failed to wait for rsnug"))
        .collect()
}

#[test]
fn concurrent_sets_do_not_lose_a_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);

    let outputs = spawn_concurrent_sets(&vault, &key, 8);

    for output in &outputs {
        assert_eq!(code(output), 0, "{}", stderr(output));
    }
    let listed: Vec<String> = run_ok(&["list"], &vault, &key)
        .lines()
        .map(str::to_owned)
        .collect();
    let expected: Vec<String> = (0..8).map(|n| format!("KEY{n}")).collect();
    assert_eq!(
        listed, expected,
        "every set reported success, so every key must be in the vault"
    );
}

fn hold_lock(vault: &Path) -> std::fs::File {
    let mut name = vault.as_os_str().to_owned();
    name.push(".lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(std::path::PathBuf::from(name))
        .expect("open lock file");
    file.lock().expect("lock");
    file
}

fn spawn(command: &mut Command) -> std::process::Child {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn rsnug")
}

#[test]
fn a_writer_waits_for_the_lock_instead_of_racing_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    let held = hold_lock(&vault);

    let child = spawn(&mut scoped(&["set", "KEY", "VALUE"], &vault, &key));
    std::thread::sleep(std::time::Duration::from_millis(300));
    drop(held);
    let output = child.wait_with_output().expect("failed to wait for rsnug");

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(run_ok(&["get", "KEY", "--reveal"], &vault, &key), "VALUE\n");
}

#[test]
fn every_mutating_command_refuses_a_locked_vault() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &vault, &key)),
        0
    );
    let legacy = dir.path().join("legacy.age");
    let legacy_key = dir.path().join("legacy-key");
    legacy_vault(&legacy, "pw", r#"{"KEY":"VALUE"}"#);

    let held = hold_lock(&vault);
    let held_legacy = hold_lock(&legacy);
    let children = vec![
        spawn(&mut scoped(&["set", "KEY", "OTHER"], &vault, &key)),
        spawn(&mut scoped(&["unset", "KEY"], &vault, &key)),
        spawn(&mut scoped(&["init", "--force"], &vault, &key)),
        spawn(scoped(&["migrate"], &legacy, &legacy_key).env("RSNUG_PASSPHRASE", "pw")),
    ];
    let outputs: Vec<Output> = children
        .into_iter()
        .map(|child| child.wait_with_output().expect("failed to wait for rsnug"))
        .collect();
    drop(held);
    drop(held_legacy);

    for output in &outputs {
        assert_eq!(code(output), 5, "{}", stderr(output));
        assert_eq!(stdout(output), "");
        assert!(stderr(output).contains("locked"), "{}", stderr(output));
    }
    assert_eq!(run_ok(&["get", "KEY", "--reveal"], &vault, &key), "VALUE\n");
    assert!(
        !legacy_key.exists(),
        "a refused migrate must not write a key file"
    );
}

#[test]
fn reads_do_not_wait_for_a_writer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &vault, &key)),
        0
    );
    let held = hold_lock(&vault);

    let started = std::time::Instant::now();
    assert_eq!(run_ok(&["list"], &vault, &key), "KEY\n");
    assert_eq!(run_ok(&["get", "KEY", "--reveal"], &vault, &key), "VALUE\n");
    let elapsed = started.elapsed();

    drop(held);
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "a reader must not queue behind a writer, took {elapsed:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_restrictive_umask_does_not_wedge_the_lock_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "umask 0277; exec '{}' --vault '{}' --key-file '{}' init",
            env!("CARGO_BIN_EXE_rsnug"),
            vault.display(),
            key.display()
        ))
        .stdin(Stdio::null())
        .env_remove("RSNUG_PASSPHRASE")
        .env_remove("RSNUG_KEY_FILE")
        .output()
        .expect("failed to run rsnug");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));

    let written = run_with_vault(&["set", "KEY", "VALUE"], &vault, &key);

    assert_eq!(
        code(&written),
        0,
        "a lock file created under a tight umask must not lock the owner out: {}",
        stderr(&written)
    );
}

#[cfg(unix)]
#[test]
fn a_lock_file_the_owner_cannot_open_points_at_itself() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    let lock = dir.path().join("vault.age.lock");
    set_mode(&lock, 0o000);
    if std::fs::File::open(&lock).is_ok() {
        eprintln!("skipped: this process opens {} at mode 000", lock.display());
        return;
    }

    let output = run_with_vault(&["set", "KEY", "VALUE"], &vault, &key);

    assert_eq!(code(&output), 4, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("vault.age.lock"),
        "a user cannot fix a lock file the message never names: {}",
        stderr(&output)
    );
}

#[cfg(unix)]
#[test]
fn a_lock_path_that_leads_nowhere_points_at_itself() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    init_vault(&vault, &key);
    let lock = dir.path().join("vault.age.lock");
    std::fs::remove_file(&lock).expect("remove lock file");
    std::os::unix::fs::symlink(dir.path().join("gone"), &lock).expect("symlink");

    let output = run_with_vault(&["set", "KEY", "VALUE"], &vault, &key);

    assert_eq!(code(&output), 4, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("vault.age.lock"),
        "a user cannot fix a lock file the message never names: {}",
        stderr(&output)
    );
}

#[test]
fn a_command_that_finds_no_vault_leaves_nothing_behind() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("key");
    write_key(&key);

    for (n, args) in [
        vec!["set", "KEY", "VALUE"],
        vec!["unset", "KEY"],
        vec!["migrate"],
    ]
    .into_iter()
    .enumerate()
    {
        let root = dir.path().join(format!("root{n}"));
        let vault = root.join("nested").join("vault.age");
        let output = scoped(&args, &vault, &key)
            .env("RSNUG_PASSPHRASE", "pw")
            .stdin(Stdio::null())
            .output()
            .expect("failed to run rsnug");

        assert_eq!(code(&output), 4, "{args:?}: {}", stderr(&output));
        assert!(
            !root.exists(),
            "{args:?} found no vault, so it must not leave a directory or a lock file behind"
        );
    }
}

#[cfg(unix)]
#[test]
fn an_init_refused_by_its_key_file_leaves_no_directory_behind() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("key");
    write_key(&key);
    set_mode(&key, 0o644);
    let root = dir.path().join("root");
    let vault = root.join("nested").join("vault.age");

    let output = run_with_vault(&["init"], &vault, &key);

    assert_eq!(code(&output), 4, "{}", stderr(&output));
    assert!(
        !root.exists(),
        "an init rsnug refused must not leave a directory or a lock file behind"
    );
}

#[test]
fn an_init_with_an_invalid_key_file_creates_no_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("key");
    std::fs::write(&key, "not an age identity\n").expect("write key");
    set_mode(&key, 0o600);
    let root = dir.path().join("root");
    let vault = root.join("nested").join("vault.age");

    let output = run_with_vault(&["init"], &vault, &key);

    assert_eq!(code(&output), 4, "{}", stderr(&output));
    assert!(
        !root.exists(),
        "a key file that is not an age identity must stop init before it creates anything"
    );
}

#[test]
fn init_refuses_a_vault_that_appeared_while_it_waited_for_the_lock() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    let key = dir.path().join("key");
    let planted = dir.path().join("planted.age");
    init_vault(&planted, &key);
    let held = hold_lock(&vault);

    let child = spawn(&mut scoped(&["init"], &vault, &key));
    std::thread::sleep(std::time::Duration::from_millis(300));
    std::fs::copy(&planted, &vault).expect("plant a vault under the waiting init");
    drop(held);
    let output = child.wait_with_output().expect("failed to wait for rsnug");

    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert_eq!(
        std::fs::read(&vault).expect("read vault"),
        std::fs::read(&planted).expect("read planted"),
        "a vault written while init waited on the lock must survive it"
    );
}

#[test]
fn concurrent_inits_share_one_key_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("key");
    let first = dir.path().join("first.age");
    let second = dir.path().join("second.age");

    let children = vec![
        spawn(&mut scoped(&["init"], &first, &key)),
        spawn(&mut scoped(&["init"], &second, &key)),
    ];
    let outputs: Vec<Output> = children
        .into_iter()
        .map(|child| child.wait_with_output().expect("failed to wait for rsnug"))
        .collect();

    for output in &outputs {
        assert_eq!(
            code(output),
            0,
            "an init that lost the race for the key file must reuse it: {}",
            stderr(output)
        );
    }
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &first, &key)),
        0
    );
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &second, &key)),
        0
    );
}

#[cfg(unix)]
fn linked_vault(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let store = dir.join("store");
    std::fs::create_dir(&store).expect("create dir");
    let vault = store.join("vault.age");
    let alias = dir.join("alias.age");
    std::os::unix::fs::symlink(&vault, &alias).expect("symlink");
    (vault, alias)
}

#[cfg(unix)]
fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .expect("symlink metadata")
        .file_type()
        .is_symlink()
}

#[cfg(unix)]
#[test]
fn a_set_through_a_symlink_lands_on_the_vault_it_points_at() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("key");
    let (vault, alias) = linked_vault(dir.path());
    init_vault(&vault, &key);
    let before = std::fs::read(&vault).expect("read vault");

    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "VALUE"], &alias, &key)),
        0
    );

    assert!(
        is_symlink(&alias),
        "a write through a link must leave the link a link"
    );
    assert_ne!(
        std::fs::read(&vault).expect("read vault"),
        before,
        "a write through a link must land on the vault the link points at"
    );
    assert_eq!(run_ok(&["get", "KEY", "--reveal"], &vault, &key), "VALUE\n");
    assert_eq!(
        run_ok(&["list"], &alias, &key),
        run_ok(&["list"], &vault, &key),
        "two names for one vault must list the same keys"
    );
}

#[cfg(unix)]
#[test]
fn an_init_through_a_symlink_creates_the_vault_it_points_at() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("key");
    let (vault, alias) = linked_vault(dir.path());

    assert_eq!(code(&run_with_vault(&["init"], &alias, &key)), 0);

    assert!(
        is_symlink(&alias),
        "an init through a link must leave the link a link"
    );
    assert!(
        vault.exists(),
        "an init through a link must create the vault the link points at"
    );
}

#[cfg(unix)]
#[test]
fn a_write_through_a_symlink_waits_on_the_lock_of_the_vault_it_points_at() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("key");
    let (vault, alias) = linked_vault(dir.path());
    init_vault(&vault, &key);

    let held = hold_lock(&vault);
    let output = run_with_vault(&["set", "KEY", "VALUE"], &alias, &key);
    drop(held);

    assert_eq!(
        code(&output),
        5,
        "one vault under two names must have one lock: {}",
        stderr(&output)
    );
}
