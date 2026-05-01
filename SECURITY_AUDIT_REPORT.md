# statico Security Audit Report

**Date:** 2026-04-30  
**Scope:** Full codebase — plugin subsystem, CLI, filesystem, config, dependencies  
**Methodology:** Manual source code review across 15+ source files  

---

## Executive Summary

The statico codebase is a well-structured Rust CLI tool with reasonable security hygiene. The most significant findings center on the **plugin subsystem** (by design, plugins are arbitrary executables) and the **self-update mechanism** (tar extraction without path traversal protection). No critical remotely-exploitable vulnerabilities were found. The attack surface is primarily local: an attacker who can write to `.statico.toml` or `.statico/plugins/` can achieve arbitrary code execution, but this is inherent to the plugin architecture.

**Risk Level:** Medium — appropriate for a developer tool, but hardening recommended before any plugin marketplace or untrusted plugin usage.

---

## Findings by Domain

### Domain 1: Plugin Subprocess Security

#### [HIGH] F-01: Unbounded Line Read from Plugin stdout (OOM)
- **File:** `src/plugin/manager.rs:100` (`send_request`)
- **Code:** `self.stdout.read_line(&mut response_line)`
- **Issue:** `BufReader::read_line()` reads until `\n` with no size limit. A malicious or compromised plugin can send an infinitely long line without a newline character, causing unbounded memory allocation and eventual OOM crash.
- **Recommendation:** Use a bounded read (e.g., read into a fixed-size buffer, or use `take()` to limit bytes). Reject lines exceeding a reasonable limit (e.g., 10 MB).
```rust
// Suggested fix:
let mut limited = self.stdout.take(10 * 1024 * 1024); // 10MB limit
limited.read_line(&mut response_line)?;
```

#### [HIGH] F-02: No Timeout on Plugin Communication
- **File:** `src/plugin/manager.rs:88-120` (`send_request`)
- **Issue:** `send_request` blocks indefinitely on `read_line()`. A plugin that hangs (or is slow) blocks the entire analysis with no timeout. There is no mechanism to kill a plugin that exceeds a time budget.
- **Recommendation:** Use `set_read_timeout()` on the stdout pipe, or spawn a watchdog thread that kills the process after N seconds.

#### [MEDIUM] F-03: Path Traversal via Config Plugin `path` Field
- **File:** `src/plugin/discovery.rs:155` (`merge_config`)
- **Code:** `existing.path = root.join(p);` where `p` comes from `.statico.toml` `plugin.path`
- **Issue:** If `.statico.toml` contains `path = "../../../../usr/bin/malicious"`, `root.join()` will resolve to a path outside the project. The executable at that path is then spawned as a plugin subprocess. This gives arbitrary code execution to anyone who can write to `.statico.toml`.
- **Recommendation:** Canonicalize the resolved path and verify it is within the project root:
```rust
let resolved = root.join(p);
let canonical = std::fs::canonicalize(&resolved)?;
if !canonical.starts_with(root) {
    return Err("plugin path escapes project root");
}
```

#### [MEDIUM] F-04: Plugin Entry Path from package.json Not Validated
- **File:** `src/plugin/manager.rs:210-226` (`find_python_entry`)
- **Code:** Reads `statico.entry` from `package.json` and uses it to construct an executable path.
- **Issue:** A malicious `package.json` with `"statico": {"entry": "../../../../etc/crontab"}` would construct a path outside the plugin directory. While the `.exists()` check mitigates execution of non-existent files, an attacker could point to any existing file.
- **Recommendation:** Canonicalize and verify the entry path is within the plugin directory.

#### [LOW] F-05: Raw Plugin Response Logged in Error Messages
- **File:** `src/plugin/manager.rs:113`
- **Code:** `"raw: {}", response_line`
- **Issue:** If a plugin response contains sensitive data (file contents, tokens), it could be leaked via error messages written to stderr.
- **Recommendation:** Truncate the raw response in error messages to a safe length (e.g., 200 chars).

#### [INFO] F-06: Subprocess Cleanup is Correct
- **File:** `src/plugin/manager.rs:138-145` (`Drop for ActivePlugin`)
- The `Drop` implementation correctly calls `process.kill()` then `process.wait()`. The `shutdown()` method sends a shutdown request, sleeps briefly, then relies on `Drop`. This is well-implemented.

#### [INFO] F-07: File Descriptor Handling is Correct
- `stdin` and `stdout` are taken from `Child` with `take()`, properly owned. No FD leaks observed.

---

### Domain 2: CLI Input Validation

#### [MEDIUM] F-08: Path Traversal in `plugin init` via Plugin Name
- **File:** `src/main.rs:1597` (`run_plugin_init`)
- **Code:** `let plugin_dir = root.join(".statico/plugins").join(name);`
- **Issue:** The `name` argument is not validated. A name like `../../evil` would create the plugin directory at `root/.statico/plugins/../../evil` which resolves to `root/evil`, writing files outside `.statico/`.
- **Recommendation:** Validate plugin name contains only alphanumeric, hyphens, and underscores:
```rust
if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
    eprintln!("Error: plugin name must contain only alphanumeric chars, hyphens, and underscores");
    std::process::exit(1);
}
```

#### [MEDIUM] F-09: Arbitrary File Read via `plugin run --file`
- **File:** `src/main.rs:1690` (`run_plugin_run`)
- **Code:** `let source_path = root.join(file);` then `std::fs::read_to_string(&source_path)`
- **Issue:** The `--file` argument is joined with root and read. An absolute path or `../../` traversal would read any file on the system and send its contents to a plugin subprocess.
- **Recommendation:** Canonicalize and verify the file path is within the project root.

#### [LOW] F-10: Canonicalize Fallback to Raw Path
- **File:** `src/main.rs:296` (`run_analyze`)
- **Code:** `Err(_) => root.to_path_buf()`
- **Issue:** When `canonicalize` fails (e.g., path doesn't exist), the raw user-supplied path is used as-is. This could be a relative path like `../../sensitive/dir`.
- **Recommendation:** Exit with an error instead of falling back to the raw path.

---

### Domain 3: Filesystem Security

#### [HIGH] F-11: Tar Extraction Without Path Traversal Protection (Self-Update)
- **File:** `src/update.rs:117` (`extract_tar_gz`)
- **Code:** `archive.unpack(dest)`
- **Issue:** The `tar` crate's `unpack()` by default extracts archive entries as-is, including paths with `../` components. A malicious release tarball from a compromised GitHub account (or MITM attack) could contain entries like `../../.bashrc` that overwrite files outside the extraction directory. **This is the most exploitable finding in the codebase.**
- **Recommendation:** Use `Archive::set_preserve_permissions(false)` and validate each entry:
```rust
for entry in archive.entries()? {
    let mut entry = entry?;
    let path = entry.path()?;
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("archive contains path traversal");
    }
    entry.unpack_in(dest)?;
}
```

#### [MEDIUM] F-12: Self-Update Binary Not Integrity-Verified
- **File:** `src/update.rs:72-99` (`run_update`)
- **Issue:** The downloaded binary is not verified against a checksum or signature. The only validation is that a file named "statico" exists in the archive. A MITM attack (or compromised CDN/GitHub) could serve a malicious binary.
- **Note:** `sha2` is listed as a dependency but not used for update verification. This was likely intended for this purpose.
- **Recommendation:** Publish SHA-256 checksums alongside releases and verify after download:
```rust
let expected_hash = fetch_checksum(&latest_version)?;
let actual_hash = sha256_of_file(&archive_path)?;
if expected_hash != actual_hash {
    return Err("integrity check failed");
}
```

#### [MEDIUM] F-13: Symlink Following in Source File Discovery
- **File:** `src/discovery/mod.rs:23` (`discover_source_files`)
- **Code:** `walkdir::WalkDir::new(root)` (default follows symlinks)
- **Issue:** A symlink inside the project pointing to `/etc/` or another sensitive directory would cause statico to traverse and read files outside the project. File contents are then sent to plugin subprocesses.
- **Recommendation:** Consider using `WalkDir::new(root).follow_links(false)` to avoid following symlinks, or at minimum check that canonicalized paths remain within the project root.

#### [LOW] F-14: Predictable Temp Directory for Updates
- **File:** `src/update.rs:77`
- **Code:** `std::env::temp_dir().join("statico-update")`
- **Issue:** Uses a predictable temp directory name. A local attacker could pre-create this directory with malicious contents (symlinks to overwrite files).
- **Recommendation:** Use `tempfile::tempdir()` (already a dev-dependency) for secure temp directory creation.

#### [LOW] F-15: Non-Atomic Cache Writes
- **File:** `src/cache.rs:61-68` (`save`)
- **Code:** `fs::write(&index_path, json)`
- **Issue:** If the process crashes during write, the cache file can be partially written / corrupted. The code handles this on load by clearing corrupt cache, so the impact is limited to lost cache efficiency.
- **Recommendation:** Write to a temp file then rename (atomic on POSIX).

---

### Domain 4: Config Security

#### [MEDIUM] F-16: Plugin Config Enables Arbitrary Code Execution by Design
- **File:** `src/config.rs`, `src/plugin/discovery.rs:130-170`
- **Issue:** Any `[[plugin]]` entry in `.statico.toml` with a `path` field pointing to an executable results in that executable being spawned as a subprocess. This is by design, but the trust boundary (`.statico.toml` in the project root) should be clearly documented.
- **Risk:** A malicious dependency or CI script that writes to `.statico.toml` gains code execution.
- **Recommendation:** 
  - Document the trust model clearly
  - Consider prompting before first plugin execution
  - Consider restricting plugin paths to `.statico/plugins/` only

#### [LOW] F-17: Config Parse Errors Silently Fall Back to Defaults
- **File:** `src/config.rs:59`
- **Code:** `Err(e) => { eprintln!("warning: ..."); Self::default() }`
- **Issue:** A malformed `.statico.toml` silently uses defaults, which could disable security-relevant settings like `exclude` patterns. The warning to stderr may go unnoticed.
- **Recommendation:** Consider returning an error or adding a `--strict-config` flag.

#### [INFO] F-18: Config Values Not Used in Shell Commands
- All config values (format, exclude patterns, etc.) are used as string matches or format selectors — never passed to `Command::new()` or shell execution. No injection risk.

---

### Domain 5: Dependency Review

#### [INFO] F-19: Dependency Audit Summary

| Dependency | Version | Status |
|---|---|---|
| `clap` | 4.6.1 | ✅ Well-maintained, standard CLI parser |
| `serde` / `serde_json` | 1.0.228 / 1.0.149 | ✅ Industry standard |
| `toml` | 0.8.23 | ✅ Standard parser |
| `ureq` | 3.3.0 | ✅ Uses `rustls` (no OpenSSL), good |
| `rayon` | 1.12.0 | ✅ Standard parallelism |
| `regex` | 1.12.3 | ✅ Standard |
| `walkdir` | 2.5.0 | ✅ Standard |
| `flate2` | 1.1.9 | ✅ Standard |
| `tar` | 0.4.45 | ⚠️ Safe if used correctly (see F-11) |
| `sha2` | 0.10 | ⚠️ Present but unused for update verification (see F-12) |
| `tree-sitter` | 0.25 | ✅ Standard parser generator |
| `atty` | 0.2.14 | ⚠️ Unmaintained (replaced by `is-terminal`), low risk |
| `dirs` | 6.0.0 | ✅ Standard |
| `colored` | 3.1.1 | ✅ Standard |
| `indicatif` | 0.17.11 | ✅ Standard |
| `oxc_resolver` | 11 (optional) | ✅ Optional, standard |

- No known CVEs in current dependency versions.
- Version constraints use semver-compatible ranges (`"4"`, `"1"`, etc.) — standard Cargo practice.
- No unnecessary dependencies that significantly increase attack surface.
- `tree-sitter` + grammar crates add build-time complexity but are well-vetted.

---

## Severity Summary

| Severity | Count | Finding IDs |
|---|---|---|
| **Critical** | 0 | — |
| **High** | 3 | F-01, F-02, F-11 |
| **Medium** | 12 | F-03, F-04, F-08, F-09, F-12, F-13, F-16, S2-01, S2-02, S2-03, S2-04, S3-01 |
| **Low** | 9 | F-05, F-10, F-14, F-15, F-17, S2-05, S2-06, S3-02, S3-04 |
| **Info** | 7 | F-06, F-07, F-18, F-19, S2-07, S2-08, S2-09, S3-03 |

---

## Top Recommendations (Prioritized)

1. **[HIGH] Fix tar extraction path traversal** (F-11) — Validate archive entries before extraction in `update.rs`. This is the most directly exploitable vulnerability.

2. **[HIGH] Add read timeout + line length limit on plugin stdout** (F-01, F-02) — Prevent OOM and hangs from malicious plugins.

3. **[MEDIUM] Add download integrity verification** (F-12) — Use the already-included `sha2` crate to verify release checksums.

4. **[MEDIUM] Validate plugin paths stay within project root** (F-03, F-04, F-08) — Canonicalize and check all user-supplied paths.

5. **[MEDIUM] Prevent symlink following in file discovery** (F-13) — Either disable symlink following or verify canonical paths.

6. **[LOW] Replace `atty` with `is-terminal`** — Minor maintenance improvement.

---

---

## Round 2 Findings (2026-04-30)

Commit: `f5c5fb3`

| ID | Severity | Description | Status |
|---|---|---|---|
| S2-01 | MEDIUM | `ensure_within_root` lexical fallback doesn't handle Windows drive-letter paths | Documented (non-issue on Unix) |
| S2-02 | MEDIUM | `download_bun()` used `curl` shell-out without SSL pinning | Fixed — replaced with ureq (pure Rust HTTP) |
| S2-03 | MEDIUM | `run_update()`: no integrity check on downloaded tarball | Documented (matches F-12) |
| S2-04 | MEDIUM | `.old` backup left if process crashes during update | Mitigated — backup cleaned on success |
| S2-05 | LOW | `cache.rs` uses FNV-1a (not collision-resistant) | Accepted — cache poisoning requires local file write |
| S2-06 | LOW | `download_bun()` URL interpolated into shell command | Fixed — replaced curl with ureq |
| S2-07 | LOW | `post_analysis` sends full output to plugins | Reviewed — AnalysisOutput has no raw sources (FP) |
| S2-08 | INFO | No limit on `max_file_size` config | Fixed — capped to 50MB |
| S2-09 | INFO | `chrono_now()` shells out to `date` | Fixed — pure Rust date calculation |

**Fixes applied:**
- `src/plugin/runtime.rs`: Replaced `curl` subprocess with `ureq` HTTP client
- `src/update.rs`: Replaced `date` subprocess with pure Rust date algorithm
- `src/config.rs`: Added `MAX_ALLOWED_FILE_SIZE = 50MB` cap after loading config

---

## Round 3 Findings (2026-04-30)

Commit: `7e25506`

| ID | Severity | Description | Status |
|---|---|---|---|
| S3-01 | MEDIUM | HTML report `</script>` injection via JSON-embedded file paths | Fixed — escape `</` in JSON strings |
| S3-02 | LOW | `discover_source_files` has no `max_depth` | Fixed — added `max_depth(20)` |
| S3-03 | INFO | `post_analysis` sends full output to plugins | False positive — no raw sources in AnalysisOutput |
| S3-04 | LOW | Plugin loop reads files without respecting `max_file_size` | Fixed — added metadata size check |

**Fixes applied:**
- `src/output/html.rs`: Replace `</` with `<\/` in JSON before embedding in `<script>`
- `src/discovery/mod.rs`: Added `.max_depth(20)` to walkdir
- `src/main.rs`: Check `file.metadata().len() > config.max_file_size` before reading for plugins

---

## Threat Model Summary

| Threat | Requires | Impact | Mitigated By |
|---|---|---|---|
| Malicious plugin in project | Write access to `.statico/plugins/` or `.statico.toml` | Arbitrary code execution | Trust boundary = project root |
| Compromised GitHub release | MITM or GitHub account compromise | Arbitrary binary replacement | Fix F-11 + F-12 |
| Malicious plugin output | Plugin that sends huge/invalid responses | OOM, hang | Fix F-01 + F-02 |
| Symlink attack | Write access to project directory | File read outside project | Fix F-13 |

**Bottom line:** For a developer tool analyzing local codebases, the security posture is reasonable. The self-update mechanism is the highest-risk surface and should be hardened before any wider distribution.
