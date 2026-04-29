use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level JSON output for `statico analyze`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AnalysisOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_frameworks: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monorepo: Option<MonorepoInfoData>,
    pub structure: Structure,
    pub dependencies: Dependencies,
    pub quality: Quality,
    pub issues: Issues,
    pub duplication: DuplicationSection,
}

/// Pre-computed summary for enriched output.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Summary {
    pub total_files: usize,
    pub total_lines: usize,
    pub total_exports: usize,
    pub total_types: usize,
    pub issue_counts: IssueCounts,
    pub health_score: f64,
    pub duplication_percentage: f64,
}

/// Counts of issues by category.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IssueCounts {
    pub dead_code: usize,
    pub unused_exports: usize,
    pub unused_types: usize,
    pub duplicate_code: usize,
    pub gotchas: usize,
    pub circular_dependencies: usize,
    pub unused_dependencies: usize,
    pub duplicate_exports: usize,
    pub unresolved_imports: usize,
    pub unlisted_dependencies: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Structure {
    pub root: PathBuf,
    /// Framework entry points — files whose exports are consumed by the framework
    /// (pages, routes, layouts, etc.). Their exports are NOT checked for usage.
    pub entry_points: Vec<String>,
    /// Implicit entries — tooling/config/scripts (tests, scripts, generated files).
    /// Their exports ARE still checked for usage by the unused_exports detector.
    pub implicit_entries: Vec<String>,
    pub source_files: Vec<SourceFile>,
    pub config_files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SourceFile {
    pub path: String,
    pub language: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Dependencies {
    pub imports: Vec<FileImports>,
    pub external: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileImports {
    pub source: String,
    pub targets: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Quality {
    pub files: Vec<FileQuality>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileQuality {
    pub path: String,
    pub metrics: Option<Metrics>,
    pub exports: Vec<String>,
    pub parse_errors: Vec<ParseError>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Metrics {
    pub lines_of_code: usize,
    pub total_lines: usize,
    pub functions: usize,
    pub classes: usize,
    pub complexity: usize,
    pub max_nesting_depth: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

// ---------------------------------------------------------------------------
// Issues
// ---------------------------------------------------------------------------

/// Detected issues in the codebase.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Issues {
    /// Files that are not reachable from any entry point.
    pub dead_code: Vec<DeadCodeIssue>,
    /// Exports that are defined but never imported by any file.
    pub unused_exports: Vec<UnusedExportIssue>,
    /// The same export name defined in multiple files.
    pub duplicate_exports: Vec<DuplicateExportIssue>,
    /// Code blocks that are similar across files (informational, not failures).
    pub duplicate_code: Vec<DuplicateCodeIssue>,
    /// Common error-prone patterns (gotchas).
    pub gotchas: Vec<GotchaIssue>,
    /// Exported TypeScript types/interfaces that are never imported.
    pub unused_types: Vec<UnusedTypeIssue>,
    /// Circular import dependency chains.
    pub circular_dependencies: Vec<CircularDepIssue>,
    /// npm dependencies listed in package.json but never imported.
    pub unused_dependencies: Vec<UnusedDepIssue>,
    /// Import specifiers that could not be resolved to actual files.
    pub unresolved_imports: Vec<UnresolvedImportIssue>,
    /// External imports not listed in package.json.
    pub unlisted_dependencies: Vec<UnlistedDepIssue>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeadCodeIssue {
    pub path: String,
    pub lines_of_code: usize,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UnusedExportIssue {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DuplicateExportIssue {
    pub name: String,
    pub locations: Vec<String>,
}

/// A pair of code blocks that are similar (informational report).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DuplicateCodeIssue {
    /// Confidence score 0.0–1.0 indicating how similar the blocks are.
    pub confidence: f64,
    /// First occurrence.
    pub location_a: CodeBlockLocation,
    /// Second occurrence.
    pub location_b: CodeBlockLocation,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodeBlockLocation {
    pub file: String,
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub snippet: String,
}

/// A common error-prone pattern (gotcha).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GotchaIssue {
    pub file: String,
    pub line: usize,
    pub rule: String,
    pub severity: String,
    pub message: String,
    pub confidence: f64,
    pub snippet: String,
}

/// An exported TypeScript type or interface that is never imported.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UnusedTypeIssue {
    pub name: String,
    pub path: String,
    pub kind: String,
}

/// A circular import dependency chain.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CircularDepIssue {
    pub files: Vec<String>,
}

/// An npm dependency listed in package.json but never imported.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UnusedDepIssue {
    pub package_name: String,
    pub location: String,
}

/// An import specifier that could not be resolved to an actual file.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UnresolvedImportIssue {
    pub source_file: String,
    pub import_spec: String,
}

/// An external import not listed in package.json.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UnlistedDepIssue {
    pub package_name: String,
    pub imported_by: String,
}

// ---------------------------------------------------------------------------
// Duplication
// ---------------------------------------------------------------------------

/// Top-level duplication section in analysis output.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DuplicationSection {
    pub stats: DuplicationStats,
    pub clone_groups: Vec<CloneGroup>,
    pub clone_families: Vec<CloneFamily>,
    pub mirrored_directories: Vec<MirroredDirectory>,
}

/// A clone group: one duplicated code block found at 2+ locations.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CloneGroup {
    /// Instances of this duplicated code.
    pub instances: Vec<CloneInstance>,
    /// Token count of the duplicated code (heuristic: line_count * 6).
    pub token_count: usize,
    /// Line count of the duplicated code.
    pub line_count: usize,
}

/// One instance of a clone group.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CloneInstance {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub snippet: String,
}

/// A clone family: multiple clone groups involving the same set of files.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CloneFamily {
    /// Files involved in this family.
    pub files: Vec<String>,
    /// Number of clone groups in this family.
    pub group_count: usize,
    /// Total duplicated lines across all groups.
    pub total_duplicated_lines: usize,
}

/// Two directories that mirror each other (significant file overlap).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MirroredDirectory {
    pub dir_a: String,
    pub dir_b: String,
    pub shared_files: Vec<String>,
    pub total_lines: usize,
}

/// Duplication statistics.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DuplicationStats {
    /// Total source lines across all analyzed files.
    pub total_lines: usize,
    /// Lines that appear in at least one clone group.
    pub duplicated_lines: usize,
    /// Percentage of total lines that are duplicated (0-100).
    pub duplication_percentage: f64,
    /// Number of clone groups.
    pub clone_groups: usize,
    /// Number of clone instances.
    pub clone_instances: usize,
    /// Number of clone families.
    pub clone_families: usize,
}

/// Monorepo/workspace information detected during analysis.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MonorepoInfoData {
    /// The monorepo tool name (pnpm, npm/yarn, nx, turborepo).
    pub kind: String,
    /// Root-relative paths to workspace package directories.
    pub packages: Vec<String>,
}
