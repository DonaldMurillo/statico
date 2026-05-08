//! Integration tests: `statico plugin` subcommands.

mod common;
use common::*;
use std::process::Command;

#[test]
fn test_plugin_list_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(statico_bin())
        .arg("plugin")
        .arg("list")
        .arg("--path")
        .arg(tmp.path())
        .output()
        .expect("failed to run statico plugin list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No plugins found"), "expected 'No plugins found' in: {stdout}");
}

#[test]
fn test_plugin_list_discovers_executable() {
    let tmp = tempfile::tempdir().unwrap();
    let plugins_dir = tmp.path().join(".statico/plugins");
    std::fs::create_dir_all(&plugins_dir).unwrap();
    make_mock_plugin_script(&plugins_dir, "my-plugin");

    let output = Command::new(statico_bin())
        .arg("plugin")
        .arg("list")
        .arg("--path")
        .arg(tmp.path())
        .output()
        .expect("failed to run statico plugin list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("my-plugin"), "expected 'my-plugin' in: {stdout}");
    assert!(stdout.contains("executable"), "expected 'executable' in: {stdout}");
}

#[test]
fn test_plugin_schema_json() {
    let output = Command::new(statico_bin())
        .arg("plugin")
        .arg("schema")
        .arg("--format")
        .arg("json")
        .output()
        .expect("failed to run statico plugin schema");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let schema: serde_json::Value = serde_json::from_str(stdout.trim()).expect("schema should be valid JSON");
    assert!(schema["methods"]["init"].is_object(), "schema should have init method");
    assert!(schema["methods"]["analyze_file"].is_object(), "schema should have analyze_file");
    assert!(schema["types"].is_object(), "schema should have types section");
}

#[test]
fn test_plugin_schema_text() {
    let output =
        Command::new(statico_bin()).arg("plugin").arg("schema").output().expect("failed to run statico plugin schema");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("HOOKS:"), "expected HOOKS section in: {stdout}");
    assert!(stdout.contains("analyze_file"), "expected analyze_file in: {stdout}");
}

#[test]
fn test_plugin_docs_output() {
    let output =
        Command::new(statico_bin()).arg("plugin").arg("docs").output().expect("failed to run statico plugin docs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Plugin Development Guide"), "expected guide title in: {stdout}");
    assert!(stdout.contains("Quick Start"), "expected Quick Start in: {stdout}");
    assert!(stdout.contains("Hook Modes"), "expected Hook Modes in: {stdout}");
}

#[test]
fn test_plugin_init_typescript() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(statico_bin())
        .args(["plugin", "init", "my-rule", "--lang", "typescript"])
        .arg("--path")
        .arg(tmp.path())
        .output()
        .expect("run plugin init");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Created TypeScript plugin"), "expected success in: {stdout}");

    let plugin_dir = tmp.path().join(".statico/plugins/my-rule");
    assert!(plugin_dir.join("index.ts").exists(), "index.ts should exist");
    assert!(plugin_dir.join("package.json").exists(), "package.json should exist");
    assert!(plugin_dir.join("fixtures/sample.ts").exists(), "fixture should exist");
}

#[test]
fn test_plugin_init_rust() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(statico_bin())
        .args(["plugin", "init", "my-rule", "--lang", "rust"])
        .arg("--path")
        .arg(tmp.path())
        .output()
        .expect("run plugin init");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Created Rust plugin"), "expected success in: {stdout}");

    let plugin_dir = tmp.path().join(".statico/plugins/my-rule");
    assert!(plugin_dir.join("src/main.rs").exists(), "main.rs should exist");
    assert!(plugin_dir.join("Cargo.toml").exists(), "Cargo.toml should exist");
}

#[test]
fn test_plugin_init_rejects_duplicate() {
    let tmp = tempfile::tempdir().unwrap();
    // First init succeeds.
    let out1 = Command::new(statico_bin())
        .args(["plugin", "init", "dup-rule", "--lang", "typescript"])
        .arg("--path")
        .arg(tmp.path())
        .output()
        .expect("run plugin init");
    assert!(out1.status.success());

    // Second init fails.
    let out2 = Command::new(statico_bin())
        .args(["plugin", "init", "dup-rule", "--lang", "typescript"])
        .arg("--path")
        .arg(tmp.path())
        .output()
        .expect("run plugin init");
    assert!(!out2.status.success(), "should fail on duplicate");
    let stderr = String::from_utf8_lossy(&out2.stderr);
    assert!(stderr.contains("already exists"), "expected error in: {stderr}");
}

#[test]
fn test_plugin_doctor() {
    let output = Command::new(statico_bin()).args(["plugin", "doctor"]).output().expect("run plugin doctor");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Plugin Doctor"), "expected Doctor header in: {stdout}");
}

#[test]
fn test_plugin_build_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(statico_bin())
        .args(["plugin", "build"])
        .arg("--path")
        .arg(tmp.path())
        .output()
        .expect("run plugin build");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No plugins found"), "expected no plugins message in: {stdout}");
}

#[test]
fn test_plugin_run_discovers_console_log() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/plugin-demo");

    // Check bun is available (skip test if not).
    let has_bun = std::process::Command::new("which")
        .arg("bun")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !has_bun {
        eprintln!("Skipping test_plugin_run (bun not installed)");
        return;
    }

    let output = Command::new(statico_bin())
        .args(["plugin", "run", "no-console-log", "--file", "src/index.ts"])
        .arg("--path")
        .arg(&fixture)
        .output()
        .expect("run plugin run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "stderr: {}", stderr);
    assert!(stdout.contains("Issues: 3"), "expected 3 issues in: {}", stdout);
    assert!(stdout.contains("console.log"), "expected console.log message in: {}", stdout);
}

#[test]
fn test_plugin_run_clean_file_no_issues() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/plugin-demo");

    let has_bun = std::process::Command::new("which")
        .arg("bun")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !has_bun {
        eprintln!("Skipping test_plugin_run_clean (bun not installed)");
        return;
    }

    let output = Command::new(statico_bin())
        .args(["plugin", "run", "no-console-log", "--file", "src/utils.ts"])
        .arg("--path")
        .arg(&fixture)
        .output()
        .expect("run plugin run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "stderr: {}", stderr);
    assert!(stdout.contains("Issues: 0"), "expected 0 issues in: {}", stdout);
}

#[test]
fn test_python_plugin_detects_bare_excepts() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/python-demo");

    let has_python = std::process::Command::new("which")
        .arg("python3")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !has_python {
        eprintln!("Skipping test_python_plugin (python3 not installed)");
        return;
    }

    let output = Command::new(statico_bin())
        .args(["plugin", "run", "no-bare-except", "--file", "src/main.py"])
        .arg("--path")
        .arg(&fixture)
        .output()
        .expect("run plugin run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "stderr: {}", stderr);
    assert!(stdout.contains("Issues: 2"), "expected 2 issues in: {}", stdout);
    assert!(stdout.contains("bare except") || stdout.contains("Bare"), "expected bare except message in: {}", stdout);
}

#[test]
fn test_python_plugin_clean_file_no_issues() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/python-demo");

    let has_python = std::process::Command::new("which")
        .arg("python3")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !has_python {
        eprintln!("Skipping test_python_plugin_clean (python3 not installed)");
        return;
    }

    let output = Command::new(statico_bin())
        .args(["plugin", "run", "no-bare-except", "--file", "src/utils.py"])
        .arg("--path")
        .arg(&fixture)
        .output()
        .expect("run plugin run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "stderr: {}", stderr);
    assert!(stdout.contains("Issues: 0"), "expected 0 issues in: {}", stdout);
}

// ---------------------------------------------------------------------------
// Plugin path-traversal end-to-end. Covered at the unit level in
// `src/plugin/discovery.rs::sec_plugin_path_traversal_rejected`; this version
// exercises the full CLI path: a `.statico.toml` containing `path = "../..."`
// must not load the plugin. A traversal that *did* load would expose anything
// the user has read access to outside the project root.
// ---------------------------------------------------------------------------

#[test]
fn sec_plugin_list_rejects_path_traversal_via_config() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();

    // A `.statico.toml` that points a plugin path one level above the
    // canonicalized root. `ensure_within_root()` should reject this.
    std::fs::write(
        root.join(".statico.toml"),
        r#"
[[plugin]]
name = "evil"
path = "../escape"
"#,
    )
    .expect("write toml");

    let output =
        Command::new(statico_bin()).args(["plugin", "list"]).arg("--path").arg(root).output().expect("run plugin list");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // The traversal plugin must not appear in the listed plugins.
    assert!(
        !stdout.contains("evil"),
        "traversal plugin must not be listed (security hole if it is). stdout: {stdout}\nstderr: {stderr}"
    );
    // statico should at least warn about the rejection on stderr.
    assert!(
        stderr.to_lowercase().contains("evil") || stderr.to_lowercase().contains("outside"),
        "should warn about the rejected plugin: stdout={stdout} stderr={stderr}"
    );
}

// ---------------------------------------------------------------------------
// Plugin override conflict end-to-end. The unit-level coverage in
// `src/plugin/discovery.rs::validate_overrides_conflict_detected` proves the
// validator works in isolation; this one wires up two real bash plugins
// that both declare `analyze_file` as `override` and asserts statico
// surfaces the conflict at startup rather than running with last-writer-wins.
// ---------------------------------------------------------------------------

/// Write a single-file executable plugin that declares `analyze_file: override`.
/// Mirrors `make_mock_plugin_script` but with override mode.
fn write_override_executable(plugins_dir: &std::path::Path, name: &str) {
    let entry = plugins_dir.join(name);
    let body = format!(
        r#"#!/bin/bash
while IFS= read -r line; do
    id=$(echo "$line" | grep -o '"id":[0-9]*' | head -1 | cut -d':' -f2)
    if echo "$line" | grep -q '"method":"init"'; then
        echo "{{\"jsonrpc\":\"2.0\",\"id\":${{id}},\"result\":{{\"name\":\"{name}\",\"version\":\"0.1.0\",\"hooks\":{{\"analyze_file\":\"override\"}},\"languages\":[],\"rules\":[]}}}}"
    elif echo "$line" | grep -q '"method":"shutdown"'; then
        exit 0
    elif echo "$line" | grep -q '"method":"analyze_file"'; then
        echo "{{\"jsonrpc\":\"2.0\",\"id\":${{id}},\"result\":{{\"issues\":[],\"exports\":[],\"dependencies\":[]}}}}"
    fi
done
"#
    );
    std::fs::write(&entry, body).expect("write plugin");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
}

/// `validate_overrides` (src/plugin/discovery.rs) is now wired into
/// `PluginPipeline::new` (src/plugin/pipeline.rs). Two plugins both
/// declaring `override` on the same hook used to race on stdin/stdout and
/// panic at `manager.rs:114` with `stdout already taken (concurrent
/// send_request?)`. The pipeline now disables the override-mode plugins
/// with a clear warning and continues with the rest — analyze still
/// completes successfully.
#[test]
fn plugin_override_conflict_surfaces_at_analyze() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();

    // Minimal source so analyze has something to do.
    std::fs::write(root.join("package.json"), r#"{"name":"override-conflict","version":"0.0.0"}"#).expect("pkg");
    std::fs::create_dir_all(root.join("src")).expect("mkdir src");
    std::fs::write(root.join("src/index.ts"), "export const a = 1;\n").expect("index");

    // Two plugins, both auto-discovered, both declaring `analyze_file: override`.
    let plugins_dir = root.join(".statico/plugins");
    std::fs::create_dir_all(&plugins_dir).expect("mkdir plugins");
    write_override_executable(&plugins_dir, "first");
    write_override_executable(&plugins_dir, "second");

    let output = Command::new(statico_bin())
        .args(["analyze", "--quiet", "--format", "json"])
        .arg(root)
        .output()
        .expect("run analyze");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // 1) Analyze still completes — graceful degradation is the contract.
    assert!(
        output.status.success(),
        "analyze must keep running on plugin override conflict (graceful degradation), stderr: {stderr}"
    );

    // 2) The conflict is surfaced on stderr by name. Silent last-writer-wins
    //    would be a real bug.
    assert!(
        stderr.contains("override conflict") || stderr.contains("conflict"),
        "stderr must mention the override conflict: {stderr}"
    );
    assert!(
        stderr.contains("first") && stderr.contains("second"),
        "stderr must name both conflicting plugins: {stderr}"
    );

    // 3) No stdin/stdout race panic — the race is exactly what wiring
    //    `validate_overrides` prevents.
    assert!(
        !stderr.contains("stdout already taken"),
        "the override-mode plugins must be disabled before they can race on stdout: {stderr}"
    );
    assert!(!stderr.contains("panicked at"), "no panic should escape: {stderr}");

    let _ = stdout;
}

// ═══════════════════════════════════════════════════════════════
// V-9 RED: canonicalize fallback must reject non-existent paths
// ═══════════════════════════════════════════════════════════════
