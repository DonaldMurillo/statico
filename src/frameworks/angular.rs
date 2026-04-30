//! Angular framework profile.
//!
//! Detects: angular.json
//! Entry points: bootstrap files, Angular modules, environment configs, polyfills
//! Implicit: test files, Storybook, e2e, generated code, tool configs

use super::{FrameworkProfile, PathMatcher};


pub const PROFILE: FrameworkProfile = FrameworkProfile {
    name: "angular",

    markers: &["angular.json"],

    dep_markers: &["@angular/core"],

    entry_matchers: &[
        // Bootstrap files.
        PathMatcher::FileName("main.ts"),
        PathMatcher::FileName("main.server.ts"),
        // Environment configs.
        PathMatcher::Prefix("environments/"),
        // Angular modules — loaded by the framework reflectively.
        PathMatcher::FileContains(".module."),
        // Standalone routing.
        PathMatcher::FileName("routes.ts"),
        // Polyfills.
        PathMatcher::FileName("polyfills.ts"),
    ],

    implicit_matchers: &[
        // Test files.
        PathMatcher::FileContains(".spec."),
        // Storybook stories.
        PathMatcher::FileContains(".stories."),
        // Test setup.
        PathMatcher::FileName("test.ts"),
        // Tool configs.
        PathMatcher::FileName("karma.conf.js"),
        PathMatcher::FileName("proxy.conf.json"),
        // Generated code.
        PathMatcher::PathContains("/generated/"),
        // E2E tests.
        PathMatcher::Prefix("e2e/"),
    ],
    gotcha_rules: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_bootstrap_files() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/main.ts")));
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/main.server.ts")));
    }

    #[test]
    fn entry_module_files() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/app/app.module.ts")));
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/app/app-routing.module.ts")));
    }

    #[test]
    fn entry_environment_and_polyfills() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("environments/environment.ts")));
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("environments/environment.prod.ts")));
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/polyfills.ts")));
    }

    #[test]
    fn implicit_test_and_stories() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/app/app.component.spec.ts")));
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/app/button.component.stories.ts")));
    }

    #[test]
    fn implicit_generated_and_e2e() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/generated/graphql.ts")));
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("e2e/app.e2e-spec.ts")));
    }

    #[test]
    fn component_is_not_entry() {
        // Components are loaded reflectively via decorators, not as entry points.
        assert!(!PROFILE.entry_matchers.iter().any(|m| m.matches("src/app/header.component.ts")));
    }

    #[test]
    fn service_is_not_entry() {
        // Services are provided via DI, not entry points.
        assert!(!PROFILE.entry_matchers.iter().any(|m| m.matches("src/app/data.service.ts")));
    }
}
