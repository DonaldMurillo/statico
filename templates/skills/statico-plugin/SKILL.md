---
name: statico-plugin
description: Build, test, or debug a statico plugin. Use when the user wants to write a project-specific rule, scaffold a plugin, or investigate plugin behavior.
---

# statico-plugin

## When to Use

- User asks to write a custom rule
- User wants to scaffold a new plugin
- User mentions debugging or testing an existing plugin under `.statico/plugins/`

## Workflow

1. **Get the current protocol — don't write it from memory.** The plugin
   protocol can change between minor releases:
   ```bash
   statico plugin docs                       # full human-readable reference
   statico plugin schema --format json       # JSON schema (paste into prompts)
   ```

2. **Scaffold a new plugin.** The CLI writes a working skeleton with the
   right SDK wiring for the language:
   ```bash
   statico plugin init my-rule --lang typescript    # default
   statico plugin init my-rule --lang rust
   statico plugin init my-rule --lang python
   ```
   Output lands in `.statico/plugins/my-rule/`.

3. **Implement the detection logic** in the scaffolded entry file
   (`index.ts` / `src/main.rs` / `plugin.py`). Look for the `TODO`
   comments. Use the SDK's hook callbacks — never hand-roll JSON-RPC
   unless you know what you're doing.

4. **Iterate against a fixture:**
   ```bash
   statico plugin run my-rule --file fixtures/sample.ts
   ```
   The output is the JSON-RPC `result` so you can see exactly what
   statico will receive.

5. **Verify the runtime is ready** before reporting failures:
   ```bash
   statico plugin doctor       # checks bun (TS) / cargo (Rust) / python3
   ```

6. **Build (Rust only):**
   ```bash
   statico plugin build --name my-rule
   ```
   TypeScript and Python don't need a build step.

## Choosing hook + mode

- `analyze_file` (mode `add`) — most plugins. Per-file detection, contributes
  alongside built-in checks.
- `post_analysis` (mode `add`) — cross-file rules that need the whole project
  state (e.g. "every exported function should have a test").
- `discover_entries`, `resolve_import`, `format_output` — `override` only.
  Replaces the built-in stage. Use sparingly.

## Common pitfalls

- Two plugins can't both `override` the same hook — fatal at startup.
- Plugins emit JSON-RPC on stdout; **all debug logs must go to stderr** or
  you'll corrupt the protocol.
- `pluginSettings` (from `[plugin.settings]` in `.statico.toml`) is capped at
  64 KB serialized JSON.
- The plugin subprocess is kept alive for the whole `statico analyze` run, so
  expensive setup goes in `init`, not `analyze_file`.

## Reference

- `statico plugin docs` — protocol, hooks, message shapes
- `statico plugin schema --format json` — wire-format JSON schema
- `examples/plugins/coverage-gap` (in the statico repo) — multi-hook reference plugin
