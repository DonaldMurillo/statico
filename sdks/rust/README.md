# statico-plugin-sdk

SDK for building [statico](https://github.com/DonaldMurillo/statico) static-analyzer plugins in Rust.

> ⚠️ **Early alpha.** APIs may change between minor versions until 1.0.

## What is statico?

statico is a customizable, AI-forward static analyzer for TypeScript and Rust. Plugins extend it
with custom rules, language support, entry-point discovery, and output formats. They run as
subprocesses and talk to the statico host over JSON-RPC 2.0 on stdin/stdout — any language can
write a plugin, but this crate gives Rust authors a typed, ergonomic wrapper.

## Quick start

```toml
# Cargo.toml
[package]
name = "my-statico-plugin"
edition = "2024"

[dependencies]
statico-plugin-sdk = "0.1"
```

```rust
use statico_plugin_sdk::{
    AnalyzeFileResult, HookMode, HookName, Issue, Plugin, PluginManifest, Severity,
};

fn main() {
    let mut plugin = Plugin::create("no-todo-comments", PluginManifest {
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
        hooks: [(HookName::AnalyzeFile, HookMode::Add)].into_iter().collect(),
        languages: vec!["typescript".into(), "rust".into()],
        rules: vec![],
    });

    plugin.on_analyze_file(|params| {
        let issues = params
            .source
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains("TODO"))
            .map(|(i, _)| Issue {
                rule_id: "no-todo".into(),
                severity: Severity::Warning,
                message: "Found TODO comment".into(),
                file: params.path.clone(),
                line: i + 1,
                column: None,
                end_line: None,
                end_column: None,
                confidence: Some(0.95),
                suggestion: None,
            })
            .collect();
        AnalyzeFileResult { issues, ..Default::default() }
    });

    plugin.start(); // never returns — runs the JSON-RPC loop
}
```

Build a release binary, then point statico at it via `.statico.toml`:

```toml
[[plugin]]
name = "no-todo-comments"
runtime = "executable"
entry = "./target/release/my-statico-plugin"
```

## Hooks

Each hook has a typed registration method on `Plugin`:

| Hook | Method | Purpose |
|---|---|---|
| `analyze_file` | `on_analyze_file` | Inspect a single source file; emit issues, exports, imports |
| `discover_entries` | `on_discover_entries` | Tell statico about reachable entry points (e.g. framework routes) |
| `resolve_import` | `on_resolve_import` | Map an import specifier to a file path |
| `post_analysis` | `on_post_analysis` | Run after the main analysis finishes; emit cross-file issues |
| `format_output` | `on_format_output` | Provide a custom output format |

See the [plugin docs](https://github.com/DonaldMurillo/statico/blob/main/docs/plugins.md) for the
full protocol contract.

## Wire format

All structs serialize to **camelCase** JSON regardless of Rust naming. For example, `Issue.rule_id`
becomes `"ruleId"` on the wire. This matches the host's expectations and the TypeScript SDK.

## Testing your plugin

The dispatcher logic is testable without spawning a subprocess via `Plugin::process_request`:

```rust
let plugin = Plugin::create("p", PluginManifest { /* ... */ });
let outcome = plugin.process_request(r#"{"jsonrpc":"2.0","id":1,"method":"init"}"#);
let response: serde_json::Value = serde_json::from_str(&outcome.response).unwrap();
assert_eq!(response["result"]["name"], "p");
```

See `tests/lifecycle.rs` for more examples.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
