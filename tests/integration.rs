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
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

/// Run `statico analyze <path>` and return (success, stdout, stderr).
fn run_analyze(path: &Path) -> (bool, String, String) {
    let output = Command::new(statico_bin())
        .arg("analyze")
        .arg(path)
        .output()
        .expect("failed to execute statico");

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
    assert!(
        json.get("dependencies").is_some(),
        "missing 'dependencies' key"
    );
    assert!(json.get("quality").is_some(), "missing 'quality' key");

    // Structure: entry points and source files.
    let entry_points = json["structure"]["entry_points"].as_array();
    assert!(entry_points.is_some(), "entry_points should be an array");
    assert!(
        !entry_points.unwrap().is_empty(),
        "expected at least one entry point"
    );

    let source_files = json["structure"]["source_files"].as_array();
    assert!(source_files.is_some(), "source_files should be an array");
    assert!(
        source_files.unwrap().len() >= 2,
        "expected at least 2 source files"
    );

    // Dependency graph: import relationships.
    let imports = json["dependencies"]["imports"].as_array();
    assert!(imports.is_some(), "imports should be an array");
    let imports = imports.unwrap();
    assert!(!imports.is_empty(), "expected import relationships");

    // At least one import should have targets.
    let has_targets = imports.iter().any(|imp| {
        imp["targets"]
            .as_array()
            .is_some_and(|t| !t.is_empty())
    });
    assert!(has_targets, "expected at least one import with targets");

    // Quality: complexity metrics per file.
    let quality_files = json["quality"]["files"].as_array();
    assert!(quality_files.is_some(), "quality.files should be an array");
    let quality_files = quality_files.unwrap();
    assert!(!quality_files.is_empty(), "expected quality metrics for files");

    for file in quality_files {
        let metrics = &file["metrics"];
        assert!(metrics.is_object(), "expected metrics for file {}", file["path"]);
        assert!(
            metrics["complexity"].is_number(),
            "expected complexity metric in {}",
            file["path"]
        );
        assert!(
            metrics["lines_of_code"].is_number(),
            "expected lines_of_code metric in {}",
            file["path"]
        );
        assert!(
            metrics["functions"].is_number(),
            "expected functions metric in {}",
            file["path"]
        );
    }
}

#[test]
fn error_path_nonexistent_directory_exits_nonzero() {
    let (success, stdout, stderr) = run_analyze(Path::new("/no/such/path/statico-test-nonexistent"));

    // Non-zero exit code.
    assert!(!success, "expected non-zero exit code");

    // stderr contains human-readable error.
    assert!(
        stderr.contains("path not found"),
        "expected 'path not found' in stderr, got: {stderr}"
    );

    // stdout is empty (no JSON output).
    assert!(
        stdout.trim().is_empty(),
        "expected empty stdout, got: {stdout}"
    );
}

#[test]
fn error_path_empty_project_reports_gracefully() {
    let (success, stdout, stderr) = run_analyze(&fixture("empty-project"));

    // Exit code 0 for empty projects.
    assert!(success, "expected exit 0 for empty project, stderr: {stderr}");

    let json = parse_json(&stdout);

    let source_files = json["structure"]["source_files"].as_array();
    assert!(source_files.is_some(), "source_files should be an array");
    assert_eq!(
        source_files.unwrap().len(),
        0,
        "expected no source files"
    );

    let imports = json["dependencies"]["imports"].as_array();
    assert!(imports.is_some(), "imports should be an array");
    assert_eq!(imports.unwrap().len(), 0, "expected no imports");

    let quality_files = json["quality"]["files"].as_array();
    assert!(quality_files.is_some(), "quality.files should be an array");
    assert_eq!(
        quality_files.unwrap().len(),
        0,
        "expected no quality entries"
    );
}

#[test]
fn error_path_malformed_ts_does_not_crash() {
    let (success, stdout, stderr) = run_analyze(&fixture("malformed-project"));

    // No panic.
    assert!(!stderr.contains("panic"), "binary panicked! stderr: {stderr}");

    // Exit code 0 (partial analysis success).
    assert!(
        success,
        "expected exit 0 for partial analysis, stderr: {stderr}"
    );

    let json = parse_json(&stdout);

    let quality_files = json["quality"]["files"].as_array();
    assert!(quality_files.is_some(), "quality.files should be an array");
    let quality_files = quality_files.unwrap();

    // Parse error entries for the broken file.
    let broken = quality_files
        .iter()
        .find(|f| f["path"].as_str().is_some_and(|p| p.ends_with("broken.ts")));
    assert!(broken.is_some(), "expected broken.ts in quality output");
    let broken = broken.unwrap();

    let parse_errors = broken["parse_errors"].as_array();
    assert!(parse_errors.is_some(), "parse_errors should be an array");
    assert!(
        !parse_errors.unwrap().is_empty(),
        "expected parse errors for broken.ts"
    );

    // Other valid files are still analyzed.
    let valid = quality_files
        .iter()
        .find(|f| f["path"].as_str().is_some_and(|p| p.ends_with("valid.ts")));
    assert!(valid.is_some(), "expected valid.ts in quality output");
    let valid = valid.unwrap();

    let valid_errors = valid["parse_errors"].as_array();
    assert!(
        valid_errors.is_some_and(|e| e.is_empty()),
        "valid.ts should have no parse errors"
    );

    let valid_metrics = &valid["metrics"];
    assert!(valid_metrics.is_object(), "valid.ts should have metrics");
    assert!(
        valid_metrics["functions"].as_u64().is_some_and(|f| f >= 1),
        "valid.ts should have at least 1 function"
    );
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
    let entry_points = json["structure"]["entry_points"]
        .as_array()
        .expect("entry_points should be array");

    let ep_paths: Vec<&str> = entry_points
        .iter()
        .filter_map(|v| v.as_str())
        .collect();

    // next.config.ts
    assert!(
        ep_paths.iter().any(|p| p.contains("next.config")),
        "expected next.config in entry points, got: {:?}",
        ep_paths
    );

    // App Router pages
    assert!(
        ep_paths.iter().any(|p| p.contains("page.tsx")),
        "expected page.tsx entries, got: {:?}",
        ep_paths
    );
    assert!(
        ep_paths.iter().any(|p| p.contains("layout.tsx")),
        "expected layout.tsx, got: {:?}",
        ep_paths
    );
    assert!(
        ep_paths.iter().any(|p| p.contains("route.ts")),
        "expected route.ts, got: {:?}",
        ep_paths
    );

    // middleware.ts
    assert!(
        ep_paths.iter().any(|p| p.contains("middleware")),
        "expected middleware.ts, got: {:?}",
        ep_paths
    );
}

#[test]
fn nextjs_detects_dead_code() {
    let (success, stdout, stderr) = run_analyze(&fixture("nextjs-project"));
    assert!(success, "expected exit 0, stderr: {stderr}");

    let json = parse_json(&stdout);
    let dead = json["issues"]["dead_code"]
        .as_array()
        .expect("dead_code should be array");

    let dead_paths: Vec<&str> = dead.iter().filter_map(|d| d["path"].as_str()).collect();

    assert!(
        dead_paths.iter().any(|p| p.contains("orphan")),
        "expected orphan.ts in dead code, got: {:?}",
        dead_paths
    );
}

// ---------------------------------------------------------------------------
// Payload CMS entry point detection
// ---------------------------------------------------------------------------

#[test]
fn payload_detects_config_as_entry_point() {
    let (success, stdout, stderr) = run_analyze(&fixture("payload-project"));
    assert!(success, "expected exit 0, stderr: {stderr}");

    let json = parse_json(&stdout);
    let entry_points = json["structure"]["entry_points"]
        .as_array()
        .expect("entry_points should be array");

    let ep_paths: Vec<&str> = entry_points
        .iter()
        .filter_map(|v| v.as_str())
        .collect();

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
    let unused = json["issues"]["unused_exports"]
        .as_array()
        .expect("unused_exports should be array");

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
    let dead = json["issues"]["dead_code"]
        .as_array()
        .expect("dead_code should be array");

    let dead_paths: Vec<&str> = dead.iter().filter_map(|d| d["path"].as_str()).collect();

    assert!(
        dead_paths.iter().any(|p| p.contains("dead1")),
        "expected dead1.ts in dead code, got: {:?}",
        dead_paths
    );
    assert!(
        dead_paths.iter().any(|p| p.contains("dead2")),
        "expected dead2.ts in dead code, got: {:?}",
        dead_paths
    );
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
    let dupes = json["issues"]["duplicate_exports"]
        .as_array()
        .expect("duplicate_exports should be array");

    assert!(!dupes.is_empty(), "expected at least one duplicate export");

    let helper_dupe = dupes.iter().find(|d| d["name"].as_str() == Some("helper"));
    assert!(helper_dupe.is_some(), "expected 'helper' duplicate");

    let locs = helper_dupe.unwrap()["locations"]
        .as_array()
        .expect("locations should be array");
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
    let files = json["structure"]["source_files"]
        .as_array()
        .expect("source_files should be array");
    assert!(!files.is_empty(), "expected source files");

    // Should have entry points (it's a Next.js project).
    let eps = json["structure"]["entry_points"]
        .as_array()
        .expect("entry_points should be array");
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
    let files = json["structure"]["source_files"]
        .as_array()
        .expect("source_files should be array");
    assert!(!files.is_empty(), "expected source files");

    // Previously had 0 entry points — should now detect some.
    let eps = json["structure"]["entry_points"]
        .as_array()
        .expect("entry_points should be array");
    assert!(!eps.is_empty(), "metacollector should now detect entry points");
}
