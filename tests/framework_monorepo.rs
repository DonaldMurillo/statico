//! Integration tests for framework detection and monorepo support.

use std::path::Path;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn analyze_fixture(name: &str) -> statico::types::AnalysisOutput {
    statico::analyzer::analyze(&fixture(name)).expect("analyze should succeed")
}

// ---------------------------------------------------------------------------
// Angular
// ---------------------------------------------------------------------------

#[test]
fn angular_framework_detected() {
    let profiles = statico::frameworks::detect_profiles(&fixture("angular-project"));
    let names: Vec<&str> = profiles.iter().map(|p| p.name).collect();
    assert!(names.contains(&"angular"), "expected angular profile, got: {:?}", names);
}

#[test]
fn angular_analyze_succeeds() {
    let output = analyze_fixture("angular-project");
    assert!(!output.structure.source_files.is_empty());
}

#[test]
fn angular_detects_dead_code() {
    let output = analyze_fixture("angular-project");
    // sidebar.component.ts and analytics.service.ts are dead code.
    let dead_paths: Vec<&str> = output.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(
        dead_paths.iter().any(|p| p.contains("sidebar")),
        "expected dead code in sidebar.component.ts, got: {:?}", dead_paths
    );
}

#[test]
fn angular_detects_orphan() {
    let output = analyze_fixture("angular-project");
    let dead_paths: Vec<&str> = output.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(
        dead_paths.iter().any(|p| p.contains("styles")),
        "expected orphan styles.ts, got: {:?}", dead_paths
    );
}

#[test]
fn angular_main_is_entry_point() {
    let output = analyze_fixture("angular-project");
    assert!(
        output.structure.entry_points.iter().any(|e| e.contains("main")),
        "main.ts should be an entry point"
    );
}

// ---------------------------------------------------------------------------
// NestJS
// ---------------------------------------------------------------------------

#[test]
fn nestjs_framework_detected() {
    let profiles = statico::frameworks::detect_profiles(&fixture("nestjs-project"));
    let names: Vec<&str> = profiles.iter().map(|p| p.name).collect();
    assert!(names.contains(&"nestjs"), "expected nestjs profile, got: {:?}", names);
}

#[test]
fn nestjs_analyze_succeeds() {
    let output = analyze_fixture("nestjs-project");
    assert!(!output.structure.source_files.is_empty());
}

#[test]
fn nestjs_detects_dead_code() {
    let output = analyze_fixture("nestjs-project");
    let dead_paths: Vec<&str> = output.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    // roles.guard.ts and deprecated.ts should be dead code.
    assert!(
        dead_paths.iter().any(|p| p.contains("deprecated") || p.contains("roles")),
        "expected dead code, got: {:?}", dead_paths
    );
}

#[test]
fn nestjs_main_is_entry_point() {
    let output = analyze_fixture("nestjs-project");
    assert!(
        output.structure.entry_points.iter().any(|e| e.contains("main")),
        "main.ts should be an entry point"
    );
}

#[test]
fn nestjs_monorepo_not_detected() {
    // NestJS fixture is NOT a monorepo.
    let info = statico::monorepo::detect_monorepo(&fixture("nestjs-project"));
    assert!(info.is_none(), "standalone NestJS should not be detected as monorepo");
}

// ---------------------------------------------------------------------------
// Monorepo detection
// ---------------------------------------------------------------------------

#[test]
fn pnpm_monorepo_detected() {
    let info = statico::monorepo::detect_monorepo(&fixture("pnpm-monorepo"))
        .expect("should detect pnpm monorepo");
    assert_eq!(info.kind, statico::monorepo::MonorepoKind::Pnpm);
    assert!(info.packages.contains(&"packages/".to_string()), "packages: {:?}", info.packages);
    assert!(info.packages.contains(&"apps/".to_string()), "packages: {:?}", info.packages);
}

#[test]
fn npm_monorepo_detected() {
    let info = statico::monorepo::detect_monorepo(&fixture("npm-monorepo"))
        .expect("should detect npm monorepo");
    assert_eq!(info.kind, statico::monorepo::MonorepoKind::Npm);
    assert!(info.packages.contains(&"packages/".to_string()), "packages: {:?}", info.packages);
}

#[test]
fn nx_monorepo_detected() {
    let info = statico::monorepo::detect_monorepo(&fixture("nx-monorepo"))
        .expect("should detect nx monorepo");
    assert_eq!(info.kind, statico::monorepo::MonorepoKind::Nx);
    assert!(info.packages.contains(&"packages/".to_string()), "packages: {:?}", info.packages);
}

#[test]
fn pnpm_monorepo_analyze_succeeds() {
    let output = analyze_fixture("pnpm-monorepo");
    assert!(output.monorepo.is_some(), "monorepo info should be populated");
    let info = output.monorepo.unwrap();
    assert_eq!(info.kind, "pnpm");
    assert!(!output.structure.source_files.is_empty());
}

#[test]
fn npm_monorepo_analyze_succeeds() {
    let output = analyze_fixture("npm-monorepo");
    assert!(output.monorepo.is_some());
    assert_eq!(output.monorepo.unwrap().kind, "npm/yarn");
}

#[test]
fn nx_monorepo_analyze_succeeds() {
    let output = analyze_fixture("nx-monorepo");
    assert!(output.monorepo.is_some());
    assert_eq!(output.monorepo.unwrap().kind, "nx");
}

#[test]
fn pnpm_monorepo_discovers_packages() {
    let root = fixture("pnpm-monorepo");
    let info = statico::monorepo::detect_monorepo(&root).unwrap();
    let roots = statico::monorepo::discover_workspace_roots(&root, &info.packages);
    assert!(roots.len() >= 2, "should find at least 2 packages (ui, shared), found {}", roots.len());
}

#[test]
fn pnpm_monorepo_detects_dead_code() {
    let output = analyze_fixture("pnpm-monorepo");
    let dead_paths: Vec<&str> = output.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(
        dead_paths.iter().any(|p| p.contains("unused") || p.contains("deprecated")),
        "expected dead code in pnpm monorepo, got: {:?}", dead_paths
    );
}

#[test]
fn npm_monorepo_detects_dead_code() {
    let output = analyze_fixture("npm-monorepo");
    let dead_paths: Vec<&str> = output.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(
        dead_paths.iter().any(|p| p.contains("orphan")),
        "expected dead code in npm monorepo, got: {:?}", dead_paths
    );
}

#[test]
fn nx_monorepo_detects_dead_code() {
    let output = analyze_fixture("nx-monorepo");
    let dead_paths: Vec<&str> = output.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(
        dead_paths.iter().any(|p| p.contains("dead") || p.contains("Dead")),
        "expected dead code in nx monorepo, got: {:?}", dead_paths
    );
}

// ---------------------------------------------------------------------------
// Framework detection shows up in analysis output
// ---------------------------------------------------------------------------

#[test]
fn analysis_includes_detected_frameworks() {
    let output = analyze_fixture("nextjs-project");
    let frameworks = output.detected_frameworks.expect("should have frameworks");
    assert!(frameworks.contains(&"nextjs".to_string()), "frameworks: {:?}", frameworks);
}

#[test]
fn angular_includes_detected_frameworks() {
    let output = analyze_fixture("angular-project");
    let frameworks = output.detected_frameworks.expect("should have frameworks");
    assert!(frameworks.contains(&"angular".to_string()), "frameworks: {:?}", frameworks);
}

// ---------------------------------------------------------------------------
// Turborepo
// ---------------------------------------------------------------------------

#[test]
fn turborepo_monorepo_detected() {
    let info = statico::monorepo::detect_monorepo(&fixture("turborepo-monorepo"))
        .expect("should detect turborepo");
    assert_eq!(info.kind, statico::monorepo::MonorepoKind::Turborepo);
}

#[test]
fn turborepo_analyze_succeeds() {
    let output = analyze_fixture("turborepo-monorepo");
    assert!(output.monorepo.is_some());
    assert_eq!(output.monorepo.unwrap().kind, "turborepo");
}

#[test]
fn turborepo_detects_dead_code() {
    let output = analyze_fixture("turborepo-monorepo");
    let dead_paths: Vec<&str> = output.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(
        dead_paths.iter().any(|p| p.contains("unused") || p.contains("Unused")),
        "expected dead code in turborepo, got: {:?}", dead_paths
    );
}

#[test]
fn turborepo_discovers_packages() {
    let root = fixture("turborepo-monorepo");
    let info = statico::monorepo::detect_monorepo(&root).unwrap();
    let roots = statico::monorepo::discover_workspace_roots(&root, &info.packages);
    assert!(roots.len() >= 2, "should find at least 2 packages, found {}", roots.len());
}

// ---------------------------------------------------------------------------
// Vue
// ---------------------------------------------------------------------------

#[test]
fn vue_framework_detected() {
    let profiles = statico::frameworks::detect_profiles(&fixture("vue-project"));
    let names: Vec<&str> = profiles.iter().map(|p| p.name).collect();
    assert!(names.contains(&"vue"), "expected vue profile, got: {:?}", names);
}

#[test]
fn vue_analyze_succeeds() {
    let output = analyze_fixture("vue-project");
    assert!(!output.structure.source_files.is_empty());
}

#[test]
fn vue_detects_dead_code() {
    let output = analyze_fixture("vue-project");
    let dead_paths: Vec<&str> = output.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(
        dead_paths.iter().any(|p| p.contains("DeadWidget")),
        "expected dead code in DeadWidget, got: {:?}", dead_paths
    );
}

#[test]
fn vue_main_is_entry_point() {
    let output = analyze_fixture("vue-project");
    assert!(
        output.structure.entry_points.iter().any(|e| e.contains("main")),
        "main.ts should be an entry point"
    );
}

// ---------------------------------------------------------------------------
// Svelte
// ---------------------------------------------------------------------------

#[test]
fn svelte_framework_detected() {
    let profiles = statico::frameworks::detect_profiles(&fixture("svelte-project"));
    let names: Vec<&str> = profiles.iter().map(|p| p.name).collect();
    assert!(names.contains(&"svelte"), "expected svelte profile, got: {:?}", names);
}

#[test]
fn svelte_analyze_succeeds() {
    let output = analyze_fixture("svelte-project");
    assert!(!output.structure.source_files.is_empty());
}

#[test]
fn svelte_detects_dead_code() {
    let output = analyze_fixture("svelte-project");
    let dead_paths: Vec<&str> = output.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(
        dead_paths.iter().any(|p| p.contains("dead-util")),
        "expected dead code in dead-util, got: {:?}", dead_paths
    );
}

#[test]
fn svelte_route_is_entry_point() {
    let output = analyze_fixture("svelte-project");
    assert!(
        output.structure.entry_points.iter().any(|e| e.contains("+page")),
        "+page should be an entry point"
    );
}

// ---------------------------------------------------------------------------
// Remix
// ---------------------------------------------------------------------------

#[test]
fn remix_framework_detected() {
    let profiles = statico::frameworks::detect_profiles(&fixture("remix-project"));
    let names: Vec<&str> = profiles.iter().map(|p| p.name).collect();
    assert!(names.contains(&"remix"), "expected remix profile, got: {:?}", names);
}

#[test]
fn remix_analyze_succeeds() {
    let output = analyze_fixture("remix-project");
    assert!(!output.structure.source_files.is_empty());
}

#[test]
fn remix_detects_dead_code() {
    let output = analyze_fixture("remix-project");
    let dead_paths: Vec<&str> = output.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(
        dead_paths.iter().any(|p| p.contains("DeadComp") || p.contains("dead")),
        "expected dead code in DeadComp or dead, got: {:?}", dead_paths
    );
}

#[test]
fn remix_routes_are_entry_points() {
    let output = analyze_fixture("remix-project");
    assert!(
        output.structure.entry_points.iter().any(|e| e.contains("routes")),
        "routes should be entry points"
    );
}

// ---------------------------------------------------------------------------
// Astro
// ---------------------------------------------------------------------------

#[test]
fn astro_framework_detected() {
    let profiles = statico::frameworks::detect_profiles(&fixture("astro-project"));
    let names: Vec<&str> = profiles.iter().map(|p| p.name).collect();
    assert!(names.contains(&"astro"), "expected astro profile, got: {:?}", names);
}

#[test]
fn astro_analyze_succeeds() {
    let output = analyze_fixture("astro-project");
    assert!(!output.structure.source_files.is_empty());
}

#[test]
fn astro_detects_dead_code() {
    let output = analyze_fixture("astro-project");
    let dead_paths: Vec<&str> = output.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(
        dead_paths.iter().any(|p| p.contains("Sidebar") || p.contains("unused")),
        "expected dead code in Sidebar or unused, got: {:?}", dead_paths
    );
}

#[test]
fn astro_pages_are_entry_points() {
    let output = analyze_fixture("astro-project");
    assert!(
        output.structure.entry_points.iter().any(|e| e.contains("pages")),
        "pages should be entry points"
    );
}

// ---------------------------------------------------------------------------
// Barrel Chain Project
// ---------------------------------------------------------------------------

#[test]
fn barrel_chain_analyze_succeeds() {
    let root = fixture("barrel-chain-project");
    let result = statico::analyzer::analyze(&root);
    assert!(result.is_ok(), "barrel chain analysis failed: {:?}", result.err());
}

#[test]
fn barrel_chain_detects_dead() {
    let root = fixture("barrel-chain-project");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(dead_paths.iter().any(|p| p.contains("dead")), "should detect dead.ts");
    assert!(dead_paths.iter().any(|p| p.contains("orphan")), "should detect orphan.ts");
}

#[test]
fn barrel_chain_barrel_files_reachable() {
    let root = fixture("barrel-chain-project");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(!dead_paths.iter().any(|p| p.contains("a.ts")), "a.ts should be reachable via barrel chain");
    assert!(!dead_paths.iter().any(|p| p.contains("b.ts")), "b.ts should be reachable via barrel chain");
    assert!(!dead_paths.iter().any(|p| p.contains("helpers")), "helpers.ts should be reachable via barrel chain");
}

// ---------------------------------------------------------------------------
// Dynamic Imports Project
// ---------------------------------------------------------------------------

#[test]
fn dynamic_imports_analyze_succeeds() {
    let root = fixture("dynamic-imports-project");
    let result = statico::analyzer::analyze(&root);
    assert!(result.is_ok(), "dynamic imports analysis failed: {:?}", result.err());
}

#[test]
fn dynamic_imports_detects_dead() {
    let root = fixture("dynamic-imports-project");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(dead_paths.iter().any(|p| p.contains("dead")), "should detect dead.ts");
}

#[test]
fn dynamic_imports_conditional_reachable() {
    let root = fixture("dynamic-imports-project");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    // conditional.ts is reachable via static import from index.ts
    assert!(!dead_paths.iter().any(|p| p.contains("conditional")), "conditional.ts should be reachable via static import");
    // lazy.ts and feature.ts are reachable via dynamic import() from index.ts
    assert!(!dead_paths.iter().any(|p| p.contains("lazy")), "lazy.ts should be reachable via dynamic import: {:?}", dead_paths);
    assert!(!dead_paths.iter().any(|p| p.contains("feature")), "feature.ts should be reachable via dynamic import: {:?}", dead_paths);
    // debug.ts is reachable via dynamic import from conditional.ts
    assert!(!dead_paths.iter().any(|p| p.contains("debug")), "debug.ts should be reachable via dynamic import: {:?}", dead_paths);
}

// ---------------------------------------------------------------------------
// Circular Deps Project
// ---------------------------------------------------------------------------

#[test]
fn circular_deps_analyze_succeeds() {
    let root = fixture("circular-deps-project");
    let result = statico::analyzer::analyze(&root);
    assert!(result.is_ok(), "circular deps analysis failed: {:?}", result.err());
}

#[test]
fn circular_deps_detected() {
    let root = fixture("circular-deps-project");
    let result = statico::analyzer::analyze(&root).unwrap();
    assert!(!result.issues.circular_dependencies.is_empty(), "should detect circular dependency a→b→c→a");
}

#[test]
fn circular_deps_detects_dead() {
    let root = fixture("circular-deps-project");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(dead_paths.iter().any(|p| p.contains("standalone")), "should detect standalone.ts as dead");
}

// ---------------------------------------------------------------------------
// Type-Only Project
// ---------------------------------------------------------------------------

#[test]
fn type_only_analyze_succeeds() {
    let root = fixture("type-only-project");
    let result = statico::analyzer::analyze(&root);
    assert!(result.is_ok(), "type-only analysis failed: {:?}", result.err());
}

#[test]
fn type_only_detects_dead_runtime() {
    let root = fixture("type-only-project");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(dead_paths.iter().any(|p| p.contains("dead-runtime")), "should detect dead-runtime.ts");
    assert!(dead_paths.iter().any(|p| p.contains("dead-types")), "should detect dead-types.ts");
}

#[test]
fn type_only_runtime_file_reachable() {
    let root = fixture("type-only-project");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    // Helper: exact filename match within a path to avoid substring false positives
    let is_dead = |name: &str| {
        dead_paths.iter().any(|p| {
            p.strip_prefix("src/").map(|s| s == name).unwrap_or(false) || *p == name
        })
    };
    // runtime.ts is reachable via regular import from index.ts
    assert!(!is_dead("runtime.ts"), "runtime.ts should be reachable: {:?}", dead_paths);
    // types.ts is reachable via type-only import — the analyzer should create a dependency edge
    assert!(!is_dead("types.ts"), "types.ts should be reachable via type-only import: {:?}", dead_paths);
}

// ---- Realworld App Tests ----

#[test]
fn realworld_analyze_succeeds() {
    let root = fixture("realworld-app");
    let result = statico::analyzer::analyze(&root);
    assert!(result.is_ok(), "realworld analysis failed: {:?}", result.err());
}

#[test]
fn realworld_detects_nextjs() {
    let root = fixture("realworld-app");
    let profiles = statico::frameworks::detect_profiles(&root);
    assert!(profiles.iter().any(|p| p.name == "nextjs"), "nextjs profile should be detected");
}

#[test]
fn realworld_detects_dead_components() {
    let root = fixture("realworld-app");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(dead_paths.iter().any(|p| p.contains("Sidebar")), "Sidebar should be dead");
    assert!(dead_paths.iter().any(|p| p.contains("Modal")), "Modal should be dead");
    assert!(dead_paths.iter().any(|p| p.contains("LegacyButton")), "LegacyButton should be dead");
}

#[test]
fn realworld_detects_dead_lib_files() {
    let root = fixture("realworld-app");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(dead_paths.iter().any(|p| p.contains("deprecated.ts")), "deprecated.ts should be dead");
    assert!(dead_paths.iter().any(|p| p.contains("analytics")), "analytics.ts should be dead");
}

#[test]
fn realworld_detects_dead_services() {
    let root = fixture("realworld-app");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(dead_paths.iter().any(|p| p.contains("email.service")), "email.service.ts should be dead");
}

#[test]
fn realworld_detects_dead_hooks() {
    let root = fixture("realworld-app");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(dead_paths.iter().any(|p| p.contains("useInfiniteScroll")), "useInfiniteScroll should be dead");
}

#[test]
fn realworld_detects_dead_types() {
    let root = fixture("realworld-app");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(dead_paths.iter().any(|p| p.contains("legacy.ts")), "legacy.ts types should be dead");
}

#[test]
fn realworld_alive_components_not_flagged() {
    let root = fixture("realworld-app");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    // These should NOT be dead
    assert!(!dead_paths.iter().any(|p| p.contains("Header.tsx")), "Header should be alive");
    assert!(!dead_paths.iter().any(|p| p.contains("Footer.tsx")), "Footer should be alive");
    assert!(!dead_paths.iter().any(|p| p.contains("Hero.tsx")), "Hero should be alive");
    assert!(!dead_paths.iter().any(|p| p.contains("Card.tsx")), "Card should be alive");
}

#[test]
fn realworld_alive_lib_files_not_flagged() {
    let root = fixture("realworld-app");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(!dead_paths.iter().any(|p| p.contains("db.ts")), "db.ts should be alive");
    assert!(!dead_paths.iter().any(|p| p.contains("auth.ts")), "auth.ts should be alive");
    assert!(!dead_paths.iter().any(|p| p.contains("utils.ts")), "utils.ts should be alive");
    assert!(!dead_paths.iter().any(|p| p.contains("constants")), "constants.ts should be alive");
}

#[test]
fn realworld_app_router_pages_are_entries() {
    let root = fixture("realworld-app");
    let result = statico::analyzer::analyze(&root).unwrap();
    let eps: Vec<&str> = result.structure.entry_points.iter().map(|s| s.as_str()).collect();
    assert!(eps.iter().any(|p| p.contains("page.tsx")), "page.tsx should be entry");
    assert!(eps.iter().any(|p| p.contains("layout.tsx")), "layout.tsx should be entry");
    assert!(eps.iter().any(|p| p.contains("route.ts")), "route.ts should be entry");
}

#[test]
fn realworld_services_are_alive() {
    let root = fixture("realworld-app");
    let result = statico::analyzer::analyze(&root).unwrap();
    let dead_paths: Vec<&str> = result.issues.dead_code.iter().map(|d| d.path.as_str()).collect();
    assert!(!dead_paths.iter().any(|p| p.contains("user.service")), "user.service should be alive");
    assert!(!dead_paths.iter().any(|p| p.contains("post.service")), "post.service should be alive");
}

// ---------------------------------------------------------------------------
// Framework-specific Gotcha Tests
// ---------------------------------------------------------------------------

#[test]
fn nestjs_body_without_dto_gotcha() {
    let root = fixture("nestjs-project");
    let result = statico::analyzer::analyze(&root).unwrap();
    let body_gotchas: Vec<_> = result.issues.gotchas.iter()
        .filter(|g| g.rule == "nestjs-body-without-dto")
        .collect();
    assert!(!body_gotchas.is_empty(), "Should detect @Body() without DTO in NestJS project");
}

#[test]
fn nextjs_gotchas_only_fire_for_nextjs() {
    let root = fixture("nestjs-project");
    let result = statico::analyzer::analyze(&root).unwrap();
    let nextjs_gotchas: Vec<_> = result.issues.gotchas.iter()
        .filter(|g| g.rule.starts_with("nextjs-"))
        .collect();
    assert!(nextjs_gotchas.is_empty(), "Next.js gotchas should not fire in NestJS project");
}
