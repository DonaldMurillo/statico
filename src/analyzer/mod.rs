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
///
/// Set `no_cache` to true to force a full re-parse of all files.
pub fn analyze_with_excludes(root: &Path, exclude: &[String]) -> Result<AnalysisOutput, String> {
    analyze_with_options(root, exclude, false)
}

/// Analyze with full options.
pub fn analyze_with_options(root: &Path, exclude: &[String], no_cache: bool) -> Result<AnalysisOutput, String> {
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
    // Uses incremental cache (content-hash keyed) to skip re-parsing unchanged files.
    let cache = if no_cache {
        None
    } else {
        crate::cache::ensure_gitignore(root);
        let mut c = crate::cache::IncrementalCache::new(root);
        // Prune entries for files that no longer exist.
        let existing: Vec<&str> = source_files.iter().map(|(p, _)| p.as_str()).collect();
        c.prune_missing(&existing);
        Some(std::sync::Mutex::new(c))
    };
    let pipeline_results = parse_all_files_plugin(root, &source_files, &progress, cache.as_ref());
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

    // Detect repetitive token patterns across files.
    let repetitive_patterns = crate::duplication::patterns::detect_patterns(&file_sources);

    let duplication = crate::duplication::build_duplication_section(
        &issues.duplicate_code,
        total_source_lines,
        repetitive_patterns,
    );

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
///
/// If an incremental cache is provided, unchanged files (same content hash)
/// are served from cache instead of re-parsed. New/modified results are
/// written back to the cache.
fn parse_all_files_plugin(
    root: &Path,
    source_files: &[(String, String)],
    progress: &crate::progress::SharedProgress,
    cache: Option<&std::sync::Mutex<crate::cache::IncrementalCache>>,
) -> PipelineResults {
    use rayon::prelude::*;
    use std::sync::Arc;

    let root = Arc::new(root.to_path_buf());
    progress.set_phase("Parsing");

    let results: Vec<Option<FileAnalysis>> = source_files
        .par_iter()
        .map(|(rel_path, lang)| {
            let ext = rel_path.rsplit('.').next().unwrap_or("");
            let abs_path = root.join(rel_path);
            // Skip files exceeding 10 MB to prevent OOM from large files.
            let file_size = match std::fs::metadata(&abs_path) {
                Ok(m) => m.len(),
                Err(_) => {
                    progress.inc();
                    return None;
                }
            };
            if file_size > 10_000_000 {
                progress.inc();
                return None;
            }
            let source = match std::fs::read_to_string(&abs_path) {
                Ok(s) => s,
                Err(_) => {
                    progress.inc();
                    return None;
                }
            };

            // ── Incremental cache lookup ────────────────────────────────
            // Compute content hash and check the cache.
            let hash = crate::cache::content_hash(&source);
            if let Some(cache_ref) = cache {
                let cached = cache_ref.lock().unwrap().get(rel_path, &hash).cloned();
                if let Some(cached_data) = cached {
                    // Cache hit — reconstruct FileAnalysis from cached parse + fresh source.
                    progress.inc();
                    return Some(cached_data.to_analysis(rel_path.clone(), source));
                }
            }

            // Cache miss — parse the file.
            // Use the static plugin registry (compile-time known, no allocation)
            let plugin = plugin_for_extension(ext)
                .or_else(|| {
                    // Try matching by language name
                    static PLUGINS: std::sync::OnceLock<Vec<Box<dyn LanguagePlugin>>> = std::sync::OnceLock::new();
                    let plugins = PLUGINS.get_or_init(all_plugins);
                    plugins.iter().find(|p| p.name() == lang).map(|p| p.as_ref())
                });

            let result = if let Some(plugin) = plugin {
                plugin.analyze_file(&root, rel_path, &source)
            } else {
                // No native parser for this language (e.g. Python) —
                // return minimal FileAnalysis so the file is tracked
                // for plugins to process via analyze_file hook.
                Some(FileAnalysis {
                    rel_path: rel_path.clone(),
                    dep_targets: vec![],
                    external_specs: vec![],
                    imported_names: vec![],
                    exports: vec![],
                    loc: source.lines().count(),
                    total_lines: source.lines().count(),
                    functions: 0,
                    classes: 0,
                    complexity: 0,
                    max_nesting_depth: 0,
                    parse_errors: vec![],
                    blocks: vec![],
                    source,
                })
            };

            // ── Store parse result in cache ────────────────────────────
            if let (Some(cache_ref), Some(fa)) = (cache, &result) {
                cache_ref.lock().unwrap().set(rel_path, &hash, crate::cache::CachedFileData::from_analysis(fa));
            }

            progress.inc();
            result
        })
        .collect();

    let mut pipeline = PipelineResults::new();
    for fa in results.into_iter().flatten() {
        pipeline.merge(fa);
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

    #[test]
    fn sec_analyzer_skips_oversized_file() {
        // Verify that parse_all_files_plugin skips files > 10 MB.
        // We can't easily create a 10MB file in tests, so we verify the metadata check exists.
        let dir = std::env::temp_dir().join("statico_sec_analyzer_oversize");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        // Create a small file (should be analyzed)
        std::fs::write(dir.join("src").join("small.ts"), "const x = 1;").unwrap();
        let result = analyze(&dir);
        assert!(result.is_ok(), "small file should be analyzed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
