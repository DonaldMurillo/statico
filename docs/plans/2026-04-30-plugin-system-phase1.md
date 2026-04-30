# Phase 1: Core Plugin Infrastructure — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add the plugin subsystem to statico — protocol types, discovery, subprocess management, and CLI commands (`plugin list`, `plugin schema`, `plugin docs`).

**Architecture:** Plugins are subprocesses communicating via newline-delimited JSON-RPC over stdin/stdout. statico discovers them from `.statico/plugins/` and `.statico.toml`, spawns them, performs a handshake to get capabilities, and dispatches pipeline hooks. Phase 1 builds all this infrastructure without yet wiring it into the analysis pipeline (that's Phase 5).

**Tech Stack:** Rust, serde_json, tokio (async subprocess), toml (already in deps)

**Reference:** `docs/plans/2026-04-30-plugin-system-design.md` — the canonical design doc.

---

### Task 1: Plugin Protocol Types

**Files:**
- Create: `src/plugin/mod.rs`
- Create: `src/plugin/protocol.rs`

**Step 1: Create the plugin module with protocol types**

```rust
// src/plugin/mod.rs
pub mod protocol;
pub mod discovery;
pub mod manager;
```

```rust
// src/plugin/protocol.rs
//! JSON-RPC protocol types for the statico plugin system.
//!
//! Every message between statico and a plugin follows the JSON-RPC 2.0 spec.
//! Plugins read newline-delimited JSON from stdin and write to stdout.

use serde::{Deserialize, Serialize};

/// Hook names that plugins can subscribe to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookName {
    AnalyzeFile,
    DiscoverEntries,
    ResolveImport,
    PostAnalysis,
    FormatOutput,
}

/// How a plugin participates in a hook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookMode {
    /// Contribute alongside built-in analysis and other plugins.
    Add,
    /// Replace the built-in stage entirely.
    Override,
}

/// Severity levels for issues reported by plugins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A rule declared by a plugin in its capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub severity: Severity,
    pub description: String,
}

/// A plugin's declared hooks and modes.
pub type HookMap = std::collections::HashMap<HookName, HookMode>;

/// The capabilities response from a plugin's init handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCapabilities {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    pub hooks: HookMap,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// A single issue reported by a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginIssue {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub file: String,
    pub line: usize,
    #[serde(default)]
    pub column: Option<usize>,
    #[serde(default)]
    pub end_line: Option<usize>,
    #[serde(default)]
    pub end_column: Option<usize>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub suggestion: Option<String>,
}

// -- JSON-RPC wrapper types --

/// A JSON-RPC 2.0 request.
#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: &'static str, // always "2.0"
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// A JSON-RPC 2.0 success response.
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub result: serde_json::Value,
}

/// A JSON-RPC 2.0 error response.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub error: RpcError,
}

/// Error details in a JSON-RPC error response.
#[derive(Debug, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

// Standard JSON-RPC error codes
pub const CODE_METHOD_NOT_FOUND: i64 = -32601;
pub const CODE_INVALID_PARAMS: i64 = -32602;
pub const CODE_INTERNAL_ERROR: i64 = -32603;
pub const CODE_PLUGIN_ERROR: i64 = -32000; // custom range

// -- Hook parameter/result types --

#[derive(Debug, Serialize, Deserialize)]
pub struct InitParams {
    pub root: String,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub plugin_settings: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeFileParams {
    pub path: String,
    pub source: String,
    pub language: String,
    #[serde(default)]
    pub existing_issues: Vec<PluginIssue>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AnalyzeFileResult {
    #[serde(default)]
    pub issues: Vec<PluginIssue>,
    #[serde(default)]
    pub exports: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub metrics: Option<PluginMetrics>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginMetrics {
    pub complexity: usize,
    pub loc: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoverEntriesParams {
    pub root: String,
    #[serde(default)]
    pub config_files: Vec<String>,
    #[serde(default)]
    pub language: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DiscoverEntriesResult {
    #[serde(default)]
    pub entry_points: Vec<EntryPoint>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EntryPoint {
    pub path: String,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub framework: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResolveImportParams {
    pub from_file: String,
    pub specifier: String,
    pub root: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResolveImportResult {
    pub resolved_path: String,
    pub external: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PostAnalysisParams {
    pub results: serde_json::Value,
    pub health_score: f64,
    pub total_files: usize,
    #[serde(default)]
    pub language: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PostAnalysisResult {
    #[serde(default)]
    pub issues: Vec<PluginIssue>,
    #[serde(default)]
    pub suggestions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FormatOutputParams {
    pub results: serde_json::Value,
    pub format: String,
    pub health_score: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FormatOutputResult {
    pub output: String,
    #[serde(default)]
    pub exit_code: i32,
}
```

**Step 2: Add `mod plugin` to `src/lib.rs`**

Add `pub mod plugin;` after the existing module declarations in `src/lib.rs`.

**Step 3: Compile and verify**

Run: `cargo check`
Expected: compiles with no errors

**Step 4: Commit**

```bash
git add src/plugin/ src/lib.rs
git commit -m "feat(plugin): protocol types for JSON-RPC plugin system"
```

---

### Task 2: Plugin Discovery

**Files:**
- Create: `src/plugin/discovery.rs`
- Modify: `src/plugin/mod.rs` (add `pub mod discovery;` if not already)

**Step 1: Write discovery tests in `src/plugin/discovery.rs`**

The discovery module scans `.statico/plugins/` for plugin directories/executables and parses `[[plugin]]` entries from `.statico.toml`.

```rust
// Tests to write first:

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_executable_plugin() {
        // Create temp dir with .statico/plugins/my-plugin (executable file)
        // Call discover_plugins(temp_dir)
        // Assert one plugin found with name "my-plugin"
    }

    #[test]
    fn discover_ts_plugin_directory() {
        // Create temp dir with .statico/plugins/my-rule/package.json + index.ts
        // Call discover_plugins(temp_dir)
        // Assert one plugin found with kind PluginKind::TypeScript
    }

    #[test]
    fn discover_rust_plugin_directory() {
        // Create temp dir with .statico/plugins/my-rule/Cargo.toml + src/main.rs
        // Call discover_plugins(temp_dir)
        // Assert one plugin found with kind PluginKind::Rust
    }

    #[test]
    fn config_overrides_auto_discovery() {
        // Create .statico.toml with [[plugin]] name = "my-plugin" enabled = false
        // Create .statico/plugins/my-plugin (executable)
        // Call discover_plugins(temp_dir)
        // Assert plugin is discovered but disabled
    }

    #[test]
    fn detect_override_conflict() {
        // Create two plugins that both override "analyze_file"
        // Call discover_plugins then validate_overrides
        // Assert error returned
    }
}
```

**Step 2: Implement discovery**

```rust
// src/plugin/discovery.rs

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
        if path.join("package.json").exists() {
            PluginKind::TypeScript
        } else if path.join("Cargo.toml").exists() {
            PluginKind::Rust
        } else if let Ok(entries) = std::fs::read_dir(path) {
            // Check if any file in the dir is an executable (not a typical project file)
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.file_name().map(|n| n.to_string_lossy().to_string()) == Some("index.ts".to_string()) {
                    return PluginKind::TypeScript;
                }
                if p.is_file() && p.file_name().map(|n| n.to_string_lossy().to_string()) == Some("main.rs".to_string()) {
                    return PluginKind::Rust;
                }
            }
            PluginKind::Executable // fallback
        } else {
            PluginKind::Executable
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
            if let Some(path) = pc.get("path").and_then(|v| v.as_str()) {
                existing.path = root.join(path);
            }
            if let Some(langs) = pc.get("languages").and_then(|v| v.as_array()) {
                existing.languages = langs.iter().filter_map(|v| v.as_str().map(String::from)).collect();
            }
            if let Some(settings) = pc.get("settings") {
                existing.settings = settings.clone();
            }
        } else {
            // Plugin declared in config but not auto-discovered.
            let path = pc.get("path")
                .and_then(|v| v.as_str())
                .map(|p| root.join(p))
                .unwrap_or_else(|| root.join(format!(".statico/plugins/{}", name)));
            plugins.push(DiscoveredPlugin {
                name,
                path,
                kind: detect_plugin_kind(&path),
                enabled: pc.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                override_all: pc.get("override").and_then(|v| v.as_bool()).unwrap_or(false),
                hook_overrides: HashMap::new(),
                settings: pc.get("settings").cloned().unwrap_or(toml::Value::Table(toml::map::Map::new())),
                languages: pc.get("languages")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
            });
        }
    }
}

/// Check for override conflicts among enabled plugins.
/// Returns Err with a description if two plugins override the same hook.
pub fn validate_overrides(
    initialized: &[(String, crate::plugin::protocol::PluginCapabilities)],
) -> Result<(), String> {
    let mut override_map: HashMap<HookName, String> = HashMap::new();
    for (name, caps) in initialized {
        for (hook, mode) in &caps.hooks {
            if mode == &HookMode::Override {
                if let Some(other) = override_map.get(hook) {
                    return Err(format!(
                        "Plugin conflict: '{}' and '{}' both override hook '{:?}'",
                        other, name, hook
                    ));
                }
                override_map.insert(hook.clone(), name.clone());
            }
        }
    }
    Ok(())
}
```

**Step 3: Run tests**

Run: `cargo test --lib plugin::discovery`
Expected: all tests pass

**Step 4: Commit**

```bash
git add src/plugin/discovery.rs src/plugin/mod.rs
git commit -m "feat(plugin): plugin discovery from .statico/plugins/ and .statico.toml"
```

---

### Task 3: Plugin Subprocess Manager

**Files:**
- Create: `src/plugin/manager.rs`

**Step 1: Write the subprocess manager**

This is the core: spawn a plugin, perform handshake, send/receive JSON-RPC messages, handle shutdown.

```rust
// src/plugin/manager.rs
//! Manages plugin subprocesses — spawn, handshake, message dispatch, shutdown.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::plugin::discovery::{DiscoveredPlugin, PluginKind};
use crate::plugin::protocol::*;

/// A running plugin subprocess with its initialized capabilities.
pub struct ActivePlugin {
    pub name: String,
    pub capabilities: PluginCapabilities,
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl ActivePlugin {
    /// Spawn and initialize a plugin.
    pub fn spawn(plugin: &DiscoveredPlugin, root: &Path) -> Result<Self, String> {
        let (cmd, args) = build_command(plugin)?;
        let mut child = Command::new(&cmd)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // pass through debug logs
            .current_dir(root)
            .spawn()
            .map_err(|e| format!("Failed to spawn plugin '{}': {}", plugin.name, e))?;

        let stdin = child.stdin.take().ok_or("Failed to get plugin stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to get plugin stdout")?;
        let mut stdout = BufReader::new(stdout);

        // Perform handshake.
        let init_params = InitParams {
            root: root.to_string_lossy().to_string(),
            config: serde_json::Value::Object(serde_json::Map::new()),
            plugin_settings: serde_json::Value::Object(serde_json::Map::new()),
        };

        let mut manager = ActivePlugin {
            name: plugin.name.clone(),
            capabilities: PluginCapabilities {
                name: String::new(),
                version: None,
                hooks: HashMap::new(),
                languages: Vec::new(),
                rules: Vec::new(),
            },
            process: child,
            stdin,
            stdout,
            next_id: 1,
        };

        let response: PluginCapabilities = manager.send_request("init", &init_params)?;
        manager.capabilities = response;
        manager.name = plugin.name.clone();

        Ok(manager)
    }

    /// Send a JSON-RPC request and read the response.
    pub fn send_request<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &mut self,
        method: &str,
        params: &T,
    ) -> Result<R, String> {
        let id = self.next_id;
        self.next_id += 1;

        let request = Request {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params: serde_json::to_value(params).unwrap_or(serde_json::Value::Null),
        };

        let mut line = serde_json::to_string(&request).map_err(|e| format!("Serialize error: {}", e))?;
        line.push('\n');

        self.stdin.write_all(line.as_bytes())
            .map_err(|e| format!("Write error to plugin '{}': {}", self.name, e))?;
        self.stdin.flush()
            .map_err(|e| format!("Flush error to plugin '{}': {}", self.name, e))?;

        // Read response.
        let mut response_line = String::new();
        self.stdout.read_line(&mut response_line)
            .map_err(|e| format!("Read error from plugin '{}': {}", self.name, e))?;

        let response_line = response_line.trim();
        if response_line.is_empty() {
            return Err(format!("Plugin '{}' returned empty response", self.name));
        }

        // Try success response first, then error.
        if let Ok(resp) = serde_json::from_str::<GenericResponse>(response_line) {
            if resp.id != id {
                return Err(format!("Plugin '{}' response id mismatch: expected {}, got {}", self.name, id, resp.id));
            }
            let result: R = serde_json::from_value(resp.result)
                .map_err(|e| format!("Plugin '{}' response deserialization error: {} — raw: {}", self.name, e, response_line))?;
            Ok(result)
        } else if let Ok(err_resp) = serde_json::from_str::<ErrorResponse>(response_line) {
            Err(format!("Plugin '{}' error: [{}] {}", self.name, err_resp.error.code, err_resp.error.message))
        } else {
            Err(format!("Plugin '{}' sent invalid JSON-RPC: {}", self.name, response_line))
        }
    }

    /// Send shutdown signal and wait for the process to exit.
    pub fn shutdown(&mut self) -> Result<(), String> {
        let _ = self.send_request::<(), serde_json::Value>("shutdown", &());
        let _ = self.process.wait();
        Ok(())
    }

    /// Check if this plugin subscribes to the given hook.
    pub fn has_hook(&self, hook: &HookName) -> bool {
        self.capabilities.hooks.contains_key(hook)
    }

    /// Get the mode for a hook (defaults to Add if somehow missing).
    pub fn hook_mode(&self, hook: &HookName) -> Option<&HookMode> {
        self.capabilities.hooks.get(hook)
    }
}

impl Drop for ActivePlugin {
    fn drop(&mut self) {
        // Best-effort shutdown.
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// A response where the result is a raw Value (for deserialization later).
#[derive(Debug, Deserialize)]
struct GenericResponse {
    jsonrpc: String,
    id: u64,
    result: serde_json::Value,
}

/// Build the command and args to spawn a plugin.
fn build_command(plugin: &DiscoveredPlugin) -> Result<(String, Vec<String>), String> {
    match plugin.kind {
        PluginKind::Executable => {
            Ok((plugin.path.to_string_lossy().to_string(), vec![]))
        }
        PluginKind::TypeScript => {
            let entry = find_ts_entry(&plugin.path)?;
            Ok(("bun".to_string(), vec![entry]))
        }
        PluginKind::Rust => {
            // Rust plugins must be compiled first. Look for the binary in target/release/.
            let name = plugin.path.file_name()
                .ok_or_else(|| "Plugin path has no file name".to_string())?
                .to_string_lossy();
            let binary = plugin.path.join("target/release").join(name.as_ref());
            if binary.exists() {
                Ok((binary.to_string_lossy().to_string(), vec![]))
            } else {
                Err(format!(
                    "Rust plugin '{}' not compiled. Run 'statico plugin build --name {}' first.",
                    plugin.name, plugin.name
                ))
            }
        }
    }
}

/// Find the TypeScript entry point in a plugin directory.
fn find_ts_entry(dir: &Path) -> Result<String, String> {
    for name in &["index.ts", "src/index.ts", "main.ts", "src/main.ts"] {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    Err(format!("No TypeScript entry point found in {}", dir.display()))
}
```

**Step 2: Write unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Create a mock plugin script that responds to init.
    fn make_echo_plugin(dir: &Path, name: &str) -> PathBuf {
        let script = dir.join(name);
        // A simple bash script that reads JSON-RPC and responds.
        let code = r#"#!/bin/bash
while IFS= read -r line; do
    method=$(echo "$line" | grep -o '"method":"[^"]*"' | cut -d'"' -f4)
    id=$(echo "$line" | grep -o '"id":[0-9]*' | cut -d':' -f2)
    if [ "$method" = "init" ]; then
        echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"name\":\"test-plugin\",\"version\":\"0.1.0\",\"hooks\":{\"analyze_file\":\"add\"},\"languages\":[],\"rules\":[]}}"
    elif [ "$method" = "shutdown" ]; then
        exit 0
    fi
done
"#;
        std::fs::write(&script, code).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        script
    }

    #[test]
    fn spawn_and_handshake() {
        let tmp = tempfile::tempdir().unwrap();
        let script = make_echo_plugin(tmp.path(), "test-plugin");

        let discovered = DiscoveredPlugin {
            name: "test-plugin".to_string(),
            path: script,
            kind: PluginKind::Executable,
            enabled: true,
            override_all: false,
            hook_overrides: HashMap::new(),
            settings: toml::Value::Table(toml::map::Map::new()),
            languages: Vec::new(),
        };

        let mut plugin = ActivePlugin::spawn(&discovered, tmp.path()).unwrap();
        assert_eq!(plugin.capabilities.name, "test-plugin");
        assert!(plugin.has_hook(&HookName::AnalyzeFile));
        plugin.shutdown().ok();
    }
}
```

**Step 3: Run tests**

Run: `cargo test --lib plugin::manager`
Expected: test passes (the bash mock plugin handshakes correctly)

**Step 4: Commit**

```bash
git add src/plugin/manager.rs src/plugin/mod.rs
git commit -m "feat(plugin): subprocess manager with JSON-RPC handshake"
```

---

### Task 4: Extend Config for Plugin Settings

**Files:**
- Modify: `src/config.rs`

**Step 1: Add plugin config to StaticoConfig**

Add an optional `plugins` field and `plugin_auto_discover` field. This doesn't break existing configs (all fields have defaults).

```rust
// Add to StaticoConfig struct:
    /// Disable auto-discovery of plugins in .statico/plugins/
    #[serde(default = "default_true")]
    pub plugin_auto_discover: bool,
    /// Plugin declarations (merged with auto-discovery).
    #[serde(default)]
    pub plugin: Vec<PluginEntry>,

// Add:
fn default_true() -> bool { true }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PluginEntry {
    pub name: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub override: bool,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub settings: toml::Value,
}
```

**Step 2: Add tests for plugin config parsing**

```rust
#[test]
fn test_parse_plugin_config() {
    let c: StaticoConfig = toml::from_str(r#"
format = "json"
plugin_auto_discover = false

[[plugin]]
name = "my-rule"
path = "./plugins/my-rule"
enabled = true
languages = ["typescript"]

[[plugin]]
name = "acme-fork"
override = true
"#).unwrap();
    assert!(!c.plugin_auto_discover);
    assert_eq!(c.plugin.len(), 2);
    assert_eq!(c.plugin[0].name, "my-rule");
    assert_eq!(c.plugin[0].path, Some("./plugins/my-rule".to_string()));
    assert!(c.plugin[0].enabled);
    assert!(c.plugin[1].r#override);
}
```

**Step 3: Run tests**

Run: `cargo test --lib config`
Expected: all tests pass (including existing ones — this is additive)

**Step 4: Commit**

```bash
git add src/config.rs
git commit -m "feat(plugin): extend config with plugin settings and auto-discovery toggle"
```

---

### Task 5: CLI Subcommands — `plugin list`, `plugin schema`, `plugin docs`

**Files:**
- Modify: `src/main.rs`

**Step 1: Add Plugin subcommand to the Commands enum**

```rust
/// Manage statico plugins.
///
/// Discover, build, test, and inspect plugins that extend statico's
/// analysis pipeline.
Plugin {
    #[command(subcommand)]
    action: PluginAction,
},
```

And the PluginAction enum:

```rust
#[derive(clap::Subcommand)]
enum PluginAction {
    /// List discovered plugins and their status.
    List {
        /// Project path (defaults to current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },

    /// Print the JSON schema for the plugin protocol.
    ///
    /// Useful for LLMs and plugin developers to understand the exact contract.
    Schema {
        /// Output format.
        #[arg(long, default_value = "text", value_name = "FORMAT")]
        format: String,
    },

    /// Print the full plugin protocol reference documentation.
    ///
    /// Human-readable guide to building plugins. Covers all hooks,
    /// message types, and lifecycle.
    Docs,
}
```

**Step 2: Implement the handlers**

```rust
fn run_plugin_list(path: &str) {
    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(_) => root.to_path_buf(),
    };

    let plugins = statico::plugin::discovery::discover_plugins(&root);

    if plugins.is_empty() {
        println!("No plugins found in {}", root.display());
        println!();
        println!("Add plugins to .statico/plugins/ or configure them in .statico.toml");
        return;
    }

    println!("Plugins in {}:\n", root.display());
    for p in &plugins {
        let status = if p.enabled { "✓" } else { "✗" };
        let kind = match p.kind {
            statico::plugin::discovery::PluginKind::Executable => "executable",
            statico::plugin::discovery::PluginKind::TypeScript => "typescript",
            statico::plugin::discovery::PluginKind::Rust => "rust",
        };
        println!("  {} {} ({}) — {}", status, p.name, kind, p.path.display());
        if p.override_all {
            println!("    └ override: all hooks");
        }
        if !p.languages.is_empty() {
            println!("    └ languages: {}", p.languages.join(", "));
        }
    }
}

fn run_plugin_schema(format: &str) {
    // Output the full protocol schema.
    match format {
        "json" => {
            let schema = serde_json::json!({
                "protocol": "json-rpc-2.0",
                "transport": "newline-delimited JSON over stdin/stdout",
                "methods": {
                    "init": {
                        "params": { "root": "string", "config": "object", "plugin_settings": "object" },
                        "result": "PluginCapabilities"
                    },
                    "analyze_file": {
                        "params": { "path": "string", "source": "string", "language": "string", "existing_issues": "PluginIssue[]" },
                        "result": { "issues": "PluginIssue[]", "exports": "string[]", "dependencies": "string[]", "metrics": "PluginMetrics?" }
                    },
                    "discover_entries": {
                        "params": { "root": "string", "config_files": "string[]", "language": "string" },
                        "result": { "entry_points": "EntryPoint[]" }
                    },
                    "resolve_import": {
                        "params": { "from_file": "string", "specifier": "string", "root": "string" },
                        "result": { "resolved_path": "string", "external": "bool" }
                    },
                    "post_analysis": {
                        "params": { "results": "object", "health_score": "f64", "total_files": "usize", "language": "string" },
                        "result": { "issues": "PluginIssue[]", "suggestions": "string[]" }
                    },
                    "format_output": {
                        "params": { "results": "object", "format": "string", "health_score": "f64" },
                        "result": { "output": "string", "exit_code": "i32" }
                    },
                    "shutdown": { "params": null, "result": null }
                },
                "types": {
                    "PluginCapabilities": {
                        "name": "string",
                        "version": "string?",
                        "hooks": "Record<HookName, HookMode>",
                        "languages": "string[]",
                        "rules": "Rule[]"
                    },
                    "HookName": "analyze_file | discover_entries | resolve_import | post_analysis | format_output",
                    "HookMode": "add | override",
                    "Severity": "error | warning | info",
                    "Rule": { "id": "string", "severity": "Severity", "description": "string" },
                    "PluginIssue": { "rule_id": "string", "severity": "Severity", "message": "string", "file": "string", "line": "usize", "column": "usize?", "end_line": "usize?", "end_column": "usize?", "confidence": "f64?", "suggestion": "string?" },
                    "EntryPoint": { "path": "string", "type": "string?", "framework": "string?" }
                }
            });
            println!("{}", serde_json::to_string_pretty(&schema).unwrap());
        }
        _ => {
            // Human-readable text format.
            println!("statico Plugin Protocol — JSON-RPC 2.0");
            println!();
            println!("Transport: newline-delimited JSON over stdin/stdout");
            println!("stderr: passed through for debug logging");
            println!();
            println!("HOOKS:");
            println!("  analyze_file    — Per-file analysis [add | override]");
            println!("  discover_entries — Entry point discovery [override only]");
            println!("  resolve_import  — Import resolution [override only]");
            println!("  post_analysis   — After full analysis [add only]");
            println!("  format_output   — Custom output formatting [override only]");
            println!();
            println!("LIFECYCLE:");
            println!("  1. statico spawns plugin subprocess");
            println!("  2. Sends 'init' request with project root");
            println!("  3. Plugin responds with capabilities (name, hooks, rules)");
            println!("  4. statico calls hook methods per the declared capabilities");
            println!("  5. statico sends 'shutdown' — plugin exits");
            println!();
            println!("Run 'statico plugin schema --format json' for machine-readable schema.");
        }
    }
}

fn run_plugin_docs() {
    println!(r#"statico Plugin Development Guide
================================

Overview
--------
Plugins extend statico's analysis pipeline. They are subprocesses that
communicate via newline-delimited JSON-RPC over stdin/stdout.

Quick Start
-----------
  statico plugin init my-rule --lang typescript   # scaffold
  cd .statico/plugins/my-rule
  # edit index.ts
  statico plugin build --name my-rule
  statico plugin run my-rule --file src/foo.ts

Plugin Types
------------
  typescript  — Bun runs .ts entry point (auto-installs Bun if needed)
  rust        — Compiled binary via cargo
  executable  — Any binary/script that speaks the protocol

Configuration (.statico.toml)
-----------------------------
  [[plugin]]
  name = "my-rule"
  path = "./plugins/my-rule"
  enabled = true
  languages = ["typescript"]
  settings = {{ max_complexity = 10 }}

  [[plugin]]
  name = "acme-fork"
  override = true    # replaces ALL hooks it registers

Hook Modes
----------
  add       — contribute alongside built-in analysis
  override  — replace the built-in stage entirely

  Two plugins cannot override the same hook. statico will error.

Protocol Messages
-----------------
Init:
  → {{"method":"init","params":{{"root":"/path/to/project"}}}}
  ← {{"result":{{"name":"my-plugin","hooks":{{"analyze_file":"add"}},"rules":[...]}}}}

Analyze File:
  → {{"method":"analyze_file","params":{{"path":"src/foo.ts","source":"...","language":"typescript"}}}}
  ← {{"result":{{"issues":[...]}}}}

Shutdown:
  → {{"method":"shutdown"}}

Full schema: statico plugin schema --format json
"#);
}
```

**Step 3: Add the Plugin match arm to main()**

```rust
Commands::Plugin { action } => {
    match action {
        PluginAction::List { path } => run_plugin_list(&path),
        PluginAction::Schema { format } => run_plugin_schema(&format),
        PluginAction::Docs => run_plugin_docs(),
    }
}
```

**Step 4: Run tests and verify CLI**

Run: `cargo run -- plugin list` and `cargo run -- plugin docs` and `cargo run -- plugin schema --format json`
Expected: all three produce output without errors

**Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat(plugin): CLI commands — plugin list, schema, docs"
```

---

### Task 6: Integration Tests with Fixture Plugin

**Files:**
- Modify: `tests/integration.rs`

**Step 1: Write integration tests**

Create a bash-based mock plugin that speaks the protocol, and test the full lifecycle:

```rust
#[test]
fn cli_plugin_list_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(statico_bin())
        .args(["plugin", "list", "--path"])
        .arg(tmp.path())
        .output()
        .expect("run plugin list");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No plugins found"));
}

#[test]
fn cli_plugin_list_discovers_executable() {
    let tmp = tempfile::tempdir().unwrap();
    let plugins_dir = tmp.path().join(".statico/plugins");
    std::fs::create_dir_all(&plugins_dir).unwrap();

    // Create a mock executable plugin.
    let plugin_path = plugins_dir.join("my-detector");
    let script = "#!/bin/bash\necho ok\n";
    std::fs::write(&plugin_path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&plugin_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let output = std::process::Command::new(statico_bin())
        .args(["plugin", "list", "--path"])
        .arg(tmp.path())
        .output()
        .expect("run plugin list");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("my-detector"));
    assert!(stdout.contains("executable"));
}

#[test]
fn cli_plugin_schema_json() {
    let output = std::process::Command::new(statico_bin())
        .args(["plugin", "schema", "--format", "json"])
        .output()
        .expect("run plugin schema");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should be valid JSON with protocol info.
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("schema should be valid JSON");
    assert_eq!(parsed["protocol"], "json-rpc-2.0");
    assert!(parsed["methods"].is_object());
}

#[test]
fn cli_plugin_docs() {
    let output = std::process::Command::new(statico_bin())
        .args(["plugin", "docs"])
        .output()
        .expect("run plugin docs");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("JSON-RPC"));
    assert!(stdout.contains("analyze_file"));
}
```

**Step 2: Run tests**

Run: `cargo test --test integration -- cli_plugin`
Expected: all 4 tests pass

**Step 3: Commit**

```bash
git add tests/integration.rs
git commit -m "test(plugin): integration tests for plugin list, schema, docs"
```

---

### Task 7: Build, Install, Verify

**Step 1: Run full test suite**

Run: `cargo test -- --test-threads=1`
Expected: all tests pass (existing + new plugin tests)

**Step 2: Build release and install**

```bash
cargo build --release --bin statico
cp target/release/statico ~/.statico/bin/statico
```

**Step 3: Manual verification**

```bash
statico plugin docs
statico plugin schema --format json | jq .
statico plugin list
```

**Step 4: Final commit**

```bash
git add -A
git commit -m "feat(plugin): Phase 1 complete — core plugin infrastructure"
```
