//! `statico plugin *` commands.

use std::process;

pub fn run_plugin_list(path: &str) {
    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: cannot resolve path '{}': {}", path, e);
            process::exit(1);
        }
    };

    let plugins = statico::plugin::discovery::discover_plugins(&root);

    if plugins.is_empty() {
        println!("No plugins found in {}", root.display());
        println!();
        println!("Add plugins to .statico/plugins/ or configure them in .statico.toml");
        return;
    }

    println!("Plugins in {}:\n", root.display());
    for p in &plugins {
        let status = if p.enabled { "\u{2713}" } else { "\u{2717}" };
        let kind = p.kind.to_string();
        println!("  {} {} ({}) — {}", status, p.name, kind, p.path.display());
        if p.override_all {
            println!("    \u{2514} override: all hooks");
        }
        if !p.languages.is_empty() {
            println!("    \u{2514} languages: {}", p.languages.join(", "));
        }
    }
}

pub fn run_plugin_schema(format: &str) {
    match format {
        "json" => {
            let schema = serde_json::json!({
                "protocol": "json-rpc-2.0",
                "transport": "newline-delimited JSON over stdin/stdout",
                "methods": {
                    "init": {
                        "params": { "root": "string", "config": "object", "plugin_settings": "object" },
                        "result": "PluginCapabilities"
                    },
                    "analyze_file": {
                        "params": { "path": "string", "source": "string", "language": "string", "existing_issues": "PluginIssue[]" },
                        "result": { "issues": "PluginIssue[]", "exports": "string[]", "dependencies": "string[]", "metrics": "PluginMetrics?" }
                    },
                    "discover_entries": {
                        "params": { "root": "string", "config_files": "string[]", "language": "string" },
                        "result": { "entry_points": "EntryPoint[]" }
                    },
                    "resolve_import": {
                        "params": { "from_file": "string", "specifier": "string", "root": "string" },
                        "result": { "resolved_path": "string", "external": "bool" }
                    },
                    "post_analysis": {
                        "params": { "results": "object", "health_score": "f64", "total_files": "usize", "language": "string" },
                        "result": { "issues": "PluginIssue[]", "suggestions": "string[]" }
                    },
                    "format_output": {
                        "params": { "results": "object", "format": "string", "health_score": "f64" },
                        "result": { "output": "string", "exit_code": "i32" }
                    },
                    "shutdown": { "params": null, "result": null }
                },
                "types": {
                    "PluginCapabilities": {
                        "name": "string",
                        "version": "string?",
                        "hooks": "Record<HookName, HookMode>",
                        "languages": "string[]",
                        "rules": "Rule[]"
                    },
                    "HookName": "analyze_file | discover_entries | resolve_import | post_analysis | format_output",
                    "HookMode": "add | override",
                    "Severity": "error | warning | info",
                    "Rule": { "id": "string", "severity": "Severity", "description": "string" },
                    "PluginIssue": { "rule_id": "string", "severity": "Severity", "message": "string", "file": "string", "line": "usize", "column": "usize?", "end_line": "usize?", "end_column": "usize?", "confidence": "f64?", "suggestion": "string?" },
                    "EntryPoint": { "path": "string", "type": "string?", "framework": "string?" }
                }
            });
            println!("{}", serde_json::to_string_pretty(&schema).unwrap());
        }
        _ => {
            println!("statico Plugin Protocol \u{2014} JSON-RPC 2.0");
            println!();
            println!("Transport: newline-delimited JSON over stdin/stdout");
            println!("stderr: passed through for debug logging");
            println!();
            println!("HOOKS:");
            println!("  analyze_file     \u{2014} Per-file analysis [add | override]");
            println!("  discover_entries  \u{2014} Entry point discovery [override only]");
            println!("  resolve_import   \u{2014} Import resolution [override only]");
            println!("  post_analysis    \u{2014} After full analysis [add only]");
            println!("  format_output    \u{2014} Custom output formatting [override only]");
            println!();
            println!("LIFECYCLE:");
            println!("  1. statico spawns plugin subprocess");
            println!("  2. Sends 'init' request with project root");
            println!("  3. Plugin responds with capabilities (name, hooks, rules)");
            println!("  4. statico calls hook methods per the declared capabilities");
            println!("  5. statico sends 'shutdown' \u{2014} plugin exits");
            println!();
            println!("Run 'statico plugin schema --format json' for machine-readable schema.");
        }
    }
}

pub fn run_plugin_docs() {
    let docs = r#"statico Plugin Development Guide
================================

Overview
--------
Plugins extend statico's analysis pipeline. They are subprocesses that
communicate via newline-delimited JSON-RPC over stdin/stdout.

Quick Start
-----------
  statico plugin init my-rule --lang typescript   # scaffold
  cd .statico/plugins/my-rule
  # edit index.ts
  statico plugin build --name my-rule
  statico plugin run my-rule --file src/foo.ts

Plugin Types
------------
  typescript  — Bun runs .ts entry point (auto-installs Bun if needed)
  rust        — Compiled binary via cargo
  executable  — Any binary/script that speaks the protocol

Configuration (.statico.toml)
-----------------------------
  [[plugin]]
  name = "my-rule"
  path = "./plugins/my-rule"
  enabled = true
  languages = ["typescript"]
  settings = { max_complexity = 10 }

  [[plugin]]
  name = "acme-fork"
  override = true    # replaces ALL hooks it registers

Hook Modes
----------
  add       — contribute alongside built-in analysis
  override  — replace the built-in stage entirely

  Two plugins cannot override the same hook. statico will error.

Protocol Messages
-----------------
Init:
  → {"method":"init","params":{"root":"/path/to/project"}}
  ← {"result":{"name":"my-plugin","hooks":{"analyze_file":"add"},"rules":[...]}}

Analyze File:
  → {"method":"analyze_file","params":{"path":"src/foo.ts","source":"...","language":"typescript"}}
  ← {"result":{"issues":[...]}}

Shutdown:
  → {"method":"shutdown"}

Full schema: statico plugin schema --format json
"#;
    println!("{}", docs);
}

pub fn run_plugin_init(name: &str, lang: &str, path: &str) {
    // Validate plugin name: only alphanumeric, hyphens, underscores.
    let valid = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid || name.is_empty() {
        eprintln!("Error: invalid plugin name '{}'. Must match ^[a-zA-Z0-9_-]+$", name);
        process::exit(1);
    }

    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: cannot resolve path '{}': {}", path, e);
            process::exit(1);
        }
    };

    let plugin_dir = root.join(".statico/plugins").join(name);
    if plugin_dir.exists() {
        eprintln!("Error: plugin '{}' already exists at {}", name, plugin_dir.display());
        process::exit(1);
    }

    match lang {
        "typescript" | "ts" => scaffold_typescript_plugin(name, &plugin_dir),
        "rust" | "rs" => scaffold_rust_plugin(name, &plugin_dir),
        "python" | "py" => scaffold_python_plugin(name, &plugin_dir),
        other => {
            eprintln!("Error: unsupported language '{}'. Use 'typescript', 'rust', or 'python'.", other);
            process::exit(1);
        }
    }
}

fn scaffold_typescript_plugin(name: &str, dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::create_dir_all(dir.join("fixtures")).unwrap();

    std::fs::write(
        dir.join("package.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "name": name,
            "version": "0.1.0",
            "main": "index.ts",
            "dependencies": {
                "@statico/plugin-sdk": "../../sdks/typescript"
            }
        }))
        .unwrap(),
    )
    .unwrap();

    std::fs::write(
        dir.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true
  },
  "include": ["index.ts"]
}"#,
    )
    .unwrap();

    std::fs::write(
        dir.join("index.ts"),
        format!(
            r#"import {{ Plugin }} from "@statico/plugin-sdk";

const plugin = Plugin.create("{name}", {{
  hooks: {{ analyze_file: "add" }},
  languages: ["typescript"],
  rules: [
    {{ id: "{name}", severity: "warning", description: "TODO: describe your rule" }},
  ],
}});

plugin.onAnalyzeFile((params) => {{
  const issues = [];
  // TODO: implement your detection logic
  // Example: detect console.log
  // if (params.source.includes("console.log")) {{
  //   issues.push({{
  //     ruleId: "{name}",
  //     severity: "warning",
  //     message: "Found console.log",
  //     file: params.path,
  //     line: 1,
  //     confidence: 0.9,
  //   }});
  // }}
  return {{ issues }};
}});

plugin.start();
"#
        ),
    )
    .unwrap();

    std::fs::write(
        dir.join("fixtures").join("sample.ts"),
        "// Test fixture for plugin development\nexport function hello() {\n  console.log('hello');\n}\n",
    )
    .unwrap();

    std::fs::write(
        dir.join("README.md"),
        format!(
            "# {name}\n\nA statico plugin.\n\n## Development\n\n```bash\nstatico plugin run {name} --file fixtures/sample.ts\n```\n\n## Protocol\n\nRun `statico plugin docs` for the full protocol reference.\n"
        ),
    ).unwrap();

    println!("Created TypeScript plugin: {}", dir.display());
    println!("\nNext steps:");
    println!("  cd {}", dir.display());
    println!("  # edit index.ts to implement your rule");
    println!("  statico plugin run {} --file fixtures/sample.ts", name);
}

fn scaffold_rust_plugin(name: &str, dir: &std::path::Path) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("fixtures")).unwrap();

    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            r#"[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\nstatico-plugin-sdk = {{ path = "../../sdks/rust" }}\nserde_json = "1"\n"#
        ),
    ).unwrap();

    std::fs::write(
        dir.join("src").join("main.rs"),
        format!(
            r#"use statico_plugin_sdk::{{Plugin, PluginManifest, HookName, HookMode}};
use std::collections::HashMap;

fn main() {{
    let mut plugin = Plugin::create("{name}", PluginManifest {{
        version: Some("0.1.0".to_string()),
        hooks: HashMap::from([(HookName::AnalyzeFile, HookMode::Add)]),
        languages: vec!["rust".to_string()],
        rules: vec![],
    }});

    plugin.on_analyze_file(|params| {{
        // TODO: implement your detection logic
        statico_plugin_sdk::AnalyzeFileResult::default()
    }});

    plugin.start();
}}
"#
        ),
    )
    .unwrap();

    std::fs::write(
        dir.join("fixtures").join("sample.rs"),
        "// Test fixture for plugin development\nfn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();

    std::fs::write(
        dir.join("README.md"),
        format!(
            "# {name}\n\nA statico Rust plugin.\n\n## Development\n\n```bash\ncargo build --release\nstatico plugin run {name} --file fixtures/sample.rs\n```\n\n## Protocol\n\nRun `statico plugin docs` for the full protocol reference.\n"
        ),
    ).unwrap();

    println!("Created Rust plugin: {}", dir.display());
    println!("\nNext steps:");
    println!("  cd {}", dir.display());
    println!("  cargo build --release");
    println!("  statico plugin run {} --file fixtures/sample.rs", name);
}

fn scaffold_python_plugin(name: &str, dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::create_dir_all(dir.join("fixtures")).unwrap();

    std::fs::write(
        dir.join("package.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "name": name,
            "version": "0.1.0",
            "statico": {
                "runtime": "python3",
                "entry": "plugin.py"
            }
        }))
        .unwrap(),
    )
    .unwrap();

    std::fs::write(
        dir.join("plugin.py"),
        format!(
            r#"#!/usr/bin/env python3
\"\"\"Statico plugin: {name}\"\"\"

import sys
import json

def send_response(result, req_id):
    msg = json.dumps({{"jsonrpc": "2.0", "id": req_id, "result": result}})
    sys.stdout.write(msg + "\n")
    sys.stdout.flush()

def send_error(code, message, req_id):
    msg = json.dumps({{"jsonrpc": "2.0", "id": req_id, "error": {{"code": code, "message": message}}}})
    sys.stdout.write(msg + "\n")
    sys.stdout.flush()

def analyze_file(params):
    source = params.get("source", "")
    path = params.get("path", "")
    issues = []

    # TODO: implement your detection logic
    # Example: detect bare except clauses
    # for i, line in enumerate(source.splitlines(), 1):
    #     if line.strip() == "except:":
    #         issues.append({{
    #             "ruleId": "{name}",
    #             "severity": "warning",
    #             "message": "Bare except clause",
    #             "file": path,
    #             "line": i,
    #             "confidence": 0.9,
    #         }})

    return {{"issues": issues}}

def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue

        method = req.get("method", "")
        req_id = req.get("id")
        params = req.get("params", {{}})

        if method == "init":
            send_response({{
                "name": "{name}",
                "version": "0.1.0",
                "hooks": {{"analyze_file": "add"}},
                "languages": ["python"],
                "rules": [],
            }}, req_id)
        elif method == "analyze_file":
            send_response(analyze_file(params), req_id)
        else:
            send_error(-32601, f"Method not found: {{method}}", req_id)

if __name__ == "__main__":
    main()
"#
        ),
    )
    .unwrap();

    std::fs::write(
        dir.join("fixtures").join("sample.py"),
        "# Test fixture for plugin development\ndef hello():\n    pass\n",
    )
    .unwrap();

    std::fs::write(
        dir.join("README.md"),
        format!(
            "# {name}\n\nA statico Python plugin.\n\n## Development\n\n```bash\nstatico plugin run {name} --file fixtures/sample.py\n```\n\n## Protocol\n\nRun `statico plugin docs` for the full protocol reference.\n"
        ),
    ).unwrap();

    println!("Created Python plugin: {}", dir.display());
    println!("\nNext steps:");
    println!("  cd {}", dir.display());
    println!("  # edit plugin.py to implement your rule");
    println!("  statico plugin run {} --file fixtures/sample.py", name);
}

pub fn run_plugin_build(name: Option<&str>, path: &str) {
    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: cannot resolve path '{}': {}", path, e);
            process::exit(1);
        }
    };

    let plugins = statico::plugin::discovery::discover_plugins(&root);
    let targets: Vec<_> = match name {
        Some(n) => plugins.into_iter().filter(|p| p.name == n).collect(),
        None => plugins,
    };

    if targets.is_empty() {
        if let Some(n) = name {
            eprintln!("Plugin '{}' not found.", n);
            process::exit(1);
        } else {
            println!("No plugins found to build.");
            return;
        }
    }

    for plugin in &targets {
        match plugin.kind {
            statico::plugin::discovery::PluginKind::Rust => {
                print!("Building Rust plugin '{}'... ", statico::strip_ansi::strip_ansi(&plugin.name));
                let output = std::process::Command::new("cargo")
                    .args(["build", "--release"])
                    .current_dir(&plugin.path)
                    .output()
                    .expect("failed to run cargo");
                if output.status.success() {
                    println!("ok");
                } else {
                    println!("FAILED");
                    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                    process::exit(1);
                }
            }
            statico::plugin::discovery::PluginKind::TypeScript => {
                print!(
                    "TypeScript plugin '{}' (no build needed with Bun)... ",
                    statico::strip_ansi::strip_ansi(&plugin.name)
                );
                println!("ok");
            }
            statico::plugin::discovery::PluginKind::Executable => {
                println!(
                    "Skipping executable plugin '{}' (no build step)",
                    statico::strip_ansi::strip_ansi(&plugin.name)
                );
            }
            statico::plugin::discovery::PluginKind::Python => {
                print!("Python plugin '{}' (no build needed)... ", statico::strip_ansi::strip_ansi(&plugin.name));
                println!("ok");
            }
        }
    }
}

pub fn run_plugin_run(name: &str, file: &str, path: &str) {
    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: cannot resolve path '{}': {}", path, e);
            process::exit(1);
        }
    };

    let source_path = root.join(file);

    // Verify the file path is within the project root.
    if let Err(e) = statico::ensure_within_root(&source_path, &root) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }

    let source = match std::fs::read_to_string(&source_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", source_path.display(), e);
            process::exit(1);
        }
    };

    let plugins = statico::plugin::discovery::discover_plugins(&root);
    let plugin = match plugins.into_iter().find(|p| p.name == name) {
        Some(p) => p,
        None => {
            eprintln!("Plugin '{}' not found.", name);
            process::exit(1);
        }
    };

    print!("Starting plugin '{}'... ", name);
    let mut active = match statico::plugin::manager::ActivePlugin::spawn(&plugin, &root) {
        Ok(a) => {
            println!("ok");
            a
        }
        Err(e) => {
            println!("FAILED");
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let lang = std::path::Path::new(file).extension().and_then(|e| e.to_str()).unwrap_or("unknown").to_string();

    let params = statico::plugin::protocol::AnalyzeFileParams {
        path: file.to_string(),
        source,
        language: lang,
        existing_issues: vec![],
    };

    let result: statico::plugin::protocol::AnalyzeFileResult = match active.send_request("analyze_file", &params) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error calling analyze_file: {}", e);
            active.shutdown().ok();
            process::exit(1);
        }
    };

    println!("\nResults:");
    println!("  Issues: {}", result.issues.len());
    for issue in &result.issues {
        println!(
            "    [{}] {} ({}:{})",
            issue.severity.as_ref(),
            statico::strip_ansi::strip_ansi(&issue.message),
            statico::strip_ansi::strip_ansi(&issue.file),
            issue.line
        );
    }
    println!("  Exports: {}", result.exports.len());
    for exp in &result.exports {
        println!("    {}", exp);
    }
    println!("  Dependencies: {}", result.dependencies.len());
    for dep in &result.dependencies {
        println!("    {}", dep);
    }

    active.shutdown().ok();
}

pub fn run_plugin_doctor(path: &str) {
    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: cannot resolve path '{}': {}", path, e);
            process::exit(1);
        }
    };

    println!("statico Plugin Doctor");
    println!("====================\n");

    // Check main binary.
    let bin_ok = super::doctor::which_exists("statico");
    super::doctor::print_status("statico binary", bin_ok);

    // Check runtimes.
    let bun_system = super::doctor::which_exists("bun");
    let bun_managed = statico::plugin::runtime::bun_is_installed();
    let bun_ok = bun_system || bun_managed;
    let bun_label = if bun_system {
        "bun (system)"
    } else if bun_managed {
        "bun (managed)"
    } else {
        "bun (TypeScript plugins)"
    };
    super::doctor::print_status(bun_label, bun_ok);
    if bun_ok
        && let Some(bun_path) = statico::plugin::runtime::find_bun()
        && let Ok(ver) = statico::plugin::runtime::check_bun_version(&bun_path)
    {
        println!("    version: {}", ver);
    }

    let cargo_ok = super::doctor::which_exists("cargo");
    super::doctor::print_status("cargo (Rust plugins)", cargo_ok);
    if cargo_ok {
        let output = std::process::Command::new("cargo").arg("--version").output().ok();
        if let Some(out) = output {
            let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
            println!("    {}", ver);
        }
    }

    // Check managed runtime dir.
    let runtime_dir = dirs::home_dir().unwrap_or_default().join(".statico/runtimes");
    println!("\nRuntime directory: {}", runtime_dir.display());
    if runtime_dir.exists() {
        for e in std::fs::read_dir(&runtime_dir).unwrap_or_else(|_| panic!("read_dir")).flatten() {
            println!("  {}", e.file_name().to_string_lossy());
        }
    } else {
        println!("  (not created yet)");
    }

    // Check for plugins.
    let plugins = statico::plugin::discovery::discover_plugins(&root);
    println!("\nPlugins in {}:", root.display());
    if plugins.is_empty() {
        println!("  (none)");
    } else {
        for p in &plugins {
            let status = if p.enabled { "enabled" } else { "disabled" };
            println!("  {} [{}] ({})", p.name, status, p.kind);
        }
    }

    // Runtime recommendation.
    let has_ts = plugins.iter().any(|p| matches!(p.kind, statico::plugin::discovery::PluginKind::TypeScript));
    let has_rust = plugins.iter().any(|p| matches!(p.kind, statico::plugin::discovery::PluginKind::Rust));

    if has_ts && !bun_ok {
        println!("\nWARNING: TypeScript plugins detected but 'bun' not found.");
        println!("  Install: curl -fsSL https://bun.sh/install | bash");
    }
    if has_rust && !cargo_ok {
        println!("\nWARNING: Rust plugins detected but 'cargo' not found.");
        println!("  Install: https://rustup.rs");
    }

    if (has_ts && bun_ok) || (has_rust && cargo_ok) || (!has_ts && !has_rust) {
        println!("\nAll good!");
    }
}
