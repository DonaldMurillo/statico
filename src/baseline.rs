//! Baseline file support — let teams ratchet down issues over time without
//! the noise of pre-existing findings (audit F1.7).
//!
//! A baseline is a JSON file containing a stable fingerprint per known
//! issue. Subsequent runs of `statico analyze --baseline <file>` filter out
//! every issue whose fingerprint is in the baseline.
//!
//! Fingerprint format
//! ------------------
//!
//! Each fingerprint is a short string of the form
//! `<category>::<key>` where `<key>` uniquely identifies the issue inside
//! its category. Examples:
//!
//! ```text
//! dead_code::src/foo.rs
//! unused_export::src/bar.ts::Helper
//! gotcha::src/lib.tsx::42::react/no-conditional-hook
//! circular_dep::src/a.rs->src/b.rs->src/a.rs
//! ```
//!
//! Fingerprints intentionally avoid line numbers for `dead_code` /
//! `unused_export` / `unused_dep` so trivial reformatting does not
//! invalidate the baseline. Gotchas and unresolved imports include the
//! line number because they are inherently location-bound.
//!
//! On-disk format
//! --------------
//!
//! ```json
//! {
//!   "version": 1,
//!   "generated_at": "2026-05-02",
//!   "fingerprints": [
//!     "dead_code::src/foo.rs",
//!     "unused_export::src/bar.ts::Helper"
//!   ]
//! }
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::types::AnalysisOutput;

/// Current baseline file schema version.
pub const BASELINE_VERSION: u32 = 1;

/// On-disk representation of a baseline file.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BaselineFile {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    pub fingerprints: Vec<String>,
}

/// In-memory baseline — a set of fingerprints we should silently suppress.
#[derive(Debug, Default, Clone)]
pub struct Baseline {
    fingerprints: BTreeSet<String>,
}

impl Baseline {
    /// Build a baseline directly from an analysis output (used when writing).
    pub fn from_output(output: &AnalysisOutput) -> Self {
        let mut fingerprints = BTreeSet::new();
        for fp in fingerprint_all(output) {
            fingerprints.insert(fp);
        }
        Self { fingerprints }
    }

    /// Load a baseline from a JSON file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("failed to read baseline {}: {}", path.display(), e))?;
        let file: BaselineFile = serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse baseline {}: {}", path.display(), e))?;
        if file.version != BASELINE_VERSION {
            return Err(format!(
                "baseline schema version mismatch — file has v{}, statico expects v{}",
                file.version, BASELINE_VERSION
            ));
        }
        Ok(Self { fingerprints: file.fingerprints.into_iter().collect() })
    }

    /// Write the baseline to a JSON file (atomically: write to .tmp, rename).
    pub fn write(&self, path: &Path) -> Result<(), String> {
        let file = BaselineFile {
            version: BASELINE_VERSION,
            generated_at: Some(today_string()),
            fingerprints: self.fingerprints.iter().cloned().collect(),
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| format!("failed to serialize baseline: {}", e))?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("failed to write {}: {}", tmp.display(), e))?;
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("failed to install baseline at {}: {}", path.display(), e)
        })?;
        Ok(())
    }

    /// Number of fingerprints in the baseline.
    pub fn len(&self) -> usize {
        self.fingerprints.len()
    }

    /// Whether the baseline is empty.
    pub fn is_empty(&self) -> bool {
        self.fingerprints.is_empty()
    }

    /// Filter the analysis output in place: remove every issue whose
    /// fingerprint is in the baseline. Returns the number of suppressed
    /// issues.
    pub fn apply(&self, output: &mut AnalysisOutput) -> usize {
        let mut suppressed = 0usize;
        let bl = &self.fingerprints;

        macro_rules! retain_with_fp {
            ($collection:expr, $fp_fn:expr) => {{
                let before = $collection.len();
                $collection.retain(|item| !bl.contains(&$fp_fn(item)));
                suppressed += before - $collection.len();
            }};
        }

        retain_with_fp!(output.issues.dead_code, |i: &crate::types::DeadCodeIssue| fp_dead_code(i));
        retain_with_fp!(output.issues.unused_exports, |i: &crate::types::UnusedExportIssue| fp_unused_export(i));
        retain_with_fp!(output.issues.duplicate_exports, |i: &crate::types::DuplicateExportIssue| fp_duplicate_export(
            i
        ));
        retain_with_fp!(output.issues.unused_types, |i: &crate::types::UnusedTypeIssue| fp_unused_type(i));
        retain_with_fp!(output.issues.gotchas, |i: &crate::types::GotchaIssue| fp_gotcha(i));
        retain_with_fp!(output.issues.unused_dependencies, |i: &crate::types::UnusedDepIssue| fp_unused_dep(i));
        retain_with_fp!(output.issues.circular_dependencies, |i: &crate::types::CircularDepIssue| fp_circular(i));
        retain_with_fp!(output.issues.unresolved_imports, |i: &crate::types::UnresolvedImportIssue| fp_unresolved(i));
        retain_with_fp!(output.issues.unlisted_dependencies, |i: &crate::types::UnlistedDepIssue| fp_unlisted(i));

        suppressed
    }
}

fn fingerprint_all(output: &AnalysisOutput) -> Vec<String> {
    let mut out = Vec::new();
    for i in &output.issues.dead_code {
        out.push(fp_dead_code(i));
    }
    for i in &output.issues.unused_exports {
        out.push(fp_unused_export(i));
    }
    for i in &output.issues.duplicate_exports {
        out.push(fp_duplicate_export(i));
    }
    for i in &output.issues.unused_types {
        out.push(fp_unused_type(i));
    }
    for i in &output.issues.gotchas {
        out.push(fp_gotcha(i));
    }
    for i in &output.issues.unused_dependencies {
        out.push(fp_unused_dep(i));
    }
    for i in &output.issues.circular_dependencies {
        out.push(fp_circular(i));
    }
    for i in &output.issues.unresolved_imports {
        out.push(fp_unresolved(i));
    }
    for i in &output.issues.unlisted_dependencies {
        out.push(fp_unlisted(i));
    }
    out
}

fn fp_dead_code(i: &crate::types::DeadCodeIssue) -> String {
    format!("dead_code::{}", i.path)
}
fn fp_unused_export(i: &crate::types::UnusedExportIssue) -> String {
    format!("unused_export::{}::{}", i.path, i.name)
}
fn fp_duplicate_export(i: &crate::types::DuplicateExportIssue) -> String {
    let mut locs = i.locations.clone();
    locs.sort();
    format!("duplicate_export::{}::{}", i.name, locs.join("|"))
}
fn fp_unused_type(i: &crate::types::UnusedTypeIssue) -> String {
    format!("unused_type::{}::{}", i.path, i.name)
}
fn fp_gotcha(i: &crate::types::GotchaIssue) -> String {
    format!("gotcha::{}::{}::{}", i.file, i.line, i.rule)
}
fn fp_unused_dep(i: &crate::types::UnusedDepIssue) -> String {
    format!("unused_dep::{}::{}", i.location, i.package_name)
}
fn fp_circular(i: &crate::types::CircularDepIssue) -> String {
    let mut chain = i.files.clone();
    // Rotate so the alphabetically smallest file is first — same cycle
    // started from any node fingerprints identically.
    if let Some((min_idx, _)) = chain.iter().enumerate().min_by_key(|(_, s)| s.as_str()) {
        chain.rotate_left(min_idx);
    }
    format!("circular_dep::{}", chain.join("->"))
}
fn fp_unresolved(i: &crate::types::UnresolvedImportIssue) -> String {
    format!("unresolved_import::{}::{}", i.source_file, i.import_spec)
}
fn fp_unlisted(i: &crate::types::UnlistedDepIssue) -> String {
    format!("unlisted_dep::{}::{}", i.imported_by, i.package_name)
}

fn today_string() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let total_days = secs / 86400;
    let z = total_days as i64 + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn empty_output() -> AnalysisOutput {
        AnalysisOutput {
            version: None,
            summary: None,
            detected_frameworks: None,
            monorepo: None,
            structure: Structure {
                root: std::path::PathBuf::from("/p"),
                entry_points: vec![],
                implicit_entries: vec![],
                source_files: vec![],
                config_files: vec![],
            },
            dependencies: Dependencies { imports: vec![], external: vec![] },
            quality: Quality { files: vec![] },
            issues: Issues {
                dead_code: vec![],
                unused_exports: vec![],
                duplicate_exports: vec![],
                duplicate_code: vec![],
                gotchas: vec![],
                unused_types: vec![],
                circular_dependencies: vec![],
                unused_dependencies: vec![],
                unresolved_imports: vec![],
                unlisted_dependencies: vec![],
                plugin_issues: vec![],
            },
            duplication: DuplicationSection {
                stats: DuplicationStats {
                    total_lines: 0,
                    duplicated_lines: 0,
                    duplication_percentage: 0.0,
                    clone_groups: 0,
                    clone_instances: 0,
                    clone_families: 0,
                },
                clone_groups: vec![],
                clone_families: vec![],
                mirrored_directories: vec![],
                repetitive_patterns: vec![],
            },
        }
    }

    #[test]
    fn fingerprint_dead_code_is_path_only() {
        let i = DeadCodeIssue {
            path: "src/foo.rs".to_string(),
            lines_of_code: 12,
            confidence: 0.95,
            reason: "unreachable".to_string(),
        };
        let i2 = DeadCodeIssue {
            // Same path, different LoC/reason — still same fingerprint
            path: "src/foo.rs".to_string(),
            lines_of_code: 999,
            confidence: 0.5,
            reason: "different".to_string(),
        };
        assert_eq!(fp_dead_code(&i), fp_dead_code(&i2));
        assert_eq!(fp_dead_code(&i), "dead_code::src/foo.rs");
    }

    #[test]
    fn fingerprint_circular_is_rotation_invariant() {
        let a = CircularDepIssue { files: vec!["b.rs".into(), "c.rs".into(), "a.rs".into()] };
        let b = CircularDepIssue { files: vec!["a.rs".into(), "b.rs".into(), "c.rs".into()] };
        let c = CircularDepIssue { files: vec!["c.rs".into(), "a.rs".into(), "b.rs".into()] };
        assert_eq!(fp_circular(&a), fp_circular(&b));
        assert_eq!(fp_circular(&b), fp_circular(&c));
    }

    #[test]
    fn baseline_apply_filters_known_issues() {
        let mut output = empty_output();
        output.issues.dead_code.push(DeadCodeIssue {
            path: "src/known.rs".into(),
            lines_of_code: 1,
            confidence: 0.9,
            reason: String::new(),
        });
        output.issues.dead_code.push(DeadCodeIssue {
            path: "src/new.rs".into(),
            lines_of_code: 1,
            confidence: 0.9,
            reason: String::new(),
        });
        let baseline = Baseline { fingerprints: BTreeSet::from(["dead_code::src/known.rs".to_string()]) };
        let suppressed = baseline.apply(&mut output);
        assert_eq!(suppressed, 1);
        assert_eq!(output.issues.dead_code.len(), 1);
        assert_eq!(output.issues.dead_code[0].path, "src/new.rs");
    }

    #[test]
    fn baseline_roundtrip_via_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("baseline.json");

        let mut output = empty_output();
        output.issues.unused_exports.push(UnusedExportIssue { name: "Helper".into(), path: "src/lib.ts".into() });
        output.issues.dead_code.push(DeadCodeIssue {
            path: "src/dead.ts".into(),
            lines_of_code: 5,
            confidence: 0.8,
            reason: String::new(),
        });

        let written = Baseline::from_output(&output);
        written.write(&path).unwrap();

        let loaded = Baseline::load(&path).unwrap();
        assert_eq!(loaded.len(), 2);

        // Apply the loaded baseline back to a fresh copy of the same output —
        // every issue should be suppressed.
        let mut to_filter = output.clone();
        let suppressed = loaded.apply(&mut to_filter);
        assert_eq!(suppressed, 2);
        assert!(to_filter.issues.dead_code.is_empty());
        assert!(to_filter.issues.unused_exports.is_empty());
    }

    #[test]
    fn baseline_load_rejects_wrong_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("baseline.json");
        std::fs::write(&path, r#"{"version":99,"fingerprints":[]}"#).unwrap();
        let result = Baseline::load(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("version"));
    }
}
