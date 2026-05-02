# @statico/cli

[statico](https://github.com/domvess/statico) — a static code analyzer for TypeScript and Rust — distributed as an npm package.

```bash
npm install -D @statico/cli
npx statico analyze .
```

The `postinstall` step downloads the matching prebuilt binary from the GitHub release, verifies its SHA-256 against the release's `SHASUMS256.txt` (when present), and drops it next to the JS shim in `node_modules/@statico/cli/bin/`.

If the install fails because of a network restriction, set `STATICO_VERSION` to pin the release tag and re-run `npm install`.

Supported targets:

| OS | Arch |
|---|---|
| macOS | aarch64, x86_64 |
| Linux | aarch64, x86_64 |

For other platforms, install via `cargo install --git https://github.com/domvess/statico` instead.
