# CLAUDE.md — Project context for AI assistants working on statico

## Project Overview

**statico** is a static code analyzer for TypeScript and Rust, written in Rust (Edition 2024).
It detects dead code, unused exports/types, circular dependencies, code duplication, and framework-specific gotchas, computing an overall code health score (0–100).

## Architecture

```
src/
├── main.rs            CLI entry point, argument parsing, command dispatch
├── lib.rs             Public API re-exports
├── analyzer/          High-level analysis orchestration
├── cache.rs           Incremental analysis cache (content hashing)
├── config.rs          .statico.toml configuration loading
├── discovery/         Source file & entry point discovery
├── duplication/       Code duplication detection (clone groups, mirrored dirs)
├── frameworks/        Framework profiles (Next.js, Angular, Vue, etc.)
├── issues/            Issue detectors (dead code, unused exports, gotchas, etc.)
├── languages/         Language plugins (TypeScript, Rust) — LanguagePlugin trait
├── output/            Output formatters (json, sarif, markdown, html, ai, mermaid, etc.)
├── parse/             AST parsing (oxc for TS, syn for Rust)
├── plugin/            Plugin system (discovery, manager, pipeline, protocol, runtime)
├── resolution/        Import resolution (relative, tsconfig paths, workspace packages)
├── types.rs           Shared types (AnalysisOutput, Summary, Issues, etc.)
├── update.rs          Self-update mechanism
├── progress.rs        Progress reporting
└── tui.rs             Interactive terminal UI

sdks/
├── typescript/        TypeScript plugin SDK (JSON-RPC 2.0 over stdin/stdout)
└── rust/              Rust plugin SDK (same protocol)

tests/
├── integration.rs     CLI integration tests (40 tests including plugin e2e)
├── output_tests.rs    Snapshot tests for output formatters
├── property_tests.rs  Property-based tests (fuzzer)
├── fixtures/          Test fixture projects
│   ├── plugin-demo/   TypeScript plugin (no-console-log)
│   └── python-demo/   Python plugin (no-bare-except)
```

## Build & Run

```bash
cargo build --release          # Build optimized binary
cargo test                     # Run all 413 tests
cargo clippy                   # Lint
cargo run --bin statico -- analyze . --format markdown
```

The binary installs to `~/.statico/bin/statico`.

## Plugin System

Subprocess-based: plugins communicate via JSON-RPC 2.0 over stdin/stdout (newline-delimited).
Supports TypeScript (Bun runtime, auto-downloaded), Rust (system cargo), and Python (system python3).
Any language that can read/write lines on stdin/stdout works — no SDK required.

Key files: `src/plugin/{protocol,discovery,manager,pipeline,runtime}.rs`

### Plugin kinds
- **TypeScript** — detected by `package.json`, run with Bun
- **Rust** — detected by `Cargo.toml`, pre-compiled binary
- **Python** — detected by `package.json` `statico.runtime: "python3"` or `.py` entry files
- **Executable** — any standalone binary

### Protocol
1. `init` → plugin returns capabilities (name, hooks, languages, rules)
2. Hook calls (`analyze_file`, `discover_entries`, etc.) → plugin returns results
3. `shutdown` → clean exit

### serde convention
All protocol types use `#[serde(rename_all = "camelCase")]` — JSON fields are camelCase on the wire, Rust fields are snake_case in code.

## Testing Conventions

- Integration tests use `assert_cmd`-style via raw `std::process::Command`
- Plugin tests auto-skip if runtime (bun/python3) not installed
- Property tests use `quickcheck`
- All tests must pass: `cargo test`

## Key Dependencies

- `tree-sitter` + `tree-sitter-typescript` + `tree-sitter-rust` — primary AST parsing for TS/Rust
- `oxc_resolver` — optional, gated behind the `deep-resolution` cargo feature for tsconfig path resolution; the default build does not pull it in
- `rayon` — parallel file analysis
- `serde_json` — all I/O is JSON
- `clap` — CLI argument parsing
- `toml` — config file parsing

## Important Patterns

- `LanguagePlugin` trait in `src/languages/mod.rs` — pluggable language support
- `OutputFormatter` trait in `src/output/mod.rs` — pluggable output formats
- Plugin pipeline: `PluginPipeline` in `src/plugin/pipeline.rs` manages lifecycle during analysis
- Plugin errors are warnings, not fatal — graceful degradation
- `send_request<T, R>` generic needs type annotation: `let result: Result<R, String> = ...`

## Branch State

- `main` — stable release branch
- `feature/plugin-system` — plugin system (current work, 17 commits ahead of main)
