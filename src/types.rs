use serde::Serialize;
use std::path::PathBuf;

/// Top-level JSON output for `statico analyze`.
#[derive(Debug, Serialize)]
pub struct AnalysisOutput {
    pub structure: Structure,
    pub dependencies: Dependencies,
    pub quality: Quality,
}

#[derive(Debug, Serialize)]
pub struct Structure {
    pub root: PathBuf,
    pub entry_points: Vec<String>,
    pub source_files: Vec<SourceFile>,
    pub config_files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SourceFile {
    pub path: String,
    pub language: String,
}

#[derive(Debug, Serialize)]
pub struct Dependencies {
    pub imports: Vec<FileImports>,
    pub external: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct FileImports {
    pub source: String,
    pub targets: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Quality {
    pub files: Vec<FileQuality>,
}

#[derive(Debug, Serialize)]
pub struct FileQuality {
    pub path: String,
    pub metrics: Option<Metrics>,
    pub parse_errors: Vec<ParseError>,
}

#[derive(Debug, Serialize)]
pub struct Metrics {
    pub lines_of_code: usize,
    pub total_lines: usize,
    pub functions: usize,
    pub classes: usize,
    pub complexity: usize,
}

#[derive(Debug, Serialize)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}
