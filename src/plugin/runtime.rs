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
const BUN_URL_TEMPLATE: &str =
    "https://github.com/oven-sh/bun/releases/latest/download/bun-{arch}.zip";

#[cfg(target_os = "linux")]
const BUN_URL_TEMPLATE: &str =
    "https://github.com/oven-sh/bun/releases/latest/download/bun-linux-{arch}.zip";

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

/// Download and extract Bun to the managed runtime directory.
fn download_bun() -> Result<PathBuf, String> {
    let target = bun_path();
    let target_dir = target.parent().unwrap();

    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create runtime dir: {}", e))?;

    let arch = arch_suffix();
    let url = BUN_URL_TEMPLATE.replace("{arch}", arch);

    eprintln!("Downloading Bun runtime to {}...", target_dir.display());

    // Download to a temp file.
    let tmp_zip = target_dir.join("bun-download.zip");
    let status = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--progress-bar",
            &url,
            "-o",
            &tmp_zip.to_string_lossy(),
        ])
        .status()
        .map_err(|e| format!("Failed to run curl: {}", e))?;

    if !status.success() {
        let _ = std::fs::remove_file(&tmp_zip);
        return Err("Failed to download Bun. Check your internet connection.".to_string());
    }

    // Extract (unzip moves the bun binary out).
    let status = std::process::Command::new("unzip")
        .args(["-o", "-q", &tmp_zip.to_string_lossy(), "-d", &target_dir.to_string_lossy()])
        .status()
        .map_err(|e| format!("Failed to run unzip: {}", e))?;

    let _ = std::fs::remove_file(&tmp_zip);

    if !status.success() {
        return Err("Failed to extract Bun archive.".to_string());
    }

    // The extracted path may be bun/bin/bun — move it up.
    let nested = target_dir.join("bun").join("bin").join("bun");
    if nested.exists() && !target.exists() {
        let _ = std::fs::rename(&nested, &target);
    }

    // Verify.
    if !target.exists() {
        return Err(format!(
            "Bun binary not found at expected path: {}",
            target.display()
        ));
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
    if let Some(major_str) = version.split('.').next() {
        if let Ok(major) = major_str.parse::<u32>() {
            let min_major: u32 = BUN_MIN_VERSION
                .split('.')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            if major < min_major {
                return Err(format!(
                    "Bun version {} is too old (minimum: {})",
                    version, BUN_MIN_VERSION
                ));
            }
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
}
