/**
 * End-to-end tests for the coverage-gap plugin.
 *
 * We treat the plugin as the host (statico) does: spawn it as a subprocess,
 * talk JSON-RPC over stdin/stdout, assert on the responses. No internals
 * are imported — this validates the actual contract.
 *
 * Run with:
 *
 *     bun test
 *
 * inside this directory. Bun is required because the plugin uses Bun-only
 * APIs (`Bun.write`) under the hood via the SDK.
 */

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { createInterface, type Interface } from "node:readline";
import { join } from "node:path";

const PLUGIN_ENTRY = join(import.meta.dir, "index.ts");

// ─── A tiny client wrapper ─────────────────────────────────────────────────
//
// Spawns the plugin and provides a request/response API. The host (statico)
// uses a similar but more complex implementation in src/plugin/manager.rs;
// this is the minimal shape needed to drive the plugin in tests.

class PluginClient {
  private proc: ChildProcessWithoutNullStreams;
  private lines: Interface;
  private nextId = 1;
  private pending = new Map<
    number,
    { resolve: (v: unknown) => void; reject: (e: Error) => void }
  >();

  constructor() {
    this.proc = spawn("bun", [PLUGIN_ENTRY], { stdio: ["pipe", "pipe", "pipe"] });

    this.lines = createInterface({ input: this.proc.stdout });
    this.lines.on("line", (raw) => {
      let msg: { id: number; result?: unknown; error?: { message: string } };
      try {
        msg = JSON.parse(raw);
      } catch {
        return;
      }
      const slot = this.pending.get(msg.id);
      if (!slot) return;
      this.pending.delete(msg.id);
      if (msg.error) {
        slot.reject(new Error(msg.error.message));
      } else {
        slot.resolve(msg.result);
      }
    });

    // Surface stderr to the test runner so plugin debug output is visible.
    this.proc.stderr.on("data", (chunk) => {
      process.stderr.write(`[plugin stderr] ${chunk}`);
    });
  }

  request<T = unknown>(method: string, params: unknown): Promise<T> {
    const id = this.nextId++;
    const line = JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n";
    return new Promise((resolve, reject) => {
      this.pending.set(id, {
        resolve: (v) => resolve(v as T),
        reject,
      });
      this.proc.stdin.write(line);
    });
  }

  async shutdown(): Promise<void> {
    try {
      await this.request("shutdown", null);
    } catch {
      // shutdown is best-effort — the plugin may exit before responding.
    }
    this.proc.kill();
    this.lines.close();
  }
}

// ─── Fixtures ──────────────────────────────────────────────────────────────

const SOURCE_FILE = `
export const formatDate = (d: Date) => d.toISOString();
export function unusedHelper() { return 42; }
export class WidgetService {}
export type Id = string;
`;

const TEST_FILE_WITH_SOME_COVERAGE = `
import { formatDate } from "./util";

test("formatDate handles UTC dates", () => {
  expect(formatDate).toBeDefined();
});

describe("WidgetService basics", () => {
  it("constructs without error", () => {});
});
`;

// Helper: run init + a sequence of analyze_file calls + post_analysis.
async function runScenario(opts: {
  files: { path: string; source: string }[];
  pluginSettings?: Record<string, unknown>;
}): Promise<{
  initResult: { name: string; rules: { id: string }[] };
  postAnalysisResult: {
    issues: { ruleId: string; file: string; line: number; message: string; confidence?: number }[];
    suggestions: string[];
  };
}> {
  const client = new PluginClient();
  try {
    const initResult = (await client.request("init", {
      root: "/fake/project",
      config: {},
      pluginSettings: opts.pluginSettings ?? {},
    })) as { name: string; rules: { id: string }[] };

    for (const f of opts.files) {
      await client.request("analyze_file", {
        path: f.path,
        source: f.source,
        language: "typescript",
        existingIssues: [],
      });
    }

    const postAnalysisResult = (await client.request("post_analysis", {
      results: {},
      healthScore: 100,
      totalFiles: opts.files.length,
      language: "",
    })) as { issues: typeof initResult extends never ? never : never[]; suggestions: string[] };

    return {
      initResult,
      postAnalysisResult: postAnalysisResult as unknown as {
        issues: {
          ruleId: string;
          file: string;
          line: number;
          message: string;
          confidence?: number;
        }[];
        suggestions: string[];
      },
    };
  } finally {
    await client.shutdown();
  }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

describe("coverage-gap plugin", () => {
  test("init reports the plugin manifest", async () => {
    const { initResult } = await runScenario({ files: [] });
    expect(initResult.name).toBe("coverage-gap");
    expect(initResult.rules.map((r) => r.id)).toContain("missing-test");
  });

  test("flags exports with no test reference", async () => {
    const { postAnalysisResult } = await runScenario({
      files: [
        { path: "src/util.ts", source: SOURCE_FILE },
        { path: "src/util.test.ts", source: TEST_FILE_WITH_SOME_COVERAGE },
      ],
    });

    const flagged = postAnalysisResult.issues.map((i) => ({
      file: i.file,
      ruleId: i.ruleId,
      message: i.message,
    }));

    // `unusedHelper` is exported but never referenced in the test file.
    expect(flagged.some((i) => i.message.includes("unusedHelper"))).toBe(true);

    // `formatDate` and `WidgetService` are referenced in the test file
    // (expect(formatDate), describe("WidgetService …")) so they should NOT
    // be flagged.
    expect(flagged.some((i) => i.message.includes("formatDate"))).toBe(false);
    expect(flagged.some((i) => i.message.includes("WidgetService"))).toBe(false);

    // Every issue uses the documented rule id.
    for (const issue of postAnalysisResult.issues) {
      expect(issue.ruleId).toBe("missing-test");
      expect(issue.file).toBe("src/util.ts");
    }
  });

  test("respects exclude_exports setting", async () => {
    const { postAnalysisResult } = await runScenario({
      files: [
        {
          path: "src/cfg.ts",
          source: `
export const ApiConfig = { url: "x" };
export const TimeoutConfig = { ms: 1000 };
export const PaymentSecretsManager = {};
`,
        },
        // No test file at all — without exclude_exports every export would be flagged.
      ],
      pluginSettings: {
        exclude_exports: ["config", "secret"],
        min_export_length: 3,
      },
    });

    // `config`-named and `secret`-named exports are excluded.
    const flagged = postAnalysisResult.issues.map((i) => i.message);
    expect(flagged.some((m) => m.includes("ApiConfig"))).toBe(false);
    expect(flagged.some((m) => m.includes("TimeoutConfig"))).toBe(false);
    expect(flagged.some((m) => m.includes("PaymentSecretsManager"))).toBe(false);

    // No issues at all in this scenario, then.
    expect(postAnalysisResult.issues).toHaveLength(0);
  });

  test("min_export_length skips short identifiers", async () => {
    const { postAnalysisResult } = await runScenario({
      files: [
        {
          path: "src/short.ts",
          source: `
export const x = 1;
export const ab = 2;
export const longEnoughName = 3;
`,
        },
      ],
      pluginSettings: { min_export_length: 5 },
    });

    const flagged = postAnalysisResult.issues.map((i) => i.message);
    // x and ab are below 5 chars — skipped.
    expect(flagged.some((m) => m.includes("`x`"))).toBe(false);
    expect(flagged.some((m) => m.includes("`ab`"))).toBe(false);
    // longEnoughName has no test ref → flagged.
    expect(flagged.some((m) => m.includes("longEnoughName"))).toBe(true);
  });

  test("custom test_globs match additional file patterns", async () => {
    // With default test_globs `*.unit.ts` is NOT a test file → exports there
    // would be treated as source. With a custom glob that includes
    // `**/*.unit.ts` the file's identifiers should be picked up as tested.
    const { postAnalysisResult } = await runScenario({
      files: [
        {
          path: "src/util.ts",
          source: `export function helperFunction() {}`,
        },
        {
          path: "src/util.unit.ts",
          source: `test("helperFunction works", () => {});`,
        },
      ],
      pluginSettings: {
        test_globs: ["**/*.unit.ts"],
      },
    });

    expect(
      postAnalysisResult.issues.some((i) => i.message.includes("helperFunction")),
    ).toBe(false);
  });

  test("post_analysis suggestions accompany flagged issues", async () => {
    const { postAnalysisResult } = await runScenario({
      files: [
        {
          path: "src/lonely.ts",
          source: `export const lonelyExport = 1;`,
        },
      ],
    });
    expect(postAnalysisResult.issues.length).toBeGreaterThan(0);
    expect(postAnalysisResult.suggestions.length).toBeGreaterThan(0);
    expect(postAnalysisResult.suggestions[0]).toContain("coverage-gap");
  });

  test("clean run produces no issues and no suggestions", async () => {
    const { postAnalysisResult } = await runScenario({
      files: [
        // No source files at all — nothing to flag.
      ],
    });
    expect(postAnalysisResult.issues).toHaveLength(0);
    expect(postAnalysisResult.suggestions).toHaveLength(0);
  });
});
