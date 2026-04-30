//! Plugin discovery from .statico/plugins/ and .statico.toml.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::plugin::protocol::{HookMode, HookName};

/// What kind of plugin this is (determines how to run it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginKind {
    /// A standalone executable (binary, shell script, etc.)
    Executable,
    /// A TypeScript plugin directory (has package.json, run with bun)
    TypeScript,
    /// A Rust plugin directory (has Cargo.toml, compile then run)
    Rust,
    /// A Python plugin (has .py entry, run with python3)
    Python,
}

impl std::fmt::Display for PluginKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginKind::Executable => write!(f, "executable"),
            PluginKind::TypeScript => write!(f, "typescript"),
            PluginKind::Rust => write!(f, "rust"),
            PluginKind::Python => write!(f, "python"),
        }
    }
}

/// A discovered but not yet initialized plugin.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    pub name: String,
    pub path: PathBuf,
    pub kind: PluginKind,
    pub enabled: bool,
    /// If set, override ALL hooks this plugin registers.
    pub override_all: bool,
    /// Per-hook mode overrides from config.
    pub hook_overrides: HashMap<HookName, HookMode>,
    /// Custom settings from config.
    pub settings: toml::Value,
    /// Only run on these languages (empty = all).
    pub languages: Vec<String>,
}

/// Discover all plugins for a project root.
///
/// Scans `.statico/plugins/` for auto-discovery, then merges `[[plugin]]`
/// entries from `.statico.toml`.
pub fn discover_plugins(root: &Path) -> Vec<DiscoveredPlugin> {
    let mut plugins = Vec::new();
    let plugins_dir = root.join(".statico/plugins");

    // Auto-discover from directory.
    if plugins_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) {
                    if name.starts_with('.') || name.starts_with('_') {
                        continue; // skip hidden/temp files
                    }
                    let kind = detect_plugin_kind(&path);
                    plugins.push(DiscoveredPlugin {
                        name,
                        path,
                        kind,
                        enabled: true,
                        override_all: false,
                        hook_overrides: HashMap::new(),
                        settings: toml::Value::Table(toml::map::Map::new()),
                        languages: Vec::new(),
                    });
                }
            }
        }
    }

    // Merge config from .statico.toml.
    merge_config(root, &mut plugins);

    plugins
}

/// Detect what kind of plugin a path is.
fn detect_plugin_kind(path: &Path) -> PluginKind {
    if path.is_file() {
        PluginKind::Executable
    } else if path.is_dir() {
        // Check package.json for statico.runtime hint first.
        let pkg_path = path.join("package.json");
        if pkg_path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&pkg_path) {
                if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&contents) {
                    if let Some(runtime) = pkg
                        .get("statico")
                        .and_then(|s| s.get("runtime"))
                        .and_then(|r| r.as_str())
                    {
                        return match runtime {
                            "python3" | "python" => PluginKind::Python,
                            "bun" | "typescript" => PluginKind::TypeScript,
                            "rust" | "cargo" => PluginKind::Rust,
                            _ => PluginKind::Executable,
                        };
                    }
                }
            }
            // Default: package.json without statico.runtime = TypeScript
            PluginKind::TypeScript
        } else if path.join("Cargo.toml").exists() {
            PluginKind::Rust
        } else {
            // Check for common entry files.
            let has_index_ts = path.join("index.ts").exists() || path.join("src/index.ts").exists();
            let has_main_py = path.join("main.py").exists() || path.join("plugin.py").exists();
            let has_main_rs = path.join("main.rs").exists() || path.join("src/main.rs").exists();
            if has_index_ts {
                PluginKind::TypeScript
            } else if has_main_py {
                PluginKind::Python
            } else if has_main_rs {
                PluginKind::Rust
            } else {
                PluginKind::Executable
            }
        }
    } else {
        PluginKind::Executable
    }
}

/// Merge plugin config from .statico.toml into discovered plugins.
fn merge_config(root: &Path, plugins: &mut Vec<DiscoveredPlugin>) {
    let config_path = root.join(".statico.toml");
    if !config_path.exists() {
        return;
    }
    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let config: toml::Value = match toml::from_str(&content) {
        Ok(c) => c,
        Err(_) => return,
    };

    let plugin_configs = match config.get("plugin").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return,
    };

    for pc in plugin_configs {
        let name = match pc.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if let Some(existing) = plugins.iter_mut().find(|p| p.name == name) {
            // Merge into auto-discovered plugin.
            if let Some(enabled) = pc.get("enabled").and_then(|v| v.as_bool()) {
                existing.enabled = enabled;
            }
            if let Some(override_all) = pc.get("override").and_then(|v| v.as_bool()) {
                existing.override_all = override_all;
            }
            if let Some(p) = pc.get("path").and_then(|v| v.as_str()) {
                let resolved = root.join(p);
                if let Err(e) = crate::ensure_within_root(&resolved, root) {
                    eprintln!("warning: skipping plugin '{}': {}", name, e);
                    continue;
                }
                existing.path = resolved;
                existing.kind = detect_plugin_kind(&existing.path);
            }
            if let Some(langs) = pc.get("languages").and_then(|v| v.as_array()) {
                existing.languages =
                    langs.iter().filter_map(|v| v.as_str().map(String::from)).collect();
            }
            if let Some(settings) = pc.get("settings") {
                existing.settings = settings.clone();
            }
        } else {
            // Plugin declared in config but not auto-discovered.
            let path = pc
                .get("path")
                .and_then(|v| v.as_str())
                .map(|p| root.join(p))
                .unwrap_or_else(|| root.join(format!(".statico/plugins/{}", name)));
            if let Err(e) = crate::ensure_within_root(&path, root) {
                eprintln!("warning: skipping plugin '{}': {}", name, e);
                continue;
            }
            let kind = if path.exists() { detect_plugin_kind(&path) } else { PluginKind::Executable };
            plugins.push(DiscoveredPlugin {
                name,
                path,
                kind,
                enabled: pc.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                override_all: pc.get("override").and_then(|v| v.as_bool()).unwrap_or(false),
                hook_overrides: HashMap::new(),
                settings: pc
                    .get("settings")
                    .cloned()
                    .unwrap_or(toml::Value::Table(toml::map::Map::new())),
                languages: pc
                    .get("languages")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
            });
        }
    }
}

/// Check for override conflicts among initialized plugins.
///
/// Returns `Err` with a description if two plugins override the same hook.
pub fn validate_overrides(
    initialized: &[(String, crate::plugin::protocol::PluginCapabilities)],
) -> Result<(), String> {
    let mut override_map: HashMap<HookName, String> = HashMap::new();
    for (name, caps) in initialized {
        for (hook, mode) in &caps.hooks {
            if mode == &HookMode::Override {
                if let Some(other) = override_map.get(hook) {
                    return Err(format!(
                        "Plugin conflict: '{}' and '{}' both override hook '{}'",
                        other,
                        name,
                        serde_json::to_value(hook)
                            .ok()
                            .and_then(|v| v.as_str().map(String::from))
                            .unwrap_or_default()
                    ));
                }
                override_map.insert(hook.clone(), name.clone());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("statico_plugin_test_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_executable(path: &Path) {
        std::fs::write(path, "#!/bin/bash\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn discover_executable_plugin() {
        let tmp = make_temp_dir("exec");
        let plugins_dir = tmp.join(".statico/plugins");
        std::fs::create_dir_all(&plugins_dir).unwrap();
        make_executable(&plugins_dir.join("my-plugin"));

        let plugins = discover_plugins(&tmp);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "my-plugin");
        assert_eq!(plugins[0].kind, PluginKind::Executable);
        assert!(plugins[0].enabled);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn discover_ts_plugin_directory() {
        let tmp = make_temp_dir("ts");
        let dir = tmp.join(".statico/plugins/my-rule");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        std::fs::write(dir.join("index.ts"), "").unwrap();

        let plugins = discover_plugins(&tmp);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "my-rule");
        assert_eq!(plugins[0].kind, PluginKind::TypeScript);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn discover_rust_plugin_directory() {
        let tmp = make_temp_dir("rust");
        let dir = tmp.join(".statico/plugins/my-rule");
        std::fs::create_dir_all(&dir.join("src")).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname=\"my-rule\"\n").unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main(){}").unwrap();

        let plugins = discover_plugins(&tmp);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].kind, PluginKind::Rust);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn skip_hidden_and_temp_files() {
        let tmp = make_temp_dir("hidden");
        let plugins_dir = tmp.join(".statico/plugins");
        std::fs::create_dir_all(&plugins_dir).unwrap();
        make_executable(&plugins_dir.join(".hidden"));
        make_executable(&plugins_dir.join("_temp"));
        make_executable(&plugins_dir.join("real-plugin"));

        let plugins = discover_plugins(&tmp);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "real-plugin");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn config_disables_plugin() {
        let tmp = make_temp_dir("disable");
        let plugins_dir = tmp.join(".statico/plugins");
        std::fs::create_dir_all(&plugins_dir).unwrap();
        make_executable(&plugins_dir.join("my-plugin"));

        std::fs::write(
            tmp.join(".statico.toml"),
            r#"[[plugin]]
name = "my-plugin"
enabled = false
"#,
        )
        .unwrap();

        let plugins = discover_plugins(&tmp);
        assert_eq!(plugins.len(), 1);
        assert!(!plugins[0].enabled);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn config_adds_new_plugin() {
        let tmp = make_temp_dir("config_add");
        // No .statico/plugins/ directory — plugin only in config.
        std::fs::write(
            tmp.join(".statico.toml"),
            r#"[[plugin]]
name = "custom-path"
path = "./tools/custom-plugin"
override = true
languages = ["typescript"]
"#,
        )
        .unwrap();

        let plugins = discover_plugins(&tmp);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "custom-path");
        assert!(plugins[0].override_all);
        assert_eq!(plugins[0].languages, vec!["typescript"]);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn validate_overrides_no_conflict() {
        let caps1 = crate::plugin::protocol::PluginCapabilities {
            name: "a".to_string(),
            version: None,
            hooks: HashMap::from([(HookName::AnalyzeFile, HookMode::Add)]),
            languages: vec![],
            rules: vec![],
        };
        let caps2 = crate::plugin::protocol::PluginCapabilities {
            name: "b".to_string(),
            version: None,
            hooks: HashMap::from([(HookName::PostAnalysis, HookMode::Override)]),
            languages: vec![],
            rules: vec![],
        };
        assert!(validate_overrides(&[("a".to_string(), caps1), ("b".to_string(), caps2)]).is_ok());
    }

    #[test]
    fn validate_overrides_conflict_detected() {
        let caps1 = crate::plugin::protocol::PluginCapabilities {
            name: "a".to_string(),
            version: None,
            hooks: HashMap::from([(HookName::AnalyzeFile, HookMode::Override)]),
            languages: vec![],
            rules: vec![],
        };
        let caps2 = crate::plugin::protocol::PluginCapabilities {
            name: "b".to_string(),
            version: None,
            hooks: HashMap::from([(HookName::AnalyzeFile, HookMode::Override)]),
            languages: vec![],
            rules: vec![],
        };
        let result = validate_overrides(&[("a".to_string(), caps1), ("b".to_string(), caps2)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("conflict"));
    }
}
