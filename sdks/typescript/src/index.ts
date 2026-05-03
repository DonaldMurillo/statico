/**
 * @statico/plugin-sdk — SDK for building statico plugins in TypeScript.
 *
 * Provides typed helpers for the JSON-RPC 2.0 protocol that statico
 * uses to communicate with plugin subprocesses over stdin/stdout.
 *
 * ## Quick Start
 *
 * ```typescript
 * import { Plugin } from "@statico/plugin-sdk";
 *
 * const plugin = Plugin.create("my-rule", {
 *   hooks: { analyze_file: "add" },
 *   languages: ["typescript"],
 *   rules: [{ id: "no-console", severity: "warning", description: "No console.log" }],
 * });
 *
 * plugin.onAnalyzeFile((params) => {
 *   const issues = [];
 *   if (params.source.includes("console.log")) {
 *     issues.push({ ruleId: "no-console", severity: "warning", message: "Found console.log", file: params.path, line: 1, confidence: 0.9 });
 *   }
 *   return { issues };
 * });
 *
 * plugin.start();
 * ```
 */

// ─── Types ───────────────────────────────────────────────────────

/** Hook names that plugins can subscribe to. */
export type HookName =
  | "analyze_file"
  | "discover_entries"
  | "resolve_import"
  | "post_analysis"
  | "format_output";

/** How a plugin participates in a hook. */
export type HookMode = "add" | "override";

/** Severity levels for issues. */
export type Severity = "error" | "warning" | "info";

/** A rule declared in the plugin manifest. */
export interface Rule {
  id: string;
  severity: Severity;
  description: string;
}

/** Plugin manifest — passed to Plugin.create(). */
export interface PluginManifest {
  version?: string;
  hooks: Partial<Record<HookName, HookMode>>;
  languages?: string[];
  rules?: Rule[];
}

/** A single issue reported by a plugin. */
export interface Issue {
  ruleId: string;
  severity: Severity;
  message: string;
  file: string;
  line: number;
  column?: number;
  endLine?: number;
  endColumn?: number;
  confidence?: number;
  suggestion?: string;
}

/** An entry point discovered by a plugin. */
export interface EntryPoint {
  path: string;
  type?: string;
  framework?: string;
}

/** File metrics reported alongside analysis. */
export interface FileMetrics {
  complexity: number;
  loc: number;
}

// ─── Hook parameter / result types ───────────────────────────────

/** analyze_file params. */
export interface AnalyzeFileParams {
  path: string;
  source: string;
  language: string;
  existingIssues: Issue[];
}

/** analyze_file result. */
export interface AnalyzeFileResult {
  issues: Issue[];
  exports?: string[];
  dependencies?: string[];
  metrics?: FileMetrics;
}

/** discover_entries params. */
export interface DiscoverEntriesParams {
  root: string;
  configFiles: string[];
  language: string;
}

/** discover_entries result. */
export interface DiscoverEntriesResult {
  entryPoints: EntryPoint[];
}

/** resolve_import params. */
export interface ResolveImportParams {
  fromFile: string;
  specifier: string;
  root: string;
}

/** resolve_import result. */
export interface ResolveImportResult {
  resolvedPath: string;
  external: boolean;
}

/** init params (forwarded to onInit handlers, if any). */
export interface InitParams {
  /** Absolute path of the project root being analyzed. */
  root: string;
  /** The host's full statico config (rarely needed; usually empty). */
  config: Record<string, unknown>;
  /**
   * Free-form settings from the plugin's `[plugin.settings]` block in
   * `.statico.toml`. Plugin authors define the shape; the SDK passes it
   * through unchanged. Capped at 64 KB / 32 nesting levels by the host.
   */
  pluginSettings: Record<string, unknown>;
}

/** post_analysis params. */
export interface PostAnalysisParams {
  results: Record<string, unknown>;
  healthScore: number;
  totalFiles: number;
  language: string;
}

/** post_analysis result. */
export interface PostAnalysisResult {
  issues: Issue[];
  suggestions: string[];
}

/** format_output params. */
export interface FormatOutputParams {
  results: Record<string, unknown>;
  format: string;
  healthScore: number;
}

/** format_output result. */
export interface FormatOutputResult {
  output: string;
  exitCode?: number;
}

// ─── JSON-RPC internals ──────────────────────────────────────────

interface JsonRpcRequest {
  jsonrpc: "2.0";
  id: number;
  method: string;
  params: unknown;
}

interface JsonRpcSuccessResponse {
  jsonrpc: "2.0";
  id: number;
  result: unknown;
}

interface JsonRpcErrorResponse {
  jsonrpc: "2.0";
  id: number;
  error: { code: number; message: string; data?: unknown };
}

type JsonRpcResponse = JsonRpcSuccessResponse | JsonRpcErrorResponse;

/**
 * Result of dispatching a single JSON-RPC request.
 *
 * `response` is the line that should be written to stdout (no trailing
 * newline). `shutdown` is `true` only for the `shutdown` method — when
 * set, the host expects the plugin to exit after the response is written.
 */
export interface ProcessOutcome {
  response: string;
  shutdown: boolean;
}

// ─── Plugin class ────────────────────────────────────────────────

type HookHandler<P, R> = (params: P) => R | Promise<R>;

/**
 * Main plugin class — reads JSON-RPC from stdin, dispatches to handlers, writes to stdout.
 *
 * Usage:
 *   const plugin = Plugin.create("my-plugin", { hooks: { analyze_file: "add" } });
 *   plugin.onAnalyzeFile((params) => ({ issues: [] }));
 *   plugin.start();
 */
export class Plugin {
  private name: string;
  private manifest: PluginManifest;
  private handlers: Map<string, HookHandler<unknown, unknown>> = new Map();

  private constructor(name: string, manifest: PluginManifest) {
    this.name = name;
    this.manifest = manifest;
  }

  /** Create a new plugin instance. */
  static create(name: string, manifest: PluginManifest): Plugin {
    return new Plugin(name, manifest);
  }

  /**
   * Register a handler invoked when statico sends the `init` request.
   *
   * The SDK still responds with the manifest automatically — your handler
   * runs **before** that response is written, so any state you set up here
   * (most commonly: parsing `params.pluginSettings`) is ready before the
   * first hook call. Synchronous and async handlers both work.
   *
   * The handler's return value is ignored — use it for side effects.
   */
  onInit(handler: HookHandler<InitParams, void>): this {
    this.handlers.set("init", handler as HookHandler<unknown, unknown>);
    return this;
  }

  /** Register a handler for the analyze_file hook. */
  onAnalyzeFile(handler: HookHandler<AnalyzeFileParams, AnalyzeFileResult>): this {
    this.handlers.set("analyze_file", handler as HookHandler<unknown, unknown>);
    return this;
  }

  /** Register a handler for the discover_entries hook. */
  onDiscoverEntries(handler: HookHandler<DiscoverEntriesParams, DiscoverEntriesResult>): this {
    this.handlers.set("discover_entries", handler as HookHandler<unknown, unknown>);
    return this;
  }

  /** Register a handler for the resolve_import hook. */
  onResolveImport(handler: HookHandler<ResolveImportParams, ResolveImportResult>): this {
    this.handlers.set("resolve_import", handler as HookHandler<unknown, unknown>);
    return this;
  }

  /** Register a handler for the post_analysis hook. */
  onPostAnalysis(handler: HookHandler<PostAnalysisParams, PostAnalysisResult>): this {
    this.handlers.set("post_analysis", handler as HookHandler<unknown, unknown>);
    return this;
  }

  /** Register a handler for the format_output hook. */
  onFormatOutput(handler: HookHandler<FormatOutputParams, FormatOutputResult>): this {
    this.handlers.set("format_output", handler as HookHandler<unknown, unknown>);
    return this;
  }

  /**
   * Process a single JSON-RPC request line and return the response plus
   * a flag indicating whether the host has asked the plugin to shut down.
   *
   * This is the unit-testable core of the SDK — `start()` is just a
   * stdin/stdout loop wrapped around it.
   */
  async processRequest(line: string): Promise<ProcessOutcome> {
    let req: JsonRpcRequest;
    try {
      req = JSON.parse(line);
    } catch {
      return {
        response: JSON.stringify({
          jsonrpc: "2.0",
          id: 0,
          error: { code: -32700, message: "Parse error" },
        }),
        shutdown: false,
      };
    }

    const { id, method, params } = req;

    if (method === "init") {
      const initHandler = this.handlers.get("init");
      if (initHandler) {
        try {
          await initHandler(params);
        } catch (err) {
          return {
            response: JSON.stringify({
              jsonrpc: "2.0",
              id,
              error: {
                code: -32603,
                message: `init handler failed: ${err instanceof Error ? err.message : String(err)}`,
              },
            }),
            shutdown: false,
          };
        }
      }
      return {
        response: JSON.stringify({
          jsonrpc: "2.0",
          id,
          result: {
            name: this.name,
            version: this.manifest.version ?? null,
            hooks: this.manifest.hooks,
            languages: this.manifest.languages ?? [],
            rules: this.manifest.rules ?? [],
          },
        }),
        shutdown: false,
      };
    }

    if (method === "shutdown") {
      return {
        response: JSON.stringify({ jsonrpc: "2.0", id, result: null }),
        shutdown: true,
      };
    }

    const handler = this.handlers.get(method);
    if (!handler) {
      return {
        response: JSON.stringify({
          jsonrpc: "2.0",
          id,
          error: { code: -32601, message: `Method not found: ${method}` },
        }),
        shutdown: false,
      };
    }

    try {
      const result = await handler(params);
      return {
        response: JSON.stringify({ jsonrpc: "2.0", id, result }),
        shutdown: false,
      };
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      return {
        response: JSON.stringify({
          jsonrpc: "2.0",
          id,
          error: { code: -32000, message },
        }),
        shutdown: false,
      };
    }
  }

  /**
   * Start the JSON-RPC read loop.
   *
   * Reads newline-delimited JSON from stdin, dispatches to registered
   * handlers, and writes responses to stdout.
   *
   * Handles the "init" and "shutdown" methods automatically.
   */
  start(): void {
    const decoder = new TextDecoder();
    let buffer = "";

    // Use Node-compatible stdin/stdout APIs so the compiled package
    // works on both Bun and Node. Bun provides full Node compat for
    // `process.stdin` / `process.stdout`.
    const stdout = process.stdout;
    const stdin = process.stdin;

    const writeResponse = (line: string): void => {
      stdout.write(line + "\n");
    };

    const handleLine = async (line: string): Promise<void> => {
      const outcome = await this.processRequest(line);
      writeResponse(outcome.response);
      if (outcome.shutdown) {
        process.exit(0);
      }
    };

    stdin.on("data", async (chunk: Buffer) => {
      buffer += decoder.decode(chunk, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";
      for (const line of lines) {
        const trimmed = line.trim();
        if (trimmed) {
          await handleLine(trimmed);
        }
      }
    });
    stdin.on("end", () => process.exit(0));
    stdin.on("error", () => process.exit(0));
  }
}
