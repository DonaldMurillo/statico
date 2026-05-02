# statico GitHub Action

Run [statico](https://github.com/DonaldMurillo/statico) inside a GitHub Actions workflow and (optionally) upload the SARIF result to GitHub Code Scanning.

## Quick start

```yaml
permissions:
  contents: read
  security-events: write

jobs:
  statico:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: DonaldMurillo/statico/.github/actions/statico@main
        with:
          format: sarif
          min-confidence: '0.5'
```

## Inputs

| Name | Default | Description |
|---|---|---|
| `version` | `latest` | statico release tag (`v0.1.0`, `latest`) |
| `path` | `.` | Project path to analyze |
| `format` | `sarif` | `json`, `sarif`, `markdown`, `html`, `ai`, `context`, `mermaid`, `pr-comment`, `fix` |
| `output-file` | `statico-results.sarif` | File to write the analysis output to |
| `min-confidence` | `0.0` | Filter issues below this confidence (0.0–1.0) |
| `exit-code` | `false` | Fail the step when any issues are reported above `min-confidence` |
| `upload-sarif` | `true` | Upload the result to Code Scanning when `format=sarif` |

## Outputs

| Name | Description |
|---|---|
| `output-file` | Path of the file the action wrote |

## Supported runners

- `ubuntu-latest` (x86_64)
- `macos-latest` (aarch64 / x86_64)

The action installs the matching prebuilt release binary, so it does not need a Rust toolchain on the runner.
