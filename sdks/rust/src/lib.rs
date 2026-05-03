//! `statico-plugin-sdk` — SDK for building statico plugins in Rust.
//!
//! Provides typed helpers for the JSON-RPC 2.0 protocol that statico uses
//! to communicate with plugin subprocesses over stdin/stdout.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use statico_plugin_sdk::{Plugin, PluginManifest, HookName, HookMode};
//!
//! fn main() {
//!     let mut plugin = Plugin::create("my-rule", PluginManifest {
//!         version: None,
//!         hooks: vec![(HookName::AnalyzeFile, HookMode::Add)].into_iter().collect(),
//!         languages: vec!["typescript".to_string()],
//!         rules: vec![],
//!     });
//!
//!     plugin.on_analyze_file(|params| {
//!         statico_plugin_sdk::AnalyzeFileResult::default()
//!     });
//!
//!     plugin.start();
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, Write};

// ─── Types ───────────────────────────────────────────────────────

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
    Add,
    Override,
}

/// Severity levels for issues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A rule declared in the plugin manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub severity: Severity,
    pub description: String,
}

/// The manifest passed to Plugin::create().
#[derive(Debug, Clone)]
pub struct PluginManifest {
    pub version: Option<String>,
    pub hooks: HashMap<HookName, HookMode>,
    pub languages: Vec<String>,
    pub rules: Vec<Rule>,
}

/// A single issue reported by a plugin.
///
/// Field names serialize to camelCase on the wire (e.g. `rule_id` → `ruleId`)
/// to match the JSON-RPC protocol the host expects.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
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

/// An entry point discovered by a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPoint {
    pub path: String,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub framework: Option<String>,
}

/// File metrics reported alongside analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetrics {
    pub complexity: usize,
    pub loc: usize,
}

// ─── Hook parameter / result types ───────────────────────────────

/// analyze_file params.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeFileParams {
    pub path: String,
    pub source: String,
    pub language: String,
    #[serde(default)]
    pub existing_issues: Vec<Issue>,
}

/// analyze_file result.
#[derive(Debug, Default, Serialize)]
pub struct AnalyzeFileResult {
    #[serde(default)]
    pub issues: Vec<Issue>,
    #[serde(default)]
    pub exports: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub metrics: Option<FileMetrics>,
}

/// discover_entries params.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverEntriesParams {
    pub root: String,
    #[serde(default)]
    pub config_files: Vec<String>,
    #[serde(default)]
    pub language: String,
}

/// discover_entries result.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverEntriesResult {
    #[serde(default)]
    pub entry_points: Vec<EntryPoint>,
}

/// resolve_import params.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveImportParams {
    pub from_file: String,
    pub specifier: String,
    pub root: String,
}

/// resolve_import result.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveImportResult {
    pub resolved_path: String,
    pub external: bool,
}

/// post_analysis params.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostAnalysisParams {
    pub results: serde_json::Value,
    pub health_score: f64,
    pub total_files: usize,
    #[serde(default)]
    pub language: String,
}

/// post_analysis result.
#[derive(Debug, Default, Serialize)]
pub struct PostAnalysisResult {
    #[serde(default)]
    pub issues: Vec<Issue>,
    #[serde(default)]
    pub suggestions: Vec<String>,
}

/// format_output params.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatOutputParams {
    pub results: serde_json::Value,
    pub format: String,
    pub health_score: f64,
}

/// format_output result.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatOutputResult {
    pub output: String,
    #[serde(default)]
    pub exit_code: i32,
}

// JsonRpcRequest is used internally but only for outbound messages.
// We parse inbound messages as raw serde_json::Value.

// ─── Plugin builder ──────────────────────────────────────────────

type HookHandlerFn = Box<dyn Fn(serde_json::Value) -> Result<serde_json::Value, String> + Send>;

/// Main plugin struct — reads JSON-RPC from stdin, dispatches to handlers, writes to stdout.
pub struct Plugin {
    name: String,
    manifest: PluginManifest,
    handlers: HashMap<String, HookHandlerFn>,
}

impl Plugin {
    /// Create a new plugin instance.
    pub fn create(name: &str, manifest: PluginManifest) -> Self {
        Plugin { name: name.to_string(), manifest, handlers: HashMap::new() }
    }

    /// Register a handler for the `analyze_file` hook.
    pub fn on_analyze_file<F>(&mut self, handler: F)
    where
        F: Fn(AnalyzeFileParams) -> AnalyzeFileResult + Send + 'static,
    {
        self.handlers.insert(
            "analyze_file".to_string(),
            Box::new(move |params| {
                let p: AnalyzeFileParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
                let result = handler(p);
                serde_json::to_value(result).map_err(|e| e.to_string())
            }),
        );
    }

    /// Register a handler for the `discover_entries` hook.
    pub fn on_discover_entries<F>(&mut self, handler: F)
    where
        F: Fn(DiscoverEntriesParams) -> DiscoverEntriesResult + Send + 'static,
    {
        self.handlers.insert(
            "discover_entries".to_string(),
            Box::new(move |params| {
                let p: DiscoverEntriesParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
                let result = handler(p);
                serde_json::to_value(result).map_err(|e| e.to_string())
            }),
        );
    }

    /// Register a handler for the `resolve_import` hook.
    pub fn on_resolve_import<F>(&mut self, handler: F)
    where
        F: Fn(ResolveImportParams) -> ResolveImportResult + Send + 'static,
    {
        self.handlers.insert(
            "resolve_import".to_string(),
            Box::new(move |params| {
                let p: ResolveImportParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
                let result = handler(p);
                serde_json::to_value(result).map_err(|e| e.to_string())
            }),
        );
    }

    /// Register a handler for the `post_analysis` hook.
    pub fn on_post_analysis<F>(&mut self, handler: F)
    where
        F: Fn(PostAnalysisParams) -> PostAnalysisResult + Send + 'static,
    {
        self.handlers.insert(
            "post_analysis".to_string(),
            Box::new(move |params| {
                let p: PostAnalysisParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
                let result = handler(p);
                serde_json::to_value(result).map_err(|e| e.to_string())
            }),
        );
    }

    /// Register a handler for the `format_output` hook.
    pub fn on_format_output<F>(&mut self, handler: F)
    where
        F: Fn(FormatOutputParams) -> FormatOutputResult + Send + 'static,
    {
        self.handlers.insert(
            "format_output".to_string(),
            Box::new(move |params| {
                let p: FormatOutputParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
                let result = handler(p);
                serde_json::to_value(result).map_err(|e| e.to_string())
            }),
        );
    }

    /// Process a single JSON-RPC request line and return the response plus
    /// a flag indicating whether the host has asked the plugin to shut down.
    ///
    /// This is the unit-testable core of the SDK — `start()` is just a
    /// stdin/stdout loop wrapped around it.
    pub fn process_request(&self, line: &str) -> ProcessOutcome {
        let raw: serde_json::Value = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                return ProcessOutcome {
                    response: serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 0,
                        "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                    })
                    .to_string(),
                    shutdown: false,
                };
            }
        };
        let id = raw.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        let method = raw.get("method").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let params = raw.get("params").cloned().unwrap_or(serde_json::Value::Null);

        let (response, shutdown) = match method.as_str() {
            "init" => {
                let caps = serde_json::json!({
                    "name": self.name,
                    "version": self.manifest.version,
                    "hooks": self.manifest.hooks.iter().map(|(k, v)| {
                        (serde_json::to_value(k).unwrap_or_default().as_str().unwrap_or("").to_string(), serde_json::to_value(v).unwrap_or_default())
                    }).collect::<HashMap<String, serde_json::Value>>(),
                    "languages": self.manifest.languages,
                    "rules": self.manifest.rules,
                });
                (serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": caps }), false)
            }
            "shutdown" => (serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": null }), true),
            _ => {
                let resp = if let Some(handler) = self.handlers.get(&method) {
                    match handler(params) {
                        Ok(result) => {
                            serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result })
                        }
                        Err(msg) => {
                            serde_json::json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": msg } })
                        }
                    }
                } else {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": format!("Method not found: {}", method) }
                    })
                };
                (resp, false)
            }
        };

        ProcessOutcome { response: response.to_string(), shutdown }
    }

    /// Start the JSON-RPC read loop.
    ///
    /// Reads newline-delimited JSON from stdin, dispatches to registered
    /// handlers, and writes responses to stdout. Handles `init` and
    /// `shutdown` methods automatically.
    pub fn start(self) -> ! {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let mut reader = std::io::BufReader::new(stdin.lock());

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // EOF — parent closed stdin.
                    std::process::exit(0);
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("plugin-sdk: stdin read error: {}", e);
                    std::process::exit(1);
                }
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let outcome = self.process_request(trimmed);
            let _ = writeln!(stdout, "{}", outcome.response);
            let _ = stdout.flush();
            if outcome.shutdown {
                std::process::exit(0);
            }
        }
    }
}

/// Result of dispatching a single JSON-RPC request.
///
/// `response` is the line that should be written to stdout (without
/// trailing newline). `shutdown` is `true` only for the `shutdown`
/// method — when set, the host expects the plugin to exit after the
/// response is written.
#[derive(Debug, Clone)]
pub struct ProcessOutcome {
    pub response: String,
    pub shutdown: bool,
}
