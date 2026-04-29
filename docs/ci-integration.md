# CI/CD Integration Guide

Integrate **statico** into your continuous integration pipeline to catch TypeScript issues before they reach production.

---

## Table of Contents

1. [Quick Start – GitHub Actions (5 minutes)](#quick-start--github-actions)
2. [Exit Code Semantics](#exit-code-semantics)
3. [SARIF Integration with GitHub Code Scanning](#sarif-integration-with-github-code-scanning)
4. [GitLab CI Setup](#gitlab-ci-setup)
5. [Custom Thresholds with `--min-confidence`](#custom-thresholds-with---min-confidence)
6. [Running in Docker](#running-in-docker)
7. [Caching Strategies for Large Monorepos](#caching-strategies-for-large-monorepos)
8. [Monorepo Tips](#monorepo-tips)

---

## Quick Start – GitHub Actions

Drop the following into `.github/workflows/statico.yml` in your TypeScript project:

```yaml
name: Statico

on:
  pull_request:
    branches: [main]
  push:
    branches: [main]

permissions:
  contents: read
  security-events: write
  pull-requests: write

jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable

      - uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: statico-${{ runner.os }}-${{ hashFiles('**/Cargo.lock') }}

      - run: cargo build --release

      - name: Run statico (SARIF)
        run: |
          ./target/release/statico analyze . \
            --format sarif \
            --exit-code \
            --min-confidence 0.7 \
            --output results.sarif

      - name: Upload to Code Scanning
        if: always()
        uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: results.sarif
          category: statico
```

That's it. Every PR and push to `main` will now be analyzed. Issues appear inline in the **Security → Code scanning alerts** tab.

> **Tip:** For a richer setup with PR comments and configurable inputs, use the full template in `templates/github-action.yml`.

---

## Exit Code Semantics

Statico uses exit codes to gate your CI pipeline:

| Exit Code | Meaning |
|-----------|---------|
| `0` | No issues found (or analysis completed without `--exit-code`) |
| `1` | Issues found above the configured confidence threshold |
| `2` | Invalid arguments or configuration error |
| `>2` | Internal error (panic, I/O failure, etc.) |

### Enabling exit-code gating

By default, statico exits with `0` even if issues are found (useful for report generation). Add `--exit-code` to make it return `1` when issues exceed the threshold:

```bash
# Fails (exit 1) if any issue has confidence ≥ 0.7
statico analyze . --format sarif --exit-code --min-confidence 0.7
```

In CI, this naturally fails the pipeline step. To allow the pipeline to continue (e.g., to upload artifacts), use `continue-on-error: true` in GitHub Actions or `|| true` in shell scripts.

---

## SARIF Integration with GitHub Code Scanning

Statico outputs [SARIF](https://sarifweb.azurewebsites.net/) (Static Analysis Results Interchange Format), which GitHub Code Scanning consumes natively.

### How it works

1. **Generate the SARIF file:**
   ```bash
   statico analyze . --format sarif --output results.sarif
   ```

2. **Upload with the CodeQL action:**
   ```yaml
   - uses: github/codeql-action/upload-sarif@v3
     with:
       sarif_file: results.sarif
       category: statico   # Distinguishes statico from other tools
   ```

3. **View results:** Navigate to your repository → **Security** → **Code scanning alerts**.

### Required permissions

Your workflow needs these permissions:

```yaml
permissions:
  contents: read
  security-events: write   # Upload SARIF
  actions: read             # Needed by upload-sarif action
```

### Category naming

The `category` field lets you run multiple static analysis tools without conflicts. Use a unique category per tool (e.g., `statico`, `eslint`, `codeql`).

---

## GitLab CI Setup

Use the template at `templates/gitlab-ci.yml` for a complete pipeline, or add these jobs to your existing `.gitlab-ci.yml`:

```yaml
stages:
  - build
  - analyze

# Build statico
statico:build:
  stage: build
  image: rust:latest
  script:
    - cargo build --release
  artifacts:
    paths:
      - target/release/statico
    expire_in: 1 day
  cache:
    key:
      files:
        - Cargo.lock
    paths:
      - .cargo/registry
      - .cargo/git
      - target
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH

# Run analysis
statico:analyze:
  stage: analyze
  image: node:20-bookworm
  needs:
    - job: statico:build
      artifacts: true
  script:
    - chmod +x target/release/statico
    # Generate HTML report (always)
    - target/release/statico analyze . --format html --output report.html || true
    # Gate with exit code
    - target/release/statico analyze . --format console --exit-code --min-confidence 0.7
  artifacts:
    paths:
      - report.html
    expire_in: 30 days
    when: always
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
```

### Accessing the HTML report

After the pipeline runs, download the HTML report from:

**CI/CD → Pipelines → [your pipeline] → `statico:analyze` job → Job artifacts → `report.html`**

Open it in any browser for a styled, interactive view of all findings.

---

## Custom Thresholds with `--min-confidence`

Statico assigns a confidence score (0.0 – 1.0) to each finding. The `--min-confidence` flag filters out noise:

```bash
# Only report high-confidence issues (strict)
statico analyze . --min-confidence 0.9 --exit-code

# Default: moderate threshold
statico analyze . --min-confidence 0.7 --exit-code

# Show everything including low-confidence hints (permissive)
statico analyze . --min-confidence 0.0 --exit-code
```

### Recommended thresholds

| Threshold | Use case |
|-----------|----------|
| `0.9` | Release branches, production gates — only critical issues |
| `0.7` | PR reviews — balanced signal-to-noise ratio (default) |
| `0.5` | Development — catch potential issues early |
| `0.0` | Full audit — inspect everything |

### CI strategy

Start permissive (`0.5`) during adoption, then tighten as you address findings. This avoids blocking developers while you build trust in the tool.

---

## Running in Docker

Build statico into a Docker image for consistent CI execution without compiling on every run.

### Dockerfile

```dockerfile
# ---- Build stage ----
FROM rust:1.78-bookworm AS builder

WORKDIR /usr/src/statico
COPY . .

RUN cargo build --release

# ---- Runtime stage ----
FROM node:20-bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/statico/target/release/statico /usr/local/bin/statico

ENTRYPOINT ["statico"]
CMD ["analyze", ".", "--format", "console"]
```

### Building and running

```bash
# Build the image
docker build -t statico .

# Run against a local project
docker run --rm -v "$(pwd):/project" -w /project statico \
  analyze . --format html --output /project/report.html --exit-code
```

### Using the Docker image in CI

**GitHub Actions:**

```yaml
- name: Run statico
  run: |
    docker run --rm -v "$(pwd):/project" -w /project statico:latest \
      analyze . --format sarif --exit-code --output results.sarif
```

**GitLab CI:**

```yaml
statico:analyze:
  stage: analyze
  image: your-registry/statico:latest
  script:
    - statico analyze . --format html --output report.html --exit-code
  artifacts:
    paths:
      - report.html
```

---

## Caching Strategies for Large Monorepos

Building statico from source on every CI run takes time. These strategies reduce build times significantly.

### GitHub Actions – Cargo cache

```yaml
- uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry
      ~/.cargo/git
      target
    key: statico-${{ runner.os }}-${{ hashFiles('**/Cargo.lock') }}
    restore-keys: |
      statico-${{ runner.os }}-
```

This caches the Cargo registry, git checkouts, and compiled artifacts. Cache hits on unchanged `Cargo.lock` files skip compilation entirely.

### GitLab CI – Cache per lockfile

```yaml
cache:
  key:
    files:
      - Cargo.lock
  paths:
    - .cargo/registry
    - .cargo/git
    - target
```

### Pre-built binary strategy

For the fastest CI, build statico once and distribute the binary:

1. **Create a release binary** (in a separate workflow or manually):
   ```bash
   cargo build --release
   tar czf statico-linux-x86_64.tar.gz target/release/statico
   ```

2. **Upload as a GitHub Release asset** or store in your artifact registry.

3. **Download in CI:**
   ```yaml
   - name: Install statico
     run: |
       curl -sL https://github.com/your-org/statico/releases/latest/download/statico-linux-x86_64.tar.gz | tar xz
       chmod +x statico
       sudo mv statico /usr/local/bin/
   ```

This avoids the Rust toolchain installation entirely and cuts ~2–5 minutes from each run.

---

## Monorepo Tips

Statico works well in monorepos. Here are patterns for analyzing specific subprojects and excluding others.

### Analyzing a single subproject

```bash
# Only analyze the "frontend" package
statico analyze ./apps/frontend --format sarif --exit-code
```

### Excluding subprojects

Use `--exclude` to skip directories:

```bash
# Analyze everything except legacy and vendor code
statico analyze . \
  --exclude "legacy/*" \
  --exclude "vendor/*" \
  --exclude "**/node_modules" \
  --format sarif \
  --exit-code
```

### Per-project CI matrix

In GitHub Actions, use a matrix to analyze each project independently:

```yaml
strategy:
  matrix:
    project:
      - apps/web
      - apps/mobile
      - packages/ui
steps:
  - run: |
      ./target/release/statico analyze "${{ matrix.project }}" \
        --format sarif \
        --exit-code \
        --output "results-${{ matrix.project }}.sarif"
```

### Selective analysis with path filters

Only run statico when relevant files change:

```yaml
on:
  pull_request:
    paths:
      - 'apps/web/**/*.ts'
      - 'apps/web/**/*.tsx'
      - 'packages/shared/**/*.ts'
```

### Confidence tuning per project

Different projects may have different quality thresholds:

```yaml
# Strict for production code
statico analyze ./apps/api --min-confidence 0.9 --exit-code

# Relaxed for prototypes
statico analyze ./apps/playground --min-confidence 0.5 --exit-code
```

---

## Template Reference

| Template | Path | Description |
|----------|------|-------------|
| GitHub Actions | `templates/github-action.yml` | Full reusable workflow with SARIF upload, PR comments, caching |
| GitLab CI | `templates/gitlab-ci.yml` | Two-stage pipeline with HTML artifacts and exit-code gating |

Copy these templates into your project and customize as needed.
