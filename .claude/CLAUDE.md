# statico

## Project Overview

statico is a static code analyzer for TypeScript and Rust projects. It detects dead code,
unused exports, circular dependencies, code duplication, and framework-specific issues.

## Architecture

- `src/analyzer/` — Core analysis engine with language plugin system
- `src/languages/` — Language plugins (TypeScript, Rust) implementing `LanguagePlugin` trait
- `src/issues/` — Issue detectors (dead code, unused exports, circular deps, gotchas)
- `src/resolution/` — Import resolution (TypeScript paths, tsconfig, Rust mod/crate)
- `src/output/` — Output formatters (JSON, SARIF, Markdown, AI, context, mermaid)
- `src/discovery/` — Entry point discovery (Next.js, Payload CMS, Angular)
- `src/tui/` — Terminal UI dashboard

## Key Commands

```bash
statico analyze .                    # Analyze project
statico analyze . --format markdown  # Markdown output
statico analyze . --format ai        # AI-optimized output
statico analyze . --exit-code        # Exit 1 on issues (CI)
statico diff before.json after.json  # Compare analyses
statico tui .                        # Interactive dashboard
statico doctor                       # Diagnose installation
```

## Output Formats

- `json` — Full structured analysis
- `sarif` — SARIF 2.1.0 for GitHub Code Scanning
- `markdown` — Human-readable report
- `ai` — Compressed format optimized for LLM context windows
- `context` — File-by-file summary with issue locations
- `mermaid` — Dependency graph visualization
- `pr-comment` — GitHub PR review comment
- `fix` — Machine-readable fix suggestions

## Development

```bash
cargo build                           # Build
cargo test                            # Run all tests
cargo test --test integration         # Integration tests
cargo bench                           # Benchmarks
cargo run -- analyze . --format json  # Dev run
```

## Language Plugin System

Adding a new language:
1. Create `src/languages/<lang>.rs` implementing `LanguagePlugin` trait
2. Register extensions in `from_path()` (patterns.rs)
3. Optionally add language-specific rules

No existing code needs modification.
