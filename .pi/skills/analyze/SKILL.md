---
name: analyze
description: Run a statico code health analysis on the current project. Presents a human-readable summary of dead code, unused exports, circular dependencies, and overall health score. Use when asked to check code health, find dead code, or review code quality.
---

# /analyze — Code Health Check

## Setup
Build statico if not already built:
```bash
cd /Users/dom/programming/statico && cargo build --release --features deep-resolution
```

## Workflow
1. Run analysis:
```bash
/Users/dom/programming/statico/target/release/statico analyze . --format ai --quiet
```

2. Parse the JSON output and present:
   - Health score (🟢 80+, 🟡 60-79, 🔴 <60)
   - Top issues by impact
   - Suggested actions

3. If `.statico/last-analysis.json` exists, compare trends.

4. Save current run: redirect JSON output to `.statico/last-analysis.json`

## Output Format
The `--format ai` JSON has:
- `summary`: health_score, total_files, issue_counts
- `top_issues`: ranked list with suggested_action, confidence, impact
- `files_at_risk`: files with most issues

## Example Commands
```bash
# Full AI-optimized analysis
/Users/dom/programming/statico/target/release/statico analyze . --format ai --quiet

# Markdown report for terminals
/Users/dom/programming/statico/target/release/statico analyze . --format markdown --quiet

# Compact context for system prompts
/Users/dom/programming/statico/target/release/statico analyze . --format context --quiet

# Fix suggestions (dry run)
/Users/dom/programming/statico/target/release/statico analyze . --fix --dry-run --quiet
```
