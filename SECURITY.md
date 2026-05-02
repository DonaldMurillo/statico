# Security policy

## Reporting a vulnerability

If you find a security issue in statico, **please do not open a public GitHub
issue.** Instead:

1. Go to https://github.com/DonaldMurillo/statico/security
2. Click **Report a vulnerability**
3. Describe the issue, reproduction steps, and impact

GitHub's private vulnerability reporting will create a draft advisory visible
only to maintainers. We will acknowledge within a few days, work on a fix, and
coordinate disclosure with you.

If GitHub's private reporting is unavailable for some reason, email
`donald.murillo07@gmail.com` with the subject `[statico security]`.

## Scope

In scope:

- The `statico` CLI binary
- The `@statico/cli` npm wrapper
- The `.github/actions/statico/` GitHub Action
- The plugin protocol (`src/plugin/protocol.rs`) and runtime
- The self-update mechanism (`src/update.rs`)
- The Bun runtime download (`src/plugin/runtime.rs`)

Out of scope:

- Vulnerabilities in third-party plugins (please report to the plugin author)
- Vulnerabilities in user code that statico happens to *parse* (not our bug)
- Findings that require an attacker who already has write access to
  `.statico.toml` or `.statico/plugins/` — that boundary is documented as the
  trust root in `SECURITY_AUDIT_REPORT.md`

## Hardening history

The repository has gone through eight rounds of security review. The full
report is in `SECURITY_AUDIT_REPORT.md` and a 2026-05 follow-up audit lives
in `docs/audit-2026-05.md`. Both are public — they describe issues that have
already been fixed.

## Self-update integrity

`statico update` downloads release tarballs over HTTPS from GitHub, validates
each tar entry against path traversal, and refuses symlinks named `statico`
inside the archive. The download is size-capped (100 MB) and extracted into
a randomized temp directory.

A SHA-256 / signature verification step against a `SHASUMS256.txt` asset is
on the roadmap — until it lands, the integrity guarantee comes from GitHub's
TLS, the size cap, and the path-traversal guards.

## Plugin trust model

Plugins listed in `.statico.toml` or dropped into `.statico/plugins/` are
spawned as subprocesses with the user's privileges. **Treat the project root
as the trust boundary** — anyone who can write `.statico.toml` or a file in
`.statico/plugins/` can run code as you.

CI tip: if you do not use plugins, set
`plugin_auto_discover = false` in `.statico.toml` to disable directory scanning.
