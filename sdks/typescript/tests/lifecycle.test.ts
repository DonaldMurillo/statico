// JSON-RPC lifecycle tests for @statico/plugin-sdk.
//
// Drives `processRequest` directly so the dispatcher is covered without
// spawning a subprocess. The stdin/stdout loop in `start()` is just a
// thin wrapper around it.
import { describe, expect, test } from "bun:test";
import { Plugin } from "../src/index";

const baseManifest = {
  version: "0.1.0",
  hooks: { analyze_file: "add" } as const,
  languages: ["typescript"],
  rules: [],
};

function parse(line: string): Record<string, unknown> {
  return JSON.parse(line) as Record<string, unknown>;
}

describe("Plugin.processRequest", () => {
  test("init returns plugin capabilities", async () => {
    const plugin = Plugin.create("test-plugin", baseManifest);
    const out = await plugin.processRequest(
      JSON.stringify({ jsonrpc: "2.0", id: 1, method: "init" }),
    );

    expect(out.shutdown).toBe(false);
    const v = parse(out.response) as any;
    expect(v.jsonrpc).toBe("2.0");
    expect(v.id).toBe(1);
    expect(v.result.name).toBe("test-plugin");
    expect(v.result.version).toBe("0.1.0");
    expect(v.result.languages).toEqual(["typescript"]);
    expect(v.result.hooks.analyze_file).toBe("add");
  });

  test("init runs onInit handler before responding", async () => {
    const plugin = Plugin.create("p", baseManifest);
    let captured: any = null;
    plugin.onInit((params) => {
      captured = params;
    });

    await plugin.processRequest(
      JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "init",
        params: { root: "/r", config: {}, pluginSettings: { foo: 1 } },
      }),
    );

    expect(captured).not.toBeNull();
    expect(captured.pluginSettings.foo).toBe(1);
  });

  test("analyze_file dispatches to registered handler", async () => {
    const plugin = Plugin.create("analyzer", baseManifest);
    plugin.onAnalyzeFile((params) => {
      expect(params.path).toBe("src/foo.ts");
      return {
        issues: [
          {
            ruleId: "demo",
            severity: "warning",
            message: "hello",
            file: params.path,
            line: 7,
            confidence: 0.9,
          },
        ],
      };
    });

    const req = JSON.stringify({
      jsonrpc: "2.0",
      id: 42,
      method: "analyze_file",
      params: { path: "src/foo.ts", source: "x", language: "typescript", existingIssues: [] },
    });
    const out = await plugin.processRequest(req);

    expect(out.shutdown).toBe(false);
    const v = parse(out.response) as any;
    expect(v.id).toBe(42);
    expect(v.result.issues[0].ruleId).toBe("demo");
    expect(v.result.issues[0].line).toBe(7);
  });

  test("unknown method returns -32601", async () => {
    const plugin = Plugin.create("p", baseManifest);
    const out = await plugin.processRequest(
      JSON.stringify({ jsonrpc: "2.0", id: 3, method: "does_not_exist" }),
    );
    expect(out.shutdown).toBe(false);
    const v = parse(out.response) as any;
    expect(v.id).toBe(3);
    expect(v.error.code).toBe(-32601);
  });

  test("malformed JSON returns parse error -32700", async () => {
    const plugin = Plugin.create("p", baseManifest);
    const out = await plugin.processRequest("not json {{{");
    expect(out.shutdown).toBe(false);
    const v = parse(out.response) as any;
    expect(v.error.code).toBe(-32700);
  });

  test("shutdown signals exit and returns null result", async () => {
    const plugin = Plugin.create("p", baseManifest);
    const out = await plugin.processRequest(
      JSON.stringify({ jsonrpc: "2.0", id: 9, method: "shutdown" }),
    );
    expect(out.shutdown).toBe(true);
    const v = parse(out.response) as any;
    expect(v.id).toBe(9);
    expect(v.result).toBeNull();
  });

  test("handler errors surface as -32000 JSON-RPC errors", async () => {
    const plugin = Plugin.create("p", baseManifest);
    plugin.onAnalyzeFile(() => {
      throw new Error("boom");
    });
    const req = JSON.stringify({
      jsonrpc: "2.0",
      id: 5,
      method: "analyze_file",
      params: { path: "a", source: "", language: "typescript", existingIssues: [] },
    });
    const out = await plugin.processRequest(req);
    expect(out.shutdown).toBe(false);
    const v = parse(out.response) as any;
    expect(v.id).toBe(5);
    expect(v.error.code).toBe(-32000);
    expect(v.error.message).toContain("boom");
  });

  test("onInit handler error produces -32603", async () => {
    const plugin = Plugin.create("p", baseManifest);
    plugin.onInit(() => {
      throw new Error("init failed");
    });
    const out = await plugin.processRequest(
      JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "init",
        params: { root: "/r", config: {}, pluginSettings: {} },
      }),
    );
    expect(out.shutdown).toBe(false);
    const v = parse(out.response) as any;
    expect(v.error.code).toBe(-32603);
  });
});
