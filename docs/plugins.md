# Plugin system

statico plugins extend the analyzer with project-specific rules. They run as
subprocesses and communicate over stdin/stdout via newline-delimited JSON-RPC
2.0, so any language that can read and write JSON works — no SDK required,
though we ship one for TypeScript and Rust to handle the boilerplate.

> ⚠️ **Pre-1.0.** The plugin protocol can change between minor releases. Print
> the current schema with `statico plugin schema --format json` and version
> your plugins against it.

For a complete working multi-hook example, see
[`examples/plugins/coverage-gap`](../examples/plugins/coverage-gap). The rest
of this doc is the protocol + tooling reference.

---

## Quick start

### Scaffold

```bash
statico plugin init my-rule --lang typescript    # default
statico plugin init my-rule --lang rust
statico plugin init my-rule --lang python
```

Creates `.statico/plugins/my-rule/` with boilerplate.

### Build (Rust only)

```bash
statico plugin build --name my-rule    # cargo build --release inside the plugin dir
```

TypeScript and Python plugins don't need a build step.

### Run

```bash
statico plugin run my-rule --file src/foo.ts
statico plugin doctor                     # checks Bun + cargo runtimes
statico plugin list                       # what's discovered
statico plugin schema --format json       # protocol JSON schema
statico plugin docs                       # human-readable protocol reference
```

---

## Layout

Plugins live under `.statico/plugins/` in the project root, one per
subdirectory. Auto-discovery scans this directory at every analyze run.

```
.statico/plugins/
├── coverage-gap/         # TypeScript plugin
│   ├── package.json
│   └── index.ts
├── my-rust-rule/         # Rust plugin (compiled output goes in target/)
│   ├── Cargo.toml
│   └── src/main.rs
└── domain-check/         # Python plugin
    ├── package.json      # with "statico": {"runtime": "python3", "entry": "plugin.py"}
    └── plugin.py
```

You can also reference a plugin outside `.statico/plugins/` via an explicit
config entry — see [Configuration](configuration.md).

### Plugin kinds

| Kind | Detection | Runtime |
|---|---|---|
| TypeScript | `package.json` (no `statico.runtime` override) | Bun (auto-downloaded to `~/.statico/runtimes/bun/` if missing) |
| Rust | `Cargo.toml` | Pre-compiled binary in plugin's `target/release/` |
| Python | `package.json` with `"statico": {"runtime": "python3"}` *or* a `.py` entry file | System `python3` |
| Executable | A single executable file in `.statico/plugins/` | Spawned directly |

Plugin entry resolution rejects path traversal — any `..` outside the plugin
directory is refused.

---

## Hooks

| Hook | When | Mode | Purpose |
|---|---|---|---|
| `analyze_file` | Per source file, after the built-in parser | `add` or `override` | Add issues, exports, and dependencies for a single file |
| `discover_entries` | During entry-point discovery | `override` only | Replace the built-in entry-point detector |
| `resolve_import` | When resolving an import specifier | `override` only | Replace the built-in resolver |
| `post_analysis` | After the full analyze pass | `add` only | Cross-cutting issues that need the whole project state |
| `format_output` | Before formatting the final report | `override` only | Provide a custom format string |

### Modes

- **`add`** — your results are merged with built-in output and any other
  plugins. Multiple `add` plugins on the same hook is fine.
- **`override`** — your plugin replaces built-in behavior for that hook. Only
  one plugin per hook can be `override`. Two plugins both overriding the same
  hook is a fatal startup error.

---

## Protocol

Every message is a newline-delimited JSON-RPC 2.0 frame on stdin/stdout.
`stderr` is passed through to the user's terminal — use it for debug logging.

### `init`

statico → plugin (always the first call):

```json
{"jsonrpc":"2.0","id":1,"method":"init","params":{
  "root":"/path/to/project",
  "config":{},
  "pluginSettings":{ "...whatever the plugin defines..." }
}}
```

Plugin → statico (capabilities + the rule set the plugin will emit):

```json
{"jsonrpc":"2.0","id":1,"result":{
  "name":"coverage-gap",
  "version":"0.1.0",
  "hooks":{"analyze_file":"add","post_analysis":"add"},
  "languages":["typescript","tsx"],
  "rules":[
    {"id":"missing-test","severity":"warning","description":"Exported function has no test reference"}
  ]
}}
```

### `analyze_file`

statico → plugin (per source file):

```json
{"jsonrpc":"2.0","id":2,"method":"analyze_file","params":{
  "path":"src/foo.ts",
  "source":"export function x() {…}",
  "language":"typescript",
  "existingIssues":[]
}}
```

Plugin → statico:

```json
{"jsonrpc":"2.0","id":2,"result":{
  "issues":[{
    "ruleId":"missing-test",
    "severity":"warning",
    "message":"`x` is exported but has no matching test reference",
    "file":"src/foo.ts",
    "line":1,
    "column":1,
    "confidence":0.9,
    "suggestion":"Add a test in src/foo.test.ts"
  }],
  "exports":[],
  "dependencies":[]
}}
```

`exports` and `dependencies` let the plugin contribute to the dep graph
(useful when overriding the parser). For pure issue-reporting plugins, leave
both empty.

### `post_analysis`

statico → plugin (once, after all files have been analyzed):

```json
{"jsonrpc":"2.0","id":3,"method":"post_analysis","params":{
  "results":{"...full AnalysisOutput JSON..."},
  "healthScore":78.4,
  "totalFiles":102,
  "language":""
}}
```

Plugin → statico:

```json
{"jsonrpc":"2.0","id":3,"result":{
  "issues":[…],
  "suggestions":["Consider extracting shared types into a barrel file"]
}}
```

### `format_output`

statico → plugin (when the user passes `--format <whatever-your-plugin-handles>`):

```json
{"jsonrpc":"2.0","id":4,"method":"format_output","params":{
  "results":{…full analysis output…},
  "format":"my-custom-format",
  "healthScore":78.4
}}
```

Plugin → statico:

```json
{"jsonrpc":"2.0","id":4,"result":{
  "output":"…the full report text…",
  "exitCode":0
}}
```

If multiple plugins return output for the same format, statico concatenates
them. To replace built-in formatters entirely, use `override` mode.

### `shutdown`

```json
{"jsonrpc":"2.0","id":5,"method":"shutdown"}
```

Plugin returns `null` and exits. statico waits ~100 ms before sending SIGKILL.

### Error response

If something goes wrong, return an error object instead of `result`:

```json
{"jsonrpc":"2.0","id":2,"error":{"code":-32603,"message":"file too large"}}
```

The plugin keeps running — statico just skips that file.

### Limits and safety

- 10 MB max response per request (anything larger and statico kills the
  plugin and reports it as a warning).
- 30 second timeout per request.
- Plugin settings (the table you put in `[plugin.settings]` of
  `.statico.toml`) are capped at 64 KB serialized JSON, 32 levels of
  nesting, 1000 entries per array/table.

---

## Writing a plugin

### TypeScript (with the SDK)

```typescript
import { Plugin, type Issue } from "@statico/plugin-sdk";

const plugin = Plugin.create("my-rule", {
  hooks: { analyze_file: "add" },
  languages: ["typescript"],
  rules: [
    { id: "my-rule", severity: "warning", description: "..." },
  ],
});

plugin.onAnalyzeFile(({ path, source, language }) => {
  const issues: Issue[] = [];
  // Walk source however you like — regex, oxc, ts-morph, tree-sitter…
  return { issues };
});

plugin.start();
```

The SDK handles handshake, transport, and shutdown.

### Rust (with the SDK)

```rust
use statico_plugin_sdk::{Plugin, AnalyzeFileResult, HookMode, Issue, Severity};

fn main() {
    let mut p = Plugin::new("my-rule")
        .version("0.1.0")
        .hook("analyze_file", HookMode::Add)
        .language("rust");

    p.on_analyze_file(|params| {
        let mut issues = vec![];
        // Walk params.source however you like — syn, tree-sitter, regex…
        AnalyzeFileResult { issues, ..Default::default() }
    });

    p.start();
}
```

### Python (no SDK)

```python
import json, sys

CAPS = {
    "name": "my-rule", "version": "0.1.0",
    "hooks": {"analyze_file": "add"},
    "languages": ["python"],
    "rules": [{"id": "my-rule", "severity": "warning", "description": "..."}],
}

def reply(msg_id, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg_id, "result": result}) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    msg = json.loads(line)
    method = msg["method"]
    if method == "init":
        reply(msg["id"], CAPS)
    elif method == "analyze_file":
        reply(msg["id"], {"issues": [], "exports": [], "dependencies": []})
    elif method == "shutdown":
        reply(msg["id"], None)
        break
```

For a fully-worked TypeScript example with two hooks (`analyze_file` +
`post_analysis`) and runtime configuration via `pluginSettings`, see
[`examples/plugins/coverage-gap`](../examples/plugins/coverage-gap).

---

## AI-assisted plugin authoring

Both `statico plugin schema --format json` and `statico plugin docs` produce
output specifically intended for LLM consumption — the schema is the literal
JSON-RPC contract, the docs are markdown the assistant can fold into prompts.

A common workflow is:

```bash
statico plugin schema --format json > /tmp/plugin-schema.json
# Then prompt your AI assistant: "Build a statico plugin that [X], using
# this protocol schema: <paste>".
```

The skills generated by `statico setup --target claude` also include a
`statico-plugin` skill that walks Claude through the protocol with the right
context.

---

## Performance baseline

| Metric | Approximate |
|---|---|
| Bun subprocess startup | ~10 ms |
| JSON-RPC round-trip per `analyze_file` | ~1–2 ms |
| Full pipeline overhead (~100 files, 1 plugin) | ~3–5% |
| Memory per Bun subprocess | ~30–50 MB |

The subprocess is kept alive for the entire `statico analyze` run, so the
spawn cost amortizes. Multiple plugins run sequentially per file (not in
parallel — keeps the protocol simple).
