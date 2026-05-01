//! Self-update mechanism for statico.
//!
//! Checks GitHub releases for new versions and performs in-place binary updates.

use std::env;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// GitHub repo for releases.
const GITHUB_REPO: &str = "domvess/statico";

/// A GitHub release.
#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    #[allow(dead_code)]
    assets: Vec<Asset>,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Base URL for the update API. Defaults to GitHub; overridden in tests.
pub fn api_base_url() -> String {
    std::env::var("STATICO_UPDATE_API_URL")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{}", GITHUB_REPO))
}

/// Base URL for download. Defaults to GitHub; overridden in tests.
pub fn download_base_url() -> String {
    std::env::var("STATICO_UPDATE_DL_URL")
        .unwrap_or_else(|_| format!("https://github.com/{}", GITHUB_REPO))
}

/// Check GitHub for the latest release version.
pub fn latest_version() -> Result<String, String> {
    let url = format!("{}/releases/latest", api_base_url());
    let agent = ureq::Agent::new_with_defaults();
    let resp = agent
        .get(&url)
        .header("User-Agent", "statico-self-update")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("failed to check for updates: {}", e))?;
    let release: Release = resp.into_body().read_json().map_err(|e| format!("failed to parse release: {}", e))?;
    // Strip leading 'v' if present.
    Ok(release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name).to_string())
}

/// Compare two semver-like version strings.
/// Returns true if `current` is older than `latest`.
pub fn is_newer(current: &str, latest: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.trim()
            .split('.')
            .filter_map(|p| p.parse().ok())
            .collect::<Vec<_>>()
    };
    let cur = parse(current);
    let lat = parse(latest);
    for i in 0..lat.len().max(cur.len()) {
        let c = cur.get(i).unwrap_or(&0);
        let l = lat.get(i).unwrap_or(&0);
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }
    false
}

/// Detect the current platform triple for download.
fn platform_triple() -> Result<String, String> {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return Err("unsupported OS".into());
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        return Err("unsupported architecture".into());
    };
    Ok(format!("{}-{}", os, arch))
}

/// Path where statico stores update metadata.
pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("statico")
}

/// Perform a self-update: download latest release and replace current binary.
pub fn run_update(dry_run: bool) -> Result<String, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let latest = latest_version()?;

    if !is_newer(&current, &latest) {
        return Ok(format!("Already up to date (v{})", current));
    }

    if dry_run {
        return Ok(format!("Update available: v{} → v{}. Run `statico update` to install.", current, latest));
    }

    let platform = platform_triple()?;
    let archive_name = format!("statico-{}.tar.gz", platform);
    let url = format!(
        "{}/releases/download/v{}/{}",
        download_base_url(), latest, archive_name
    );

    eprintln!("Downloading statico v{} for {}...", latest, platform);

    // Download archive to a secure, randomized temp directory.
    let tmp_builder = tempfile::TempDir::new().map_err(|e| format!("failed to create temp dir: {}", e))?;
    let tmp_dir = tmp_builder.path();
    let archive_path = tmp_dir.join(&archive_name);

    let agent = ureq::Agent::new_with_defaults();
    let resp = agent
        .get(&url)
        .header("User-Agent", "statico-self-update")
        .call()
        .map_err(|e| format!("download failed: {}", e))?;

    {
        let mut file = fs::File::create(&archive_path).map_err(|e| format!("failed to create file: {}", e))?;
        let mut reader = resp.into_parts().1.into_reader();
        io::copy(&mut reader, &mut file)
            .map_err(|e| format!("download write failed: {}", e))?;
    }

    // Extract the binary from the tarball.
    let extract_dir = tmp_dir.join("extracted");
    fs::create_dir_all(&extract_dir).map_err(|e| format!("failed to create extract dir: {}", e))?;

    extract_tar_gz(&archive_path, &extract_dir)?;

    // Find the statico binary in the extracted files.
    let new_binary = find_binary(&extract_dir)?;

    // Replace current binary.
    let current_exe = env::current_exe().map_err(|e| format!("cannot determine current executable: {}", e))?;
    replace_binary(&current_exe, &new_binary)?;

    // Clean up (temp dir is removed on drop, but be explicit).
    drop(tmp_builder);

    // Record update metadata.
    let data = data_dir();
    let _ = fs::create_dir_all(&data);
    let _ = fs::write(data.join("last-version"), &latest);
    let _ = fs::write(data.join("last-check"), today_string());

    Ok(format!("Updated statico v{} → v{} ✓", current, latest))
}

/// Extract a .tar.gz archive safely.
///
/// Validates each entry's path to prevent path traversal attacks (e.g. `../../etc/passwd`).
fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| format!("failed to open archive: {}", e))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive.set_preserve_permissions(false);

    for entry in archive.entries().map_err(|e| format!("failed to read archive entries: {}", e))? {
        let mut entry = entry.map_err(|e| format!("failed to read archive entry: {}", e))?;
        let path = entry
            .path()
            .map_err(|e| format!("failed to read entry path: {}", e))?
            .into_owned();

        // Reject any entry whose path contains a parent directory component.
        if path.components().any(|c| c == Component::ParentDir) {
            return Err(format!(
                "refusing to extract archive entry with path traversal: {}",
                path.display()
            ));
        }

        entry.unpack_in(dest).map_err(|e| {
            format!(
                "failed to extract archive entry '{}': {}",
                path.display(),
                e
            )
        })?;
    }

    Ok(())
}

/// Find the statico binary in extracted files.
fn find_binary(dir: &Path) -> Result<PathBuf, String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("failed to read dir: {}", e))? {
        let entry = entry.map_err(|e| format!("failed to read entry: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            if let Ok(found) = find_binary(&path) {
                return Ok(found);
            }
        } else if path.file_name().is_some_and(|n| n == "statico") {
            return Ok(path);
        }
    }
    Err("statico binary not found in archive".into())
}

/// Replace the current binary with the new one.
fn replace_binary(current: &Path, new: &Path) -> Result<(), String> {
    // Make new binary executable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(new, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("failed to set permissions: {}", e))?;
    }

    // On Unix, we can't overwrite a running binary directly.
    // Instead: rename current to .old, copy new to current location, delete .old.
    let backup = current.with_extension("old");
    fs::rename(current, &backup).map_err(|e| format!("failed to backup current binary: {}", e))?;

    if let Err(e) = fs::copy(new, current) {
        // Restore backup on failure.
        let _ = fs::rename(&backup, current);
        return Err(format!("failed to install new binary: {}", e));
    }

    let _ = fs::remove_file(&backup);
    Ok(())
}

/// Get current date as ISO 8601 date string (pure Rust, no subprocess).
fn today_string() -> String {
    // Simple approach: use SystemTime and format as date.
    // We only need the date portion for rate-limiting.
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let total_days = duration.as_secs() / 86400;
    // Compute year/month/day from unix epoch days.
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = total_days as i64 + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Check if we should notify about updates (rate-limited to once per day).
pub fn should_check_update() -> bool {
    let data = data_dir();
    let last_check = data.join("last-check");
    if !last_check.exists() {
        return true;
    }
    let content = fs::read_to_string(&last_check).unwrap_or_default();
    // Very simple: if the date string differs from today, check again.
    let today = today_string();
    today != content.trim()
}

/// Run a background version check and print a notice if update available.
/// Non-blocking: if the check fails for any reason, silently skip.
pub fn check_and_notify() {
    if !should_check_update() {
        // Still check if the cached version is newer.
        let data = data_dir();
        let cached = data.join("last-version");
        if let Ok(latest) = fs::read_to_string(&cached) {
            let current = env!("CARGO_PKG_VERSION");
            if is_newer(current, &latest) {
                eprintln!(
                    "\x1b[33mstatico v{} is available (you have v{}). Run `statico update` to upgrade.\x1b[0m",
                    latest.trim(),
                    current
                );
            }
        }
        return;
    }

    // Try to fetch latest version.
    if let Ok(latest) = latest_version() {
        let current = env!("CARGO_PKG_VERSION");
        let data = data_dir();
        let _ = fs::create_dir_all(&data);
        let _ = fs::write(data.join("last-version"), &latest);
        let _ = fs::write(data.join("last-check"), today_string());

        if is_newer(current, &latest) {
            eprintln!(
                "\x1b[33mstatico v{} is available (you have v{}). Run `statico update` to upgrade.\x1b[0m",
                latest, current
            );
        }
    }
    // Silently ignore errors — this is a best-effort check.
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer() {
        assert!(is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("1.0.0", "2.0.0"));
        assert!(!is_newer("0.2.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn test_is_newer_different_lengths() {
        assert!(is_newer("0.1", "0.1.1"));
        assert!(!is_newer("0.1.0", "0.1"));
    }
}
