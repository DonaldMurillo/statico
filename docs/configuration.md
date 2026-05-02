# Configuration

Fine-tune statico's analysis with a `.statico.toml` file in your project root.

---

## Basic Configuration

```toml
[analysis]
min_confidence = 0.7
ignore = ["**/generated/**", "**/*.d.ts"]
max_file_size = "1MB"
```

## Analysis Options

### `min_confidence`

Minimum confidence threshold for dead code detection (0.0–1.0). Default: `0.5`.

```toml
[analysis]
min_confidence = 0.8  # Only report high-confidence dead code
```

### `ignore`

Glob patterns for files to exclude from analysis.

```toml
[analysis]
ignore = [
  "**/generated/**",
  "**/*.d.ts",
  "**/dist/**",
  "**/node_modules/**",
]
```

### `max_file_size`

Maximum file size to parse. Files larger than this are skipped. Default: `2MB`.

```toml
[analysis]
max_file_size = "500KB"
```

## Framework Configuration

```toml
[framework]
name = "auto"  # Auto-detect framework (default)
```

Supported values: `auto`, `nextjs`, `angular`, `vue`, `svelte`, `astro`, `remix`, `nestjs`, `payload`, `shadcn`.

### Custom Entry Points

Override auto-detected entry points:

```toml
[framework]
name = "auto"

[[framework.entries]]
path = "src/app/main.ts"
type = "main"

[[framework.entries]]
path = "src/app/admin.ts"
type = "main"
```

## Duplication Detection

```toml
[duplication]
min_lines = 6          # Minimum lines for a clone (default: 6)
similarity = 0.8       # Minimum similarity ratio (default: 0.8)
ignore = ["**/test/**"]
```

## Output Configuration

```toml
[output]
# Default format when no --format flag is given
default_format = "terminal"

# For HTML reports
html_dark_mode = true
html_inline_assets = true
```

## Plugin Configuration

```toml
[plugins]
# Directory containing plugins (relative to project root)
directory = ".statico/plugins"

# Enable/disable all plugins
enabled = true
```

## Cache Configuration

```tomoml
[cache]
# Enable incremental analysis cache
enabled = true

# Cache directory (relative to project root)
directory = ".statico/cache"
```

## Full Example

```toml
[analysis]
min_confidence = 0.7
ignore = [
  "**/generated/**",
  "**/*.d.ts",
  "**/dist/**",
  "**/__tests__/**",
]
max_file_size = "1MB"

[framework]
name = "nextjs"

[[framework.entries]]
path = "src/pages/_app.tsx"
type = "main"

[duplication]
min_lines = 8
similarity = 0.85

[plugins]
directory = ".statico/plugins"
enabled = true

[cache]
enabled = true
```
