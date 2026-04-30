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
        // Works in both flat repos (registry/) and monorepos (apps/v4/registry/).
        PathMatcher::Prefix("registry/"),
        PathMatcher::PathContains("/registry/"),
        // Style variant directories (e.g. apps/v4/styles/base-nova/ui-rtl/button.tsx)
        PathMatcher::PathContains("/styles/"),
    ],

    implicit_matchers: &[
        // Build scripts for the registry.
        PathMatcher::Prefix("scripts/"),
        PathMatcher::PathContains("/scripts/"),
        // Example/demo pages.
        PathMatcher::Prefix("examples/"),
        PathMatcher::PathContains("/examples/"),
        // Block templates — also consumed by the CLI.
        PathMatcher::PathContains("/blocks/"),
        // Generated registry output.
        PathMatcher::PathContains("/generated/"),
        // Style directories consumed by CLI.
        PathMatcher::Prefix("styles/"),
        PathMatcher::PathContains("/styles/"),
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
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("registry/new-york-v4/ui/button.tsx")));
    }

    #[test]
    fn styles_are_implicit() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("apps/v4/styles/radix-lyra/ui/button.tsx")));
    }
}
