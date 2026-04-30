//! Main analysis orchestrator. Coordinates parsing, discovery, and issue detection.

mod parse_rust;
mod parse_typescript;

use std::path::Path;

use crate::parse::AstParser;
use crate::types::*;

use crate::discovery::{discover_config_files, discover_entry_points, discover_source_files};
use crate::issues::{detect_issues, IssueContext};
use crate::resolution::Resolver;

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

    let parser = AstParser::new().map_err(|e| format!("failed to init parser: {}", e))?;

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

    // Build resolver with tsconfig path aliases and workspace package mappings.
    let mut resolver = Resolver::new(root);
    let tsconfig_path = root.join("tsconfig.json");
    if tsconfig_path.exists() {
        resolver.load_tsconfig_paths(&tsconfig_path);
    }
    // Also try tsconfig.app.json (Next.js 15+ splits these).
    let tsconfig_app = root.join("tsconfig.app.json");
    if tsconfig_app.exists() {
        resolver.load_tsconfig_paths(&tsconfig_app);
    }
    // Load workspace package name → directory mappings for monorepo support.
    resolver.load_workspace_packages();
    // Load tsconfig path aliases from all sub-project tsconfig.json files.
    // This ensures @/ aliases in workspace packages (apps/api/tsconfig.json etc.)
    // are resolved correctly, not just the root tsconfig.
    resolver.load_all_tsconfig_paths();

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

    let (
        dependencies,
        quality,
        dep_graph,
        file_exports,
        file_loc,
        file_total_lines,
        file_blocks,
        file_sources,
        imported_names,
    ) = parse_typescript::parse_all_files_parallel(root, &source_files, &parser, &resolver, &progress);
    progress.finish();

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
