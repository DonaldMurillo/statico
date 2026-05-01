//! Incremental cache for parsed file results.
//!
//! Caches per-file parse data keyed by content hash so unchanged files
//! are skipped on re-runs. Cache lives in `{project_root}/.statico/cache/`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Per-file cached data.
///
/// Stores the full parse result (except raw source) so unchanged files
/// can skip re-parsing on subsequent runs.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct CachedFileData {
    // -- Dependency graph --
    pub dep_targets: Vec<String>,
    pub external_specs: Vec<String>,
    /// Per-target imported names: (resolved_file, [name1, name2, ...]).
    pub imported_names: Vec<(String, Vec<String>)>,

    // -- Exports --
    pub exports: Vec<String>,

    // -- Quality metrics --
    pub loc: usize,
    pub total_lines: usize,
    pub functions: usize,
    pub classes: usize,
    pub complexity: usize,
    pub max_nesting_depth: usize,
    pub parse_errors: Vec<crate::types::ParseError>,

    // -- Code blocks (for duplication detection) --
    pub blocks: Vec<crate::parse::blocks::CodeBlock>,
}

impl CachedFileData {
    /// Extract cacheable data from a completed `FileAnalysis`.
    pub fn from_analysis(fa: &crate::languages::FileAnalysis) -> Self {
        Self {
            dep_targets: fa.dep_targets.clone(),
            external_specs: fa.external_specs.clone(),
            imported_names: fa.imported_names.clone(),
            exports: fa.exports.clone(),
            loc: fa.loc,
            total_lines: fa.total_lines,
            functions: fa.functions,
            classes: fa.classes,
            complexity: fa.complexity,
            max_nesting_depth: fa.max_nesting_depth,
            parse_errors: fa.parse_errors.clone(),
            blocks: fa.blocks.clone(),
        }
    }

    /// Reconstruct a `FileAnalysis` from cached data + the raw source text.
    pub fn to_analysis(&self, rel_path: String, source: String) -> crate::languages::FileAnalysis {
        crate::languages::FileAnalysis {
            rel_path,
            dep_targets: self.dep_targets.clone(),
            external_specs: self.external_specs.clone(),
            imported_names: self.imported_names.clone(),
            exports: self.exports.clone(),
            loc: self.loc,
            total_lines: self.total_lines,
            functions: self.functions,
            classes: self.classes,
            complexity: self.complexity,
            max_nesting_depth: self.max_nesting_depth,
            parse_errors: self.parse_errors.clone(),
            blocks: self.blocks.clone(),
            source,
        }
    }
}

/// Incremental file cache manager.
pub struct IncrementalCache {
    cache_dir: PathBuf,
    entries: BTreeMap<String, CacheEntry>,
    dirty: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    hash: String,
    data: CachedFileData,
}

impl IncrementalCache {
    /// Create or load a cache for the given project root.
    pub fn new(project_root: &Path) -> Self {
        let cache_dir = project_root.join(".statico").join("cache");
        let mut cache = Self { cache_dir, entries: BTreeMap::new(), dirty: false };
        cache.load();
        cache
    }

    /// Look up a cached result by file path and content hash.
    pub fn get(&self, file_path: &str, content_hash: &str) -> Option<&CachedFileData> {
        self.entries.get(file_path).and_then(|e| if e.hash == content_hash { Some(&e.data) } else { None })
    }

    /// Store a parse result in the cache.
    pub fn set(&mut self, file_path: &str, content_hash: &str, data: CachedFileData) {
        self.entries.insert(file_path.to_string(), CacheEntry { hash: content_hash.to_string(), data });
        self.dirty = true;
    }

    /// Remove entries for files that no longer exist.
    pub fn prune_missing(&mut self, existing_files: &[&str]) {
        let existing: std::collections::HashSet<&str> = existing_files.iter().copied().collect();
        let before = self.entries.len();
        self.entries.retain(|k, _| existing.contains(k.as_str()));
        if self.entries.len() != before {
            self.dirty = true;
        }
    }

    /// Persist the cache to disk.
    pub fn save(&self) {
        if !self.dirty {
            return;
        }
        if let Err(e) = fs::create_dir_all(&self.cache_dir) {
            eprintln!("warning: failed to create cache dir: {}", e);
            return;
        }
        let index_path = self.cache_dir.join("index.json");
        let json = match serde_json::to_string(&self.entries) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("warning: failed to serialize cache: {}", e);
                return;
            }
        };
        if let Err(e) = {
            let tmp_path = index_path.with_extension("json.tmp");
            let write_result = fs::write(&tmp_path, &json);
            // Set restrictive permissions on the temp file (owner-only on Unix).
            #[cfg(unix)]
            if write_result.is_ok() {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600));
            }
            match write_result {
                Ok(()) => fs::rename(&tmp_path, &index_path),
                Err(e) => Err(e),
            }
        } {
            eprintln!("warning: failed to write cache: {}", e);
        }
    }

    fn load(&mut self) {
        let index_path = self.cache_dir.join("index.json");
        if !index_path.exists() {
            return;
        }
        let content = match fs::read_to_string(&index_path) {
            Ok(c) => c,
            Err(_) => return,
        };
        match serde_json::from_str(&content) {
            Ok(entries) => {
                self.entries = entries;
                self.dirty = false;
            }
            Err(_) => {
                // Corrupt cache — start fresh.
                self.entries.clear();
                self.dirty = false;
            }
        }
    }
}

impl Drop for IncrementalCache {
    fn drop(&mut self) {
        self.save();
    }
}

/// Compute a fast hash of file contents (SHA-256 for collision resistance).
pub fn content_hash(content: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// Ensure `.statico/` is in the project's .gitignore.
/// V-8: Refuses to modify .gitignore if it is a symlink (prevents corruption
/// of linked files).
/// V7-4: Checks for `.statico/` as a standalone gitignore pattern on its
/// own line (not just a substring match), preventing false positives from
/// comments like `# statico is great` or negation `!.statico`.
pub fn ensure_gitignore(project_root: &Path) {
    let gitignore_path = project_root.join(".gitignore");
    // V-8: Don't follow symlinks — could point to important system files
    if gitignore_path.is_symlink() {
        return;
    }
    let existing = fs::read_to_string(&gitignore_path).unwrap_or_default();
    // V7-4: Check for `.statico` as a gitignore pattern (on its own line),
    // not just as a substring. A comment like `# statico` or a negation like
    // `!.statico` should not prevent us from adding the real ignore entry.
    let has_statico_pattern = existing.lines().any(|line| {
        let trimmed = line.trim();
        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return false;
        }
        // Check if the pattern matches .statico (with or without trailing /)
        trimmed == ".statico" || trimmed == ".statico/" || trimmed == "/.statico" || trimmed == "/.statico/"
    });
    if !has_statico_pattern
        && let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&gitignore_path) {
            let _ = writeln!(f, "\n# statico cache\n.statico/");
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a minimal CachedFileData for tests.
    fn test_cache_data(exports: Vec<&str>) -> CachedFileData {
        CachedFileData {
            dep_targets: vec![],
            external_specs: vec![],
            imported_names: vec![],
            exports: exports.into_iter().map(|s| s.to_string()).collect(),
            loc: 1,
            total_lines: 1,
            functions: 0,
            classes: 0,
            complexity: 0,
            max_nesting_depth: 0,
            parse_errors: vec![],
            blocks: vec![],
        }
    }

    #[test]
    fn test_content_hash_deterministic() {
        let a = content_hash("hello world");
        let b = content_hash("hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn test_content_hash_differs() {
        let a = content_hash("hello");
        let b = content_hash("world");
        assert_ne!(a, b);
    }

    #[test]
    fn test_cache_set_get() {
        let dir = std::env::temp_dir().join("statico_test_cache_set");
        let _ = fs::remove_dir_all(&dir);
        let mut cache = IncrementalCache::new(&dir);
        let data = test_cache_data(vec!["foo"]);
        cache.set("src/a.ts", "abc123", data.clone());
        assert!(cache.get("src/a.ts", "abc123").is_some());
        assert!(cache.get("src/a.ts", "wrong").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cache_persist() {
        let dir = std::env::temp_dir().join("statico_test_cache_persist");
        let _ = fs::remove_dir_all(&dir);
        let data = test_cache_data(vec![]);
        {
            let mut cache = IncrementalCache::new(&dir);
            cache.set("src/x.ts", "hash1", data);
            cache.save();
        }
        {
            let cache = IncrementalCache::new(&dir);
            assert!(cache.get("src/x.ts", "hash1").is_some());
        }
        let _ = fs::remove_dir_all(&dir);
    }

    // ── Security tests ──────────────────────────────────────────────────

    #[test]
    fn sec_cache_atomic_write_no_partial_file() {
        // After save(), the cache file should exist and be valid JSON.
        // No .json.tmp file should be left behind.
        let dir = std::env::temp_dir().join("statico_sec_cache_atomic2");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let cache_dir = dir.join(".statico").join("cache");
        {
            let mut cache = IncrementalCache::new(&dir);
            cache.set("src/a.ts", "hash", test_cache_data(vec!["foo"]));
            cache.save();
        }
        // The main cache file should exist and be valid
        let index_path = cache_dir.join("index.json");
        assert!(index_path.exists(), "index.json should exist after save at {:?}", index_path);
        let content = fs::read_to_string(&index_path).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&content).is_ok(),
            "index.json should be valid JSON");
        // No temp file should be left
        assert!(!cache_dir.join("index.json.tmp").exists(),
            "index.json.tmp should not exist after atomic save");
        let _ = fs::remove_dir_all(&dir);
    }

    // ── V-8 RED: ensure_gitignore must not follow symlinks ──

    #[test]
    fn sec_gitignore_rejects_symlink() {
        let dir = std::env::temp_dir().join("statico_sec_gitignore_symlink");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // Create a regular file, and a symlink .gitignore pointing to it
        let target = dir.join("target_file.txt");
        fs::write(&target, "important contents\n").unwrap();
        let link = dir.join(".gitignore");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        // ensure_gitignore should NOT modify a symlinked .gitignore
        ensure_gitignore(&dir);
        let contents = fs::read_to_string(&target).unwrap();
        assert!(!contents.contains(".statico"),
            "ensure_gitignore should not modify a symlinked .gitignore; contents: {}", contents);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sec_content_hash_no_trivial_collision() {
        // SHA-256 should not collide on short, similar inputs.
        // With the old FNV-1a hash, crafted collisions were trivial.
        let inputs = vec![
            ("aaaaaaaaaaaaaaaa", "aaaaaaaaaaaaaaab"),
            ("foobar\\x00", "foobar\\x01"),
            ("A", "B"),
            ("test file 1", "test file 2"),
        ];
        for (a, b) in inputs {
            assert_ne!(content_hash(a), content_hash(b),
                "SHA-256 should not collide on '{}' vs '{}'", a, b);
        }
    }

    #[test]
    fn sec_cache_file_permissions_restricted() {
        // Cache files should be readable only by the owner (0600 on Unix).
        let dir = std::env::temp_dir().join("statico_sec_cache_perms");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        {
            let mut cache = IncrementalCache::new(&dir);
            cache.set("src/a.ts", "hash1", test_cache_data(vec!["secret_export"]));
            cache.save();
        }
        let cache_file = dir.join(".statico").join("cache").join("index.json");
        if cache_file.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = fs::metadata(&cache_file).unwrap().permissions().mode();
                let others_perm = mode & 0o007;
                assert_eq!(others_perm, 0,
                    "Cache file should not be world-readable: mode={:o}", mode);
            }
        }
        let _ = fs::remove_dir_all(&dir);
    }

    // ── V7-4: ensure_gitignore must detect real patterns, not substrings ──
    #[test]
    fn sec_gitignore_adds_when_only_comment() {
        let dir = std::env::temp_dir().join("statico_sec_gitignore_comment");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // A .gitignore with a comment mentioning statico should NOT prevent
        // the real `.statico/` pattern from being added.
        fs::write(dir.join(".gitignore"), "# statico is a great tool\nnode_modules/\n").unwrap();
        ensure_gitignore(&dir);
        let contents = fs::read_to_string(dir.join(".gitignore")).unwrap();
        assert!(contents.contains(".statico/"),
            "ensure_gitignore should add .statico/ even when comment mentions it, got:\n{}", contents);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sec_gitignore_skips_when_pattern_present() {
        let dir = std::env::temp_dir().join("statico_sec_gitignore_exists");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".gitignore"), ".statico/\nnode_modules/\n").unwrap();
        ensure_gitignore(&dir);
        let contents = fs::read_to_string(dir.join(".gitignore")).unwrap();
        // Should NOT add a duplicate entry
        let count = contents.matches(".statico").count();
        assert_eq!(count, 1,
            "should not add duplicate .statico entry, found {} occurrences:\n{}", count, contents);
        let _ = fs::remove_dir_all(&dir);
    }
}
