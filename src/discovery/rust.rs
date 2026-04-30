//! Rust-specific entry point discovery from Cargo.toml files.

use std::collections::BTreeSet;
use std::collections::HashSet;
use std::path::Path;

/// Discover entry points from Rust workspace crates.
/// Each subdirectory with a Cargo.toml is a potential crate root.
/// Its src/lib.rs and src/main.rs are entry points.
pub fn add_rust_crate_entries(root: &Path, source_set: &HashSet<&str>, entry_points: &mut BTreeSet<String>) {
    // Only run if there's a root Cargo.toml (Rust project)
    if !root.join("Cargo.toml").exists() {
        return;
    }

    let rust_entries = ["src/lib.rs", "src/main.rs"];

    // Check root crate
    for entry in &rust_entries {
        if source_set.contains(*entry) {
            entry_points.insert(entry.to_string());
        }
    }

    // Find nested Cargo.toml files (workspace members)
    for entry in walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            name != ".git" && name != "target" && name != "node_modules"
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.file_name() != Some(std::ffi::OsStr::new("Cargo.toml")) {
            continue;
        }

        let rel_cargo = crate::resolution::path_relative_to(root, path);
        let crate_dir = rel_cargo.rsplit_once('/').map(|(d, _)| d).unwrap_or("");

        for entry in &rust_entries {
            let rel = format!("{}/{}", crate_dir, entry);
            if source_set.contains(rel.as_str()) {
                entry_points.insert(rel);
            }
        }
    }
}

/// Parse Cargo.toml for `[lib] path =` and `[[bin]]` targets → entry points.
pub fn add_rust_cargo_entries(root: &Path, source_set: &HashSet<&str>, entry_points: &mut BTreeSet<String>) {
    // Only run if there's a root Cargo.toml
    if !root.join("Cargo.toml").exists() {
        return;
    }

    // Walk all Cargo.toml files (workspace members)
    for entry in walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            name != ".git" && name != "target" && name != "node_modules"
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.file_name() != Some(std::ffi::OsStr::new("Cargo.toml")) {
            continue;
        }

        let rel_cargo = crate::resolution::path_relative_to(root, path);
        let crate_dir = rel_cargo.rsplit_once('/').map(|(d, _)| d).unwrap_or("");

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Parse [lib] path = "..."
        if let Some(lib_path) = parse_cargo_lib_path(&content) {
            let rel = if crate_dir.is_empty() { lib_path.clone() } else { format!("{}/{}", crate_dir, lib_path) };
            if source_set.contains(rel.as_str()) {
                entry_points.insert(rel);
            }
        }

        // Parse [[bin]] targets
        for bin_path in parse_cargo_bin_paths(&content) {
            let rel = if crate_dir.is_empty() { bin_path.clone() } else { format!("{}/{}", crate_dir, bin_path) };
            if source_set.contains(rel.as_str()) {
                entry_points.insert(rel);
            }
        }

        // Also check [[test]], [[bench]], [[example]] targets
        for target_path in parse_cargo_target_paths(&content) {
            let rel = if crate_dir.is_empty() { target_path.clone() } else { format!("{}/{}", crate_dir, target_path) };
            if source_set.contains(rel.as_str()) {
                entry_points.insert(rel);
            }
        }
    }
}

/// Extract `[lib] path = "..."` from Cargo.toml content.
fn parse_cargo_lib_path(content: &str) -> Option<String> {
    let mut in_lib = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[lib]" {
            in_lib = true;
            continue;
        }
        if in_lib {
            if trimmed.starts_with('[') {
                break;
            }
            if let Some(path) = parse_toml_string_value(trimmed, "path") {
                return Some(path);
            }
        }
    }
    None
}

/// Extract `[[bin]]` paths from Cargo.toml content.
/// If `path` is not specified, uses convention: `src/bin/{name}.rs`.
fn parse_cargo_bin_paths(content: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut in_section = false;
    let mut current_path: Option<String> = None;
    let mut current_name: Option<String> = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[[bin]]" {
            // Flush previous section
            if let Some(p) = current_path.take() {
                paths.push(p);
            } else if let Some(n) = current_name.take() {
                paths.push(format!("src/bin/{}.rs", n));
            }
            current_name = None;
            in_section = true;
            continue;
        }
        if in_section {
            if trimmed.starts_with("[[") {
                if let Some(p) = current_path.take() {
                    paths.push(p);
                } else if let Some(n) = current_name.take() {
                    paths.push(format!("src/bin/{}.rs", n));
                }
                in_section = false;
                continue;
            }
            if trimmed.starts_with('[') {
                if let Some(p) = current_path.take() {
                    paths.push(p);
                } else if let Some(n) = current_name.take() {
                    paths.push(format!("src/bin/{}.rs", n));
                }
                in_section = false;
                continue;
            }
            if let Some(p) = parse_toml_string_value(trimmed, "path") {
                current_path = Some(p);
            }
            if let Some(n) = parse_toml_string_value(trimmed, "name") {
                current_name = Some(n);
            }
        }
    }
    // Flush last section
    if let Some(p) = current_path {
        paths.push(p);
    } else if let Some(n) = current_name {
        paths.push(format!("src/bin/{}.rs", n));
    }
    paths
}

/// Extract paths from [[test]], [[bench]], [[example]] sections.
fn parse_cargo_target_paths(content: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for section in &["[[test]]", "[[bench]]", "[[example]]"] {
        paths.extend(parse_cargo_array_paths(content, section));
    }
    paths
}

/// Extract `path = "..."` values from TOML array-of-tables sections.
fn parse_cargo_array_paths(content: &str, section: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut in_section = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == section {
            in_section = true;
            continue;
        }
        if in_section {
            if trimmed.starts_with("[[") || (trimmed.starts_with('[') && !trimmed.starts_with("[")) {
                in_section = false;
                continue;
            }
            if trimmed.starts_with('[') && trimmed != section {
                in_section = false;
                continue;
            }
            if let Some(path) = parse_toml_string_value(trimmed, "path") {
                paths.push(path);
            }
        }
    }
    paths
}

/// Parse `key = "value"` from a TOML line.
fn parse_toml_string_value(line: &str, key: &str) -> Option<String> {
    let prefix = format!("{} = ", key);
    let trimmed = line.trim();
    if !trimmed.starts_with(&prefix) {
        return None;
    }
    let rest = trimmed[prefix.len()..].trim();
    // Extract quoted string
    if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
        Some(rest[1..rest.len() - 1].to_string())
    } else {
        None
    }
}

/// Rust implicit entries: build.rs, files in tests/, benches/, examples/, fuzz/, exercises/.
/// These are compiled by cargo but not imported via `mod`.
pub fn add_rust_implicit_entries(
    source_files: &[(String, String)],
    entry_points: &mut BTreeSet<String>,
    framework_eps: &mut BTreeSet<String>,
) {
    for (rel, lang) in source_files {
        if lang != "rust" {
            continue;
        }

        // build.rs in any crate root
        if rel.ends_with("/build.rs") || rel == "build.rs" {
            entry_points.insert(rel.clone());
            continue;
        }

        let lower = rel.to_lowercase();

        // src/bin/ files are runtime binaries → framework entries
        if lower.contains("/src/bin/") || lower.starts_with("src/bin/") {
            framework_eps.insert(rel.clone());
            continue;
        }

        // Standard Rust target directories → implicit entries
        for dir in &["tests/", "benches/", "examples/", "fuzz/", "exercises/", "solutions/"] {
            if lower.contains(&format!("/{}", dir)) || lower.starts_with(dir) {
                entry_points.insert(rel.clone());
                break;
            }
        }
    }
}
