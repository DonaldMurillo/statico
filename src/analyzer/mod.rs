//! Main analysis orchestrator. Coordinates parsing, discovery, and issue detection.

mod parse_rust;
mod parse_typescript;

pub use parse_rust::parse_rust_file_standalone;
pub use parse_rust::resolve_rust_use_path_public;

use std::path::Path;

use crate::types::*;

use crate::discovery::{discover_config_files, discover_entry_points, discover_source_files};
use crate::issues::{detect_issues, IssueContext};
use crate::languages::{FileAnalysis, LanguagePlugin, PipelineResults, all_plugins, plugin_for_extension};

/// Run the full analysis on a project directory.
pub fn analyze(root: &Path) -> Result<AnalysisOutput, String> {
    analyze_with_excludes(root, &[])
}

/// Analyze with optional exclude patterns.
pub fn analyze_with_excludes(root: &Path, exclude: &[String]) -> Result<AnalysisOutput, String> {
    if !root.exists() {
        return Err(format!("path not found: {}", root.display()));
    }
    if !root.is_dir() {
        return Err(format!("not a directory: {}", root.display()));
    }

    let source_files = discover_source_files(root)?;
    let source_files = crate::discovery::filter_excluded(source_files, exclude);
    let config_files = discover_config_files(root);
    let entry = discover_entry_points(root, &source_files);

    let entry_points: Vec<String> = entry.framework.iter().cloned().collect();
    let implicit_entries: Vec<String> = entry.implicit.iter().cloned().collect();
    let mut public_api: Vec<String> = entry.public_api.iter().cloned().collect();
    let all_entries: Vec<String> = entry.all().into_iter().collect();

    // Auto-detect published packages under packages/ — their source files
    // are public API. Published packages expose their exports for external
    // consumers, so unused exports within the monorepo are expected.
    let published_dirs = crate::resolution::find_published_package_dirs(root);
    if !published_dirs.is_empty() {
        for (rel_path, _lang) in &source_files {
            for pkg_dir in &published_dirs {
                if rel_path.starts_with(pkg_dir.as_str()) || rel_path.starts_with(&format!("{}/", pkg_dir)) {
                    public_api.push(rel_path.clone());
                    break;
                }
            }
        }
    }

    let structure = Structure {
        root: root.to_path_buf(),
        entry_points,
        implicit_entries,
        source_files: source_files
            .iter()
            .map(|(rel, lang)| SourceFile { path: rel.clone(), language: lang.clone() })
            .collect(),
        config_files,
    };

    let progress = crate::progress::shared_progress(source_files.len());
    progress.set_quiet(true); // TODO: wire to --quiet flag

    // ── Plugin-dispatched parsing ──────────────────────────────────────
    // Route each file to its language plugin and collect results.
    let pipeline_results = parse_all_files_plugin(root, &source_files, &progress);
    progress.finish();

    let PipelineResults {
        dependencies,
        quality,
        dep_graph,
        file_exports,
        file_loc,
        file_total_lines,
        file_blocks,
        file_sources,
        imported_names,
    } = pipeline_results;

    let fw_profiles = crate::frameworks::detect_profiles(root);

    let issues = detect_issues(&IssueContext {
        all_entries: &all_entries,
        framework_entries: &structure.entry_points,
        dep_graph: &dep_graph,
        file_exports: &file_exports,
        file_loc: &file_loc,
        file_blocks: &file_blocks,
        file_sources: &file_sources,
        root,
        external_imports: &dependencies.external,
        imported_names: &imported_names,
        profiles: &fw_profiles,
        public_api: &public_api,
    });

    // Use total lines (including blanks/comments) for dup % — matches jscpd/fallow methodology.
    let total_source_lines: usize = file_total_lines.values().sum();
    let duplication = crate::duplication::build_duplication_section(&issues.duplicate_code, total_source_lines);

    // Detect monorepo setup.
    let monorepo = crate::monorepo::detect_monorepo(root)
        .map(|info| MonorepoInfoData { kind: info.kind.to_string(), packages: info.packages.clone() });

    // Framework names derived from profiles detected earlier.
    let detected_frameworks: Vec<String> = fw_profiles.iter().map(|p| p.name.to_string()).collect();

    Ok(AnalysisOutput {
        version: None,
        summary: None,
        detected_frameworks: Some(detected_frameworks),
        monorepo,
        structure,
        dependencies,
        quality,
        issues,
        duplication,
    })
}

/// Parse all source files by dispatching to language plugins in parallel.
fn parse_all_files_plugin(
    root: &Path,
    source_files: &[(String, String)],
    progress: &crate::progress::SharedProgress,
) -> PipelineResults {
    use rayon::prelude::*;
    use std::sync::Arc;

    let root = Arc::new(root.to_path_buf());
    progress.set_phase("Parsing");

    let results: Vec<Option<FileAnalysis>> = source_files
        .par_iter()
        .map(|(rel_path, lang)| {
            let ext = rel_path.rsplit('.').next().unwrap_or("");
            // Use the static plugin registry (compile-time known, no allocation)
            let plugin = plugin_for_extension(ext)
                .or_else(|| {
                    // Try matching by language name
                    static PLUGINS: std::sync::OnceLock<Vec<Box<dyn LanguagePlugin>>> = std::sync::OnceLock::new();
                    let plugins = PLUGINS.get_or_init(all_plugins);
                    plugins.iter().find(|p| p.name() == lang).map(|p| p.as_ref())
                })
                .unwrap_or_else(|| {
                    // Last resort: use TypeScript plugin
                    plugin_for_extension("ts").unwrap()
                });

            let abs_path = root.join(rel_path);
            let source = match std::fs::read_to_string(&abs_path) {
                Ok(s) => s,
                Err(_) => {
                    progress.inc();
                    return None;
                }
            };

            let result = plugin.analyze_file(&root, rel_path, &source);
            progress.inc();
            result
        })
        .collect();

    let mut pipeline = PipelineResults::new();
    for res in results {
        if let Some(fa) = res {
            pipeline.merge(fa);
        }
    }
    pipeline.sort();
    pipeline
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_nonexistent_path() {
        let result = analyze(Path::new("/no/such/path"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("path not found"));
    }
}
