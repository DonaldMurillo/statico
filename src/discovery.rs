//! File discovery and entry point detection.
//!
//! Uses declarative `FrameworkProfile` definitions to detect entry points
//! and implicit entries, rather than hardcoded framework logic.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use crate::frameworks;

const SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "rs"];

/// Discover all source files in the project, returning (relative_path, language).
/// If `exclude` is provided, files matching those glob patterns are skipped.
pub fn discover_source_files(root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut files: Vec<(String, String)> = Vec::new();

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_skipped_dir(e.path()))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let rel = crate::resolution::path_relative_to(root, path);
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => continue,
        };

        if !SOURCE_EXTENSIONS.contains(&ext) {
            continue;
        }

        if rel.ends_with(".d.ts") {
            continue;
        }

        let lang = match ext {
            "ts" => "typescript",
            "tsx" => "tsx",
            "js" => "javascript",
            "jsx" => "jsx",
            "rs" => "rust",
            _ => continue,
        };

        files.push((rel, lang.to_string()));
    }

    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// Filter source files by exclude glob patterns.
/// Returns only files that don't match any exclude pattern.
/// Supports `*` wildcard and `**` for recursive matching.
pub fn filter_excluded(
    files: Vec<(String, String)>,
    exclude: &[String],
) -> Vec<(String, String)> {
    if exclude.is_empty() {
        return files;
    }
    files.into_iter().filter(|(rel, _)| {
        for pat in exclude {
            if match_glob(pat, rel) {
                return false;
            }
        }
        true
    }).collect()
}

/// Simple glob matcher supporting `*` (any non-slash) and `**` (any including slashes).
fn match_glob(pattern: &str, path: &str) -> bool {
    let parts: Vec<&str> = pattern.split("**").collect();
    if parts.len() == 1 {
        // No ** — treat as simple glob
        return match_simple_glob(pattern, path);
    }
    // Split on ** and check each segment appears in order
    let mut idx = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(pos) = path[idx..].find(part) {
            idx += pos + part.len();
        } else if i == 0 {
            // First part must match from start
            return path.starts_with(part);
        } else {
            return false;
        }
    }
    // If pattern ends with /**, it matches everything after
    pattern.ends_with("**") || idx >= path.len()
}

/// Match a simple glob pattern (no **, just * for any non-slash chars).
fn match_simple_glob(pattern: &str, path: &str) -> bool {
    if pattern.contains('*') {
        let regex = pattern.replace('*', "*/?"); // rough approach
        // Use a simpler approach: split on * and check segments in order
        let segments: Vec<&str> = pattern.split('*').collect();
        if segments.len() == 1 {
            return path == pattern;
        }
        let mut idx = 0;
        for (i, seg) in segments.iter().enumerate() {
            if seg.is_empty() { continue; }
            if i == 0 {
                if !path.starts_with(seg) { return false; }
                idx = seg.len();
            } else if i == segments.len() - 1 {
                if !path.ends_with(seg) { return false; }
            } else {
                if let Some(pos) = path[idx..].find(seg) {
                    idx += pos + seg.len();
                } else {
                    return false;
                }
            }
        }
        return true;
    }
    path == pattern
}

/// Discover config files present in the project root.
pub fn discover_config_files(root: &Path) -> Vec<String> {
    let configs = [
        "tsconfig.json",
        "package.json",
        "jsconfig.json",
        "next.config.ts",
        "next.config.js",
        "next.config.mjs",
        "pnpm-workspace.yaml",
        "nx.json",
        "turbo.json",
        "Cargo.toml",
    ];
    let mut found: Vec<String> = Vec::new();
    for name in &configs {
        if root.join(name).exists() {
            found.push(name.to_string());
        }
    }
    found.sort();
    found
}

/// Categorized entry points: framework entries vs. implicit/tooling entries.
pub struct EntryPoints {
    /// Files whose exports are consumed by the framework (pages, routes, layouts, etc.).
    pub framework: BTreeSet<String>,
    /// Tooling/config/scripts (tests, scripts, generated files, migrations, etc.).
    /// Their exports should still be checked for usage.
    pub implicit: BTreeSet<String>,
    /// Package public API files — files referenced by package.json `main`/`module`/`exports`
    /// in ANY workspace package. Their exports form the package's public API and should
    /// NOT be flagged as unused, even if no internal file imports them.
    pub public_api: BTreeSet<String>,
}

impl EntryPoints {
    /// Union of framework, implicit, and public API entries.
    pub fn all(&self) -> BTreeSet<String> {
        self.framework
            .union(&self.implicit)
            .cloned()
            .collect::<BTreeSet<_>>()
            .union(&self.public_api)
            .cloned()
            .collect()
    }
}

/// Detect entry points using framework profiles + package.json/tsconfig + defaults.
/// Returns categorized entry points separating framework entries from implicit ones.
pub fn discover_entry_points(
    root: &Path,
    source_files: &[(String, String)],
) -> EntryPoints {
    let mut framework: BTreeSet<String> = BTreeSet::new();
    let mut implicit: BTreeSet<String> = BTreeSet::new();
    let mut public_api: BTreeSet<String> = BTreeSet::new();
    let source_set: HashSet<&str> = source_files.iter().map(|(p, _)| p.as_str()).collect();

    // 1. Detect active framework profiles.
    let profiles = frameworks::detect_profiles(root);

    // 2. Apply profile entry matchers against all source files → framework entries.
    for (rel, _) in source_files {
        for profile in &profiles {
            if profile.entry_matchers.iter().any(|m| m.matches(rel)) {
                framework.insert(rel.clone());
            }
        }
    }

    // 3. Apply profile implicit matchers against all source files → implicit entries.
    for (rel, _) in source_files {
        for profile in &profiles {
            if profile.implicit_matchers.iter().any(|m| m.matches(rel)) {
                implicit.insert(rel.clone());
            }
        }
    }

    // 4. package.json main/module/browser/exports fields → framework entries.
    add_package_json_entries(root, &source_set, &mut framework);

    // 4b. For monorepos, also discover entry points from each workspace package.
    add_workspace_entries(root, &source_set, &mut framework, &mut public_api);

    // 5. tsconfig.json files field → framework entries.
    add_tsconfig_entries(root, &mut framework);

    // 6. Default entry point locations → framework entries.
    add_default_entries(&source_set, &mut framework);

    // 6b. Rust workspace crates: find Cargo.toml files and add their src/lib.rs, src/main.rs.
    add_rust_crate_entries(root, &source_set, &mut framework);

    // 6c. Parse Cargo.toml for [lib] path and [[bin]] targets → entry points.
    add_rust_cargo_entries(root, &source_set, &mut framework);

    // 7. Tooling & config scripts → implicit entries (loaded by config, not ES imports).
    add_tooling_entries(source_files, &mut implicit);

    // 7b. Rust implicit entries: build.rs, tests/, benches/, examples/, fuzz/.
    // Also src/bin/ files as FRAMEWORK entries (they're runtime binaries).
    add_rust_implicit_entries(source_files, &mut implicit, &mut framework);

    EntryPoints { framework, implicit, public_api }
}

// ---------------------------------------------------------------------------
// package.json / tsconfig / default entries (not framework-specific)
// ---------------------------------------------------------------------------

fn add_package_json_entries(
    root: &Path,
    source_set: &HashSet<&str>,
    entry_points: &mut BTreeSet<String>,
) {
    let content = match std::fs::read_to_string(root.join("package.json")) {
        Ok(c) => c,
        Err(_) => return,
    };
    let pkg: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    for field in &["main", "module", "browser"] {
        if let Some(val) = pkg.get(field).and_then(|v| v.as_str()) {
            let rel = normalize_entry(val);
            if source_set.contains(rel.as_str()) {
                entry_points.insert(rel);
            } else {
                let ts_rel = rel.replace(".js", ".ts").replace(".jsx", ".tsx");
                if source_set.contains(ts_rel.as_str()) {
                    entry_points.insert(ts_rel);
                } else {
                    entry_points.insert(rel);
                }
            }
        }
    }

    if let Some(exports) = pkg.get("exports") {
        extract_exports_paths(exports, entry_points);
    }
}

fn add_tsconfig_entries(root: &Path, entry_points: &mut BTreeSet<String>) {
    let content = match std::fs::read_to_string(root.join("tsconfig.json")) {
        Ok(c) => c,
        Err(_) => return,
    };
    let tsconfig: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };
    if let Some(files) = tsconfig.get("files").and_then(|v| v.as_array()) {
        for f in files {
            if let Some(s) = f.as_str() {
                entry_points.insert(s.to_string());
            }
        }
    }
}

/// For monorepos, discover entry points from each workspace package.
/// Looks for package.json main/module/exports fields and default entry files
/// within each workspace package directory.
fn add_workspace_entries(
    root: &Path,
    source_set: &HashSet<&str>,
    entry_points: &mut BTreeSet<String>,
    public_api: &mut BTreeSet<String>,
) {
    let mono = match crate::monorepo::detect_monorepo(root) {
        Some(m) => m,
        None => return, // Not a monorepo, nothing to do.
    };

    let defaults = [
        "src/index.ts",
        "src/index.tsx",
        "src/main.ts",
        "src/main.tsx",
        "index.ts",
        "main.ts",
        "src/main.rs",
        "src/lib.rs",
    ];

    // Enumerate actual package directories from the workspace prefixes.
    let mut pkg_dirs: Vec<String> = Vec::new();
    for prefix in &mono.packages {
        let prefix_path = root.join(prefix);
        if !prefix_path.is_dir() {
            continue;
        }
        // If prefix is a leaf directory (not a glob pattern like packages/*),
        // check if it has a package.json — it's a package itself.
        if prefix_path.join("package.json").exists() {
            pkg_dirs.push(prefix.clone());
            continue;
        }
        // Enumerate subdirectories as potential packages.
        for entry in std::fs::read_dir(&prefix_path).ok().into_iter().flatten() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().map_or(false, |t| t.is_dir()) {
                continue;
            }
            let name = match entry.file_name().to_str() {
                Some(n) => n.to_string(),
                None => continue,
            };
            // Skip hidden and known non-package dirs.
            if name.starts_with('.') || name == "node_modules" {
                continue;
            }
            pkg_dirs.push(format!("{}/{}", prefix.trim_end_matches('/'), name));
        }
    }

    for pkg_dir in &pkg_dirs {
        // Try package.json main/module/exports fields.
        let pkg_json_path = root.join(pkg_dir).join("package.json");
        if let Ok(content) = std::fs::read_to_string(&pkg_json_path) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                // main/module → both entry_point and public_api
                for field in &["main", "module"] {
                    if let Some(val) = pkg.get(field).and_then(|v| v.as_str()) {
                        let rel = format!(
                            "{}/{}",
                            pkg_dir,
                            val.trim_start_matches("./")
                        );
                        let resolved = resolve_to_source(&rel, source_set);
                        if let Some(r) = resolved {
                            entry_points.insert(r.clone());
                            public_api.insert(r);
                        }
                    }
                }
                // exports field → public_api (these define the package's external interface)
                if let Some(exports) = pkg.get("exports") {
                    let mut export_paths = BTreeSet::new();
                    extract_exports_paths(exports, &mut export_paths);
                    for ep in &export_paths {
                        let rel = format!("{}/{}", pkg_dir, ep.trim_start_matches("./"));
                        let resolved = resolve_to_source(&rel, source_set);
                        if let Some(r) = resolved {
                            public_api.insert(r);
                        }
                    }
                }
            }
        }

        // Try default entry points within the package dir.
        for def in &defaults {
            let rel = format!("{}/{}", pkg_dir, def);
            if source_set.contains(rel.as_str()) {
                entry_points.insert(rel);
            }
        }
    }
}

fn add_default_entries(source_set: &HashSet<&str>, entry_points: &mut BTreeSet<String>) {
    let defaults = [
        "src/index.ts",
        "src/index.tsx",
        "src/index.js",
        "src/index.jsx",
        "src/main.ts",
        "src/main.tsx",
        "index.ts",
        "index.tsx",
        "index.js",
        "index.jsx",
        "main.ts",
        "src/main.rs",
        "src/lib.rs",
    ];
    for def in &defaults {
        if source_set.contains(*def) {
            entry_points.insert(def.to_string());
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn extract_exports_paths(exports: &serde_json::Value, paths: &mut BTreeSet<String>) {
    match exports {
        serde_json::Value::String(s) => {
            paths.insert(s.clone());
        }
        serde_json::Value::Object(map) => {
            for val in map.values() {
                extract_exports_paths(val, paths);
            }
        }
        _ => {}
    }
}

fn normalize_entry(path: &str) -> String {
    path.trim_start_matches("./").to_string()
}

/// Resolve a path (possibly .js/.jsx) to an actual source file in the set.
fn resolve_to_source(rel: &str, source_set: &HashSet<&str>) -> Option<String> {
    if source_set.contains(rel) {
        return Some(rel.to_string());
    }
    // Try .ts/.tsx extensions
    let ts_rel = rel.replace(".js", ".ts").replace(".jsx", ".tsx");
    if source_set.contains(ts_rel.as_str()) {
        return Some(ts_rel);
    }
    // Try appending /index.ts
    let idx = format!("{}/index.ts", rel.trim_end_matches(".ts").trim_end_matches(".js"));
    if source_set.contains(idx.as_str()) {
        return Some(idx);
    }
    None
}

/// Directories to skip during file traversal.
pub fn is_skipped_dir(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    matches!(
        name,
        "node_modules"
            | ".git"
            | "dist"
            | "build"
            | "out"
            | ".next"
            | ".nuxt"
            | "coverage"
            | ".cache"
            | "target"
            | ".turbo"
    )
}

/// Tooling/config directories whose scripts are entry points (loaded by config, not imports).
const TOOLING_DIRS: &[&str] = &[
    ".claude/hooks",
    ".claude/skills",
    "eslint-plugins",
    "eslint-rules",
    ".eslint-rules",
    "scripts",
    "tools",
    "gulpfile",
    "gruntfile",
];

/// Discover entry points from Rust workspace crates.
/// Each subdirectory with a Cargo.toml is a potential crate root.
/// Its src/lib.rs and src/main.rs are entry points.
fn add_rust_crate_entries(
    root: &Path,
    source_set: &HashSet<&str>,
    entry_points: &mut BTreeSet<String>,
) {
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
    // Walk up to 3 levels deep to avoid scanning everything
    for entry in walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            name != ".git" && name != "target" && name != "node_modules"
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() { continue; }
        let path = entry.path();
        if path.file_name() != Some(std::ffi::OsStr::new("Cargo.toml")) { continue; }

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
fn add_rust_cargo_entries(
    root: &Path,
    source_set: &HashSet<&str>,
    entry_points: &mut BTreeSet<String>,
) {
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
        if !entry.file_type().is_file() { continue; }
        let path = entry.path();
        if path.file_name() != Some(std::ffi::OsStr::new("Cargo.toml")) { continue; }

        let rel_cargo = crate::resolution::path_relative_to(root, path);
        let crate_dir = rel_cargo.rsplit_once('/').map(|(d, _)| d).unwrap_or("");

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Parse [lib] path = "..."
        if let Some(lib_path) = parse_cargo_lib_path(&content) {
            let rel = if crate_dir.is_empty() {
                lib_path.clone()
            } else {
                format!("{}/{}", crate_dir, lib_path)
            };
            if source_set.contains(rel.as_str()) {
                entry_points.insert(rel);
            }
        }

        // Parse [[bin]] targets
        for bin_path in parse_cargo_bin_paths(&content) {
            let rel = if crate_dir.is_empty() {
                bin_path.clone()
            } else {
                format!("{}/{}", crate_dir, bin_path)
            };
            if source_set.contains(rel.as_str()) {
                entry_points.insert(rel);
            }
        }

        // Also check [[test]], [[bench]], [[example]] targets
        for target_path in parse_cargo_target_paths(&content) {
            let rel = if crate_dir.is_empty() {
                target_path.clone()
            } else {
                format!("{}/{}", crate_dir, target_path)
            };
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
            if trimmed.starts_with('[') { break; }
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
                // Convention: src/bin/{name}.rs
                paths.push(format!("src/bin/{}.rs", n));
            }
            current_name = None;
            in_section = true;
            continue;
        }
        if in_section {
            if trimmed.starts_with("[[") {
                // New section — flush
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
    if !trimmed.starts_with(&prefix) { return None; }
    let rest = trimmed[prefix.len()..].trim();
    // Extract quoted string
    if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
        Some(rest[1..rest.len()-1].to_string())
    } else { None }
}

/// Rust implicit entries: build.rs, files in tests/, benches/, examples/, fuzz/, exercises/.
/// These are compiled by cargo but not imported via `mod`.
fn add_rust_implicit_entries(
    source_files: &[(String, String)],
    entry_points: &mut BTreeSet<String>,
    framework_eps: &mut BTreeSet<String>,
) {
    for (rel, lang) in source_files {
        if lang != "rust" { continue; }

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
        for dir in &[
            "tests/", "benches/", "examples/", "fuzz/", "exercises/", "solutions/",
        ] {
            if lower.contains(&format!("/{}", dir)) || lower.starts_with(dir) {
                entry_points.insert(rel.clone());
                break;
            }
        }
    }
}

fn add_tooling_entries(
    source_files: &[(String, String)],
    entry_points: &mut BTreeSet<String>,
) {
    for (rel, _) in source_files {
        let lower = rel.to_lowercase();
        for dir in TOOLING_DIRS {
            if lower.starts_with(dir) || lower.starts_with(&format!("./{dir}")) {
                entry_points.insert(rel.clone());
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_entry() {
        assert_eq!(normalize_entry("./src/index.ts"), "src/index.ts");
        assert_eq!(normalize_entry("src/index.ts"), "src/index.ts");
        assert_eq!(normalize_entry("./index.ts"), "index.ts");
    }

    #[test]
    fn test_is_skipped_dir() {
        assert!(is_skipped_dir(Path::new("/project/node_modules")));
        assert!(is_skipped_dir(Path::new("/project/.git")));
        assert!(is_skipped_dir(Path::new("/project/dist")));
        assert!(!is_skipped_dir(Path::new("/project/src")));
    }

    #[test]
    fn test_profiles_loaded_for_nextjs_fixture() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("nextjs-project");
        let profiles = frameworks::detect_profiles(&root);
        let names: Vec<&str> = profiles.iter().map(|p| p.name).collect();
        assert!(names.contains(&"nextjs"), "expected nextjs profile, got: {:?}", names);
        assert!(names.contains(&"generic"), "expected generic fallback");
    }

    #[test]
    fn test_profiles_loaded_for_payload_fixture() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("payload-project");
        let profiles = frameworks::detect_profiles(&root);
        let names: Vec<&str> = profiles.iter().map(|p| p.name).collect();
        assert!(names.contains(&"payload"), "expected payload profile, got: {:?}", names);
    }
}
