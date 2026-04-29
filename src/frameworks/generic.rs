//! Generic TypeScript/JavaScript project profile (fallback).
//!
//! Always included. Handles: test files, scripts, tool configs, migrations, generated files.

use super::{FrameworkProfile, PathMatcher};

pub const PROFILE: FrameworkProfile = FrameworkProfile {
    name: "generic",

    // No markers — always included as fallback.
    markers: &[],

    // No framework-specific entry matchers.
    // package.json/tsconfig/default detection happens in discovery.rs.
    entry_matchers: &[],

    implicit_matchers: &[
        // Test files.
        PathMatcher::FileContains(".test."),
        PathMatcher::FileContains(".spec."),
        // E2E tests.
        PathMatcher::Prefix("e2e/"),
        PathMatcher::Prefix("tests/e2e/"),
        PathMatcher::Prefix("cypress/"),
        // Scripts.
        PathMatcher::Prefix("scripts/"),
        // Examples.
        PathMatcher::Prefix("examples/"),
        // Tool configs.
        PathMatcher::FileName("jest.config.ts"),
        PathMatcher::FileName("jest.config.js"),
        PathMatcher::FileName("vitest.config.ts"),
        PathMatcher::FileName("vitest.config.js"),
        PathMatcher::FileContains("vitest."),
        PathMatcher::FileContains(".setup."),
        PathMatcher::FileName("playwright.config.ts"),
        PathMatcher::FileName("playwright.config.js"),
        PathMatcher::FileName("postcss.config.js"),
        PathMatcher::FileName("postcss.config.ts"),
        PathMatcher::FileName("tailwind.config.ts"),
        PathMatcher::FileName("tailwind.config.js"),
        PathMatcher::FileName("webpack.config.ts"),
        PathMatcher::FileName("webpack.config.js"),
        PathMatcher::FileName("vite.config.ts"),
        PathMatcher::FileName("vite.config.js"),
        PathMatcher::FileName("eslint.config.js"),
        PathMatcher::FileName("eslint.config.ts"),
        PathMatcher::FileName("eslint.config.mjs"),
        PathMatcher::FileName("prettier.config.js"),
        PathMatcher::FileName("prettier.config.ts"),
        // Migrations.
        PathMatcher::PathContains("migrations/"),
        // Generated directories (e.g., src/generated/, __generated__/).
        PathMatcher::PathContains("/generated/"),
        // Test setup/fixtures.
        PathMatcher::Prefix("tests/"),
        PathMatcher::Prefix("__mocks__/"),
    ],
    gotcha_rules: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implicit_test_files() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/utils.test.ts")));
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("e2e/login.spec.ts")));
    }

    #[test]
    fn implicit_scripts() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("scripts/build.ts")));
    }

    #[test]
    fn implicit_migrations() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/migrations/001_init.ts")));
    }
}
