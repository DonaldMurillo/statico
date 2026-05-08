//! npm/yarn monorepo profile.
//!
//! Detects: package.json with "workspaces" field (fallback when no other monorepo tool detected)
//! Workspaces: parsed from package.json "workspaces" field

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;
use std::path::Path;

use super::MonorepoProfile;

/// npm/yarn monorepo profile.
pub struct NpmProfile;

impl MonorepoProfile for NpmProfile {
    fn name(&self) -> &'static str {
        "npm/yarn"
    }

    fn detect(&self, root: &Path, _pkg_deps: Option<&HashSet<String>>) -> bool {
        // Any package.json with a "workspaces" field.
        let content = match std::fs::read_to_string(root.join("package.json")) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let pkg: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return false,
        };
        pkg.get("workspaces").is_some()
    }

    fn parse_workspaces(&self, root: &Path) -> Vec<String> {
        parse_npm_workspaces(root)
    }
}

// ---------------------------------------------------------------------------
// Parsers
// ---------------------------------------------------------------------------

fn parse_npm_workspaces(root: &Path) -> Vec<String> {
    let content = match std::fs::read_to_string(root.join("package.json")) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let pkg: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let ws = match pkg.get("workspaces") {
        Some(w) => w,
        None => return Vec::new(),
    };
    let items = match ws {
        serde_json::Value::Array(arr) => arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect(),
        serde_json::Value::Object(obj) => {
            // yarn workspaces: { packages: [...] }
            if let Some(packages) = obj.get("packages").and_then(|p| p.as_array()) {
                packages.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
            } else {
                return Vec::new();
            }
        }
        _ => return Vec::new(),
    };
    super::glob_to_prefix(items)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_detect_via_workspaces() {
        let tmp = std::env::temp_dir().join("statico_test_npm_detect");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("package.json"), r#"{"workspaces": ["packages/*"]}"#).unwrap();
        let profile = NpmProfile;
        assert!(profile.detect(&tmp, None));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn npm_parse_workspaces_array() {
        let tmp = std::env::temp_dir().join("statico_test_npm_ws");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("package.json"), r#"{"workspaces": ["packages/*", "apps/*"]}"#).unwrap();
        let profile = NpmProfile;
        let packages = profile.parse_workspaces(&tmp);
        assert_eq!(packages, vec!["packages/", "apps/"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn npm_parse_workspaces_object() {
        let tmp = std::env::temp_dir().join("statico_test_npm_ws_obj");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("package.json"), r#"{"workspaces": {"packages": ["packages/*"]}}"#).unwrap();
        let profile = NpmProfile;
        let packages = profile.parse_workspaces(&tmp);
        assert_eq!(packages, vec!["packages/"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
