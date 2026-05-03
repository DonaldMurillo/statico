# coverage-gap

A multi-hook statico plugin that flags exported TypeScript identifiers with
no matching test reference.

This is the showcase example for statico's plugin protocol. It exercises:

- `init` — read `pluginSettings` from `.statico.toml` and store them on
  module scope so the rest of the plugin sees the resolved configuration.
- `analyze_file` — split each file into one of two buckets: test files
  (collect identifiers used inside `test`, `it`, `describe`, `expect`) or
  source files (collect top-level export names). Per-file state lives in
  module-scoped maps that survive across calls.
- `post_analysis` — runs once after every file has been analyzed. Diffs
  exports against the union of all tested names and emits one
  `coverage-gap::missing-test` issue per export with no test reference.

> ⚠️ The "missing test" heuristic is rough — it's identifier-based, not
> AST-based, so it'll miss tests that reference imported symbols indirectly,
> and it'll match coincidental words inside `it('foo')` strings. The plugin
> exists to **demonstrate the protocol**, not to be your coverage gate.
> For real test-coverage tooling, use `c8`, `vitest --coverage`, or your
> framework's built-in.

---

## Try it

```bash
cd examples/plugins/coverage-gap
bun install        # links @statico/plugin-sdk from ../../../sdks/typescript

# From a project with TypeScript files + tests:
mkdir -p .statico/plugins
ln -s "$(pwd)" /path/to/your-project/.statico/plugins/coverage-gap

cd /path/to/your-project
statico analyze . --format markdown | head -40
```

The plugin auto-loads on the next `statico analyze` thanks to the
`.statico/plugins/` symlink and the `package.json` in the example dir.

## Run the test suite

The plugin ships with end-to-end JSON-RPC tests in `index.test.ts` —
they spawn the plugin as a subprocess (just like statico does) and
verify the `init` → `analyze_file` → `post_analysis` flow with several
configurations.

```bash
bun install   # if you haven't already
bun test
```

These tests run in CI on every push as part of the main `ci` workflow.

## Configure

Drop a `[[plugin]]` block in your project's `.statico.toml`:

```toml
[[plugin]]
name = "coverage-gap"
languages = ["typescript", "tsx"]

[plugin.settings]
test_globs = ["**/*.test.ts", "**/*.spec.ts", "**/*.unit.ts"]
min_export_length = 4              # ignore short identifiers
exclude_exports = ["default", "metadata", "config", "schema"]
severity = "warning"               # or "error" to block CI
```

## How it works

1. `Plugin.create(...)` declares two hooks (`analyze_file` and
   `post_analysis`) and a single rule (`missing-test`).
2. `plugin.onInit(...)` parses `pluginSettings` and clears the module-level
   state maps. The SDK still responds to the host's `init` request with the
   manifest after this handler runs.
3. `plugin.onAnalyzeFile(...)` classifies each file, captures exports or
   tested names, and returns no per-file issues — the diff happens later.
4. `plugin.onPostAnalysis(...)` unions all tested names, walks every
   recorded export, and emits a `missing-test` issue for any export not in
   the union. Confidence is scaled by name length (longer names are less
   likely to false-match a random word in a test label).

## File map

```
examples/plugins/coverage-gap/
├── README.md            # this file
├── package.json         # links @statico/plugin-sdk
├── tsconfig.json        # bun-types, strict
└── index.ts             # the plugin (~150 LOC)
```

## See also

- [`docs/plugins.md`](../../../docs/plugins.md) — full plugin protocol
  reference.
- `statico plugin schema --format json` — current JSON-RPC contract.
- [`sdks/typescript/src/index.ts`](../../../sdks/typescript/src/index.ts) —
  the SDK source. `Plugin` is the only public class; everything else is
  types.
