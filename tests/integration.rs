//! Acceptance oracle for core-cli-ast-engine goal.
//!
//! Tests the compiled Rust `statico` binary against all 5 gherkin scenarios.
//! Run via: cargo test

use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the compiled statico binary.
fn statico_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_statico"))
}

/// Path to a test fixture directory.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name)
}

/// Run `statico analyze <path>` and return (success, stdout, stderr).
fn run_analyze(path: &Path) -> (bool, String, String) {
    let output = Command::new(statico_bin()).arg("analyze").arg(path).output().expect("failed to execute statico");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (output.status.success(), stdout, stderr)
}

/// Parse stdout as JSON, panicking with context if invalid.
fn parse_json(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout).unwrap_or_else(|e| {
        panic!("stdout is not valid JSON: {e}\n--- stdout ---\n{stdout}");
    })
}

#[test]
fn happy_path_analyze_ts_project_produces_valid_json() {
    let (success, stdout, stderr) = run_analyze(&fixture("minimal-ts-project"));

    // Exit code 0.
    assert!(success, "expected exit 0, stderr: {stderr}");

    // Valid JSON with all required top-level keys.
    let json = parse_json(&stdout);
    assert!(json.get("structure").is_some(), "missing 'structure' key");
    assert!(json.get("dependencies").is_some(), "missing 'dependencies' key");
    assert!(json.get("quality").is_some(), "missing 'quality' key");

    // Structure: entry points and source files.
    let entry_points = json["structure"]["entry_points"].as_array();
    assert!(entry_points.is_some(), "entry_points should be an array");
    assert!(!entry_points.unwrap().is_empty(), "expected at least one entry point");

    let source_files = json["structure"]["source_files"].as_array();
    assert!(source_files.is_some(), "source_files should be an array");
    assert!(source_files.unwrap().len() >= 2, "expected at least 2 source files");

    // Dependency graph: import relationships.
    let imports = json["dependencies"]["imports"].as_array();
    assert!(imports.is_some(), "imports should be an array");
    let imports = imports.unwrap();
    assert!(!imports.is_empty(), "expected import relationships");

    // At least one import should have targets.
    let has_targets = imports.iter().any(|imp| imp["targets"].as_array().is_some_and(|t| !t.is_empty()));
    assert!(has_targets, "expected at least one import with targets");

    // Quality: complexity metrics per file.
    let quality_files = json["quality"]["files"].as_array();
    assert!(quality_files.is_some(), "quality.files should be an array");
    let quality_files = quality_files.unwrap();
    assert!(!quality_files.is_empty(), "expected quality metrics for files");

    for file in quality_files {
        let metrics = &file["metrics"];
        assert!(metrics.is_object(), "expected metrics for file {}", file["path"]);
        assert!(metrics["complexity"].is_number(), "expected complexity metric in {}", file["path"]);
        assert!(metrics["lines_of_code"].is_number(), "expected lines_of_code metric in {}", file["path"]);
        assert!(metrics["functions"].is_number(), "expected functions metric in {}", file["path"]);
    }
}

#[test]
fn error_path_nonexistent_directory_exits_nonzero() {
    let (success, stdout, stderr) = run_analyze(Path::new("/no/such/path/statico-test-nonexistent"));

    // Non-zero exit code.
    assert!(!success, "expected non-zero exit code");

    // stderr contains human-readable error.
    assert!(stderr.contains("path not found"), "expected 'path not found' in stderr, got: {stderr}");

    // stdout is empty (no JSON output).
    assert!(stdout.trim().is_empty(), "expected empty stdout, got: {stdout}");
}

#[test]
fn error_path_empty_project_reports_gracefully() {
    let (success, stdout, stderr) = run_analyze(&fixture("empty-project"));

    // Exit code 0 for empty projects.
    assert!(success, "expected exit 0 for empty project, stderr: {stderr}");

    let json = parse_json(&stdout);

    let source_files = json["structure"]["source_files"].as_array();
    assert!(source_files.is_some(), "source_files should be an array");
    assert_eq!(source_files.unwrap().len(), 0, "expected no source files");

    let imports = json["dependencies"]["imports"].as_array();
    assert!(imports.is_some(), "imports should be an array");
    assert_eq!(imports.unwrap().len(), 0, "expected no imports");

    let quality_files = json["quality"]["files"].as_array();
    assert!(quality_files.is_some(), "quality.files should be an array");
    assert_eq!(quality_files.unwrap().len(), 0, "expected no quality entries");
}

#[test]
fn error_path_malformed_ts_does_not_crash() {
    let (success, stdout, stderr) = run_analyze(&fixture("malformed-project"));

    // No panic.
    assert!(!stderr.contains("panic"), "binary panicked! stderr: {stderr}");

    // Exit code 0 (partial analysis success).
    assert!(success, "expected exit 0 for partial analysis, stderr: {stderr}");

    let json = parse_json(&stdout);

    let quality_files = json["quality"]["files"].as_array();
    assert!(quality_files.is_some(), "quality.files should be an array");
    let quality_files = quality_files.unwrap();

    // Parse error entries for the broken file.
    let broken = quality_files.iter().find(|f| f["path"].as_str().is_some_and(|p| p.ends_with("broken.ts")));
    assert!(broken.is_some(), "expected broken.ts in quality output");
    let broken = broken.unwrap();

    let parse_errors = broken["parse_errors"].as_array();
    assert!(parse_errors.is_some(), "parse_errors should be an array");
    assert!(!parse_errors.unwrap().is_empty(), "expected parse errors for broken.ts");

    // Other valid files are still analyzed.
    let valid = quality_files.iter().find(|f| f["path"].as_str().is_some_and(|p| p.ends_with("valid.ts")));
    assert!(valid.is_some(), "expected valid.ts in quality output");
    let valid = valid.unwrap();

    let valid_errors = valid["parse_errors"].as_array();
    assert!(valid_errors.is_some_and(|e| e.is_empty()), "valid.ts should have no parse errors");

    let valid_metrics = &valid["metrics"];
    assert!(valid_metrics.is_object(), "valid.ts should have metrics");
    assert!(valid_metrics["functions"].as_u64().is_some_and(|f| f >= 1), "valid.ts should have at least 1 function");
}

#[test]
fn contract_preserved_output_is_deterministic() {
    let (ok1, out1, err1) = run_analyze(&fixture("minimal-ts-project"));
    let (ok2, out2, err2) = run_analyze(&fixture("minimal-ts-project"));

    assert!(ok1, "first run failed: {err1}");
    assert!(ok2, "second run failed: {err2}");

    // Byte-identical output.
    assert_eq!(out1, out2, "output not deterministic across identical runs");
}

// ---------------------------------------------------------------------------
// Next.js entry point detection
// ---------------------------------------------------------------------------

#[test]
fn nextjs_detects_app_router_entry_points() {
    let (success, stdout, stderr) = run_analyze(&fixture("nextjs-project"));
    assert!(success, "expected exit 0, stderr: {stderr}");

    let json = parse_json(&stdout);
    let entry_points = json["structure"]["entry_points"].as_array().expect("entry_points should be array");

    let ep_paths: Vec<&str> = entry_points.iter().filter_map(|v| v.as_str()).collect();

    // next.config.ts
    assert!(
        ep_paths.iter().any(|p| p.contains("next.config")),
        "expected next.config in entry points, got: {:?}",
        ep_paths
    );

    // App Router pages
    assert!(ep_paths.iter().any(|p| p.contains("page.tsx")), "expected page.tsx entries, got: {:?}", ep_paths);
    assert!(ep_paths.iter().any(|p| p.contains("layout.tsx")), "expected layout.tsx, got: {:?}", ep_paths);
    assert!(ep_paths.iter().any(|p| p.contains("route.ts")), "expected route.ts, got: {:?}", ep_paths);

    // middleware.ts
    assert!(ep_paths.iter().any(|p| p.contains("middleware")), "expected middleware.ts, got: {:?}", ep_paths);
}

#[test]
fn nextjs_detects_dead_code() {
    let (success, stdout, stderr) = run_analyze(&fixture("nextjs-project"));
    assert!(success, "expected exit 0, stderr: {stderr}");

    let json = parse_json(&stdout);
    let dead = json["issues"]["dead_code"].as_array().expect("dead_code should be array");

    let dead_paths: Vec<&str> = dead.iter().filter_map(|d| d["path"].as_str()).collect();

    assert!(dead_paths.iter().any(|p| p.contains("orphan")), "expected orphan.ts in dead code, got: {:?}", dead_paths);
}

// ---------------------------------------------------------------------------
// Payload CMS entry point detection
// ---------------------------------------------------------------------------

#[test]
fn payload_detects_config_as_entry_point() {
    let (success, stdout, stderr) = run_analyze(&fixture("payload-project"));
    assert!(success, "expected exit 0, stderr: {stderr}");

    let json = parse_json(&stdout);
    let entry_points = json["structure"]["entry_points"].as_array().expect("entry_points should be array");

    let ep_paths: Vec<&str> = entry_points.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        ep_paths.iter().any(|p| p.contains("payload.config")),
        "expected payload.config.ts in entry points, got: {:?}",
        ep_paths
    );
}

#[test]
fn payload_detects_unused_exports() {
    let (success, stdout, stderr) = run_analyze(&fixture("payload-project"));
    assert!(success, "expected exit 0, stderr: {stderr}");

    let json = parse_json(&stdout);
    let unused = json["issues"]["unused_exports"].as_array().expect("unused_exports should be array");

    let unused_names: Vec<&str> = unused.iter().filter_map(|u| u["name"].as_str()).collect();

    assert!(
        unused_names.iter().any(|n| *n == "standaloneHelper"),
        "expected standaloneHelper in unused exports, got: {:?}",
        unused_names
    );
}

// ---------------------------------------------------------------------------
// Dead code project
// ---------------------------------------------------------------------------

#[test]
fn dead_code_detects_unreachable_files() {
    let (success, stdout, stderr) = run_analyze(&fixture("dead-code-project"));
    assert!(success, "expected exit 0, stderr: {stderr}");

    let json = parse_json(&stdout);
    let dead = json["issues"]["dead_code"].as_array().expect("dead_code should be array");

    let dead_paths: Vec<&str> = dead.iter().filter_map(|d| d["path"].as_str()).collect();

    assert!(dead_paths.iter().any(|p| p.contains("dead1")), "expected dead1.ts in dead code, got: {:?}", dead_paths);
    assert!(dead_paths.iter().any(|p| p.contains("dead2")), "expected dead2.ts in dead code, got: {:?}", dead_paths);
    assert!(
        !dead_paths.iter().any(|p| p.contains("alive") || p.contains("shared")),
        "alive.ts and shared.ts should NOT be dead, got: {:?}",
        dead_paths
    );
}

// ---------------------------------------------------------------------------
// Duplicate exports project
// ---------------------------------------------------------------------------

#[test]
fn duplicate_exports_detects_same_name_in_multiple_files() {
    let (success, stdout, stderr) = run_analyze(&fixture("duplicate-exports-project"));
    assert!(success, "expected exit 0, stderr: {stderr}");

    let json = parse_json(&stdout);
    let dupes = json["issues"]["duplicate_exports"].as_array().expect("duplicate_exports should be array");

    assert!(!dupes.is_empty(), "expected at least one duplicate export");

    let helper_dupe = dupes.iter().find(|d| d["name"].as_str() == Some("helper"));
    assert!(helper_dupe.is_some(), "expected 'helper' duplicate");

    let locs = helper_dupe.unwrap()["locations"].as_array().expect("locations should be array");
    assert_eq!(locs.len(), 2, "helper should be in 2 files");
}

// ---------------------------------------------------------------------------
// Real repos: tnc-blog
// ---------------------------------------------------------------------------

#[test]
fn real_repo_tnc_blog_analyzes_cleanly() {
    let tnc = Path::new("/Users/dom/programming/websites/tnc-blog");
    if !tnc.exists() {
        eprintln!("skipping: tnc-blog not found");
        return;
    }

    let (success, stdout, stderr) = run_analyze(tnc);
    assert!(success, "expected exit 0, stderr: {stderr}");

    let json = parse_json(&stdout);
    let files = json["structure"]["source_files"].as_array().expect("source_files should be array");
    assert!(!files.is_empty(), "expected source files");

    // Should have entry points (it's a Next.js project).
    let eps = json["structure"]["entry_points"].as_array().expect("entry_points should be array");
    assert!(!eps.is_empty(), "tnc-blog should have entry points");
}

// ---------------------------------------------------------------------------
// Real repos: metacollector
// ---------------------------------------------------------------------------

#[test]
fn real_repo_metacollector_analyzes_cleanly() {
    let meta = Path::new("/Users/dom/programming/products/metacollector");
    if !meta.exists() {
        eprintln!("skipping: metacollector not found");
        return;
    }

    let (success, stdout, stderr) = run_analyze(meta);
    assert!(success, "expected exit 0, stderr: {stderr}");

    let json = parse_json(&stdout);
    let files = json["structure"]["source_files"].as_array().expect("source_files should be array");
    assert!(!files.is_empty(), "expected source files");

    // Previously had 0 entry points — should now detect some.
    let eps = json["structure"]["entry_points"].as_array().expect("entry_points should be array");
    assert!(!eps.is_empty(), "metacollector should now detect entry points");
}

// ---------------------------------------------------------------------------
// CLI: update, init, doctor commands
// ---------------------------------------------------------------------------

#[test]
fn cli_version_flag_works() {
    let output = Command::new(statico_bin())
        .arg("--version")
        .output()
        .expect("failed to execute statico --version");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "--version should exit 0");
    assert!(stdout.contains("0.1.0"), "version should contain 0.1.0, got: {stdout}");
}

#[test]
fn cli_help_lists_new_commands() {
    let output = Command::new(statico_bin())
        .arg("--help")
        .output()
        .expect("failed to execute statico --help");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success());
    assert!(stdout.contains("update"), "help should mention 'update' command");
    assert!(stdout.contains("init"), "help should mention 'init' command");
    assert!(stdout.contains("doctor"), "help should mention 'doctor' command");
    assert!(stdout.contains("setup"), "help should mention 'setup' command");
}

#[test]
fn cli_update_check_handles_missing_releases() {
    let output = Command::new(statico_bin())
        .args(["update", "--check"])
        .output()
        .expect("failed to execute statico update --check");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(!output.status.success(), "expected non-zero exit for missing releases");
    assert!(
        stderr.contains("failed to check for updates") || stderr.contains("404"),
        "expected error message about update failure, got: {stderr}"
    );
    assert!(!stderr.contains("panic"), "should not panic");
}

#[test]
fn cli_doctor_runs_without_crash() {
    let output = Command::new(statico_bin())
        .arg("doctor")
        .output()
        .expect("failed to execute statico doctor");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "doctor should exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("Binary:"), "doctor should report binary location");
    assert!(stdout.contains("Version:"), "doctor should report version");
    assert!(stdout.contains("PATH:"), "doctor should check PATH");
    assert!(stdout.contains("Alias:"), "doctor should check alias");
    assert!(stdout.contains("Complete:"), "doctor should check completions");
    assert!(stdout.contains("Updates:"), "doctor should check update status");
}

#[test]
fn cli_init_writes_shell_rc_file() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let home = tmp.path().to_path_buf();

    // Create a fake .zshrc
    let zshrc = home.join(".zshrc");
    std::fs::write(&zshrc, "# existing content\n").expect("write .zshrc");

    let output = Command::new(statico_bin())
        .args(["init", "--shell", "zsh"])
        .env("HOME", &home)
        .env("SHELL", "/bin/zsh")
        .output()
        .expect("failed to execute statico init");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "init should exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("Shell integration configured"), "expected success message, got: {stdout}");

    // Verify .zshrc was modified.
    let rc_content = std::fs::read_to_string(&zshrc).expect("read .zshrc");
    assert!(rc_content.contains("# statico"), "rc file should contain statico marker, got: {rc_content}");
    assert!(rc_content.contains("export PATH="), "rc file should contain PATH export, got: {rc_content}");
    assert!(rc_content.contains("alias st='statico'"), "rc file should contain alias, got: {rc_content}");
    assert!(rc_content.contains("source"), "rc file should source completions, got: {rc_content}");
    // Should preserve existing content.
    assert!(rc_content.contains("# existing content"), "rc file should preserve existing content, got: {rc_content}");
}

#[test]
fn cli_init_is_idempotent() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let home = tmp.path().to_path_buf();
    let zshrc = home.join(".zshrc");
    std::fs::write(&zshrc, "").expect("write .zshrc");

    // Run init twice.
    for _ in 0..2 {
        let output = Command::new(statico_bin())
            .args(["init", "--shell", "zsh"])
            .env("HOME", &home)
            .env("SHELL", "/bin/zsh")
            .output()
            .expect("failed to execute statico init");
        assert!(output.status.success());
    }

    let rc_content = std::fs::read_to_string(&zshrc).expect("read .zshrc");
    let count = rc_content.matches("# statico").count();
    assert_eq!(count, 1, "init should be idempotent, but # statico appeared {} times:\n{}", count, rc_content);
    let alias_count = rc_content.matches("alias st='statico'").count();
    assert_eq!(alias_count, 1, "alias should appear once, appeared {} times", alias_count);
}

#[test]
fn cli_quiet_suppresses_update_notification() {
    let output = Command::new(statico_bin())
        .args(["--quiet", "analyze", "."])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to execute");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(!stderr.contains("panic"), "should not panic with --quiet");
}

// ---------------------------------------------------------------------------
// Mock server test for self-update
// ---------------------------------------------------------------------------

/// Spin up a tiny HTTP server, serve a mock release + tarball, and verify
/// the full download-extract-swap flow.
#[test]
fn cli_update_downloads_and_extracts_from_mock_server() {
    use std::io::Write as IoWrite;

    let tmp = tempfile::tempdir().expect("temp dir");
    let tmp_path = tmp.path().to_path_buf();

    // 1. Build the mock tarball with a fake "statico" binary inside.
    let fake_binary_content = "#!/bin/sh\necho 'statico 99.0.0'\n";
    let tarball_path = tmp_path.join("archive.tar.gz");
    {
        let mut tar_builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_path("statico").expect("set path");
        header.set_size(fake_binary_content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar_builder.append(&header, fake_binary_content.as_bytes()).expect("append");
        let tar_data = tar_builder.into_inner().expect("tar data");
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar_data).expect("gzip");
        let gz_data = gz.finish().expect("finish gzip");
        std::fs::write(&tarball_path, &gz_data).expect("write tarball");
    }
    let tarball_bytes = std::fs::read(&tarball_path).expect("read tarball");

    // 2. Build the mock release JSON.
    let platform = if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "macos-aarch64"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        "macos-x86_64"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        "linux-aarch64"
    } else {
        "linux-x86_64"
    };
    let release_json = format!(r#"{{"tag_name":"v99.0.0","assets":[{{"name":"statico-{}.tar.gz","browser_download_url":"http://MOCK/releases/download/v99.0.0/statico-{}.tar.gz"}}]}}"#, platform, platform);

    // 3. Start a tiny HTTP server in a background thread.
    let server = tiny_http::ServerBuilder::new().with_random_port().build().expect("start mock server");
    let port = server.server_addr().port();
    let base_url = format!("http://127.0.0.1:{}", port);

    let tarball_bytes_clone = tarball_bytes.clone();
    let release_json_clone = release_json.clone();
    let server_handle = std::thread::spawn(move || {
        // Handle exactly 2 requests from the update command:
        // 1. version check: GET /releases/latest
        // 2. download:      GET /releases/download/v99.0.0/...
        //
        // NOTE: We pass --quiet to suppress the startup check_and_notify()
        //       which would otherwise make a 3rd request.
        for _ in 0..2 {
            if let Ok(req) = server.recv() {
                let path = req.url();
                if path.contains("/releases/latest") && !path.contains("download") {
                    let resp = tiny_http::Response::from_string(&release_json_clone)
                        .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).expect("header"));
                    let _ = req.respond(resp);
                } else {
                    let resp = tiny_http::Response::from_data(tarball_bytes_clone.clone())
                        .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/gzip"[..]).expect("header"));
                    let _ = req.respond(resp);
                }
            }
        }
    });

    // 4. Create a fake current binary (copy the real one to temp location).
    let fake_exe_dir = tmp_path.join("bin");
    std::fs::create_dir_all(&fake_exe_dir).expect("create bin dir");
    let fake_exe = fake_exe_dir.join("statico");
    std::fs::copy(statico_bin(), &fake_exe).expect("copy real binary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_exe, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    // 5. Run the fake binary with `--quiet update`, pointing at our mock server.
    //    --quiet suppresses check_and_notify() which would otherwise hit the mock server.
    let output = std::process::Command::new(&fake_exe)
        .args(["--quiet", "update"])
        .env("STATICO_UPDATE_API_URL", &base_url)
        .env("STATICO_UPDATE_DL_URL", &base_url
        )
        .output()
        .expect("run fake binary");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Wait for server to finish.
    let _ = server_handle.join();

    // The update should succeed — binary swapped.
    if !output.status.success() {
        panic!("update command failed.\nstdout: {stdout}\nstderr: {stderr}");
    }

    assert!(stdout.contains("Updated statico"), "expected update success message, got: {stdout}");
    assert!(stdout.contains("99.0.0"), "expected new version in output, got: {stdout}");

    // 6. Verify the binary was replaced — it should now print our fake version.
    let verify = std::process::Command::new(&fake_exe)
        .arg("--version")
        .output()
        .expect("verify replaced binary");
    let verify_stdout = String::from_utf8_lossy(&verify.stdout).to_string();
    assert!(
        verify_stdout.contains("99.0.0"),
        "binary should have been replaced with mock, got: {verify_stdout}"
    );
}

// ---------------------------------------------------------------------------
// setup command tests
// ---------------------------------------------------------------------------

#[test]
fn cli_setup_generates_claude_files() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let tmp_path = tmp.path();

    // Init a git repo so .gitignore works.
    std::process::Command::new("git")
        .arg("init")
        .current_dir(tmp_path)
        .output()
        .expect("git init");
    std::fs::write(tmp_path.join(".gitignore"), "node_modules/\n").expect("gitignore");

    let output = std::process::Command::new(statico_bin())
        .args(["setup", "--target", "claude"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("run setup");

    assert!(output.status.success(), "setup should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("wrote"), "should report files written: {stdout}");

    assert!(tmp_path.join(".claude/CLAUDE.md").exists(), "CLAUDE.md should exist");
    assert!(tmp_path.join(".claude/skills/statico-analyze/SKILL.md").exists(), "analyze skill should exist");
    assert!(tmp_path.join(".claude/skills/statico-fix/SKILL.md").exists(), "fix skill should exist");

    // Verify .gitignore updated.
    let gitignore = std::fs::read_to_string(tmp_path.join(".gitignore")).expect("read gitignore");
    assert!(gitignore.contains(".claude/"), ".gitignore should contain .claude/: {gitignore}");
}

#[test]
fn cli_setup_generates_cursor_rules() {
    let tmp = tempfile::tempdir().expect("temp dir");

    let output = std::process::Command::new(statico_bin())
        .args(["setup", "--target", "cursor"])
        .arg("--path")
        .arg(tmp.path())
        .output()
        .expect("run setup");

    assert!(output.status.success(), "setup should succeed");
    assert!(tmp.path().join(".cursor/rules/statico.mdc").exists(), "cursor rules should exist");
}

#[test]
fn cli_setup_is_idempotent() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let tmp_path = tmp.path();
    std::process::Command::new("git").arg("init").current_dir(tmp_path).output().ok();

    // Run twice.
    let first = std::process::Command::new(statico_bin())
        .args(["setup"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("first setup");
    assert!(first.status.success());

    let second = std::process::Command::new(statico_bin())
        .args(["setup"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("second setup");
    assert!(second.status.success());
    let stdout = String::from_utf8_lossy(&second.stdout);
    assert!(stdout.contains("already exist"), "second run should say already exist: {stdout}");
}

#[test]
fn cli_setup_force_overwrites() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let tmp_path = tmp.path();
    std::process::Command::new("git").arg("init").current_dir(tmp_path).output().ok();

    // Run once, then modify a file, then --force.
    let first = std::process::Command::new(statico_bin())
        .args(["setup"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("first setup");
    assert!(first.status.success());

    // Overwrite CLAUDE.md with custom content.
    std::fs::write(tmp_path.join(".claude/CLAUDE.md"), "CUSTOM CONTENT").expect("overwrite");

    let force = std::process::Command::new(statico_bin())
        .args(["setup", "--force"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("force setup");
    assert!(force.status.success());

    // File should be overwritten.
    let content = std::fs::read_to_string(tmp_path.join(".claude/CLAUDE.md")).expect("read");
    assert!(content.contains("statico"), "should be regenerated: {content}");
    assert!(!content.contains("CUSTOM CONTENT"), "custom content should be gone");
}

#[test]
fn cli_setup_generates_pi_skills() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let tmp_path = tmp.path();
    std::process::Command::new("git").arg("init").current_dir(tmp_path).output().ok();
    std::fs::write(tmp_path.join(".gitignore"), "node_modules/\n").expect("gitignore");

    let output = std::process::Command::new(statico_bin())
        .args(["setup", "--target", "pi"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("run setup");

    assert!(output.status.success(), "setup should succeed");
    assert!(tmp_path.join(".pi/skills/statico-analyze/SKILL.md").exists(), "pi analyze skill should exist");
    assert!(tmp_path.join(".pi/skills/statico-fix/SKILL.md").exists(), "pi fix skill should exist");

    // Verify frontmatter has correct name field.
    let content = std::fs::read_to_string(tmp_path.join(".pi/skills/statico-analyze/SKILL.md")).expect("read");
    assert!(content.contains("name: statico-analyze"), "SKILL.md should have correct name frontmatter: {content}");

    // Verify .gitignore updated.
    let gitignore = std::fs::read_to_string(tmp_path.join(".gitignore")).expect("read gitignore");
    assert!(gitignore.contains(".pi/"), ".gitignore should contain .pi/: {gitignore}");
}

// ─── Plugin integration tests ────────────────────────────────────

/// Helper to create a mock bash plugin that speaks JSON-RPC.
fn make_mock_plugin_script(dir: &Path, name: &str) -> PathBuf {
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
    let output = Command::new(statico_bin())
        .arg("plugin")
        .arg("schema")
        .output()
        .expect("failed to run statico plugin schema");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("HOOKS:"), "expected HOOKS section in: {stdout}");
    assert!(stdout.contains("analyze_file"), "expected analyze_file in: {stdout}");
}

#[test]
fn test_plugin_docs_output() {
    let output = Command::new(statico_bin())
        .arg("plugin")
        .arg("docs")
        .output()
        .expect("failed to run statico plugin docs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Plugin Development Guide"), "expected guide title in: {stdout}");
    assert!(stdout.contains("Quick Start"), "expected Quick Start in: {stdout}");
    assert!(stdout.contains("Hook Modes"), "expected Hook Modes in: {stdout}");
}

