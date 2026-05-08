# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.6] - 2026-05-08

### Added — Test coverage

Closing 9 of the 10 audit-identified test gaps. Total tests went from
606 → 626 + 1 ignored (a real bug surfaced — see Fixed/Known Issues).

- `tests/integration_cli.rs`: `cli_fix_*` (3 tests) cover `statico fix`'s
  dry-run and `--apply` paths — the only state-mutating subcommand had
  zero coverage. Asserts the documented dry-run safety contract and
  that `--apply` strips the right `export` keyword while leaving the
  used one intact.
- `tests/integration_cli.rs`: `cli_diff_*` (2 tests) cover `statico
  diff` happy path + non-zero exit on new issues.
- `tests/integration_cli.rs`: `cli_baseline_*` (3 tests) cover
  `--update-baseline` schema, end-to-end `--baseline + --exit-code`
  gating, and forward-incompatible-version diagnostics. This was the
  recommended CI gate with no coverage.
- `tests/integration_cli.rs`: `cli_watch_reanalyzes_on_file_change`
  (1 test) spawns `analyze --watch`, edits a source file, and asserts
  ≥2 JSON runs stream from stdout within 20s.
- `tests/output_tests.rs`: `test_context_formatter_*`,
  `test_mermaid_formatter_*`, `test_pr_comment_formatter_smoke` (5
  tests) cover the three formatters that previously had no test file
  referencing the format string.
- `tests/integration_analyze.rs`:
  `shadcn_detects_components_json_and_registry_entries` covers the
  shadcn framework profile + the new `fixtures/shadcn-project/`.
- `tests/integration_plugin.rs`:
  `sec_plugin_list_rejects_path_traversal_via_config` exercises
  `ensure_within_root` rejection at the CLI surface (not just the
  unit test).
- `src/plugin/runtime.rs`: `verify_archive_sha` extracted as a pure
  function with 4 unit tests. The Bun-runtime SHA-256 integrity gate
  (audit S4.1) is now provably enforced — accepts case-insensitive +
  whitespace-tolerant matches, rejects mismatch with the canonical
  error string, rejects empty/whitespace-only expected values.

### Fixed

- **Plugin override conflicts no longer race on stdin/stdout.**
  `validate_overrides` in `src/plugin/discovery.rs` had passing unit
  tests but was never called from any runtime path. Two plugins both
  declaring `override` on the same hook used to load successfully,
  race each other for the request stream, and panic at
  `src/plugin/manager.rs:114` with `stdout already taken (concurrent
  send_request?)`. `PluginPipeline::new` now invokes
  `validate_overrides` immediately after init; on conflict, every
  plugin that declared `override` is dropped with a clear warning
  naming the culprits, and analysis continues with the remaining
  plugins. The previously `#[ignore]`d
  `plugin_override_conflict_surfaces_at_analyze` integration test is
  now active and asserts both the conflict warning and the absence of
  the panic.
- `clippy::unwrap_used` and `clippy::expect_used` are now `#[warn]` at
  the crate root (`src/lib.rs`). Every existing module that uses
  `unwrap()` / `expect()` carries a per-file
  `#![allow(clippy::unwrap_used, clippy::expect_used)]` — each one is
  a future cleanup target, but the lint now blocks new uses in any
  fresh module. CI's `cargo clippy --all-targets -- -D warnings`
  remains green.

### Changed
- Architecture cleanup: removed the `src/analyzer/parse_typescript.rs` gravestone,
  moved the Rust parser into `src/languages/rust_parser.rs`, restructured
  `src/issues/duplicate_code/` into a directory (drops the `#[path]`
  indirection), renamed top-level `src/monorepo.rs` to `src/workspace.rs`,
  and split `tests/integration.rs` (1071 lines) into per-command files
  under `tests/integration_*.rs` sharing helpers via `tests/common/mod.rs`.
- `src/main.rs` is now a 7-line dispatch shim; clap definitions live in
  `src/commands/cli.rs`.
- Plugin scaffolding templates (`statico plugin init`) moved out of
  inline Rust string literals into `templates/plugin/{ts,rust,python}/`.
  Side effect: fixes pre-existing scaffold bugs (Python triple-quote
  escaping, Rust `Cargo.toml` line collapse).
- `templates/CLAUDE.md` (shipped to user repos via `statico setup`) and
  the in-repo `CLAUDE.md` rewritten to remove content derivable from
  `ls` / `cat Cargo.toml` / `git log`.
- `templates/skills/statico-plugin/SKILL.md` got `name` + `description`
  frontmatter (so it auto-discovers in Claude Code) and was trimmed of
  duplicated SDK boilerplate that already lives in `docs/plugins.md`.
- `src/lib.rs`: every internal module is now `#[doc(hidden)]`. The public
  API surface is intentionally undocumented until a stability commitment
  for `1.0.0`.

### Added
- `.editorconfig`, `clippy.toml` (`msrv = "1.91"`), `.typos.toml`,
  `deny.toml`.
- New `hygiene` CI job runs `typos`, `cargo-deny check advisories
  licenses sources bans`, and `cargo-machete` (advisory-only).
- New `windows-build` CI job builds the binary and runs `cargo test
  --lib` on `windows-latest` so build regressions caught at release
  time get caught at PR time instead.
- Pre-commit hook also runs `bun run check` against the TypeScript SDK
  when SDK files are staged.
- Pre-push hook now skips clippy + tests for docs-only / config-only
  pushes (no `.rs` / `Cargo.{toml,lock}` changes).
- New byte-identical snapshot test
  (`cli_setup_output_matches_templates_byte_for_byte`) catches drift
  between `templates/` and what `statico setup` writes.

### Removed
- Public `docs/audit-2026-05.md` and the matching link in
  `docs/getting-started.md`. The internal copy under `docs/internal/`
  (gitignored) is unaffected.

## [0.1.5] - 2026-05-08

### Fixed
- tsconfig discovery: `load_all_tsconfig_paths` now matches `tsconfig*.json` (not just
  `tsconfig.json`), so `tsconfig.base.json` used by Nx/Turborepo/Lerna monorepos is
  picked up automatically.
- Root tsconfig loading: `build_resolver` now also checks `tsconfig.base.json` at root.
- Free `resolve_import()` function now falls through to the plugin resolver when
  relative resolution fails — callers in `src/issues/` no longer silently bypass
  the plugin hook for non-relative imports.

### Added
- Debug logging when plugin resolver is consulted in `Resolver::resolve()` step 5.
- 2 new tests: tsconfig glob matching + free function plugin fallback.

## [0.1.4] - 2026-05-08

### Fixed
- Windows release build: `shasum` not available on Windows runner — use
  `sha256sum` as fallback for per-asset checksum generation.
- Windows cross-compilation: added `BUN_URL_TEMPLATE` cfg for `target_os = "windows"`
  so the release build succeeds on all 5 targets.
- Release notes now generated from CHANGELOG.md for GitHub releases.

### Added
- `.github/release-notes-template.md` holds static install/verification boilerplate;
  `scripts/release.sh` prepends the version's changelog section automatically.

## [0.1.3] - 2026-05-08

### Fixed
- `resolve_import` plugin hook wired into the analysis pipeline.
- `analyze_file` dispatch now guarded by `has_hook(&HookName::AnalyzeFile)`.
- macOS Bun download URL fixed (`bun-{arch}.zip` → `bun-darwin-{arch}.zip`).
- `.gitignore` mutation now checks `.git/info/exclude` before writing.

### Added
- 11 new tests (407 → 418 unit tests, 603 total).

## [0.1.2] - 2026-05-03

## [0.1.1] - 2026-05-03

## [0.1.0] - 2026-05-03

## [0.0.0-rc1] - 2026-05-03

## [0.0.0-rc1] - 2026-05-03

### Added

- TypeScript plugin SDK exposes `Plugin.onInit(handler)` so plugin authors
  can read `pluginSettings` from `.statico.toml` before any hook fires.
- Multi-hook plugin example at `examples/plugins/coverage-gap/` —
  uses `init` + `analyze_file` + `post_analysis` together with
  configurable settings. Documented in `docs/plugins.md`.
- `.claude/skills/docs-sync/` — proactive skill that audits doc surfaces
  whenever CLI flags, config keys, output formats, or the plugin protocol
  change.

### Changed

- Repositioned README and metadata. statico now leads with **early-alpha,
  customizable, AI-forward** rather than "single fast Rust binary". Honest
  comparisons against knip / oxlint added; an explicit "What statico is not
  good at" section calls out speed and pre-1.0 stability.
- Cargo and npm package descriptions updated to match.
- `docs/configuration.md` rewritten — the previous file documented a
  `[analysis]` / `[framework]` / `[duplication]` schema that doesn't exist.
  Current schema is flat top-level keys plus `[[plugin]]` arrays.
- `docs/output-formats.md` and `docs/getting-started.md` refreshed for the
  current command surface (`fix`, `--watch`, `--baseline`, tty-aware
  default format).
- `docs/plugins.md` refreshed; points at the new complex example.

- `statico fix` subcommand for safe automated fixes (unused exports + unused
  npm dependencies, dry-run by default, `--apply` to write).
- `analyze --baseline <path>` and `analyze --update-baseline <path>` to
  ratchet down issues over time without per-PR noise.
- `analyze --watch` re-runs analysis on file changes (uses the existing
  incremental cache).
- TTY-aware default output format — `analyze` defaults to `markdown` on a
  terminal, `json` when piped.
- Bun runtime download now logs SHA-256 and verifies against the optional
  `STATICO_TRUSTED_BUN_SHA256` env var. Extraction is pure-Rust (no `unzip`
  subprocess).
- Official GitHub Action at `.github/actions/statico/` and a sample workflow.
- npm wrapper (`@statico/cli`) so JS-first teams can run `npx statico`.
- Default `is_skipped_dir` list extended with `.angular`, `.svelte-kit`,
  `.vite`, `.parcel-cache`, `.astro`, `.docusaurus`, `.expo`, `.vercel`,
  `.serverless`, `.output`, `__pycache__`, `.pytest_cache`, `.mypy_cache`,
  `.ruff_cache`, `vendor`, `bower_components`, `.yarn`, `.terraform`,
  `.hg`, `.svn`. Cuts a fresh-clone Angular project from ~195k LoC to
  ~21k and lifts the health score from 0/100 to 48/100.
- `LICENSE-MIT`, `LICENSE-APACHE`, `CONTRIBUTING.md`, `SECURITY.md`,
  `CHANGELOG.md`, and full Cargo.toml metadata.

### Changed

- `Cargo.toml` description now mentions Rust (was TypeScript-only).
- `CLAUDE.md` parser claim corrected — statico uses tree-sitter, not oxc
  (oxc_resolver is an optional `deep-resolution` feature).
- Fish shell init snippet now wraps paths in single quotes; bash
  `source` line is quoted too (paths with spaces survive).

### Fixed

- `cargo clippy --all-targets -- -D warnings` is now clean (was 70+ errors
  in test code).
- Removed unused `PluginPipeline.root` field that broke
  `RUSTFLAGS=-D warnings` builds.

## [0.1.0] - TBD

Initial public release.