//! Unused export detection — individual named exports that no file imports.
//! Excludes exports from entry point files, since frameworks consume those by convention.

use std::collections::{BTreeMap, HashSet};

use crate::types::UnusedExportIssue;

/// Find named exports that no other file imports.
/// `file_exports` maps file path → its export names.
/// `imported_names` maps target file path → names imported FROM that file (by any importer).
/// Files that are entry points are excluded — the framework consumes their exports.
pub fn detect(
    file_exports: &BTreeMap<String, Vec<String>>,
    imported_names: &BTreeMap<String, HashSet<String>>,
    entry_points: &[String],
) -> Vec<UnusedExportIssue> {
    let ep_set: HashSet<&str> = entry_points.iter().map(|s| s.as_str()).collect();

    let mut unused: Vec<UnusedExportIssue> = Vec::new();
    for (path, exports) in file_exports {
        if ep_set.contains(path.as_str()) {
            continue;
        }

        let names_imported = imported_names.get(path);

        for name in exports {
            let is_used = names_imported.is_some_and(|set| set.contains(name));
            if !is_used {
                unused.push(UnusedExportIssue {
                    name: name.clone(),
                    path: path.clone(),
                });
            }
        }
    }

    unused.sort_by(|a, b| a.path.cmp(&b.path).then(a.name.cmp(&b.name)));
    unused
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_when_file_not_imported() {
        let exports = BTreeMap::from([
            ("src/utils.ts".into(), vec!["helper".into(), "formatDate".into()]),
            ("src/index.ts".into(), vec!["main".into()]),
        ]);
        // Nothing imports from utils.ts
        let imported = BTreeMap::new();
        let eps = vec!["src/index.ts".into()];
        let unused = detect(&exports, &imported, &eps);
        // utils.ts is not an entry point and nothing imports its names.
        assert_eq!(unused.len(), 2);
        assert_eq!(unused[0].name, "formatDate");
        assert_eq!(unused[1].name, "helper");
    }

    #[test]
    fn flagged_when_not_entry_point_and_not_imported() {
        let exports = BTreeMap::from([
            ("src/utils.ts".into(), vec!["helper".into()]),
            ("src/dead.ts".into(), vec!["oldThing".into()]),
        ]);
        let imported = BTreeMap::from([
            ("src/utils.ts".into(), HashSet::from(["helper".into()])),
        ]);
        let eps = vec!["src/app.ts".into()];
        let unused = detect(&exports, &imported, &eps);
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].name, "oldThing");
        assert_eq!(unused[0].path, "src/dead.ts");
    }

    #[test]
    fn all_used_when_every_name_is_imported() {
        let exports = BTreeMap::from([
            ("src/utils.ts".into(), vec!["helper".into()]),
        ]);
        let imported = BTreeMap::from([
            ("src/utils.ts".into(), HashSet::from(["helper".into()])),
        ]);
        let unused = detect(&exports, &imported, &[]);
        assert!(unused.is_empty());
    }

    #[test]
    fn detects_partial_unused() {
        // File exports foo, bar, baz. Only foo is imported → bar and baz are unused.
        let exports = BTreeMap::from([
            ("src/utils.ts".into(), vec!["foo".into(), "bar".into(), "baz".into()]),
        ]);
        let imported = BTreeMap::from([
            ("src/utils.ts".into(), HashSet::from(["foo".into()])),
        ]);
        let unused = detect(&exports, &imported, &[]);
        assert_eq!(unused.len(), 2);
        assert_eq!(unused[0].name, "bar");
        assert_eq!(unused[1].name, "baz");
    }

    #[test]
    fn entry_point_exports_excluded() {
        let exports = BTreeMap::from([
            ("src/index.ts".into(), vec!["main".into(), "App".into()]),
            ("src/utils.ts".into(), vec!["helper".into()]),
        ]);
        // Nobody imports anything from either file.
        let imported = BTreeMap::new();
        let eps = vec!["src/index.ts".into()];
        let unused = detect(&exports, &imported, &eps);
        // Only utils.ts exports should be flagged (index.ts is an entry point).
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].name, "helper");
        assert_eq!(unused[0].path, "src/utils.ts");
    }
}
