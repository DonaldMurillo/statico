//! Shared helpers for the integration test suites.
//!
//! Each `tests/integration_*.rs` is its own test binary; rustc evaluates this
//! module independently per binary and warns about any helper that binary
//! happens not to call. Suppress at the file level — we can't predict the
//! exact subset each split uses.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the compiled statico binary.
pub fn statico_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_statico"))
}

/// Path to a test fixture directory.
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name)
}

/// Run `statico analyze <path>` and return (success, stdout, stderr).
pub fn run_analyze(path: &Path) -> (bool, String, String) {
    let output = Command::new(statico_bin()).arg("analyze").arg(path).output().expect("failed to execute statico");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (output.status.success(), stdout, stderr)
}

/// Parse stdout as JSON, panicking with context if invalid.
pub fn parse_json(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout).unwrap_or_else(|e| {
        panic!("stdout is not valid JSON: {e}\n--- stdout ---\n{stdout}");
    })
}

/// Helper to create a mock bash plugin that speaks JSON-RPC.
pub fn make_mock_plugin_script(dir: &Path, name: &str) -> PathBuf {
    let script = dir.join(name);
    let code = r#"#!/bin/bash
while IFS= read -r line; do
    id=$(echo "$line" | grep -o '"id":[0-9]*' | head -1 | cut -d':' -f2)
    if echo "$line" | grep -q '"method":"init"'; then
        echo "{\"jsonrpc\":\"2.0\",\"id\":${id},\"result\":{\"name\":\"mock-plugin\",\"version\":\"0.1.0\",\"hooks\":{\"analyze_file\":\"add\"},\"languages\":[],\"rules\":[]}}"
    elif echo "$line" | grep -q '"method":"shutdown"'; then
        exit 0
    elif echo "$line" | grep -q '"method":"analyze_file"'; then
        echo "{\"jsonrpc\":\"2.0\",\"id\":${id},\"result\":{\"issues\":[],\"exports\":[],\"dependencies\":[]}}"
    fi
done
"#;
    std::fs::write(&script, code).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}
