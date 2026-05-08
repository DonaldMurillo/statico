//! Rust language plugin for statico.
//!
//! Handles .rs files using tree-sitter-rust and the crate's Rust-specific
//! import/export resolution logic.

use std::path::Path;

use crate::languages::{FileAnalysis, LanguagePlugin};

/// Rust language plugin.
pub struct RustPlugin;

impl LanguagePlugin for RustPlugin {
    fn extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn name(&self) -> &str {
        "rust"
    }

    fn analyze_file(&self, root: &Path, rel_path: &str, source: &str) -> Option<FileAnalysis> {
        super::rust_parser::parse_rust_file_standalone(root, rel_path, source)
    }

    fn config_files(&self) -> &[&str] {
        &["Cargo.toml", "Cargo.lock"]
    }

    fn skip_dirs(&self) -> &[&str] {
        &["target"]
    }

    fn resolve_import(&self, root: &Path, from_file: &str, spec: &str) -> Option<String> {
        super::rust_parser::resolve_rust_use_path_public(root, from_file, spec)
    }
}
