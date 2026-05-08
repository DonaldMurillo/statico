//! pnpm monorepo profile.
//!
//! Detects: pnpm-workspace.yaml or pnpm-workspace.yml
//! Workspaces: parsed from pnpm-workspace.yaml

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;
use std::path::Path;

use super::MonorepoProfile;

/// pnpm monorepo profile.
pub struct PnpmProfile;

impl MonorepoProfile for PnpmProfile {
    fn name(&self) -> &'static str {
        "pnpm"
    }

    fn detect(&self, root: &Path, _pkg_deps: Option<&HashSet<String>>) -> bool {
        root.join("pnpm-workspace.yaml").exists() || root.join("pnpm-workspace.yml").exists()
    }

    fn parse_workspaces(&self, root: &Path) -> Vec<String> {
        parse_pnpm_workspaces(root)
    }
}

// ---------------------------------------------------------------------------
// Parsers
// ---------------------------------------------------------------------------

fn parse_pnpm_workspaces(root: &Path) -> Vec<String> {
    for name in &["pnpm-workspace.yaml", "pnpm-workspace.yml"] {
        let path = root.join(name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            return parse_yaml_packages(&content);
        }
    }
    Vec::new()
}

/// Minimal YAML parser for pnpm-workspace.yaml — extracts the `packages:` list.
fn parse_yaml_packages(content: &str) -> Vec<String> {
    let mut packages = Vec::new();
    let mut in_packages = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if trimmed == "packages:" || trimmed.starts_with("packages:") {
            in_packages = true;
            if let Some(rest) = trimmed.strip_prefix("packages:") {
                let rest = rest.trim();
                if rest.starts_with('[') {
                    return parse_yaml_flow_list(rest);
                }
            }
            continue;
        }
        if in_packages {
            if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.starts_with('-') {
                in_packages = false;
                continue;
            }
            if let Some(val) = trimmed.strip_prefix("- ") {
                let val = val.trim().trim_matches('"').trim_matches('\'');
                packages.push(val.to_string());
            }
        }
    }

    super::glob_to_prefix(packages)
}

fn parse_yaml_flow_list(content: &str) -> Vec<String> {
    let content = content.trim_start_matches('[').trim_end_matches(']');
    let items: Vec<String> = content
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    super::glob_to_prefix(items)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pnpm_detect_via_yaml() {
        let tmp = std::env::temp_dir().join("statico_test_pnpm_detect");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("pnpm-workspace.yaml"),
            r#"packages:
  - "packages/*"
"#,
        )
        .unwrap();
        let profile = PnpmProfile;
        assert!(profile.detect(&tmp, None));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn pnpm_parse_workspaces() {
        let tmp = std::env::temp_dir().join("statico_test_pnpm_ws");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("pnpm-workspace.yaml"),
            r#"packages:
  - "packages/*"
  - "apps/*"
"#,
        )
        .unwrap();
        let profile = PnpmProfile;
        let packages = profile.parse_workspaces(&tmp);
        assert_eq!(packages, vec!["packages/", "apps/"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn pnpm_parse_flow_style() {
        let tmp = std::env::temp_dir().join("statico_test_pnpm_flow");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("pnpm-workspace.yaml"), r#"packages: ["packages/*", "libs/*"]"#).unwrap();
        let profile = PnpmProfile;
        let packages = profile.parse_workspaces(&tmp);
        assert_eq!(packages, vec!["packages/", "libs/"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
