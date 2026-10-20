//! Command-line option errors, as getopt's warnx and usage print them.

#![cfg(unix)]

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tiri"))
        .args(args)
        .env_remove("TMUX")
        .output()
        .expect("run tiri")
}

fn assert_usage_error(args: &[&str], message: &str) {
    let output = run(args);
    assert_eq!(output.status.code(), Some(1), "{args:?}");
    assert!(output.stdout.is_empty(), "{args:?}");
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 stderr");
    let expected = format!("{message}usage: tiri [-2CDhlNuVv] [-c shell-command]");
    assert!(stderr.starts_with(&expected), "{args:?}: {stderr:?}");
}

#[test]
fn version_prints_to_stdout() {
    let output = run(&["-V"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"tmux 3.8\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn unknown_options_are_reported_before_usage() {
    assert_usage_error(&["-x"], "tiri: unknown option -- x\n");
    assert_usage_error(&["-2x"], "tiri: unknown option -- x\n");
    assert_usage_error(&["--foo"], "tiri: unknown option -- -\n");
}

#[test]
fn a_missing_argument_is_reported_before_usage() {
    assert_usage_error(&["-c"], "tiri: option requires an argument -- c\n");
}

#[test]
fn accepted_flags_without_a_meaning_print_only_usage() {
    assert_usage_error(&["-d"], "");
    assert_usage_error(&["-U"], "");
}
