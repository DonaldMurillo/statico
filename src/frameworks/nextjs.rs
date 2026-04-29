//! Next.js framework profile.
//!
//! Detects: next.config.{ts,js,mjs}
//! Entry points: App Router pages/layouts/routes, middleware, config, Pages Router
//! Implicit: test files, e2e, scripts, examples, tool configs, generated

use super::{FrameworkGotchaPattern, FrameworkGotchaRule, FrameworkProfile, PathMatcher};

const NEXTJS_GOTCHAS: &[FrameworkGotchaRule] = &[
    // 'use client' file that only exports types/interfaces — no runtime code.
    // Next.js will still bundle it as a client module unnecessarily.
    FrameworkGotchaRule {
        rule: "nextjs-client-types-only",
        message: "`'use client'` file only exports types — consider removing the directive (server-compatible)",
        severity: "warning",
        confidence: 0.8,
        pattern: FrameworkGotchaPattern::ContainsAll(&["'use client'", "export type", "export interface"]),
    },
    // `metadata` export in a 'use client' file — Next.js silently ignores it.
    FrameworkGotchaRule {
        rule: "nextjs-metadata-in-client",
        message: "`metadata` export in a `'use client'` file is silently ignored by Next.js",
        severity: "warning",
        confidence: 0.85,
        pattern: FrameworkGotchaPattern::ContainsAll(&["'use client'", "export const metadata"]),
    },
    // `generateStaticParams` in a non-page file — only works in page/route.
    FrameworkGotchaRule {
        rule: "nextjs-static-params-location",
        message: "`generateStaticParams` only works in page.tsx or route.ts files",
        severity: "warning",
        confidence: 0.75,
        pattern: FrameworkGotchaPattern::ContainsButNot {
            needle: "generateStaticParams",
            exclusions: &["/page.tsx", "/route.ts", "page.ts", "route.ts"],
        },
    },
];

pub const PROFILE: FrameworkProfile = FrameworkProfile {
    name: "nextjs",

    markers: &["next.config.ts", "next.config.js", "next.config.mjs"],

    dep_markers: &["next"],

    entry_matchers: &[
        // Config file.
        PathMatcher::FileName("next.config.ts"),
        PathMatcher::FileName("next.config.js"),
        PathMatcher::FileName("next.config.mjs"),
        // Middleware.
        PathMatcher::FileName("middleware.ts"),
        PathMatcher::FileName("middleware.js"),
        // App Router: app/**/page, layout, route, etc.
        PathMatcher::DirAndStems {
            dir: "app",
            stems: &["page", "layout", "route", "loading", "error", "not-found", "template", "default", "global-error"],
        },
        // Pages Router.
        PathMatcher::Prefix("pages/"),
        PathMatcher::Prefix("src/pages/"),
    ],

    implicit_matchers: &[
        // Test files.
        PathMatcher::FileContains(".test."),
        PathMatcher::FileContains(".spec."),
        // E2E tests.
        PathMatcher::Prefix("e2e/"),
        PathMatcher::Prefix("tests/e2e/"),
        // Scripts and examples.
        PathMatcher::Prefix("scripts/"),
        PathMatcher::Prefix("examples/"),
        // Tool configs.
        PathMatcher::FileName("playwright.config.ts"),
        PathMatcher::FileName("playwright.config.js"),
        PathMatcher::FileName("postcss.config.js"),
        PathMatcher::FileName("postcss.config.ts"),
        PathMatcher::FileName("tailwind.config.ts"),
        PathMatcher::FileName("tailwind.config.js"),
        // Generated directories.
        PathMatcher::PathContains("/generated/"),
        PathMatcher::PathContains("payload-types"),
        // Storybook stories.
        PathMatcher::FileContains(".stories."),
        PathMatcher::FileContains(".story."),
        // Drizzle/Prisma schema files (loaded by ORM tooling, not imported).
        PathMatcher::PathContains("/drizzle/"),
        PathMatcher::FileName("schema.prisma"),
        // Storybook config.
        PathMatcher::FileName(".storybook/preview.tsx"),
        PathMatcher::FileName(".storybook/preview.ts"),
        PathMatcher::FileName(".storybook/main.ts"),
        // Instrumentation (Next.js 15+).
        PathMatcher::FileName("instrumentation.ts"),
        PathMatcher::FileName("instrumentation.js"),
    ],
    gotcha_rules: NEXTJS_GOTCHAS,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_page_matches() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/app/page.tsx")));
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/app/blog/[slug]/page.tsx")));
    }

    #[test]
    fn entry_route_matches() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/app/api/hello/route.ts")));
    }

    #[test]
    fn entry_middleware_matches() {
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("src/middleware.ts")));
        assert!(PROFILE.entry_matchers.iter().any(|m| m.matches("middleware.ts")));
    }

    #[test]
    fn implicit_test_files() {
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("src/utils.test.ts")));
        assert!(PROFILE.implicit_matchers.iter().any(|m| m.matches("e2e/login.spec.ts")));
    }

    #[test]
    fn not_entry_for_component() {
        assert!(!PROFILE.entry_matchers.iter().any(|m| m.matches("src/components/Button.tsx")));
    }
}
