//! Astro framework profile.
//!
//! Detects: astro.config.{mjs,ts}
//! Entry points: src/pages/*, src/content/*
//! Implicit: test files, spec files, e2e, generated code

use super::{FrameworkProfile, PathMatcher};

pub const PROFILE: FrameworkProfile = FrameworkProfile {
    name: "astro",

    markers: &["astro.config.mjs", "astro.config.ts"],

    dep_markers: &["astro"],

    entry_matchers: &[
        // File-system routing — pages are entry points.
        PathMatcher::Prefix("src/pages/"),
        // Content collections are loaded by Astro's content layer.
        PathMatcher::Prefix("src/content/"),
    ],

    implicit_matchers: &[
        // Test files.
        PathMatcher::FileContains(".test."),
        PathMatcher::FileContains(".spec."),
        // E2E tests.
        PathMatcher::Prefix("e2e/"),
        // Generated code.
        PathMatcher::PathContains("/generated/"),
    ],
    gotcha_rules: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_pages() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/pages/index.astro")));
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/pages/about.astro")));
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/pages/blog/[...slug].astro")));
    }

    #[test]
    fn entry_content() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/content/config.ts")));
    }

    #[test]
    fn implicit_test_files() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/components/Header.test.ts")));
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("e2e/home.cy.ts")));
    }

    #[test]
    fn implicit_generated() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/generated/graphql.ts")));
    }

    #[test]
    fn component_is_not_entry() {
        assert!(!PROFILE.entry_matchers.iter().any(|m| m.matches("src/components/Header.ts")));
    }
}
