---
name: release
description: Cut a new statico release. Use when the user says "cut a release", "release vX.Y.Z", "ship X.Y.Z", "publish a release", or otherwise asks to produce a new versioned release of statico. Drives `scripts/release.sh` end-to-end (version bump → tests → commit → tag → optional push) with conservative error handling.
---

# release

Cut a new statico release by driving `scripts/release.sh` from start to finish.

## When to use

Trigger this skill when the user asks to:

- Cut, ship, publish, or tag a new release of statico
- Run `scripts/release.sh`
- Bump the version + tag + push for the project
- "Release vX.Y.Z" / "release 0.1.0" / "let's go 0.2.0"

Do **not** use this skill for:

- Hotfix branches that need a custom version-bump strategy — discuss with the user first
- Pre-release tags from non-`main` / non-`audit/*` branches without explicit confirmation
- `cargo publish` or `npm publish` — those are out of scope; the workflow only attaches GitHub release artifacts

## What you'll do

1. **Parse the version + flags from the user's message.** Common shapes: `release 0.1.0`, `cut v0.1.0`, `release 0.2.0 --push`. If no version is given, ask for one before doing anything.

2. **Preflight in parallel.** Read these and surface anything unusual:
   - `git status --short` — uncommitted changes?
   - `git branch --show-current` — release should normally be cut from `main`
   - Current `[package].version` in `Cargo.toml` — make sure the new one is actually higher (semver)
   - The `## [Unreleased]` section in `CHANGELOG.md` — does it have meaningful entries to release?

   If anything's wrong (dirty tree, unexpected branch, version going backwards, empty changelog), stop and ask the user.

3. **Always start with `--dry-run`.** Show the user the planned changes:

   ```bash
   ./scripts/release.sh <version> --dry-run
   ```

   Then ask the user to confirm before the real run.

4. **Run for real once confirmed.**

   ```bash
   ./scripts/release.sh <version>
   ```

   Pass `--push` only if the user explicitly asked for it.

5. **Report what happened:**
   - `git log -1 --oneline` — the bump commit
   - `git show v<version> --stat | head` — the tag
   - The exact push commands the user should run next:
     ```
     git push origin <branch>
     git push origin v<version>
     ```
   - Note that pushing the tag triggers `.github/workflows/release.yml`, which builds platform tarballs and creates the GitHub Release.

6. **If the user asks you to push**, run both push commands, then suggest:
   - `gh run watch` — follow the release workflow
   - `gh release view v<version>` — verify the release once the workflow finishes

## Failure modes — be conservative

- **Tests fail** → stop. Show the failing test. Never pass `--skip-tests` without explicit user permission.
- **Clippy fails** → stop. Show the lint. Fix it as a separate commit before retrying.
- **Tag already exists** → stop. Ask whether to bump to next patch or delete the existing tag (deleting a published tag is a separate, riskier conversation).
- **Working tree dirty** → never auto-pass `--allow-dirty`. Show `git status` and ask the user to stash, commit, or explicitly opt in.
- **Push rejected because remote is ahead** → stop. Don't force-push without explicit approval.

## Notes on the script

`scripts/release.sh` is BSD-sed-safe, strict-mode (`set -euo pipefail`), and writes via temp+rename so a Ctrl-C never leaves a half-rewritten file. It bumps:

- `Cargo.toml` `[package].version` (only the package section — dependency versions stay put)
- `npm/package.json` top-level `version` (skipped if `node` isn't installed)
- `install/statico.rb` asset URL if the file exists
- `CHANGELOG.md` — splits `## [Unreleased]` into a fresh empty `[Unreleased]` plus a dated `[X.Y.Z]` section containing the previous entries
- `Cargo.lock` (refreshed via `cargo check`)

It then runs `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release`, commits, and tags. Re-running with the same version after a fix is safe *up until* the commit/tag step — once those exist, ask the user whether to amend or proceed.
