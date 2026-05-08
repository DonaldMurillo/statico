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

---

<!--
This file is the body of every release. Edit it before tagging to call out
breaking changes or notable additions; the changelog is in CHANGELOG.md.
-->

> ⚠️ **Early alpha — not production-ready.** Output schemas, plugin protocol,
> and CLI flags can change between releases until `v1.0.0`. Pin a version if
> you depend on it.

See [`CHANGELOG.md`](https://github.com/DonaldMurillo/statico/blob/main/CHANGELOG.md) for the full list of changes.

## Install

### Cargo

```bash
cargo install statico
```

### npm

```bash
npm install -D @statico/cli
npx statico analyze .
```

### Direct download

Pick the matching tarball below, untar, and put `statico` on your `PATH`.

```bash
# Example: macOS arm64
curl -fsSL https://github.com/DonaldMurillo/statico/releases/latest/download/statico-macos-aarch64.tar.gz \
  | tar -xz
sudo install -m 0755 statico /usr/local/bin/statico
```

> **macOS quarantine note:** if you downloaded the tarball through a browser,
> macOS may refuse to run the unsigned binary with an "unidentified developer"
> dialog. Clear the quarantine flag with:
>
> ```bash
> xattr -d com.apple.quarantine /usr/local/bin/statico
> ```
>
> The `npx`, `cargo install`, and `curl | tar` paths above are not affected —
> only Safari/Chrome downloads set the quarantine attribute.

## Verifying the download

Each release includes a `SHASUMS256.txt` listing the SHA-256 of every tarball,
plus a per-asset `*.sha256` file.

```bash
curl -fsSL https://github.com/DonaldMurillo/statico/releases/latest/download/SHASUMS256.txt | shasum -a 256 -c --ignore-missing
```