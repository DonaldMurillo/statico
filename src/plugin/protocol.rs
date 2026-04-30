//! JSON-RPC protocol types for the statico plugin system.
//!
//! Every message between statico and a plugin follows the JSON-RPC 2.0 spec.
//! Plugins read newline-delimited JSON from stdin and write to stdout.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

impl AsRef<str> for Severity {
    fn as_ref(&self) -> &str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// A rule declared by a plugin in its capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub severity: Severity,
    pub description: String,
}

/// A plugin's declared hooks and modes.
pub type HookMap = HashMap<HookName, HookMode>;

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
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// A JSON-RPC 2.0 success response (generic).
#[derive(Debug, Serialize, Deserialize)]
pub struct GenericResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: serde_json::Value,
}

/// A JSON-RPC 2.0 error response.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub jsonrpc: String,
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

/// Standard JSON-RPC error codes.
pub const CODE_METHOD_NOT_FOUND: i64 = -32601;
pub const CODE_INVALID_PARAMS: i64 = -32602;
pub const CODE_INTERNAL_ERROR: i64 = -32603;
/// Custom plugin error codes start here.
pub const CODE_PLUGIN_ERROR: i64 = -32000;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_request() {
        let req = Request {
            jsonrpc: "2.0",
            id: 1,
            method: "init".to_string(),
            params: serde_json::json!({"root": "/tmp"}),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"init\""));
    }

    #[test]
    fn deserialize_capabilities() {
        let json = r#"{
            "name": "test-plugin",
            "version": "0.1.0",
            "hooks": {"analyze_file": "add", "post_analysis": "add"},
            "languages": ["typescript"],
            "rules": [{"id": "no-console", "severity": "warning", "description": "test"}]
        }"#;
        let caps: PluginCapabilities = serde_json::from_str(json).unwrap();
        assert_eq!(caps.name, "test-plugin");
        assert_eq!(caps.hooks.len(), 2);
        assert_eq!(caps.hooks[&HookName::AnalyzeFile], HookMode::Add);
    }

    #[test]
    fn roundtrip_analyze_result() {
        let result = AnalyzeFileResult {
            issues: vec![PluginIssue {
                rule_id: "test".to_string(),
                severity: Severity::Warning,
                message: "bad".to_string(),
                file: "foo.ts".to_string(),
                line: 1,
                column: None,
                end_line: None,
                end_column: None,
                confidence: Some(0.9),
                suggestion: None,
            }],
            exports: vec!["foo".to_string()],
            dependencies: vec![],
            metrics: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: AnalyzeFileResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.issues.len(), 1);
        assert_eq!(parsed.exports, vec!["foo"]);
    }
}
