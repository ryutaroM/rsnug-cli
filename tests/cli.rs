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

fn run_ok(args: &[&str], vault: &Path, passphrase: &str) -> String {
    let output = run_with_vault(args, vault, Some(passphrase));
    assert_eq!(code(&output), 0, "{args:?}: {}", stderr(&output));
    stdout(&output)
}

fn trash_timestamps(vault: &Path, passphrase: &str) -> Vec<String> {
    run_ok(&["list", "--trash"], vault, passphrase)
        .lines()
        .map(|line| {
            line.split_once('\t')
                .expect("key and timestamp are tab separated")
                .1
                .to_owned()
        })
        .collect()
}

fn tick() {
    std::thread::sleep(std::time::Duration::from_secs(1));
}

fn assert_lists_commands(text: &str) {
    for command in ["init", "set", "get", "list", "unset", "restore"] {
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
fn init_with_force_and_the_right_passphrase_empties_the_vault() {
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

    assert_eq!(
        code(&run_with_vault(&["init", "--force"], &vault, Some("pw"))),
        0
    );

    let output = run_with_vault(&["list"], &vault, Some("pw"));
    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "");
}

#[test]
fn init_with_force_and_the_wrong_passphrase_keeps_the_vault_intact() {
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

    let output = run_with_vault(&["init", "--force"], &vault, Some("pw-b"));

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
    assert_eq!(
        stdout(&run_with_vault(
            &["get", "KEY", "--reveal"],
            &vault,
            Some("pw-a")
        )),
        "VALUE\n"
    );
}

#[test]
fn init_with_force_on_an_undecryptable_vault_says_to_delete_the_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    std::fs::write(&vault, b"not an age file").expect("write");

    let output = run_with_vault(&["init", "--force"], &vault, Some("pw"));

    assert_eq!(code(&output), 4);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("delete"), "{}", stderr(&output));
    assert_eq!(std::fs::read(&vault).expect("read"), b"not an age file");
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
fn unset_removes_the_key_from_list_and_get() {
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

    let output = run_with_vault(&["unset", "KEY"], &vault, Some("pw"));

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "Unset KEY\n");
    assert_eq!(stdout(&run_with_vault(&["list"], &vault, Some("pw"))), "");
    assert_eq!(
        code(&run_with_vault(&["get", "KEY"], &vault, Some("pw"))),
        3
    );
}

#[test]
fn unset_leaves_other_keys_alone() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    for (key, value) in [("ONE", "1"), ("TWO", "2")] {
        assert_eq!(
            code(&run_with_vault(&["set", key, value], &vault, Some("pw"))),
            0
        );
    }

    assert_eq!(
        code(&run_with_vault(&["unset", "ONE"], &vault, Some("pw"))),
        0
    );

    let output = run_with_vault(&["list"], &vault, Some("pw"));
    assert_eq!(stdout(&output), "TWO\n");
}

#[test]
fn list_trash_is_empty_on_a_fresh_vault() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");

    let output = run_with_vault(&["list", "--trash"], &vault, Some("pw"));

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "");
}

#[test]
fn list_trash_shows_unset_keys_with_an_rfc3339_timestamp() {
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
    assert_eq!(
        code(&run_with_vault(&["unset", "KEY"], &vault, Some("pw"))),
        0
    );

    let output = run_with_vault(&["list", "--trash"], &vault, Some("pw"));

    assert_eq!(code(&output), 0);
    let text = stdout(&output);
    let (key, deleted_at) = text
        .trim_end()
        .split_once('\t')
        .expect("key and timestamp are tab separated");
    assert_eq!(key, "KEY");
    assert_eq!(deleted_at.len(), 20);
    assert_eq!(&deleted_at[4..5], "-");
    assert_eq!(&deleted_at[10..11], "T");
    assert!(deleted_at.ends_with('Z'));
}

#[test]
fn list_trash_never_leaks_the_value() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    assert_eq!(
        code(&run_with_vault(
            &["set", "KEY", "SUPERSECRET"],
            &vault,
            Some("pw")
        )),
        0
    );
    assert_eq!(
        code(&run_with_vault(&["unset", "KEY"], &vault, Some("pw"))),
        0
    );

    for args in [
        vec!["list", "--trash"],
        vec!["--format", "json", "list", "--trash"],
    ] {
        let output = run_with_vault(&args, &vault, Some("pw"));
        assert_eq!(code(&output), 0);
        assert!(!stdout(&output).contains("SUPERSECRET"), "{args:?}");
    }
}

#[test]
fn list_trash_json_lists_keys_separately_from_live_keys() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    for (key, value) in [("GONE", "1"), ("LIVE", "2")] {
        assert_eq!(
            code(&run_with_vault(&["set", key, value], &vault, Some("pw"))),
            0
        );
    }
    assert_eq!(
        code(&run_with_vault(&["unset", "GONE"], &vault, Some("pw"))),
        0
    );

    let live = run_with_vault(&["--format", "json", "list"], &vault, Some("pw"));
    let trashed = run_with_vault(&["--format", "json", "list", "--trash"], &vault, Some("pw"));

    assert_eq!(stdout(&live), "{\"keys\":[\"LIVE\"]}\n");
    let text = stdout(&trashed);
    assert!(text.contains("\"trashed\""), "{text}");
    assert!(text.contains("\"GONE\""), "{text}");
    assert!(!text.contains("LIVE"), "{text}");
}

#[test]
fn restore_brings_back_the_unset_value() {
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
    assert_eq!(
        code(&run_with_vault(&["unset", "KEY"], &vault, Some("pw"))),
        0
    );

    let output = run_with_vault(&["restore", "KEY"], &vault, Some("pw"));

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "Restored KEY\n");
    assert_eq!(
        stdout(&run_with_vault(
            &["get", "KEY", "--reveal"],
            &vault,
            Some("pw")
        )),
        "VALUE\n"
    );
}

#[test]
fn restore_twice_fails_because_the_trash_no_longer_holds_the_key() {
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
    assert_eq!(
        code(&run_with_vault(&["unset", "KEY"], &vault, Some("pw"))),
        0
    );
    assert_eq!(
        code(&run_with_vault(&["restore", "KEY"], &vault, Some("pw"))),
        0
    );

    let output = run_with_vault(&["restore", "KEY"], &vault, Some("pw"));

    assert_eq!(code(&output), 3);
    assert_eq!(stdout(&output), "");
}

#[test]
fn restore_missing_key_fails_with_key_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");

    let output = run_with_vault(&["restore", "NOPE"], &vault, Some("pw"));

    assert_eq!(code(&output), 3);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
}

#[test]
fn restore_refuses_to_overwrite_a_live_key_and_keeps_the_trash() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "OLD"], &vault, Some("pw"))),
        0
    );
    assert_eq!(
        code(&run_with_vault(&["unset", "KEY"], &vault, Some("pw"))),
        0
    );
    assert_eq!(
        code(&run_with_vault(&["set", "KEY", "NEW"], &vault, Some("pw"))),
        0
    );

    let output = run_with_vault(&["restore", "KEY"], &vault, Some("pw"));

    assert_eq!(code(&output), 1);
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stdout(&run_with_vault(
            &["get", "KEY", "--reveal"],
            &vault,
            Some("pw")
        )),
        "NEW\n"
    );
    let trash = stdout(&run_with_vault(&["list", "--trash"], &vault, Some("pw")));
    assert_eq!(trash.lines().count(), 1, "{trash}");
}

#[test]
fn restore_at_brings_back_an_older_generation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    run_ok(&["set", "KEY", "OLD"], &vault, "pw");
    run_ok(&["unset", "KEY"], &vault, "pw");
    tick();
    run_ok(&["set", "KEY", "NEW"], &vault, "pw");
    run_ok(&["unset", "KEY"], &vault, "pw");

    let timestamps = trash_timestamps(&vault, "pw");
    assert_eq!(timestamps.len(), 2, "{timestamps:?}");
    run_ok(&["restore", "KEY", "--at", &timestamps[0]], &vault, "pw");

    assert_eq!(run_ok(&["get", "KEY", "--reveal"], &vault, "pw"), "OLD\n");
}

#[test]
fn restore_at_leaves_the_newer_generation_in_the_trash() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    run_ok(&["set", "KEY", "OLD"], &vault, "pw");
    run_ok(&["unset", "KEY"], &vault, "pw");
    tick();
    run_ok(&["set", "KEY", "NEW"], &vault, "pw");
    run_ok(&["unset", "KEY"], &vault, "pw");

    let timestamps = trash_timestamps(&vault, "pw");
    run_ok(&["restore", "KEY", "--at", &timestamps[0]], &vault, "pw");

    assert_eq!(trash_timestamps(&vault, "pw"), vec![timestamps[1].clone()]);
}

#[test]
fn restore_at_with_an_unknown_timestamp_fails_with_key_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    run_ok(&["set", "KEY", "VALUE"], &vault, "pw");
    run_ok(&["unset", "KEY"], &vault, "pw");

    let output = run_with_vault(
        &["restore", "KEY", "--at", "2020-01-01T00:00:00Z"],
        &vault,
        Some("pw"),
    );

    assert_eq!(code(&output), 3);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
    assert_eq!(trash_timestamps(&vault, "pw").len(), 1);
}

#[test]
fn restore_at_on_a_missing_key_fails_with_key_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");

    let output = run_with_vault(
        &["restore", "NOPE", "--at", "2020-01-01T00:00:00Z"],
        &vault,
        Some("pw"),
    );

    assert_eq!(code(&output), 3);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
}

#[test]
fn restore_at_onto_a_live_key_is_refused_and_keeps_the_trash() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    run_ok(&["set", "KEY", "OLD"], &vault, "pw");
    run_ok(&["unset", "KEY"], &vault, "pw");
    run_ok(&["set", "KEY", "NEW"], &vault, "pw");
    let timestamps = trash_timestamps(&vault, "pw");
    assert_eq!(timestamps.len(), 1, "{timestamps:?}");

    let output = run_with_vault(
        &["restore", "KEY", "--at", &timestamps[0]],
        &vault,
        Some("pw"),
    );

    assert_eq!(code(&output), 1);
    assert_eq!(stdout(&output), "");
    assert_eq!(run_ok(&["get", "KEY", "--reveal"], &vault, "pw"), "NEW\n");
    assert_eq!(trash_timestamps(&vault, "pw"), timestamps);
}

#[test]
fn restore_at_with_a_malformed_timestamp_is_a_usage_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");

    let output = run_with_vault(&["restore", "KEY", "--at", "yesterday"], &vault, Some("pw"));

    assert_eq!(code(&output), 2);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
}

#[test]
fn restore_at_never_leaks_the_value_in_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    run_ok(&["set", "KEY", "SUPERSECRET"], &vault, "pw");
    run_ok(&["unset", "KEY"], &vault, "pw");

    let output = run_with_vault(
        &["restore", "KEY", "--at", "2020-01-01T00:00:00Z"],
        &vault,
        Some("pw"),
    );

    assert!(!stdout(&output).contains("SUPERSECRET"));
    assert!(!stderr(&output).contains("SUPERSECRET"));
}

#[test]
fn every_trashed_generation_is_reachable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    for value in ["A", "B", "C"] {
        run_ok(&["set", "KEY", value], &vault, "pw");
        run_ok(&["unset", "KEY"], &vault, "pw");
        tick();
    }

    let timestamps = trash_timestamps(&vault, "pw");
    assert_eq!(timestamps.len(), 3, "{timestamps:?}");

    for (index, value) in ["A", "B", "C"].iter().enumerate() {
        run_ok(
            &["restore", "KEY", "--at", &timestamps[index]],
            &vault,
            "pw",
        );
        assert_eq!(
            run_ok(&["get", "KEY", "--reveal"], &vault, "pw"),
            format!("{value}\n")
        );
        run_ok(&["unset", "KEY"], &vault, "pw");
        tick();
    }
}

#[test]
fn unsetting_a_key_twice_keeps_the_earlier_value_in_the_trash() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    for value in ["OLD", "NEW"] {
        assert_eq!(
            code(&run_with_vault(&["set", "KEY", value], &vault, Some("pw"))),
            0
        );
        assert_eq!(
            code(&run_with_vault(&["unset", "KEY"], &vault, Some("pw"))),
            0
        );
    }

    let trash = stdout(&run_with_vault(&["list", "--trash"], &vault, Some("pw")));
    assert_eq!(trash.lines().count(), 2, "{trash}");
    assert!(
        trash.lines().all(|line| line.starts_with("KEY\t")),
        "{trash}"
    );

    assert_eq!(
        code(&run_with_vault(&["restore", "KEY"], &vault, Some("pw"))),
        0
    );
    assert_eq!(
        stdout(&run_with_vault(
            &["get", "KEY", "--reveal"],
            &vault,
            Some("pw")
        )),
        "NEW\n"
    );

    let remaining = stdout(&run_with_vault(&["list", "--trash"], &vault, Some("pw")));
    assert_eq!(remaining.lines().count(), 1, "{remaining}");
}

#[test]
fn set_on_a_new_key_leaves_the_trash_empty() {
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

    assert_eq!(
        stdout(&run_with_vault(&["list", "--trash"], &vault, Some("pw"))),
        ""
    );
}

#[test]
fn set_over_a_live_key_trashes_the_previous_value() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    for value in ["OLD", "NEW"] {
        assert_eq!(
            code(&run_with_vault(&["set", "KEY", value], &vault, Some("pw"))),
            0
        );
    }

    let trash = stdout(&run_with_vault(&["list", "--trash"], &vault, Some("pw")));
    assert_eq!(trash.lines().count(), 1, "{trash}");
    assert!(trash.starts_with("KEY\t"), "{trash}");
    assert_eq!(
        stdout(&run_with_vault(
            &["get", "KEY", "--reveal"],
            &vault,
            Some("pw")
        )),
        "NEW\n"
    );

    assert_eq!(
        code(&run_with_vault(&["unset", "KEY"], &vault, Some("pw"))),
        0
    );
    let trash = stdout(&run_with_vault(&["list", "--trash"], &vault, Some("pw")));
    assert_eq!(trash.lines().count(), 2, "{trash}");

    assert_eq!(
        code(&run_with_vault(&["restore", "KEY"], &vault, Some("pw"))),
        0
    );
    assert_eq!(
        stdout(&run_with_vault(
            &["get", "KEY", "--reveal"],
            &vault,
            Some("pw")
        )),
        "NEW\n"
    );
    let trash = stdout(&run_with_vault(&["list", "--trash"], &vault, Some("pw")));
    assert_eq!(trash.lines().count(), 1, "{trash}");
}

#[test]
fn set_never_leaks_the_overwritten_value_into_list_trash() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    for value in ["SUPERSECRET", "NEW"] {
        assert_eq!(
            code(&run_with_vault(&["set", "KEY", value], &vault, Some("pw"))),
            0
        );
    }

    for args in [
        vec!["list", "--trash"],
        vec!["--format", "json", "list", "--trash"],
    ] {
        let output = run_with_vault(&args, &vault, Some("pw"));
        assert_eq!(code(&output), 0);
        assert!(!stdout(&output).contains("SUPERSECRET"), "{args:?}");
    }
}

#[test]
fn setting_a_key_to_the_value_it_already_has_does_not_stack_the_trash() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");
    for _ in 0..3 {
        assert_eq!(
            code(&run_with_vault(
                &["set", "KEY", "VALUE"],
                &vault,
                Some("pw")
            )),
            0
        );
    }

    assert_eq!(
        stdout(&run_with_vault(&["list", "--trash"], &vault, Some("pw"))),
        ""
    );
}

#[test]
fn unset_missing_key_fails_with_key_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");
    init_vault(&vault, "pw");

    let output = run_with_vault(&["unset", "NOPE"], &vault, Some("pw"));

    assert_eq!(code(&output), 3);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
}

#[test]
fn unset_on_missing_vault_fails_with_vault_unavailable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = dir.path().join("vault.age");

    let output = run_with_vault(&["unset", "KEY"], &vault, Some("pw"));

    assert_eq!(code(&output), 4);
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
