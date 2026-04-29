//! shadcn/ui framework profile.
//!
//! Detects: components.json (shadcn config)
//! Entry points: registry files (consumed by the CLI), style variant files
//! Implicit: build scripts, generated registry, examples

use super::{FrameworkProfile, PathMatcher};

pub const PROFILE: FrameworkProfile = FrameworkProfile {
    name: "shadcn",

    markers: &["components.json"],

    dep_markers: &["shadcn"],

    entry_matchers: &[
        // Registry component files — consumed by the shadcn CLI at build time.
        PathMatcher::Prefix("registry/"),
        // Style variant component files — also consumed by the CLI.
        PathMatcher::PathContains("/ui/"),
    ],

    implicit_matchers: &[
        // Build scripts for the registry.
        PathMatcher::Prefix("scripts/"),
        // Example/demo pages.
        PathMatcher::Prefix("examples/"),
        // Generated registry output.
        PathMatcher::PathContains("/generated/"),
        // Style directories consumed by CLI.
        PathMatcher::Prefix("styles/"),
        // Source config for content.
        PathMatcher::FileName("source.config.ts"),
    ],

    gotcha_rules: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_entry() {
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("registry/new-york-v4/ui/button.tsx")));
    }

    #[test]
    fn styles_are_implicit() {
        assert!(PROFILE
            .implicit_matchers
            .iter()
            .any(|m| m.matches("styles/radix-lyra/ui/button.tsx")));
    }
}
