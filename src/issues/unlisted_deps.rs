//! Unlisted dependency detection.
//!
//! Finds external imports that are NOT listed in `package.json`'s
//! `dependencies` or `devDependencies`. These would fail on a clean install.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::Path;

use crate::types::UnlistedDepIssue;

/// Detect external imports that are not listed in package.json.
///
/// * `root` — project root directory (where `package.json` lives).
/// * `external_imports` — `(importing_file, package_name)` pairs.
///   Each pair records which file imported which external package.
pub fn detect(root: &Path, external_imports: &[(String, String)]) -> Vec<UnlistedDepIssue> {
    let all_deps = read_all_deps(root);

    let mut issues = Vec::new();
    for (importing_file, package_name) in external_imports {
        if !all_deps.contains(package_name.as_str()) {
            issues.push(UnlistedDepIssue { package_name: package_name.clone(), imported_by: importing_file.clone() });
        }
    }

    issues.sort_by(|a, b| a.package_name.cmp(&b.package_name).then(a.imported_by.cmp(&b.imported_by)));
    issues
}

// ---------------------------------------------------------------------------
// package.json parsing
// ---------------------------------------------------------------------------

fn read_all_deps(root: &Path) -> BTreeSet<String> {
    let pkg_path = root.join("package.json");
    if !pkg_path.exists() {
        return BTreeSet::new();
    }
    let content = std::fs::read_to_string(&pkg_path).unwrap_or_default();
    let pkg: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
    let mut all = BTreeSet::new();
    for field in &["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"] {
        if let Some(obj) = pkg.get(field).and_then(|v| v.as_object()) {
            for key in obj.keys() {
                all.insert(key.clone());
            }
        }
    }
    all
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn no_package_json_means_all_unlisted() {
        let tmp = std::env::temp_dir().join("statico-test-no-pkg2");
        let _ = fs::create_dir_all(&tmp);
        let imports = vec![("src/app.ts".into(), "react".into())];
        let issues = detect(&tmp, &imports);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].package_name, "react");
        let _ = fs::remove_dir(&tmp);
    }

    #[test]
    fn all_imports_listed() {
        let tmp = std::env::temp_dir().join("statico-test-listed");
        let _ = fs::create_dir_all(&tmp);
        fs::write(tmp.join("package.json"), r#"{"dependencies": {"react": "^18.0.0", "lodash": "^4.0.0"}}"#).unwrap();
        let imports = vec![("src/app.ts".into(), "react".into()), ("src/utils.ts".into(), "lodash".into())];
        let issues = detect(&tmp, &imports);
        assert!(issues.is_empty());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn unlisted_import_reported() {
        let tmp = std::env::temp_dir().join("statico-test-unlisted");
        let _ = fs::create_dir_all(&tmp);
        fs::write(tmp.join("package.json"), r#"{"dependencies": {"react": "^18.0.0"}}"#).unwrap();
        let imports = vec![("src/app.ts".into(), "react".into()), ("src/app.ts".into(), "lodash".into())];
        let issues = detect(&tmp, &imports);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].package_name, "lodash");
        assert_eq!(issues[0].imported_by, "src/app.ts");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn dev_deps_also_counted() {
        let tmp = std::env::temp_dir().join("statico-test-devdeps");
        let _ = fs::create_dir_all(&tmp);
        fs::write(tmp.join("package.json"), r#"{"devDependencies": {"jest": "^29.0.0"}}"#).unwrap();
        let imports = vec![("src/app.test.ts".into(), "jest".into())];
        let issues = detect(&tmp, &imports);
        assert!(issues.is_empty(), "devDependencies should be counted as listed");
        let _ = fs::remove_dir_all(&tmp);
    }
}
