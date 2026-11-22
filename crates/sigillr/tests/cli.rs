//! Command-line behaviour of the `sigillr` binary in release 0.1.0.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sigillr"))
        .args(args)
        .output()
        .expect("sigillr binary runs")
}

/// R-1: `--version` prints the crate name and version, then exits 0.
#[test]
fn cli_version_prints_name_and_version() {
    let out = run(&["--version"]);
    assert_eq!(out.status.code(), Some(0));
    let expected = format!("sigillr {}\n", env!("CARGO_PKG_VERSION"));
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
}

/// R-2: `--help` prints usage and the planned commands, then exits 0.
#[test]
fn cli_help_prints_usage() {
    let out = run(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Usage: sigillr"),
        "usage line missing: {stdout}"
    );
    assert!(
        stdout.contains("index rebuild"),
        "planned commands missing: {stdout}"
    );
}

/// Unknown arguments are refused by clap with exit code 2.
#[test]
fn cli_unknown_argument_exits_2() {
    let out = run(&["--no-such-flag"]);
    assert_eq!(out.status.code(), Some(2));
}
