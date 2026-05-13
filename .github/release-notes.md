## [0.1.7] - 2026-05-13

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