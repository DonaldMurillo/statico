# Configuration

statico reads `.statico.toml` from the project root. Every key is optional —
with no config file, the defaults below are used. CLI flags always win over
config values.

> ⚠️ **Pre-1.0.** The config schema can change between minor releases. Pin a
> statico version if you embed `.statico.toml` in CI.

---

## Quick example

```toml
# .statico.toml — all keys optional
format = "markdown"
min_confidence = 0.5
exit_code = false

exclude = [
  "vendor/**",
  "**/*.generated.ts",
]

# Plugin auto-discovery from .statico/plugins/ stays on by default.
plugin_auto_discover = true

# Plus an explicit plugin entry with settings.
[[plugin]]
name = "coverage-gap"
path = "./plugins/coverage-gap"
languages = ["typescript", "tsx"]

[plugin.settings]
test_globs = ["**/*.test.ts", "**/*.spec.ts"]
strict = true
```

---

## Top-level keys

All keys live at the top level of `.statico.toml`. There are no `[analysis]`
or `[output]` sections — the schema is intentionally flat.

| Key | Type | Default | Description |
|---|---|---|---|
| `format` | string | `json` (or `markdown` if stdout is a terminal) | Default output format. Same set as `--format`: `json`, `sarif`, `markdown`, `html`, `ai`, `context`, `mermaid`, `pr-comment`, `fix`. |
| `min_confidence` | float | `0.0` | Drop issues below this confidence (0.0–1.0). Clamped to that range; NaN and out-of-range values are coerced. |
| `exit_code` | bool | `false` | Exit 1 from `statico analyze` if any issue passes the confidence + baseline filter. |
| `quiet` | bool | `false` | Suppress progress + info lines on stderr. |
| `exclude` | string[] | `[]` | Glob patterns to skip. The built-in skip list (`node_modules`, `.git`, `target`, `dist`, `build`, `.next`, `.angular`, `.svelte-kit`, `.vite`, `__pycache__`, etc.) always applies on top of this. |
| `include` | string[] | `[]` | Glob patterns that override `exclude`. |
| `max_file_size` | integer | `1_000_000` | Files larger than this (bytes) are skipped. Hard-capped at 50 MB internally — values beyond that are clamped. |
| `threads` | integer | `0` | Worker thread count. `0` means auto-detect. |
| `plugin_auto_discover` | bool | `true` | When `false`, only plugins explicitly listed in `[[plugin]]` arrays are loaded. |
| `plugin` | array of tables | `[]` | Explicit plugin entries (see below). Merged with auto-discovery results. |

## `[[plugin]]` entries

Each entry is a TOML table inside an array (`[[plugin]]` syntax). Keys:

| Key | Type | Default | Description |
|---|---|---|---|
| `name` | string | required | Plugin identifier. Must match the directory name under `.statico/plugins/` if you're augmenting an auto-discovered plugin. |
| `path` | string | — | Path to a plugin directory (relative to project root). Required if the plugin isn't already under `.statico/plugins/`. Path-traversal is rejected — `path` must resolve inside the project root. |
| `enabled` | bool | `true` | Set to `false` to disable a plugin without removing it. |
| `override` | bool | `false` | When `true`, every hook this plugin registers is treated as `override` mode regardless of what the plugin declares. Two plugins can't `override` the same hook. |
| `languages` | string[] | `[]` | Restrict the plugin to specific languages. Empty array means all languages. Match values: `typescript`, `tsx`, `javascript`, `jsx`, `rust`, `python`. |
| `settings` | table | `{}` | Free-form settings forwarded to the plugin's `init` call as `plugin_settings`. The plugin parses whatever it expects — statico just passes it through. Capped at 64 KB serialized JSON; nesting capped at 32 levels. |

```toml
[[plugin]]
name = "coverage-gap"
path = ".statico/plugins/coverage-gap"
enabled = true
languages = ["typescript", "tsx"]

[plugin.settings]
test_globs = ["**/*.test.ts", "**/*.spec.ts"]
exclude_dirs = ["vendor"]
strict = true
```

---

## Defaults the file does **not** override

Two important behaviors are not configurable today (intentional):

- **Built-in skip list.** Directories like `node_modules`, `.git`, `target`,
  `dist`, `build`, `.next`, `.nuxt`, `.angular`, `.svelte-kit`, `.vite`,
  `.parcel-cache`, `.cache`, `__pycache__`, `.terraform`, etc., are always
  skipped before `exclude` is even consulted. Source: `is_skipped_dir` in
  `src/discovery/mod.rs`.
- **Path safety.** Plugin paths and CLI paths are canonicalized and rejected
  if they escape the project root, regardless of config. Source:
  `src/path_safety.rs`.

If those don't fit your use case, that's a real bug — open an issue.

---

## CLI flag precedence

For every key in the table above, the CLI flag wins over the config value,
which wins over the built-in default:

```
CLI flag > .statico.toml > built-in default
```

A few CLI flags don't have a config equivalent (because they're inherently
per-invocation):

- `--no-cache` — force a full re-parse
- `--baseline <path>` — gate against a fingerprint file
- `--update-baseline <path>` — write a baseline and exit
- `--watch` — re-run on file changes
- `--apply` (on `statico fix`) — actually rewrite files

---

## Common recipes

### CI gate: fail on new issues only

```toml
# .statico.toml
exit_code = true
min_confidence = 0.7
```

```bash
statico analyze . --baseline statico-baseline.json
```

The first time you set this up, generate the baseline:

```bash
statico analyze . --update-baseline statico-baseline.json --min-confidence 0.7
```

Commit `statico-baseline.json`. From then on, any new issue above 0.7
confidence breaks the build.

### Monorepo: only analyze one workspace

```toml
exclude = [
  "packages/legacy/**",
  "apps/old-frontend/**",
]
```

Or analyze a subdirectory directly:

```bash
statico analyze ./packages/api
```

### Skip the gotcha noise

Most gotchas fire below 0.7 confidence. To get a clean default report:

```toml
min_confidence = 0.7
```

The 0.0–0.69 range is mostly framework hints and stylistic flags — useful when
you go looking for them, noisy when you're not.

### Disable plugins

If you don't write plugins and don't want statico scanning `.statico/plugins/`:

```toml
plugin_auto_discover = false
```

Explicitly listed `[[plugin]]` entries are still respected.

---

## Validation behavior

statico is forgiving about config errors:

- Missing `.statico.toml` → defaults are used silently.
- Unparsable `.statico.toml` → defaults are used; a warning is printed to
  stderr.
- Invalid values → coerced where reasonable (NaN `min_confidence` becomes
  `0.0`; oversized `max_file_size` is clamped to 50 MB).
- Unknown keys → ignored (tolerated for forward-compat).

If you want strict-config behavior (fail on any error or unknown key), open an
issue — it's a frequently requested feature that hasn't been built yet.
