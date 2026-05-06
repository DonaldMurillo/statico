//! Turborepo monorepo profile.
//!
//! Detects: turbo.json
//! Workspaces: parsed from package.json workspaces or pnpm-workspace.yaml

use std::collections::HashSet;
use std::path::Path;

use super::MonorepoProfile;

/// Turborepo monorepo profile.
pub struct TurboProfile;

impl MonorepoProfile for TurboProfile {
    fn name(&self) -> &'static str {
        "turborepo"
    }

    fn detect(&self, root: &Path, _pkg_deps: Option<&HashSet<String>>) -> bool {
        root.join("turbo.json").exists()
    }

    fn parse_workspaces(&self, root: &Path) -> Vec<String> {
        // Turborepo uses npm/yarn/pnpm workspaces under the hood.
        parse_package_json_workspaces(root)
    }
}

// ---------------------------------------------------------------------------
// Parsers
// ---------------------------------------------------------------------------

fn parse_package_json_workspaces(root: &Path) -> Vec<String> {
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
    fn turbo_detect_via_json() {
        let tmp = std::env::temp_dir().join("statico_test_turbo_detect");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("turbo.json"), r#"{"$schema": "https://turbo.build/schema.json"}"#).unwrap();
        let profile = TurboProfile;
        assert!(profile.detect(&tmp, None));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
