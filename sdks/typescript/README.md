# @statico/plugin-sdk

SDK for building [statico](https://github.com/DonaldMurillo/statico) static-analyzer plugins in TypeScript.

> ⚠️ **Early alpha.** APIs may change between minor versions until 1.0.

## What is statico?

statico is a customizable, AI-forward static analyzer for TypeScript and Rust. Plugins extend it
with custom rules, language support, entry-point discovery, and output formats. They run as
subprocesses and talk to the statico host over JSON-RPC 2.0 on stdin/stdout — any language can
write a plugin, but this SDK gives TypeScript authors a typed, ergonomic wrapper.

## Quick start

```bash
bun add @statico/plugin-sdk
# or: npm install @statico/plugin-sdk
```

```typescript
// index.ts
import { Plugin } from "@statico/plugin-sdk";

const plugin = Plugin.create("no-todo-comments", {
  hooks: { analyze_file: "add" },
  languages: ["typescript", "rust"],
  rules: [
    { id: "no-todo", severity: "warning", description: "Detect TODO comments" },
  ],
});

plugin.onAnalyzeFile((params) => {
  const issues = params.source.split("\n").flatMap((line, i) =>
    line.includes("TODO")
      ? [{
          ruleId: "no-todo",
          severity: "warning" as const,
          message: "Found TODO comment",
          file: params.path,
          line: i + 1,
          confidence: 0.95,
        }]
      : [],
  );
  return { issues };
});

plugin.start();
```

Point statico at your plugin via `.statico.toml`:

```toml
[[plugin]]
name = "no-todo-comments"
runtime = "bun"
entry = "./.statico/plugins/no-todo-comments/index.ts"
```

## Hooks

Each hook has a typed registration method on `Plugin`:

| Hook | Method | Purpose |
|---|---|---|
| `init` | `onInit` | Read plugin settings before the first hook call |
| `analyze_file` | `onAnalyzeFile` | Inspect a single source file; emit issues, exports, imports |
| `discover_entries` | `onDiscoverEntries` | Tell statico about reachable entry points |
| `resolve_import` | `onResolveImport` | Map an import specifier to a file path |
| `post_analysis` | `onPostAnalysis` | Run after main analysis; emit cross-file issues |
| `format_output` | `onFormatOutput` | Provide a custom output format |

See the [plugin docs](https://github.com/DonaldMurillo/statico/blob/main/docs/plugins.md) for the
full protocol contract.

## Runtime requirements

The compiled SDK uses Node-compatible `process.stdin` / `process.stdout` APIs, so it runs on both
Bun and Node ≥ 18. statico itself invokes TypeScript plugins with Bun by default — plugins do not
need to provision a runtime themselves; statico downloads Bun on first use.

## Testing your plugin

The dispatcher logic is testable without spawning a subprocess via `Plugin.processRequest`:

```typescript
import { Plugin } from "@statico/plugin-sdk";

const plugin = Plugin.create("p", { hooks: { analyze_file: "add" } });
plugin.onAnalyzeFile(({ source }) => ({
  issues: source.includes("foo")
    ? [{ ruleId: "x", severity: "warning", message: "found foo", file: "f", line: 1 }]
    : [],
}));

const out = await plugin.processRequest(
  JSON.stringify({
    jsonrpc: "2.0",
    id: 1,
    method: "analyze_file",
    params: { path: "f", source: "foo", language: "typescript", existingIssues: [] },
  }),
);
const v = JSON.parse(out.response);
console.log(v.result.issues); // [{ruleId: "x", ...}]
```

See `tests/lifecycle.test.ts` for more examples.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
