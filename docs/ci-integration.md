# CI integration

Run statico in CI to catch dead code, unused exports, and framework gotchas
before they ship. statico fits anywhere you can run a binary and read JSON or
SARIF — examples below for GitHub Actions, GitLab CI, and Docker.

> ⚠️ **Pre-1.0.** Output schemas can shift between minor releases. Pin a
> statico version in CI (`v0.1.x`) instead of `latest` if your downstream
> tooling is sensitive to schema drift.

[← Back to README](../README.md)

---

## Table of contents

1. [GitHub Actions — the official Action](#github-actions--the-official-action)
2. [GitHub Actions — manual install](#github-actions--manual-install)
3. [Exit-code semantics](#exit-code-semantics)
4. [Baseline-gated CI (recommended)](#baseline-gated-ci-recommended)
5. [SARIF + GitHub Code Scanning](#sarif--github-code-scanning)
6. [GitLab CI](#gitlab-ci)
7. [Docker](#docker)
8. [Confidence thresholds](#confidence-thresholds)
9. [Monorepo tips](#monorepo-tips)

---

## GitHub Actions — the official Action

The repo ships a composite action at `.github/actions/statico/`. From any
other repo:

```yaml
name: statico
on: [push, pull_request]

permissions:
  contents: read
  security-events: write   # only if you upload SARIF

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: DonaldMurillo/statico/.github/actions/statico@v0.1.1
        with:
          format: sarif
          min-confidence: '0.7'
          exit-code: 'false'           # set 'true' to fail the build
          upload-sarif: 'true'         # default; set 'false' to skip Code Scanning
```

The Action installs the prebuilt binary for the runner's OS/arch
(macOS / Linux × x86_64 / aarch64) — no Rust toolchain needed.

**Inputs:**

| Name | Default | Description |
|---|---|---|
| `version` | `latest` | Release tag (e.g. `v0.1.1`) or `latest` |
| `path` | `.` | Project path to analyze |
| `format` | `sarif` | Any `--format` value: `json`, `sarif`, `markdown`, `html`, `ai`, `context`, `mermaid`, `pr-comment`, `fix` |
| `output-file` | `statico-results.sarif` | Where to write the analysis |
| `min-confidence` | `0.0` | Drop issues below this confidence |
| `exit-code` | `false` | Fail the step on any reported issue |
| `upload-sarif` | `true` | When format is `sarif`, upload to Code Scanning |

**Outputs:**

| Name | Description |
|---|---|
| `output-file` | Path of the file the action wrote |

---

## GitHub Actions — manual install

If you can't use the composite action (custom runner, hardened policy,
mirror), install the binary directly:

```yaml
jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install statico
        run: |
          # Pin a version — `latest` will follow new releases including breaking changes.
          VERSION=v0.1.1
          os=linux ; arch=$(uname -m | sed 's/aarch64/aarch64/;s/x86_64/x86_64/')
          curl -fsSL "https://github.com/DonaldMurillo/statico/releases/download/${VERSION}/statico-${os}-${arch}.tar.gz" \
            -o /tmp/statico.tar.gz
          tar -xzf /tmp/statico.tar.gz -C /tmp \
            --no-same-owner --no-same-permissions ./statico
          sudo install -m 0755 /tmp/statico /usr/local/bin/statico

      - name: Analyze
        run: statico analyze . --format sarif --min-confidence 0.7 > results.sarif

      - name: Upload SARIF
        uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: results.sarif
          category: statico
```

Or via npm:

```yaml
- uses: actions/setup-node@v4
  with: { node-version: '22' }
- run: npm install -g @statico/cli
- run: statico analyze . --format sarif > results.sarif
```

---

## Exit-code semantics

| Exit code | Meaning |
|---|---|
| `0` | Analysis completed. Issues may still be present — this is the default. |
| `1` | `--exit-code` was set and at least one issue passed `--baseline` + `--min-confidence` filtering. |
| Non-zero (other) | Internal error (panic, bad arguments, IO failure). |

By default `statico analyze` does **not** gate on findings. Add `--exit-code`
when you want CI to fail on issues:

```bash
statico analyze . --min-confidence 0.7 --exit-code
```

To allow the pipeline to continue past a non-zero exit (e.g. so you can
upload the report as an artifact), use `continue-on-error: true` in GitHub
Actions or `|| true` in shell scripts.

---

## Baseline-gated CI (recommended)

Naive `--exit-code` gates fail the moment anyone introduces a new — even
harmless — finding. Use a baseline file so only **new** issues fail the
build.

```bash
# One-time, locally:
statico analyze . --update-baseline statico-baseline.json --min-confidence 0.7
git add statico-baseline.json
git commit -m "chore: statico baseline"
```

```yaml
- run: statico analyze . --baseline statico-baseline.json --min-confidence 0.7 --exit-code
```

To accept new findings (after a deliberate refactor), regenerate the
baseline locally and commit it.

The baseline file is a list of stable per-issue fingerprints — see
[Configuration](configuration.md) for the schema.

---

## SARIF + GitHub Code Scanning

`--format sarif` produces SARIF 2.1.0, which Code Scanning consumes natively
and surfaces inline on PRs and in the **Security → Code scanning** tab.

```yaml
permissions:
  contents: read
  security-events: write    # required for upload-sarif

jobs:
  scan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: DonaldMurillo/statico/.github/actions/statico@v0.1.1
        with:
          format: sarif
          upload-sarif: 'true'
```

The Action's `upload-sarif: true` (default) wraps `github/codeql-action/upload-sarif@v3`
with `category: statico` so multiple analyzers don't collide.

**SARIF mapping** (rule IDs you'll see in alerts):

| Rule ID | Source category |
|---|---|
| `dead_code` | `issues.dead_code` |
| `unused_export` | `issues.unused_exports` |
| `unused_type` | `issues.unused_types` |
| `duplicate_export` | `issues.duplicate_exports` |
| `duplicate_code` | `issues.duplicate_code` |
| `gotcha` | `issues.gotchas` |
| `circular_dependency` | `issues.circular_dependencies` |
| `unused_dependency` | `issues.unused_dependencies` |
| `unlisted_dependency` | `issues.unlisted_dependencies` |
| `unresolved_import` | `issues.unresolved_imports` |

---

## GitLab CI

```yaml
stages: [analyze]

statico:
  stage: analyze
  image: ubuntu:24.04
  before_script:
    - apt-get update && apt-get install -y curl ca-certificates
    - VERSION=v0.1.1
    - arch=$(uname -m); [ "$arch" = "aarch64" ] || arch=x86_64
    - curl -fsSL "https://github.com/DonaldMurillo/statico/releases/download/${VERSION}/statico-linux-${arch}.tar.gz" -o /tmp/s.tgz
    - tar -xzf /tmp/s.tgz -C /tmp --no-same-owner --no-same-permissions ./statico
    - install -m 0755 /tmp/statico /usr/local/bin/statico
  script:
    - statico analyze . --format markdown
    - statico analyze . --format html > report.html
    - statico analyze . --baseline statico-baseline.json --min-confidence 0.7 --exit-code
  artifacts:
    paths: [report.html]
    expire_in: 30 days
    when: always
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
```

Download `report.html` from **CI/CD → Pipelines → [pipeline] → statico job
→ Job artifacts**.

---

## Docker

If you need an isolated runtime, build statico into a slim image:

```dockerfile
FROM rust:1.91-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --bin statico

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/statico /usr/local/bin/statico
ENTRYPOINT ["statico"]
CMD ["analyze", ".", "--format", "markdown"]
```

```bash
docker build -t statico:local .
docker run --rm -v "$(pwd):/project" -w /project statico:local \
  analyze . --format sarif --min-confidence 0.7 > results.sarif
```

For most cases the prebuilt tarball is faster than building inside Docker
on every run — pick the build approach only when you need image isolation.

---

## Confidence thresholds

statico assigns a confidence score (0.0–1.0) to each issue. Use
`--min-confidence` to drop low-signal noise:

| Threshold | Use case |
|---|---|
| `0.9` | Release branches — only critical issues |
| `0.7` | Default for PR review — balanced (matches Action default) |
| `0.5` | Development — catch potential issues early |
| `0.0` | Full audit — show everything |

The gotcha detector emits many low-confidence patterns (e.g. `console.log`
in non-test code at 0.4). At 0.7 most of those drop out. Tune to taste.

---

## Monorepo tips

### Analyze one subproject

```bash
statico analyze ./apps/web
```

### Exclude paths

Add to `.statico.toml`:

```toml
exclude = [
  "vendor/**",
  "**/*.generated.ts",
  "apps/legacy/**",
]
```

CLI flag form is not currently available — exclude lists must live in
`.statico.toml`.

### Per-project matrix

```yaml
strategy:
  matrix:
    project: [apps/web, apps/mobile, packages/ui]
steps:
  - uses: actions/checkout@v4
  - uses: DonaldMurillo/statico/.github/actions/statico@v0.1.1
    with:
      path: ${{ matrix.project }}
      output-file: results-${{ matrix.project }}.sarif
```

### Path-filter the workflow

Only run when relevant files change:

```yaml
on:
  pull_request:
    paths:
      - 'apps/web/**'
      - 'packages/shared/**'
```

### Per-project thresholds

```yaml
# Strict for production code
- run: statico analyze ./apps/api --min-confidence 0.9 --exit-code

# Relaxed for prototypes
- run: statico analyze ./apps/playground --min-confidence 0.5
```
