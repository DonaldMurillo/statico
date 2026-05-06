//! Declarative framework profiles for entry point and implicit-entry detection.
//!
//! Each framework defines:
//!   - `markers` — files whose presence identifies the framework
//!   - `entry_matchers` — rules for explicit entry points (pages, routes, configs)
//!   - `implicit_matchers` — rules for convention-consumed files (tests, migrations, scripts)
//!
//! Adding a new framework = adding one file + one line in `all_profiles()`.

use std::collections::HashSet;
use std::path::Path;

pub mod angular;
pub mod astro;
pub mod generic;
pub mod monorepo_npm;
pub mod monorepo_nx;
pub mod monorepo_pnpm;
pub mod monorepo_turbo;
pub mod nestjs;
pub mod nextjs;
pub mod payload;
pub mod remix;
pub mod shadcn;
pub mod svelte;
pub mod vue;

// ---------------------------------------------------------------------------
// PathMatcher — a single declarative matching rule
// ---------------------------------------------------------------------------

/// A declarative rule for matching file paths.
#[derive(Debug, Clone)]
pub enum PathMatcher {
    /// Exact filename at project root: `"next.config.ts"`.
    FileName(&'static str),

    /// Path starts with prefix: `"e2e/"`, `"scripts/"`.
    Prefix(&'static str),

    /// Filename (with extension) contains substring: `".test."`, `".spec."`.
    FileContains(&'static str),

    /// Path contains substring anywhere: `"migrations/"`, `"generated"`.
    PathContains(&'static str),

    /// Directory segment exists in path AND filename stem (before extension)
    /// matches one of the given names.
    DirAndStems { dir: &'static str, stems: &'static [&'static str] },

    /// Path starts with prefix AND filename is exactly one of the given names.
    PrefixAndFile { prefix: &'static str, files: &'static [&'static str] },
}

impl PathMatcher {
    /// Check if a relative file path matches this rule.
    pub fn matches(&self, rel: &str) -> bool {
        match self {
            Self::FileName(name) => rel == *name || rel.ends_with(&format!("/{}", name)),
            Self::Prefix(prefix) => rel.starts_with(prefix),
            Self::FileContains(needle) => {
                let filename = rel.rsplit('/').next().unwrap_or(rel);
                filename.contains(needle)
            }
            Self::PathContains(needle) => rel.contains(needle),
            Self::DirAndStems { dir, stems } => {
                let segments: Vec<&str> = rel.split('/').collect();
                if !segments.contains(dir) {
                    return false;
                }
                let filename = segments.last().unwrap_or(&"");
                let stem = filename.split('.').next().unwrap_or("");
                stems.contains(&stem)
            }
            Self::PrefixAndFile { prefix, files } => {
                if !rel.starts_with(prefix) {
                    return false;
                }
                let filename = rel.rsplit('/').next().unwrap_or(rel);
                files.contains(&filename)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FrameworkProfile — declarative definition of a framework's conventions
// ---------------------------------------------------------------------------

/// A framework profile describes how to identify a framework and what
/// files it treats as entry points or convention-consumed implicit entries.
/// A single framework-specific gotcha rule.
/// Each rule describes a pattern to detect and what to report when found.
#[derive(Debug, Clone)]
pub struct FrameworkGotchaRule {
    /// Unique rule identifier, e.g. "react-conditional-hook".
    pub rule: &'static str,
    /// Human-readable message template.
    pub message: &'static str,
    /// Severity: "critical", "warning", or "info".
    pub severity: &'static str,
    /// Base confidence score (0.0–1.0).
    pub confidence: f64,
    /// What to look for in each line.
    pub pattern: FrameworkGotchaPattern,
}

/// Declarative pattern for framework-specific gotchas.
#[derive(Debug, Clone)]
pub enum FrameworkGotchaPattern {
    /// Line contains ALL of these substrings.
    ContainsAll(&'static [&'static str]),
    /// Line contains ANY of these substrings.
    ContainsAny(&'static [&'static str]),
    /// Line contains `needle` but NOT any of `exclusions`.
    ContainsButNot { needle: &'static str, exclusions: &'static [&'static str] },
    /// Line matches a regex pattern.
    Regex(&'static str),
    /// File-level: check if the file imports `imports` and uses `usage`
    /// in the same function scope (for hook rules).
    ImportAndUse { import_sub: &'static str, usage_sub: &'static str },
}

pub struct FrameworkProfile {
    /// Human-readable name.
    pub name: &'static str,

    /// Files whose presence at the project root indicates this framework.
    pub markers: &'static [&'static str],

    /// Dependency names in package.json that indicate this framework.
    /// Checked in both `dependencies` and `devDependencies`.
    pub dep_markers: &'static [&'static str],

    /// Rules for explicit entry points (imported/invoked by the framework).
    pub entry_matchers: &'static [PathMatcher],

    /// Rules for implicit entries (consumed by tooling, not imported).
    pub implicit_matchers: &'static [PathMatcher],

    /// Framework-specific gotcha rules. Empty for frameworks with no extra rules.
    pub gotcha_rules: &'static [FrameworkGotchaRule],
}

// ---------------------------------------------------------------------------
// MonorepoProfile — monorepo tool detection
// ---------------------------------------------------------------------------

/// A monorepo profile describes how to detect and parse a monorepo tool's
/// workspace configuration. Unlike FrameworkProfile, this is a trait because
/// each tool parses its config differently (YAML vs JSON vs package.json).
pub trait MonorepoProfile: Send + Sync {
    /// Human-readable name ("nx", "pnpm", "turborepo", "npm").
    fn name(&self) -> &'static str;

    /// Check if this monorepo tool is present at the given root.
    fn detect(&self, root: &Path, pkg_deps: Option<&HashSet<String>>) -> bool;

    /// Parse workspace package directory prefixes from the monorepo config.
    /// Returns root-relative directory prefixes like `["packages/", "apps/"]`.
    fn parse_workspaces(&self, root: &Path) -> Vec<String>;
}

/// All known monorepo profiles, ordered by detection priority.
/// Most specific first: pnpm > turbo > nx > npm.
pub fn all_monorepo_profiles() -> &'static [Box<dyn MonorepoProfile>] {
    // Lazily initialized to avoid heap allocation when monorepo detection isn't needed.
    use std::sync::LazyLock;
    static PROFILES: LazyLock<Vec<Box<dyn MonorepoProfile>>> = LazyLock::new(|| {
        vec![
            Box::new(monorepo_pnpm::PnpmProfile),
            Box::new(monorepo_turbo::TurboProfile),
            Box::new(monorepo_nx::NxProfile),
            Box::new(monorepo_npm::NpmProfile),
        ]
    });
    &PROFILES
}

/// Detect monorepo configuration at the project root.
/// Returns `None` if no monorepo setup is found.
/// Delegates to each MonorepoProfile in priority order.
pub fn detect_monorepo(root: &Path) -> Option<MonorepoInfo> {
    let pkg_deps = load_package_deps(root);
    for profile in all_monorepo_profiles().iter() {
        if profile.detect(root, pkg_deps.as_ref()) {
            let packages = profile.parse_workspaces(root);
            return Some(MonorepoInfo {
                kind: profile.name().to_string(),
                packages,
            });
        }
    }
    None
}

/// Monorepo information returned by detection.
#[derive(Debug, Clone)]
pub struct MonorepoInfo {
    /// The monorepo tool name ("pnpm", "nx", "turborepo", "npm/yarn").
    pub kind: String,
    /// Root-relative directory prefixes for workspace packages.
    pub packages: Vec<String>,
}

/// Check if a path is inside a known workspace package directory.
pub fn is_workspace_package_file(rel: &str, packages: &[String]) -> bool {
    for pkg in packages {
        if rel.starts_with(pkg.as_str()) {
            return true;
        }
    }
    false
}

/// Given a monorepo root, find the package.json directories that are
/// workspace members.
pub fn discover_workspace_roots(root: &Path, packages: &[String]) -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    for pkg_pattern in packages {
        // Reject patterns with path traversal.
        if pkg_pattern.split(['/', '\\']).any(|c| c == "..") {
            continue;
        }
        if pkg_pattern.starts_with('/') {
            continue;
        }
        if pkg_pattern.ends_with('/') {
            // Directory prefix like "packages/" — enumerate subdirs with package.json.
            let dir = root.join(pkg_pattern.trim_end_matches('/'));
            if dir.is_dir()
                && let Ok(entries) = std::fs::read_dir(&dir)
            {
                for entry in entries.flatten() {
                    if entry.path().is_dir() && entry.path().join("package.json").exists() {
                        roots.push(entry.path());
                    }
                }
            }
        } else if pkg_pattern.contains('*') {
            // Handle glob patterns like "packages/*".
            if let Some(parent) = pkg_pattern.trim_end_matches("/*").strip_suffix('*') {
                let parent = parent.trim_end_matches('/');
                let parent_dir = root.join(parent);
                if parent_dir.is_dir()
                    && let Ok(entries) = std::fs::read_dir(&parent_dir)
                {
                    for entry in entries.flatten() {
                        if entry.path().is_dir() && entry.path().join("package.json").exists() {
                            roots.push(entry.path());
                        }
                    }
                }
            }
        } else {
            let pkg_dir = root.join(pkg_pattern);
            if pkg_dir.join("package.json").exists() {
                roots.push(pkg_dir);
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

/// Convert glob patterns like "packages/*" to directory prefixes like "packages/".
/// Handles double-star patterns like "packages/**" and "packages/**/".
pub fn glob_to_prefix(patterns: Vec<String>) -> Vec<String> {
    patterns
        .into_iter()
        .map(|p| {
            if p.ends_with("/*") {
                format!("{}/", &p[..p.len() - 2])
            } else if p.ends_with("/**/") {
                format!("{}/", &p[..p.len() - 4])
            } else if p.ends_with("/**") {
                format!("{}/", &p[..p.len() - 3])
            } else {
                p
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Auto-detection
// ---------------------------------------------------------------------------

/// Detect which profiles apply to a project by checking marker files.
/// Returns all matching profiles (a project can be both Next.js + Payload).
/// Always includes the generic fallback.
///
/// For monorepos, also searches workspace sub-directories for markers,
/// since frameworks like Angular/NestJS/Vue may have their config in
/// a sub-package rather than the monorepo root.
pub fn detect_profiles(root: &Path) -> Vec<&'static FrameworkProfile> {
    let all = all_profiles();
    let mut matched: Vec<&'static FrameworkProfile> = Vec::new();

    // 0. Load package.json deps once (if present).
    let pkg_deps = load_package_deps(root);

    // 1. Check markers at the project root AND dependency markers.
    for profile in all {
        // File-based markers.
        if profile.markers.iter().any(|m| root.join(m).exists()) {
            matched.push(profile);
            continue;
        }
        // Dependency-based markers.
        if !profile.dep_markers.is_empty()
            && pkg_deps.as_ref().is_some_and(|deps| profile.dep_markers.iter().any(|dm| deps.contains(*dm)))
        {
            matched.push(profile);
        }
    }

    // 2. If this is a monorepo, also scan workspace package dirs for markers.
    if matched.len() <= 1 {
        // <=1 means only generic matched
        if let Some(mono) = detect_monorepo(root) {
            let ws_roots = discover_workspace_roots(root, &mono.packages);
            for ws_root in &ws_roots {
                let ws_deps = load_package_deps(ws_root);
                for profile in all {
                    if profile.name == "generic" {
                        continue;
                    }
                    if profile.markers.iter().any(|m| ws_root.join(m).exists())
                        && !matched.iter().any(|p| p.name == profile.name)
                    {
                        matched.push(profile);
                    }
                    // Also check deps in workspace package.json.
                    if !profile.dep_markers.is_empty()
                        && ws_deps.as_ref().is_some_and(|deps| profile.dep_markers.iter().any(|dm| deps.contains(*dm)))
                        && !matched.iter().any(|p| p.name == profile.name)
                    {
                        matched.push(profile);
                    }
                }
            }
        }

        // 2b. Also try a shallow scan: walk first-level subdirectories.
        if matched.len() <= 1
            && let Ok(entries) = std::fs::read_dir(root)
        {
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.')
                    || name_str == "node_modules"
                    || name_str == "dist"
                    || name_str == "build"
                    || name_str == "target"
                    || name_str == "benchmarks"
                    || name_str == "fixtures"
                    || name_str == "tools"
                {
                    continue;
                }
                let sub = entry.path();
                let sub_deps = load_package_deps(&sub);
                for profile in all {
                    if profile.name == "generic" {
                        continue;
                    }
                    if profile.markers.iter().any(|m| sub.join(m).exists())
                        && !matched.iter().any(|p| p.name == profile.name)
                    {
                        matched.push(profile);
                    }
                    if !profile.dep_markers.is_empty()
                        && sub_deps.as_ref().is_some_and(|deps| profile.dep_markers.iter().any(|dm| deps.contains(*dm)))
                        && !matched.iter().any(|p| p.name == profile.name)
                    {
                        matched.push(profile);
                    }
                }
            }
        }
    }

    // Always include generic fallback.
    if !matched.iter().any(|p| p.name == "generic") {
        matched.push(&generic::PROFILE);
    }

    matched
}

/// Load dependency names from a package.json file.
/// Returns None if no package.json exists.
fn load_package_deps(dir: &Path) -> Option<HashSet<String>> {
    let pkg_path = dir.join("package.json");
    let content = std::fs::read_to_string(&pkg_path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&content).ok()?;
    let mut deps = HashSet::new();
    // Include the package's own name — enables detecting the shadcn monorepo
    // where packages/shadcn/package.json has name "shadcn" but it's not a dep.
    if let Some(name) = val.get("name").and_then(|v| v.as_str()) {
        deps.insert(name.to_string());
    }
    if let Some(obj) = val.get("dependencies").and_then(|v| v.as_object()) {
        deps.extend(obj.keys().cloned());
    }
    if let Some(obj) = val.get("devDependencies").and_then(|v| v.as_object()) {
        deps.extend(obj.keys().cloned());
    }
    Some(deps)
}

/// All known framework profiles, ordered by specificity (most specific first).
pub fn all_profiles() -> &'static [&'static FrameworkProfile] {
    &[
        &nextjs::PROFILE,
        &payload::PROFILE,
        &shadcn::PROFILE,
        &angular::PROFILE,
        &nestjs::PROFILE,
        &vue::PROFILE,
        &svelte::PROFILE,
        &remix::PROFILE,
        &astro::PROFILE,
        &generic::PROFILE,
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_matcher_file_name() {
        let m = PathMatcher::FileName("next.config.ts");
        assert!(m.matches("next.config.ts"));
        assert!(m.matches("src/next.config.ts")); // matches at any depth
        assert!(!m.matches("something.ts"));
    }

    #[test]
    fn path_matcher_prefix() {
        let m = PathMatcher::Prefix("e2e/");
        assert!(m.matches("e2e/login.spec.ts"));
        assert!(!m.matches("src/e2e/test.ts"));
    }

    #[test]
    fn path_matcher_file_contains() {
        let m = PathMatcher::FileContains(".test.");
        assert!(m.matches("src/utils.test.ts"));
        assert!(m.matches("components/Button.test.tsx"));
        assert!(!m.matches("src/testing.ts"));
    }

    #[test]
    fn path_matcher_path_contains() {
        let m = PathMatcher::PathContains("migrations/");
        assert!(m.matches("src/migrations/001_init.ts"));
        assert!(!m.matches("src/models/user.ts"));
    }

    #[test]
    fn path_matcher_dir_and_stems() {
        let m = PathMatcher::DirAndStems { dir: "app", stems: &["page", "layout", "route"] };
        assert!(m.matches("src/app/page.tsx"));
        assert!(m.matches("src/app/blog/[slug]/page.tsx"));
        assert!(m.matches("app/layout.tsx"));
        assert!(m.matches("src/app/api/auth/route.ts"));
        assert!(!m.matches("src/components/page.tsx"));
        assert!(!m.matches("src/app.tsx"));
    }

    #[test]
    fn path_matcher_prefix_and_file() {
        let m = PathMatcher::PrefixAndFile { prefix: "src/collections/", files: &["index.ts", "config.ts"] };
        assert!(m.matches("src/collections/Users/index.ts"));
        assert!(m.matches("src/collections/Posts/config.ts"));
        assert!(!m.matches("src/collections/Users/fields.ts"));
        assert!(!m.matches("src/components/index.ts"));
    }

    #[test]
    fn detect_generic_always_included() {
        use std::path::PathBuf;
        let tmp = PathBuf::from("/tmp/nonexistent-statico-test");
        let profiles = detect_profiles(&tmp);
        assert!(profiles.iter().any(|p| p.name == "generic"));
    }
}
