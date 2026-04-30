//! Plugin subprocess manager — spawn, handshake, message dispatch, shutdown.
//!
//! Security hardening (audit F-01, F-02, F-05):
//! - 10MB response size limit prevents OOM from malicious plugins
//! - 30s read timeout prevents blocking on unresponsive plugins
//! - Error messages truncate raw responses to 200 chars

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::plugin::discovery::{DiscoveredPlugin, PluginKind};
use crate::plugin::protocol::*;

/// Maximum bytes to read from a plugin response (10 MB) — prevents OOM (F-01).
const MAX_RESPONSE_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum time to wait for a plugin response — prevents hangs (F-02).
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum chars of raw response to include in error messages (F-05).
const MAX_ERROR_RAW_LEN: usize = 200;

/// A running plugin subprocess with its initialized capabilities.
pub struct ActivePlugin {
    pub name: String,
    pub capabilities: PluginCapabilities,
    process: Child,
    stdin: ChildStdin,
    // Wrapped in Option so we can temporarily move it to a read thread.
    stdout: Option<BufReader<std::process::ChildStdout>>,
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
            .stderr(Stdio::inherit())
            .current_dir(root)
            .spawn()
            .map_err(|e| format!("Failed to spawn plugin '{}': {}", plugin.name, e))?;

        let stdin = child.stdin.take().ok_or("Failed to get plugin stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to get plugin stdout")?;

        let init_params = InitParams {
            root: root.to_string_lossy().to_string(),
            config: serde_json::Value::Object(serde_json::Map::new()),
            plugin_settings: toml_to_json_value(&plugin.settings),
        };

        let mut active = ActivePlugin {
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
            stdout: Some(BufReader::new(stdout)),
            next_id: 1,
        };

        let caps: PluginCapabilities = active.send_request("init", &init_params)?;
        let caps = if plugin.override_all {
            PluginCapabilities {
                hooks: caps
                    .hooks
                    .into_keys()
                    .map(|k| (k, HookMode::Override))
                    .collect(),
                ..caps
            }
        } else {
            caps
        };

        active.capabilities = caps;
        active.name = plugin.name.clone();
        Ok(active)
    }

    /// Send a JSON-RPC request and read the response.
    ///
    /// Spawns a background thread that reads with a 10MB limit.
    /// If the read doesn't complete within 30s, kills the child process
    /// and returns an error.
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

        let mut line =
            serde_json::to_string(&request).map_err(|e| format!("Serialize error: {}", e))?;
        line.push('\n');

        self.stdin
            .write_all(line.as_bytes())
            .map_err(|e| format!("Write error to plugin '{}': {}", self.name, e))?;
        self.stdin
            .flush()
            .map_err(|e| format!("Flush error to plugin '{}': {}", self.name, e))?;

        // Take stdout and move to a read thread for bounded + timed read.
        let mut stdout = self.stdout.take().expect("stdout already taken (concurrent send_request?)");
        let name = self.name.clone();
        let (tx, rx) = mpsc::channel();
        let read_thread = std::thread::spawn(move || {
            let mut response_line = String::new();
            let result = (&mut stdout).take(MAX_RESPONSE_SIZE).read_line(&mut response_line);
            let _ = tx.send((result, response_line));
            stdout // Return ownership
        });

        match rx.recv_timeout(RESPONSE_TIMEOUT) {
            Ok((read_result, response_line)) => {
                // Recover stdout from thread
                self.stdout = Some(read_thread.join().expect("plugin read thread panicked"));

                let bytes_read = read_result
                    .map_err(|e| format!("Read error from plugin '{}': {}", name, e))?;
                if bytes_read == 0 && response_line.is_empty() {
                    return Err(format!("Plugin '{}' returned empty response (EOF)", name));
                }
                if response_line.len() as u64 >= MAX_RESPONSE_SIZE {
                    return Err(format!("Plugin '{}' response exceeded 10MB limit", name));
                }

                parse_response::<R>(&name, id, &response_line)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = self.process.kill();
                Err(format!(
                    "Plugin '{}' timed out waiting for response (>{}s)",
                    name,
                    RESPONSE_TIMEOUT.as_secs()
                ))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(format!("Plugin '{}' reader thread panicked", name))
            }
        }
    }

    /// Send shutdown signal and wait for the process to exit.
    pub fn shutdown(&mut self) -> Result<(), String> {
        let id = self.next_id;
        self.next_id += 1;
        let request = Request {
            jsonrpc: "2.0",
            id,
            method: "shutdown".to_string(),
            params: serde_json::Value::Null,
        };
        if let Ok(mut line) = serde_json::to_string(&request) {
            line.push('\n');
            let _ = self.stdin.write_all(line.as_bytes());
            let _ = self.stdin.flush();
        }
        std::thread::sleep(Duration::from_millis(100));
        let _ = self.process.wait();
        Ok(())
    }

    /// Check if this plugin subscribes to the given hook.
    pub fn has_hook(&self, hook: &HookName) -> bool {
        self.capabilities.hooks.contains_key(hook)
    }

    /// Get the mode for a hook.
    pub fn hook_mode(&self, hook: &HookName) -> Option<&HookMode> {
        self.capabilities.hooks.get(hook)
    }
}

/// Parse a JSON-RPC response, truncating raw output in error messages (F-05).
fn parse_response<R: serde::de::DeserializeOwned>(
    name: &str,
    expected_id: u64,
    raw: &str,
) -> Result<R, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("Plugin '{}' returned empty response", name));
    }

    let truncated = truncate_str(trimmed, MAX_ERROR_RAW_LEN);

    if let Ok(resp) = serde_json::from_str::<GenericResponse>(trimmed) {
        if resp.id != expected_id {
            return Err(format!(
                "Plugin '{}' response id mismatch: expected {}, got {}",
                name, expected_id, resp.id
            ));
        }
        serde_json::from_value(resp.result).map_err(|e| {
            format!(
                "Plugin '{}' response deserialization error: {} — raw: {}…",
                name, e, truncated
            )
        })
    } else if let Ok(err_resp) = serde_json::from_str::<ErrorResponse>(trimmed) {
        Err(format!(
            "Plugin '{}' error: [{}] {}",
            name, err_resp.error.code, err_resp.error.message
        ))
    } else {
        Err(format!(
            "Plugin '{}' sent invalid JSON-RPC: {}…",
            name, truncated
        ))
    }
}

/// Truncate a string at a char boundary near `max_len`.
fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}

impl Drop for ActivePlugin {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// Build the command and args to spawn a plugin.
fn build_command(plugin: &DiscoveredPlugin) -> Result<(String, Vec<String>), String> {
    match plugin.kind {
        PluginKind::Executable => {
            if !plugin.path.exists() {
                return Err(format!(
                    "Plugin executable not found: {}",
                    plugin.path.display()
                ));
            }
            Ok((plugin.path.to_string_lossy().to_string(), vec![]))
        }
        PluginKind::TypeScript => {
            let entry = find_ts_entry(&plugin.path)?;
            let bun = crate::plugin::runtime::ensure_bun()
                .map_err(|e| format!("Bun runtime unavailable: {}", e))?;
            Ok((bun.to_string_lossy().to_string(), vec![entry]))
        }
        PluginKind::Rust => {
            let name = plugin
                .path
                .file_name()
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
        PluginKind::Python => {
            let entry = find_python_entry(&plugin.path)?;
            Ok(("python3".to_string(), vec![entry]))
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
    Err(format!(
        "No TypeScript entry point found in {}",
        dir.display()
    ))
}

/// Find the Python entry point in a plugin directory.
fn find_python_entry(dir: &Path) -> Result<String, String> {
    let pkg_path = dir.join("package.json");
    if pkg_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&pkg_path) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&contents) {
                if let Some(entry) = pkg
                    .get("statico")
                    .and_then(|s| s.get("entry"))
                    .and_then(|e| e.as_str())
                {
                    let candidate = dir.join(entry);
                    // Verify entry path stays within plugin directory (F-04).
                    if !candidate.starts_with(dir) {
                        return Err(format!("entry path '{}' escapes plugin directory", entry));
                    }
                    if candidate.exists() {
                        return Ok(candidate.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    for name in &["plugin.py", "main.py", "index.py", "src/main.py"] {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    Err(format!(
        "No Python entry point found in {}",
        dir.display()
    ))
}

/// Convert toml::Value to serde_json::Value.
fn toml_to_json_value(val: &toml::Value) -> serde_json::Value {
    match val {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::Value::Number((*i).into()),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(toml_to_json_value).collect())
        }
        toml::Value::Table(tbl) => serde_json::Value::Object(
            tbl.iter().map(|(k, v)| (k.clone(), toml_to_json_value(v))).collect(),
        ),
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_mock_plugin(dir: &Path, name: &str) -> PathBuf {
        let script = dir.join(name);
        let code = r#"#!/bin/bash
while IFS= read -r line; do
    id=$(echo "$line" | grep -o '"id":[0-9]*' | head -1 | cut -d':' -f2)
    if echo "$line" | grep -q '"method":"init"'; then
        echo "{\"jsonrpc\":\"2.0\",\"id\":${id},\"result\":{\"name\":\"mock-plugin\",\"version\":\"0.1.0\",\"hooks\":{\"analyze_file\":\"add\"},\"languages\":[],\"rules\":[]}}"
    elif echo "$line" | grep -q '"method":"shutdown"'; then
        exit 0
    elif echo "$line" | grep -q '"method":"analyze_file"'; then
        echo "{\"jsonrpc\":\"2.0\",\"id\":${id},\"result\":{\"issues\":[],\"exports\":[],\"dependencies\":[]}}"
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
        let script = make_mock_plugin(tmp.path(), "mock-plugin");
        let discovered = DiscoveredPlugin {
            name: "mock-plugin".to_string(),
            path: script,
            kind: PluginKind::Executable,
            enabled: true,
            override_all: false,
            hook_overrides: HashMap::new(),
            settings: toml::Value::Table(toml::map::Map::new()),
            languages: Vec::new(),
        };
        let mut plugin = ActivePlugin::spawn(&discovered, tmp.path()).unwrap();
        assert_eq!(plugin.capabilities.name, "mock-plugin");
        assert!(plugin.has_hook(&HookName::AnalyzeFile));
        assert!(!plugin.has_hook(&HookName::PostAnalysis));
        plugin.shutdown().ok();
    }

    #[test]
    fn spawn_with_override_all() {
        let tmp = tempfile::tempdir().unwrap();
        let script = make_mock_plugin(tmp.path(), "override-plugin");
        let discovered = DiscoveredPlugin {
            name: "override-plugin".to_string(),
            path: script,
            kind: PluginKind::Executable,
            enabled: true,
            override_all: true,
            hook_overrides: HashMap::new(),
            settings: toml::Value::Table(toml::map::Map::new()),
            languages: Vec::new(),
        };
        let mut plugin = ActivePlugin::spawn(&discovered, tmp.path()).unwrap();
        assert_eq!(
            plugin.hook_mode(&HookName::AnalyzeFile),
            Some(&HookMode::Override)
        );
        plugin.shutdown().ok();
    }

    #[test]
    fn analyze_file_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let script = make_mock_plugin(tmp.path(), "analyze-plugin");
        let discovered = DiscoveredPlugin {
            name: "analyze-plugin".to_string(),
            path: script,
            kind: PluginKind::Executable,
            enabled: true,
            override_all: false,
            hook_overrides: HashMap::new(),
            settings: toml::Value::Table(toml::map::Map::new()),
            languages: Vec::new(),
        };
        let mut plugin = ActivePlugin::spawn(&discovered, tmp.path()).unwrap();
        let params = AnalyzeFileParams {
            path: "test.ts".to_string(),
            source: "console.log('hi')".to_string(),
            language: "typescript".to_string(),
            existing_issues: vec![],
        };
        let result: AnalyzeFileResult = plugin.send_request("analyze_file", &params).unwrap();
        assert!(result.issues.is_empty());
        plugin.shutdown().ok();
    }
}
