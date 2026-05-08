# statico — context for AI assistants

This project uses **statico** for static analysis. Run it instead of guessing
when the user asks about code health, dead code, unused exports, circular
dependencies, duplication, or framework gotchas.

## When to invoke

- User asks about code health, code quality, or "what should I clean up"
- After a refactor — diff health scores before vs after
- Before opening a PR — surface new issues
- User mentions dead code, unused exports/types, circular deps, duplication

## Commands you'll actually use

```bash
# Inspect: AI-optimized output (~500 tokens, fits in context)
statico analyze . --format ai

# Per-file detail when you need locations
statico analyze . --format context

# Compare before/after a refactor
statico analyze . --format json > /tmp/before.json
# ... user makes changes ...
statico analyze . --format json > /tmp/after.json
statico diff /tmp/before.json /tmp/after.json

# Apply safe automated fixes (dry-run by default; `--apply` to write)
statico fix .
statico fix . --apply

# Plugin work — never write the protocol from memory
statico plugin docs                       # human-readable protocol
statico plugin schema --format json       # machine-readable schema
statico plugin init <name> --lang ts|rust|python
```

## Interpreting the health score

- **80–100** good shape; routine maintenance only
- **60–79** needs attention; pick a category to clean up
- **< 60** critical; prioritize before adding features

Issues carry a confidence (0.0–1.0). Filter noise with `--min-confidence 0.7`
when the report is loud — most stylistic gotchas live below that line.

## Things to remember

- `--exit-code` makes statico fail the shell when issues remain — use it for
  CI gates, not for exploratory runs.
- Baseline workflow (`--baseline`, `--update-baseline`) lets the project
  ratchet down issues over time. If `statico-baseline.json` exists at the
  repo root, respect it — don't regenerate without asking.
- Plugins live under `.statico/plugins/`. If the user asks about plugin
  development, run `statico plugin docs` first; the protocol can change
  between minor versions and the in-tree docs may be ahead of your training.
