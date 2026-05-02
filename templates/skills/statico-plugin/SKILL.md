# statico Plugin Development

Use when the user wants to create, modify, or debug a statico plugin.

## Quick Start

```bash
# Scaffold a new TypeScript plugin
statico plugin init my-plugin --lang typescript

# Scaffold a new Rust plugin
statico plugin init my-plugin --lang rust

# Run a plugin against a test file
statico plugin run my-plugin --file fixtures/sample.ts

# Check runtime readiness
statico plugin doctor
```

## Plugin Protocol

Plugins communicate via JSON-RPC 2.0 over stdin/stdout (newline-delimited).

### Lifecycle

1. statico spawns the plugin subprocess
2. Sends `init` request → plugin responds with capabilities
3. Sends hook requests (`analyze_file`, `discover_entries`, etc.)
4. Sends `shutdown` → plugin exits

### Hooks

| Hook | When | Mode |
|------|------|------|
| `analyze_file` | Per-file analysis | add |
| `discover_entries` | Find entry points | add |
| `resolve_import` | Resolve import specifiers | override |
| `post_analysis` | After full analysis | add |
| `format_output` | Before displaying results | override |

### Modes

- **add**: Contribute alongside built-in analysis and other plugins
- **override**: Replace the built-in stage entirely (only one plugin per hook)

## TypeScript SDK

```typescript
import { Plugin } from "@statico/plugin-sdk";

const plugin = Plugin.create("my-rule", {
  hooks: { analyze_file: "add" },
  languages: ["typescript"],
  rules: [{ id: "my-rule", severity: "warning", description: "..." }],
});

plugin.onAnalyzeFile((params) => {
  const issues = [];
  // params.path, params.source, params.language
  // Detect patterns in params.source
  return { issues };
});

plugin.start();
```

## Rust SDK

```rust
use statico_plugin_sdk::{Plugin, PluginManifest, HookName, HookMode};
use std::collections::HashMap;

fn main() {
    let mut plugin = Plugin::create("my-rule", PluginManifest {
        version: Some("0.1.0".to_string()),
        hooks: HashMap::from([(HookName::AnalyzeFile, HookMode::Add)]),
        languages: vec!["rust".to_string()],
        rules: vec![],
    });

    plugin.on_analyze_file(|params| {
        // params.path, params.source, params.language
        statico_plugin_sdk::AnalyzeFileResult::default()
    });

    plugin.start();
}
```

## Protocol Reference

Run `statico plugin docs` for the full protocol reference.
Run `statico plugin schema --format json` for the JSON schema.
