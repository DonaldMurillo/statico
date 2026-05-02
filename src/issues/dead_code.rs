//! Dead code detection — files unreachable from any entry point.
//!
//! Two layers:
//! 1. **Unreachable**: files not reachable from ANY entry point (high confidence).
//! 2. **Framework-dead**: files only reachable through implicit entries like
//!    migrations, scripts, mocks — alive for tooling but dead at runtime (medium confidence).

use std::collections::{BTreeMap, HashSet};

use crate::types::DeadCodeIssue;

/// Find files not reachable from any entry point via the import graph.
pub fn detect(
    entry_points: &[String],
    dep_graph: &BTreeMap<String, Vec<String>>,
    file_loc: &BTreeMap<String, usize>,
) -> Vec<DeadCodeIssue> {
    let reachable = bfs_reachable(entry_points, dep_graph);

    let mut dead: Vec<DeadCodeIssue> = dep_graph
        .keys()
        .filter(|path| !reachable.contains(*path))
        .map(|path| DeadCodeIssue {
            path: path.clone(),
            lines_of_code: file_loc.get(path).copied().unwrap_or(0),
            confidence: 0.95,
            reason: "not reachable from any entry point".into(),
        })
        .collect();

    dead.sort_by(|a, b| a.path.cmp(&b.path));
    dead
}

/// Find files only reachable through implicit entry points, not framework ones.
///
/// These are files like individual migration files, mock files, and scripts
/// that are loaded by tooling but not part of the runtime application.
///
/// Only flags files that are THEMSELVES implicit entry points, not their
/// transitive dependencies. This avoids false positives on shared utilities
/// that happen to be imported by both app and test code.
pub fn detect_framework_dead(
    framework_eps: &[String],
    implicit_eps: &[String],
    dep_graph: &BTreeMap<String, Vec<String>>,
    file_loc: &BTreeMap<String, usize>,
) -> Vec<DeadCodeIssue> {
    let framework_reachable = bfs_reachable(framework_eps, dep_graph);

    // Only flag implicit EPs that aren't also reachable from framework EPs.
    // Restrict to src/ files — tooling files (scripts, .claude, eslint-plugins)
    // are not meaningful dead code findings.
    let mut framework_dead: Vec<DeadCodeIssue> = implicit_eps
        .iter()
        .filter(|ep| !framework_reachable.contains(*ep))
        .filter(|ep| dep_graph.contains_key(*ep))
        .filter(|ep| ep.starts_with("src/") || ep.starts_with("__mocks__/") || ep.starts_with("scripts/"))
        // Exclude test files — they're tooling, not dead code.
        .filter(|ep| !ep.contains(".test.") && !ep.contains(".spec."))
        .map(|path| DeadCodeIssue {
            path: path.clone(),
            lines_of_code: file_loc.get(path).copied().unwrap_or(0),
            confidence: 0.7,
            reason: "implicit entry point not reachable from runtime".into(),
        })
        .collect();

    framework_dead.sort_by(|a, b| a.path.cmp(&b.path));
    framework_dead
}

/// BFS from given starting points through the import graph.
fn bfs_reachable<'a>(starts: &'a [String], dep_graph: &'a BTreeMap<String, Vec<String>>) -> HashSet<&'a String> {
    let mut reachable: HashSet<&'a String> = HashSet::new();
    let mut stack: Vec<&str> = Vec::new();

    for ep in starts {
        if dep_graph.contains_key(ep) && reachable.insert(ep) {
            stack.push(ep);
        }
    }

    while let Some(current) = stack.pop() {
        if let Some(targets) = dep_graph.get(current) {
            for target in targets {
                if reachable.insert(target) {
                    stack.push(target);
                }
            }
        }
    }

    reachable
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_dead_code_when_all_reachable() {
        let ep = vec!["src/index.ts".into()];
        let graph =
            BTreeMap::from([("src/index.ts".into(), vec!["src/utils.ts".into()]), ("src/utils.ts".into(), vec![])]);
        let loc = BTreeMap::new();
        assert!(detect(&ep, &graph, &loc).is_empty());
    }

    #[test]
    fn orphan_file_is_dead() {
        let ep = vec!["src/index.ts".into()];
        let graph = BTreeMap::from([("src/index.ts".into(), vec![]), ("src/orphan.ts".into(), vec![])]);
        let loc = BTreeMap::from([("src/orphan.ts".into(), 45)]);
        let dead = detect(&ep, &graph, &loc);
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].path, "src/orphan.ts");
        assert_eq!(dead[0].lines_of_code, 45);
    }

    #[test]
    fn transitive_reachability() {
        let ep = vec!["src/a.ts".into()];
        let graph = BTreeMap::from([
            ("src/a.ts".into(), vec!["src/b.ts".into()]),
            ("src/b.ts".into(), vec!["src/c.ts".into()]),
            ("src/c.ts".into(), vec![]),
            ("src/dead.ts".into(), vec![]),
        ]);
        let dead = detect(&ep, &graph, &BTreeMap::new());
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].path, "src/dead.ts");
    }

    #[test]
    fn no_entry_points_means_all_dead() {
        let graph = BTreeMap::from([("src/a.ts".into(), vec!["src/b.ts".into()]), ("src/b.ts".into(), vec![])]);
        let dead = detect(&[], &graph, &BTreeMap::new());
        assert_eq!(dead.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Framework-aware entry point tests
    // -----------------------------------------------------------------------
    // When only framework EPs are passed (not implicit ones), files reachable
    // solely through implicit entries (migrations, scripts, mocks) are dead.

    #[test]
    fn migration_files_dead_when_only_framework_eps_used() {
        // Simulates: framework EP imports utils; migration barrel imports
        // individual migrations. If we only pass framework EPs, migrations
        // should be dead.
        let framework_eps = vec!["src/index.ts".into()];
        let graph = BTreeMap::from([
            // Framework entry → app utilities
            ("src/index.ts".into(), vec!["src/utils.ts".into()]),
            ("src/utils.ts".into(), vec![]),
            // Migration barrel (implicit EP) → individual migrations
            ("src/migrations/index.ts".into(), vec!["src/migrations/001_create_users.ts".into()]),
            ("src/migrations/001_create_users.ts".into(), vec![]),
        ]);
        let dead = detect(&framework_eps, &graph, &BTreeMap::new());
        // Both migration barrel and individual migration are dead from app perspective
        assert_eq!(dead.len(), 2);
        let paths: Vec<&str> = dead.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"src/migrations/index.ts"));
        assert!(paths.contains(&"src/migrations/001_create_users.ts"));
    }

    #[test]
    fn shared_utility_not_dead_even_if_also_imported_by_migration() {
        // A utility used by both app and migrations should NOT be dead.
        let framework_eps = vec!["src/index.ts".into()];
        let graph = BTreeMap::from([
            ("src/index.ts".into(), vec!["src/utils.ts".into()]),
            ("src/utils.ts".into(), vec![]),
            ("src/migrations/index.ts".into(), vec!["src/utils.ts".into()]),
        ]);
        let dead = detect(&framework_eps, &graph, &BTreeMap::new());
        // utils.ts is reachable from framework EP, so it's alive
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].path, "src/migrations/index.ts");
    }

    #[test]
    fn mock_files_dead_when_only_framework_eps_used() {
        let framework_eps = vec!["src/app.ts".into()];
        let graph = BTreeMap::from([
            ("src/app.ts".into(), vec!["src/lib.ts".into()]),
            ("src/lib.ts".into(), vec![]),
            ("__mocks__/payload-config.ts".into(), vec![]),
        ]);
        let dead = detect(&framework_eps, &graph, &BTreeMap::new());
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].path, "__mocks__/payload-config.ts");
    }

    #[test]
    fn script_files_dead_when_only_framework_eps_used() {
        let framework_eps = vec!["src/index.ts".into()];
        let graph = BTreeMap::from([
            ("src/index.ts".into(), vec![]),
            ("scripts/backfill-metadata-search-text.ts".into(), vec!["src/db.ts".into()]),
            ("src/db.ts".into(), vec![]),
        ]);
        // If src/db.ts is NOT reachable from framework EPs, both script and db are dead
        let dead = detect(&framework_eps, &graph, &BTreeMap::new());
        assert_eq!(dead.len(), 2);
        let paths: Vec<&str> = dead.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"scripts/backfill-metadata-search-text.ts"));
        assert!(paths.contains(&"src/db.ts"));
    }
}
