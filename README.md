# statico

**Static code analyzer for TypeScript and Rust projects.**

[![Rust Edition 2024](https://img.shields.io/badge/rust-2024-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-2024-edition.html)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

statico detects dead code, unused exports and types, circular dependencies, code duplication, framework-specific gotchas, and computes an overall code health score — all from a single fast Rust binary.

---

## What It Does

statico performs whole-project static analysis by parsing your source files, resolving imports, and tracing reachability from framework entry points. It reports:

- **Dead code** — files unreachable from any entry point
- **Unused exports** — named exports never imported elsewhere
- **Unused types** — exported TypeScript interfaces/types never referenced
- **Unused dependencies** — packages in `package.json` never imported
- **Circular dependencies** — import cycles between files
- **Duplicate code** — similar code blocks across your project
- **Framework gotchas** — common error-prone patterns specific to your framework
- **Code health score** — a single number from 0–100 summarizing project health

## Features

- 🔍 **Dead code detection** — identifies files unreachable from any entry point with confidence scoring
- 📦 **Unused export detection** — finds named exports never imported by any file
- 📝 **Unused type detection** — flags exported TypeScript types/interfaces never referenced
- 🔄 **Circular dependency analysis** — traces import cycles with full chain reporting
- 📋 **Code duplication detection** — finds similar code blocks, clone groups, and mirrored directories
- ⚠️ **Framework-specific gotchas** — detects common error-prone patterns (e.g. conditional hooks, missing keys)
- 🧩 **Automatic framework detection** — recognizes Next.js, Angular, Vue, Svelte, Astro, Remix, NestJS, Payload CMS, and shadcn/ui projects
- 📊 **Code health score** — a single 0–100 metric combining issue density and duplication
- 🏗️ **Monorepo support** — detects pnpm, npm/yarn, Nx, and Turborepo workspaces
- 🖥️ **Interactive TUI** — terminal dashboard for exploring issues
- 📐 **Dependency graphs** — Mermaid flowcharts with color-coded nodes (entry points, dead code, hotspots)
- 🤖 **AI-optimized output** — compact JSON payload designed for LLM consumption (~500 tokens)
- 🔧 **Auto-fix suggestions** — dry-run mode for safe, actionable fix hints
- 📄 **SARIF 2.1.0 output** — integrates with GitHub Code Scanning and Azure DevOps
- 🎨 **HTML interactive report** — self-contained dark/light theme HTML dashboard
- 💬 **PR comment format** — GitHub-flavored Markdown ready for PR review comments
- ⚡ **Parallel analysis** — multi-threaded parsing via Rayon
- 🧩 **Plugin system** — extend analysis with custom rules in any language (TypeScript, Python, Rust, or any executable)

## Installation

### Cargo

```bash
cargo install statico
```

### npm (macOS / Linux × x86_64 / aarch64)

```bash
npm install -D @statico/cli
npx statico analyze .
```

### Prebuilt release

Download a tarball from the [latest release](https://github.com/DonaldMurillo/statico/releases/latest) and extract:

```bash
# Example: macOS arm64
curl -fsSL https://github.com/DonaldMurillo/statico/releases/latest/download/statico-macos-aarch64.tar.gz \
  | tar -xz
sudo install -m 0755 statico /usr/local/bin/statico
```

> **macOS quarantine note:** if you downloaded the tarball with a browser
> (Safari/Chrome/Firefox) the binary is marked unsigned and Gatekeeper
> refuses to run it with an "unidentified developer" dialog. Clear the
> quarantine flag once with:
>
> ```bash
> xattr -d com.apple.quarantine /usr/local/bin/statico
> ```
>
> The `npx`, `cargo install`, and `curl | tar` paths above do not set the
> quarantine attribute, so they don't hit this dialog.

### GitHub Action

```yaml
- uses: DonaldMurillo/statico/.github/actions/statico@main
  with:
    format: sarif
    min-confidence: '0.5'
```

### From source

```bash
git clone https://github.com/DonaldMurillo/statico.git
cd statico
cargo install --path .
```

### Prerequisites (build from source only)

- [Rust](https://rustup.rs/) 1.85+ (Edition 2024)

## Quick Start

Analyze a TypeScript project:

```bash
statico analyze ./my-project --format markdown
```

Get a compact health summary for AI consumption:

```bash
statico analyze ./my-project --format ai
```

Fail CI if issues are found:

```bash
statico analyze ./my-project --format sarif --min-confidence 0.7 --exit-code
```

Compare two snapshots (before/after refactoring):

```bash
statico analyze ./my-project --format json > before.json
# ... make changes ...
statico analyze ./my-project --format json > after.json
statico diff before.json after.json --format markdown
```

Explore issues interactively:

```bash
statico tui ./my-project
```

## CLI Reference

```
statico [OPTIONS] <COMMAND>

Commands:
  analyze  Analyze a TypeScript project
  tui      Show interactive terminal dashboard
  diff     Compare two analysis outputs
  plugin   Plugin management (init, build, run, list, doctor, schema, docs)

Options:
  --quiet   Suppress progress output

Common Options:
  -h, --help     Print help
  -V, --version  Print version
```

### `statico analyze`

```
statico analyze <PATH> [OPTIONS]
```

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `string` | `json` (or config file value) | Output format: `json`, `sarif`, `markdown`, `html`, `ai`, `context`, `mermaid`, `pr-comment`, `fix` |
| `--min-confidence` | `float` | `0.0` (or config file value) | Minimum confidence threshold (0.0–1.0) for filtering issues |
| `--exit-code` | `flag` | `false` | Exit with code 1 if issues are found above `--min-confidence` |
| `--no-cache` | `flag` | `false` | Disable the incremental cache and force a full re-parse of all files |

### `statico tui`

```
statico tui <PATH> [OPTIONS]
```

| Flag | Type | Default | Description |
|---|---|---|---|
| `--min-confidence` | `float` | `0.5` | Minimum confidence threshold for displayed issues |

### `statico diff`

```
statico diff <BEFORE> <AFTER> [OPTIONS]
```

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `string` | `json` | Diff output format: `json` or `markdown` |

Exits with code 1 if there are new issues in the `after` snapshot.

## Output Formats

### `json` (default) — Enriched JSON

Full analysis output with schema version, computed summary, detected frameworks, and all issue details. Suitable for piping into other tools or saving as a snapshot for `statico diff`.

```bash
statico analyze . --format json
```

### `sarif` — SARIF 2.1.0

Industry-standard Static Analysis Results Interchange Format. Integrates with GitHub Code Scanning, Azure DevOps, and other SARIF consumers.

```bash
statico analyze . --format sarif > results.sarif
```

### `markdown` / `md` — Markdown Report

Human-readable report with tables for dead code, unused exports, duplication stats, circular dependencies, and a health dashboard with progress bar.

```bash
statico analyze . --format markdown > report.md
```

### `html` — Interactive HTML Report

Self-contained HTML file with dark/light theme toggle, collapsible sections, file heat map, and sortable tables. No external dependencies — works offline.

```bash
statico analyze . --format html > report.html
```

### `ai` — LLM-Optimized JSON

Compact schema-versioned JSON payload (~500 tokens) with the top 20 most impactful issues, per-file risk scores, and suggested actions (`safe-to-delete`, `remove`, `investigate`). Designed for LLM tool calls and AI-assisted code review.

```bash
statico analyze . --format ai
```

### `context` — Ultra-Compact One-Liner

~100 tokens of plain text summarizing code health. Intended for system prompt injection or `AGENTS.md` context.

```bash
statico analyze . --format context
```

### `pr-comment` — GitHub PR Comment

GitHub-flavored Markdown with emoji indicators, issue breakdown table, top issues ranked by impact, and circular dependency chains. Ready to post as a PR review comment.

```bash
statico analyze . --format pr-comment
```

### `mermaid` — Dependency Graph

Renders the project dependency graph as a [Mermaid](https://mermaid.js.org/) flowchart. Nodes are color-coded:

- 🟢 **Green** — entry points
- 🔴 **Red** — dead code
- 🟠 **Orange** — issue hotspots
- Thick red arrows — circular dependencies

Automatically simplifies large graphs (>30 files) by selecting the most important nodes.

```bash
statico analyze . --format mermaid > graph.mmd
```

### `fix` — Dry-Run Fix Suggestions

Comment-prefixed hints listing high-confidence dead files (≥80%) safe to delete and unused exports safe to remove. Each suggestion includes a `git show` command for review.

```bash
statico analyze . --format fix
```

## Configuration

statico reads optional configuration from `.statico.toml` in your project root. CLI flags override config values.

```toml
# .statico.toml

# Default output format (overridden by --format)
format = "json"

# Minimum confidence threshold 0.0–1.0 (overridden by --min-confidence)
min_confidence = 0.0

# Exit with code 1 if issues found (overridden by --exit-code)
exit_code = false

# Suppress progress output (overridden by --quiet)
quiet = false

# Glob patterns to exclude from analysis
exclude = ["node_modules", "dist", "build", "**/*.generated.ts"]

# Glob patterns to include (overrides exclude)
include = ["src/**/*.ts", "src/**/*.tsx"]

# Maximum file size in bytes to analyze (skip larger files)
max_file_size = 1_000_000

# Number of threads (0 = auto-detect)
threads = 0
```

When no `.statico.toml` is present, statico uses sensible defaults (format: `json`, all source files included, auto-threading).

## Plugins

statico supports plugins that extend analysis with custom rules. Plugins run as subprocesses communicating via JSON-RPC 2.0 over stdin/stdout — **any language** works.

```bash
# Scaffold a new plugin
statico plugin init my-rule --lang typescript
statico plugin init my-rule --lang rust

# Build a Rust plugin
statico plugin build --name my-rule

# Run a plugin against a file
statico plugin run my-rule --file src/index.ts

# List discovered plugins
statico plugin list

# Check runtime health
statico plugin doctor
```

Plugins are auto-discovered from `.statico/plugins/` in your project root. See [docs/plugins.md](docs/plugins.md) for the full guide including TypeScript SDK, Python (no SDK), Rust SDK, protocol reference, and AI-assisted development.

## Framework Profiles

statico automatically detects your framework and adjusts entry point detection, implicit entries, and gotcha rules. A project can match multiple profiles (e.g. Next.js + Payload CMS + shadcn/ui).

| Framework | Detection | Entry Points | Gotcha Rules |
|---|---|---|---|
| **Next.js** | `next.config.*` | `page.tsx`, `layout.tsx`, `route.ts`, `loading.tsx`, `error.tsx`, `not-found.tsx` | Conditional hooks, missing keys |
| **Angular** | `angular.json` | Component, directive, pipe, service, module files | — |
| **Vue** | `vue.config.*`, `nuxt.config.*`, `vite.config.*` + Vue plugin | `.vue` SFCs | — |
| **Svelte** | `svelte.config.*` | `.svelte` components, `+page.svelte`, `+layout.svelte` | — |
| **Astro** | `astro.config.*` | `.astro` pages, layouts | — |
| **Remix** | `remix.config.*` | Route files in `app/routes/` | — |
| **NestJS** | `nest-cli.json` | Controllers, modules, services, guards, pipes | — |
| **Payload CMS** | `payload.config.*` | Collections, blocks, globals, field configs | — |
| **shadcn/ui** | `components.json` | UI component files | — |
| **Generic** | *(always active)* | Test files, e2e specs, scripts | — |

Detection works by checking for marker files and `package.json` dependencies. For monorepos, statico also scans workspace package directories.

## AI Integration

statico is designed to work alongside AI coding assistants:

### `ai` Output Format

The `--format ai` flag produces a compact, schema-versioned JSON payload optimized for LLM tool calls. It includes:

- A health score and issue counts summary
- The top 20 most impactful issues ranked by lines of code affected
- Per-file risk scores with category breakdowns
- Suggested actions: `safe-to-delete`, `remove`, or `investigate`

This format fits within ~500 tokens and is ideal for feeding into AI code review workflows.

### `context` Output Format

The `--format context` flag produces an ultra-compact (~100 token) plain-text summary suitable for injecting into system prompts or `AGENTS.md` files.

### Pi Skills

The project includes [Pi](https://github.com/DonaldMurillo/pi) skills for common workflows:

| Skill | Description |
|---|---|
| **`analyze`** | Run a code health analysis and present a human-readable summary |
| **`dead-code-cleanup`** | Interactive dead code cleanup — identify, review, and safely remove dead files |
| **`refactor-impact`** | Measure refactoring impact by comparing before/after analysis snapshots |

These skills enable AI assistants to use statico as a tool for code quality analysis, cleanup, and refactoring verification.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
