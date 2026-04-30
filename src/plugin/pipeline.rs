//! Pipeline integration for plugins.
//!
//! Coordinates calling plugin hooks at the right points during analysis:
//! - `analyze_file`: per-file analysis augmentation
//! - `discover_entries`: entry point discovery augmentation
//! - `post_analysis`: whole-result enrichment
//! - `format_output`: output formatting override

use crate::plugin::discovery::{discover_plugins, DiscoveredPlugin};
use crate::plugin::manager::ActivePlugin;
use crate::plugin::protocol::{AnalyzeFileParams, AnalyzeFileResult, PostAnalysisParams};
use crate::types::AnalysisOutput;
use std::path::Path;

/// Manages active plugins during an analysis run.
///
/// Spawns plugins on creation, dispatches hooks, and shuts down on drop.
pub struct PluginPipeline {
    plugins: Vec<(DiscoveredPlugin, ActivePlugin)>,
    root: std::path::PathBuf,
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
                    eprintln!(
                        "warning: plugin '{}' failed to start: {}",
                        plugin.name, e
                    );
                }
            }
        }

        PluginPipeline {
            plugins: active,
            root: root.to_path_buf(),
        }
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
    pub fn analyze_file(
        &mut self,
        path: &str,
        source: &str,
        language: &str,
    ) -> Vec<AnalyzeFileResult> {
        let mut results = Vec::new();

        for (_disc, plugin) in &mut self.plugins {
            let params = AnalyzeFileParams {
                path: path.to_string(),
                source: source.to_string(),
                language: language.to_string(),
                existing_issues: vec![],
            };

            match plugin.send_request("analyze_file", &params) {
                Ok(result) => results.push(result),
                Err(e) => {
                    eprintln!(
                        "warning: plugin error in analyze_file for '{}': {}",
                        path, e
                    );
                }
            }
        }

        results
    }

    /// Call `post_analysis` on all plugins that subscribe to it.
    ///
    /// Allows plugins to add cross-cutting issues and suggestions after
    /// the full analysis is complete.
    pub fn post_analysis(&mut self, output: &AnalysisOutput) -> Vec<serde_json::Value> {
        let health_score = output
            .summary
            .as_ref()
            .map(|s| s.health_score)
            .unwrap_or(0.0);

        let total_files = output
            .summary
            .as_ref()
            .map(|s| s.total_files)
            .unwrap_or(0);

        let output_json = serde_json::to_value(output).unwrap_or(serde_json::Value::Null);

        let mut results = Vec::new();

        for (_disc, plugin) in &mut self.plugins {
            let params = PostAnalysisParams {
                results: output_json.clone(),
                health_score,
                total_files,
                language: String::new(),
            };

            let result: Result<crate::plugin::protocol::PostAnalysisResult, String> = plugin.send_request("post_analysis", &params);
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
    pub fn format_output(
        &mut self,
        output: &AnalysisOutput,
        format: &str,
    ) -> Option<String> {
        let health_score = output
            .summary
            .as_ref()
            .map(|s| s.health_score)
            .unwrap_or(0.0);

        let output_json = serde_json::to_value(output).unwrap_or(serde_json::Value::Null);

        let mut combined = String::new();
        let mut any_handled = false;

        for (_disc, plugin) in &mut self.plugins {
            let params = crate::plugin::protocol::FormatOutputParams {
                results: output_json.clone(),
                format: format.to_string(),
                health_score,
            };

            let result: Result<crate::plugin::protocol::FormatOutputResult, String> = plugin.send_request("format_output", &params);
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

        if any_handled {
            Some(combined.trim_end().to_string())
        } else {
            None
        }
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
