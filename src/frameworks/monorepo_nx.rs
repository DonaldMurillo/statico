//! Nx monorepo profile.
//!
//! Detects: nx.json or nx/@nrwl/workspace in package.json deps
//! Workspaces: parsed from package.json workspaces or pnpm-workspace.yaml
//! Enhanced: parses nx.json and project.json for deeper project info

use std::collections::HashSet;
use std::path::Path;

use super::MonorepoProfile;

/// Nx monorepo profile.
pub struct NxProfile;

impl MonorepoProfile for NxProfile {
    fn name(&self) -> &'static str {
        "nx"
    }

    fn detect(&self, root: &Path, pkg_deps: Option<&HashSet<String>>) -> bool {
        // nx.json at root
        if root.join("nx.json").exists() {
            return true;
        }
        // nx or @nrwl/workspace in dependencies/devDependencies
        if let Some(deps) = pkg_deps {
            if deps.contains("nx") || deps.contains("@nrwl/workspace") {
                return true;
            }
        }
        false
    }

    fn parse_workspaces(&self, root: &Path) -> Vec<String> {
        // Nx uses npm/yarn/pnpm workspaces under the hood for package dirs.
        // Try package.json workspaces first, then pnpm-workspace.yaml.
        let mut packages = parse_package_json_workspaces(root);
        if packages.is_empty() {
            packages = parse_pnpm_workspaces(root);
        }
        packages
    }
}

// ---------------------------------------------------------------------------
// Workspace parsers
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
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        serde_json::Value::Object(obj) => {
            // yarn workspaces: { packages: [...] }
            if let Some(packages) = obj.get("packages").and_then(|p| p.as_array()) {
                packages
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            } else {
                return Vec::new();
            }
        }
        _ => return Vec::new(),
    };
    super::glob_to_prefix(items)
}

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
// Nx-specific: project.json parsing
// ---------------------------------------------------------------------------

/// Parsed information from a project.json file.
#[derive(Debug, Clone)]
pub struct NxProject {
    /// Project name.
    pub name: String,
    /// Source root directory (e.g., "src").
    pub source_root: Option<String>,
    /// "application" or "library".
    pub project_type: Option<String>,
    /// Tags from the project configuration.
    pub tags: Vec<String>,
    /// Build target main entry point, if configured.
    pub main_entry: Option<String>,
}

/// Parse a project.json file into structured NxProject info.
pub fn parse_project_json(path: &Path) -> Option<NxProject> {
    let content = std::fs::read_to_string(path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&content).ok()?;

    let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let source_root = val.get("sourceRoot").and_then(|v| v.as_str()).map(|s| s.to_string());
    let project_type = val.get("projectType").and_then(|v| v.as_str()).map(|s| s.to_string());

    let tags = val
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    // Extract main entry from targets.build.options.main
    let main_entry = val
        .get("targets")
        .and_then(|t| t.get("build"))
        .and_then(|b| b.get("options"))
        .and_then(|o| o.get("main"))
        .and_then(|m| m.as_str())
        .map(|s| s.to_string());

    Some(NxProject {
        name,
        source_root,
        project_type,
        tags,
        main_entry,
    })
}

/// Parse nx.json for workspace-level configuration.
#[derive(Debug, Clone)]
pub struct NxConfig {
    /// Named inputs defined at the workspace level.
    pub named_inputs: Vec<String>,
}

/// Parse nx.json into structured config.
pub fn parse_nx_json(path: &Path) -> Option<NxConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&content).ok()?;

    let named_inputs = val
        .get("namedInputs")
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    Some(NxConfig { named_inputs })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nx_detect_via_nx_json() {
        let tmp = std::env::temp_dir().join("statico_test_nx_detect");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("nx.json"), r#"{"targetDefaults": {}}"#).unwrap();
        let profile = NxProfile;
        assert!(profile.detect(&tmp, None));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn nx_detect_via_deps() {
        let tmp = std::env::temp_dir().join("statico_test_nx_deps");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let mut deps = HashSet::new();
        deps.insert("nx".to_string());
        let profile = NxProfile;
        assert!(profile.detect(&tmp, Some(&deps)));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn nx_parse_workspaces_from_package_json() {
        let tmp = std::env::temp_dir().join("statico_test_nx_ws");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("package.json"),
            r#"{"devDependencies": {"nx": "19.0.0"}, "workspaces": ["packages/*"]}"#,
        )
        .unwrap();
        let profile = NxProfile;
        let packages = profile.parse_workspaces(&tmp);
        assert_eq!(packages, vec!["packages/"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn nx_parse_project_json() {
        let tmp = std::env::temp_dir().join("statico_test_nx_project");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("project.json"),
            r#"{
                "name": "ui",
                "sourceRoot": "src",
                "projectType": "library",
                "tags": ["scope:ui", "type:lib"],
                "targets": {
                    "build": {
                        "executor": "@nx/js:tsc",
                        "options": {
                            "main": "src/index.ts",
                            "outputPath": "dist/packages/ui"
                        }
                    }
                }
            }"#,
        )
        .unwrap();
        let project = parse_project_json(&tmp.join("project.json")).expect("should parse");
        assert_eq!(project.name, "ui");
        assert_eq!(project.source_root.as_deref(), Some("src"));
        assert_eq!(project.project_type.as_deref(), Some("library"));
        assert_eq!(project.tags, vec!["scope:ui", "type:lib"]);
        assert_eq!(project.main_entry.as_deref(), Some("src/index.ts"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn nx_parse_nx_json() {
        let tmp = std::env::temp_dir().join("statico_test_nx_config");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("nx.json"),
            r#"{
                "namedInputs": {
                    "default": ["{projectRoot}/**/*"],
                    "production": ["!{projectRoot}/**/*.spec.ts"]
                },
                "targetDefaults": {
                    "build": { "dependsOn": ["^build"] }
                }
            }"#,
        )
        .unwrap();
        let config = parse_nx_json(&tmp.join("nx.json")).expect("should parse");
        assert!(config.named_inputs.contains(&"default".to_string()));
        assert!(config.named_inputs.contains(&"production".to_string()));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
