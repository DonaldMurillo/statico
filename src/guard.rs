//! File guard — protect critical files from unintended modification.
//!
//! A guard manifest (`.statico/guard.json`) stores SHA-256 hashes for
//! registered files. `statico guard check` verifies that every guarded file
//! still matches its recorded hash, making it easy to catch accidental or
//! malicious edits in CI pipelines or pre-commit hooks.
//!
//! # Manifest format
//!
//! ```json
//! {
//!   "version": 1,
//!   "files": {
//!     "src/config.rs": {
//!       "hash": "sha256:abcdef...",
//!       "description": "Core configuration — do not modify without approval"
//!     }
//!   }
//! }
//! ```
//!
//! Paths are relative to the project root and stored with forward slashes
//! for cross-platform consistency.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Current manifest schema version.
pub const MANIFEST_VERSION: u32 = 1;

/// Default manifest file name (relative to project root).
pub const MANIFEST_FILE: &str = ".statico/guard.json";

/// On-disk manifest representation.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GuardManifest {
    pub version: u32,
    /// Map of relative path → file entry. BTreeMap for deterministic ordering.
    pub files: BTreeMap<String, GuardEntry>,
}

/// A single guarded file entry.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GuardEntry {
    /// SHA-256 hash prefixed with `sha256:`.
    pub hash: String,
    /// Optional human-readable description of why this file is guarded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Result of checking a single guarded file.
#[derive(Debug, Clone)]
pub enum FileStatus {
    /// File hash matches the manifest.
    Ok { path: String },
    /// File hash differs from the manifest.
    Mismatch { path: String, expected: String, actual: String },
    /// File listed in manifest but missing from disk.
    Missing { path: String },
}

/// Result of a full `check` run.
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub statuses: Vec<FileStatus>,
}

impl CheckResult {
    /// Returns true if any file failed verification (mismatch or missing).
    pub fn has_failures(&self) -> bool {
        self.statuses.iter().any(|s| !matches!(s, FileStatus::Ok { .. }))
    }

    /// Number of files that passed.
    pub fn passed(&self) -> usize {
        self.statuses.iter().filter(|s| matches!(s, FileStatus::Ok { .. })).count()
    }

    /// Number of files that failed.
    pub fn failed(&self) -> usize {
        self.statuses.iter().filter(|s| !matches!(s, FileStatus::Ok { .. })).count()
    }
}

impl GuardManifest {
    /// Create an empty manifest.
    pub fn new() -> Self {
        Self { version: MANIFEST_VERSION, files: BTreeMap::new() }
    }

    /// Load manifest from the project root's `.statico/guard.json`.
    pub fn load(root: &Path) -> Result<Self, String> {
        let path = root.join(MANIFEST_FILE);
        Self::load_from(&path)
    }

    /// Load manifest from an explicit path.
    pub fn load_from(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path).map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
        let manifest: GuardManifest =
            serde_json::from_str(&content).map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;
        if manifest.version != MANIFEST_VERSION {
            return Err(format!(
                "guard manifest version mismatch — file has v{}, statico expects v{}",
                manifest.version, MANIFEST_VERSION
            ));
        }
        Ok(manifest)
    }

    /// Write manifest to `.statico/guard.json` under the project root.
    /// Creates the `.statico/` directory if it doesn't exist.
    pub fn write(&self, root: &Path) -> Result<(), String> {
        let path = root.join(MANIFEST_FILE);
        self.write_to(&path)
    }

    /// Write manifest to an explicit path. Creates parent directories.
    pub fn write_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
        }
        let json =
            serde_json::to_string_pretty(self).map_err(|e| format!("failed to serialize guard manifest: {}", e))?;
        // Atomic write: tmp then rename.
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("failed to write {}: {}", tmp.display(), e))?;
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&path.with_extension("tmp"));
            format!("failed to install guard manifest at {}: {}", path.display(), e)
        })?;
        Ok(())
    }

    /// Number of guarded files.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether the manifest is empty.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Add (or update) files to the manifest. Returns the number of files added.
    /// Paths are resolved relative to `root` and normalized.
    pub fn add(&mut self, root: &Path, file_paths: &[&str], description: Option<&str>) -> Result<usize, String> {
        let mut added = 0;
        for raw in file_paths {
            let rel = normalize_path(root, raw)?;
            let abs = root.join(&rel);
            if !abs.exists() {
                return Err(format!("file not found: {}", raw));
            }
            let hash = hash_file(&abs)?;
            self.files.insert(rel.clone(), GuardEntry { hash, description: description.map(|d| d.to_string()) });
            added += 1;
        }
        Ok(added)
    }

    /// Remove files from the manifest. Returns the number actually removed.
    pub fn remove(&mut self, root: &Path, file_paths: &[&str]) -> Result<usize, String> {
        let mut removed = 0;
        for raw in file_paths {
            let rel = normalize_path(root, raw)?;
            if self.files.remove(&rel).is_some() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Re-hash specified files (or all if empty) and update the manifest.
    /// Returns the number of files updated.
    pub fn update(&mut self, root: &Path, file_paths: &[&str]) -> Result<usize, String> {
        let mut updated = 0;
        let targets: Vec<String> = if file_paths.is_empty() {
            self.files.keys().cloned().collect()
        } else {
            file_paths.iter().map(|raw| normalize_path(root, raw)).collect::<Result<Vec<_>, _>>()?
        };
        for rel in targets {
            let abs = root.join(&rel);
            if !abs.exists() {
                return Err(format!("file not found: {}", rel));
            }
            let hash = hash_file(&abs)?;
            if let Some(entry) = self.files.get_mut(&rel) {
                entry.hash = hash;
                updated += 1;
            }
        }
        Ok(updated)
    }

    /// Check all guarded files against their recorded hashes.
    pub fn check(&self, root: &Path) -> CheckResult {
        let mut statuses = Vec::new();
        for (rel, entry) in &self.files {
            let abs = root.join(rel);
            if !abs.exists() {
                statuses.push(FileStatus::Missing { path: rel.clone() });
                continue;
            }
            match hash_file(&abs) {
                Ok(actual) => {
                    if actual == entry.hash {
                        statuses.push(FileStatus::Ok { path: rel.clone() });
                    } else {
                        statuses.push(FileStatus::Mismatch { path: rel.clone(), expected: entry.hash.clone(), actual });
                    }
                }
                Err(_) => {
                    statuses.push(FileStatus::Missing { path: rel.clone() });
                }
            }
        }
        CheckResult { statuses }
    }
}

/// Compute SHA-256 hash of a file, returned as `sha256:<hex>`.
fn hash_file(path: &Path) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let result = hasher.finalize();
    Ok(format!("sha256:{:x}", result))
}

/// Normalize a user-supplied path to a relative forward-slash path from root.
fn normalize_path(root: &Path, raw: &str) -> Result<String, String> {
    let p = PathBuf::from(raw);
    let rel = if p.is_absolute() {
        p.strip_prefix(root).map_err(|_| format!("path '{}' is outside the project root", raw))?.to_path_buf()
    } else {
        p
    };
    // Use forward slashes for consistency.
    let normalized = rel.to_string_lossy().replace('\\', "/");
    Ok(normalized)
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a temp project root, cleaned up on drop.
    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("statico_guard_test_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Create a file with content under root, creating parent dirs as needed.
    fn touch(root: &Path, rel: &str, content: &[u8]) -> PathBuf {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        path
    }

    // ─── Core: construction & defaults ──────────────────────────

    #[test]
    fn new_manifest_is_empty() {
        let m = GuardManifest::new();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
        assert_eq!(m.version, MANIFEST_VERSION);
    }

    #[test]
    fn check_empty_manifest_returns_empty_result() {
        let root = temp_root("empty_check");
        let m = GuardManifest::new();
        let result = m.check(&root);
        assert!(!result.has_failures());
        assert_eq!(result.passed(), 0);
        assert_eq!(result.failed(), 0);
        assert!(result.statuses.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    // ─── Add operations ────────────────────────────────────────

    #[test]
    fn add_single_file() {
        let root = temp_root("add1");
        touch(&root, "src/main.rs", b"fn main() {}");

        let mut m = GuardManifest::new();
        let added = m.add(&root, &["src/main.rs"], None).unwrap();
        assert_eq!(added, 1);
        assert_eq!(m.len(), 1);
        assert!(m.files.contains_key("src/main.rs"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn add_multiple_files_at_once() {
        let root = temp_root("add_multi");
        touch(&root, "a.txt", b"aaa");
        touch(&root, "b.txt", b"bbb");
        touch(&root, "c.txt", b"ccc");

        let mut m = GuardManifest::new();
        let added = m.add(&root, &["a.txt", "b.txt", "c.txt"], None).unwrap();
        assert_eq!(added, 3);
        assert_eq!(m.len(), 3);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn add_overwrites_existing_entry() {
        let root = temp_root("add_overwrite");
        let path = touch(&root, "config.rs", b"v1");

        let mut m = GuardManifest::new();
        m.add(&root, &["config.rs"], Some("initial".into())).unwrap();
        let first_hash = m.files["config.rs"].hash.clone();

        // Change file and re-add.
        std::fs::write(&path, b"v2").unwrap();
        m.add(&root, &["config.rs"], Some("updated".into())).unwrap();

        assert_eq!(m.len(), 1, "should still be 1 file, not duplicated");
        assert_ne!(m.files["config.rs"].hash, first_hash, "hash should change after re-add");
        assert_eq!(m.files["config.rs"].description.as_deref(), Some("updated"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn add_nonexistent_file_errors() {
        let root = temp_root("add_noexist");
        let mut m = GuardManifest::new();
        let result = m.add(&root, &["no_such_file.rs"], None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn add_with_description() {
        let root = temp_root("add_desc");
        touch(&root, "a.rs", b"fn a() {}");

        let mut m = GuardManifest::new();
        m.add(&root, &["a.rs"], Some("do not touch".into())).unwrap();
        assert_eq!(m.files["a.rs"].description.as_deref(), Some("do not touch"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn add_without_description_defaults_to_none() {
        let root = temp_root("add_nodesc");
        touch(&root, "b.rs", b"fn b() {}");

        let mut m = GuardManifest::new();
        m.add(&root, &["b.rs"], None).unwrap();
        assert!(m.files["b.rs"].description.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn add_file_in_subdirectory() {
        let root = temp_root("add_subdir");
        touch(&root, "src/deep/nested/mod.rs", b"pub mod deep;");

        let mut m = GuardManifest::new();
        let added = m.add(&root, &["src/deep/nested/mod.rs"], None).unwrap();
        assert_eq!(added, 1);
        assert!(m.files.contains_key("src/deep/nested/mod.rs"));
        let _ = std::fs::remove_dir_all(&root);
    }

    // ─── Remove operations ──────────────────────────────────────

    #[test]
    fn remove_existing_file() {
        let root = temp_root("rm_ok");
        touch(&root, "a.txt", b"content");

        let mut m = GuardManifest::new();
        m.add(&root, &["a.txt"], None).unwrap();
        assert_eq!(m.len(), 1);

        let removed = m.remove(&root, &["a.txt"]).unwrap();
        assert_eq!(removed, 1);
        assert!(m.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn remove_nonexistent_file_returns_zero() {
        let root = temp_root("rm_miss");
        touch(&root, "a.txt", b"x");

        let mut m = GuardManifest::new();
        m.add(&root, &["a.txt"], None).unwrap();

        let removed = m.remove(&root, &["ghost.txt"]).unwrap();
        assert_eq!(removed, 0);
        assert_eq!(m.len(), 1, "a.txt should still be guarded");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ─── Update operations ──────────────────────────────────────

    #[test]
    fn update_all_rehashes_every_file() {
        let root = temp_root("upd_all");
        let p1 = touch(&root, "a.txt", b"v1");
        let p2 = touch(&root, "b.txt", b"v1");

        let mut m = GuardManifest::new();
        m.add(&root, &["a.txt", "b.txt"], None).unwrap();

        // Modify both.
        std::fs::write(&p1, b"v2").unwrap();
        std::fs::write(&p2, b"v2").unwrap();

        let updated = m.update(&root, &[]).unwrap();
        assert_eq!(updated, 2);
        assert!(!m.check(&root).has_failures(), "hashes should now match v2");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn update_specific_file_only() {
        let root = temp_root("upd_specific");
        let p1 = touch(&root, "a.txt", b"v1");
        let p2 = touch(&root, "b.txt", b"v1");

        let mut m = GuardManifest::new();
        m.add(&root, &["a.txt", "b.txt"], None).unwrap();

        // Modify both.
        std::fs::write(&p1, b"v2").unwrap();
        std::fs::write(&p2, b"v2").unwrap();

        // Only update a.txt.
        let updated = m.update(&root, &["a.txt"]).unwrap();
        assert_eq!(updated, 1);

        let result = m.check(&root);
        assert_eq!(result.passed(), 1, "a.txt should pass");
        assert_eq!(result.failed(), 1, "b.txt should still fail");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn update_nonexistent_file_errors() {
        let root = temp_root("upd_noexist");
        touch(&root, "a.txt", b"x");

        let mut m = GuardManifest::new();
        m.add(&root, &["a.txt"], None).unwrap();

        // Add a bogus entry manually so update tries to read a missing file.
        m.files.insert("ghost.txt".into(), GuardEntry { hash: "sha256:deadbeef".into(), description: None });

        let result = m.update(&root, &["ghost.txt"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
        let _ = std::fs::remove_dir_all(&root);
    }

    // ─── Check operations ───────────────────────────────────────

    #[test]
    fn check_passes_for_unchanged_file() {
        let root = temp_root("chk_ok");
        touch(&root, "file.rs", b"fn foo() {}");

        let mut m = GuardManifest::new();
        m.add(&root, &["file.rs"], None).unwrap();

        let result = m.check(&root);
        assert!(!result.has_failures());
        assert_eq!(result.passed(), 1);
        assert_eq!(result.failed(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn check_detects_modification() {
        let root = temp_root("chk_mod");
        let path = touch(&root, "config.toml", b"original content");

        let mut m = GuardManifest::new();
        m.add(&root, &["config.toml"], Some("critical config".into())).unwrap();

        std::fs::write(&path, b"tampered content").unwrap();

        let result = m.check(&root);
        assert!(result.has_failures());
        assert_eq!(result.failed(), 1);
        assert_eq!(result.passed(), 0);

        match &result.statuses[0] {
            FileStatus::Mismatch { path, expected, actual } => {
                assert_eq!(path, "config.toml");
                assert_ne!(expected, actual);
                assert!(expected.starts_with("sha256:"));
                assert!(actual.starts_with("sha256:"));
            }
            other => panic!("expected Mismatch, got {:?}", other),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn check_detects_missing_file() {
        let root = temp_root("chk_miss");
        let path = touch(&root, "gone.txt", b"temporary");

        let mut m = GuardManifest::new();
        m.add(&root, &["gone.txt"], None).unwrap();

        std::fs::remove_file(&path).unwrap();

        let result = m.check(&root);
        assert!(result.has_failures());
        match &result.statuses[0] {
            FileStatus::Missing { path } => assert_eq!(path, "gone.txt"),
            other => panic!("expected Missing, got {:?}", other),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn check_mixed_results() {
        let root = temp_root("chk_mixed");
        touch(&root, "ok.txt", b"unchanged");
        let mod_file = touch(&root, "mod.txt", b"original");
        let del_file = touch(&root, "del.txt", b"will be deleted");

        let mut m = GuardManifest::new();
        m.add(&root, &["ok.txt", "mod.txt", "del.txt"], None).unwrap();

        // Modify one, delete another.
        std::fs::write(&mod_file, b"modified").unwrap();
        std::fs::remove_file(&del_file).unwrap();

        let result = m.check(&root);
        assert!(result.has_failures());
        assert_eq!(result.passed(), 1, "ok.txt should pass");
        assert_eq!(result.failed(), 2, "mod.txt + del.txt should fail");
        assert_eq!(result.statuses.len(), 3);
        let _ = std::fs::remove_dir_all(&root);
    }

    // ─── Hash determinism & format ──────────────────────────────

    #[test]
    fn hash_is_deterministic() {
        let root = temp_root("hash_det");
        touch(&root, "file.rs", b"const X: u32 = 42;");

        let mut m1 = GuardManifest::new();
        m1.add(&root, &["file.rs"], None).unwrap();
        let mut m2 = GuardManifest::new();
        m2.add(&root, &["file.rs"], None).unwrap();

        assert_eq!(m1.files["file.rs"].hash, m2.files["file.rs"].hash);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hash_differs_for_different_content() {
        let root = temp_root("hash_diff");
        touch(&root, "a.txt", b"content A");
        touch(&root, "b.txt", b"content B");

        let mut m = GuardManifest::new();
        m.add(&root, &["a.txt", "b.txt"], None).unwrap();

        assert_ne!(m.files["a.txt"].hash, m.files["b.txt"].hash, "different content must produce different hashes");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hash_has_sha256_prefix_and_64_hex_chars() {
        let root = temp_root("hash_prefix");
        touch(&root, "f.txt", b"data");

        let mut m = GuardManifest::new();
        m.add(&root, &["f.txt"], None).unwrap();
        let hash = &m.files["f.txt"].hash;
        assert!(hash.starts_with("sha256:"), "hash should be prefixed: {hash}");
        // SHA-256 = 64 hex chars after prefix.
        let hex = &hash[7..];
        assert_eq!(hex.len(), 64, "SHA-256 hex should be 64 chars");
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()), "should be hex: {hex}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_file_hashes_to_known_sha256() {
        let root = temp_root("hash_empty");
        touch(&root, "empty.txt", b"");

        let mut m = GuardManifest::new();
        m.add(&root, &["empty.txt"], None).unwrap();
        let hash = &m.files["empty.txt"].hash;
        // SHA-256 of empty bytes is a well-known constant.
        assert!(
            hash.contains("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
            "empty file should have well-known SHA-256: {hash}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn binary_file_hashes_and_checks_correctly() {
        let root = temp_root("hash_bin");
        let binary: Vec<u8> = (0..=255).collect();
        touch(&root, "blob.bin", &binary);

        let mut m = GuardManifest::new();
        let added = m.add(&root, &["blob.bin"], None).unwrap();
        assert_eq!(added, 1);

        let result = m.check(&root);
        assert!(!result.has_failures(), "binary file check should pass");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn large_file_single_byte_change_detected() {
        let root = temp_root("hash_large");
        // 1 MB of repeated data.
        let large = vec![0xAB_u8; 1_000_000];
        touch(&root, "big.bin", &large);

        let mut m = GuardManifest::new();
        m.add(&root, &["big.bin"], None).unwrap();

        // Modify a single byte.
        let mut modified = large.clone();
        modified[500_000] = 0xCD;
        std::fs::write(root.join("big.bin"), &modified).unwrap();

        let result = m.check(&root);
        assert!(result.has_failures(), "single byte change should be detected");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ─── Path normalization ─────────────────────────────────────

    #[test]
    fn normalize_relative_path() {
        let root = PathBuf::from("/project");
        assert_eq!(normalize_path(&root, "src/main.rs").unwrap(), "src/main.rs");
    }

    #[test]
    fn normalize_absolute_path_inside_root() {
        let root = temp_root("norm_abs");
        touch(&root, "src/a.rs", b"x");
        let abs = root.join("src/a.rs").to_string_lossy().to_string();
        let rel = normalize_path(&root, &abs).unwrap();
        assert_eq!(rel, "src/a.rs");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn normalize_absolute_path_outside_root_errors() {
        let root = PathBuf::from("/project");
        let result = normalize_path(&root, "/other/file.rs");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside"));
    }

    #[test]
    fn normalize_uses_forward_slashes() {
        let root = PathBuf::from("/project");
        let rel = normalize_path(&root, "deep/nested/file.rs").unwrap();
        assert_eq!(rel, "deep/nested/file.rs");
        assert!(!rel.contains('\\'), "should not contain backslashes");
    }

    // ─── Serialization & persistence ────────────────────────────

    #[test]
    fn serde_roundtrip_preserves_data() {
        let root = temp_root("serde");
        touch(&root, "a.rs", b"a");
        touch(&root, "b.rs", b"b");

        let mut m = GuardManifest::new();
        m.add(&root, &["a.rs"], Some("file a".into())).unwrap();
        m.add(&root, &["b.rs"], None).unwrap();

        let json = serde_json::to_string_pretty(&m).unwrap();
        let parsed: GuardManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, m.version);
        assert_eq!(parsed.len(), m.len());
        assert_eq!(parsed.files["a.rs"].hash, m.files["a.rs"].hash);
        assert_eq!(parsed.files["a.rs"].description, Some("file a".to_string()));
        assert!(parsed.files["b.rs"].description.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn manifest_json_is_deterministic_and_sorted() {
        let root = temp_root("json_det");
        touch(&root, "a.txt", b"a");
        touch(&root, "b.txt", b"b");

        let mut m = GuardManifest::new();
        m.add(&root, &["a.txt", "b.txt"], None).unwrap();

        let json1 = serde_json::to_string_pretty(&m).unwrap();
        let json2 = serde_json::to_string_pretty(&m).unwrap();
        assert_eq!(json1, json2, "serialization must be deterministic");

        // BTreeMap should produce alphabetical order.
        let lines: Vec<&str> = json1.lines().collect();
        let a_pos = lines.iter().position(|l| l.contains("a.txt")).unwrap();
        let b_pos = lines.iter().position(|l| l.contains("b.txt")).unwrap();
        assert!(a_pos < b_pos, "a.txt should appear before b.txt in sorted output");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn disk_roundtrip_via_load_and_write() {
        let root = temp_root("disk_rt");
        touch(&root, "x.rs", b"pub fn x() {}");

        let mut m = GuardManifest::new();
        m.add(&root, &["x.rs"], Some("entry point".into())).unwrap();
        m.write(&root).unwrap();

        let loaded = GuardManifest::load(&root).unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(loaded.files.contains_key("x.rs"));
        assert_eq!(loaded.files["x.rs"].description.as_deref(), Some("entry point"));
        assert!(!loaded.check(&root).has_failures());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_creates_statico_directory() {
        let root = temp_root("mkdir");
        touch(&root, "f.txt", b"x");

        let mut m = GuardManifest::new();
        m.add(&root, &["f.txt"], None).unwrap();

        assert!(!root.join(".statico").exists(), ".statico should not exist yet");
        m.write(&root).unwrap();
        assert!(root.join(".statico/guard.json").exists(), "manifest should be written");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_to_creates_arbitrary_parent_dirs() {
        let root = temp_root("mkdir_deep");
        let path = root.join("deep/nested/dir/manifest.json");

        let m = GuardManifest::new();
        m.write_to(&path).unwrap();

        assert!(path.exists(), "file should be written at custom path");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_nonexistent_path_returns_empty() {
        let root = temp_root("load_miss");
        let m = GuardManifest::load(&root).unwrap();
        assert!(m.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_rejects_future_version() {
        let root = temp_root("load_ver");
        let path = root.join(".statico/guard.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"version":99,"files":{}}"#).unwrap();

        let result = GuardManifest::load(&root);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("version"), "error should mention version: {err}");
        assert!(err.contains("99"), "error should mention the file's version: {err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_rejects_invalid_json() {
        let root = temp_root("load_badjson");
        let path = root.join(".statico/guard.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not json at all").unwrap();

        let result = GuardManifest::load(&root);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("parse"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn written_manifest_is_valid_json_schema() {
        let root = temp_root("valid_json");
        touch(&root, "a.rs", b"a");

        let mut m = GuardManifest::new();
        m.add(&root, &["a.rs"], Some("test".into())).unwrap();
        m.write(&root).unwrap();

        let content = std::fs::read_to_string(root.join(".statico/guard.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("manifest must be valid JSON");
        assert_eq!(parsed["version"], 1);
        assert!(parsed["files"].is_object());
        assert!(parsed["files"]["a.rs"].is_object());
        assert!(parsed["files"]["a.rs"]["hash"].is_string());
        assert_eq!(parsed["files"]["a.rs"]["description"], "test");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ─── End-to-end workflows ───────────────────────────────────

    #[test]
    fn workflow_add_check_modify_update_check() {
        let root = temp_root("workflow");
        let file = touch(&root, "config.rs", b"fn config() {}");

        // Step 1: Add
        let mut m = GuardManifest::new();
        m.add(&root, &["config.rs"], Some("core config".into())).unwrap();
        m.write(&root).unwrap();

        // Step 2: Check passes
        let m = GuardManifest::load(&root).unwrap();
        assert!(!m.check(&root).has_failures());

        // Step 3: Modify
        std::fs::write(&file, b"fn config() { /* changed */ }").unwrap();
        assert!(m.check(&root).has_failures(), "modification should be detected");

        // Step 4: Update
        let mut m = m;
        m.update(&root, &[]).unwrap();
        m.write(&root).unwrap();

        // Step 5: Check passes again
        let m = GuardManifest::load(&root).unwrap();
        assert!(!m.check(&root).has_failures(), "updated manifest should match");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn workflow_add_remove_verify_gone() {
        let root = temp_root("wf_rm");
        touch(&root, "a.txt", b"a");
        touch(&root, "b.txt", b"b");

        let mut m = GuardManifest::new();
        m.add(&root, &["a.txt", "b.txt"], None).unwrap();
        m.write(&root).unwrap();

        // Remove one file.
        let mut m = GuardManifest::load(&root).unwrap();
        m.remove(&root, &["a.txt"]).unwrap();
        m.write(&root).unwrap();

        // Verify only b.txt remains.
        let m = GuardManifest::load(&root).unwrap();
        assert_eq!(m.len(), 1);
        assert!(!m.files.contains_key("a.txt"));
        assert!(m.files.contains_key("b.txt"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn workflow_update_preserves_descriptions() {
        let root = temp_root("wf_desc");
        let file = touch(&root, "a.rs", b"v1");

        let mut m = GuardManifest::new();
        m.add(&root, &["a.rs"], Some("important file".into())).unwrap();

        // Modify and update.
        std::fs::write(&file, b"v2").unwrap();
        m.update(&root, &[]).unwrap();

        assert_eq!(
            m.files["a.rs"].description.as_deref(),
            Some("important file"),
            "update should preserve description"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn workflow_check_after_restore_passes() {
        let root = temp_root("wf_restore");
        let file = touch(&root, "data.json", b"{\"key\": \"value\"}");

        let mut m = GuardManifest::new();
        m.add(&root, &["data.json"], None).unwrap();

        // Tamper and verify detection.
        let original = std::fs::read(&file).unwrap();
        std::fs::write(&file, b"{\"key\": \"tampered\"}").unwrap();
        assert!(m.check(&root).has_failures());

        // Restore original content.
        std::fs::write(&file, &original).unwrap();
        assert!(!m.check(&root).has_failures(), "restoring original content should pass check");
        let _ = std::fs::remove_dir_all(&root);
    }
}
