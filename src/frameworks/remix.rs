//! Remix framework profile.
//!
//! Detects: remix.config.{js,ts}
//! Entry points: app/routes/*, root, entry.server, entry.client
//! Implicit: test files, spec files, e2e

use super::{FrameworkProfile, PathMatcher};

pub const PROFILE: FrameworkProfile = FrameworkProfile {
    name: "remix",

    markers: &["remix.config.js", "remix.config.ts"],

    dep_markers: &["@remix-run/react", "@remix-run/node"],

    entry_matchers: &[
        // Route files — the Remix file-system router.
        PathMatcher::Prefix("app/routes/"),
        // Root layout.
        PathMatcher::FileName("root.tsx"),
        PathMatcher::FileName("root.ts"),
        // Server/client entry points.
        PathMatcher::FileName("entry.server.tsx"),
        PathMatcher::FileName("entry.client.tsx"),
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
    fn entry_route_files() {
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("app/routes/_index.tsx")));
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("app/routes/about.tsx")));
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("app/routes/blog.$slug.tsx")));
    }

    #[test]
    fn entry_root_and_entries() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("app/root.tsx")));
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("app/entry.server.tsx")));
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("app/entry.client.tsx")));
    }

    #[test]
    fn implicit_test_files() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("app/routes/_index.test.tsx")));
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("e2e/home.cy.ts")));
    }

    #[test]
    fn lib_file_is_not_entry() {
        assert!(!PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("app/lib/posts.ts")));
    }

    #[test]
    fn component_is_not_entry() {
        assert!(!PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("app/components/Layout.tsx")));
    }
}
