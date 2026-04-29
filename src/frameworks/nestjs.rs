//! NestJS framework profile.
//!
//! Detects: nest-cli.json
//! Entry points: bootstrap file, NestJS modules (loaded reflectively), environment configs
//! Implicit: test files, e2e specs, test directory, tool configs, generated code

use super::{FrameworkGotchaPattern, FrameworkGotchaRule, FrameworkProfile, PathMatcher};

const NESTJS_GOTCHAS: &[FrameworkGotchaRule] = &[
    // `@Body()` without a DTO — accepts unvalidated input.
    // TypeScript erases types at runtime, so `@Body() body: any` or bare `@Body()`
    // is a security gap that NestJS validation pipes can't catch.
    FrameworkGotchaRule {
        rule: "nestjs-body-without-dto",
        message: "`@Body()` without a DTO class — request body is unvalidated at runtime (TS types are erased)",
        severity: "warning",
        confidence: 0.75,
        pattern: FrameworkGotchaPattern::ContainsButNot {
            needle: "@Body()",
            exclusions: &["CreateDto", "UpdateDto", "DTO", "Dto", ".dto"],
        },
    },
];

pub const PROFILE: FrameworkProfile = FrameworkProfile {
    name: "nestjs",

    markers: &["nest-cli.json"],

    dep_markers: &["@nestjs/core", "@nestjs/common", "@nestjs/platform-express", "@nestjs/apollo", "@nestjs/graphql"],

    entry_matchers: &[
        // Bootstrap file.
        PathMatcher::FileName("main.ts"),
        // NestJS modules — the framework loads these reflectively.
        PathMatcher::FileContains(".module."),
        // Environment configs.
        PathMatcher::Prefix("environments/"),
    ],

    implicit_matchers: &[
        // Unit tests.
        PathMatcher::FileContains(".spec."),
        // E2E tests.
        PathMatcher::FileContains(".e2e-spec."),
        // Test directory.
        PathMatcher::Prefix("test/"),
        // Tool configs.
        PathMatcher::FileName("jest-e2e.json"),
        PathMatcher::FileName("jest.config.ts"),
        PathMatcher::FileName("nest-cli.json"),
        // Generated code.
        PathMatcher::PathContains("/generated/"),
    ],
    gotcha_rules: NESTJS_GOTCHAS,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_main_bootstrap() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/main.ts")));
    }

    #[test]
    fn entry_module_files() {
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("src/app.module.ts")));
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("src/users/users.module.ts")));
    }

    #[test]
    fn entry_environments() {
        assert!(PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("environments/environment.ts")));
    }

    #[test]
    fn implicit_test_files() {
        assert!(PROFILE
            .implicit_matchers
            .iter()
            .any(|m| m.matches("src/users/users.service.spec.ts")));
        assert!(PROFILE
            .implicit_matchers
            .iter()
            .any(|m| m.matches("test/app.e2e-spec.ts")));
    }

    #[test]
    fn implicit_tool_configs() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("nest-cli.json")));
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("jest.config.ts")));
    }

    #[test]
    fn controller_is_not_entry() {
        // Controllers are registered in modules, not entry points themselves.
        assert!(!PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("src/users/users.controller.ts")));
    }

    #[test]
    fn service_is_not_entry() {
        // Services are injected via DI; orphaned ones should be dead code.
        assert!(!PROFILE
            .entry_matchers
            .iter()
            .any(|m| m.matches("src/users/users.service.ts")));
    }
}
