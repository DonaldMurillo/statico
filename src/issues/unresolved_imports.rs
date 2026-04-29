//! Unresolved import detection.
//!
//! Finds import specifiers in the dep_graph that still start with relative or
//! alias prefixes (`./`, `../`, `@/`, `~`, `#`), meaning the resolver could
//! not map them to an actual file.

use std::collections::BTreeMap;

use crate::types::UnresolvedImportIssue;

/// Detect unresolved import specifiers.
///
/// In the dep_graph, a resolved import is a relative path like `src/utils.ts`.
/// An unresolved one still looks like the original specifier: `./missing`,
/// `@/components/foo`, `~/lib/bar`, `#internal/baz`.
pub fn detect(dep_graph: &BTreeMap<String, Vec<String>>) -> Vec<UnresolvedImportIssue> {
    let mut issues = Vec::new();

    for (source_file, targets) in dep_graph {
        for spec in targets {
            if is_unresolved(spec) {
                issues.push(UnresolvedImportIssue {
                    source_file: source_file.clone(),
                    import_spec: spec.clone(),
                });
            }
        }
    }

    issues.sort_by(|a, b| a.source_file.cmp(&b.source_file).then(a.import_spec.cmp(&b.import_spec)));
    issues
}

/// A specifier is considered unresolved if it still starts with a relative
/// or alias prefix — meaning it was never mapped to an actual file path.
fn is_unresolved(spec: &str) -> bool {
    spec.starts_with("./")
        || spec.starts_with("../")
        || spec.starts_with("@/")
        || spec.starts_with("~/")
        || spec.starts_with('#')
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_unresolved() {
        let graph = BTreeMap::from([
            ("src/app.ts".into(), vec!["src/utils.ts".into(), "react".into()]),
        ]);
        let issues = detect(&graph);
        assert!(issues.is_empty());
    }

    #[test]
    fn unresolved_relative() {
        let graph = BTreeMap::from([
            ("src/app.ts".into(), vec!["./missing.ts".into(), "src/utils.ts".into()]),
        ]);
        let issues = detect(&graph);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].import_spec, "./missing.ts");
        assert_eq!(issues[0].source_file, "src/app.ts");
    }

    #[test]
    fn unresolved_alias() {
        let graph = BTreeMap::from([
            ("src/app.ts".into(), vec!["@/components/foo".into(), "~/lib/bar".into(), "#internal/baz".into()]),
        ]);
        let issues = detect(&graph);
        assert_eq!(issues.len(), 3);
    }

    #[test]
    fn mixed_resolved_and_unresolved() {
        let graph = BTreeMap::from([
            ("src/app.ts".into(), vec!["src/utils.ts".into(), "./broken".into(), "lodash".into()]),
            ("src/utils.ts".into(), vec!["../helpers".into()]),
        ]);
        let issues = detect(&graph);
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().any(|i| i.import_spec == "./broken"));
        assert!(issues.iter().any(|i| i.import_spec == "../helpers"));
    }

    #[test]
    fn empty_graph() {
        let graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let issues = detect(&graph);
        assert!(issues.is_empty());
    }
}
