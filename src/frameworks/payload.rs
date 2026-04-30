//! Payload CMS framework profile.
//!
//! Detects: payload.config.{ts,js}
//! Entry points: payload.config
//! Implicit: collections, globals, blocks (index/config), endpoints, migrations, seed, generated

use super::{FrameworkProfile, PathMatcher};

pub const PROFILE: FrameworkProfile = FrameworkProfile {
    name: "payload",

    markers: &["payload.config.ts", "payload.config.js", "src/payload.config.ts", "src/payload.config.js"],

    dep_markers: &["payload"],

    entry_matchers: &[PathMatcher::FileName("payload.config.ts"), PathMatcher::FileName("payload.config.js")],

    implicit_matchers: &[
        // Collections: index.ts or config.ts in any collection dir.
        PathMatcher::PrefixAndFile { prefix: "src/collections/", files: &["index.ts", "config.ts"] },
        // Globals.
        PathMatcher::PrefixAndFile { prefix: "src/globals/", files: &["index.ts", "config.ts"] },
        // Blocks.
        PathMatcher::PrefixAndFile { prefix: "src/blocks/", files: &["index.ts", "config.ts"] },
        // Endpoints.
        PathMatcher::Prefix("src/endpoints/"),
        PathMatcher::Prefix("endpoints/"),
        // Migrations.
        PathMatcher::PathContains("migrations/"),
        // Seed scripts.
        PathMatcher::PathContains("seed"),
        // Generated files.
        PathMatcher::PathContains("payload-generated"),
        PathMatcher::PathContains("payload-types"),
    ],
    gotcha_rules: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_config() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("payload.config.ts")));
    }

    #[test]
    fn implicit_collections() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/collections/Users/index.ts")));
        assert!(!PROFILE.implicit_matchers.iter().any(|m| m.matches("src/collections/Users/fields.ts")));
    }

    #[test]
    fn implicit_endpoints() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/endpoints/seed.ts")));
    }

    #[test]
    fn implicit_generated() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/payload-generated-schema.ts")));
    }
}
