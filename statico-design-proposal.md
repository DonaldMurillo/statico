# Statico Language-Agnostic Plugin Architecture

**Author:** Architecture Review  
**Date:** 2026-04-30  
**Status:** Draft Proposal  

---

## Executive Summary

Statico was built as a TypeScript static analyzer and is gaining ad-hoc Rust support via `parse/rust.rs` and `analyzer/parse_rust.rs`. The current architecture has parsing, import/export extraction, gotcha detection, and module resolution hardcoded to TypeScript/JavaScript semantics, with Rust bolted on through `if is_rust { ... }` branches.

This proposal introduces a **trait-based plugin architecture** where each language is a self-contained plugin implementing a `Language` trait, and each issue detector is a `Rule` that declares which languages it applies to. The existing pipeline (discover → parse → resolve → analyze → report) becomes language-agnostic orchestration that delegates to per-language implementations.

**Design influences:** ESLint v9 `Language` interface, Semgrep's generic AST, Clippy's `LintPass` trait, Ruff's rule-as-struct pattern.

---

## Table of Contents

1. [Problem Analysis](#1-problem-analysis)
2. [Core Language Trait](#2-core-language-trait)
3. [Rule Trait](#3-rule-trait)
4. [Pipeline Redesign](#4-pipeline-redesign)
5. [Adding Python — A Concrete Example](#5-adding-python--a-concrete-example)
6. [Migration Strategy](#6-migration-strategy)
7. [File Layout](#7-file-layout)
8. [Open Questions](#8-open-questions)

---

## 1. Problem Analysis

### Current Pain Points (with code references)

| # | Problem | Location | Impact |
|---|---------|----------|--------|
| 1 | `AstParser` hardcoded to `tree-sitter-typescript` | `parse/mod.rs:14-18` | Can't parse other languages |
| 2 | Import extraction assumes `import_statement` / `export_statement` node types | `parse/imports.rs:16-20`, `parse/exports.rs:14` | Breaks on Rust `use` / `pub` |
| 3 | Gotcha rules like `loose-equality` flag Rust `==` as bugs | `issues/gotchas/patterns.rs:22-36` | False positives on Rust files |
| 4 | Resolution assumes `./relative`, `@/alias`, `package.json` | `resolution/mod.rs` entire file | No Cargo module resolution |
| 5 | Issue detectors re-create `AstParser` assuming TS | `issues/mod.rs:71,94` | Crashes or wrong results for `.rs` |
| 6 | `parse_typescript.rs` has `if is_rust { return parse_rust(...) }` | `analyzer/parse_typescript.rs:55-57` | Leaky abstraction |

### What the Rust Support Looks Like Now

Rust was added by creating parallel files (`parse/rust.rs`, `discovery/rust.rs`, `analyzer/parse_rust.rs`) and then branching inside `parse_single_file()`:

```rust
// analyzer/parse_typescript.rs:55
if is_rust {
    return super::parse_rust::parse_rust_file(root, rel_path, &abs_path, &source, resolver);
}
```

This approach doesn't scale. Adding Go, Python, or Java means adding more `if` branches and more parallel files that all need to be wired into the orchestrator.

---

## 2. Core Language Trait

### 2.1 Design Principles

1. **Self-contained** — A language plugin knows how to parse, extract imports/exports, resolve modules, and describe its file types. No external code needs to understand its internals.
2. **Unified intermediate representation** — All languages produce the same output shapes (`ParsedFile`, `ImportSpec`, `ExportDecl`) so the rest of the pipeline stays language-agnostic.
3. **Optional AST** — Tree-sitter is the default but not required. A language plugin could use regex, libsyntax, or any parser.
4. **Composable resolution** — Module resolution is a trait method the language provides, not something the core assumes.

### 2.2 Core Types

```rust
// src/language/types.rs

/// A normalized import, language-agnostic.
/// Every language plugin translates its import syntax into this shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSpec {
    /// The raw specifier as written in source (e.g., "./utils", "std::collections", "./foo.py").
    pub raw: String,
    /// The specific names imported (e.g., ["HashMap", "BTreeMap"]).
    /// Empty for glob/star imports.
    pub names: Vec<String>,
    /// Whether this is a glob/star/wildcard import.
    pub is_glob: bool,
    /// Whether the plugin classifies this as internal (project-local) vs external.
    pub is_internal: bool,
}

/// A normalized export, language-agnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportDecl {
    /// The exported name (e.g., "MyComponent", "MAX_SIZE").
    pub name: String,
    /// What kind of thing this is.
    pub kind: ExportKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportKind {
    Function,
    Class,
    Struct,
    Enum,
    Trait,
    Const,
    Type,
    Module,
    Variable,
    Default,
    Other(String),
}

/// A normalized module dependency edge.
#[derive(Debug, Clone)]
pub struct ResolvedDep {
    /// Relative path from project root to the source file.
    pub from: String,
    /// Relative path from project root to the target file.
    pub to: String,
    /// Which names are imported from `to` (empty = unknown/glob).
    pub imported_names: Vec<String>,
}

/// The complete result of parsing a single file.
/// This is the unified output that all language plugins produce.
#[derive(Debug, Clone)]
pub struct ParsedFile {
    /// Relative path from project root.
    pub path: String,
    /// Language that parsed this file.
    pub language: String,
    /// Raw source text (kept for gotcha detection and code blocks).
    pub source: String,
    /// All imports found in this file.
    pub imports: Vec<ImportSpec>,
    /// All exports found in this file.
    pub exports: Vec<ExportDecl>,
    /// Resolved dependencies (after module resolution).
    pub resolved_deps: Vec<ResolvedDep>,
    /// External package names imported (e.g., "serde", "lodash").
    pub external_imports: Vec<String>,
    /// Code metrics.
    pub metrics: FileMetrics,
    /// Code blocks for duplication detection.
    pub blocks: Vec<CodeBlock>,
    /// Any parse errors encountered.
    pub parse_errors: Vec<ParseError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetrics {
    pub lines_of_code: usize,
    pub total_lines: usize,
    pub functions: usize,
    pub classes: usize,
    pub complexity: usize,
    pub max_nesting_depth: usize,
}

/// Minimal code block for duplication detection.
#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub snippet: String,
    pub kind: String, // "function", "method", "class_body", etc.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}
```

### 2.3 The `Language` Trait

```rust
// src/language/trait.rs

use std::path::{Path, PathBuf};

use super::types::*;

/// A language plugin for statico.
///
/// Each language (TypeScript, Rust, Python, Go, ...) implements this trait
/// and registers itself via the language registry.
///
/// Implementors are typically zero-sized or hold only parser configuration.
/// They must be `Send + Sync` because the pipeline uses rayon.
pub trait Language: Send + Sync + 'static {
    // -----------------------------------------------------------------------
    // Identity
    // -----------------------------------------------------------------------

    /// Unique identifier for this language (e.g., "typescript", "rust", "python").
    fn id(&self) -> &str;

    /// Human-readable display name (e.g., "TypeScript", "Rust").
    fn display_name(&self) -> &str;

    /// File extensions this language handles, without the dot (e.g., ["ts", "tsx"]).
    fn extensions(&self) -> &[&str];

    // -----------------------------------------------------------------------
    // Parsing
    // -----------------------------------------------------------------------

    /// Parse a source file and extract all structural information.
    ///
    /// This is the main entry point. It should:
    /// 1. Parse the source code (tree-sitter, regex, whatever)
    /// 2. Extract imports and exports
    /// 3. Compute metrics (LOC, complexity, nesting)
    /// 4. Extract code blocks for duplication detection
    /// 5. Collect any parse errors
    ///
    /// The `path` is relative to the project root.
    /// Module resolution happens separately via `resolve_import()`.
    fn parse_file(&self, path: &str, source: &str) -> ParseFileResult;

    // -----------------------------------------------------------------------
    // Module Resolution
    // -----------------------------------------------------------------------

    /// Resolve a raw import specifier to a file path.
    ///
    /// - `from_dir`: absolute path to the directory containing the importing file
    /// - `spec`: the raw import specifier (e.g., "./utils", "std::collections", "@angular/core")
    /// - `project_root`: absolute path to the project root
    ///
    /// Returns `Some(resolved_relative_path)` if the import can be resolved to a
    /// project-local file, or `None` if it's external/unresolvable.
    fn resolve_import(
        &self,
        from_dir: &Path,
        spec: &str,
        project_root: &Path,
    ) -> Option<String>;

    /// Whether this import specifier is internal (project-local) or external.
    fn is_internal_import(&self, spec: &str) -> bool;

    /// Extract the package name from an external import specifier.
    fn extract_package_name(&self, spec: &str) -> String;

    // -----------------------------------------------------------------------
    // Entry Point Discovery
    // -----------------------------------------------------------------------

    /// Discover entry points for this language in the given project.
    fn discover_entry_points(
        &self,
        root: &Path,
        source_files: &[(String, String)],
    ) -> EntryPoints;

    // -----------------------------------------------------------------------
    // Dependency Manifest
    // -----------------------------------------------------------------------

    /// Parse the dependency manifest for this language (e.g., package.json,
    /// Cargo.toml, go.mod, requirements.txt).
    fn declared_dependencies(&self, root: &Path) -> Vec<String>;

    // -----------------------------------------------------------------------
    // Configuration
    // -----------------------------------------------------------------------

    /// Additional directories to skip during file discovery (beyond the defaults).
    fn skip_dirs(&self) -> &[&str] { &[] }

    /// Files/patterns that indicate this language is used in the project.
    fn marker_files(&self) -> &[&str] { &[] }
}

/// Result of parsing a file -- imports and exports are raw (pre-resolution).
pub struct ParseFileResult {
    pub imports: Vec<ImportSpec>,
    pub exports: Vec<ExportDecl>,
    pub metrics: FileMetrics,
    pub blocks: Vec<CodeBlock>,
    pub parse_errors: Vec<ParseError>,
}

/// Entry points discovered for a language.
pub struct EntryPoints {
    pub framework: Vec<String>,
    pub implicit: Vec<String>,
}
```

### 2.4 Language Registry

```rust
// src/language/registry.rs

use std::sync::Arc;

use super::trait_defs::Language;

/// Registry of all available language plugins.
pub struct LanguageRegistry {
    languages: Vec<Arc<dyn Language>>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        let mut registry = Self { languages: Vec::new() };
        registry.register(crate::languages::typescript::TypeScriptLanguage::new());
        registry.register(crate::languages::rust::RustLanguage::new());
        registry
    }

    pub fn register(&mut self, lang: impl Language) {
        self.languages.push(Arc::new(lang));
    }

    /// Find the language plugin for a file based on its extension.
    pub fn for_file(&self, path: &str) -> Option<Arc<dyn Language>> {
        let ext = path.rsplit('.').next()?;
        self.languages.iter().find(|lang| {
            lang.extensions().iter().any(|e| *e == ext)
        }).cloned()
    }

    /// Get all registered languages.
    pub fn all(&self) -> &[Arc<dyn Language>] { &self.languages }

    /// Get all file extensions across all languages.
    pub fn all_extensions(&self) -> Vec<&str> {
        self.languages.iter().flat_map(|l| l.extensions().iter().copied()).collect()
    }

    /// Get a specific language by ID.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Language>> {
        self.languages.iter().find(|l| l.id() == id).cloned()
    }
}
```

### 2.5 TypeScript Plugin (Sketch)

Here's what the TypeScript language plugin would look like -- essentially extracting existing code:

```rust
// src/languages/typescript/mod.rs

use std::path::Path;

use crate::language::trait_defs::*;
use crate::language::types::*;

pub struct TypeScriptLanguage;

impl TypeScriptLanguage {
    pub fn new() -> Self { Self }
}

impl Language for TypeScriptLanguage {
    fn id(&self) -> &str { "typescript" }
    fn display_name(&self) -> &str { "TypeScript" }
    fn extensions(&self) -> &[&str] { &["ts", "tsx", "js", "jsx"] }

    fn parse_file(&self, path: &str, source: &str) -> ParseFileResult {
        let parser = crate::parse::AstParser::new().expect("parser init");
        let is_tsx = path.ends_with(".tsx") || path.ends_with(".jsx");
        let result = match parser.parse(source, is_tsx) {
            Some(r) => r,
            None => return ParseFileResult::error("parse failed"),
        };
        let root = result.tree.root_node();

        let (internal, external) = crate::parse::imports::extract_imports(root, source);
        let raw_named = crate::parse::imports::extract_named_imports(root, source);

        let mut imports: Vec<ImportSpec> = Vec::new();
        for spec in internal.iter().chain(external.iter()) {
            let names = raw_named.get(spec).cloned().unwrap_or_default();
            let is_internal = spec.starts_with('.')
                || spec.starts_with('/') || spec.starts_with("@/")
                || spec.starts_with('~') || spec.starts_with('#');
            imports.push(ImportSpec {
                raw: spec.clone(), names, is_glob: false, is_internal,
            });
        }

        let ts_exports = crate::parse::exports::extract_exports(root, source);
        let exports: Vec<ExportDecl> = ts_exports.into_iter().map(|name| ExportDecl {
            name, kind: ExportKind::Variable,
        }).collect();

        let (loc, total) = crate::parse::metrics::count_loc(source);
        let funcs = crate::parse::metrics::count_functions(root);
        let classes = crate::parse::metrics::count_classes(root);
        let cx = crate::parse::complexity::compute_metrics(root, source.as_bytes());
        let blocks = crate::parse::blocks::extract_blocks(root, source.as_bytes());
        let parse_errors = if result.has_errors {
            crate::parse::errors::collect_errors(root, source.as_bytes())
                .into_iter().map(|(msg, line, col)| ParseError { message: msg, line, column: col })
                .collect()
        } else { vec![] };

        ParseFileResult { imports, exports, metrics: FileMetrics {
            lines_of_code: loc, total_lines: total, functions: funcs,
            classes, complexity: cx.complexity, max_nesting_depth: cx.max_nesting_depth,
        }, blocks, parse_errors }
    }

    fn resolve_import(&self, from_dir: &Path, spec: &str, root: &Path) -> Option<String> {
        // Delegate to the existing Resolver (would be injected/configured)
        let resolver = crate::resolution::Resolver::new(root);
        resolver.resolve(from_dir, spec)
            .map(|p| crate::resolution::path_relative_to(root, &p))
    }

    fn is_internal_import(&self, spec: &str) -> bool {
        spec.starts_with('.') || spec.starts_with('/') || spec.starts_with("@/")
            || spec.starts_with('~') || spec.starts_with('#')
    }

    fn extract_package_name(&self, spec: &str) -> String {
        crate::parse::imports::extract_package_name(spec)
    }

    fn discover_entry_points(&self, root: &Path, source_files: &[(String, String)]) -> EntryPoints {
        let entry = crate::discovery::discover_entry_points(root, source_files);
        EntryPoints {
            framework: entry.framework.iter().cloned().collect(),
            implicit: entry.implicit.iter().cloned().collect(),
        }
    }

    fn declared_dependencies(&self, root: &Path) -> Vec<String> {
        let pkg_path = root.join("package.json");
        if !pkg_path.exists() { return vec![]; }
        // ... parse package.json dependencies + devDependencies
        vec![]
    }

    fn skip_dirs(&self) -> &[&str] { &["node_modules", ".next", ".nuxt"] }
    fn marker_files(&self) -> &[&str] { &["tsconfig.json", "package.json"] }
}
```

### 2.6 Rust Plugin (Sketch)

```rust
// src/languages/rust/mod.rs

pub struct RustLanguage;

impl Language for RustLanguage {
    fn id(&self) -> &str { "rust" }
    fn display_name(&self) -> &str { "Rust" }
    fn extensions(&self) -> &[&str] { &["rs"] }

    fn parse_file(&self, path: &str, source: &str) -> ParseFileResult {
        let parser = crate::parse::rust::RustAstParser::new().ok()?;
        let result = parser.parse(source)?;
        // ... translate RustExport/RustImport to ImportSpec/ExportDecl
    }

    fn resolve_import(&self, from_dir: &Path, spec: &str, root: &Path) -> Option<String> {
        // Use the crate::analyzer::parse_rust::resolve_rust_use_path logic
    }

    fn is_internal_import(&self, spec: &str) -> bool {
        spec.starts_with("crate::") || spec.starts_with("super::") || spec.starts_with("self::")
    }

    fn extract_package_name(&self, spec: &str) -> String {
        spec.split("::").next().unwrap_or(spec).to_string()
    }

    fn discover_entry_points(&self, root: &Path, source_files: &[(String, String)]) -> EntryPoints {
        // Use crate::discovery::rust::add_rust_crate_entries logic
    }

    fn declared_dependencies(&self, root: &Path) -> Vec<String> {
        // Parse Cargo.toml [dependencies]
    }

    fn skip_dirs(&self) -> &[&str] { &["target"] }
    fn marker_files(&self) -> &[&str] { &["Cargo.toml"] }
}
```

---

## 3. Rule Trait

### 3.1 Design

Rules are the replacement for the current `issues/*` modules. Each rule:

1. Declares which **languages** it applies to (e.g., loose-equality is TS-only)
2. Declares a **category** (correctness, style, complexity, security)
3. Implements a `check()` method that receives a `RuleContext` and returns issues
4. Can optionally be language-specific or cross-language

### 3.2 Rule Trait

```rust
// src/rule/trait.rs

use crate::language::types::*;

/// Categories for rules, following Clippy's convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleCategory {
    Correctness,   // Incorrect code (bugs)
    Style,         // Code style issues
    Complexity,    // Overly complex code
    Performance,   // Performance issues
    Security,      // Security vulnerabilities
    DeadCode,      // Dead code, unused exports
    Dependencies,  // Dependency issues
}

/// Severity of a rule violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

/// A rule that detects issues in source code.
///
/// Rules can be:
/// - **Cross-language**: Apply to all files (e.g., deep nesting, high complexity)
/// - **Language-specific**: Only apply to files of certain languages
/// - **Language-aware**: Apply to multiple languages but with different behavior
pub trait Rule: Send + Sync + 'static {
    /// Unique identifier (e.g., "loose-equality", "unused-exports").
    fn id(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// Which category this rule belongs to.
    fn category(&self) -> RuleCategory;

    /// Which languages this rule applies to.
    /// Returns None to apply to ALL languages.
    fn languages(&self) -> Option<&[&str]> { None }

    /// Default severity for issues from this rule.
    fn default_severity(&self) -> Severity { Severity::Warning }

    /// Check a single file for issues.
    fn check(&self, context: &RuleContext) -> Vec<Issue>;
}

/// Context provided to rules for checking a file.
pub struct RuleContext<'a> {
    /// The parsed file with all extracted information.
    pub file: &'a ParsedFile,
    /// The language plugin that parsed this file.
    pub language: &'a dyn crate::language::trait_defs::Language,
    /// The project root (absolute path).
    pub project_root: &'a Path,
    /// All entry points in the project.
    pub entry_points: &'a [String],
    /// The dependency graph (file -> files it imports).
    pub dep_graph: &'a BTreeMap<String, Vec<String>>,
    /// All file exports (file -> exported names).
    pub file_exports: &'a BTreeMap<String, Vec<String>>,
    /// All imported names per file (file -> set of names imported from it).
    pub imported_names: &'a BTreeMap<String, HashSet<String>>,
}

/// A detected issue, language-agnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub rule_id: String,
    pub file: String,
    pub line: usize,
    pub column: Option<usize>,
    pub severity: Severity,
    pub message: String,
    pub confidence: f64,
    pub snippet: String,
    pub suggestion: Option<FixSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixSuggestion {
    pub description: String,
    pub replacement: Option<String>,
}
```

### 3.3 Rule Registry

```rust
// src/rule/registry.rs

use super::trait_defs::Rule;

pub struct RuleRegistry {
    rules: Vec<Box<dyn Rule>>,
}

impl RuleRegistry {
    pub fn new() -> Self {
        let mut registry = Self { rules: Vec::new() };
        // Register built-in rules
        registry.register(crate::rules::dead_code::DeadCodeRule::new());
        registry.register(crate::rules::unused_exports::UnusedExportsRule::new());
        registry.register(crate::rules::circular_deps::CircularDepsRule::new());
        registry.register(crate::rules::gotchas::ts::LooseEqualityRule::new());
        registry.register(crate::rules::gotchas::ts::AnyTypeRule::new());
        registry.register(crate::rules::gotchas::ts::EvalUsageRule::new());
        // Cross-language rules
        registry.register(crate::rules::deep_nesting::DeepNestingRule::new());
        registry.register(crate::rules::high_complexity::HighComplexityRule::new());
        registry.register(crate::rules::todo_comments::TodoCommentRule::new());
        registry
    }

    pub fn register(&mut self, rule: impl Rule) {
        self.rules.push(Box::new(rule));
    }

    /// Get all rules applicable to a given language.
    pub fn for_language(&self, language_id: &str) -> Vec<&dyn Rule> {
        self.rules.iter()
            .filter(|r| match r.languages() {
                None => true,
                Some(langs) => langs.contains(&language_id),
            })
            .map(|r| r.as_ref())
            .collect()
    }

    pub fn all(&self) -> &[Box<dyn Rule>] { &self.rules }
}
```

### 3.4 Rule Examples

**Language-specific rule (TypeScript only):**
```rust
// src/rules/gotchas/ts/loose_equality.rs

pub struct LooseEqualityRule;

impl Rule for LooseEqualityRule {
    fn id(&self) -> &str { "loose-equality" }
    fn description(&self) -> &str { "Use `===` instead of `==` to avoid type coercion bugs" }
    fn category(&self) -> RuleCategory { RuleCategory::Style }
    fn default_severity(&self) -> Severity { Severity::Info }
    fn languages(&self) -> Option<&[&str]> { Some(&["typescript"]) }

    fn check(&self, ctx: &RuleContext) -> Vec<Issue> {
        let mut issues = Vec::new();
        for (i, line) in ctx.file.source.lines().enumerate() {
            let line_num = i + 1;
            if is_comment_line(line) { continue; }
            if has_bare_eq(line) {
                issues.push(Issue {
                    rule_id: self.id().into(),
                    file: ctx.file.path.clone(),
                    line: line_num, column: None,
                    severity: self.default_severity(),
                    message: self.description().into(),
                    confidence: 0.4,
                    snippet: truncate_line(line),
                    suggestion: Some(FixSuggestion {
                        description: "Replace `==` with `===`".into(),
                        replacement: None,
                    }),
                });
            }
        }
        issues
    }
}
```

**Cross-language rule:**
```rust
// src/rules/deep_nesting.rs

pub struct DeepNestingRule;

impl Rule for DeepNestingRule {
    fn id(&self) -> &str { "deep-nesting" }
    fn description(&self) -> &str { "Code is deeply nested (>=5 levels)" }
    fn category(&self) -> RuleCategory { RuleCategory::Complexity }
    fn default_severity(&self) -> Severity { Severity::Info }
    // languages() returns None -> applies to ALL languages

    fn check(&self, ctx: &RuleContext) -> Vec<Issue> {
        if ctx.file.metrics.max_nesting_depth >= 5 {
            vec![Issue {
                rule_id: self.id().into(),
                file: ctx.file.path.clone(),
                line: 0, column: None,
                severity: self.default_severity(),
                message: format!("Nesting depth of {} exceeds recommended maximum of 4",
                    ctx.file.metrics.max_nesting_depth),
                confidence: 0.6,
                snippet: String::new(),
                suggestion: None,
            }]
        } else { vec![] }
    }
}
```

**Graph-level (project-scope) rule:**
```rust
// src/rules/dead_code.rs

pub struct DeadCodeRule;

impl Rule for DeadCodeRule {
    fn id(&self) -> &str { "dead-code" }
    fn description(&self) -> &str { "File not reachable from any entry point" }
    fn category(&self) -> RuleCategory { RuleCategory::DeadCode }
    fn default_severity(&self) -> Severity { Severity::Warning }
    // languages() returns None -> works for all languages

    fn check(&self, _ctx: &RuleContext) -> Vec<Issue> {
        // Per-file check is empty; see check_project() below
        vec![]
    }

    /// Project-level check -- called once per analysis, not per file.
    fn check_project(&self, project_ctx: &ProjectContext) -> Vec<Issue> {
        let reachable = bfs_reachable(&project_ctx.entry_points, &project_ctx.dep_graph);
        project_ctx.dep_graph.keys()
            .filter(|path| !reachable.contains(*path))
            .map(|path| Issue {
                rule_id: self.id().into(),
                file: path.clone(),
                line: 0, column: None,
                severity: self.default_severity(),
                message: "not reachable from any entry point".into(),
                confidence: 0.95,
                snippet: String::new(),
                suggestion: None,
            })
            .collect()
    }
}
```

---

## 4. Pipeline Redesign

### 4.1 Current Pipeline (Simplified)

```
discover_source_files()
    |
AstParser::new()  <-- hardcoded to tree-sitter-typescript
    |
for each file:
    if is_rust -> parse_rust_file()     <-- branch
    else -> parse TS, extract_imports, extract_exports
    |
Resolver (tsconfig + workspace packages)
    |
detect_issues(IssueContext)  <-- monolithic, runs all detectors
    |
format output
```

### 4.2 New Pipeline

```
+-------------------------------------------------------------------+
|  LanguageRegistry::new() + RuleRegistry::new()                     |
+-------------------------------------------------------------------+
                              |
+-------------------------------------------------------------------+
|  Phase 1: Discover                                                 |
|                                                                    |
|  registry.all_extensions() -> walk directory                       |
|  For each file: registry.for_file(path) -> language               |
|  Collect: Vec<(rel_path, Arc<dyn Language>)>                      |
|                                                                    |
|  For each language: lang.discover_entry_points()                  |
|  Merge all entry points                                            |
+-------------------------------------------------------------------+
                              |
+-------------------------------------------------------------------+
|  Phase 2: Parse (parallel via rayon)                               |
|                                                                    |
|  For each file:                                                    |
|    language = registry.for_file(path)                              |
|    result = language.parse_file(path, source)                      |
|    Collect: Vec<ParsedFile>                                        |
+-------------------------------------------------------------------+
                              |
+-------------------------------------------------------------------+
|  Phase 3: Resolve                                                  |
|                                                                    |
|  For each file:                                                    |
|    language = registry.for_file(path)                              |
|    For each import:                                                |
|      resolved = language.resolve_import(from_dir, spec, root)      |
|    Build dep_graph: file -> [resolved files]                       |
|    Build imported_names: target_file -> [names]                    |
|    Classify external imports                                       |
+-------------------------------------------------------------------+
                              |
+-------------------------------------------------------------------+
|  Phase 4: Analyze (run rules)                                      |
|                                                                    |
|  Per-file rules (parallel):                                        |
|    For each file:                                                  |
|      applicable = rule_registry.for_language(lang.id())            |
|      For each rule:                                                |
|        issues.extend(rule.check(&RuleContext { file, ... }))       |
|                                                                    |
|  Project-level rules (sequential):                                 |
|    For each project-level rule:                                    |
|      issues.extend(rule.check_project(&ProjectContext { ... }))    |
+-------------------------------------------------------------------+
                              |
+-------------------------------------------------------------------+
|  Phase 5: Report                                                   |
|                                                                    |
|  Convert Vec<Issue> to existing Issues struct                     |
|  Apply output formatters (unchanged)                               |
+-------------------------------------------------------------------+
```

### 4.3 Orchestrator Implementation

```rust
// src/analyzer/orchestrator.rs

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use rayon::prelude::*;

use crate::language::registry::LanguageRegistry;
use crate::rule::registry::RuleRegistry;
use crate::rule::trait_defs::{Issue, Rule, RuleContext};
use crate::language::types::*;

pub struct AnalysisResult {
    pub files: Vec<ParsedFile>,
    pub dep_graph: BTreeMap<String, Vec<String>>,
    pub file_exports: BTreeMap<String, Vec<String>>,
    pub imported_names: BTreeMap<String, HashSet<String>>,
    pub external_imports: Vec<String>,
    pub issues: Vec<Issue>,
    pub entry_points: Vec<String>,
}

pub fn run_analysis(
    root: &Path,
    registry: &LanguageRegistry,
    rules: &RuleRegistry,
    exclude: &[String],
) -> Result<AnalysisResult, String> {
    // Phase 1: Discover
    let source_files = discover_all_files(root, registry, exclude)?;
    let entry_points = discover_all_entry_points(root, &source_files, registry);

    // Phase 2: Parse (parallel)
    let parsed: Vec<ParsedFile> = source_files
        .par_iter()
        .filter_map(|(path, lang)| {
            let abs = root.join(path);
            let source = std::fs::read_to_string(&abs).ok()?;
            let result = lang.parse_file(path, &source)?;
            Some(ParsedFile {
                path: path.clone(),
                language: lang.id().to_string(),
                source,
                ..result
            })
        })
        .collect();

    // Phase 3: Resolve
    let mut dep_graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut file_exports: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut imported_names: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    let mut external_imports: HashSet<String> = HashSet::new();

    for file in &parsed {
        let lang = registry.for_file(&file.path).expect("language for parsed file");
        let from_dir = root.join(&file.path).parent().unwrap_or(root).to_path_buf();

        file_exports.insert(file.path.clone(),
            file.exports.iter().map(|e| e.name.clone()).collect());

        let mut deps: Vec<String> = Vec::new();
        for imp in &file.imports {
            if imp.is_internal {
                if let Some(resolved) = lang.resolve_import(&from_dir, &imp.raw, root) {
                    if !deps.contains(&resolved) { deps.push(resolved.clone()); }
                    imported_names.entry(resolved).or_default()
                        .extend(imp.names.iter().cloned());
                }
            } else {
                let pkg = lang.extract_package_name(&imp.raw);
                external_imports.insert(pkg);
            }
        }
        deps.sort();
        dep_graph.insert(file.path.clone(), deps);
    }

    // Phase 4: Analyze (run rules)
    let all_entries: Vec<String> = entry_points.framework.iter()
        .chain(entry_points.implicit.iter()).cloned().collect();

    // Per-file rules (parallel)
    let per_file_issues: Vec<Issue> = parsed
        .par_iter()
        .flat_map(|file| {
            let applicable_rules = rules.for_language(&file.language);
            let lang = registry.for_file(&file.path).unwrap();
            let ctx = RuleContext {
                file, language: lang.as_ref(), project_root: root,
                entry_points: &all_entries, dep_graph: &dep_graph,
                file_exports: &file_exports, imported_names: &imported_names,
            };
            applicable_rules.iter()
                .flat_map(|rule| rule.check(&ctx))
                .collect::<Vec<_>>()
        })
        .collect();

    let mut issues = per_file_issues;
    // Project-level rules run here (dead_code, circular_deps, etc.)

    Ok(AnalysisResult {
        files: parsed, dep_graph, file_exports, imported_names,
        external_imports: external_imports.into_iter().collect(),
        issues, entry_points: all_entries,
    })
}
```

### 4.4 Key Changes from Current Code

| Current | New |
|---------|-----|
| `AstParser::new()` hardcoded to TS | `LanguageRegistry::for_file(path).parse_file()` |
| `if is_rust { ... }` branches in `parse_single_file` | Dispatch via trait object |
| `parse/imports.rs` knows about `import_statement` nodes | Each language extracts its own imports |
| `parse/exports.rs` knows about `export_statement` nodes | Each language extracts its own exports |
| `issues/gotchas/patterns.rs` runs on all files including `.rs` | Rules declare `languages() -> Some(&["typescript"])` |
| `resolution/mod.rs` is TS-specific | Each language has its own `resolve_import()` |
| `issues/mod.rs::detect_issues()` is monolithic | `RuleRegistry::for_language()` selects applicable rules |
| `IssueContext` is a god struct | `RuleContext` scoped per-file; `ProjectContext` for graph-level rules |
| `types.rs` has TS-specific comments | Language-agnostic types in `language/types.rs` |

### 4.5 Output Compatibility

The output formatters (`output/json.rs`, `output/markdown.rs`, etc.) currently consume `AnalysisOutput` from `types.rs`. To avoid breaking all formatters:

1. The new pipeline produces `AnalysisResult` (new shape)
2. A conversion function maps `AnalysisResult -> AnalysisOutput`
3. This keeps existing output formatters working unchanged
4. Later, formatters can migrate to consume `AnalysisResult` directly

---

## 5. Adding Python -- A Concrete Example

This section demonstrates what adding a new language looks like under the proposed architecture.

### Step 1: Add the tree-sitter dependency

```toml
# Cargo.toml
[dependencies]
tree-sitter-python = "0.23"
```

### Step 2: Create the language plugin

```rust
// src/languages/python/mod.rs

use std::path::Path;
use crate::language::trait_defs::*;
use crate::language::types::*;

pub struct PythonLanguage;

impl PythonLanguage {
    pub fn new() -> Self { Self }
}

impl Language for PythonLanguage {
    fn id(&self) -> &str { "python" }
    fn display_name(&self) -> &str { "Python" }
    fn extensions(&self) -> &[&str] { &["py", "pyi"] }

    fn parse_file(&self, path: &str, source: &str) -> ParseFileResult {
        use tree_sitter::Parser;
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
        parser.set_language(&lang).expect("python grammar");
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return ParseFileResult::error("parse failed"),
        };
        let root = tree.root_node();

        let imports = extract_python_imports(root, source);
        let exports = extract_python_exports(root, source);
        let (loc, total) = count_loc(source);
        let cx = compute_complexity(root, source.as_bytes());
        let blocks = extract_blocks(root, source.as_bytes());

        ParseFileResult { imports, exports, metrics: FileMetrics {
            lines_of_code: loc, total_lines: total,
            functions: count_top_level_defs(root),
            classes: count_classes(root),
            complexity: cx.complexity,
            max_nesting_depth: cx.max_nesting_depth,
        }, blocks, parse_errors: vec![] }
    }

    fn resolve_import(&self, from_dir: &Path, spec: &str, root: &Path) -> Option<String> {
        resolve_python_import(from_dir, spec, root)
    }

    fn is_internal_import(&self, spec: &str) -> bool { spec.starts_with('.') }

    fn extract_package_name(&self, spec: &str) -> String {
        spec.split('.').next().unwrap_or(spec).to_string()
    }

    fn discover_entry_points(&self, root: &Path, _source_files: &[(String, String)]) -> EntryPoints {
        let mut framework = Vec::new();
        let mut implicit = Vec::new();
        if root.join("manage.py").exists() { framework.push("manage.py".into()); }
        for name in &["setup.py", "conftest.py", "__main__.py"] {
            if root.join(name).exists() { implicit.push(name.to_string()); }
        }
        EntryPoints { framework, implicit }
    }

    fn declared_dependencies(&self, root: &Path) -> Vec<String> { parse_python_deps(root) }
    fn skip_dirs(&self) -> &[&str] { &["__pycache__", ".venv", "venv", ".tox"] }
    fn marker_files(&self) -> &[&str] { &["pyproject.toml", "setup.py", "requirements.txt"] }
}
```

### Step 3: Add Python-specific rules (optional)

```rust
// src/rules/gotchas/python/bare_except.rs

pub struct BareExceptRule;

impl Rule for BareExceptRule {
    fn id(&self) -> &str { "bare-except" }
    fn description(&self) -> &str {
        "Bare `except:` catches all exceptions including KeyboardInterrupt"
    }
    fn category(&self) -> RuleCategory { RuleCategory::Correctness }
    fn default_severity(&self) -> Severity { Severity::Warning }
    fn languages(&self) -> Option<&[&str]> { Some(&["python"]) }

    fn check(&self, ctx: &RuleContext) -> Vec<Issue> {
        // AST scan for `except:` without specific exception type
        vec![]
    }
}
```

### Step 4: Register

```rust
// src/language/registry.rs -- in LanguageRegistry::new()
registry.register(crate::languages::python::PythonLanguage::new());

// src/rule/registry.rs -- in RuleRegistry::new()
registry.register(crate::rules::gotchas::python::BareExceptRule::new());
```

### Step 5: That's it.

No changes to discovery, orchestrator, output formatters, or any existing TS/Rust code.

### Summary of what was touched:

| File | Change |
|------|--------|
| `Cargo.toml` | Add `tree-sitter-python` |
| `src/languages/python/mod.rs` | **NEW** -- Language trait impl |
| `src/rules/gotchas/python/bare_except.rs` | **NEW** -- Python-specific rule |
| `src/language/registry.rs` | 1 line -- `registry.register(...)` |
| `src/rule/registry.rs` | 1 line -- `registry.register(...)` |

**Total: 2 new files, 2 one-line changes to existing files.**

---

## 6. Migration Strategy

The migration must be **incremental** -- each step produces a working binary, and we never break existing functionality. The strategy follows the "strangler fig" pattern: build the new system alongside the old, gradually routing code through it.

### Phase 0: Preparation (1 day)

**Goal:** Set up module structure without changing behavior.

- Create `src/language/` directory with `types.rs`, `trait.rs`, `registry.rs`
- Create `src/rule/` directory with `trait.rs`, `registry.rs`
- Create `src/languages/` directory (empty, with mod.rs)
- Create `src/rules/` directory (empty, with mod.rs)
- Add `mod language; mod rule; mod languages; mod rules;` to `lib.rs`
- **No behavioral changes** -- everything compiles, tests pass

### Phase 1: Extract Language Types (2 days)

**Goal:** Define the unified types that all languages produce.

- Define `ParsedFile`, `ImportSpec`, `ExportDecl`, `FileMetrics`, `CodeBlock` in `src/language/types.rs`
- Create conversion function `to_legacy()` mapping new types to old `types.rs`
- **Tests:** Unit tests for type conversions, verify no behavioral change

### Phase 2: Define the Language Trait (1 day)

**Goal:** Define the `Language` trait.

- Write trait definition in `src/language/trait.rs`
- Write `LanguageRegistry` in `src/language/registry.rs`
- Write `RuleRegistry` in `src/rule/registry.rs`
- **Tests:** Verify registry creation, `for_file()` dispatch

### Phase 3: TypeScript + Rust Language Plugins (3 days)

**Goal:** Extract all TS- and Rust-specific code into language plugins.

This is the largest phase because most existing code is TS-specific.

1. **Create `src/languages/typescript/mod.rs`**
   - `parse_file()`: Wrap existing `AstParser::parse()` + imports + exports + metrics
   - `resolve_import()`: Wrap existing `resolution::Resolver::resolve()`
   - `is_internal_import()`: Move classification from `imports.rs`
   - `discover_entry_points()`: Wrap existing `discovery::discover_entry_points()`
   - `declared_dependencies()`: Extract from `unused_deps.rs`

2. **Create `src/languages/rust/mod.rs`**
   - `parse_file()`: Wrap existing `parse/rust.rs` functions
   - `resolve_import()`: Wrap existing `analyzer/parse_rust.rs::resolve_rust_use_path()`
   - `discover_entry_points()`: Wrap existing `discovery/rust.rs`

3. **Wire into orchestrator**
   - Create `src/analyzer/orchestrator.rs` with the new pipeline
   - Add a feature flag `--new-pipeline` that routes through the orchestrator
   - The old `analyzer::analyze()` remains the default
   - Run both pipelines on fixtures and compare output

4. **Tests:** Existing fixtures produce identical (or very similar) output

### Phase 4: Define the Rule Trait + Extract Rules (2 days)

**Goal:** Extract issue detectors into Rule implementations.

1. **Create per-file rules:**
   - `src/rules/gotchas/ts/loose_equality.rs` from `gotchas/patterns.rs`
   - `src/rules/gotchas/ts/any_type.rs`, `eval.rs`, `xss.rs`, `console.rs`, `env_no_fallback.rs`
   - `src/rules/deep_nesting.rs` -- cross-language
   - `src/rules/todo_comments.rs` -- cross-language

2. **Create project-level rules:**
   - `src/rules/dead_code.rs` from `issues/dead_code.rs`
   - `src/rules/unused_exports.rs` from `issues/unused_exports.rs`
   - `src/rules/circular_deps.rs` from `issues/circular_deps.rs`
   - `src/rules/duplicate_code.rs` from `issues/duplicate_code.rs`

3. **Add Rust-specific rules:**
   - `src/rules/gotchas/rust/unused_mut.rs`
   - `src/rules/gotchas/rust/unnecessary_return.rs`

4. Wire into orchestrator, compare output with old pipeline

### Phase 5: Switch Over (1 day)

**Goal:** Make the new pipeline the default.

1. Remove feature flag / old pipeline path
2. Remove old `analyzer/parse_typescript.rs`, `analyzer/parse_rust.rs`
3. Clean up `parse/imports.rs`, `parse/exports.rs` -- become internal to TS plugin
4. Clean up `issues/mod.rs` -- replaced by rule registry
5. Update `types.rs` -- remove TS-specific comments
6. Full regression test on all fixtures

### Phase 6: Cleanup (1 day)

1. Move `parse/rust.rs` into `languages/rust/`
2. Move `discovery/rust.rs` into `languages/rust/`
3. Move `resolution/` internals into relevant language plugins
4. Remove dead code
5. Update docs

### Migration Timeline

| Phase | Duration | Risk | Rollback Strategy |
|-------|----------|------|-------------------|
| 0: Preparation | 1 day | None | Just delete new dirs |
| 1: Extract types | 2 days | Very low | Delete new types |
| 2: Define traits | 1 day | Very low | Delete trait files |
| 3: TS + Rust plugins | 3 days | Medium | Feature flag keeps old pipeline |
| 4: Rule trait | 2 days | Medium | Both rule paths coexist |
| 5: Switch over | 1 day | Medium | Git revert |
| 6: Cleanup | 1 day | Low | Git revert |
| **Total** | **~11 days** | | |

---

## 7. File Layout

### New Directory Structure

```
src/
+-- language/                    # Language plugin infrastructure
|   +-- mod.rs                   # Re-exports
|   +-- types.rs                 # ImportSpec, ExportDecl, ParsedFile, etc.
|   +-- trait.rs                 # Language trait definition
|   +-- registry.rs              # LanguageRegistry
|
+-- languages/                   # Built-in language plugins
|   +-- mod.rs                   # Re-exports
|   +-- typescript/              # TypeScript/JavaScript plugin
|   |   +-- mod.rs               # TypeScriptLanguage trait impl
|   |   +-- parser.rs            # Wraps AstParser (tree-sitter-typescript)
|   |   +-- imports.rs           # TS import extraction (from parse/imports.rs)
|   |   +-- exports.rs           # TS export extraction (from parse/exports.rs)
|   |   +-- resolution.rs        # TS module resolution (from resolution/)
|   |   +-- metrics.rs           # TS metrics helpers
|   +-- rust/                    # Rust plugin
|   |   +-- mod.rs               # RustLanguage trait impl
|   |   +-- parser.rs            # Wraps RustAstParser (from parse/rust.rs)
|   |   +-- resolution.rs        # Rust module resolution (from analyzer/parse_rust.rs)
|   |   +-- entry_points.rs      # Cargo.toml entry discovery (from discovery/rust.rs)
|   +-- python/                  # Future: Python plugin
|       +-- mod.rs
|       +-- ...
|
+-- rule/                        # Rule infrastructure
|   +-- mod.rs                   # Re-exports
|   +-- trait.rs                 # Rule trait, Issue, RuleContext
|   +-- registry.rs              # RuleRegistry
|   +-- project.rs               # ProjectContext for graph-level rules
|
+-- rules/                       # Built-in rules
|   +-- mod.rs                   # Re-exports
|   +-- dead_code.rs             # Dead code detection (graph-level)
|   +-- unused_exports.rs        # Unused exports (graph-level)
|   +-- circular_deps.rs         # Circular dependency detection (graph-level)
|   +-- duplicate_code.rs        # Code duplication detection
|   +-- duplicate_exports.rs     # Duplicate export names
|   +-- unused_deps.rs           # Unused package dependencies
|   +-- unresolved_imports.rs    # Unresolved import specifiers
|   +-- deep_nesting.rs          # Cross-language: deep nesting
|   +-- high_complexity.rs       # Cross-language: high cyclomatic complexity
|   +-- todo_comments.rs         # Cross-language: TODO/FIXME/HACK
|   +-- gotchas/                 # Language-specific gotcha rules
|       +-- ts/                  # TypeScript gotchas
|       |   +-- loose_equality.rs
|       |   +-- any_type.rs
|       |   +-- eval.rs
|       |   +-- xss.rs
|       |   +-- console.rs
|       |   +-- env_no_fallback.rs
|       +-- rust/                # Rust gotchas
|       |   +-- unused_mut.rs
|       |   +-- unnecessary_return.rs
|       +-- python/              # Future: Python gotchas
|           +-- bare_except.rs
|           +-- mutable_default_arg.rs
|
+-- analyzer/                    # Pipeline orchestration
|   +-- mod.rs                   # Re-exports analyze()
|   +-- orchestrator.rs          # New pipeline
|   +-- convert.rs               # AnalysisResult -> AnalysisOutput bridge
|
+-- parse/                       # SHARED parsing utilities (used by language plugins)
|   +-- mod.rs                   # collect_nodes, unquote
|   +-- blocks.rs                # Generic block extraction (tree-sitter based)
|   +-- complexity.rs            # Generic complexity computation
|   +-- metrics.rs               # Generic LOC counting
|   +-- errors.rs                # Generic error collection
|
+-- output/                      # Output formatters (unchanged)
+-- frameworks/                  # Framework detection (unchanged)
+-- types.rs                     # Legacy types -> gradually migrate to language/types.rs
```

### What Gets Moved Where

| Current Location | New Location | Notes |
|-----------------|--------------|-------|
| `parse/imports.rs` | `languages/typescript/imports.rs` | TS-specific |
| `parse/exports.rs` | `languages/typescript/exports.rs` | TS-specific |
| `parse/rust.rs` | `languages/rust/parser.rs` | Rust-specific |
| `parse/blocks.rs` | Stays in `parse/` | Generic (tree-sitter) |
| `parse/complexity.rs` | Stays in `parse/` | Generic (tree-sitter) |
| `parse/metrics.rs` | Stays in `parse/` | Generic |
| `parse/errors.rs` | Stays in `parse/` | Generic |
| `discovery/rust.rs` | `languages/rust/entry_points.rs` | Rust-specific |
| `analyzer/parse_typescript.rs` | `languages/typescript/` | Absorbed into plugin |
| `analyzer/parse_rust.rs` | `languages/rust/` | Absorbed into plugin |
| `issues/gotchas/*` | `rules/gotchas/ts/*`, `rules/gotchas/rust/*` | Split by language |
| `issues/dead_code.rs` | `rules/dead_code.rs` | Graph-level rule |
| `issues/unused_exports.rs` | `rules/unused_exports.rs` | Graph-level rule |
| `issues/circular_deps.rs` | `rules/circular_deps.rs` | Graph-level rule |
| `resolution/mod.rs` | `languages/typescript/resolution.rs` | TS-specific |
| `resolution/paths.rs` | Stays shared or moved to TS | Extension resolution |
| `resolution/tsconfig.rs` | `languages/typescript/resolution.rs` | TS-specific |

---

## 8. Open Questions

### Q1: Should the `Resolver` be per-language or shared?

**Current:** A single `Resolver` struct handles TS-style resolution.

**Proposal:** Each language owns its resolution via `resolve_import()`. The TS plugin creates and configures its own resolver. Shared helpers in `parse/paths.rs` for common operations.

**Recommendation:** Per-language resolution, with shared path utilities.

### Q2: How do graph-level rules fit the per-file Rule trait?

Two-phase rules: per-file rules get `RuleContext` (parallel), project-level rules get `ProjectContext` (sequential). The orchestrator runs them separately. The trait has an optional `check_project()` with a default empty implementation.

### Q3: How to handle mixed-language dependency edges?

No special handling needed. The dep_graph is language-agnostic -- just file-to-file edges. If a TS import resolves to a `.rs` file (e.g., via wasm-pack), that edge goes into the shared graph naturally.

### Q4: How do framework profiles interact with the language plugin?

Framework profiles become an optional cross-cutting concern. The language plugin's `discover_entry_points()` can accept framework profiles as an optional parameter, or framework profiles become their own plugin type that augment entry point discovery.

### Q5: Should language plugins be dynamically loadable (WASM/FFI)?

Start with static registration. The trait-based design makes dynamic loading a future option without redesigning the core.

### Q6: Tree-sitter version compatibility?

Different tree-sitter grammars may require different core versions. Pin versions in the workspace and verify grammar compatibility. Feature flags can gate language plugins if conflicts arise.

---

## Appendix A: Comparison with Existing Tools

| Aspect | ESLint v9 | Clippy | Semgrep | Ruff | **Statico (proposed)** |
|--------|-----------|--------|---------|------|----------------------|
| Language model | `Language` interface | Compiler internals | Generic AST + translators | Per-rule structs | `Language` trait + `Rule` trait |
| Adding a language | Implement `Language` plugin | N/A (Rust only) | Add grammar + translator | N/A (Python only) | Implement `Language` trait |
| Adding a rule | `CustomRuleDefinition` | `declare_clippy_lint!` + `LintPass` | YAML pattern on generic AST | Struct + checker trait | Struct implementing `Rule` trait |
| Rule language scope | Typed per-language | Rust only | Generic AST (cross-language) | Python only | `languages()` method on rule |
| Resolution | Per-language in config | Cargo | Not built-in | Not built-in | `resolve_import()` on Language trait |
| Dynamic plugins | Yes (ESM) | No | Yes (registry) | Planned | Future (WASM) |

## Appendix B: Why Not Semgrep-Style "Generic AST"?

Semgrep's approach of mapping all languages to a single generic AST is elegant for pattern matching but has trade-offs:

**Pros:** One rule works across all languages; pattern matching is powerful.

**Cons:** Language-specific semantics get lost; the generic AST is a lowest-common-denominator; building/maintaining translators is significant work; doesn't help with module resolution.

**Decision:** Statico's approach is closer to ESLint: each language has its own parser producing a unified output shape (`ParsedFile`), but parsing itself is language-specific. Rules can be cross-language (operating on unified output) or language-specific (operating on raw source with language semantics).

This gives the best of both worlds:
- Cross-language rules for universal concerns (complexity, nesting, TODOs)
- Language-specific rules for gotchas (loose equality in TS, mutable defaults in Python)
- Language-specific module resolution without a generic abstraction
