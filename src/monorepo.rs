//! Monorepo and workspace detection.
//!
//! Detects pnpm workspaces, npm/yarn workspaces, Nx monorepos, and Turborepo
//! setups. Returns structured workspace information that the analyzer uses to
//! discover packages and treat workspace-internal imports correctly.

use std::path::{Path, PathBuf};

/// Describes the monorepo setup detected in a project.
#[derive(Debug, Clone)]
pub struct MonorepoInfo {
    /// The kind of monorepo tool detected.
    pub kind: MonorepoKind,
    /// Root-relative paths to each workspace package directory.
    pub packages: Vec<String>,
}

/// The type of monorepo tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonorepoKind {
    /// pnpm workspaces (pnpm-workspace.yaml).
    Pnpm,
    /// npm/yarn workspaces (package.json "workspaces" field).
    Npm,
    /// Nx monorepo (nx.json or nx in package.json).
    Nx,
    /// Turborepo (turbo.json).
    Turborepo,
}

impl std::fmt::Display for MonorepoKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pnpm => write!(f, "pnpm"),
            Self::Npm => write!(f, "npm/yarn"),
            Self::Nx => write!(f, "nx"),
            Self::Turborepo => write!(f, "turborepo"),
        }
    }
}

/// Detect monorepo configuration at the project root.
/// Returns `None` if no monorepo setup is found.
pub fn detect_monorepo(root: &Path) -> Option<MonorepoInfo> {
    // Priority order: pnpm > turbo > nx > npm (most specific first).

    // 1. pnpm workspaces — pnpm-workspace.yaml
    if root.join("pnpm-workspace.yaml").exists() || root.join("pnpm-workspace.yml").exists() {
        let packages = parse_pnpm_workspaces(root);
        return Some(MonorepoInfo { kind: MonorepoKind::Pnpm, packages });
    }

    // 2. Turborepo — turbo.json
    if root.join("turbo.json").exists() {
        let packages = parse_package_json_workspaces(root);
        // Also check for Nx integration.
        return Some(MonorepoInfo { kind: MonorepoKind::Turborepo, packages });
    }

    // 3. Nx — nx.json or "nx" in package.json
    if root.join("nx.json").exists() || has_nx_in_package_json(root) {
        let packages = parse_nx_workspaces(root);
        return Some(MonorepoInfo { kind: MonorepoKind::Nx, packages });
    }

    // 4. npm/yarn workspaces — package.json "workspaces" field
    if let Some(packages) = detect_npm_workspaces(root) {
        return Some(packages);
    }

    None
}

/// Check if a path is inside a known workspace package directory.
pub fn is_workspace_package_file(rel: &str, packages: &[String]) -> bool {
    for pkg in packages {
        if rel.starts_with(pkg.as_str()) {
            return true;
        }
    }
    false
}

/// Given a monorepo root, find the package.json directories that are
/// workspace members.
pub fn discover_workspace_roots(root: &Path, packages: &[String]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for pkg_pattern in packages {
        // Reject patterns with path traversal.
        if pkg_pattern.split(['/', '\\']).any(|c| c == "..") {
            continue;
        }
        if pkg_pattern.starts_with('/') {
            continue;
        }
        if pkg_pattern.ends_with('/') {
            // Directory prefix like "packages/" — enumerate subdirs with package.json.
            let dir = root.join(pkg_pattern.trim_end_matches('/'));
            if dir.is_dir()
                && let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        if entry.path().is_dir() && entry.path().join("package.json").exists() {
                            roots.push(entry.path());
                        }
                    }
                }
        } else if pkg_pattern.contains('*') {
            // Handle glob patterns like "packages/*".
            if let Some(parent) = pkg_pattern.trim_end_matches("/*").strip_suffix('*') {
                let parent = parent.trim_end_matches('/');
                let parent_dir = root.join(parent);
                if parent_dir.is_dir()
                    && let Ok(entries) = std::fs::read_dir(&parent_dir) {
                        for entry in entries.flatten() {
                            if entry.path().is_dir() && entry.path().join("package.json").exists() {
                                roots.push(entry.path());
                            }
                        }
                    }
            }
        } else {
            let pkg_dir = root.join(pkg_pattern);
            if pkg_dir.join("package.json").exists() {
                roots.push(pkg_dir);
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
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
            // Handle inline: packages: ["packages/*"]
            if let Some(rest) = trimmed.strip_prefix("packages:") {
                let rest = rest.trim();
                if rest.starts_with('[') {
                    return parse_yaml_flow_list(rest);
                }
            }
            continue;
        }
        if in_packages {
            // If we hit another top-level key, stop.
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

    glob_to_prefix(packages)
}

fn parse_yaml_flow_list(content: &str) -> Vec<String> {
    let content = content.trim_start_matches('[').trim_end_matches(']');
    let items: Vec<String> = content
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    glob_to_prefix(items)
}

fn detect_npm_workspaces(root: &Path) -> Option<MonorepoInfo> {
    let content = std::fs::read_to_string(root.join("package.json")).ok()?;
    let pkg: serde_json::Value = serde_json::from_str(&content).ok()?;
    let ws = pkg.get("workspaces")?;

    let packages = match ws {
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            glob_to_prefix(items)
        }
        serde_json::Value::Object(obj) => {
            // yarn workspaces: { packages: [...] }
            if let Some(packages) = obj.get("packages").and_then(|p| p.as_array()) {
                let items: Vec<String> = packages.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                glob_to_prefix(items)
            } else {
                Vec::new()
            }
        }
        _ => return None,
    };

    Some(MonorepoInfo { kind: MonorepoKind::Npm, packages })
}

fn parse_package_json_workspaces(root: &Path) -> Vec<String> {
    detect_npm_workspaces(root).map(|info| info.packages).unwrap_or_default()
}

fn has_nx_in_package_json(root: &Path) -> bool {
    let content = match std::fs::read_to_string(root.join("package.json")) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let pkg: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };
    // Check devDependencies or dependencies for nx.
    for field in &["devDependencies", "dependencies"] {
        if let Some(deps) = pkg.get(field).and_then(|v| v.as_object())
            && (deps.contains_key("nx") || deps.contains_key("@nrwl/workspace")) {
                return true;
            }
    }
    false
}

fn parse_nx_workspaces(root: &Path) -> Vec<String> {
    // Nx uses npm/yarn/pnpm workspaces under the hood for package dirs.
    // Also check nx.json for explicit configuration.
    let mut packages = parse_package_json_workspaces(root);

    // Also try pnpm workspaces.
    if packages.is_empty() {
        packages = parse_pnpm_workspaces(root);
    }

    packages
}

/// Convert glob patterns like "packages/*" to directory prefixes like "packages/".
/// V7-6: Also handles double-star patterns like "packages/**" and "packages/**/"
/// which should become "packages/".
fn glob_to_prefix(patterns: Vec<String>) -> Vec<String> {
    patterns
        .into_iter()
        .map(|p| {
            if p.ends_with("/*") {
                // "packages/*" → "packages/"
                format!("{}/", &p[..p.len() - 2])
            } else if p.ends_with("/**/") {
                // "packages/**/" → "packages/"
                format!("{}/", &p[..p.len() - 4])
            } else if p.ends_with("/**") {
                // "packages/**" → "packages/"
                format!("{}/", &p[..p.len() - 3])
            } else {
                p
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_yaml_packages_block() {
        let yaml = r#"
packages:
  - "packages/*"
  - "apps/*"
"#;
        let result = parse_yaml_packages(yaml);
        assert_eq!(result, vec!["packages/", "apps/"]);
    }

    #[test]
    fn test_parse_yaml_packages_flow() {
        let yaml = r#"packages: ["packages/*", "libs/*"]"#;
        let result = parse_yaml_packages(yaml);
        assert_eq!(result, vec!["packages/", "libs/"]);
    }

    #[test]
    fn test_glob_to_prefix() {
        let result = glob_to_prefix(vec!["packages/*".to_string(), "libs/*".to_string(), "apps/admin".to_string()]);
        assert_eq!(result, vec!["packages/", "libs/", "apps/admin"]);
    }

    // ── V7-6: glob_to_prefix must handle double-star patterns ──
    #[test]
    fn sec_monorepo_glob_to_prefix_double_star() {
        let result = glob_to_prefix(vec![
            "packages/*".to_string(),
            "libs/**".to_string(),
            "apps/**/".to_string(),
            "tools/admin".to_string(),
        ]);
        assert_eq!(result, vec!["packages/", "libs/", "apps/", "tools/admin"],
            "double-star patterns should normalize to directory prefixes, got: {:?}", result);
    }

    #[test]
    fn test_detect_monorepo_none() {
        let tmp = std::env::temp_dir().join("statico_test_no_monorepo");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(detect_monorepo(&tmp).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_pnpm_workspace() {
        let tmp = std::env::temp_dir().join("statico_test_pnpm");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("pnpm-workspace.yaml"),
            r#"packages:
  - "packages/*"
"#,
        )
        .unwrap();
        let info = detect_monorepo(&tmp).expect("should detect pnpm");
        assert_eq!(info.kind, MonorepoKind::Pnpm);
        assert_eq!(info.packages, vec!["packages/"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_npm_workspace() {
        let tmp = std::env::temp_dir().join("statico_test_npm");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("package.json"), r#"{"workspaces": ["packages/*"]}"#).unwrap();
        let info = detect_monorepo(&tmp).expect("should detect npm");
        assert_eq!(info.kind, MonorepoKind::Npm);
        assert_eq!(info.packages, vec!["packages/"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_turborepo() {
        let tmp = std::env::temp_dir().join("statico_test_turbo");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("turbo.json"), r#"{"$schema": "https://turbo.build/schema.json"}"#).unwrap();
        let info = detect_monorepo(&tmp).expect("should detect turborepo");
        assert_eq!(info.kind, MonorepoKind::Turborepo);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_nx_via_nx_json() {
        let tmp = std::env::temp_dir().join("statico_test_nx");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("nx.json"), r#"{"targetDefaults": {}}"#).unwrap();
        std::fs::write(
            tmp.join("package.json"),
            r#"{"devDependencies": {"nx": "19.0.0"}, "workspaces": ["packages/*"]}"#,
        )
        .unwrap();
        let info = detect_monorepo(&tmp).expect("should detect nx");
        assert_eq!(info.kind, MonorepoKind::Nx);
        assert_eq!(info.packages, vec!["packages/"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_is_workspace_package_file() {
        let packages = vec!["packages/".to_string(), "apps/".to_string()];
        assert!(is_workspace_package_file("packages/ui/src/button.tsx", &packages));
        assert!(is_workspace_package_file("apps/web/src/page.tsx", &packages));
        assert!(!is_workspace_package_file("src/index.ts", &packages));
    }

    #[test]
    fn test_discover_workspace_roots() {
        let tmp = std::env::temp_dir().join("statico_test_ws_roots");
        let _ = std::fs::remove_dir_all(&tmp);
        // Create packages/ui/ and packages/core/ with package.json.
        std::fs::create_dir_all(tmp.join("packages/ui")).unwrap();
        std::fs::create_dir_all(tmp.join("packages/core")).unwrap();
        std::fs::create_dir_all(tmp.join("packages/empty")).unwrap();
        std::fs::write(tmp.join("packages/ui/package.json"), "{}").unwrap();
        std::fs::write(tmp.join("packages/core/package.json"), "{}").unwrap();
        // empty dir has no package.json — should be skipped.

        let roots = discover_workspace_roots(&tmp, &["packages/".to_string()]);
        assert_eq!(roots.len(), 2);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sec_monorepo_workspace_roots_reject_traversal() {
        let tmp = std::env::temp_dir().join("statico_sec_ws_traversal");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let roots = discover_workspace_roots(&tmp, &["../../etc".to_string()]);
        assert!(roots.is_empty(),
            "traversal pattern should produce no roots: {:?}", roots);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sec_monorepo_workspace_roots_reject_absolute() {
        let tmp = std::env::temp_dir().join("statico_sec_ws_absolute");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let roots = discover_workspace_roots(&tmp, &["/etc".to_string()]);
        assert!(roots.is_empty(),
            "absolute pattern should produce no roots: {:?}", roots);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
