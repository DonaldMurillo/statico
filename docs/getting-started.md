# Getting Started

**statico** is a static code analyzer for TypeScript and Rust projects. It detects dead code, unused exports and types, circular dependencies, code duplication, framework-specific gotchas, and computes an overall code health score — all from a single fast Rust binary.

---

## Installation

### From source

```bash
git clone https://github.com/nickelc/statico.git
cd statico
cargo build --release
```

The binary installs to `~/.statico/bin/statico`.

## Quick Start

```bash
# Analyze the current directory
statico analyze .

# Analyze with markdown output
statico analyze . --format markdown

# Get JSON output (great for CI)
statico analyze . --format json

# Interactive TUI mode
statico tui .
```

## What It Detects

| Feature | Description |
|---------|-------------|
| **Dead code** | Files unreachable from any entry point |
| **Unused exports** | Named exports never imported elsewhere |
| **Unused types** | Exported TypeScript interfaces/types never referenced |
| **Unused dependencies** | Packages in `package.json` never imported |
| **Circular dependencies** | Import cycles between files |
| **Code duplication** | Similar code blocks across your project |
| **Framework gotchas** | Common error-prone patterns specific to your framework |
| **Health score** | A single 0–100 metric combining issue density and duplication |

## Supported Frameworks

statico automatically detects your framework:

- **Next.js** — pages/app router entries, API routes
- **Angular** — bootstrap modules, lazy routes
- **Vue** — main.ts entries, router pages
- **Svelte** — SvelteKit routes
- **Astro** — pages directory
- **Remix** — route entries
- **NestJS** — module graph entries
- **Payload CMS** — config entries
- **shadcn/ui** — component registry

## Output Formats

```bash
--format json      # Machine-readable JSON
--format markdown  # GitHub-flavored Markdown
--format sarif     # SARIF 2.1.0 (GitHub Code Scanning)
--format html      # Self-contained interactive report
--format ai        # Compact JSON for LLM consumption (~500 tokens)
--format mermaid   # Dependency graph visualization
```

## Configuration

Create a `.statico.toml` in your project root:

```toml
[analysis]
min_confidence = 0.7
ignore = ["**/generated/**", "**/*.d.ts"]

[framework]
name = "nextjs"  # or "auto" for auto-detection
```

## Monorepo Support

statico detects and handles monorepo structures:

- **pnpm workspaces** — `pnpm-workspace.yaml`
- **npm/yarn workspaces** — `workspaces` in root `package.json`
- **Nx** — `nx.json` + workspace config
- **Turborepo** — `turbo.json`

Each package is analyzed independently with its own entry points.

## Next Steps

- [CI/CD Integration](/docs/ci-integration) — Set up statico in your pipeline
- [Plugin System](/docs/plugins) — Extend analysis with custom rules
- [Configuration](/docs/configuration) — Fine-tune analysis settings
