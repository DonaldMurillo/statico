//! Integration tests: `statico guard` — file integrity protection.

mod common;
use common::*;
use std::process::Command;

// ─── Guard help surface ─────────────────────────────────────────────────

#[test]
fn guard_help_lists_all_subcommands() {
    let output =
        Command::new(statico_bin()).args(["guard", "--help"]).output().expect("failed to run statico guard --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "guard --help should exit 0");
    assert!(stdout.contains("add"), "should list 'add' subcommand");
    assert!(stdout.contains("remove"), "should list 'remove' subcommand");
    assert!(stdout.contains("list"), "should list 'list' subcommand");
    assert!(stdout.contains("check"), "should list 'check' subcommand");
    assert!(stdout.contains("update"), "should list 'update' subcommand");
}

#[test]
fn guard_add_help_works() {
    let output = Command::new(statico_bin())
        .args(["guard", "add", "--help"])
        .output()
        .expect("failed to run statico guard add --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--description"), "should show --description flag");
    assert!(stdout.contains("--path"), "should show --path flag");
}

#[test]
fn guard_check_help_shows_exit_code() {
    let output = Command::new(statico_bin())
        .args(["guard", "check", "--help"])
        .output()
        .expect("failed to run statico guard check --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--exit-code"), "should show --exit-code flag");
}

// ─── Guard add ──────────────────────────────────────────────────────────

#[test]
fn guard_add_creates_manifest_with_correct_structure() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();

    // Create test files.
    std::fs::write(root.join("config.rs"), b"fn config() {}").unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), b"fn main() {}").unwrap();

    let output = Command::new(statico_bin())
        .args(["guard", "add", "config.rs", "src/main.rs", "--path"])
        .arg(root)
        .output()
        .expect("failed to run statico guard add");

    assert!(output.status.success(), "guard add should exit 0, stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Added 2 file(s)"), "should report 2 files added: {stdout}");

    // Verify manifest file was created.
    let manifest_path = root.join(".statico/guard.json");
    assert!(manifest_path.exists(), "manifest file should be created");

    let content = std::fs::read_to_string(&manifest_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).expect("manifest must be valid JSON");
    assert_eq!(json["version"], 1);
    assert!(json["files"]["config.rs"].is_object());
    assert!(json["files"]["src/main.rs"].is_object());
    assert!(json["files"]["config.rs"]["hash"].is_string());
}

#[test]
fn guard_add_with_description() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    std::fs::write(root.join("critical.rs"), b"fn critical() {}").unwrap();

    let output = Command::new(statico_bin())
        .args(["guard", "add", "critical.rs", "--description", "Approval required", "--path"])
        .arg(root)
        .output()
        .expect("failed to run statico guard add");

    assert!(output.status.success());

    let manifest = std::fs::read_to_string(root.join(".statico/guard.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(json["files"]["critical.rs"]["description"], "Approval required");
}

#[test]
fn guard_add_nonexistent_file_fails() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();

    let output = Command::new(statico_bin())
        .args(["guard", "add", "no_such_file.rs", "--path"])
        .arg(root)
        .output()
        .expect("failed to run statico guard add");

    assert!(!output.status.success(), "should fail for nonexistent file");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "should mention file not found: {stderr}");
}

#[test]
fn guard_add_is_idempotent() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    std::fs::write(root.join("a.txt"), b"content").unwrap();

    // Add twice.
    for _ in 0..2 {
        let output = Command::new(statico_bin())
            .args(["guard", "add", "a.txt", "--path"])
            .arg(root)
            .output()
            .expect("guard add");
        assert!(output.status.success());
    }

    // Should have exactly 1 file in manifest.
    let manifest = std::fs::read_to_string(root.join(".statico/guard.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    let files = json["files"].as_object().unwrap();
    assert_eq!(files.len(), 1, "add should be idempotent, not duplicate entries");
}

// ─── Guard check ────────────────────────────────────────────────────────

#[test]
fn guard_check_passes_for_unchanged_files() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    std::fs::write(root.join("a.txt"), b"hello").unwrap();

    // Add file.
    let add =
        Command::new(statico_bin()).args(["guard", "add", "a.txt", "--path"]).arg(root).output().expect("guard add");
    assert!(add.status.success());

    // Check should pass.
    let check = Command::new(statico_bin())
        .args(["guard", "check", "--exit-code", "--path"])
        .arg(root)
        .output()
        .expect("guard check");
    assert!(
        check.status.success(),
        "check should pass for unchanged files, stderr: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("pass"), "should report passing check: {stdout}");
}

#[test]
fn guard_check_exit_code_fails_on_modified_file() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    let file = root.join("config.rs");
    std::fs::write(&file, b"fn config() {}").unwrap();

    // Add.
    let add = Command::new(statico_bin())
        .args(["guard", "add", "config.rs", "--path"])
        .arg(root)
        .output()
        .expect("guard add");
    assert!(add.status.success());

    // Tamper.
    std::fs::write(&file, b"fn config() { /* hacked */ }").unwrap();

    // Check with --exit-code should fail.
    let check = Command::new(statico_bin())
        .args(["guard", "check", "--exit-code", "--path"])
        .arg(root)
        .output()
        .expect("guard check");
    assert!(!check.status.success(), "check --exit-code should exit 1 on modified file");

    let combined = format!("{}{}", String::from_utf8_lossy(&check.stdout), String::from_utf8_lossy(&check.stderr));
    assert!(combined.contains("MODIFIED") || combined.contains("FAILED"), "should report modification: {combined}");
    assert!(combined.contains("config.rs"), "should name the modified file: {combined}");
}

#[test]
fn guard_check_detects_missing_file() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    let file = root.join("gone.txt");
    std::fs::write(&file, b"temporary").unwrap();

    // Add.
    let add =
        Command::new(statico_bin()).args(["guard", "add", "gone.txt", "--path"]).arg(root).output().expect("guard add");
    assert!(add.status.success());

    // Delete the file.
    std::fs::remove_file(&file).unwrap();

    // Check should report missing.
    let check = Command::new(statico_bin())
        .args(["guard", "check", "--exit-code", "--path"])
        .arg(root)
        .output()
        .expect("guard check");
    assert!(!check.status.success());

    let combined = format!("{}{}", String::from_utf8_lossy(&check.stdout), String::from_utf8_lossy(&check.stderr));
    assert!(combined.contains("MISSING"), "should report missing file: {combined}");
    assert!(combined.contains("gone.txt"), "should name the missing file: {combined}");
}

#[test]
fn guard_check_on_empty_manifest() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();

    let check = Command::new(statico_bin()).args(["guard", "check", "--path"]).arg(root).output().expect("guard check");

    assert!(check.status.success(), "empty manifest check should exit 0");
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("empty"), "should mention empty manifest: {stdout}");
}

// ─── Guard list ─────────────────────────────────────────────────────────

#[test]
fn guard_list_shows_guarded_files() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    std::fs::write(root.join("a.rs"), b"a").unwrap();
    std::fs::write(root.join("b.rs"), b"b").unwrap();

    // Add files.
    let add = Command::new(statico_bin())
        .args(["guard", "add", "a.rs", "b.rs", "--description", "test files", "--path"])
        .arg(root)
        .output()
        .expect("guard add");
    assert!(add.status.success());

    // List.
    let list = Command::new(statico_bin()).args(["guard", "list", "--path"]).arg(root).output().expect("guard list");
    assert!(list.status.success());

    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains("Guarded files (2)"), "should show count: {stdout}");
    assert!(stdout.contains("a.rs"), "should list a.rs: {stdout}");
    assert!(stdout.contains("b.rs"), "should list b.rs: {stdout}");
    assert!(stdout.contains("test files"), "should show description: {stdout}");
}

#[test]
fn guard_list_on_empty_manifest() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();

    let list = Command::new(statico_bin()).args(["guard", "list", "--path"]).arg(root).output().expect("guard list");
    assert!(list.status.success());

    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains("empty"), "should mention empty manifest: {stdout}");
}

// ─── Guard remove ───────────────────────────────────────────────────────

#[test]
fn guard_remove_deletes_from_manifest() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    std::fs::write(root.join("a.txt"), b"a").unwrap();
    std::fs::write(root.join("b.txt"), b"b").unwrap();

    // Add two files.
    let add = Command::new(statico_bin())
        .args(["guard", "add", "a.txt", "b.txt", "--path"])
        .arg(root)
        .output()
        .expect("guard add");
    assert!(add.status.success());

    // Remove one.
    let remove = Command::new(statico_bin())
        .args(["guard", "remove", "a.txt", "--path"])
        .arg(root)
        .output()
        .expect("guard remove");
    assert!(remove.status.success());

    let stdout = String::from_utf8_lossy(&remove.stdout);
    assert!(stdout.contains("Removed 1 file(s)"), "should report 1 removed: {stdout}");

    // Verify manifest.
    let manifest = std::fs::read_to_string(root.join(".statico/guard.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert!(json["files"]["a.txt"].is_null(), "a.txt should be removed");
    assert!(json["files"]["b.txt"].is_object(), "b.txt should remain");
}

#[test]
fn guard_remove_nonexistent_file_noops() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    std::fs::write(root.join("a.txt"), b"a").unwrap();

    // Add one file.
    let add =
        Command::new(statico_bin()).args(["guard", "add", "a.txt", "--path"]).arg(root).output().expect("guard add");
    assert!(add.status.success());

    // Try to remove a different file.
    let remove = Command::new(statico_bin())
        .args(["guard", "remove", "ghost.txt", "--path"])
        .arg(root)
        .output()
        .expect("guard remove");
    assert!(remove.status.success(), "remove should not fail for missing entry");

    let stdout = String::from_utf8_lossy(&remove.stdout);
    assert!(stdout.contains("no matching files"), "should report no match: {stdout}");
}

// ─── Guard update ───────────────────────────────────────────────────────

#[test]
fn guard_update_rehashes_after_modification() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    let file = root.join("data.rs");
    std::fs::write(&file, b"v1").unwrap();

    // Add.
    let add =
        Command::new(statico_bin()).args(["guard", "add", "data.rs", "--path"]).arg(root).output().expect("guard add");
    assert!(add.status.success());

    // Modify.
    std::fs::write(&file, b"v2").unwrap();

    // Check should fail.
    let check1 = Command::new(statico_bin())
        .args(["guard", "check", "--exit-code", "--path"])
        .arg(root)
        .output()
        .expect("guard check");
    assert!(!check1.status.success(), "should fail before update");

    // Update.
    let update =
        Command::new(statico_bin()).args(["guard", "update", "--path"]).arg(root).output().expect("guard update");
    assert!(update.status.success());

    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(stdout.contains("Updated"), "should report update: {stdout}");

    // Check should now pass.
    let check2 = Command::new(statico_bin())
        .args(["guard", "check", "--exit-code", "--path"])
        .arg(root)
        .output()
        .expect("guard check");
    assert!(check2.status.success(), "should pass after update");
}

#[test]
fn guard_update_specific_file_only() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    std::fs::write(root.join("a.txt"), b"v1").unwrap();
    std::fs::write(root.join("b.txt"), b"v1").unwrap();

    // Add both.
    let add = Command::new(statico_bin())
        .args(["guard", "add", "a.txt", "b.txt", "--path"])
        .arg(root)
        .output()
        .expect("guard add");
    assert!(add.status.success());

    // Modify both.
    std::fs::write(root.join("a.txt"), b"v2").unwrap();
    std::fs::write(root.join("b.txt"), b"v2").unwrap();

    // Update only a.txt.
    let update = Command::new(statico_bin())
        .args(["guard", "update", "a.txt", "--path"])
        .arg(root)
        .output()
        .expect("guard update");
    assert!(update.status.success());

    // Check should fail because b.txt is still stale.
    let check = Command::new(statico_bin())
        .args(["guard", "check", "--exit-code", "--path"])
        .arg(root)
        .output()
        .expect("guard check");
    assert!(!check.status.success(), "b.txt should still be detected as modified");
}

// ─── Full CI workflow ───────────────────────────────────────────────────

#[test]
fn full_workflow_add_modify_detect_update_verify() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    let config = root.join("src/config.rs");
    let entry = root.join("src/main.rs");

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(&config, b"pub fn config() {}").unwrap();
    std::fs::write(&entry, b"fn main() {}").unwrap();

    // Step 1: Add files.
    let add = Command::new(statico_bin())
        .args(["guard", "add", "src/config.rs", "src/main.rs", "--description", "critical", "--path"])
        .arg(root)
        .output()
        .expect("guard add");
    assert!(add.status.success());

    // Step 2: Verify manifest is valid JSON committed to .statico/.
    let manifest_path = root.join(".statico/guard.json");
    assert!(manifest_path.exists());
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["files"]["src/config.rs"]["description"], "critical");

    // Step 3: Initial check passes.
    let check1 = Command::new(statico_bin())
        .args(["guard", "check", "--exit-code", "--path"])
        .arg(root)
        .output()
        .expect("guard check");
    assert!(check1.status.success());

    // Step 4: Modify config.rs.
    std::fs::write(&config, b"pub fn config() { /* changed */ }").unwrap();

    // Step 5: Check detects modification.
    let check2 = Command::new(statico_bin())
        .args(["guard", "check", "--exit-code", "--path"])
        .arg(root)
        .output()
        .expect("guard check");
    assert!(!check2.status.success());

    // Step 6: Update re-hashes.
    let update =
        Command::new(statico_bin()).args(["guard", "update", "--path"]).arg(root).output().expect("guard update");
    assert!(update.status.success());

    // Step 7: Check passes again.
    let check3 = Command::new(statico_bin())
        .args(["guard", "check", "--exit-code", "--path"])
        .arg(root)
        .output()
        .expect("guard check");
    assert!(check3.status.success());

    // Step 8: Remove config.rs from guard.
    let remove = Command::new(statico_bin())
        .args(["guard", "remove", "src/config.rs", "--path"])
        .arg(root)
        .output()
        .expect("guard remove");
    assert!(remove.status.success());

    // Step 9: List should show only main.rs.
    let list = Command::new(statico_bin()).args(["guard", "list", "--path"]).arg(root).output().expect("guard list");
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains("Guarded files (1)"), "only 1 file should remain: {stdout}");
    assert!(!stdout.contains("config.rs"), "config.rs should be gone from list");
    assert!(stdout.contains("main.rs"), "main.rs should still be listed");
}
