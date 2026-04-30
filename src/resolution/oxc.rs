//! oxc_resolver integration for deep module resolution.
//!
//! Builds per-tsconfig resolver scopes so `@/` in `apps/api/v2/`
//! resolves differently than `@/` in `packages/atoms/`.

use std::path::{Path, PathBuf};

use super::path_relative_to;

/// Collection of per-tsconfig oxc_resolver instances for scoped resolution.
pub(super) struct OxcScopes {
    /// Global resolver using root tsconfig (or default if none).
    pub global: oxc_resolver::Resolver,
    /// Per-directory resolvers: (tsconfig_dir_relative, resolver).
    pub scopes: Vec<(String, oxc_resolver::Resolver)>,
    /// Workspace aliases for all scopes.
    #[allow(dead_code)]
    pub workspace_aliases: Vec<(String, Vec<oxc_resolver::AliasValue>)>,
}

/// Build oxc_resolver scopes: one global + one per tsconfig sub-directory.
pub(super) fn build_oxc_scopes(root: &Path) -> OxcScopes {
    use oxc_resolver::{ResolveOptions, TsconfigDiscovery, TsconfigOptions, TsconfigReferences};

    let workspace_aliases = build_workspace_aliases(root);
    let root_str = root.to_string_lossy().to_string();

    let default_opts = ResolveOptions {
        extensions: vec![".ts".into(), ".tsx".into(), ".js".into(), ".jsx".into(), ".mjs".into(), ".cjs".into()],
        main_fields: vec!["types".into(), "typings".into(), "module".into(), "main".into()],
        condition_names: vec![
            "import".into(),
            "module".into(),
            "require".into(),
            "default".into(),
            "types".into(),
            "node".into(),
        ],
        alias: workspace_aliases.clone(),
        modules: vec!["node_modules".into(), root_str.clone()],
        ..ResolveOptions::default()
    };

    // Global resolver (no tsconfig or root tsconfig).
    let global_tsconfig = root.join("tsconfig.json");
    let global_opts = if global_tsconfig.exists() {
        ResolveOptions {
            tsconfig: Some(TsconfigDiscovery::Manual(TsconfigOptions {
                config_file: global_tsconfig.clone(),
                references: TsconfigReferences::Auto,
            })),
            ..default_opts.clone()
        }
    } else {
        default_opts.clone()
    };
    let global = oxc_resolver::Resolver::new(global_opts);

    // Per-scope resolvers from sub-project tsconfig files.
    let mut scopes: Vec<(String, oxc_resolver::Resolver)> = Vec::new();
    for entry in walkdir::WalkDir::new(root).max_depth(6).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() || path.file_name().map_or(true, |n| n != "tsconfig.json") {
            continue;
        }
        let rel = path_relative_to(root, path);
        if rel.contains("node_modules")
            || rel.contains(".svelte-kit")
            || rel.contains("dist/")
            || rel.contains(".next/")
            || rel == "tsconfig.json"
        {
            continue;
        }
        let tsconfig_dir = match path.parent() {
            Some(d) => d,
            None => continue,
        };
        let tsconfig_dir_rel = path_relative_to(root, tsconfig_dir);

        let opts = ResolveOptions {
            tsconfig: Some(TsconfigDiscovery::Manual(TsconfigOptions {
                config_file: path.to_path_buf(),
                references: TsconfigReferences::Auto,
            })),
            ..default_opts.clone()
        };
        scopes.push((tsconfig_dir_rel, oxc_resolver::Resolver::new(opts)));
    }

    OxcScopes { global, scopes, workspace_aliases }
}

/// Build oxc-compatible alias list from workspace package.json files.
fn build_workspace_aliases(root: &Path) -> Vec<(String, Vec<oxc_resolver::AliasValue>)> {
    use oxc_resolver::AliasValue;

    let mut aliases: Vec<(String, Vec<AliasValue>)> = Vec::new();

    for entry in walkdir::WalkDir::new(root).max_depth(5).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() || path.file_name().map_or(true, |n| n != "package.json") {
            continue;
        }
        let rel = path_relative_to(root, path);
        if rel.contains("node_modules") {
            continue;
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let pkg: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let name = match pkg.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };
        if !name.starts_with('@') && !name.contains('/') {
            continue;
        }

        let pkg_dir = path.parent().unwrap();
        aliases.push((format!("{}$", name), vec![AliasValue::Path(pkg_dir.to_string_lossy().to_string())]));
        aliases.push((name.to_string(), vec![AliasValue::Path(pkg_dir.to_string_lossy().to_string())]));
    }

    aliases
}

/// Try oxc resolution using the nearest tsconfig scope.
pub(super) fn resolve_oxc(
    oxc_scopes: &OxcScopes,
    root: &Path,
    from_dir: &Path,
    spec: &str,
) -> Option<PathBuf> {
    let from_rel = path_relative_to(root, from_dir);

    // Find the nearest scope (longest matching prefix).
    let best_scope = oxc_scopes
        .scopes
        .iter()
        .filter(|(dir_rel, _)| from_rel.starts_with(dir_rel))
        .max_by_key(|(dir_rel, _)| dir_rel.len())
        .map(|(_, resolver)| resolver);

    let resolver = best_scope.unwrap_or(&oxc_scopes.global);

    match resolver.resolve(from_dir, spec) {
        Ok(resolution) => {
            let path = resolution.full_path();
            if let Ok(rel) = path.strip_prefix(root) {
                Some(root.join(rel))
            } else {
                Some(path.to_path_buf())
            }
        }
        Err(_) => None,
    }
}
