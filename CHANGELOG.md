# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
