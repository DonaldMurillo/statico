//! Runtime management for plugin subprocesses.
//!
//! Handles lazy download and verification of language runtimes
//! (e.g. Bun for TypeScript plugins).

use std::path::PathBuf;

/// Directory where statico stores downloaded runtimes.
const RUNTIME_DIR: &str = ".statico/runtimes";

/// Minimum Bun version we require.
const BUN_MIN_VERSION: &str = "1.0.0";

/// Bun download URL template. Supports macOS (arm64, x64) and Linux (arm64, x64).
#[cfg(target_os = "macos")]
const BUN_URL_TEMPLATE: &str = "https://github.com/oven-sh/bun/releases/latest/download/bun-darwin-{arch}.zip";

#[cfg(target_os = "linux")]
const BUN_URL_TEMPLATE: &str = "https://github.com/oven-sh/bun/releases/latest/download/bun-linux-{arch}.zip";

#[cfg(target_os = "windows")]
const BUN_URL_TEMPLATE: &str = "https://github.com/oven-sh/bun/releases/latest/download/bun-windows-{arch}.zip";

/// Get the architecture suffix for download URLs.
fn arch_suffix() -> &'static str {
    #[cfg(target_arch = "aarch64")]
    {
        "aarch64"
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        "x64"
    }
}

/// Get the path to the managed Bun binary.
///
/// Returns `~/.statico/runtimes/bun/bun` (or `bun.exe` on Windows).
pub fn bun_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(RUNTIME_DIR).join("bun").join("bun")
}

/// Check whether the managed Bun binary exists.
pub fn bun_is_installed() -> bool {
    bun_path().exists()
}

/// Find a usable Bun binary.
///
/// Priority:
/// 1. System `bun` on `$PATH`
/// 2. Managed `~/.statico/runtimes/bun/bun`
///
/// Returns `None` if neither is found.
pub fn find_bun() -> Option<PathBuf> {
    // Check system PATH first.
    if which_exists("bun") {
        // Return the string "bun" — it's on PATH.
        Some(PathBuf::from("bun"))
    } else if bun_is_installed() {
        Some(bun_path())
    } else {
        None
    }
}

/// Auto-download Bun if TypeScript plugins are detected and Bun is not available.
///
/// Returns the path to the Bun binary (either system or managed).
/// Returns an error if Bun cannot be found or downloaded.
pub fn ensure_bun() -> Result<PathBuf, String> {
    // 1. System bun.
    if which_exists("bun") {
        return Ok(PathBuf::from("bun"));
    }

    // 2. Already downloaded.
    if bun_is_installed() {
        return Ok(bun_path());
    }

    // 3. Download.
    download_bun()
}

/// Environment variable users can set to require a specific Bun zip SHA-256.
///
/// When set, statico computes the SHA-256 of the downloaded archive and
/// refuses to extract unless it matches (case-insensitive, hex). Audit S4.1.
const TRUSTED_SHA_ENV: &str = "STATICO_TRUSTED_BUN_SHA256";

/// Download and extract Bun to the managed runtime directory.
///
/// Audit S4.1+S4.2: extraction is now pure-Rust (no `unzip` subprocess), and
/// the downloaded archive's SHA-256 is computed and logged. If
/// `STATICO_TRUSTED_BUN_SHA256` is set, the hash is verified before extraction;
/// any mismatch aborts the install.
fn download_bun() -> Result<PathBuf, String> {
    let target = bun_path();
    let target_dir = target.parent().unwrap();

    std::fs::create_dir_all(target_dir).map_err(|e| format!("Failed to create runtime dir: {}", e))?;

    let arch = arch_suffix();
    let url = BUN_URL_TEMPLATE.replace("{arch}", arch);

    eprintln!("Downloading Bun runtime to {}...", target_dir.display());

    // Download to a temp file using ureq (no shell-out, no injection risk).
    // Size-limited to 200 MB to prevent disk-fill DoS.
    const MAX_BUN_DOWNLOAD: u64 = 200 * 1024 * 1024;
    let tmp_zip = target_dir.join("bun-download.zip");
    let agent = ureq::Agent::new_with_defaults();
    let resp = agent
        .get(&url)
        .header("User-Agent", "statico-runtime-download")
        .call()
        .map_err(|e| format!("Failed to download Bun: {}", e))?;

    {
        let mut file = std::fs::File::create(&tmp_zip).map_err(|e| format!("Failed to create temp file: {}", e))?;
        let mut reader = resp.into_parts().1.into_reader();
        let mut limited = std::io::Read::take(&mut reader, MAX_BUN_DOWNLOAD);
        let bytes =
            std::io::copy(&mut limited, &mut file).map_err(|e| format!("Failed to write Bun download: {}", e))?;
        if bytes >= MAX_BUN_DOWNLOAD {
            let _ = std::fs::remove_file(&tmp_zip);
            return Err("Bun download exceeded maximum size (200 MB)".to_string());
        }
    }

    // Compute SHA-256 and (if configured) verify before extraction.
    let archive_sha = sha256_file(&tmp_zip)?;
    eprintln!("  archive SHA-256: {}", archive_sha);
    if let Ok(expected) = std::env::var(TRUSTED_SHA_ENV) {
        let expected_norm = expected.trim().to_ascii_lowercase();
        if expected_norm != archive_sha {
            let _ = std::fs::remove_file(&tmp_zip);
            return Err(format!(
                "Bun archive SHA-256 mismatch — expected {}, got {} (set via {})",
                expected_norm, archive_sha, TRUSTED_SHA_ENV
            ));
        }
        eprintln!("  ✓ archive SHA-256 matched {}", TRUSTED_SHA_ENV);
    }

    // Extract using the pure-Rust `zip` crate. Each entry path is checked for
    // traversal components — the same protection we apply to tar in update.rs.
    extract_zip(&tmp_zip, target_dir)?;
    let _ = std::fs::remove_file(&tmp_zip);

    // The extracted path may be bun/bin/bun — move it up.
    let nested = target_dir.join("bun").join("bin").join("bun");
    if nested.exists() && !target.exists() {
        let _ = std::fs::rename(&nested, &target);
    }

    // Verify the binary landed at the expected path.
    if !target.exists() {
        return Err(format!("Bun binary not found at expected path: {}", target.display()));
    }

    // Make executable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&target, perms).ok();
    }

    // Quick version check.
    let output = std::process::Command::new(&target)
        .arg("--version")
        .output()
        .map_err(|e| format!("Failed to run bun --version: {}", e))?;

    if !output.status.success() {
        return Err("Downloaded Bun binary failed version check.".to_string());
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    eprintln!("Bun {} installed successfully.", version);

    Ok(target)
}

/// Compute the SHA-256 of a file, returned as a lowercase hex string.
fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path).map_err(|e| format!("Failed to open archive for hashing: {}", e))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| format!("Failed to hash archive: {}", e))?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Extract a zip archive into `dest` using the pure-Rust `zip` crate.
///
/// Rejects any entry whose normalized path escapes `dest` (path-traversal
/// guard, parallel to `extract_tar_gz` in `src/update.rs`).
fn extract_zip(archive: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    use std::path::Component;

    let file = std::fs::File::open(archive).map_err(|e| format!("Failed to open archive: {}", e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("Failed to read zip archive: {}", e))?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("Failed to read zip entry {}: {}", i, e))?;

        // The zip crate exposes the entry's enclosed name with traversal
        // components stripped, but we belt-and-suspender: reject any entry
        // whose mangled path contains `..`, just like extract_tar_gz.
        let entry_path = match entry.enclosed_name() {
            Some(p) => p,
            None => return Err(format!("zip entry {} has unsafe path", i)),
        };
        if entry_path.components().any(|c| c == Component::ParentDir) {
            return Err(format!("refusing to extract zip entry with path traversal: {}", entry_path.display()));
        }

        let outpath = dest.join(&entry_path);
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath)
                .map_err(|e| format!("Failed to create dir {}: {}", outpath.display(), e))?;
            continue;
        }
        if let Some(parent) = outpath.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent {}: {}", parent.display(), e))?;
        }
        let mut out =
            std::fs::File::create(&outpath).map_err(|e| format!("Failed to create {}: {}", outpath.display(), e))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("Failed to extract {}: {}", outpath.display(), e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                let _ = std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode));
            }
        }
    }

    Ok(())
}

/// Check that a Bun binary meets the minimum version requirement.
pub fn check_bun_version(bun: &std::path::Path) -> Result<String, String> {
    let output = std::process::Command::new(bun)
        .arg("--version")
        .output()
        .map_err(|e| format!("Failed to run bun --version: {}", e))?;

    if !output.status.success() {
        return Err("Failed to get Bun version.".to_string());
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Simple version comparison — just check major version.
    if let Some(major_str) = version.split('.').next()
        && let Ok(major) = major_str.parse::<u32>()
    {
        let min_major: u32 = BUN_MIN_VERSION.split('.').next().and_then(|s| s.parse().ok()).unwrap_or(1);
        if major < min_major {
            return Err(format!("Bun version {} is too old (minimum: {})", version, BUN_MIN_VERSION));
        }
    }

    Ok(version)
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bun_path_is_under_home() {
        let path = bun_path();
        assert!(path.to_string_lossy().contains(".statico/runtimes/bun"));
    }

    #[test]
    fn test_find_bun_returns_system_or_managed() {
        // This test just verifies the function doesn't panic.
        let result = find_bun();
        // On CI there may be no bun, so just check it returns Some or None.
        let _ = result.is_some();
    }

    #[test]
    fn test_arch_suffix_is_valid() {
        let arch = arch_suffix();
        assert!(arch == "x64" || arch == "aarch64");
    }

    // ── S4.1 / S4.2: zip extraction & SHA-256 verification ───────────────

    fn write_test_zip(path: &std::path::Path, entries: &[(&str, &[u8])]) {
        use std::io::Write;
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts: zip::write::SimpleFileOptions =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn sec_zip_extract_writes_files() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("a.zip");
        write_test_zip(&archive, &[("hello.txt", b"world")]);

        let dest = tmp.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();
        extract_zip(&archive, &dest).unwrap();

        let extracted = std::fs::read(dest.join("hello.txt")).unwrap();
        assert_eq!(extracted, b"world");
    }

    #[test]
    fn sec_zip_extract_rejects_path_traversal() {
        // Manually craft a zip with a `..` path. The `zip` crate's
        // SimpleFileOptions accepts arbitrary names — perfect for this test.
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("evil.zip");
        write_test_zip(&archive, &[("../escape.txt", b"pwned")]);

        let dest = tmp.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let result = extract_zip(&archive, &dest);

        // Either the zip crate strips the traversal in `enclosed_name` (so
        // the file lands safely inside dest) or our explicit ParentDir check
        // rejects it. Both outcomes are acceptable; what matters is that no
        // file is written outside `dest`.
        let escape_target = tmp.path().join("escape.txt");
        assert!(
            !escape_target.exists(),
            "path-traversal entry must not be written outside dest (result was {:?})",
            result
        );
    }

    #[test]
    fn sec_sha256_file_known_value() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("data");
        std::fs::write(&path, b"hello").unwrap();
        // sha256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        let hash = sha256_file(&path).unwrap();
        assert_eq!(hash, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn sec_trusted_sha_env_constant_is_set() {
        // Document the env var name as a stable contract.
        assert_eq!(TRUSTED_SHA_ENV, "STATICO_TRUSTED_BUN_SHA256");
    }

    #[test]
    fn test_bun_url_contains_platform() {
        let arch = arch_suffix();
        let url = BUN_URL_TEMPLATE.replace("{arch}", arch);
        assert!(url.contains("darwin") || url.contains("linux"), "URL should contain platform: {}", url);
        assert!(url.contains("bun-"), "URL should contain bun- prefix: {}", url);
    }

    #[test]
    fn test_bun_url_macos_format() {
        // On macOS, the URL MUST contain "darwin" between "bun-" and "{arch}".
        // This is the exact bug that was reported: the URL was missing "darwin-".
        // On Linux this test just checks the URL contains "linux".
        let url = BUN_URL_TEMPLATE.replace("{arch}", arch_suffix());
        #[cfg(target_os = "macos")]
        {
            assert!(url.contains("bun-darwin-"), "macOS Bun URL must contain 'bun-darwin-', got: {}", url);
        }
        #[cfg(target_os = "linux")]
        {
            assert!(url.contains("bun-linux-"), "Linux Bun URL must contain 'bun-linux-', got: {}", url);
        }
    }
}
