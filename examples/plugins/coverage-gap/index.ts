/**
 * coverage-gap — a statico plugin that flags exported TypeScript identifiers
 * with no matching test reference.
 *
 * Demonstrates a multi-hook plugin:
 *
 *   • `analyze_file` — runs once per source file, classifying each as a
 *     test file or not. For test files, extract identifiers referenced
 *     inside `test(...)`, `it(...)`, `describe(...)`, and `expect(X)` calls.
 *     For non-test files, extract top-level export names.
 *
 *   • `post_analysis` — runs once at the end with cross-cutting state.
 *     Diff exports vs tested names, flag any export with no matching
 *     reference as a `coverage-gap::missing-test` issue.
 *
 * Configurable via `[plugin.settings]` in `.statico.toml`:
 *
 *     test_globs        — globs that mark a file as a test file
 *                         (default: `**\/*.test.ts`, `**\/*.spec.ts`,
 *                          and any path containing `__tests__`)
 *     min_export_length — skip identifiers shorter than this (default 3)
 *     exclude_exports   — case-insensitive substring blocklist for export
 *                         names that should never be flagged
 *                         (default `["default", "metadata", "config"]`)
 *     severity          — issue severity level (default `"warning"`)
 *
 * The point of this example is to show the plugin protocol end-to-end
 * (settings, two hooks with cross-call state, custom rule reporting,
 * confidence scoring). The "missing test" heuristic itself is rough —
 * production-grade test-coverage tooling needs a real type-checker pass.
 * Use this as a template for your own multi-hook plugins, not as the
 * coverage gate for your repo.
 */

import {
  Plugin,
  type AnalyzeFileParams,
  type AnalyzeFileResult,
  type Issue,
  type PostAnalysisParams,
  type PostAnalysisResult,
  type Severity,
} from "@statico/plugin-sdk";

// ─── Settings ──────────────────────────────────────────────────────────────

interface CoverageGapSettings {
  test_globs?: string[];
  min_export_length?: number;
  exclude_exports?: string[];
  severity?: Severity;
}

const DEFAULTS: Required<CoverageGapSettings> = {
  test_globs: ["**/*.test.ts", "**/*.test.tsx", "**/*.spec.ts", "**/*.spec.tsx"],
  min_export_length: 3,
  exclude_exports: ["default", "metadata", "config"],
  severity: "warning",
};

let settings: Required<CoverageGapSettings> = DEFAULTS;

function loadSettings(raw: unknown): Required<CoverageGapSettings> {
  const s = (raw ?? {}) as CoverageGapSettings;
  return {
    test_globs: Array.isArray(s.test_globs) ? s.test_globs : DEFAULTS.test_globs,
    min_export_length:
      typeof s.min_export_length === "number" && s.min_export_length > 0
        ? s.min_export_length
        : DEFAULTS.min_export_length,
    exclude_exports: Array.isArray(s.exclude_exports)
      ? s.exclude_exports.map((x) => String(x).toLowerCase())
      : DEFAULTS.exclude_exports,
    severity:
      s.severity === "error" || s.severity === "info" || s.severity === "warning"
        ? s.severity
        : DEFAULTS.severity,
  };
}

// ─── Per-file scratch state ────────────────────────────────────────────────
//
// `analyze_file` populates these maps; `post_analysis` reads them.
// Both maps are keyed by the file's relative path inside the project root.

interface ExportSite {
  file: string;
  name: string;
  line: number;
}

const exportsByFile = new Map<string, ExportSite[]>();
const testedNamesPerFile = new Map<string, Set<string>>();

// ─── Pattern helpers ───────────────────────────────────────────────────────

const EXPORT_DECL_RE =
  /^[\t ]*export\s+(?:async\s+)?(?:default\s+)?(?:const|let|var|function|class|type|interface|enum)\s+([A-Za-z_$][\w$]*)/;

// Catches `test('x', …)`, `it("x", …)`, `describe(\`x\`, …)`. The captured
// group is the test/spec/describe label, which is what humans use to refer
// to the unit under test. We also pull out identifiers passed to
// `expect(...)` since those tend to be the actual symbol names.
const TEST_LABEL_RE = /(?:^|[^A-Za-z_$])(?:test|it|describe|bench)\s*\(\s*(?:"([^"]+)"|'([^']+)'|`([^`]+)`)/g;
const EXPECT_IDENT_RE = /(?:^|[^A-Za-z_$])expect\s*\(\s*([A-Za-z_$][\w$]*)/g;

function isTestFile(path: string): boolean {
  for (const glob of settings.test_globs) {
    if (matchGlob(glob, path)) return true;
  }
  // Also treat anything inside __tests__ as a test file — that's the Jest
  // convention and several frameworks pick it up.
  return path.includes("/__tests__/") || path.startsWith("__tests__/");
}

/**
 * Tiny glob matcher. Supports `**` (any path including `/`) and `*` (any
 * non-slash chars). Mirrors statico's built-in matcher closely enough for
 * the test-file detection use case.
 */
function matchGlob(pattern: string, path: string): boolean {
  // Normalize: strip leading `./`
  const p = path.replace(/^\.\//, "");
  // Convert glob to regex.
  const re =
    "^" +
    pattern
      .replace(/[.+^${}()|[\]\\]/g, "\\$&")
      .replace(/\*\*\//g, "(?:.*/)?")
      .replace(/\*\*/g, ".*")
      .replace(/\*/g, "[^/]*") +
    "$";
  return new RegExp(re).test(p);
}

function collectExports(source: string): { name: string; line: number }[] {
  const out: { name: string; line: number }[] = [];
  const lines = source.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const m = EXPORT_DECL_RE.exec(lines[i] ?? "");
    if (!m) continue;
    const name = m[1];
    if (!name) continue;
    if (name.length < settings.min_export_length) continue;
    if (settings.exclude_exports.some((x) => name.toLowerCase().includes(x))) continue;
    out.push({ name, line: i + 1 });
  }
  return out;
}

function collectTestedNames(source: string): Set<string> {
  const names = new Set<string>();

  // Identifiers from labels — split on common separators and keep tokens
  // that look like JS identifiers (so a `test('formatDate handles UTC')`
  // contributes `formatDate`, `handles`, `UTC`).
  TEST_LABEL_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = TEST_LABEL_RE.exec(source)) !== null) {
    const label = m[1] ?? m[2] ?? m[3] ?? "";
    for (const tok of label.split(/[\s,.()/[\]{}<>:;-]+/)) {
      if (/^[A-Za-z_$][\w$]*$/.test(tok)) names.add(tok);
    }
  }

  // Identifiers inside expect(...).
  EXPECT_IDENT_RE.lastIndex = 0;
  while ((m = EXPECT_IDENT_RE.exec(source)) !== null) {
    if (m[1]) names.add(m[1]);
  }

  return names;
}

// ─── Plugin wiring ─────────────────────────────────────────────────────────

const plugin = Plugin.create("coverage-gap", {
  version: "0.1.0",
  hooks: {
    analyze_file: "add",
    post_analysis: "add",
  },
  languages: ["typescript", "tsx"],
  rules: [
    {
      id: "missing-test",
      severity: "warning",
      description: "Exported identifier has no matching test reference",
    },
  ],
});

// `init` happens automatically inside Plugin.start(); we override it here
// only to capture pluginSettings before the SDK forwards them.
plugin.onInit((params) => {
  settings = loadSettings(params.pluginSettings);
  // Reset state — `init` may be called more than once if the plugin is
  // reused across runs in the same process (today statico spawns a fresh
  // subprocess per analyze, but we don't rely on that).
  exportsByFile.clear();
  testedNamesPerFile.clear();
});

plugin.onAnalyzeFile((params: AnalyzeFileParams): AnalyzeFileResult => {
  const { path, source } = params;
  if (isTestFile(path)) {
    testedNamesPerFile.set(path, collectTestedNames(source));
  } else {
    exportsByFile.set(
      path,
      collectExports(source).map((e) => ({ file: path, name: e.name, line: e.line })),
    );
  }
  // We don't surface per-file issues — the diff happens in post_analysis.
  return { issues: [] };
});

plugin.onPostAnalysis((_params: PostAnalysisParams): PostAnalysisResult => {
  // Union all tested names across all test files.
  const allTested = new Set<string>();
  for (const set of testedNamesPerFile.values()) {
    for (const name of set) allTested.add(name);
  }

  const issues: Issue[] = [];
  for (const [file, exportSites] of exportsByFile) {
    for (const site of exportSites) {
      if (allTested.has(site.name)) continue;
      issues.push({
        ruleId: "missing-test",
        severity: settings.severity,
        message: `\`${site.name}\` is exported but never referenced in any test file`,
        file,
        line: site.line,
        // Confidence reflects the heuristic — short names get a lower
        // score because they're more likely to collide with random words
        // in test labels we couldn't parse.
        confidence: site.name.length >= 6 ? 0.85 : 0.65,
        suggestion: `Add a test that references \`${site.name}\` (e.g. \`test('${site.name}', …)\`).`,
      });
    }
  }

  // Suggestions are surfaced separately from issues — the host can use
  // them in summary blocks like "AI tips" sections of formatted reports.
  const suggestions: string[] = [];
  if (issues.length > 0) {
    suggestions.push(
      `coverage-gap: ${issues.length} exported identifier(s) without a matching test reference. ` +
        `Tune via [plugin.settings] coverage-gap.exclude_exports / .min_export_length.`,
    );
  }

  return { issues, suggestions };
});

plugin.start();
