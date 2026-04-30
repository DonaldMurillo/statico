# Plugin System

statico supports plugins that extend its analysis capabilities. Plugins run as subprocesses communicating via JSON-RPC 2.0 over stdin/stdout, which means **any language** can be used to write a plugin.

## Quick Start

### Create a plugin

```bash
# Scaffold a TypeScript plugin
statico plugin init my-plugin --lang typescript

# Scaffold a Rust plugin
statico plugin init my-plugin --lang rust
```

This creates a `.statico/plugins/my-plugin/` directory with boilerplate code.

### Run a plugin

```bash
# Run a specific plugin against a file
statico plugin run my-plugin --file src/index.ts

# Run against a specific project
statico plugin run my-plugin --file src/index.ts --path ./my-project
```

### Check setup

```bash
# Verify runtimes and plugin health
statico plugin doctor

# List discovered plugins
statico plugin list
```

## Supported Languages

| Language | Runtime | Detection | Build Step |
|---|---|---|---|
| **TypeScript** | Bun (auto-downloaded) | `package.json` in plugin dir | None (Bun runs .ts directly) |
| **Rust** | System cargo | `Cargo.toml` in plugin dir | `statico plugin build --name my-plugin` |
| **Python** | System python3 | `package.json` with `"statico": {"runtime": "python3"}` or `.py` files | None |
| **Any** | Any executable | Single file in plugin dir | Your own |

## Plugin Structure

Place plugins in `.statico/plugins/<name>/` in your project root:

```
.my-project/
├── .statico/
│   └── plugins/
│       ├── my-ts-plugin/
│       │   ├── package.json
│       │   └── index.ts
│       ├── my-rust-plugin/
│       │   ├── Cargo.toml
│       │   └── src/main.rs
│       └── my-py-plugin/
│           ├── package.json    # with "statico": {"runtime": "python3", "entry": "plugin.py"}
│           └── plugin.py
```

## Protocol

Plugins communicate via newline-delimited JSON-RPC 2.0 over stdin/stdout:

### 1. Init

statico sends:
```json
{"jsonrpc":"2.0","id":1,"method":"init","params":{"root":"/path/to/project","config":{},"pluginSettings":{}}}
```

Plugin responds with capabilities:
```json
{
  "jsonrpc":"2.0","id":1,"result":{
    "name":"my-plugin",
    "version":"1.0.0",
    "hooks":{"analyze_file":"add"},
    "languages":["typescript"],
    "rules":[{"id":"no-console","severity":"warning","description":"No console.log"}]
  }
}
```

### 2. Hook calls

statico sends hook requests as needed. The most common is `analyze_file`:

```json
{
  "jsonrpc":"2.0","id":2,"method":"analyze_file",
  "params":{
    "path":"src/index.ts",
    "source":"console.log('hello')",
    "language":"ts",
    "existingIssues":[]
  }
}
```

Plugin responds with issues:
```json
{
  "jsonrpc":"2.0","id":2,"result":{
    "issues":[{
      "ruleId":"no-console",
      "severity":"warning",
      "message":"Found console.log",
      "file":"src/index.ts",
      "line":1,
      "column":1,
      "confidence":0.95,
      "suggestion":"Remove console.log or use a proper logger"
    }]
  }
}
```

### 3. Shutdown

```json
{"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}
```
Plugin responds with `{"jsonrpc":"2.0","id":3,"result":null}` and exits.

## Hooks

| Hook | When | Mode | Description |
|---|---|---|---|
| `analyze_file` | Per source file | `add` or `override` | Analyze a file and return issues |
| `discover_entries` | Discovery phase | `add` | Discover custom entry points |
| `resolve_import` | Resolution phase | `add` or `override` | Resolve import specifiers |
| `post_analysis` | After all analysis | `add` | Post-process results |
| `format_output` | Output phase | `override` | Custom output formatting |

### Modes

- **`add`** — plugin results are merged with built-in analysis
- **`override`** — plugin completely replaces built-in behavior for that hook. Only one plugin can override a given hook.

## Writing a Plugin

### TypeScript (with SDK)

```typescript
import { Plugin, Issue } from "@statico/plugin-sdk";

const plugin = Plugin.create("my-plugin", {
  hooks: { analyze_file: "add" },
  languages: ["typescript"],
  rules: [
    { id: "my-rule", severity: "warning", description: "My custom rule" },
  ],
});

plugin.onAnalyzeFile((params) => {
  const issues: Issue[] = [];
  // Analyze params.source and params.path
  return { issues };
});

plugin.start();
```

The SDK handles JSON-RPC transport, init, and shutdown automatically.

### Python (no SDK)

Python plugins just need to read JSON lines from stdin and write JSON lines to stdout:

```python
import json, sys

def handle_init(msg_id):
    return {"jsonrpc":"2.0","id":msg_id,"result":{
        "name":"my-plugin","version":"1.0.0",
        "hooks":{"analyze_file":"add"},
        "languages":["python"],
        "rules":[{"id":"my-rule","severity":"warning","description":"My rule"}]
    }}

def handle_analyze_file(msg_id, params):
    source = params.get("source","")
    issues = []
    # Analyze source...
    return {"jsonrpc":"2.0","id":msg_id,"result":{"issues":issues}}

for line in sys.stdin:
    msg = json.loads(line.strip())
    if msg["method"] == "init":
        resp = handle_init(msg["id"])
    elif msg["method"] == "analyze_file":
        resp = handle_analyze_file(msg["id"], msg["params"])
    elif msg["method"] == "shutdown":
        print(json.dumps({"jsonrpc":"2.0","id":msg["id"],"result":None}))
        sys.exit(0)
    print(json.dumps(resp))
    sys.stdout.flush()
```

### Rust (with SDK)

```rust
use statico_plugin_sdk::{Plugin, AnalyzeFileParams, AnalyzeFileResult, Issue};

fn main() {
    let mut plugin = Plugin::new("my-plugin")
        .version("1.0.0")
        .hook("analyze_file", HookMode::Add)
        .language("rust");

    plugin.on_analyze_file(|params: AnalyzeFileParams| -> AnalyzeFileResult {
        let issues = vec![];
        // Analyze params.source and params.path
        AnalyzeFileResult { issues, ..Default::default() }
    });

    plugin.start();
}
```

## Configuration

### Plugin config in `.statico.toml`

```toml
[[plugin]]
name = "my-plugin"
enabled = true
override_all = false

[plugin.settings]
min_severity = "warning"
```

### Auto-discovery

Plugins placed in `.statico/plugins/` are automatically discovered. No configuration needed.

## Performance

| Metric | Value |
|---|---|
| Bun subprocess startup | ~11ms |
| JSON-RPC round-trip per file | ~1-2ms |
| Full pipeline overhead (87 files) | ~3-4% |
| Memory per Bun subprocess | ~30-50MB |

The subprocess is kept alive for the entire analysis run. Spawn cost is amortized.

## AI-Assisted Plugin Development

```bash
# Get the JSON schema for plugin development
statico plugin schema

# Get documentation for AI assistants
statico plugin docs
```

These commands output structured data designed for LLM consumption, making it easy to generate plugins with AI assistance.
