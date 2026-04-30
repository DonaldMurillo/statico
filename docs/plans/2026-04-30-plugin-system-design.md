# statico Plugin System Design

> **Status:** Design phase  
> **Branch:** `feature/plugin-system`  
> **Date:** 2026-04-30

## Overview

A user-facing plugin system that enables anyone to extend statico's analysis pipeline. Plugins are subprocesses that communicate via newline-delimited JSON-RPC over stdin/stdout. Users can write plugins in **Rust** or **TypeScript** (via Bun), with full scaffolding, SDK, and AI-assisted development support.

The system supports two modes:
- **Additive** — plugin contributes alongside built-in analysis
- **Override** — plugin completely replaces a pipeline stage

A single plugin that overrides every stage can function as a "completely local fork" of statico.

## Guiding Principles

1. **LLM-first** — The protocol is simple enough for an LLM to generate a working plugin in one shot. `statico plugin schema` outputs the full contract. Skills and agents provide guided development.
2. **Subprocess isolation** — Every plugin is a separate process. Crashes don't take down statico. OS-level sandboxing.
3. **Convention + config** — Auto-discover plugins in `.statico/plugins/`, override/configure in `.statico.toml`.
4. **Lazy runtimes** — Only download runtimes (Bun) when needed. Rust plugins require `cargo` on the user's system.
5. **Typed SDKs** — Thin wrappers over JSON-RPC for both TypeScript and Rust. The JSON schema is canonical; SDKs are optional convenience.

---

## 1. Architecture

### Execution Model

```
┌─────────────┐     JSON-RPC stdin/stdout     ┌──────────────────┐
│  statico    │ ◄─────────────────────────────►│  plugin process  │
│  (parent)   │   newline-delimited JSON       │  (subprocess)    │
└─────────────┘                                └──────────────────┘
```

- Transport: newline-delimited JSON over stdin/stdout
- stderr: passed through for debug logging
- Each message is a single JSON object terminated by `\n`
- Protocol version: JSON-RPC 2.0

### Plugin Runtimes

| Language | Runtime | Build | Entry |
|----------|---------|-------|-------|
| TypeScript | Bun (auto-downloaded to `~/.statico/runtimes/bun`) | `bun build` or run directly | `index.ts` |
| Rust | System `cargo` | `cargo build --release` | Compiled binary |
| Any | Any executable that speaks the protocol | Manual | Any binary/script |

### Runtime Bootstrap

statico lazily manages runtimes:

1. Scan project for plugins (`.statico/plugins/` + `.statico.toml`)
2. Determine which runtimes are needed (TS plugins → Bun, Rust plugins → cargo)
3. **Bun**: if not found, download to `~/.statico/runtimes/bun` (~80MB, single binary)
4. **Rust**: if no `cargo`, `statico plugin doctor` prints install instructions (no silent install — too opinionated)
5. `statico plugin doctor` is the single source of truth for runtime readiness

---

## 2. JSON-RPC Protocol

### Message Flow

```
statico                          plugin
  │                                │
  │── init ───────────────────────►│
  │◄─ capabilities ───────────────│
  │                                │
  │── analyze_file ──────────────►│  (per file, if hook registered)
  │◄─ file_result ────────────────│
  │                                │
  │── discover_entries ──────────►│  (if hook registered)
  │◄─ entries_result ─────────────│
  │                                │
  │── resolve_import ────────────►│  (per import, if hook registered)
  │◄─ resolve_result ─────────────│
  │                                │
  │── post_analysis ─────────────►│  (if hook registered)
  │◄─ post_result ────────────────│
  │                                │
  │── format_output ─────────────►│  (if hook registered)
  │◄─ format_result ──────────────│
  │                                │
  │── shutdown ──────────────────►│
  │                                │
```

### Init

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "init",
  "params": {
    "root": "/abs/path/to/project",
    "config": {},
    "pluginSettings": {}
  }
}
```

**Response (capabilities):**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "name": "my-detector",
    "version": "1.0.0",
    "hooks": {
      "analyze_file": "add",
      "post_analysis": "add"
    },
    "languages": ["typescript"],
    "rules": [
      {
        "id": "no-console-log",
        "severity": "warning",
        "description": "No console.log in production code"
      }
    ]
  }
}
```

Hook modes:
- `"add"` — plugin contributes alongside built-in analysis and other plugins
- `"override"` — plugin replaces the built-in stage entirely

**Conflict resolution:** If two plugins declare `"override"` on the same hook, statico prints an error and exits. The user must resolve the conflict explicitly.

---

## 3. Pipeline Hooks

### `analyze_file` — Per-file analysis

**Modes:** `add` or `override`

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "analyze_file",
  "params": {
    "path": "src/utils/helpers.ts",
    "source": "export function foo() { ... }",
    "language": "typescript",
    "existingIssues": []
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "issues": [
      {
        "ruleId": "no-console-log",
        "severity": "warning",
        "message": "Unexpected console.log",
        "file": "src/utils/helpers.ts",
        "line": 42,
        "column": 3,
        "endLine": 42,
        "endColumn": 15,
        "confidence": 0.9,
        "suggestion": "Use a proper logger"
      }
    ],
    "exports": ["foo", "Bar"],
    "dependencies": ["./other"],
    "metrics": {
      "complexity": 5,
      "loc": 120
    }
  }
}
```

### `discover_entries` — Custom entry point discovery

**Modes:** `override` only

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "discover_entries",
  "params": {
    "root": "/path/to/project",
    "configFiles": ["next.config.js", "tsconfig.json"],
    "language": "typescript"
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "entryPoints": [
      { "path": "src/app/page.tsx", "type": "page", "framework": "next.js" }
    ]
  }
}
```

### `resolve_import` — Custom import resolution

**Modes:** `override` only

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "resolve_import",
  "params": {
    "fromFile": "src/components/Button.tsx",
    "specifier": "@ui/utils",
    "root": "/path/to/project"
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "result": {
    "resolvedPath": "src/ui/utils/index.ts",
    "external": false
  }
}
```

### `post_analysis` — After full analysis

**Modes:** `add` only

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "post_analysis",
  "params": {
    "results": {},
    "healthScore": 72.5,
    "totalFiles": 150,
    "language": "typescript"
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "result": {
    "issues": [],
    "suggestions": [
      "Consider splitting the auth module — 15 circular dependencies detected"
    ]
  }
}
```

### `format_output` — Custom output formatting

**Modes:** `override` only

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "method": "format_output",
  "params": {
    "results": {},
    "format": "slack-webhook",
    "healthScore": 72.5
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "result": {
    "output": "{\"text\": \"Health: 72.5 — 3 new issues\"}",
    "exitCode": 0
  }
}
```

### `shutdown` — Clean teardown

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 99,
  "method": "shutdown"
}
```

No response needed — plugin exits with code 0.

---

## 4. Plugin Discovery & Configuration

### Convention-based (auto-discovery)

Drop executables or plugin directories in `.statico/plugins/`:

```
.statico/plugins/
├── detect-auth-middleware      # compiled Rust binary or any executable
├── vue-parser/
│   ├── package.json
│   └── index.ts                # Bun runs this directly
└── custom-output               # executable
```

statico auto-discovers by scanning this directory. Each entry is probed:
- If it's an executable → register as a plugin
- If it's a directory with `package.json` → register as a TS/Bun plugin
- If it's a directory with `Cargo.toml` → register as a Rust plugin

### Config-based (`.statico.toml`)

```toml
# Auto-discovery is ON by default. Disable it:
# plugin_auto_discover = false

[[plugin]]
name = "my-custom-rules"
path = "./plugins/my-rules"
enabled = true
languages = ["typescript"]
settings = { max_complexity = 10 }

[[plugin]]
name = "acme-fork"
path = "./plugins/acme-statico"
override = true                           # overrides ALL registered hooks
settings = {}

[[plugin]]
name = "disabled-experiment"
enabled = false
```

### Priority rules

1. Config entries override auto-discovery for the same plugin name
2. If two plugins both declare `"override"` on the same hook → **error at startup**
3. `enabled = false` skips the plugin entirely
4. `override = true` at the config level is shorthand for declaring all hooks as override

---

## 5. CLI Commands

```bash
# Scaffolding
statico plugin init <name> --lang typescript   # Scaffold TS plugin
statico plugin init <name> --lang rust         # Scaffold Rust plugin

# Build & test
statico plugin build [--name <name>]           # Build one or all plugins
statico plugin test [--name <name>]            # Run against test fixtures
statico plugin run <name> --file src/foo.ts    # Run single plugin in isolation

# Introspection
statico plugin list                            # List discovered plugins + status
statico plugin schema [--format json]          # Print full protocol schema
statico plugin docs                            # Print human-readable protocol reference
statico plugin doctor                          # Check runtimes, SDK, dependencies

# AI integration
statico setup                                  # Now also generates plugin-dev skill + agent
```

### Scaffold output — TypeScript

`statico plugin init my-rule --lang typescript` generates:

```
.statico/plugins/my-rule/
├── package.json          # with @statico/plugin-sdk dependency
├── tsconfig.json
├── index.ts              # entry point with scaffolded hooks
├── fixtures/
│   └── sample.ts         # test fixture
└── README.md             # auto-generated with protocol docs
```

Scaffolded `index.ts`:
```typescript
import { Plugin, AnalyzeFileHook } from "@statico/plugin-sdk";

const plugin = Plugin.create("my-rule", {
  hooks: { analyze_file: "add" },
  languages: ["typescript"],
  rules: [
    { id: "my-rule", severity: "warning", description: "TODO: describe your rule" },
  ],
});

plugin.onAnalyzeFile((params) => {
  const issues = [];
  // TODO: implement your detection logic
  if (params.source.includes("console.log")) {
    issues.push({
      ruleId: "my-rule",
      severity: "warning",
      message: "Found console.log",
      file: params.path,
      line: 1,
      confidence: 0.9,
    });
  }
  return { issues };
});

plugin.start();
```

### Scaffold output — Rust

`statico plugin init my-rule --lang rust` generates:

```
.statico/plugins/my-rule/
├── Cargo.toml            # with statico-plugin-sdk dependency
├── src/
│   └── main.rs           # scaffolded hooks
├── fixtures/
│   └── sample.rs
└── README.md
```

---

## 6. Plugin SDKs

### TypeScript SDK (`@statico/plugin-sdk`)

~200 lines. Reads JSON-RPC from stdin, dispatches to handlers, writes to stdout.

```typescript
export declare class Plugin {
  static create(name: string, manifest: PluginManifest): Plugin;
  onAnalyzeFile(handler: (params: AnalyzeFileParams) => AnalyzeFileResult): void;
  onDiscoverEntries(handler: (params: DiscoverEntriesParams) => DiscoverEntriesResult): void;
  onResolveImport(handler: (params: ResolveImportParams) => ResolveImportResult): void;
  onPostAnalysis(handler: (params: PostAnalysisParams) => PostAnalysisResult): void;
  onFormatOutput(handler: (params: FormatOutputParams) => FormatOutputResult): void;
  start(): void;
}

export interface PluginManifest {
  hooks: Record<HookName, "add" | "override">;
  languages?: string[];
  rules?: Rule[];
}

export interface Rule {
  id: string;
  severity: "error" | "warning" | "info";
  description: string;
}

export interface Issue {
  ruleId: string;
  severity: "error" | "warning" | "info";
  message: string;
  file: string;
  line: number;
  column?: number;
  endLine?: number;
  endColumn?: number;
  confidence?: number;
  suggestion?: string;
}

// Param/result types mirror the JSON protocol exactly (see Section 3)
```

**Publishing:** npm (verdaccio for local dev testing)

### Rust SDK (`statico-plugin-sdk` crate)

```rust
pub struct Plugin { /* ... */ }

impl Plugin {
    pub fn create(name: &str, manifest: PluginManifest) -> Self;
    pub fn on_analyze_file(&mut self, handler: impl Fn(AnalyzeFileParams) -> AnalyzeFileResult + Send + 'static);
    pub fn on_discover_entries(&mut self, handler: impl Fn(DiscoverEntriesParams) -> DiscoverEntriesResult + Send + 'static);
    pub fn on_resolve_import(&mut self, handler: impl Fn(ResolveImportParams) -> ResolveImportResult + Send + 'static);
    pub fn on_post_analysis(&mut self, handler: impl Fn(PostAnalysisParams) -> PostAnalysisResult + Send + 'static);
    pub fn on_format_output(&mut self, handler: impl Fn(FormatOutputParams) -> FormatOutputResult + Send + 'static);
    pub fn start(self);  // begins JSON-RPC read loop on stdin
}
```

Uses `serde_json` + `std::io::BufRead` for the newline-delimited protocol.

**Publishing:** crates.io

---

## 7. AI Integration

### Skills (auto-generated by `statico setup`)

**`statico-plugin-dev`** skill — installed to:
- `.claude/skills/statico-plugin-dev/SKILL.md`
- `.pi/skills/statico-plugin-dev/SKILL.md`

Teaches the LLM:
- Full protocol reference
- How to scaffold a plugin (`statico plugin init`)
- SDK API surface for both TS and Rust
- How to test (`statico plugin test`, `statico plugin run`)
- Common patterns (custom detectors, entry point discovery, import resolution)

### Agent

**`statico-plugin-builder`** agent — installed to:
- `.claude/agents/statico-plugin-builder.md`
- `.pi/agents/statico-plugin-builder.md`

A specialized agent that:
1. Understands the full plugin protocol
2. Can scaffold, implement, test, and debug plugins
3. Runs `statico plugin schema` to verify against the canonical contract
4. Handles both TypeScript and Rust plugin development

### Self-documenting CLI

- `statico plugin docs` — prints full protocol reference (for LLMs reading stdout)
- `statico plugin schema --format json` — machine-readable JSON schema of every message type

---

## 8. Implementation Phases

### Phase 1: Core Plugin Infrastructure
- Plugin discovery (directory scan + TOML config parsing)
- Subprocess management (spawn, stdin/stdout JSON-RPC, lifecycle)
- Hook registration and dispatch (add vs override, conflict detection)
- New CLI subcommands: `plugin list`, `plugin schema`, `plugin docs`

### Phase 2: Plugin SDKs
- `@statico/plugin-sdk` TypeScript package (npm/verdaccio)
- `statico-plugin-sdk` Rust crate
- Both implement the same JSON-RPC read/write loop

### Phase 3: Scaffolding & Build
- `statico plugin init` for TypeScript and Rust
- `statico plugin build` (delegates to `bun build` or `cargo build`)
- `statico plugin test` (runs against fixtures)
- `statico plugin run` (single plugin isolation mode)

### Phase 4: Runtime Management
- Lazy Bun download to `~/.statico/runtimes/bun`
- `statico plugin doctor` — runtime readiness checks
- Runtime version pinning in `.statico.toml`

### Phase 5: Pipeline Integration
- Wire plugins into the existing analysis pipeline
- Additive hooks: merge plugin results with built-in results
- Override hooks: replace built-in stage entirely
- Error handling: plugin timeouts, crash recovery, stderr logging

### Phase 6: AI Integration
- `statico-plugin-dev` skill for Claude Code and pi
- `statico-plugin-builder` agent
- Update `statico setup` to generate plugin dev skill + agent
- Test end-to-end: LLM generates a plugin, statico runs it

---

## 9. Testing Strategy

- **Unit tests:** Protocol serialization, hook dispatch, conflict detection
- **Integration tests:** Full subprocess lifecycle with fixture plugins
- **E2E test:** `statico plugin init --lang typescript` → scaffold → `statico plugin build` → `statico plugin run`
- **Fixture plugins:** Simple `detect-console-log` (TS) and `detect-unwrap` (Rust) used throughout tests
- **Verdaccio:** Local npm registry for SDK package testing

---

## 10. Open Questions

- **Plugin versioning:** Should `.statico.toml` support version pins for plugins?
- **Plugin marketplace:** Central registry (statico.io/plugins)? Or just git repos?
- **Plugin permissions:** Should plugins declare what they need (filesystem read, network, env vars)?
- **Concurrent plugins:** Run multiple `analyze_file` hooks in parallel? Probably yes — each is an independent subprocess.
- **Hot reload:** Watch plugin source and re-run on change during development?
