//! Tooling/config entry point discovery.

use std::collections::BTreeSet;

/// Tooling/config directories whose scripts are entry points (loaded by config, not imports).
const TOOLING_DIRS: &[&str] = &[
    ".claude/hooks",
    ".claude/skills",
    "eslint-plugins",
    "eslint-rules",
    ".eslint-rules",
    "scripts",
    "tools",
    "gulpfile",
    "gruntfile",
];

/// Discover entry points from tooling directories.
pub fn add_tooling_entries(source_files: &[(String, String)], entry_points: &mut BTreeSet<String>) {
    for (rel, _) in source_files {
        let lower = rel.to_lowercase();
        for dir in TOOLING_DIRS {
            if lower.starts_with(dir) || lower.starts_with(&format!("./{dir}")) {
                entry_points.insert(rel.clone());
                break;
            }
        }
    }
}
