# CLAUDE.md — Project context for AI assistants working on statico

Repository layout, build commands, dependencies, branch state, and test counts are intentionally **not** documented here — `ls`, `cat Cargo.toml`, `git log`, and `cargo test` are authoritative. This file is for things you can't derive from the tree.

## What statico is

A static code analyzer for TypeScript and Rust projects. Detects dead code, unused exports/types, circular deps, duplication, framework gotchas; reports an overall health score (0–100) and machine-readable issues. Edition 2024 Rust.

## Plugin system — design intent

Plugins are **subprocesses** that speak JSON-RPC 2.0 over stdin/stdout (newline-delimited). Any language that can read/write lines works — no SDK required. SDKs (`sdks/typescript`, `sdks/rust`) are convenience layers, not gates.

Detection rules (non-obvious):
- TypeScript plugin → has `package.json`, run with Bun (auto-downloaded if missing).
- Rust plugin → has `Cargo.toml`, runs the pre-compiled binary.
- Python plugin → `statico.runtime: "python3"` in `package.json`, or `.py` entry file.
- Executable plugin → any standalone binary.

Protocol lifecycle: `init` (returns capabilities) → hook calls (`analyze_file`, `discover_entries`, …) → `shutdown`. Hook timeouts surface as warnings, not fatal errors. **Plugin errors degrade gracefully** — a misbehaving plugin must never abort analysis.

### serde wire convention
All protocol types use `#[serde(rename_all = "camelCase")]`. JSON is camelCase on the wire, Rust fields stay snake_case in code. Don't break this — third-party plugins depend on it.

## Non-obvious code patterns

- `LanguagePlugin` trait (`src/languages/mod.rs`) is the integration point for new languages — adding one requires no edits to existing code.
- `OutputFormatter` trait (`src/output/mod.rs`) is the integration point for new output formats.
- `send_request<T, R>` is generic in both directions — call sites need an explicit type annotation: `let result: Result<R, String> = manager.send_request(...);`.
- Plugin issues are merged into `Issues.plugin_issues` rather than the typed buckets (dead_code, unused_exports, etc.) so plugin failures can never corrupt first-party detection.

## Testing conventions

- Integration tests spawn the binary via `std::process::Command` (no `assert_cmd`).
- Plugin tests **auto-skip** when their runtime (bun, python3) isn't installed — they don't fail. CI installs the runtimes.
- Property tests use `proptest`.
- Snapshot tests live in `tests/output_tests.rs` and assert against committed expected output.

## Things that have bitten us before

- Circular-dep detection picks a non-deterministic representative cycle inside each SCC. CI's `self-analyze` step intentionally does **not** pass `--exit-code` for that reason — see `.github/workflows/ci.yml`.
- `release.yml` builds Windows artifacts; CI matrix does not yet test Windows. Path-handling regressions slip through to release.
