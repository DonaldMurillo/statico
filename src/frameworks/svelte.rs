//! Svelte/SvelteKit framework profile.
//!
//! Detects: svelte.config.js
//! Entry points: route files (+page, +layout, +error, +loading), root route configs
//! Implicit: test files, spec files, e2e

use super::{FrameworkProfile, PathMatcher};

pub const PROFILE: FrameworkProfile = FrameworkProfile {
    name: "svelte",

    markers: &["svelte.config.js"],

    dep_markers: &["@sveltejs/kit", "svelte"],

    entry_matchers: &[
        // SvelteKit route convention files.
        PathMatcher::DirAndStems {
            dir: "routes",
            stems: &["+page", "+layout", "+error", "+loading"],
        },
        // Root-level route config files.
        PathMatcher::FileName("+page.ts"),
        PathMatcher::FileName("+layout.ts"),
    ],

    implicit_matchers: &[
        // Test files.
        PathMatcher::FileContains(".test."),
        PathMatcher::FileContains(".spec."),
        // E2E tests.
        PathMatcher::Prefix("e2e/"),
    ],
    gotcha_rules: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_route_pages() {
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("src/routes/+page.svelte")));
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("src/routes/about/+page.svelte")));
    }

    #[test]
    fn entry_layout_files() {
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("src/routes/+layout.svelte")));
    }

    #[test]
    fn entry_root_configs() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("+page.ts")));
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("+layout.ts")));
    }

    #[test]
    fn implicit_test_files() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/lib/util.test.ts")));
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/lib/util.spec.ts")));
    }

    #[test]
    fn lib_file_is_not_entry() {
        assert!(!PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("src/lib/util.ts")));
    }
}
