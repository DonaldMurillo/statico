# Contributing to statico

Thanks for your interest! This is a small project, so the process is light:
file an issue or open a PR.

## Development setup

```bash
git clone https://github.com/DonaldMurillo/statico.git
cd statico
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```

`cargo clippy --all-targets -- -D warnings` is enforced — please run it before
opening a PR.

## Project layout

See `CLAUDE.md` for an architectural overview. The short version:

- `src/analyzer/` — orchestrates parsing, discovery, issue detection
- `src/issues/` — individual detectors (dead code, unused exports, gotchas, etc.)
- `src/languages/` — `LanguagePlugin` trait + per-language plugins
- `src/output/` — `OutputFormatter` trait + per-format implementations
- `src/plugin/` — JSON-RPC subprocess plugin host
- `tests/` — integration tests; `tests/fixtures/` has the example projects

## Adding a language

1. Implement `LanguagePlugin` in `src/languages/<lang>.rs`.
2. Register file extensions in `from_path()`.
3. Add a fixture project under `tests/fixtures/`.

No existing code needs to change.

## Adding an output format

1. Implement `OutputFormatter` in `src/output/<format>.rs`.
2. Wire it into the match statement in `src/commands/analyze.rs::run_analyze_inner`.
3. Add a snapshot test in `tests/output_tests.rs`.

## Adding a plugin

Plugins are out-of-tree — see `docs/plugins.md` and the `sdks/` directory.
You don't need to fork statico to ship a plugin.

## Testing

- Unit tests live next to the code they test (`#[cfg(test)] mod tests`).
- Integration tests live in `tests/`.
- Property tests use `proptest`.
- `tests/fixtures/` projects are used for end-to-end runs.

CI runs `cargo test`, `cargo clippy --all-targets -- -D warnings`, and the
self-analysis workflow (`statico analyze .` against this repo).

## Commit style

- Imperative mood, no prefix: "Add foo", "Fix bar" (the existing log mixes
  prefixed and prefix-free; prefix-free is fine for new commits).
- Reference audit IDs (e.g. `audit S4.4`) when the change closes one.
- Co-Authored-By footers are welcome but not required.

## Reporting security issues

See [SECURITY.md](SECURITY.md). **Do not open public issues for security
problems** — use GitHub's private vulnerability reporting instead.

## License

By contributing, you agree your contribution will be dual-licensed under
MIT and Apache-2.0, matching the project license.
