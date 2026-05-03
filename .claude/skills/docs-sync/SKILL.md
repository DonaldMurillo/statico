---
name: docs-sync
description: Keep statico's user-facing documentation in lockstep with code changes. Auto-invoke proactively whenever the user adds, removes, or modifies CLI flags, subcommands, config keys (`.statico.toml` / `StaticoConfig`), output formats, plugin hooks, plugin protocol fields, the public Rust API (`src/lib.rs` re-exports), template content (`templates/`), or framework profiles. Also invoke when bumping `Cargo.toml` `version`, `rust-version`, or dependency versions that change behavior, and when the user mentions "ship", "release", "publish", or "the docs are stale". Verifies README.md, CHANGELOG.md, the files under `docs/`, the templates under `templates/`, and the example plugin under `examples/plugins/`. Default action is to AUDIT and report drift; only edit when the user confirms or the drift is mechanically obvious (e.g., the README CLI table is missing a flag we just merged).
---

# docs-sync

Statico has a lot of doc surface — README, `docs/`, `CHANGELOG`, templates,
plugin examples, the GitHub Action README, the npm wrapper README — and it
drifts fast. This skill exists so docs stay accurate without me asking
every time.

## When to use

Auto-trigger this skill when the conversation involves any of:

- A change to `src/main.rs` `Commands` enum (subcommand added/removed/renamed)
- A change to a `Commands::Foo { … }` variant's `#[arg]` set (flag
  added/removed/renamed/default-changed)
- A change to `src/config.rs` (`.statico.toml` schema)
- A change to `src/output/mod.rs` or any `src/output/*.rs` (new format,
  removed format, behavior change in an existing format)
- A change to `src/plugin/protocol.rs` (plugin protocol fields)
- A change to `src/lib.rs` (`pub mod`/`pub use` — affects the public Rust API)
- A change to `templates/**` (the content `statico setup` writes)
- A change to `Cargo.toml` (`version`, `rust-version`, `description`,
  dependency adds/removes)
- A change to `.github/workflows/release.yml` or `.github/actions/statico/**`
  (changes how users install or use the GitHub Action)

Phrases like "ship it", "release this", "publish", "cut a tag",
"the docs are stale", or "is this in the docs" should also trigger.

Do **not** trigger for changes that are doc-only, test-only, or internal
refactors that don't touch any of the above surfaces.

## What to do

1. **Identify the change.** Read the diff (or ask the user if their request
   doesn't make it obvious). Bucket it into one of:
   - new flag or subcommand
   - new config key
   - new output format
   - new plugin hook or protocol change
   - new public Rust export
   - new template content
   - dependency / metadata bump

2. **Map to doc surfaces.** Use this table — it's the source of truth for
   "where does X get documented":

   | Change kind | Surfaces to check |
   |---|---|
   | New `Commands::Foo` subcommand | `README.md` (CLI reference table), `docs/getting-started.md` (Quick start), maybe `docs/ci-integration.md` if it's CI-relevant, `CHANGELOG.md` |
   | New `#[arg]` flag on existing subcommand | `README.md` (matching flag table for that subcommand), `docs/configuration.md` if it has a config equivalent, `CHANGELOG.md` |
   | New / changed `StaticoConfig` field | `docs/configuration.md` (schema table), `README.md` (Configuration section), `CHANGELOG.md` |
   | New output format | `README.md` (Output formats table), `docs/output-formats.md`, `CHANGELOG.md` |
   | Plugin protocol change | `docs/plugins.md` (Protocol section), `examples/plugins/coverage-gap/index.ts` if affected, `sdks/typescript/src/index.ts` (the SDK types should already match), `CHANGELOG.md` |
   | New `pub mod` / `pub use` in `src/lib.rs` | nothing user-facing usually, but flag for review if it's a major API addition |
   | `templates/` change | `templates/README.md` (layout doc) if filenames changed, `CHANGELOG.md` |
   | `Cargo.toml` version bump | `CHANGELOG.md` (move `[Unreleased]` → version), `npm/package.json` version, `.github/release-notes.md` review |
   | `Cargo.toml` description / metadata | `npm/package.json` description, `README.md` opening line, `.github/release-notes.md`, the `gh repo edit --description` value |
   | `Cargo.toml` `rust-version` | `README.md` Prerequisites, `CONTRIBUTING.md` if it mentions a version |
   | New `.github/workflows/` or composite Action input | `.github/actions/statico/README.md`, `README.md` GitHub Action snippet, `docs/ci-integration.md` |

3. **Audit, don't auto-rewrite.** Read each surface in the table and
   compare against the change. Default mode is **report drift**:

   - List files that need an update.
   - For each, summarize the gap in one sentence.
   - Show the user the planned edits (a paragraph each, or the diff).

   Only auto-apply when the gap is **mechanically obvious** and trivially
   safe — for instance, "the new `--apply` flag isn't in the
   `statico fix` flag table in README". For anything that involves
   prose rewriting, get the user's confirmation first.

4. **Stay consistent with positioning.** statico's voice is:
   - Pre-1.0 alpha. Don't promise stability.
   - Customizable + AI-forward. Don't claim "fastest".
   - Honest. Comparative tables list what statico doesn't do, too.

   The README's opening alpha banner and the "What statico is not good at"
   section are deliberate — keep them when editing the README.

5. **Cross-link.** When adding a section, hyperlink to / from related
   docs. Specifically:
   - `README.md` should always link to `docs/configuration.md`,
     `docs/output-formats.md`, `docs/plugins.md` from the relevant
     sections.
   - Each `docs/*.md` should link back to `README.md` once at the top.
   - `examples/plugins/coverage-gap/README.md` should reference
     `docs/plugins.md`.

6. **Keep templates in sync.** If the change affects what `statico setup`
   should write into a user's project (e.g. a new format worth mentioning
   in the `statico-analyze` skill), update the matching file in
   `templates/`. The Rust code does `include_str!` at compile time, so
   no codegen step — just edit the markdown.

## What not to do

- Don't rewrite the audit doc (`docs/audit-2026-05.md`). It's a frozen
  historical record. New audits get new files.
- Don't touch `docs/internal/` — that directory is gitignored and the
  user's working notes.
- Don't add cargo or npm install commands that won't work yet (e.g.
  `cargo install statico` only works after the crate is published — it's
  fine in user-facing docs *now* because v0.1.0 publish is the next step,
  but if someone bumps to 0.2.x we need to confirm the publish landed).
- Don't ship a doc change without running `cargo test --doc` if the doc
  contains compile-checked Rust examples (today there aren't any, but if
  someone adds them, run the test).

## Reporting format

When you finish the audit, summarize like:

```
docs-sync audit for <change>:

✓ already up to date:
  - <surface>

⚠ drift detected:
  - README.md:<section> — <one-line gap>
  - docs/configuration.md — <one-line gap>

→ proposed edits:
  - <file> — <what you'd change>

Apply now? (y / n / show-me-the-diff)
```

Then act on the user's response. Keep edits batched into one commit
per drift category — don't fragment into many tiny commits.

## Quick checklist (paste at end of every audit)

- [ ] README.md table for the affected subcommand
- [ ] README.md "Output formats" table if format-related
- [ ] docs/<relevant>.md
- [ ] CHANGELOG.md `[Unreleased]` entry
- [ ] templates/ if user-facing instructions change
- [ ] examples/plugins/coverage-gap/ if plugin protocol changed
- [ ] .github/actions/statico/README.md if Action inputs changed
- [ ] npm/README.md if install or top-level positioning changed
