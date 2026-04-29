//! Vue framework profile.
//!
//! Detects: vue.config.{ts,js}
//! Entry points: main bootstrap, router index, store index
//! Implicit: test files, spec files, e2e, stories

use super::{FrameworkProfile, PathMatcher};

pub const PROFILE: FrameworkProfile = FrameworkProfile {
    name: "vue",

    markers: &["vue.config.ts", "vue.config.js"],

    dep_markers: &["vue"],

    entry_matchers: &[
        // Bootstrap entry.
        PathMatcher::FileName("main.ts"),
        PathMatcher::FileName("main.js"),
        // Router entry.
        PathMatcher::DirAndStems {
            dir: "router",
            stems: &["index"],
        },
        // Store entry.
        PathMatcher::PrefixAndFile {
            prefix: "src/stores/",
            files: &["index.ts", "index.js"],
        },
    ],

    implicit_matchers: &[
        // Test files.
        PathMatcher::FileContains(".test."),
        PathMatcher::FileContains(".spec."),
        // E2E tests.
        PathMatcher::Prefix("e2e/"),
        // Storybook stories.
        PathMatcher::Prefix("stories/"),
    ],
    gotcha_rules: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_main_files() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/main.ts")));
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/main.js")));
    }

    #[test]
    fn entry_router_index() {
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("src/router/index.ts")));
    }

    #[test]
    fn entry_store_index() {
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("src/stores/index.ts")));
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("src/stores/index.js")));
    }

    #[test]
    fn implicit_test_files() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/utils.test.ts")));
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/utils.spec.ts")));
    }

    #[test]
    fn implicit_e2e_and_stories() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("e2e/app.cy.ts")));
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("stories/Button.stories.ts")));
    }

    #[test]
    fn component_is_not_entry() {
        assert!(!PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("src/components/Header.ts")));
    }
}
