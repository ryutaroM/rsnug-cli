use std::path::Path;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rsnug"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("failed to run rsnug")
}

fn run_with_vault(args: &[&str], vault: &Path, passphrase: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rsnug"));
    command
        .arg("--vault")
        .arg(vault)
        .args(args)
        .stdin(Stdio::null());
    match passphrase {
        Some(value) => command.env("RSNUG_PASSPHRASE", value),
        None => command.env_remove("RSNUG_PASSPHRASE"),
    };
    command.output().expect("failed to run rsnug")
}

fn run_with_vault_stdin(
    args: &[&str],
    vault: &Path,
    passphrase: Option<&str>,
    input: &str,
) -> Output {
    use std::io::Write;

    let mut command = Command::new(env!("CARGO_BIN_EXE_rsnug"));
    command
        .arg("--vault")
        .arg(vault)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match passphrase {
        Some(value) => command.env("RSNUG_PASSPHRASE", value),
        None => command.env_remove("RSNUG_PASSPHRASE"),
    };
    let mut child = command.spawn().expect("failed to spawn rsnug");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("failed to wait for rsnug")
}

fn init_vault(vault: &Path, passphrase: &str) {
    assert_eq!(code(&run_with_vault(&["init"], vault, Some(passphrase))), 0);
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

fn assert_lists_commands(text: &str) {
    for command in ["init", "set", "get", "list"] {
        assert!(
            text.contains(command),
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

    let output = run_with_vault(&["list"], &vault, Some("pw"));

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
}

#[test]
fn list_on_freshly_initialized_vault_is_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");

    let output = run_with_vault(&["list"], &vault, Some("pw"));

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "");
}

#[test]
fn list_returns_all_keys_sorted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    for (key, value) in [("ZEBRA", "1"), ("APPLE", "2"), ("MANGO", "3")] {
        assert_eq!(
            code(&run_with_vault(&["set", key, value], &vault, Some("pw"))),
            0
        );
    }

    let output = run_with_vault(&["list"], &vault, Some("pw"));

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

    let output = run_with_vault(&["set", "KEY", "--stdin"], &vault, None);

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

    let output = run_with_vault(&["init"], &vault, Some("correct horse"));

    assert_eq!(code(&output), 0);
    assert!(vault.exists());
    assert!(stdout(&output).contains(vault.to_str().unwrap()));
    assert_eq!(stderr(&output), "");
}

#[test]
fn init_without_passphrase_env_fails_with_vault_unavailable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");

    let output = run_with_vault(&["init"], &vault, None);

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
    assert!(!vault.exists());
}

#[test]
fn init_twice_without_force_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");

    assert_eq!(code(&run_with_vault(&["init"], &vault, Some("pw"))), 0);
    let second = run_with_vault(&["init"], &vault, Some("pw"));

    assert_eq!(code(&second), 1);
    assert_eq!(stdout(&second), "");
}

#[test]
fn init_with_force_overwrites() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");

    assert_eq!(code(&run_with_vault(&["init"], &vault, Some("pw"))), 0);
    let second = run_with_vault(&["init", "--force"], &vault, Some("pw"));

    assert_eq!(code(&second), 0);
}

#[test]
fn set_on_missing_vault_fails_with_vault_unavailable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");

    let output = run_with_vault(&["set", "KEY", "VALUE"], &vault, Some("pw"));

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
}

#[test]
fn set_succeeds_after_init() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");

    let output = run_with_vault(&["set", "KEY", "VALUE"], &vault, Some("pw"));

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "Set KEY\n");
}

#[test]
fn set_with_stdin_reads_the_piped_value() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");

    let output = run_with_vault_stdin(&["set", "KEY", "--stdin"], &vault, Some("pw"), "VALUE\n");

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "Set KEY\n");
}

#[test]
fn set_then_get_reveal_round_trips_the_value() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    assert_eq!(
        code(&run_with_vault(
            &["set", "KEY", "VALUE"],
            &vault,
            Some("pw")
        )),
        0
    );

    let output = run_with_vault(&["get", "KEY", "--reveal"], &vault, Some("pw"));

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "VALUE\n");
}

#[test]
fn get_without_reveal_shows_metadata_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    assert_eq!(
        code(&run_with_vault(
            &["set", "KEY", "VALUE"],
            &vault,
            Some("pw")
        )),
        0
    );

    let output = run_with_vault(&["get", "KEY"], &vault, Some("pw"));

    assert_eq!(code(&output), 0);
    assert!(!stdout(&output).contains("VALUE"));
}

#[test]
fn get_without_reveal_never_leaks_value_in_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    assert_eq!(
        code(&run_with_vault(
            &["set", "KEY", "VALUE"],
            &vault,
            Some("pw")
        )),
        0
    );

    let output = run_with_vault(&["--format", "json", "get", "KEY"], &vault, Some("pw"));

    assert_eq!(code(&output), 0);
    let text = stdout(&output);
    assert!(!text.contains("value"));
    assert!(!text.contains("VALUE"));
}

#[test]
fn get_missing_key_fails_with_key_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");

    let output = run_with_vault(&["get", "NOPE"], &vault, Some("pw"));

    assert_eq!(code(&output), 3);
    assert_eq!(stdout(&output), "");
}

#[test]
fn get_with_wrong_passphrase_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw-a");
    assert_eq!(
        code(&run_with_vault(
            &["set", "KEY", "VALUE"],
            &vault,
            Some("pw-a")
        )),
        0
    );

    let output = run_with_vault(&["get", "KEY", "--reveal"], &vault, Some("pw-b"));

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
}
