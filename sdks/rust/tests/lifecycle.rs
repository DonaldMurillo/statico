//! End-to-end JSON-RPC lifecycle tests for the Rust plugin SDK.
//!
//! These drive `Plugin::process_request` directly so the dispatcher is
//! covered without spawning a subprocess (which would require an extra
//! example binary). The subprocess loop in `start()` is a thin wrapper
//! around `process_request` plus stdin/stdout I/O.

use serde_json::{Value, json};
use statico_plugin_sdk::{AnalyzeFileResult, HookMode, HookName, Issue, Plugin, PluginManifest, Severity};

fn manifest_with(hook: HookName) -> PluginManifest {
    PluginManifest {
        version: Some("0.1.0".to_string()),
        hooks: [(hook, HookMode::Add)].into_iter().collect(),
        languages: vec!["typescript".to_string()],
        rules: vec![],
    }
}

fn parse(line: &str) -> Value {
    serde_json::from_str(line).expect("response must be valid JSON")
}

#[test]
fn init_returns_plugin_capabilities() {
    let plugin = Plugin::create("test-plugin", manifest_with(HookName::AnalyzeFile));
    let outcome = plugin.process_request(r#"{"jsonrpc":"2.0","id":1,"method":"init"}"#);

    assert!(!outcome.shutdown, "init must not trigger shutdown");
    let v = parse(&outcome.response);
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["id"], 1);
    assert_eq!(v["result"]["name"], "test-plugin");
    assert_eq!(v["result"]["version"], "0.1.0");
    assert_eq!(v["result"]["languages"][0], "typescript");
    assert_eq!(v["result"]["hooks"]["analyze_file"], "add");
}

#[test]
fn analyze_file_dispatches_to_registered_handler() {
    let mut plugin = Plugin::create("analyzer", manifest_with(HookName::AnalyzeFile));
    plugin.on_analyze_file(|params| {
        assert_eq!(params.path, "src/foo.ts");
        AnalyzeFileResult {
            issues: vec![Issue {
                rule_id: "demo".into(),
                severity: Severity::Warning,
                message: "hello".into(),
                file: params.path.clone(),
                line: 7,
                column: None,
                end_line: None,
                end_column: None,
                confidence: Some(0.9),
                suggestion: None,
            }],
            ..Default::default()
        }
    });

    let req = json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "analyze_file",
        "params": { "path": "src/foo.ts", "source": "x", "language": "typescript" }
    })
    .to_string();
    let outcome = plugin.process_request(&req);

    assert!(!outcome.shutdown);
    let v = parse(&outcome.response);
    assert_eq!(v["id"], 42);
    assert_eq!(v["result"]["issues"][0]["ruleId"], "demo");
    assert_eq!(v["result"]["issues"][0]["line"], 7);
}

#[test]
fn unknown_method_returns_method_not_found() {
    let plugin = Plugin::create("p", manifest_with(HookName::AnalyzeFile));
    let req = r#"{"jsonrpc":"2.0","id":3,"method":"does_not_exist"}"#;
    let outcome = plugin.process_request(req);

    assert!(!outcome.shutdown);
    let v = parse(&outcome.response);
    assert_eq!(v["id"], 3);
    assert_eq!(v["error"]["code"], -32601);
}

#[test]
fn malformed_json_returns_parse_error() {
    let plugin = Plugin::create("p", manifest_with(HookName::AnalyzeFile));
    let outcome = plugin.process_request("not json {{{");
    assert!(!outcome.shutdown);
    let v = parse(&outcome.response);
    assert_eq!(v["error"]["code"], -32700);
}

#[test]
fn shutdown_signals_exit_with_response() {
    let plugin = Plugin::create("p", manifest_with(HookName::AnalyzeFile));
    let outcome = plugin.process_request(r#"{"jsonrpc":"2.0","id":9,"method":"shutdown"}"#);
    assert!(outcome.shutdown, "shutdown must signal exit");
    let v = parse(&outcome.response);
    assert_eq!(v["id"], 9);
    assert!(v["result"].is_null());
}

#[test]
fn analyze_file_decodes_camelcase_existing_issues() {
    // existing_issues is sent as `existingIssues` on the wire — verify
    // the SDK accepts it.
    let mut plugin = Plugin::create("p", manifest_with(HookName::AnalyzeFile));
    plugin.on_analyze_file(|params| {
        assert_eq!(params.existing_issues.len(), 1);
        assert_eq!(params.existing_issues[0].rule_id, "previous");
        AnalyzeFileResult::default()
    });

    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "analyze_file",
        "params": {
            "path": "src/foo.ts",
            "source": "x",
            "language": "typescript",
            "existingIssues": [{
                "ruleId": "previous",
                "severity": "warning",
                "message": "old",
                "file": "src/foo.ts",
                "line": 1
            }]
        }
    })
    .to_string();
    let outcome = plugin.process_request(&req);
    assert!(!outcome.shutdown);
    let v = parse(&outcome.response);
    assert!(v.get("error").is_none(), "got error: {}", v);
}

#[test]
fn handler_error_returns_jsonrpc_error() {
    let mut plugin = Plugin::create("p", manifest_with(HookName::AnalyzeFile));
    // analyze_file requires `source` field — omitting it forces the
    // SDK's deserializer to fail, which should surface as a JSON-RPC error
    // rather than crash the plugin.
    plugin.on_analyze_file(|_| AnalyzeFileResult::default());

    let req = r#"{"jsonrpc":"2.0","id":5,"method":"analyze_file","params":{"path":"a"}}"#;
    let outcome = plugin.process_request(req);
    assert!(!outcome.shutdown);
    let v = parse(&outcome.response);
    assert_eq!(v["id"], 5);
    assert_eq!(v["error"]["code"], -32000);
}
