//! Unused type/interface export detection.
//!
//! Finds exported TypeScript `type` aliases and `interface` declarations that
//! are never imported by any other file.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::types::UnusedTypeIssue;

/// Detect exported types/interfaces that no file imports.
///
/// * `type_exports` — maps file path → list of `(name, kind)` pairs
///   where `kind` is `"type"` or `"interface"`.
/// * `file_sources` — `(path, source)` pairs used to scan for import usage.
/// * `entry_points` — files whose exports are considered consumed by convention.
pub fn detect(
    type_exports: &BTreeMap<String, Vec<(String, String)>>,
    file_sources: &[(String, String)],
    entry_points: &[String],
) -> Vec<UnusedTypeIssue> {
    let ep_set: HashSet<&str> = entry_points.iter().map(|s| s.as_str()).collect();

    // Collect all type names that appear in import statements across the codebase.
    let mut imported_names: BTreeSet<String> = BTreeSet::new();
    for (_path, source) in file_sources {
        collect_imported_names(source, &mut imported_names);
    }

    let mut issues = Vec::new();
    for (path, types) in type_exports {
        // Skip entry points — their exports are consumed by the framework.
        if ep_set.contains(path.as_str()) {
            continue;
        }
        for (name, kind) in types {
            if !imported_names.contains(name) {
                issues.push(UnusedTypeIssue { name: name.clone(), path: path.clone(), kind: kind.clone() });
            }
        }
    }

    issues.sort_by(|a, b| a.path.cmp(&b.path).then(a.name.cmp(&b.name)));
    issues
}

/// Scan source text for imported identifiers.
///
/// Matches patterns like:
/// - `import { Name } from ...`
/// - `import type { Name } from ...`
/// - `import { type Name } from ...`
/// - `import Name from ...`
fn collect_imported_names(source: &str, out: &mut BTreeSet<String>) {
    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("import ") {
            continue;
        }
        // Try to extract names between { and }.
        if let Some(braces) = extract_braces(trimmed) {
            for part in braces.split(',') {
                let name = part.trim().trim_start_matches("type ").split_whitespace().next().unwrap_or("");
                if !name.is_empty() && name != "type" {
                    out.insert(name.to_string());
                }
            }
        } else {
            // Default/namespace import: `import Name from ...`
            let without_import = trimmed.strip_prefix("import ").unwrap_or(trimmed);
            // Skip `type` keyword after import for `import type Foo from ...`.
            let without_type = without_import.strip_prefix("type ").unwrap_or(without_import);
            let first_word = without_type.split_whitespace().next().unwrap_or("");
            // Only add if it looks like an identifier (not a keyword/brace/star).
            if !first_word.is_empty() && !first_word.starts_with('{') && first_word != "*" && first_word != "from" {
                out.insert(first_word.to_string());
            }
        }
    }
}

/// Extract the content between the first `{` and matching `}` in a string.
fn extract_braces(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end > start { Some(&s[start + 1..end]) } else { None }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_unused_type() {
        let type_exports = BTreeMap::from([(
            "src/types.ts".into(),
            vec![("Config".into(), "interface".into()), ("Result".into(), "type".into())],
        )]);
        let file_sources = vec![
            (
                "src/types.ts".into(),
                "export interface Config { debug: boolean } export type Result<T> = T | null;".into(),
            ),
            ("src/app.ts".into(), "import type { Config } from './types';".into()),
        ];
        let issues = detect(&type_exports, &file_sources, &[]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].name, "Result");
        assert_eq!(issues[0].kind, "type");
    }

    #[test]
    fn skip_entry_points() {
        let type_exports = BTreeMap::from([("src/index.ts".into(), vec![("Config".into(), "interface".into())])]);
        let file_sources = vec![("src/index.ts".into(), "export interface Config { debug: boolean }".into())];
        let issues = detect(&type_exports, &file_sources, &["src/index.ts".into()]);
        assert!(issues.is_empty());
    }

    #[test]
    fn all_types_used() {
        let type_exports = BTreeMap::from([("src/types.ts".into(), vec![("Config".into(), "interface".into())])]);
        let file_sources = vec![
            ("src/types.ts".into(), "export interface Config { debug: boolean }".into()),
            ("src/app.ts".into(), "import { Config } from './types';".into()),
        ];
        let issues = detect(&type_exports, &file_sources, &[]);
        assert!(issues.is_empty());
    }

    #[test]
    fn collect_imported_names_various_patterns() {
        let mut names = BTreeSet::new();
        collect_imported_names("import { Foo, Bar } from './mod';", &mut names);
        collect_imported_names("import type { Baz } from './mod';", &mut names);
        collect_imported_names("import { type Qux } from './mod';", &mut names);
        collect_imported_names("import MyDefault from './mod';", &mut names);
        assert!(names.contains("Foo"));
        assert!(names.contains("Bar"));
        assert!(names.contains("Baz"));
        assert!(names.contains("Qux"));
        assert!(names.contains("MyDefault"));
    }
}
