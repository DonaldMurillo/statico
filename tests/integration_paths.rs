//! Integration tests: path-rejection regressions (V9-class).

mod common;
use common::*;
use std::process::Command;

#[test]
fn test_v9_plugin_list_rejects_nonexistent_path() {
    let output = Command::new(statico_bin())
        .args(["plugin", "list", "--path", "/statico-v9-does-not-exist"])
        .output()
        .expect("failed to run statico plugin list");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "should fail for non-existent path, stderr: {stderr}");
    assert!(
        stderr.contains("cannot resolve") || stderr.contains("not found"),
        "should mention path resolution error, stderr: {stderr}"
    );
}

#[test]
fn test_v9_plugin_doctor_rejects_nonexistent_path() {
    let output = Command::new(statico_bin())
        .args(["plugin", "doctor", "--path", "/statico-v9-does-not-exist"])
        .output()
        .expect("failed to run statico plugin doctor");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "should fail for non-existent path, stderr: {stderr}");
    assert!(
        stderr.contains("cannot resolve") || stderr.contains("not found"),
        "should mention path resolution error, stderr: {stderr}"
    );
}

#[test]
fn test_v9_tui_rejects_nonexistent_path() {
    let output = Command::new(statico_bin())
        .args(["tui", "/statico-v9-does-not-exist"])
        .output()
        .expect("failed to run statico tui");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "should fail for non-existent path, stderr: {stderr}");
    assert!(
        stderr.contains("cannot resolve") || stderr.contains("not found"),
        "should mention path resolution error, stderr: {stderr}"
    );
}
