//! Declarative framework profiles for entry point and implicit-entry detection.
//!
//! Each framework defines:
//!   - `markers` — files whose presence identifies the framework
//!   - `entry_matchers` — rules for explicit entry points (pages, routes, configs)
//!   - `implicit_matchers` — rules for convention-consumed files (tests, migrations, scripts)
//!
//! Adding a new framework = adding one file + one line in `all_profiles()`.

use std::path::Path;

pub mod angular;
pub mod astro;
pub mod generic;
pub mod nestjs;
pub mod nextjs;
pub mod payload;
pub mod remix;
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
            Self::FileName(name) => {
                rel == *name || rel.ends_with(&format!("/{}", name))
            }
            Self::Prefix(prefix) => rel.starts_with(prefix),
            Self::FileContains(needle) => {
                let filename = rel.rsplit('/').next().unwrap_or(rel);
                filename.contains(needle)
            }
            Self::PathContains(needle) => rel.contains(needle),
            Self::DirAndStems { dir, stems } => {
                let segments: Vec<&str> = rel.split('/').collect();
                if !segments.iter().any(|s| *s == *dir) {
                    return false;
                }
                let filename = segments.last().unwrap_or(&"");
                let stem = filename.split('.').next().unwrap_or("");
                stems.iter().any(|s| *s == stem)
            }
            Self::PrefixAndFile { prefix, files } => {
                if !rel.starts_with(prefix) {
                    return false;
                }
                let filename = rel.rsplit('/').next().unwrap_or(rel);
                files.iter().any(|f| *f == filename)
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

    /// Rules for explicit entry points (imported/invoked by the framework).
    pub entry_matchers: &'static [PathMatcher],

    /// Rules for implicit entries (consumed by tooling, not imported).
    pub implicit_matchers: &'static [PathMatcher],

    /// Framework-specific gotcha rules. Empty for frameworks with no extra rules.
    pub gotcha_rules: &'static [FrameworkGotchaRule],
}

// ---------------------------------------------------------------------------
// Auto-detection
// ---------------------------------------------------------------------------

/// Detect which profiles apply to a project by checking marker files.
/// Returns all matching profiles (a project can be both Next.js + Payload).
/// Always includes the generic fallback.
pub fn detect_profiles(root: &Path) -> Vec<&'static FrameworkProfile> {
    let all = all_profiles();
    let mut matched: Vec<&'static FrameworkProfile> = Vec::new();

    for profile in all {
        if profile.markers.iter().any(|m| root.join(m).exists()) {
            matched.push(profile);
        }
    }

    // Always include generic fallback.
    if !matched.iter().any(|p| p.name == "generic") {
        matched.push(&generic::PROFILE);
    }

    matched
}

/// All known framework profiles, ordered by specificity (most specific first).
pub fn all_profiles() -> &'static [&'static FrameworkProfile] {
    &[
        &nextjs::PROFILE,
        &payload::PROFILE,
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
        let m = PathMatcher::DirAndStems {
            dir: "app",
            stems: &["page", "layout", "route"],
        };
        assert!(m.matches("src/app/page.tsx"));
        assert!(m.matches("src/app/blog/[slug]/page.tsx"));
        assert!(m.matches("app/layout.tsx"));
        assert!(m.matches("src/app/api/auth/route.ts"));
        assert!(!m.matches("src/components/page.tsx"));
        assert!(!m.matches("src/app.tsx"));
    }

    #[test]
    fn path_matcher_prefix_and_file() {
        let m = PathMatcher::PrefixAndFile {
            prefix: "src/collections/",
            files: &["index.ts", "config.ts"],
        };
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
