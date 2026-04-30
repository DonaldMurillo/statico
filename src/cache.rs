//! Incremental cache for parsed file results.
//!
//! Caches per-file parse data keyed by content hash so unchanged files
//! are skipped on re-runs. Cache lives in `{project_root}/.statico/cache/`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Per-file cached data.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct CachedFileData {
    pub exports: Vec<String>,
    pub loc: usize,
    pub total_lines: usize,
    pub functions: usize,
    pub classes: usize,
    pub complexity: usize,
    pub max_nesting_depth: usize,
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
        if let Err(e) = fs::write(&index_path, json) {
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

/// Compute a fast hash of file contents (using a simple FNV-1a-like approach).
pub fn content_hash(content: &str) -> String {
    // Simple hash — not cryptographic, just for cache invalidation.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in content.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

/// Ensure `.statico/` is in the project's .gitignore.
pub fn ensure_gitignore(project_root: &Path) {
    let gitignore_path = project_root.join(".gitignore");
    let existing = fs::read_to_string(&gitignore_path).unwrap_or_default();
    if !existing.contains(".statico")
        && let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&gitignore_path) {
            let _ = writeln!(f, "\n# statico cache\n.statico/");
        }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let data = CachedFileData {
            exports: vec!["foo".into()],
            loc: 10,
            total_lines: 15,
            functions: 2,
            classes: 1,
            complexity: 3,
            max_nesting_depth: 2,
        };
        cache.set("src/a.ts", "abc123", data.clone());
        assert!(cache.get("src/a.ts", "abc123").is_some());
        assert!(cache.get("src/a.ts", "wrong").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cache_persist() {
        let dir = std::env::temp_dir().join("statico_test_cache_persist");
        let _ = fs::remove_dir_all(&dir);
        let data = CachedFileData {
            exports: vec![],
            loc: 5,
            total_lines: 8,
            functions: 0,
            classes: 0,
            complexity: 1,
            max_nesting_depth: 0,
        };
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
}
