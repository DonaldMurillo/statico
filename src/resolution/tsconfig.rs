//! Tsconfig parsing and path alias management.
//!
//! Handles JSONC stripping, tsconfig path alias extraction, and scoped alias resolution.

use std::path::{Path, PathBuf};

use super::paths::try_extensions;
use super::{path_relative_to, PathAlias};

/// Path aliases loaded from a single tsconfig.json.
#[derive(Clone)]
pub(super) struct TsconfigScope {
    /// Directory containing this tsconfig, relative to root (e.g. "apps/web").
    pub dir_rel: String,
    /// Path aliases from this tsconfig's compilerOptions.paths.
    pub aliases: Vec<PathAlias>,
}

/// Strip JSONC comments (// and /* */) from a string so serde_json can parse it.
/// Handles strings correctly — comments inside string literals are preserved.
pub(super) fn strip_jsonc(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut in_string = false;

    while i < chars.len() {
        if in_string {
            out.push(chars[i]);
            if chars[i] == '\\' && i + 1 < chars.len() {
                i += 1;
                out.push(chars[i]);
            } else if chars[i] == '"' {
                in_string = false;
            }
            i += 1;
        } else if chars[i] == '"' {
            in_string = true;
            out.push(chars[i]);
            i += 1;
        } else if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            // Line comment — skip to end of line.
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            // Block comment — skip to *​/.
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < chars.len() {
                i += 2; // skip *​/
            } else {
                break; // unterminated block comment — stop
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Load path aliases from a tsconfig.json file (if it exists).
pub(super) fn parse_tsconfig_paths(tsconfig_path: &Path) -> Option<Vec<PathAlias>> {
    let content = std::fs::read_to_string(tsconfig_path).ok()?;
    let tsconfig: serde_json::Value = serde_json::from_str(&strip_jsonc(&content)).ok()?;
    let paths = tsconfig.get("compilerOptions").and_then(|co| co.get("paths")).and_then(|p| p.as_object())?;

    let mut aliases = Vec::new();
    for (pattern, targets) in paths {
        let is_wildcard = pattern.ends_with('*');
        let prefix = if is_wildcard {
            pattern.trim_end_matches('*').to_string()
        } else {
            pattern.clone()
        };

        let target_list: Vec<String> = targets
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .map(|t| if t.ends_with('*') { t.trim_end_matches('*').to_string() } else { t.clone() })
                    .filter(|t| !t.starts_with('/') && !t.contains(".."))
                    .collect()
            })
            .unwrap_or_default();

        if !prefix.is_empty() && !target_list.is_empty() {
            aliases.push(PathAlias { prefix, targets: target_list });
        }
    }
    Some(aliases)
}

/// Load tsconfig paths from a sub-project tsconfig, converting relative paths
/// to root-relative. Returns (global_aliases, scoped_alias).
pub(super) fn parse_tsconfig_paths_relative(
    tsconfig_path: &Path,
    root: &Path,
) -> Option<(Vec<PathAlias>, TsconfigScope)> {
    let content = std::fs::read_to_string(tsconfig_path).ok()?;
    let tsconfig: serde_json::Value = serde_json::from_str(&strip_jsonc(&content)).ok()?;
    let paths = tsconfig.get("compilerOptions").and_then(|co| co.get("paths")).and_then(|p| p.as_object())?;

    let tsconfig_dir = tsconfig_path.parent()?;
    let tsconfig_dir_rel = path_relative_to(root, tsconfig_dir);

    let base_url = tsconfig
        .get("compilerOptions")
        .and_then(|co| co.get("baseUrl"))
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    let convert_target = |t: &str, tsconfig_dir_rel: &str, base_url: &str| -> Option<String> {
        let bare = if t.ends_with('*') { t.trim_end_matches('*').to_string() } else { t.to_string() };
        // Reject targets that escape the project via ..
        if bare.starts_with('/') || bare.contains("..") {
            return None;
        }
        let resolved = if bare.starts_with('.') {
            format!("./{}/{}", tsconfig_dir_rel, bare.trim_start_matches("./"))
        } else {
            format!(
                "./{}/{}/{}",
                tsconfig_dir_rel,
                base_url.trim_start_matches("./"),
                bare.trim_start_matches("./")
            )
        };
        Some(resolved.replace("././", "./").replace("//", "/"))
    };

    let mut global_aliases: Vec<PathAlias> = Vec::new();
    let mut scoped_aliases: Vec<PathAlias> = Vec::new();

    for (pattern, targets) in paths {
        let is_wildcard = pattern.ends_with('*');
        let prefix = if is_wildcard { pattern.trim_end_matches('*').to_string() } else { pattern.clone() };

        let target_list: Vec<String> = targets
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .filter_map(|t| convert_target(&t, &tsconfig_dir_rel, base_url))
                    .collect()
            })
            .unwrap_or_default();

        if !prefix.is_empty() && !target_list.is_empty() {
            global_aliases.push(PathAlias { prefix: prefix.clone(), targets: target_list.clone() });
            scoped_aliases.push(PathAlias { prefix, targets: target_list });
        }
    }

    if scoped_aliases.is_empty() {
        return None;
    }

    Some((global_aliases, TsconfigScope { dir_rel: tsconfig_dir_rel, aliases: scoped_aliases }))
}

/// Try resolving using the nearest tsconfig scope's aliases.
pub(super) fn resolve_scoped(
    scopes: &[TsconfigScope],
    root: &Path,
    from_dir: &Path,
    spec: &str,
) -> Option<PathBuf> {
    let from_rel = path_relative_to(root, from_dir);

    // Find the nearest scope (longest matching prefix).
    let best_scope = scopes
        .iter()
        .filter(|scope| from_rel.starts_with(&scope.dir_rel))
        .max_by_key(|scope| scope.dir_rel.len());

    let scope = best_scope?;

    for alias in &scope.aliases {
        if let Some(rest) = spec.strip_prefix(&alias.prefix) {
            for target in &alias.targets {
                let resolved_path = root.join(target).join(rest);
                if let Some(found) = try_extensions(&resolved_path) {
                    return Some(found);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn sec_jsonc_handles_unterminated_comment() {
        // Unterminated block comment should not panic or over-read
        let input = "/* this is never closed";
        let result = strip_jsonc(input);
        // Should return empty (comment content removed, no panic)
        assert_eq!(result, "", "unterminated block comment should produce empty output, got: {}", result);
    }

    #[test]
    fn sec_jsonc_strips_block_comment() {
        let input = "before /* comment */ after";
        let result = strip_jsonc(input);
        assert_eq!(result, "before  after");
    }

    #[test]
    fn sec_tsconfig_target_rejects_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let tsconfig_dir = tmp.path().join("apps/web");
        std::fs::create_dir_all(&tsconfig_dir).unwrap();
        std::fs::write(
            tsconfig_dir.join("tsconfig.json"),
            r#"{
                "compilerOptions": {
                    "paths": {
                        "@evil/*": ["../../etc/*"]
                    }
                }
            }"#, 
        ).unwrap();
        let result = parse_tsconfig_paths_relative(&tsconfig_dir.join("tsconfig.json"), tmp.path());
        if let Some((ref aliases, _)) = result {
            for alias in aliases {
                for target in &alias.targets {
                    assert!(!target.contains(".."),
                        "target should not contain ..: {}", target);
                }
            }
        }
        // The traversal targets should be filtered out, so no alias with prefix @evil/ should exist
        if let Some((aliases, _)) = result {
            let evil_aliases: Vec<_> = aliases.iter().filter(|a| a.prefix == "@evil/").collect();
            for a in &evil_aliases {
                assert!(a.targets.is_empty(),
                    "@evil/ alias targets should be empty after filtering: {:?}", a.targets);
            }
        }
    }

    #[test]
    fn sec_tsconfig_non_relative_rejects_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("tsconfig.json"),
            r#"{
                "compilerOptions": {
                    "paths": {
                        "@app/*": ["../../etc/*"],
                        "@ok/*": ["src/*"]
                    }
                }
            }"#, 
        ).unwrap();
        let result = parse_tsconfig_paths(&tmp.path().join("tsconfig.json"));
        if let Some(aliases) = result {
            for alias in &aliases {
                for target in &alias.targets {
                    assert!(!target.contains(".."),
                        "target should not contain ..: {}", target);
                    assert!(!target.starts_with('/'),
                        "target should not be absolute: {}", target);
                }
            }
        }
    }
}
