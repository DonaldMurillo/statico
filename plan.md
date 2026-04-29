# Statico v0.2 — 20-Task Improvement Plan

## Overview

Transform statico from a JSON-only CLI tool into a multi-format, AI-friendly, performant static analyzer with framework support for Angular, NestJS, and more. Each task is scoped to ≤3 files and ≤250 lines per file.

---

## Phase 1: Output & Reporting (Tasks 1–5)

### Task 1: SARIF Output Format
**Files:** `src/output/mod.rs`, `src/output/sarif.rs`, `src/main.rs`

Add `--format sarif` flag. SARIF (Static Analysis Results Interchange Format) is the standard for CI/CD integration (GitHub Code Scanning, Azure DevOps). Map each issue type to SARIF `result` objects with `locations`, `ruleId`, and `severity`.

- Create `src/output/` directory with `mod.rs` (trait `OutputFormatter`) and `sarif.rs`
- SARIF schema: `$schema`, `version: "2.1.0`, `runs[].results[]`
- Each detector maps to a SARIF rule with `helpUri` and `severity`
- Add `--format json|sarif` to CLI via clap

### Task 2: Markdown Summary Report
**Files:** `src/output/markdown.rs`, `src/output/mod.rs`

Add `--format markdown`. Produces a human-readable `.md` report with:
- Executive summary (file count, issue count by severity, dup %)
- Top-10 lists: most-duplicated files, most unused exports, largest dead code zones
- Framework-specific sections (if Next.js detected, show orphan pages, etc.)
- Tables with file links for IDE navigation

### Task 3: HTML Interactive Report
**Files:** `src/output/html.rs`, `src/output/mod.rs`

Add `--format html`. Single self-contained HTML file with:
- Embedded CSS (dark/light mode)
- Collapsible issue sections per category
- File-level heat map showing which files have the most issues
- Duplication treemap (files sized by dup lines, colored by dup %)
- All data embedded as JSON in a `<script>` tag, rendered client-side with vanilla JS
- No external dependencies — fully offline-capable

### Task 4: AI-Friendly JSON Schema & Exit Codes
**Files:** `src/output/json_schema.rs`, `src/main.rs`, `src/types.rs`

Make JSON output machine-parseable:
- Add `$schema` and `version` fields to `AnalysisOutput`
- Add `summary` top-level field with pre-computed counts and percentages (no need for AI to count arrays)
- Add `--exit-code` flag: exit 0/1/2 based on issue thresholds (0=clean, 1=warnings, 2=critical)
- Add `--min-confidence 0.7` filter to exclude low-confidence findings
- Publish a `statico-output.schema.json` file that AI tools can reference

### Task 5: Diff & Trend Reports
**Files:** `src/output/diff.rs`, `src/output/mod.rs`, `src/main.rs`

Add `statico diff <before.json> <after.json>` subcommand:
- Shows what changed between two runs: new/fixed/regressed issues
- Output as markdown table or JSON
- Enables CI trend tracking: "3 new dead code files, 5 unused exports fixed"
- AI agents can run this to see impact of their changes

---

## Phase 2: Performance (Tasks 6–8)

### Task 6: Parallel File Processing with Rayon
**Files:** `src/analyzer.rs`, `Cargo.toml`

Current: sequential file parsing loop. Target: parse all files in parallel.
- Add `rayon` dependency
- Parallelize the file reading + tree-sitter parsing in `parse_all_files`
- Use `par_iter()` for the file processing loop
- Collect results into maps with proper synchronization
- Benchmark: should reduce analysis time by ~4x on 4-core machines
- File count stays under 250 lines by extracting a `parse_single_file` function

### Task 7: Incremental Analysis Cache
**Files:** `src/cache.rs`, `src/analyzer.rs`, `src/main.rs`

Add `--cache-dir` flag. Cache parsed AST results keyed by (file path, mtime hash):
- On re-run, skip unchanged files
- Store `HashMap<String, (u64, FileResult)>` as bincode in `.statico-cache/`
- Cache invalidation: mtime + content hash comparison
- Subcommand: `statico analyze --cache-dir .statico-cache <path>`
- Expected: 90%+ speedup on incremental runs (only changed files re-parsed)

### Task 8: Progress Reporting & Streaming Output
**Files:** `src/progress.rs`, `src/analyzer.rs`, `src/main.rs`

Add progress reporting for large codebases:
- `--progress` flag shows stderr progress bar: `[45/1769 files] parsing...`
- `--stream` flag outputs JSON objects line-by-line as each detector finishes
  - Enables AI agents to start processing results before full analysis completes
- Structured log format: `{"type":"progress","phase":"parsing","current":45,"total":1769}`

---

## Phase 3: Framework Support (Tasks 9–12)

### Task 9: Angular Framework Profile
**Files:** `src/frameworks/angular.rs`, `src/frameworks/mod.rs`

Angular-specific entry points and conventions:
- **Markers:** `angular.json`
- **Entry matchers:**
  - Files with `@Component`/`@Directive`/`@Pipe`/`@Injectable`/`@NgModule` decorators (detected by filename convention: `.component.ts`, `.directive.ts`, `.pipe.ts`, `.service.ts`, `.module.ts`, `.guard.ts`, `.interceptor.ts`, `.resolver.ts`)
  - `main.ts`, `environments/`
  - Routing files: files ending in `-routing.module.ts` or `routes.ts`
- **Implicit matchers:**
  - `.spec.ts` test files
  - `test.ts`, `test-setup.ts`
  - `.stories.ts` (Storybook)
  - `proxy.conf.json`, `karma.conf.js`
- **Gotchas:** Template-bound properties not traced (informational)

### Task 10: NestJS Framework Profile
**Files:** `src/frameworks/nestjs.rs`, `src/frameworks/mod.rs`

NestJS-specific entry points:
- **Markers:** `nest-cli.json`
- **Entry matchers:**
  - Files with `@Module`/`@Controller`/`@Injectable`/`@Guard`/`@Interceptor`/`@Pipe` decorators
  - Convention: `.module.ts`, `.controller.ts`, `.service.ts`, `.guard.ts`, `.interceptor.ts`, `.pipe.ts`, `.middleware.ts`, `.decorator.ts`, `.provider.ts`, `.factory.ts`
  - `main.ts`
- **Implicit matchers:**
  - `.spec.ts`, `.e2e-spec.ts`
  - `test/` directory
  - `.entity.ts`, `.dto.ts` (data files, often referenced reflectively)

### Task 11: Angular + NestJS Integration Tests
**Files:** `fixtures/angular-project/`, `fixtures/nestjs-project/`, `tests/integration.rs`

Create fixture projects and integration tests:
- `fixtures/angular-project/`: `angular.json`, `src/app/app.component.ts`, `src/app/app.module.ts`, `src/app/dead.component.ts`, `main.ts`, `.spec.ts` files
- `fixtures/nestjs-project/`: `nest-cli.json`, `src/app.module.ts`, `src/cats/cats.controller.ts`, `src/cats/cats.service.ts`, `src/orphan/orphan.service.ts`, `.spec.ts` files
- Tests verify: entry points detected, dead code found, unused exports flagged, spec files treated as implicit

### Task 12: Framework-Specific Gotchas & Health Scores
**Files:** `src/issues/gotchas.rs`, `src/analyzer.rs`

Add framework-specific gotcha rules and per-framework health scoring:
- **Angular gotchas:** Component without template, standalone mixing with NgModule, unused `providedIn`
- **NestJS gotchas:** Controller without module registration, circular dependency between modules
- **Health score:** per-detector score (0-100) aggregated into overall health, weighted by severity
- `AnalysisOutput` gets a new `health` section with breakdown

---

## Phase 4: AI Integration & Testing (Tasks 13–16)

### Task 13: Statico Skill for Pi Agent
**Files:** `.pi/skills/statico/SKILL.md`

Create a pi agent skill that teaches AI agents how to use statico:
- When to invoke: before/after refactoring, during code review, CI setup
- How to interpret results (confidence scores, informational vs actionable)
- How to run diff reports to see impact of changes
- Example prompts: "check if my refactoring introduced dead code"
- JSON schema reference for programmatic consumption
- Integration with `/plan` and `/subagent` workflows

### Task 14: AI Agent Integration Tests
**Files:** `tests/ai_integration.rs`, `tests/fixtures/ai-test-project/`

Test that AI agents can meaningfully interact with statico output:
- Fixture project with known issues at known locations
- Test: `statico analyze` produces valid JSON with expected structure
- Test: `statico analyze --format markdown` is human-readable
- Test: `statico diff` correctly identifies new/fixed issues
- Test: JSON output round-trips through serde (schema validation)
- Test: `--min-confidence` correctly filters results

### Task 15: Benchmarking Suite
**Files:** `benches/analyze_bench.rs`, `Cargo.toml`

Add criterion benchmarks:
- Benchmark: parsing 100 files, 1000 files, 5000 files (synthetic fixtures)
- Benchmark: individual detectors (dead_code, unused_exports, duplication)
- Benchmark: HTML vs JSON output formatting
- Track memory usage with `jemalloc` feature flag
- Add `[[bench]]` section to Cargo.toml
- Store baseline results for regression detection

### Task 16: Property-Based & Fuzz Testing
**Files:** `tests/property_tests.rs`, `tests/fuzz_targets.rs`

Add robustness testing:
- Property tests with `proptest`: random AST structures, ensure no panics
- Fuzz the parser: random UTF-8 strings fed to tree-sitter, verify graceful handling
- Property: dead_code results are always a subset of all source files
- Property: unused_exports are always a subset of all exports
- Property: duplication_percentage is always 0.0–100.0

---

## Phase 5: Polish & DX (Tasks 17–20)

### Task 17: Filter & Exclude CLI Flags
**Files:** `src/main.rs`, `src/discovery.rs`, `src/types.rs`

Add filtering CLI options:
- `--exclude "node_modules/**,dist/**,.next/**"` — glob patterns to skip
- `--only "issues.dead_code,issues.unused_exports"` — only run specific detectors
- `--framework nextjs` — force framework (skip auto-detection)
- `--no-default-excludes` — don't auto-exclude node_modules, dist, .git, etc.
- These enable AI agents to run targeted analysis efficiently

### Task 18: Config File Support (.statico.toml)
**Files:** `src/config.rs`, `src/main.rs`, `Cargo.toml`

Add `.statico.toml` project config:
```toml
[exclude]
patterns = ["generated/**", "**/*.generated.ts"]

[framework]
force = "nextjs"  # skip auto-detection

[detectors]
dead_code = { enabled = true, min_confidence = 0.8 }
unused_exports = { enabled = true }
duplication = { enabled = false }  # disable slow detector

[output]
format = "html"
cache_dir = ".statico-cache"
```
- Add `toml` dependency
- Merge CLI flags > config file > defaults

### Task 19: CI/CD Integration Templates
**Files:** `templates/github-action.yml`, `templates/gitlab-ci.yml`, `docs/ci-integration.md`

Provide ready-to-use CI templates:
- **GitHub Actions:** Run on PR, post comment with diff report, upload SARIF to code scanning
- **GitLab CI:** Run in pipeline, generate HTML report as artifact
- **Documentation:** How to set up in 5 minutes, how to customize thresholds
- Exit code semantics for CI: 0=clean, 1=warnings exceeded threshold, 2=errors

### Task 20: Interactive TUI Mode
**Files:** `src/tui.rs`, `src/main.rs`, `Cargo.toml`

Add `statico tui <path>` subcommand — terminal UI for exploring results:
- Use `ratatui` crate for terminal rendering
- Overview screen: health score, issue counts, dup % gauge
- Drill-down: select issue category → list issues → select issue → show context
- Key bindings: `/` to filter, `j/k` to navigate, `q` to quit, `d` to show diff
- Colors: green/yellow/red by severity
- Enables developers to explore findings without leaving the terminal

---

## Dependency Graph

```
Phase 1 (Output):     1 → 2 → 3 (formats build on OutputFormatter trait)
                       4 (independent)
                       5 (depends on 4 for JSON structure)
Phase 2 (Perf):       6 → 7 (parallel first, then cache on top)
                       8 (independent)
Phase 3 (Frameworks): 9, 10 (parallel) → 11 (tests) → 12 (gotchas)
Phase 4 (AI/Test):    13 (depends on 1,4 for skill docs)
                       14 (depends on 1,2,3,5)
                       15, 16 (independent)
Phase 5 (Polish):     17, 18 (independent, can parallelize)
                       19 (depends on 1 for SARIF, 4 for exit codes)
                       20 (depends on 1 for data access)
```

## New Dependencies

| Crate | Task | Purpose |
|---|---|---|
| `rayon` | 6 | Parallel file processing |
| `bincode` | 7 | Binary cache serialization |
| `sha2` | 7 | Content hashing for cache |
| `indicatif` | 8 | Progress bar |
| `toml` | 18 | Config file parsing |
| `ratatui` + `crossterm` | 20 | Terminal UI |
| `criterion` | 15 | Benchmarking |
| `proptest` | 16 | Property testing |
