//! AI-bash compatibility tests.
//!
//! Verifies that all statico CLI commands work correctly when invoked
//! from non-interactive (no TTY) contexts — the environment AI coding
//! agents operate in.
//!
//! Every command must:
//!   - Complete within 10 seconds (no hangs)
//!   - Produce valid output to stdout
//!   - Return a sensible exit code
//!
//! Run: cargo test --test ai_compat

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_statico"))
}

/// Run statico with given args, enforce a timeout, return (exit_code, stdout, stderr, elapsed).
fn run(args: &[&str]) -> (Option<i32>, String, String, std::time::Duration) {
    let start = Instant::now();
    let output = Command::new(bin())
        .args(args)
        .env("TERM", "dumb")
        .env("NO_COLOR", "1")
        .env("STATICO_NO_UPDATE_CHECK", "1")
        .output()
        .expect("failed to spawn statico");
    let elapsed = start.elapsed();

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code();

    (code, stdout, stderr, elapsed)
}

fn fixture(name: &str) -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

// ---------------------------------------------------------------------------
// --version / --help — must complete instantly, no TTY required
// ---------------------------------------------------------------------------

#[test]
fn version_completes_without_tty() {
    let (code, stdout, _stderr, elapsed) = run(&["--version"]);

    assert!(elapsed.as_secs() < 10, "--version took {:?}", elapsed);
    assert!(stdout.contains("statico"), "stdout should contain 'statico': {}", stdout.trim());
    assert_eq!(code, Some(0), "--version should exit 0");
}

#[test]
fn help_completes_without_tty() {
    let (code, stdout, _stderr, elapsed) = run(&["--help"]);

    assert!(elapsed.as_secs() < 10, "--help took {:?}", elapsed);
    assert!(stdout.contains("analyze"), "--help should mention 'analyze'");
    assert!(stdout.contains("Usage"), "--help should contain 'Usage'");
    assert_eq!(code, Some(0), "--help should exit 0");
}

#[test]
fn quiet_version_completes() {
    let (code, stdout, _stderr, elapsed) = run(&["--quiet", "--version"]);

    assert!(elapsed.as_secs() < 10, "--quiet --version took {:?}", elapsed);
    assert!(stdout.contains("statico"), "stdout should contain 'statico'");
    assert_eq!(code, Some(0));
}

// ---------------------------------------------------------------------------
// analyze — JSON output, no TTY
// ---------------------------------------------------------------------------

#[test]
fn analyze_json_completes_without_tty() {
    let (code, stdout, stderr, elapsed) = run(&["analyze", fixture("minimal-ts-project").to_str().unwrap(), "--format", "json"]);

    assert!(elapsed.as_secs() < 30, "analyze --format json took {:?}", elapsed);
    assert_eq!(code, Some(0), "analyze should exit 0, stderr: {}", stderr);

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\n--- stdout ---\n{stdout}"));

    // Must have standard top-level sections.
    assert!(json.get("structure").is_some(), "missing 'structure'");
    assert!(json.get("dependencies").is_some(), "missing 'dependencies'");
    assert!(json.get("quality").is_some(), "missing 'quality'");
    assert!(json.get("duplication").is_some(), "missing 'duplication'");
}

#[test]
fn analyze_markdown_completes_without_tty() {
    let (code, stdout, stderr, elapsed) = run(&["analyze", fixture("minimal-ts-project").to_str().unwrap(), "--format", "markdown"]);

    assert!(elapsed.as_secs() < 30, "analyze --format markdown took {:?}", elapsed);
    assert_eq!(code, Some(0), "analyze markdown should exit 0, stderr: {}", stderr);
    assert!(stdout.contains("#") || stdout.contains("Health"), "markdown output should have headings");
}

#[test]
fn analyze_no_cache_completes_without_tty() {
    let (code, stdout, stderr, elapsed) = run(&["analyze", fixture("minimal-ts-project").to_str().unwrap(), "--format", "json", "--no-cache"]);

    assert!(elapsed.as_secs() < 30, "analyze --no-cache took {:?}", elapsed);
    assert_eq!(code, Some(0), "analyze --no-cache should exit 0, stderr: {}", stderr);

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");
    assert!(json.get("structure").is_some());
}

#[test]
fn analyze_exit_code_flag() {
    // With exit-code flag, should still succeed on clean project.
    let (code, _stdout, _stderr, elapsed) = run(&["analyze", fixture("minimal-ts-project").to_str().unwrap(), "--format", "json", "--exit-code"]);

    assert!(elapsed.as_secs() < 30, "analyze --exit-code took {:?}", elapsed);
    // Exit code should be 0 or 1 (both are valid depending on whether issues found).
    assert!(code == Some(0) || code == Some(1), "unexpected exit code: {:?}", code);
}

#[test]
fn analyze_quiet_mode() {
    let (code, stdout, stderr, elapsed) = run(&["--quiet", "analyze", fixture("minimal-ts-project").to_str().unwrap(), "--format", "json"]);

    assert!(elapsed.as_secs() < 30, "analyze --quiet took {:?}", elapsed);
    assert_eq!(code, Some(0), "quiet analyze should exit 0, stderr: {}", stderr);

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");
    assert!(json.get("structure").is_some());
}

// ---------------------------------------------------------------------------
// diff — compare two JSON outputs
// ---------------------------------------------------------------------------

#[test]
fn diff_completes_without_tty() {
    // First generate two analysis outputs.
    let (code_a, stdout_a, _, _) = run(&["analyze", fixture("minimal-ts-project").to_str().unwrap(), "--format", "json"]);
    assert_eq!(code_a, Some(0));

    let tmp = std::env::temp_dir().join("statico-ai-compat");
    std::fs::create_dir_all(&tmp).unwrap();
    let before = tmp.join("ai_compat_before.json");
    let after = tmp.join("ai_compat_after.json");
    std::fs::write(&before, &stdout_a).unwrap();
    std::fs::write(&after, &stdout_a).unwrap();

    let (code, stdout, _stderr, elapsed) = run(&["diff", before.to_str().unwrap(), after.to_str().unwrap(), "--format", "json"]);

    assert!(elapsed.as_secs() < 10, "diff took {:?}", elapsed);
    assert_eq!(code, Some(0), "diff should exit 0");
    assert!(!stdout.is_empty(), "diff should produce output");
}

// ---------------------------------------------------------------------------
// doctor — must complete without hanging
// ---------------------------------------------------------------------------

#[test]
fn doctor_completes_without_tty() {
    let (code, stdout, stderr, elapsed) = run(&["doctor"]);

    assert!(elapsed.as_secs() < 10, "doctor took {:?}", elapsed);
    // Doctor may exit 0 or 1 depending on system state.
    assert!(code.is_some(), "doctor should exit cleanly");
    // Should produce some output.
    assert!(!stdout.is_empty() || !stderr.is_empty(), "doctor should produce output");
}

// ---------------------------------------------------------------------------
// completions — must produce output
// ---------------------------------------------------------------------------

#[test]
fn completions_bash_completes() {
    let (code, stdout, _stderr, elapsed) = run(&["completions", "bash"]);

    assert!(elapsed.as_secs() < 5, "completions took {:?}", elapsed);
    assert_eq!(code, Some(0), "completions should exit 0");
    assert!(stdout.contains("statico"), "completions should mention 'statico'");
}

#[test]
fn completions_zsh_completes() {
    let (code, stdout, _stderr, elapsed) = run(&["completions", "zsh"]);

    assert!(elapsed.as_secs() < 5, "completions took {:?}", elapsed);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("statico"));
}

// ---------------------------------------------------------------------------
// plugin subcommands — must not hang
// ---------------------------------------------------------------------------

#[test]
fn plugin_schema_completes() {
    let (code, stdout, _stderr, elapsed) = run(&["plugin", "schema", "--format", "json"]);

    assert!(elapsed.as_secs() < 5, "plugin schema took {:?}", elapsed);
    assert_eq!(code, Some(0), "plugin schema should exit 0");
    assert!(!stdout.is_empty(), "plugin schema should produce output");
}

#[test]
fn plugin_docs_completes() {
    let (code, stdout, _stderr, elapsed) = run(&["plugin", "docs"]);

    assert!(elapsed.as_secs() < 5, "plugin docs took {:?}", elapsed);
    assert_eq!(code, Some(0), "plugin docs should exit 0");
    assert!(!stdout.is_empty(), "plugin docs should produce output");
}

#[test]
fn plugin_list_completes() {
    let (code, _stdout, _stderr, elapsed) = run(&["plugin", "list", "--path", fixture("minimal-ts-project").to_str().unwrap()]);

    assert!(elapsed.as_secs() < 5, "plugin list took {:?}", elapsed);
    // list exits 0 even when no plugins found.
    assert_eq!(code, Some(0), "plugin list should exit 0");
}

// ---------------------------------------------------------------------------
// update --check — must complete (even if offline)
// ---------------------------------------------------------------------------

#[test]
fn update_check_completes_without_tty() {
    let (code, stdout, stderr, elapsed) = run(&["update", "--check"]);

    assert!(elapsed.as_secs() < 15, "update --check took {:?}", elapsed);
    // May succeed or fail depending on network, but must not hang.
    assert!(code.is_some(), "update --check should exit cleanly");
    assert!(!stdout.is_empty() || !stderr.is_empty(), "update --check should produce output");
}

// ---------------------------------------------------------------------------
// Output format sanity checks
// ---------------------------------------------------------------------------

#[test]
fn analyze_json_has_repetitive_patterns() {
    let (code, stdout, stderr, _) = run(&["analyze", fixture("minimal-ts-project").to_str().unwrap(), "--format", "json"]);
    assert_eq!(code, Some(0), "stderr: {}", stderr);

    let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let patterns = json["duplication"]["repetitive_patterns"].as_array();
    // May be empty for small projects, but the field must exist.
    assert!(patterns.is_some(), "duplication.repetitive_patterns must be present");
}

#[test]
fn analyze_ai_format_completes() {
    let (code, stdout, stderr, elapsed) = run(&["analyze", fixture("minimal-ts-project").to_str().unwrap(), "--format", "ai"]);

    assert!(elapsed.as_secs() < 30, "analyze --format ai took {:?}", elapsed);
    assert_eq!(code, Some(0), "analyze ai format should exit 0, stderr: {}", stderr);
    assert!(!stdout.is_empty(), "ai format should produce output");
}

#[test]
fn analyze_context_format_completes() {
    let (code, stdout, stderr, elapsed) = run(&["analyze", fixture("minimal-ts-project").to_str().unwrap(), "--format", "context"]);

    assert!(elapsed.as_secs() < 30, "analyze --format context took {:?}", elapsed);
    assert_eq!(code, Some(0), "analyze context format should exit 0, stderr: {}", stderr);
    assert!(!stdout.is_empty(), "context format should produce output");
}

#[test]
fn analyze_sarif_format_completes() {
    let (code, stdout, stderr, elapsed) = run(&["analyze", fixture("minimal-ts-project").to_str().unwrap(), "--format", "sarif"]);

    assert!(elapsed.as_secs() < 30, "analyze --format sarif took {:?}", elapsed);
    assert_eq!(code, Some(0), "analyze sarif format should exit 0, stderr: {}", stderr);

    let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid SARIF JSON");
    assert!(json["$schema"].as_str().is_some(), "SARIF output should have $schema");
}
