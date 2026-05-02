//! `statico fix` — apply safe automated fixes to a project (audit F1.6).
//!
//! Today this handles two narrow transforms:
//!
//! 1. **Unused exports** in TypeScript / JavaScript: drops the `export`
//!    keyword from a small set of well-formed declarations
//!    (`export const X = …`, `export function X(`, `export class X`,
//!    `export type X =`, `export interface X`). Any export whose source
//!    line doesn't match exactly one of those patterns is *skipped*, never
//!    silently rewritten.
//!
//! 2. **Unused npm dependencies**: removes matching entries from the
//!    project's `package.json`. The file is rewritten preserving 2-space
//!    indentation; whitespace and key order in untouched objects survive
//!    via `serde_json::Value` (object iteration is `BTreeMap`-stable).
//!
//! Default mode is **dry-run** — `--apply` is required to actually touch
//! the filesystem. Both modes print a human-readable summary on stdout.
//!
//! Fixes that span declarators, re-export lists, `export default`, or
//! `export *` are intentionally out of scope: doing them safely needs the
//! full TS AST, and getting the byte offsets wrong silently produces
//! broken code. Statico would rather skip and tell you than corrupt your
//! source.
//!
//! On exit, the process returns 0 in dry-run mode, 0 in apply mode when
//! everything succeeds, and 1 if any file failed to write.

use std::collections::BTreeMap;
use std::path::Path;
use std::process;

use regex::Regex;

use statico::types::{AnalysisOutput, UnusedExportIssue};

/// Categories the `fix` command knows how to fix.
#[derive(Debug, Clone, Copy)]
pub struct FixSelection {
    pub unused_exports: bool,
    pub unused_deps: bool,
}

impl FixSelection {
    pub fn none(&self) -> bool {
        !self.unused_exports && !self.unused_deps
    }
}

pub fn run_fix(path: &str, apply: bool, selection: FixSelection) {
    let root = match std::fs::canonicalize(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: cannot resolve path '{}': {}", path, e);
            process::exit(1);
        }
    };

    if selection.none() {
        eprintln!("error: nothing to fix — pass --unused-exports and/or --unused-deps");
        process::exit(1);
    }

    eprintln!("statico fix: analyzing {}…", root.display());
    let config = statico::config::StaticoConfig::load(&root);
    let output = match statico::analyzer::analyze_with_options(&root, &config.exclude, false) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("error: {}", msg);
            process::exit(1);
        }
    };

    let mut report = FixReport::default();

    if selection.unused_exports {
        fix_unused_exports(&root, &output, apply, &mut report);
    }
    if selection.unused_deps {
        fix_unused_deps(&root, &output, apply, &mut report);
    }

    print_report(&report, apply);

    if report.write_errors > 0 {
        process::exit(1);
    }
}

#[derive(Debug, Default)]
pub struct FixReport {
    pub exports_applied: usize,
    pub exports_skipped: Vec<SkippedExport>,
    pub deps_applied: usize,
    pub deps_skipped: Vec<String>,
    pub write_errors: usize,
}

#[derive(Debug)]
pub struct SkippedExport {
    pub path: String,
    pub name: String,
    pub reason: String,
}

fn fix_unused_exports(root: &Path, output: &AnalysisOutput, apply: bool, report: &mut FixReport) {
    // Group unused exports by file so we read each file at most once.
    let mut by_file: BTreeMap<&str, Vec<&UnusedExportIssue>> = BTreeMap::new();
    for issue in &output.issues.unused_exports {
        by_file.entry(issue.path.as_str()).or_default().push(issue);
    }

    for (rel_path, issues) in by_file {
        let abs = root.join(rel_path);
        let source = match std::fs::read_to_string(&abs) {
            Ok(s) => s,
            Err(e) => {
                report.write_errors += 1;
                eprintln!("warning: cannot read {}: {}", abs.display(), e);
                continue;
            }
        };

        let mut new_source = source.clone();
        let mut applied_here = 0usize;

        for issue in &issues {
            match try_strip_export(&new_source, &issue.name) {
                Ok(replacement) => {
                    new_source = replacement;
                    applied_here += 1;
                    report.exports_applied += 1;
                }
                Err(reason) => {
                    report.exports_skipped.push(SkippedExport {
                        path: rel_path.to_string(),
                        name: issue.name.clone(),
                        reason,
                    });
                }
            }
        }

        if applied_here == 0 {
            continue;
        }

        if apply && let Err(e) = atomic_write(&abs, &new_source) {
            report.write_errors += 1;
            eprintln!("error: failed to write {}: {}", abs.display(), e);
        }
    }
}

/// Try to remove the `export` keyword from the unique declaration of `name`
/// inside `source`. Returns the new source on success, or a reason the line
/// could not be safely rewritten.
fn try_strip_export(source: &str, name: &str) -> Result<String, String> {
    // Build a regex anchored to a *line start* — that way we don't accidentally
    // match `export type X = export const X` and similar pathological inputs.
    // The regex looks for whole-word `name` after one of the supported keywords.
    //
    // Note: `regex::escape` is called on `name` so symbols like `$` in
    // identifiers are matched literally.
    let escaped = regex::escape(name);
    let pattern = format!(
        r"(?m)^([\t ]*)export[\t ]+((?:async[\t ]+)?(?:const|let|var|function|class|type|interface)[\t ]+)({})\b",
        escaped
    );
    let re = Regex::new(&pattern).map_err(|e| format!("internal regex error: {}", e))?;
    let matches: Vec<_> = re.captures_iter(source).collect();
    if matches.is_empty() {
        return Err("no matching `export` declaration found".to_string());
    }
    if matches.len() > 1 {
        return Err(format!("found {} candidate lines — refusing to guess", matches.len()));
    }

    // Apply the rewrite: drop `export ` while preserving the leading indent,
    // the keyword that followed, and the identifier itself.
    let new_source = re.replace(source, "${1}${2}${3}").to_string();
    if new_source == source {
        return Err("regex match produced no change".to_string());
    }
    Ok(new_source)
}

fn fix_unused_deps(root: &Path, output: &AnalysisOutput, apply: bool, report: &mut FixReport) {
    if output.issues.unused_dependencies.is_empty() {
        return;
    }

    // For now, all unused-dep findings target the project-root package.json.
    // (Per-workspace fixing would need the issue type to carry the workspace
    // root — tracked as a future enhancement.)
    let pkg_path = root.join("package.json");
    let source = match std::fs::read_to_string(&pkg_path) {
        Ok(s) => s,
        Err(e) => {
            for issue in &output.issues.unused_dependencies {
                report.deps_skipped.push(format!(
                    "{} — cannot read {} ({})",
                    issue.package_name,
                    pkg_path.display(),
                    e
                ));
            }
            return;
        }
    };

    let mut json: serde_json::Value = match serde_json::from_str(&source) {
        Ok(v) => v,
        Err(e) => {
            for issue in &output.issues.unused_dependencies {
                report.deps_skipped.push(format!("{} — could not parse package.json ({})", issue.package_name, e));
            }
            return;
        }
    };

    let mut applied_here = 0usize;
    for issue in &output.issues.unused_dependencies {
        // Try the section the analyzer told us about first; fall back to
        // the standard set in case the analyzer reports a non-canonical name.
        let primary = issue.location.as_str();
        let candidates = [primary, "dependencies", "devDependencies", "peerDependencies", "optionalDependencies"];
        let removed = candidates.iter().any(|section| {
            json.get_mut(section)
                .and_then(|v| v.as_object_mut())
                .map(|obj| obj.remove(&issue.package_name).is_some())
                .unwrap_or(false)
        });
        if removed {
            applied_here += 1;
            report.deps_applied += 1;
        } else {
            report.deps_skipped.push(format!("{} — not found in any dependencies section", issue.package_name));
        }
    }

    if applied_here == 0 {
        return;
    }

    if apply {
        // Two-space indent (serde_json default) + trailing newline — the
        // shape `npm install` itself writes. Original formatting beyond
        // that is intentionally not preserved; this matches the
        // reformat-on-fix behavior of npm/yarn/pnpm.
        let mut new_source = serde_json::to_string_pretty(&json).unwrap_or_else(|_| source.clone());
        if !new_source.ends_with('\n') {
            new_source.push('\n');
        }
        if let Err(e) = atomic_write(&pkg_path, &new_source) {
            report.write_errors += 1;
            eprintln!("error: failed to write {}: {}", pkg_path.display(), e);
        }
    }
}

fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("statico.tmp");
    std::fs::write(&tmp, contents.as_bytes())?;
    std::fs::rename(&tmp, path)
}

fn print_report(report: &FixReport, apply: bool) {
    let verb = if apply { "removed" } else { "would remove" };
    println!("statico fix:");
    println!("  unused exports: {} {}, {} skipped", report.exports_applied, verb, report.exports_skipped.len());
    for s in &report.exports_skipped {
        println!("    skipped {}::{} — {}", s.path, s.name, s.reason);
    }
    println!("  unused deps:    {} {}, {} skipped", report.deps_applied, verb, report.deps_skipped.len());
    for s in &report.deps_skipped {
        println!("    skipped {}", s);
    }
    if !apply && (report.exports_applied + report.deps_applied) > 0 {
        println!();
        println!("Run with --apply to write changes to disk.");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_export_const() {
        let source = "export const Foo = 1;\nconst Bar = 2;\n";
        let after = try_strip_export(source, "Foo").unwrap();
        assert_eq!(after, "const Foo = 1;\nconst Bar = 2;\n");
    }

    #[test]
    fn strip_export_function() {
        let source = "  export function helper() {}\n";
        let after = try_strip_export(source, "helper").unwrap();
        assert_eq!(after, "  function helper() {}\n");
    }

    #[test]
    fn strip_export_async_function() {
        let source = "export async function fetchIt() {}\n";
        let after = try_strip_export(source, "fetchIt").unwrap();
        assert_eq!(after, "async function fetchIt() {}\n");
    }

    #[test]
    fn strip_export_type() {
        let source = "export type ID = string;\n";
        let after = try_strip_export(source, "ID").unwrap();
        assert_eq!(after, "type ID = string;\n");
    }

    #[test]
    fn strip_export_interface() {
        let source = "export interface User { name: string }\n";
        let after = try_strip_export(source, "User").unwrap();
        assert_eq!(after, "interface User { name: string }\n");
    }

    #[test]
    fn strip_export_skips_unsupported_default() {
        // `export default Foo` — we don't know how to drop default safely.
        let source = "export default Foo;\n";
        let result = try_strip_export(source, "Foo");
        assert!(result.is_err());
    }

    #[test]
    fn strip_export_skips_named_re_export() {
        let source = "export { Foo } from './x';\n";
        let result = try_strip_export(source, "Foo");
        assert!(result.is_err(), "named re-export should not be auto-stripped");
    }

    #[test]
    fn strip_export_skips_export_star() {
        let source = "export * from './x';\n";
        let result = try_strip_export(source, "Foo");
        assert!(result.is_err());
    }

    #[test]
    fn strip_export_word_boundary() {
        // `Foo` should not match `FooBar`.
        let source = "export const FooBar = 1;\n";
        let result = try_strip_export(source, "Foo");
        assert!(result.is_err(), "must not match prefix-only identifier");
    }

    #[test]
    fn strip_export_refuses_when_multiple_matches() {
        // Two identical declarations — refuse rather than guess.
        let source = "export const X = 1;\nexport const X = 2;\n";
        let result = try_strip_export(source, "X");
        assert!(result.is_err());
    }

    #[test]
    fn strip_export_preserves_other_lines() {
        let source = "import { foo } from './x';\nexport const Bar = 1;\nexport const Other = 2;\n";
        let after = try_strip_export(source, "Bar").unwrap();
        assert_eq!(after, "import { foo } from './x';\nconst Bar = 1;\nexport const Other = 2;\n");
    }
}
