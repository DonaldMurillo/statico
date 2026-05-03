# Getting started

> ⚠️ **statico is in early alpha.** Use it on real projects, but pin a version
> in CI — output schemas and CLI flags can change before `v1.0.0`.

statico is a static code analyzer for TypeScript and Rust projects. It detects
dead code, unused exports/types, circular dependencies, code duplication, and
framework-specific gotchas, then computes a 0–100 health score. Outputs are
designed for both humans and LLMs.

[← Back to README](../README.md)

---

## Installation

### Cargo (any platform with Rust)

```bash
cargo install statico
```

### npm (macOS / Linux × x86_64 / aarch64)

```bash
npm install -D @statico/cli
npx statico analyze .
```

### Prebuilt release tarball

```bash
# macOS arm64 example — pick the matching tarball from the releases page
curl -fsSL https://github.com/DonaldMurillo/statico/releases/latest/download/statico-macos-aarch64.tar.gz \
  | tar -xz
sudo install -m 0755 statico /usr/local/bin/statico
```

> If macOS Gatekeeper blocks a browser-downloaded tarball, run
> `xattr -d com.apple.quarantine /usr/local/bin/statico` once. The
> `cargo install`, `npx`, and `curl | tar` paths above don't trigger this.

### From source

```bash
git clone https://github.com/DonaldMurillo/statico.git
cd statico
cargo install --path .
```

Prerequisites for source builds: [Rust](https://rustup.rs/) 1.91+ (Edition 2024).

---

## Verify the install

```bash
statico --version
statico doctor       # checks PATH, completions, and shell integration
```

If `doctor` reports anything missing, run `statico init` once — it sets up
shell completions and a `st` alias.

---

## Your first analysis

```bash
cd path/to/your-project
statico analyze .
```

In a terminal you'll see a Markdown report with a health-score dashboard and
tables for each issue category. The same command piped to a file produces
JSON instead, so it's safe in scripts:

```bash
statico analyze . > report.json
```

Run with `--format ai` for a compressed (~500 token) summary suitable for
feeding to a coding assistant:

```bash
statico analyze . --format ai
```

---

## Tune the noise floor

By default, statico reports every detected issue, including low-confidence
gotchas. To drop everything below a threshold, set `--min-confidence`:

```bash
statico analyze . --min-confidence 0.7
```

`0.7` is a sensible starting point — the gotcha detector emits a lot of
0.4–0.6 stylistic hints that are noisy in CI.

Make it the default in `.statico.toml`:

```toml
min_confidence = 0.7
```

---

## Wire it into CI without flake

The naive approach (`statico analyze . --exit-code`) breaks any time someone
introduces a new — even harmless — finding. Use a baseline file instead:

```bash
# one-time, locally
statico analyze . --update-baseline statico-baseline.json --min-confidence 0.7
git add statico-baseline.json
git commit -m "chore: statico baseline"
```

Then in CI:

```bash
statico analyze . \
  --baseline statico-baseline.json \
  --min-confidence 0.7 \
  --exit-code
```

Only **new** issues fail the build. To accept new findings as the new
baseline, regenerate and commit.

---

## Apply safe automated fixes

`statico fix` removes the `export` keyword from declarations whose export is
unused, and drops unused entries from `package.json`. Default is dry-run —
pass `--apply` to actually rewrite files:

```bash
statico fix .             # dry-run; prints what it would do
statico fix . --apply     # rewrite files
```

It refuses to touch anything ambiguous (named re-exports, `export default`,
`export *`, multiple matches on the same identifier). Skipped items are
listed with a reason.

---

## Where to next

- **[Configuration](configuration.md)** — `.statico.toml` schema reference
- **[Output formats](output-formats.md)** — when to use each `--format`
- **[CI integration](ci-integration.md)** — GitHub Actions, GitLab, SARIF
- **[Plugins](plugins.md)** — write project-specific rules in any language
- **[Audit (May 2026)](audit-2026-05.md)** — current state, known limitations,
  what's stable, what's not
