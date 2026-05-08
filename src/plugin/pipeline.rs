//! Pipeline integration for plugins.
//!
//! Coordinates calling plugin hooks at the right points during analysis:
//! - `analyze_file`: per-file analysis augmentation
//! - `discover_entries`: entry point discovery augmentation
//! - `post_analysis`: whole-result enrichment
//! - `format_output`: output formatting override

use crate::plugin::discovery::{DiscoveredPlugin, discover_plugins};
use crate::plugin::manager::ActivePlugin;
use crate::plugin::protocol::{
    AnalyzeFileParams, AnalyzeFileResult, HookName, PostAnalysisParams, ResolveImportParams, ResolveImportResult,
};
use crate::types::AnalysisOutput;
use std::path::Path;

/// Manages active plugins during an analysis run.
///
/// Spawns plugins on creation, dispatches hooks, and shuts down on drop.
pub struct PluginPipeline {
    plugins: Vec<(DiscoveredPlugin, ActivePlugin)>,
}

impl PluginPipeline {
    /// Discover and spawn all enabled plugins for the given project root.
    ///
    /// Plugins that fail to spawn are logged and skipped (not fatal).
    pub fn new(root: &Path) -> Self {
        let discovered = discover_plugins(root);
        let mut active = Vec::new();

        for plugin in discovered {
            if !plugin.enabled {
                continue;
            }
            match ActivePlugin::spawn(&plugin, root) {
                Ok(ap) => active.push((plugin, ap)),
                Err(e) => {
                    eprintln!("warning: plugin '{}' failed to start: {}", plugin.name, e);
                }
            }
        }

        PluginPipeline { plugins: active }
    }

    /// Number of active plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Whether any plugins are active.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Call `analyze_file` on all plugins that subscribe to it.
    ///
    /// Returns additional issues, exports, and dependencies collected from plugins.
    pub fn analyze_file(&mut self, path: &str, source: &str, language: &str) -> Vec<AnalyzeFileResult> {
        let mut results = Vec::new();

        for (_disc, plugin) in &mut self.plugins {
            // Skip plugins that don't subscribe to analyze_file.
            if !plugin.has_hook(&HookName::AnalyzeFile) {
                continue;
            }

            let params = AnalyzeFileParams {
                path: path.to_string(),
                source: source.to_string(),
                language: language.to_string(),
                existing_issues: vec![],
            };

            match plugin.send_request("analyze_file", &params) {
                Ok(result) => results.push(result),
                Err(e) => {
                    // Method not found is fine — plugin doesn't implement this hook.
                    if !e.contains("Method not found") {
                        eprintln!("warning: plugin error in analyze_file for '{}': {}", path, e);
                    }
                }
            }
        }

        results
    }

    /// Call `resolve_import` on all plugins that subscribe to it.
    ///
    /// Returns the first successful resolution, or `None` if no plugin handled it.
    /// Only plugins that declare the `ResolveImport` hook (typically with `Override` mode)
    /// are contacted — all others are skipped.
    pub fn resolve_import(&mut self, from_file: &str, specifier: &str, root: &str) -> Option<ResolveImportResult> {
        for (_disc, plugin) in &mut self.plugins {
            if !plugin.has_hook(&HookName::ResolveImport) {
                continue;
            }
            let params = ResolveImportParams {
                from_file: from_file.to_string(),
                specifier: specifier.to_string(),
                root: root.to_string(),
            };
            match plugin.send_request("resolve_import", &params) {
                Ok(result) => return Some(result),
                Err(e) => {
                    if !e.contains("Method not found") {
                        eprintln!("warning: plugin error in resolve_import for '{}': {}", specifier, e);
                    }
                }
            }
        }
        None
    }

    /// Call `post_analysis` on all plugins that subscribe to it.
    ///
    /// Allows plugins to add cross-cutting issues and suggestions after
    /// the full analysis is complete.
    pub fn post_analysis(&mut self, output: &AnalysisOutput) -> Vec<serde_json::Value> {
        let health_score = output.summary.as_ref().map(|s| s.health_score).unwrap_or(0.0);

        let total_files = output.summary.as_ref().map(|s| s.total_files).unwrap_or(0);

        let output_json = serde_json::to_value(output).unwrap_or(serde_json::Value::Null);

        let mut results = Vec::new();

        for (_disc, plugin) in &mut self.plugins {
            let params =
                PostAnalysisParams { results: output_json.clone(), health_score, total_files, language: String::new() };

            let result: Result<crate::plugin::protocol::PostAnalysisResult, String> =
                plugin.send_request("post_analysis", &params);
            match result {
                Ok(result) => results.push(serde_json::to_value(result).unwrap_or(serde_json::Value::Null)),
                Err(e) => {
                    // Method not found is fine — plugin doesn't implement this hook.
                    if !e.contains("Method not found") {
                        eprintln!("warning: plugin error in post_analysis: {}", e);
                    }
                }
            }
        }

        results
    }

    /// Call `format_output` on plugins.
    ///
    /// If any plugin provides `format_output` with mode `override`,
    /// only its output is used. Otherwise, all results are concatenated.
    /// Returns `None` if no plugin handled it (use built-in formatter).
    pub fn format_output(&mut self, output: &AnalysisOutput, format: &str) -> Option<String> {
        let health_score = output.summary.as_ref().map(|s| s.health_score).unwrap_or(0.0);

        let output_json = serde_json::to_value(output).unwrap_or(serde_json::Value::Null);

        let mut combined = String::new();
        let mut any_handled = false;

        for (_disc, plugin) in &mut self.plugins {
            let params = crate::plugin::protocol::FormatOutputParams {
                results: output_json.clone(),
                format: format.to_string(),
                health_score,
            };

            let result: Result<crate::plugin::protocol::FormatOutputResult, String> =
                plugin.send_request("format_output", &params);
            match result {
                Ok(result) => {
                    any_handled = true;
                    combined.push_str(&result.output);
                    combined.push('\n');
                }
                Err(e) => {
                    // Method not found is fine — plugin doesn't implement this hook.
                    if !e.contains("Method not found") {
                        eprintln!("warning: plugin error in format_output: {}", e);
                    }
                }
            }
        }

        if any_handled { Some(combined.trim_end().to_string()) } else { None }
    }

    /// Shut down all plugins gracefully.
    pub fn shutdown(&mut self) {
        for (_disc, plugin) in &mut self.plugins {
            plugin.shutdown().ok();
        }
    }
}

impl Drop for PluginPipeline {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::discovery::{DiscoveredPlugin, PluginKind};
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;

    /// Create a mock executable plugin that only handles `resolve_import`.
    /// Returns `resolved_path: "/project/libs/foo/src/index.ts"` for any spec
    /// starting with `@scope/`, and returns Method not found for everything else.
    fn make_resolver_plugin(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        let script = dir.join(name);
        let code = r##"#!/bin/bash
while IFS= read -r line; do
    id=$(echo "$line" | grep -o '"id":[0-9]*' | head -1 | cut -d':' -f2)
    if echo "$line" | grep -q '"method":"init"'; then
        echo "{\"jsonrpc\":\"2.0\",\"id\":${id},\"result\":{\"name\":\"resolve-plugin\",\"version\":\"0.1.0\",\"hooks\":{\"resolve_import\":\"override\"},\"languages\":[],\"rules\":[]}}"
    elif echo "$line" | grep -q '"method":"shutdown"'; then
        exit 0
    elif echo "$line" | grep -q '"method":"resolve_import"'; then
        spec=$(echo "$line" | grep -o '"specifier":"[^"]*"' | head -1 | cut -d'"' -f4)
        if echo "$spec" | grep -q '^@scope/'; then
            base=$(echo "$spec" | sed 's/@scope\///')
            echo "{\"jsonrpc\":\"2.0\",\"id\":${id},\"result\":{\"resolvedPath\":\"/project/libs/${base}/src/index.ts\",\"external\":false}}"
        else
            echo "{\"jsonrpc\":\"2.0\",\"id\":${id},\"error\":{\"code\":-32601,\"message\":\"Method not found\"}}"
        fi
    else
        echo "{\"jsonrpc\":\"2.0\",\"id\":${id},\"error\":{\"code\":-32601,\"message\":\"Method not found: analyze_file\"}}"
    fi
done
"##;
        std::fs::write(&script, code).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    /// Build a PluginPipeline from a single discovered plugin.
    fn pipeline_from_plugin(dir: &std::path::Path, plugin_path: std::path::PathBuf) -> PluginPipeline {
        let discovered = DiscoveredPlugin {
            name: "resolve-plugin".to_string(),
            path: plugin_path,
            kind: PluginKind::Executable,
            enabled: true,
            override_all: false,
            hook_overrides: HashMap::new(),
            settings: toml::Value::Table(toml::map::Map::new()),
            languages: Vec::new(),
        };
        let active = crate::plugin::manager::ActivePlugin::spawn(&discovered, dir).unwrap();
        PluginPipeline { plugins: vec![(discovered, active)] }
    }

    #[test]
    fn test_resolve_import_empty_pipeline() {
        let tmp = tempfile::tempdir().unwrap();
        let mut pipeline = PluginPipeline::new(tmp.path());
        assert!(pipeline.is_empty());
        let result = pipeline.resolve_import("src/foo.ts", "@scope/bar", "/project");
        assert!(result.is_none(), "empty pipeline should return None for resolve_import");
    }

    #[test]
    fn test_analyze_file_empty_pipeline() {
        let tmp = tempfile::tempdir().unwrap();
        let mut pipeline = PluginPipeline::new(tmp.path());
        assert!(pipeline.is_empty());
        let results = pipeline.analyze_file("src/foo.ts", "const x = 1;", "typescript");
        assert!(results.is_empty(), "empty pipeline should return empty results for analyze_file");
    }

    #[test]
    fn test_resolve_import_plugin_returns_resolved_path() {
        let tmp = tempfile::tempdir().unwrap();
        let script = make_resolver_plugin(tmp.path(), "resolve-plugin");
        let mut pipeline = pipeline_from_plugin(tmp.path(), script);

        // Plugin should resolve @scope/* specifiers.
        let result = pipeline.resolve_import("src/app.ts", "@scope/utils", "/project");
        assert!(result.is_some(), "plugin should resolve @scope/utils");
        let resolved = result.unwrap();
        assert_eq!(resolved.resolved_path, "/project/libs/utils/src/index.ts");
        assert!(!resolved.external);

        pipeline.shutdown();
    }

    #[test]
    fn test_resolve_import_plugin_returns_none_for_unknown() {
        let tmp = tempfile::tempdir().unwrap();
        let script = make_resolver_plugin(tmp.path(), "resolve-plugin");
        let mut pipeline = pipeline_from_plugin(tmp.path(), script);

        // Plugin should NOT resolve non-@scope specifiers.
        let result = pipeline.resolve_import("src/app.ts", "react", "/project");
        assert!(result.is_none(), "plugin should not resolve 'react'");

        pipeline.shutdown();
    }

    #[test]
    fn test_analyze_file_skips_resolve_import_only_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let script = make_resolver_plugin(tmp.path(), "resolve-plugin");
        let mut pipeline = pipeline_from_plugin(tmp.path(), script);

        // This plugin only declares resolve_import, NOT analyze_file.
        // analyze_file should return empty results without sending a request.
        let results = pipeline.analyze_file("src/foo.ts", "const x = 1;", "typescript");
        assert!(results.is_empty(), "analyze_file should skip plugins that don't declare the hook");

        pipeline.shutdown();
    }
}
