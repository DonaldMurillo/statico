# Output Formats

statico supports multiple output formats for different use cases.

---

## Terminal (Default)

Human-readable terminal output with color-coded severity levels.

```bash
statico analyze .
```

## JSON

Machine-readable JSON output, ideal for tooling integration.

```bash
statico analyze . --format json
```

```json
{
  "summary": {
    "health_score": 82,
    "total_files": 156,
    "dead_files": 3,
    "unused_exports": 12,
    "circular_dependencies": 2,
    "duplicate_blocks": 5
  },
  "issues": [
    {
      "type": "dead_code",
      "file": "src/legacy/utils.ts",
      "confidence": 0.95,
      "message": "File is unreachable from any entry point"
    }
  ]
}
```

## Markdown

GitHub-flavored Markdown for PR comments and documentation.

```bash
statico analyze . --format markdown
```

## SARIF 2.1.0

Static Analysis Results Interchange Format for GitHub Code Scanning and Azure DevOps.

```bash
statico analyze . --format sarif
```

Integrates directly with GitHub Actions:

```yaml
- name: Run statico
  run: statico analyze . --format sarif > results.sarif

- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```

## HTML

Self-contained interactive HTML report with dark/light theme support.

```bash
statico analyze . --format html > report.html
```

Features:
- Interactive issue browser
- Dependency graph visualization
- Duplicate code inspector
- Dark/light theme toggle

## AI-Optimized

Compact JSON payload designed for LLM consumption (~500 tokens).

```bash
statico analyze . --format ai
```

Useful for feeding analysis results into AI coding assistants for context-aware suggestions.

## Mermaid

Dependency graph visualization in Mermaid diagram format.

```bash
statico analyze . --format mermaid
```

Produces color-coded flowcharts:
- 🟢 **Green** — Entry points
- 🔴 **Red** — Dead code
- 🟡 **Yellow** — High-dependency hotspots

## PR Comment

GitHub-flavored Markdown formatted specifically for PR review comments.

```bash
statico analyze . --format pr-comment
```
