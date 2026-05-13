//! `statico guard` command — protect files from unintended modification.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process;

pub fn run_guard_add(files: &[String], description: Option<&str>, path: &str) {
    let root = resolve_root(path);
    let mut manifest = load_manifest(&root);
    let file_refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    match manifest.add(&root, &file_refs, description) {
        Ok(n) => {
            if let Err(e) = manifest.write(&root) {
                eprintln!("error: {}", e);
                process::exit(1);
            }
            println!("\x1b[32m✓ Added {} file(s) to guard manifest\x1b[0m ({} now guarded)", n, manifest.len());
        }
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    }
}

pub fn run_guard_remove(files: &[String], path: &str) {
    let root = resolve_root(path);
    let mut manifest = load_manifest(&root);
    if manifest.is_empty() {
        println!("guard manifest is empty — nothing to remove.");
        return;
    }
    let file_refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    let removed = manifest.remove(&root, &file_refs).unwrap_or(0);
    if let Err(e) = manifest.write(&root) {
        eprintln!("error: {}", e);
        process::exit(1);
    }
    if removed > 0 {
        println!("\x1b[32m✓ Removed {} file(s) from guard manifest\x1b[0m ({} remaining)", removed, manifest.len());
    } else {
        println!("no matching files found in the guard manifest.");
    }
}

pub fn run_guard_list(path: &str) {
    let root = resolve_root(path);
    let manifest = load_manifest(&root);
    if manifest.is_empty() {
        println!("guard manifest is empty. Use `statico guard add <files>` to protect files.");
        return;
    }
    println!("\x1b[1mGuarded files ({}):\x1b[0m\n", manifest.len());
    for (rel, entry) in &manifest.files {
        let desc = entry.description.as_deref().map(|d| format!(" \x1b[2m— {}\x1b[0m", d)).unwrap_or_default();
        println!("  {}  \x1b[2m{}{}\x1b[0m", rel, &entry.hash[..16], desc);
    }
    println!();
}

pub fn run_guard_check(path: &str, exit_code: bool) {
    let root = resolve_root(path);
    let manifest = load_manifest(&root);
    if manifest.is_empty() {
        println!("guard manifest is empty — nothing to check.");
        return;
    }

    let result = manifest.check(&root);

    if !result.has_failures() {
        println!("\x1b[32m✓ All {} guarded file(s) pass integrity check\x1b[0m", result.passed());
        return;
    }

    eprintln!("\x1b[31m✗ Guard integrity check FAILED\x1b[0m\n");
    for status in &result.statuses {
        match status {
            statico::guard::FileStatus::Ok { path } => {
                println!("  \x1b[32m  OK\x1b[0m  {}", path);
            }
            statico::guard::FileStatus::Mismatch { path, expected, actual } => {
                eprintln!("  \x1b[31mMODIFIED\x1b[0m {}", path);
                eprintln!("           expected: {}", &expected[..24]);
                eprintln!("           actual:   {}", &actual[..24]);
            }
            statico::guard::FileStatus::Missing { path } => {
                eprintln!("  \x1b[31m MISSING\x1b[0m {}", path);
            }
        }
    }

    eprintln!(
        "\n  {} passed, {} failed out of {} guarded file(s)",
        result.passed(),
        result.failed(),
        result.statuses.len()
    );

    if exit_code {
        process::exit(1);
    }
}

pub fn run_guard_update(files: &[String], path: &str) {
    let root = resolve_root(path);
    let mut manifest = load_manifest(&root);
    if manifest.is_empty() {
        println!("guard manifest is empty — nothing to update.");
        return;
    }
    let file_refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    match manifest.update(&root, &file_refs) {
        Ok(n) => {
            if let Err(e) = manifest.write(&root) {
                eprintln!("error: {}", e);
                process::exit(1);
            }
            println!("\x1b[32m✓ Updated hashes for {} file(s)\x1b[0m", n);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    }
}

fn resolve_root(path: &str) -> std::path::PathBuf {
    match std::fs::canonicalize(std::path::Path::new(path)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: cannot resolve path '{}': {}", path, e);
            process::exit(1);
        }
    }
}

fn load_manifest(root: &std::path::Path) -> statico::guard::GuardManifest {
    match statico::guard::GuardManifest::load(root) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    }
}
