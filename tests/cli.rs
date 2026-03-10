//! Integration tests for the TDB CLI binary.
//!
//! These tests exercise the binary itself via `std::process::Command`,
//! verifying that the CLI argument parsing and error handling work correctly.
//! They do NOT require root/sudo since they only test argument validation paths.

use std::process::Command;

fn tdb() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tdb"))
}

// ── No arguments → usage + exit 1 ──

#[test]
fn no_args_prints_usage_and_exits_nonzero() {
    let output = tdb().output().expect("failed to run tdb");
    assert!(
        !output.status.success(),
        "tdb with no args should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage"),
        "stderr should contain usage info: {}",
        stderr
    );
    assert!(
        stderr.contains("Commands"),
        "stderr should list commands: {}",
        stderr
    );
}

// ── Unknown command → error + exit 1 ──

#[test]
fn unknown_command_prints_error() {
    let output = tdb().arg("foobar").output().expect("failed to run tdb");
    assert!(
        !output.status.success(),
        "unknown command should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Unknown command"),
        "should say unknown command: {}",
        stderr
    );
    assert!(
        stderr.contains("foobar"),
        "should echo the bad command name: {}",
        stderr
    );
}

// ── `run` without enough args ──

#[test]
fn run_missing_args() {
    let output = tdb().arg("run").output().expect("failed to run tdb");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage") || stderr.contains("run"),
        "should show run usage: {}",
        stderr
    );
}

// ── `trace` without enough args ──

#[test]
fn trace_missing_args() {
    let output = tdb().arg("trace").output().expect("failed to run tdb");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage") || stderr.contains("trace"),
        "should show trace usage: {}",
        stderr
    );
}

// ── `view` without trace file ──

#[test]
fn view_missing_args() {
    let output = tdb().arg("view").output().expect("failed to run tdb");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage") || stderr.contains("view"),
        "should show view usage: {}",
        stderr
    );
}

// ── `tui` without trace file ──

#[test]
fn tui_missing_args() {
    let output = tdb().arg("tui").output().expect("failed to run tdb");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage") || stderr.contains("tui"),
        "should show tui usage: {}",
        stderr
    );
}

// ── `stats` without trace file ──

#[test]
fn stats_missing_args() {
    let output = tdb().arg("stats").output().expect("failed to run tdb");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage") || stderr.contains("stats"),
        "should show stats usage: {}",
        stderr
    );
}

// ── `view` with nonexistent file ──

#[test]
fn view_nonexistent_file_fails() {
    let output = tdb()
        .arg("view")
        .arg("/tmp/this_file_absolutely_does_not_exist_tdb_test.tdb")
        .output()
        .expect("failed to run tdb");
    assert!(
        !output.status.success(),
        "viewing nonexistent file should fail"
    );
}

// ── `stats` with nonexistent file ──

#[test]
fn stats_nonexistent_file_fails() {
    let output = tdb()
        .arg("stats")
        .arg("/tmp/this_file_absolutely_does_not_exist_tdb_test.tdb")
        .output()
        .expect("failed to run tdb");
    assert!(
        !output.status.success(),
        "stats on nonexistent file should fail"
    );
}

// ── Usage text mentions all commands ──

#[test]
fn usage_lists_all_commands() {
    let output = tdb().output().expect("failed to run tdb");
    let stderr = String::from_utf8_lossy(&output.stderr);
    for cmd in &["run", "trace", "view", "tui", "stats"] {
        assert!(
            stderr.contains(cmd),
            "Usage should mention '{}' command. Got:\n{}",
            cmd,
            stderr
        );
    }
}
