use std::process::{Command, Output, Stdio};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rsnug"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("failed to run rsnug")
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
fn unimplemented_command_keeps_stdout_empty() {
    let output = run(&["list"]);
    assert_eq!(code(&output), 1);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
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
    let output = run(&["set", "KEY", "--stdin"]);
    assert_ne!(code(&output), 2);
}

#[test]
fn unknown_command_is_a_usage_error() {
    assert_eq!(code(&run(&["bogus"])), 2);
}
