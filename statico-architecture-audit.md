# Statico Language-Coupling Architecture Audit

**Date:** 2026-04-30  
**Scope:** Map every TypeScript-specific coupling point to plan a refactor toward a `LanguagePlugin` trait model.

---

## Executive Summary

Statico already has **partial** Rust support bolted on via ad-hoc branching (`if is_rust { ... } else { ... }`), but the architecture is fundamentally TypeScript-first. TypeScript assumptions are baked into **8 of the 10 key modules** at the type level, function signatures, hardcoded AST node names, and string literals. The Rust support was added by copy-pasting the TS pattern rather than abstracting it.

The pipeline is: **discovery → parse → analyze → issues → output**. Language-specific branching currently exists at parse-time only. All downstream stages (issues, output) are language-agnostic by accident — they operate on uniform types (`Vec<String>`, `BTreeMap<String, Vec<String>>`) — but they're fed exclusively TS-shaped data.

### Coupling Density (quick reference)

| File | TS-specific LOC | Language-agnostic LOC | Coupling Severity |
|------|----------------|----------------------|-------------------|
| `parse/mod.rs` | ~30 (AstParser) | ~40 (helpers) | 🔴 **Critical** |
| `parse/imports.rs` | ~200 | ~0 | 🔴 **Critical** |
| `parse/exports.rs` | ~250 | ~0 | 🔴 **Critical** |
| `parse/rust.rs` | ~350 | ~0 | 🔴 **Rust mirror** |
| `discovery/mod.rs` | ~20 | ~80 | 🟡 **Medium** |
| `discovery/rust.rs` | ~130 | ~0 | 🔴 **Rust mirror** |
| `analyzer/mod.rs` | ~30 | ~100 | 🟡 **Medium** |
| `analyzer/parse_typescript.rs` | ~180 | ~20 | 🔴 **Critical** |
| `analyzer/parse_rust.rs` | ~250 | ~0 | 🔴 **Rust mirror** |
| `resolution/mod.rs` | ~40 | ~160 | 🟡 **Medium** |
| `resolution/paths.rs` | ~5 | ~40 | 🟢 **Low** |
| `issues/gotchas/mod.rs` | ~0 | ~80 | 🟢 **Low** |
| `issues/gotchas/patterns.rs` | ~180 | ~0 | 🔴 **Critical** |
| `issues/gotchas/ast.rs` | ~120 | ~10 | 🔴 **Critical** |
| `issues/unused_exports.rs` | ~15 | ~130 | 🟡 **Medium** |
| `issues/dead_code.rs` | ~0 | ~100 | 🟢 **Low** |
| `types.rs` | ~5 (doc comments) | ~200 | 🟢 **Low** |
| `issues/mod.rs` | ~30 (extract_type_exports_map) | ~60 | 🟡 **Medium** |

---

## 1. `src/parse/mod.rs` — The Parser

### What's TypeScript-specific
- **`AstParser::new()`** hardcodes `tree_sitter_typescript::LANGUAGE_TYPESCRIPT` as the default language.
- **`AstParser::parse(&self, code: &str, is_tsx: bool)`** — the `is_tsx` parameter is TS-specific. It switches between `LANGUAGE_TSX` and `LANGUAGE_TYPESCRIPT`.
- The entire struct is named/wrapped around the concept of "TypeScript/TSX parsing" (doc comment says exactly that).

### What's language-agnostic
- `ParseResult` struct (tree + has_errors) — universal.
- `collect_nodes()` — generic tree-sitter traversal by node kind string.
- `unquote()` — string literal stripping.

### Current signatures that bake in TS assumptions
```rust
pub struct AstParser { parser: Mutex<Parser> }
impl AstParser {
    pub fn new() -> Result<Self, tree_sitter::LanguageError>  // hardcoded TS
    pub fn parse(&self, code: &str, is_tsx: bool) -> Option<ParseResult>  // TSX toggle
}
```

### What a `LanguagePlugin` trait needs here
```rust
trait LanguagePlugin {
    fn language_id(&self) -> &str;  // "typescript", "rust", etc.
    fn tree_sitter_language(&self) -> tree_sitter::Language;
    fn file_extensions(&self) -> &[&str];  // [".ts", ".tsx"] or [".rs"]
    fn parse(&self, code: &str, file_extension: &str) -> Option<ParseResult>;
}
```
- The `is_tsx` parameter should be replaced by passing the file extension; the plugin decides which grammar variant to use.

---

## 2. `src/parse/imports.rs` — Import Extraction

### What's TypeScript-specific (everything)
- **AST node kinds are TS-specific:** `"import_statement"`, `"export_statement"`, `"call_expression"`, `"import_clause"`, `"named_imports"`, `"import_specifier"`, `"namespace_import"`, `"new_expression"`, `"export_clause"`, `"export_specifier"`, `"arguments"`, `"string"`, `"template_string"`, `"identifier"`.
- **Import patterns are TS-specific:**
  - `import { foo } from './utils'` (static imports)
  - `import('./module')` (dynamic imports)
  - `require('module')` (CommonJS)
  - `new Worker(new URL('./path', import.meta.url))` (Web Workers)
- **Classification logic is TS-specific:** `classify_import` checks for `.` (relative), `@/` (tsconfig alias), `~` (webpack), `#` (Node subpath imports).
- **Package name extraction** assumes npm conventions (`@scope/package`).

### Language-agnostic parts
- `extract_package_name()` could be generic if parameterized by package ecosystem conventions.

### Current signatures that bake in TS assumptions
```rust
pub fn extract_imports(root: Node, source: &str) -> (Vec<String>, Vec<String>)
pub fn extract_named_imports(root: Node, source: &str) -> BTreeMap<String, Vec<String>>
fn classify_import(spec: &str, internal: &mut Vec<String>, external: &mut Vec<String>)
pub fn extract_package_name(spec: &str) -> String
```

### What a `LanguagePlugin` trait needs here
```rust
trait LanguagePlugin {
    fn extract_imports(&self, root: Node, source: &str) -> ImportExtractionResult;
    fn classify_import(&self, spec: &str) -> ImportClassification;
}

struct ImportExtractionResult {
    internal: Vec<String>,
    external: Vec<String>,
    named: BTreeMap<String, Vec<String>>,  // module → names
}

enum ImportClassification { Internal, External(String), Skip }
```

---

## 3. `src/parse/exports.rs` — Export Extraction

### What's TypeScript-specific (everything)
- **AST node kinds are TS-specific:** `"export_statement"`, `"function_declaration"`, `"generator_function_declaration"`, `"class_declaration"`, `"type_alias_declaration"`, `"interface_declaration"`, `"lexical_declaration"`, `"variable_declaration"`, `"variable_declarator"`, `"export_clause"`, `"export_specifier"`, `"type_identifier"`, `"namespace_import"`.
- **Export patterns are TS-specific:**
  - `export function foo() {}`
  - `export const A = 1, B = 2`
  - `export { foo, bar as baz }`
  - `export * from './mod'`
  - `export default function main() {}`
  - `export type Result<T> = ...`
  - `export interface Config { ... }`
- **Type-specific extraction:** `extract_type_exports`, `extract_reexport_types`, `extract_reexport_sources`, `extract_star_reexport_specs` — all assume TS type/interface/export syntax.
- **PascalCase heuristic** (`is_pascal_case`) is used to guess whether a re-exported name is a type.

### Language-agnostic parts
- The concept of "extracting names from declarations" is universal, but the implementation is 100% TS AST.

### Current signatures
```rust
pub fn extract_exports(root: Node, source: &str) -> Vec<String>
pub fn extract_type_exports(root: Node, source: &str) -> Vec<(String, String)>
pub fn extract_reexport_types(root: Node, source: &str) -> Vec<(String, String)>
pub fn extract_star_reexport_specs(root: Node, source: &str) -> Vec<String>
pub fn extract_reexport_sources(root: Node, source: &str) -> Vec<(String, String)>
```

### What a `LanguagePlugin` trait needs here
```rust
trait LanguagePlugin {
    fn extract_exports(&self, root: Node, source: &str) -> Vec<Export>;
    fn extract_reexports(&self, root: Node, source: &str) -> Vec<Reexport>;
}

struct Export { name: String, kind: ExportKind }  // function, type, class, const, etc.
struct Reexport { name: String, source_spec: String, is_star: bool }
```

---

## 4. `src/parse/rust.rs` — Rust Parsing (Mirror)

This is the Rust **equivalent** of `imports.rs` + `exports.rs` but implemented as a separate module with its own:
- `RustAstParser` (mirrors `AstParser`)
- `extract_exports()` returning `RustExport` (mirrors TS `extract_exports`)
- `extract_imports()` returning `RustImport` (mirrors TS `extract_imports`)
- `extract_mod_decls()` returning `RustModDecl` (Rust-specific: `mod foo;`)

### Problem
This is a **parallel implementation**, not a plugin. Adding a third language would require yet another copy.

### What the plugin model should unify
- `RustAstParser` and `AstParser` should be the same trait object.
- `RustExport` and TS export results should conform to a shared `Export` type.
- `RustImport` and TS import results should conform to a shared `Import` type.
- `RustModDecl` (Rust's `mod` system) is language-specific and would live in the Rust plugin.

---

## 5. `src/discovery/mod.rs` — File Discovery

### What's TypeScript-specific
- **`SOURCE_EXTENSIONS`** hardcodes `["ts", "tsx", "js", "jsx", "rs"]` — Rust is already included but any new language requires editing this constant.
- **`.d.ts` filter:** `if rel.ends_with(".d.ts") { continue; }` — TS-specific file exclusion.
- **Language mapping is hardcoded:**
  ```rust
  let lang = match ext {
      "ts" => "typescript", "tsx" => "tsx",
      "js" => "javascript", "jsx" => "jsx",
      "rs" => "rust", _ => continue,
  };
  ```
- **Config file discovery** lists `tsconfig.json`, `jsconfig.json`, `next.config.*` alongside `Cargo.toml`.
- **Skipped directories** include TS-specific entries: `node_modules`, `.next`, `.nuxt`.

### What's language-agnostic
- `walkdir`-based traversal.
- `filter_excluded()` with glob matching.
- `is_skipped_dir()` (most entries are universal: `.git`, `dist`, `build`, `target`).

### What a `LanguagePlugin` trait needs here
```rust
trait LanguagePlugin {
    fn extensions(&self) -> &[&str];
    fn language_name(&self) -> &str;
    fn should_skip_file(&self, rel_path: &str) -> bool;
    fn config_file_names(&self) -> &[&str];
}
```
The discovery module would iterate over registered plugins' extensions to build the file list.

---

## 6. `src/resolution/mod.rs` — Module Resolution

### What's TypeScript-specific
- **`Resolver` is entirely built around TS/Node.js resolution:**
  - `load_tsconfig_paths()` — loads `paths` aliases from `tsconfig.json`.
  - `load_all_tsconfig_paths()` — walks for all `tsconfig.json` files in workspace.
  - `load_workspace_packages()` — reads `package.json` files, maps `@scope/name` to directories.
  - Parses `package.json` `"exports"` field for sub-path imports.
  - Falls back to `index.ts`/`index.tsx` for missing `main` fields.
- **`PathAlias`** is tsconfig-specific (prefix → targets pattern).
- **`TsconfigScope`** — per-tsconfig scoped resolution.
- **`find_published_package_dirs`** assumes `packages/` directory and `package.json` files.
- **Extension resolution** (`paths.rs`) tries `[".ts", ".tsx", ".js", ".jsx", ".rs"]` — Rust was bolted on.

### What's language-agnostic
- The `resolve(from_dir, spec) -> Option<PathBuf>` interface itself.
- The concept of path aliases (though the tsconfig format is TS-specific).

### What a `LanguagePlugin` trait needs here
```rust
trait LanguagePlugin {
    fn resolve_import(&self, from_dir: &Path, spec: &str, root: &Path) -> Option<PathBuf>;
    fn load_config(&mut self, root: &Path);  // e.g., parse tsconfig.json, Cargo.toml
}
```
Rust resolution (via `resolve_rust_use_path` in `analyzer/parse_rust.rs`) should move into the Rust plugin's `resolve_import`.

---

## 7. `src/issues/gotchas/mod.rs` — Gotcha Detection Pipeline

### What's TypeScript-specific
- The **pipeline itself** is language-agnostic (parallel detection across files, sorting by severity).
- **But:** It currently runs ALL rules against ALL files regardless of language. This means Rust files get flagged for:
  - `==` vs `===` (Rust doesn't have `===`)
  - `: any` type annotations (Rust doesn't have `any`)
  - `as any` casts
  - `console.log()` (Rust uses `println!`)
  - `process.env` without fallback
  - `dangerouslySetInnerHTML`
  - `.innerHTML`
  - `eval()` — shared concern, but TS-specific heuristics

### What's language-agnostic
- `GotchaIssue` struct and severity model.
- Framework gotcha integration (rules are declarative).
- `is_test_file()`, `is_example_or_script()`, `truncate_line()`.

### Current signatures
```rust
pub fn detect(source_files: &[(String, String)]) -> Vec<GotchaIssue>
pub fn detect_with_frameworks(source_files: &[(String, String)], profiles: &[&FrameworkProfile]) -> Vec<GotchaIssue>
```
**These don't take a language parameter.** Every file is treated identically.

### What a `LanguagePlugin` trait needs here
```rust
trait LanguagePlugin {
    fn gotcha_rules(&self) -> Vec<Box<dyn GotchaRule>>;
}

trait GotchaRule {
    fn detect(&self, file: &str, source: &str) -> Vec<GotchaIssue>;
    fn language_id(&self) -> &str;  // only run for matching language files
}
```
Each plugin contributes its own gotcha rules. The pipeline filters files by language before applying rules.

---

## 8. `src/issues/gotchas/patterns.rs` — Pattern-Based Gotchas

### What's TypeScript-specific (essentially everything)
All pattern-matching rules are TS/JS-specific:

| Rule | TS-specific? | Applies to Rust? |
|------|-------------|-----------------|
| `loose-equality` (`==` vs `===`) | ✅ | ❌ Rust has no `===` |
| `loose-inequality` (`!=` vs `!==`) | ✅ | ❌ |
| `any-type` (`: any`) | ✅ | ❌ |
| `as-any-cast` (`as any`) | ✅ | ❌ |
| `any-cast-angle` (`<any>`) | ✅ | ❌ |
| `eval-usage` | ⚠️ shared concern | ⚠️ but heuristics are TS-specific |
| `xss-innerhtml` | ✅ DOM-specific | ❌ |
| `xss-dangerously-set` | ✅ React-specific | ❌ |
| `console-statement` | ✅ | ❌ (Rust uses `println!`) |
| `unresolved-comment` (TODO/FIXME) | 🟢 universal | ✅ |
| `env-no-fallback` (`process.env`) | ✅ Node-specific | ❌ |

**~11 of 12 rules are TS-specific.** Running them on `.rs` files produces **~250 false positives** (as noted in the task description).

### What a `LanguagePlugin` trait needs here
The TS plugin would register these rules. The Rust plugin would register its own rules (e.g., `unsafe` usage, `unwrap()` in production code, `clone()` on large types, etc.). Universal rules (TODO/FIXME) would be language-agnostic.

---

## 9. `src/issues/gotchas/ast.rs` — AST-Based Gotchas

### What's TypeScript-specific
- Uses `AstParser::new()` (hardcoded TS grammar) to parse **every file** — including `.rs` files. This means Rust files get parsed with the TS grammar, producing garbage AST.
- **Empty catch detection** looks for `"catch_clause"` and `"statement_block"` / `"block"` node kinds — TS-specific AST nodes. Rust uses `match` and `Result`, not try/catch.
- **Unhandled promise detection** looks for `.then()` without `.catch()` — JavaScript-specific. Rust uses `?` operator.
- **Callback hell detection** looks for `function(` and `=>` — JS/TS-specific.
- `detect_high_complexity` and `detect_deep_nesting` are **language-agnostic** in concept but use TS `AstParser` for re-parsing.

### Current signature
```rust
pub fn detect_ast_gotchas(rel_path: &str, source: &str, issues: &mut Vec<GotchaIssue>)
```
**No language parameter.** Parses every file as TypeScript.

### Fix required
This function needs to receive a `LanguagePlugin` reference and use the appropriate parser. Or each plugin provides its own AST-based gotcha detection.

---

## 10. `src/issues/unused_exports.rs` — Export Usage Tracking

### What's TypeScript-specific
- **Barrel file detection** (`detect_barrel_files`) looks for TS/JS-specific patterns:
  - `index.ts`, `index.tsx`, `index.js`, `index.jsx` as barrel filenames.
  - `export { ... } from '...'` pattern matching (TS syntax).
  - `export * from`, `export type {` — TS syntax.
- **`is_rust_lib_file()`** — bolted-on special case for `lib.rs` as crate root.
- **`is_test_fixture()`** mixes TS and Rust patterns: `.snap`, `.config.ts`, `-types.ts` (TS) alongside `_test.rs`, `/tests/`, `/benches/` (Rust).

### What's language-agnostic
- The core algorithm: compare `file_exports` against `imported_names`, exclude entry points and public API.
- `detect()` function signature is language-agnostic (works on string maps).
- Deep resolution (re-export chain tracing) is conceptually universal.

### What a `LanguagePlugin` trait needs here
```rust
trait LanguagePlugin {
    fn is_barrel_file(&self, path: &str, source: &str) -> bool;
    fn is_public_api_file(&self, path: &str) -> bool;  // e.g., lib.rs, index.ts
    fn is_test_fixture(&self, path: &str) -> bool;
}
```

---

## 11. `src/issues/dead_code.rs` — Dead Code Detection

### What's TypeScript-specific
- **Nothing in the detection logic.** The BFS reachability algorithm is fully language-agnostic.
- Test file filtering uses `.test.`, `.spec.` (TS conventions) but also applies generically.

### What's language-agnostic
- **Everything.** This module operates on the dependency graph (`BTreeMap<String, Vec<String>>`) which is language-agnostic by design.
- The `detect_framework_dead` concept (implicit vs framework entry points) is universal.

### Assessment
🟢 **No changes needed** for the plugin model. Dead code detection already works across languages because it only cares about file-level reachability.

---

## 12. `src/types.rs` — Data Types

### What's TypeScript-specific
- `UnusedTypeIssue` has doc comment: "An exported TypeScript type or interface that is never imported." — the type itself is language-agnostic (name + path + kind).
- `UnusedDepIssue` has field `location: String` (doc says "package.json location") — assumes npm.
- `UnlistedDepIssue` field `imported_by: String` — language-agnostic.
- `Issues` struct has `unused_types: Vec<UnusedTypeIssue>` — this is TS-specific as a top-level field. Rust doesn't have TypeScript-style type exports.

### What's language-agnostic
- **Most types:** `AnalysisOutput`, `Structure`, `SourceFile`, `Dependencies`, `FileImports`, `Quality`, `FileQuality`, `Metrics`, `DeadCodeIssue`, `DuplicateCodeIssue`, `CircularDepIssue`, `CodeBlockLocation`, `DuplicationSection`, `GotchaIssue`, etc.

### What would change
- `UnusedTypeIssue` should be generalized or made plugin-specific.
- `UnusedDepIssue` / `UnlistedDepIssue` should reference a package manager concept, not hardcoded to npm.
- `SourceFile.language: String` should become an enum or plugin identifier rather than a freeform string.

---

## 13. `src/analyzer/mod.rs` — Main Pipeline

### What's TypeScript-specific
- **`AstParser::new()`** is called unconditionally — always creates a TS parser.
- **Resolver initialization** hardcodes TS config loading:
  ```rust
  let tsconfig_path = root.join("tsconfig.json");
  if tsconfig_path.exists() { resolver.load_tsconfig_paths(&tsconfig_path); }
  let tsconfig_app = root.join("tsconfig.app.json");
  if tsconfig_app.exists() { resolver.load_tsconfig_paths(&tsconfig_app); }
  resolver.load_workspace_packages();
  resolver.load_all_tsconfig_paths();
  ```
- **Calls `parse_typescript::parse_all_files_parallel()`** — named "typescript" but actually dispatches to Rust parsing internally. This is the **only language branch point** in the pipeline.
- **`extract_type_exports_map` in `issues/mod.rs`** re-parses files with TS `AstParser` to find type exports — fails silently on Rust files.

### What's language-agnostic
- The overall pipeline structure: discover → parse → detect issues → build output.
- Issue detection, framework detection, monorepo detection.
- Output construction.

### Current pipeline flow
```
discover_source_files(root)
  → Vec<(String, String)>  // (path, language)
  
parse_typescript::parse_all_files_parallel(root, &source_files, &parser, &resolver, &progress)
  → BUT internally: if is_rust → parse_rust::parse_rust_file()
  → Results merged into uniform types

detect_issues(&IssueContext { ... })
  → Issues (language-agnostic types)
```

### What a `LanguagePlugin` trait needs here
```rust
trait LanguagePlugin {
    fn init(&mut self, root: &Path);  // load config (tsconfig, Cargo.toml, etc.)
    fn parse_file(&self, root: &Path, rel_path: &str, source: &str) -> Option<FileResult>;
    fn resolve_import(&self, from_dir: &Path, spec: &str, root: &Path) -> Option<PathBuf>;
}

// Pipeline:
fn analyze(root: &Path) -> Result<AnalysisOutput, String> {
    let plugins: Vec<Box<dyn LanguagePlugin>> = detect_languages(root);
    for plugin in &plugins { plugin.init(root); }
    let source_files = discover_source_files(root, &plugins);
    let results = parse_all_files_parallel(root, &source_files, &plugins);
    // ... rest is language-agnostic
}
```

---

## 14. `src/analyzer/parse_typescript.rs` — TS File Parsing

### What's TypeScript-specific
- **`parse_single_file()`** hardcodes the TS parsing path:
  ```rust
  let is_tsx = rel_path.ends_with(".tsx") || rel_path.ends_with(".jsx");
  let is_rust = rel_path.ends_with(".rs");
  if is_rust {
      return super::parse_rust::parse_rust_file(...);  // ← the ONLY dispatch point
  }
  // ... TS parsing continues
  ```
- Uses `AstParser` (TS) to parse, then calls TS-specific:
  - `crate::parse::imports::extract_imports(root_node, &source)`
  - `crate::parse::imports::extract_named_imports(root_node, &source)`
  - `crate::parse::exports::extract_exports(root_node, &source)`
- Creates a new `AstParser::new()` per thread — always TS.
- **External import resolution** tries to resolve workspace packages (`@scope/name` → local files).

### What would change
This entire module should be replaced by a generic `parse_file()` that delegates to the appropriate `LanguagePlugin`. The `is_rust` branch is the exact place where a trait dispatch should happen.

---

## 15. `src/analyzer/parse_rust.rs` — Rust File Parsing (Mirror)

### What's Rust-specific (everything)
- **Rust module resolution** (`resolve_rust_use_path`): handles `crate::`, `super::`, `self::`, bare paths.
- **`find_crate_src_root()`** walks up to find `Cargo.toml`.
- **`detect_crate_name()`** reads `Cargo.toml` for the package name.
- **`try_rust_file()`** tries `{path}.rs`, `{path}/mod.rs`, `{path}/lib.rs`.
- **`is_extern_crate()`** — hardcoded list of known external crates.
- **`extract_mod_name_from_line()`** — text-based fallback for macro-expanded mod declarations.
- **Filesystem scanning** for modules when tree-sitter can't see macro-expanded `mod` declarations.

### Problem
This is ~250 lines of Rust-specific logic that lives in the analyzer rather than in a plugin. It duplicates the structure of `parse_typescript.rs` instead of sharing a common interface.

---

## 16. `src/issues/mod.rs` — Issue Detection Pipeline

### What's TypeScript-specific
- **`extract_type_exports_map()`** re-parses **every source file** with `AstParser::new()` (TS parser) to extract type/interface declarations. This means:
  - Rust files get parsed as TypeScript → produces empty/garbage results.
  - The `is_tsx` check: `path.ends_with(".tsx") || path.ends_with(".jsx")` — TS-specific.
- **`build_external_import_pairs()`** also re-parses every file with TS parser to extract external imports.

### What's language-agnostic
- The `detect_issues()` pipeline and `IssueContext` struct.
- Dead code, circular deps, duplicate exports detection.

### Fix required
- `extract_type_exports_map` should use the appropriate parser per file.
- `build_external_import_pairs` should delegate to the language plugin's import extraction.

---

## Pipeline Summary: Where Language Branching Happens

```
┌─────────────────────────────────────────────────────────────────────┐
│  DISCOVERY (discovery/mod.rs)                                       │
│  Extensions hardcoded: ts, tsx, js, jsx, rs                        │
│  Language dispatch: match on extension → string                     │
│  🔴 Needs: Plugin registry for extensions                           │
├─────────────────────────────────────────────────────────────────────┤
│  ENTRY POINTS (discovery/entry_points.rs + rust.rs)                 │
│  TS: next.config, pages/, app/ router, etc.                        │
│  Rust: Cargo.toml → lib.rs, main.rs, build.rs                      │
│  🟡 Separate modules, needs unified plugin interface                │
├─────────────────────────────────────────────────────────────────────┤
│  PARSE (analyzer/parse_typescript.rs)                               │
│  *** THE KEY DISPATCH POINT ***                                     │
│  if is_rust { parse_rust_file() } else { TS parsing }              │
│  🔴 Needs: Dispatch to LanguagePlugin.parse_file()                  │
├─────────────────────────────────────────────────────────────────────┤
│  IMPORT/EXPORT EXTRACTION (parse/imports.rs, exports.rs, rust.rs)  │
│  TS: import/export statements, require, dynamic import             │
│  Rust: use, mod, pub                                                │
│  🔴 Separate implementations, need unified trait                    │
├─────────────────────────────────────────────────────────────────────┤
│  RESOLUTION (resolution/mod.rs)                                     │
│  TS: tsconfig paths, package.json exports, workspace packages      │
│  Rust: crate::, super::, mod.rs resolution (in parse_rust.rs!)     │
│  🔴 Rust resolution lives in wrong module                           │
├─────────────────────────────────────────────────────────────────────┤
│  ISSUES (issues/*)                                                  │
│  dead_code.rs:    🟢 Fully language-agnostic                        │
│  unused_exports:  🟡 Minor TS assumptions in barrel detection       │
│  gotchas:         🔴 11/12 rules are TS-specific, run on all files │
│  unused_types:    🔴 Re-parses with TS parser, TS-specific types   │
│  circular_deps:   🟢 Fully language-agnostic                        │
│  unused_deps:     🟡 Assumes npm/package.json                       │
│  unresolved:      🟢 Language-agnostic                              │
│  unlisted_deps:   🟡 Assumes npm/package.json                       │
├─────────────────────────────────────────────────────────────────────┤
│  OUTPUT (types.rs)                                                  │
│  🟢 Mostly language-agnostic, minor TS wording in types             │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Proposed `LanguagePlugin` Trait

```rust
/// A language plugin provides all language-specific behavior for statico.
pub trait LanguagePlugin: Send + Sync {
    // Identity
    fn id(&self) -> &str;                          // "typescript", "rust"
    fn display_name(&self) -> &str;                // "TypeScript/TSX"
    fn file_extensions(&self) -> &[&str];          // [".ts", ".tsx"] or [".rs"]

    // Parsing
    fn tree_sitter_language(&self, extension: &str) -> tree_sitter::Language;
    fn parse_file(&self, root: &Path, rel_path: &str, source: &str) -> Option<FileResult>;

    // Import/Export extraction
    fn extract_imports(&self, tree: &Tree, source: &str) -> ImportExtraction;
    fn extract_exports(&self, tree: &Tree, source: &str) -> ExportExtraction;

    // Resolution
    fn init_resolver(&self, root: &Path) -> Box<dyn LanguageResolver>;
    
    // Discovery
    fn entry_points(&self, root: &Path, source_files: &[(String, String)]) -> Vec<String>;
    fn implicit_entries(&self, source_files: &[(String, String)]) -> Vec<String>;
    fn should_skip_file(&self, rel_path: &str) -> bool;
    fn config_file_names(&self) -> Vec<&str>;

    // Gotcha rules
    fn gotcha_rules(&self) -> Vec<Box<dyn GotchaDetector>>;

    // Export analysis helpers
    fn is_barrel_file(&self, path: &str, source: &str) -> bool;
    fn is_public_api_file(&self, path: &str) -> bool;
}

pub trait LanguageResolver: Send + Sync {
    fn resolve(&self, from_dir: &Path, spec: &str) -> Option<PathBuf>;
}

pub trait GotchaDetector: Send + Sync {
    fn detect(&self, path: &str, source: &str, tree: Option<&Tree>) -> Vec<GotchaIssue>;
    fn id(&self) -> &str;
}
```

---

## Migration Priority

### Phase 1: Fix the bleeding (immediate)
1. **Gotcha detection must filter by language.** Add a `language` parameter to `detect_with_frameworks()` and skip TS-specific rules for `.rs` files. This eliminates the 250 false positives.
2. **`extract_type_exports_map`** must use the Rust parser for Rust files.

### Phase 2: Extract the plugin trait (structural)
1. Define `LanguagePlugin` trait with the methods above.
2. Create `TypeScriptPlugin` and `RustPlugin` implementations.
3. Move `parse/mod.rs::AstParser` → `TypeScriptPlugin`.
4. Move `parse/rust.rs::RustAstParser` → `RustPlugin`.
5. Move `analyzer/parse_typescript.rs` TS path → `TypeScriptPlugin::parse_file()`.
6. Move `analyzer/parse_rust.rs` → `RustPlugin::parse_file()`.

### Phase 3: Unify the pipeline
1. `analyzer/mod.rs::analyze()` detects languages and creates plugins.
2. `discovery/mod.rs` queries plugins for extensions.
3. `parse_all_files_parallel` dispatches to `plugin.parse_file()`.
4. `issues/mod.rs` gets per-language type extraction and import extraction.

### Phase 4: Move resolution
1. Move Rust `resolve_rust_use_path()` from `analyzer/parse_rust.rs` into `RustPlugin`'s resolver.
2. TS resolver stays in `resolution/mod.rs` but is wrapped as `TypeScriptPlugin`'s resolver.

---

## Key Insight: The "Two-Language Hack"

The current architecture has **one language dispatch point** — the `if is_rust` branch in `analyzer/parse_typescript.rs::parse_single_file()`. Everything else is either:
- **TS-only** (imports.rs, exports.rs, gotchas) — no dispatch needed because only TS was ever expected.
- **Already language-agnostic** (dead_code, circular_deps, the output types) — works across languages by accident.

The refactor is essentially: **replace that single `if is_rust` with a trait dispatch, and make the gotcha/unused-types pipelines language-aware.** The rest of the pipeline already speaks in universal terms (file paths, import graphs, export names).
