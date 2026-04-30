//! Unused npm dependency detection.
//!
//! Compares `dependencies` and `devDependencies` from `package.json` against
//! the set of external imports actually used in the codebase.

use std::collections::BTreeSet;
use std::path::Path;

use crate::types::UnusedDepIssue;

/// Detect dependencies listed in `package.json` that are never imported.
///
/// * `root` — project root directory (where `package.json` lives).
/// * `external_imports` — package names extracted from all import statements.
pub fn detect(root: &Path, external_imports: &[String]) -> Vec<UnusedDepIssue> {
    let (deps, dev_deps) = read_package_deps(root);
    let imported: BTreeSet<&str> = external_imports.iter().map(|s| s.as_str()).collect();

    let mut issues = Vec::new();

    for pkg in &deps {
        if !imported.contains(pkg.as_str()) {
            issues.push(UnusedDepIssue { package_name: pkg.clone(), location: "dependencies".to_string() });
        }
    }

    for pkg in &dev_deps {
        if !imported.contains(pkg.as_str()) {
            issues.push(UnusedDepIssue { package_name: pkg.clone(), location: "devDependencies".to_string() });
        }
    }

    issues.sort_by(|a, b| a.package_name.cmp(&b.package_name));
    issues
}

// ---------------------------------------------------------------------------
// package.json parsing
// ---------------------------------------------------------------------------

fn read_package_deps(root: &Path) -> (BTreeSet<String>, BTreeSet<String>) {
    let pkg_path = root.join("package.json");
    if !pkg_path.exists() {
        return (BTreeSet::new(), BTreeSet::new());
    }
    let content = std::fs::read_to_string(&pkg_path).unwrap_or_default();
    let pkg: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
    let deps = extract_dep_names(&pkg, "dependencies");
    let dev_deps = extract_dep_names(&pkg, "devDependencies");
    (deps, dev_deps)
}

fn extract_dep_names(pkg: &serde_json::Value, field: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(obj) = pkg.get(field).and_then(|v| v.as_object()) {
        for key in obj.keys() {
            names.insert(key.clone());
        }
    }
    names
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn no_package_json() {
        let tmp = std::env::temp_dir().join("statico-test-no-pkg");
        let _ = fs::create_dir_all(&tmp);
        let issues = detect(&tmp, &["react".to_string()]);
        assert!(issues.is_empty());
        let _ = fs::remove_dir(&tmp);
    }

    #[test]
    fn all_deps_used() {
        let tmp = std::env::temp_dir().join("statico-test-used-deps");
        let _ = fs::create_dir_all(&tmp);
        fs::write(tmp.join("package.json"), r#"{"dependencies": {"react": "^18.0.0", "lodash": "^4.0.0"}}"#).unwrap();
        let issues = detect(&tmp, &["react".to_string(), "lodash".to_string()]);
        assert!(issues.is_empty());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn unused_deps_reported() {
        let tmp = std::env::temp_dir().join("statico-test-unused-deps");
        let _ = fs::create_dir_all(&tmp);
        fs::write(
            tmp.join("package.json"),
            r#"{"dependencies": {"react": "^18.0.0"}, "devDependencies": {"jest": "^29.0.0"}}"#,
        )
        .unwrap();
        let issues = detect(&tmp, &["react".to_string()]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].package_name, "jest");
        assert_eq!(issues[0].location, "devDependencies");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn extract_dep_names_handles_invalid_json() {
        let pkg: serde_json::Value = serde_json::from_str("not json").unwrap_or_default();
        let names = extract_dep_names(&pkg, "dependencies");
        assert!(names.is_empty());
    }
}
