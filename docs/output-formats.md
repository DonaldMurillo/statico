# Output formats

`statico analyze` emits one of nine formats. Pick with `--format <name>` or set
the default in `.statico.toml`.

> ⚠️ **Pre-1.0.** Field names in the JSON-shaped formats can shift between
> minor releases. The `version` field at the root of `json` and `ai` outputs
> tracks the schema; consumers should check it.

---

## Format selection rules

`statico analyze` picks the default this way:

1. `--format` flag if given.
2. Otherwise, `format = "..."` in `.statico.toml`.
3. Otherwise, `markdown` if stdout is a terminal.
4. Otherwise (piped, redirected), `json`.

So `statico analyze .` in your terminal prints a readable report; piping it
into a file (`> report.txt`) gives you JSON to parse. No flag needed.

---

## Format reference

### `json` (default when piped)

Full enriched analysis: schema version, computed summary, detected
frameworks, monorepo info, full file structure, dependencies, quality
metrics, and every issue category.

```bash
statico analyze . --format json
```

Top-level shape:

```jsonc
{
  "version": "0.2.0",                  // schema version
  "summary": { ... },                  // health_score, totals, issue_counts
  "detected_frameworks": ["nextjs"],
  "monorepo": { "kind": "pnpm", "packages": [...] },
  "structure": { "root": ..., "entry_points": [...], ... },
  "dependencies": { "imports": [...], "external": [...] },
  "quality": { "files": [...] },
  "issues": {
    "dead_code": [...],
    "unused_exports": [...],
    "unused_types": [...],
    "duplicate_exports": [...],
    "duplicate_code": [...],
    "gotchas": [...],
    "circular_dependencies": [...],
    "unused_dependencies": [...],
    "unresolved_imports": [...],
    "unlisted_dependencies": [...],
    "plugin_issues": [...]
  },
  "duplication": {
    "stats": { ... },
    "clone_groups": [...],
    "clone_families": [...],
    "mirrored_directories": [...],
    "repetitive_patterns": [...]
  }
}
```

This is the format `statico diff` consumes — save snapshots before/after a
refactor.

### `markdown` (default on tty)

Human-readable report with tables for each issue category and a header health
dashboard. Same content as the json format, just rendered.

```bash
statico analyze . --format markdown > report.md
```

### `sarif`

SARIF 2.1.0 — the format GitHub Code Scanning, Azure DevOps, and most other
"upload your static-analysis results here" services consume.

```bash
statico analyze . --format sarif > results.sarif
```

GitHub Action snippet:

```yaml
- run: statico analyze . --format sarif > results.sarif
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```

### `html`

Self-contained interactive HTML — no external CSS/JS, no network calls. Open
the file in a browser. Dark/light theme toggle, collapsible sections, sortable
tables.

```bash
statico analyze . --format html > report.html
open report.html
```

### `ai` — LLM-optimized JSON

Compact (~500 token) payload tuned for tool-use loops in Claude / Cursor /
Codex. Includes:

- Health score + issue counts summary
- Top 20 issues ranked by impact (lines of code affected)
- Per-file risk scores with category breakdowns
- A suggested action per issue: `safe-to-delete`, `remove`, `investigate`

```bash
statico analyze . --format ai
```

### `context` — ultra-compact summary

~100 tokens of plain text. Designed for system-prompt injection or pasting
into `AGENTS.md` / `CLAUDE.md`. Single block, no JSON.

```bash
statico analyze . --format context
```

### `pr-comment` — GitHub PR review

GFM with emoji headers, an issue-counts table, top-issues ranked by impact,
circular-dep chains, and a top-10 dead-code list. Ready to post via `gh pr
comment` or a GitHub Action.

```bash
statico analyze . --format pr-comment
```

### `mermaid` — dependency graph

Mermaid flowchart of the import graph with color-coded nodes:

- 🟢 entry points
- 🔴 dead code
- 🟠 issue hotspots (top-5 by total issue count)
- thick red arrows for cycles

Auto-simplifies to the most important nodes when the graph exceeds 30 files.

```bash
statico analyze . --format mermaid > graph.mmd
```

Render with the [Mermaid CLI](https://github.com/mermaid-js/mermaid-cli) or
embed in any GFM-rendering surface (GitHub, GitLab, Notion).

### `fix` — dry-run cleanup hints

Comment-prefixed shell commands you can review and run. Lists high-confidence
dead files (≥80%) safe to delete and unused exports safe to remove. Each line
is a `git show` so you can inspect before committing.

```bash
statico analyze . --format fix
```

For *applying* fixes, use `statico fix` (the subcommand), not the format.

---

## Choosing the right format

| You want to... | Use |
|---|---|
| read the report yourself | `markdown` (default on tty) |
| upload to GitHub Code Scanning | `sarif` |
| post a PR comment | `pr-comment` |
| feed an AI assistant | `ai` (tool calls) or `context` (system prompt) |
| diff two snapshots | `json` (input to `statico diff`) |
| share an offline report | `html` |
| visualize the dep graph | `mermaid` |
| inspect cleanup candidates | `fix` (or `statico fix .` for dry-run) |
| pipe into another tool | `json` |
