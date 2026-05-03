//! Language plugin system for statico.
//!
//! Each language provides its own parsing, import/export extraction, resolution,
//! and gotcha rules through the `LanguagePlugin` trait. Adding a new language
//! means implementing this trait and registering it in `all_plugins()`.

pub mod rust;
pub mod typescript;

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use crate::parse::blocks::CodeBlock;
use crate::types::*;

// ---------------------------------------------------------------------------
// Core types shared across all languages
// ---------------------------------------------------------------------------

/// A parsed file's contribution to the analysis pipeline.
///
/// This is the unified output type that both TypeScript and Rust (and future
/// languages) produce. The analyzer consumes these uniformly.
pub struct FileAnalysis {
    pub rel_path: String,

    // -- Dependency graph --
    /// Resolved file-level dependencies (imports / use / mod).
    pub dep_targets: Vec<String>,
    /// External (unresolvable) package names (npm crates, cargo crates).
    pub external_specs: Vec<String>,
    /// Per-target imported names: (resolved_file, [name1, name2, ...]).
    /// Use "*" for glob imports / wildcard re-exports.
    pub imported_names: Vec<(String, Vec<String>)>,

    // -- Exports --
    /// Names this file exports.
    pub exports: Vec<String>,

    // -- Quality metrics --
    pub loc: usize,
    pub total_lines: usize,
    pub functions: usize,
    pub classes: usize,
    pub complexity: usize,
    pub max_nesting_depth: usize,
    pub parse_errors: Vec<ParseError>,

    // -- Code blocks (for duplication detection) --
    pub blocks: Vec<CodeBlock>,

    // -- Raw source (for gotcha detection and output) --
    pub source: String,
}

impl FileAnalysis {
    /// Build a `FileImports` (used in the output `Dependencies` section).
    pub fn to_file_imports(&self) -> FileImports {
        FileImports { source: self.rel_path.clone(), targets: self.dep_targets.clone() }
    }

    /// Build a `FileQuality` (used in the output `Quality` section).
    pub fn to_file_quality(&self) -> FileQuality {
        FileQuality {
            path: self.rel_path.clone(),
            metrics: Some(Metrics {
                lines_of_code: self.loc,
                total_lines: self.total_lines,
                functions: self.functions,
                classes: self.classes,
                complexity: self.complexity,
                max_nesting_depth: self.max_nesting_depth,
            }),
            exports: self.exports.clone(),
            parse_errors: self.parse_errors.clone(),
        }
    }
}

/// Aggregated results from parsing all files across all languages.
pub struct PipelineResults {
    pub dependencies: Dependencies,
    pub quality: Quality,
    pub dep_graph: BTreeMap<String, Vec<String>>,
    pub file_exports: BTreeMap<String, Vec<String>>,
    pub file_loc: BTreeMap<String, usize>,
    pub file_total_lines: BTreeMap<String, usize>,
    pub file_blocks: BTreeMap<String, Vec<CodeBlock>>,
    pub file_sources: Vec<(String, String)>,
    pub imported_names: BTreeMap<String, HashSet<String>>,
}

impl PipelineResults {
    /// Merge a single file's analysis into the aggregated results.
    pub fn merge(&mut self, fa: FileAnalysis) {
        for ext in &fa.external_specs {
            if !self.dependencies.external.contains(ext) {
                self.dependencies.external.push(ext.clone());
            }
        }
        let file_imports = fa.to_file_imports();
        let file_quality = fa.to_file_quality();
        self.dep_graph.insert(fa.rel_path.clone(), fa.dep_targets);
        self.file_exports.insert(fa.rel_path.clone(), fa.exports);
        self.file_loc.insert(fa.rel_path.clone(), fa.loc);
        self.file_total_lines.insert(fa.rel_path.clone(), fa.total_lines);
        self.file_blocks.insert(fa.rel_path.clone(), fa.blocks);
        self.dependencies.imports.push(file_imports);
        self.quality.files.push(file_quality);
        self.file_sources.push((fa.rel_path, fa.source));

        for (target_key, names) in fa.imported_names {
            self.imported_names.entry(target_key).or_default().extend(names);
        }
    }

    /// Create empty results.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sort all collections for deterministic output.
    ///
    /// This includes sorting each file's dep targets so the dep graph itself
    /// is deterministic — without this, parallel parsing via rayon plus
    /// non-deterministic resolver ordering could yield different cycle
    /// reports on different runs (an SCC has many possible representative
    /// cycles, and the DFS picks whichever back-edge it sees first).
    pub fn sort(&mut self) {
        self.dependencies.imports.sort_by(|a, b| a.source.cmp(&b.source));
        for fi in &mut self.dependencies.imports {
            fi.targets.sort();
        }
        self.dependencies.external.sort();
        self.quality.files.sort_by(|a, b| a.path.cmp(&b.path));
        self.file_sources.sort_by(|a, b| a.0.cmp(&b.0));
        for targets in self.dep_graph.values_mut() {
            targets.sort();
        }
    }
}

impl Default for PipelineResults {
    fn default() -> Self {
        Self {
            dependencies: Dependencies { imports: vec![], external: vec![] },
            quality: Quality { files: vec![] },
            dep_graph: BTreeMap::new(),
            file_exports: BTreeMap::new(),
            file_loc: BTreeMap::new(),
            file_total_lines: BTreeMap::new(),
            file_blocks: BTreeMap::new(),
            file_sources: vec![],
            imported_names: BTreeMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// LanguagePlugin trait
// ---------------------------------------------------------------------------

/// A language plugin provides parsing and analysis for one programming language.
///
/// Implement this trait to add support for a new language. Register your plugin
/// in `all_plugins()` below.
pub trait LanguagePlugin: Send + Sync {
    /// File extensions this plugin handles, without the dot (e.g., ["ts", "tsx"]).
    fn extensions(&self) -> &[&str];

    /// Human-readable language name (e.g., "typescript", "rust").
    fn name(&self) -> &str;

    /// Parse a single source file and extract all analysis data.
    ///
    /// This is the main entry point: given the root directory, the file's
    /// relative path, and the file's source code, return a `FileAnalysis`.
    ///
    /// Returns `None` if the file cannot be parsed.
    fn analyze_file(&self, root: &Path, rel_path: &str, source: &str) -> Option<FileAnalysis>;

    /// Language-specific config files to discover (e.g., "tsconfig.json", "Cargo.toml").
    fn config_files(&self) -> &[&str] {
        &[]
    }

    /// Directories to skip during traversal (e.g., "node_modules", "target").
    fn skip_dirs(&self) -> &[&str] {
        &[]
    }

    /// Whether to skip a specific file pattern.
    /// Return true to exclude this file from analysis.
    fn should_skip_file(&self, _rel_path: &str) -> bool {
        false
    }

    /// Resolve an import specifier to a file path relative to the project root.
    ///
    /// Each language has its own resolution strategy:
    /// - TypeScript: tsconfig path aliases, relative imports, extension probing
    /// - Rust: mod.rs resolution, Cargo.toml crate discovery
    ///
    /// Returns the resolved file path relative to `root`, or `None` if unresolvable.
    fn resolve_import(&self, _root: &Path, _from_file: &str, _spec: &str) -> Option<String> {
        None
    }
}

// ---------------------------------------------------------------------------
// Plugin registry
// ---------------------------------------------------------------------------

/// Return all registered language plugins.
///
/// To add a new language:
/// 1. Create `src/languages/my_lang.rs`
/// 2. Implement `LanguagePlugin`
/// 3. Add `Box::new(my_lang::MyLangPlugin {})` here
pub fn all_plugins() -> Vec<Box<dyn LanguagePlugin>> {
    vec![Box::new(typescript::TypeScriptPlugin::new()), Box::new(rust::RustPlugin {})]
}

/// Find the plugin that handles a given file extension.
pub fn plugin_for_extension(ext: &str) -> Option<&'static dyn LanguagePlugin> {
    // We use a static list since plugins are compile-time
    static PLUGINS: std::sync::OnceLock<Vec<Box<dyn LanguagePlugin>>> = std::sync::OnceLock::new();
    let plugins = PLUGINS.get_or_init(all_plugins);
    plugins.iter().find(|p| p.extensions().contains(&ext)).map(|p| p.as_ref())
}

/// Find the plugin that handles a given language name.
pub fn plugin_for_language(lang: &str) -> Option<&'static dyn LanguagePlugin> {
    static PLUGINS: std::sync::OnceLock<Vec<Box<dyn LanguagePlugin>>> = std::sync::OnceLock::new();
    let plugins = PLUGINS.get_or_init(all_plugins);
    plugins.iter().find(|p| p.name() == lang).map(|p| p.as_ref())
}
