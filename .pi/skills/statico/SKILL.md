---
name: statico
description: Rust CLI static analyzer for TypeScript and Rust projects. Detects dead code, unused exports/types/dependencies, circular dependencies, code duplication, and framework-specific issues. Supports AI-optimized output formats, auto-fix suggestions, and Mermaid dependency graphs. Use when analyzing code health, measuring refactoring impact, setting up CI code-quality gates, or exploring unfamiliar codebases.
trigger: statico, static analysis, dead code, unused exports, code health, refactoring impact, SARIF, code duplication, TypeScript analysis, Rust analysis, ai format, mermaid, pr-comment
---

# statico — Static Analyzer for TypeScript & Rust

A Rust CLI tool that statically analyzes TypeScript and Rust projects for dead code, unused exports, circular dependencies, duplication, and more. Auto-detects frameworks (Next.js, Payload CMS, Angular, NestJS) for accurate entry-point-aware analysis.

## Building

```bash
cd /Users/dom/programming/statico && cargo build --release --features deep-resolution
# Binary: target/release/statico
```

## Commands

```bash
# Analyze a project (default: JSON to stdout)
statico analyze <path>

# Output formats
statico analyze <path> --format html        # Interactive HTML report (open in browser)
statico analyze <path> --format markdown    # Markdown summary for terminals/PRs
statico analyze <path> --format sarif       # SARIF for GitHub Code Scanning
statico analyze <path> --format ai          # Compact LLM-optimized JSON (~500 tokens)
statico analyze <path> --format context     # Ultra-compact one-liner (~100 tokens) for system prompts
statico analyze <path> --format mermaid     # Dependency graph as Mermaid diagram
statico analyze <path> --format pr-comment  # GitHub-ready PR comment with emoji tables

# Auto-fix suggestions (dry run)
statico analyze <path> --fix --dry-run      # Unified diff patches, safe to pipe to git apply --check

# Filters & CI
statico analyze <path> --min-confidence 0.8  # Suppress low-confidence findings
statico analyze <path> --exit-code            # Exit 1 if issues found (CI gating)

# Compare two snapshots (before/after refactoring)
statico diff before.json after.json
```

## Language Support

| Language | Dead Code | Unused Exports | Circular Deps | Duplication | Framework Detection |
|----------|-----------|----------------|---------------|-------------|---------------------|
| **TypeScript** | ✅ | ✅ | ✅ | ✅ | Next.js, Payload CMS, Angular, NestJS |
| **Rust** | ✅ | ✅ (pub items) | ✅ | ✅ | — |

## When to Use

| Situation | Action |
|---|---|
| **Before/after refactoring** | `analyze` → save JSON → refactor → `analyze` → `diff` |
| **Code review** | `analyze --format markdown` to spot new dead code or unused exports |
| **PR comment** | `analyze --format pr-comment` for GitHub-ready output |
| **Adding features** | Run `analyze` to verify no regressions introduced |
| **CI/CD setup** | `--format sarif` for GitHub Code Scanning, `--exit-code` for pass/fail |
| **Exploring a codebase** | `--format html` for an interactive overview |
| **AI agent context** | `--format ai` for LLM-optimized JSON, `--format context` for system prompts |
| **Dependency visualization** | `--format mermaid` for import graph with dead code islands |
| **Auto-fix cleanup** | `--fix --dry-run` to generate safe patches |

## Understanding Results

- **Confidence scores**: `0.95+` = high (unreachable from any entry point). `~0.7` = medium (not reachable from framework entry points but reachable from scripts/migrations/tests). Filter with `--min-confidence`.
- **Issues are informational** — not pass/fail. A codebase with 0 issues is unusual.
- **Duplication %** is informational. 10–20% is typical for mature codebases.
- **Health score**: 0–100, weighted by issue severity.

## AI Output Formats

### `--format ai` — LLM-Optimized JSON

Compact JSON (~500 tokens) with only actionable data. Issues ranked by impact (LOC wasted). Includes suggested action per issue.

```jsonc
{
  "summary": {
    "health_score": 53.9,
    "total_files": 412,
    "issue_counts": { "dead_code": 12, "unused_exports": 847 }
  },
  "top_issues": [
    {
      "type": "dead_code",
      "path": "src/legacy/utils.ts",
      "lines_of_code": 340,
      "confidence": 0.98,
      "impact": "high",
      "suggested_action": "Delete file — unreachable from any entry point"
    }
  ],
  "files_at_risk": [
    { "path": "src/legacy/index.ts", "issue_count": 14 }
  ]
}
```

### `--format context` — Ultra-Compact Summary

~100 tokens. Designed for AGENTS.md injection or system prompt context.

```
Health: 53.9/100 | 412 files | Dead code: 12 (3,401 LOC) | Unused exports: 847 | Circles: 3 | Dup: 18.2% | Top risk: src/legacy/index.ts (14 issues)
```

### `--format mermaid` — Dependency Graph

Mermaid diagram showing module dependencies, circular deps highlighted in red, dead code islands in gray.

### `--format pr-comment` — GitHub PR Comment

GitHub-flavored Markdown with emoji tables, before/after comparison support, and actionable items grouped by severity.

### `--fix --dry-run` — Auto-Fix Patches

Outputs unified diff patches for safe auto-fixes:
- Dead file removal patches
- Unused export removal hints
- Safe to pipe to `git apply --check` for validation

## JSON Output Schema (default `--format json`)

```jsonc
{
  "version": "0.2.0",
  "$schema": "...",
  "summary": {
    "total_files": 0,
    "total_lines": 0,
    "total_exports": 0,
    "total_types": 0,
    "issue_counts": { /* per-category counts */ },
    "health_score": 0,        // 0–100
    "duplication_percentage": 0.0
  },
  "structure": { /* file tree, module graph */ },
  "dependencies": { /* import graph, dependency tree */ },
  "quality": { /* metrics per file */ },
  "issues": {
    "dead_code": [],
    "unused_exports": [],
    "unused_types": [],
    "duplicate_code": [],
    "gotchas": [],
    "circular_dependencies": [],
    "unused_dependencies": [],
    "duplicate_exports": [],
    "unresolved_imports": [],
    "unlisted_dependencies": []
  },
  "duplication": { /* clone pairs, percentages */ },
  "detected_frameworks": []  // e.g. ["nextjs", "payload-cms"]
}
```

## Framework Detection

Statico auto-detects these frameworks and adjusts entry-point conventions for more accurate dead code analysis:

- **Next.js** — pages router, app router, API routes, middleware
- **Payload CMS** — config, collections, blocks, field-level hooks
- **Angular** — modules, components, services, guards, resolvers
- **NestJS** — modules, controllers, providers, guards, interceptors

## AI Workflow Examples

### One-command code health check (for AI agents)

```bash
/Users/dom/programming/statico/target/release/statico analyze . --format ai --quiet
```

### Check if refactoring introduced dead code

```bash
/Users/dom/programming/statico/target/release/statico analyze ./src > /tmp/before.json
# ... make changes ...
/Users/dom/programming/statico/target/release/statico analyze ./src > /tmp/after.json
/Users/dom/programming/statico/target/release/statico diff /tmp/before.json /tmp/after.json
```

### Generate context for system prompt injection

```bash
/Users/dom/programming/statico/target/release/statico analyze . --format context --quiet
# Paste output into AGENTS.md or CLAUDE.md
```

### Auto-fix suggestions (review before applying)

```bash
/Users/dom/programming/statico/target/release/statico analyze . --fix --dry-run --quiet
# Review patches, then: git apply < patch.diff
```

### PR comment for code review

```bash
/Users/dom/programming/statico/target/release/statico analyze . --format pr-comment --quiet
# Paste into GitHub PR comment
```

### Dependency graph visualization

```bash
/Users/dom/programming/statico/target/release/statico analyze . --format mermaid --quiet
# Paste into any Markdown renderer that supports Mermaid
```

### Generate a shareable code health report

```bash
/Users/dom/programming/statico/target/release/statico analyze ./src --format html --output report.html
# Share report.html with the team
```

### Set up GitHub Actions CI

```yaml
- name: Static analysis
  run: target/release/statico analyze ./src --format sarif --output results.sarif --exit-code
- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```
