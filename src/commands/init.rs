//! `statico init` and `statico setup` commands.

use std::io::Write;
use std::process;

use clap_complete::{generate, Shell};

pub fn run_init(shell: Option<&str>, cli_command: &mut clap::Command) {
    let shell = shell
        .map(|s| s.to_string())
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| "bash".to_string());

    let is_zsh = shell.contains("zsh");
    let _is_bash = shell.contains("bash");
    let is_fish = shell.contains("fish");

    let rc_file = if is_zsh {
        dirs::home_dir().map(|h| h.join(".zshrc"))
    } else if is_fish {
        dirs::home_dir().map(|h| h.join(".config/fish/config.fish"))
    } else {
        dirs::home_dir().map(|h| h.join(".bashrc"))
    };

    let Some(rc_file) = rc_file else {
        eprintln!("error: cannot determine home directory");
        process::exit(1);
    };

    // Ensure data dir exists.
    let data_dir = statico::update::data_dir();
    let completions_dir = data_dir.join("completions");
    std::fs::create_dir_all(&completions_dir).ok();

    // Generate completion file.
    let completion_file = if is_fish {
        completions_dir.join("statico.fish")
    } else {
        completions_dir.join("statico.bash")
    };

    let shell_type = if is_fish {
        Shell::Fish
    } else if is_zsh {
        Shell::Zsh
    } else {
        Shell::Bash
    };

    {
        let mut file = std::fs::File::create(&completion_file)
            .expect("failed to create completion file");
        generate(shell_type, cli_command, "statico", &mut file);
    }

    // Build the rc snippet — PATH + alias + completions.
    let exe = std::env::current_exe().expect("cannot determine current executable");
    let bin_dir = exe.parent().unwrap_or_else(|| std::path::Path::new("/usr/local/bin"));
    let bin_dir_escaped = statico::shell::shell_escape(&bin_dir.display().to_string());
    let completion_escaped = statico::shell::shell_escape(&completion_file.display().to_string());

    let snippet = if is_fish {
        // Fish does not interpret `\$` / `\\`` inside double quotes the way bash does;
        // single-quote the escaped value so paths containing spaces or shell metachars
        // survive intact (audit S4.4).
        let bin_dir_fish = fish_single_quote(&bin_dir.display().to_string());
        let completion_fish = fish_single_quote(&completion_file.display().to_string());
        format!(
            "\n# statico\nset -gx PATH {bin_dir_fish} $PATH\nalias st statico\nsource {completion_fish}\n"
        )
    } else {
        format!(
            "\n# statico\nexport PATH=\"{bin_dir_escaped}:$PATH\"\nalias st='statico'\nsource \"{completion_escaped}\"\n"
        )
    };

    // Check if already configured.
    let rc_content = std::fs::read_to_string(&rc_file).unwrap_or_default();
    if rc_content.contains("# statico") {
        println!("Shell integration already configured in {}", rc_file.display());
        println!("  alias: st='statico'");
        println!("  completions: {}", completion_file.display());
        return;
    }

    // Append to rc file.
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&rc_file)
        .expect("failed to open rc file");
    file.write_all(snippet.as_bytes()).expect("failed to write rc file");

    println!("\x1b[32m✓ Shell integration configured!\x1b[0m");
    println!("  Shell:    {}", if is_zsh { "zsh" } else if is_fish { "fish" } else { "bash" });
    println!("  Config:   {}", rc_file.display());
    println!("  Alias:    st → statico");
    println!("  Complete: {}", completion_file.display());
    println!();
    println!("Restart your shell or run:");
    if is_fish {
        println!("  source {}", rc_file.display());
    } else {
        println!("  source {}", rc_file.display());
    }
}

/// Wrap a string in fish single-quotes, escaping `\` and `'`.
///
/// Inside fish single-quotes only `\\` and `\'` are special — every other
/// character is literal, which makes single-quote wrapping the safest way to
/// embed user-supplied paths in a fish script.
fn fish_single_quote(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{}'", escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sec_fish_single_quote_wraps_normal_path() {
        assert_eq!(fish_single_quote("/usr/local/bin"), "'/usr/local/bin'");
    }

    #[test]
    fn sec_fish_single_quote_handles_path_with_space() {
        assert_eq!(
            fish_single_quote("/Users/Alice Smith/.statico/bin"),
            "'/Users/Alice Smith/.statico/bin'"
        );
    }

    #[test]
    fn sec_fish_single_quote_escapes_single_quote() {
        // path contains a literal single quote (yes, this is legal on POSIX)
        assert_eq!(fish_single_quote("/tmp/o'reilly/bin"), "'/tmp/o\\'reilly/bin'");
    }

    #[test]
    fn sec_fish_single_quote_escapes_backslash() {
        assert_eq!(fish_single_quote(r"C:\stat\bin"), r"'C:\\stat\\bin'");
    }
}

pub fn run_setup(target: &str, path: &str, force: bool) {
    let root = std::path::Path::new(path);
    if !root.exists() {
        eprintln!("error: path {} does not exist", path);
        process::exit(1);
    }

    let generate_all = target == "all";
    let generate_claude = generate_all || target == "claude";
    let generate_cursor = generate_all || target == "cursor";
    let generate_pi = generate_all || target == "pi";

    if !generate_claude && !generate_cursor && !generate_pi {
        eprintln!("error: unknown target '{}'. Use: claude, cursor, pi, or all", target);
        process::exit(1);
    }

    let mut files_written = 0usize;

    // Update .gitignore for user-specific AI config dirs.
    let gitignore = root.join(".gitignore");
    if gitignore.exists() && !gitignore.is_symlink() {
        let content = std::fs::read_to_string(&gitignore).unwrap_or_default();
        let mut additions = Vec::new();
        if generate_claude && !content.lines().any(|l| l == ".claude/") {
            additions.push(".claude/");
        }
        if generate_pi && !content.lines().any(|l| l == ".pi/") {
            additions.push(".pi/");
        }
        if !additions.is_empty() {
            let mut f = std::fs::OpenOptions::new().append(true).open(&gitignore).expect("open .gitignore");
            for a in &additions {
                let _ = f.write_all(format!("\n{}\n", a).as_bytes());
            }
        }
    }

    // --- Claude Code setup ---
    if generate_claude {
        let claude_dir = root.join(".claude");

        let claude_md = claude_dir.join("CLAUDE.md");
        if claude_md.exists() && !force {
            println!("  skipping {} (already exists, use --force to overwrite)", claude_md.display());
        } else {
            std::fs::create_dir_all(&claude_dir).expect("create .claude dir");
            std::fs::write(&claude_md, generate_claude_md()).expect("write CLAUDE.md");
            println!("  wrote {}", claude_md.display());
            files_written += 1;
        }

        files_written += write_skill(&claude_dir.join("skills").join("statico-analyze"), "statico-analyze", generate_skill_analyze(), force);
        files_written += write_skill(&claude_dir.join("skills").join("statico-fix"), "statico-fix", generate_skill_fix(), force);
        files_written += write_skill(&claude_dir.join("skills").join("statico-plugin"), "statico-plugin", generate_skill_plugin(), force);
    }

    // --- Pi setup ---
    if generate_pi {
        let pi_dir = root.join(".pi");
        files_written += write_skill(&pi_dir.join("skills").join("statico-analyze"), "statico-analyze", generate_skill_analyze(), force);
        files_written += write_skill(&pi_dir.join("skills").join("statico-fix"), "statico-fix", generate_skill_fix(), force);
        files_written += write_skill(&pi_dir.join("skills").join("statico-plugin"), "statico-plugin", generate_skill_plugin(), force);
    }

    // --- Cursor setup ---
    if generate_cursor {
        let rules_file = root.join(".cursor").join("rules").join("statico.mdc");
        if rules_file.exists() && !force {
            println!("  skipping {} (already exists)", rules_file.display());
        } else {
            std::fs::create_dir_all(rules_file.parent().unwrap()).expect("create cursor rules dir");
            std::fs::write(&rules_file, generate_cursor_rules()).expect("write cursor rules");
            println!("  wrote {}", rules_file.display());
            files_written += 1;
        }
    }

    if files_written > 0 {
        println!("\n\x1b[32m✓ AI integration set up!\x1b[0m {} file(s) generated.", files_written);
    } else {
        println!("All files already exist. Use --force to overwrite.");
    }
}

/// Write a skill directory with SKILL.md. Returns 1 if written, 0 if skipped.
fn write_skill(dir: &std::path::Path, _name: &str, content: String, force: bool) -> usize {
    let skill_file = dir.join("SKILL.md");
    if skill_file.exists() && !force {
        println!("  skipping {} (already exists)", skill_file.display());
        return 0;
    }
    std::fs::create_dir_all(dir).unwrap_or_else(|_| panic!("create {} dir", dir.display()));
    std::fs::write(&skill_file, &content).unwrap_or_else(|_| panic!("write {}", skill_file.display()));
    println!("  wrote {}", skill_file.display());
    1
}

fn generate_claude_md() -> String {
    format!(r#"# statico

## Project Overview

statico is a static code analyzer for TypeScript and Rust projects. It detects dead code,
unused exports, circular dependencies, code duplication, and framework-specific issues.

## Architecture

- `src/analyzer/` — Core analysis engine with language plugin system
- `src/languages/` — Language plugins (TypeScript, Rust) implementing `LanguagePlugin` trait
- `src/issues/` — Issue detectors (dead code, unused exports, circular deps, gotchas)
- `src/resolution/` — Import resolution (TypeScript paths, tsconfig, Rust mod/crate)
- `src/output/` — Output formatters (JSON, SARIF, Markdown, AI, context, mermaid)
- `src/discovery/` — Entry point discovery (Next.js, Payload CMS, Angular)
- `src/tui/` — Terminal UI dashboard

## Key Commands

```bash
statico analyze .                    # Analyze project
statico analyze . --format markdown  # Markdown output
statico analyze . --format ai        # AI-optimized output
statico analyze . --exit-code        # Exit 1 on issues (CI)
statico diff before.json after.json  # Compare analyses
statico tui .                        # Interactive dashboard
statico doctor                       # Diagnose installation
```

## Output Formats

- `json` — Full structured analysis
- `sarif` — SARIF 2.1.0 for GitHub Code Scanning
- `markdown` — Human-readable report
- `ai` — Compressed format optimized for LLM context windows
- `context` — File-by-file summary with issue locations
- `mermaid` — Dependency graph visualization
- `pr-comment` — GitHub PR review comment
- `fix` — Machine-readable fix suggestions

## Development

```bash
cargo build                           # Build
cargo test                            # Run all tests
cargo test --test integration         # Integration tests
cargo bench                           # Benchmarks
cargo run -- analyze . --format json  # Dev run
```

## Language Plugin System

Adding a new language:
1. Create `src/languages/<lang>.rs` implementing `LanguagePlugin` trait
2. Register extensions in `from_path()` (patterns.rs)
3. Optionally add language-specific rules

No existing code needs modification.

## Plugin System

statico supports external plugins via a subprocess-based JSON-RPC 2.0 protocol.

```bash
statico plugin init my-rule --lang typescript  # Scaffold plugin
statico plugin run my-rule --file test.ts      # Test plugin
statico plugin doctor                          # Check runtimes
statico plugin docs                            # Protocol reference
```

- TypeScript plugins use Bun runtime (auto-downloaded)
- Rust plugins compile via system cargo
- Plugin SDKs: `sdks/typescript/` and `sdks/rust/`
- 5 pipeline hooks: `analyze_file`, `discover_entries`, `resolve_import`, `post_analysis`, `format_output`
"#)
}

fn generate_skill_analyze() -> String {
    r#"---
name: statico-analyze
description: Run statico code analysis on the current project. Use when asked to check code health, find dead code, review code quality, or analyze dependencies.
---

# statico-analyze

## When to Use

- User asks to check code health or code quality
- User wants to find dead code, unused exports, or circular dependencies
- User wants to understand the dependency graph
- Before/after refactoring to measure impact
- CI pipeline code quality gates

## Instructions

1. Run the analysis:
   ```bash
   statico analyze . --format ai
   ```
   The `--format ai` output is compressed for LLM context windows.

2. For detailed issue locations, use:
   ```bash
   statico analyze . --format context
   ```

3. For dependency visualization:
   ```bash
   statico analyze . --format mermaid
   ```

4. Interpret the results:
   - **Health score** (0–100): Overall code health. 80+ is good, 60–80 needs attention, <60 is critical.
   - **Dead code**: Files/exports that nothing references. Safe to remove.
   - **Unused exports**: Exports not imported anywhere. Consider making internal.
   - **Circular dependencies**: Files that import each other. Break with dependency injection or events.
   - **Duplication**: Code blocks duplicated across files. Extract shared utilities.
   - **Confidence** (0.0–1.0): How certain the detector is. Filter with `--min-confidence 0.7`.

5. To compare before/after changes:
   ```bash
   statico analyze . --format json > before.json
   # ... make changes ...
   statico analyze . --format json > after.json
   statico diff before.json after.json
   ```
"#.to_string()
}

fn generate_skill_fix() -> String {
    r#"---
name: statico-fix
description: Fix code quality issues found by statico. Use after running statico analyze to address dead code, unused exports, and other detected issues.
---

# statico-fix

## When to Use

- After running statico analyze and getting issues
- User asks to fix, clean up, or resolve detected code quality problems
- User wants to remove dead code or unused exports

## Instructions

1. First, get the list of issues:
   ```bash
   statico analyze . --format fix
   ```
   This outputs machine-readable fix suggestions.

2. For each issue type:

   ### Dead Code (unreachable files)
   - Verify the file is truly unused (check dynamic imports, config references)
   - Delete the file
   - Remove any related test files

   ### Unused Exports
   - If the export is only used internally, remove the `export` keyword
   - If nothing uses it, remove the entire function/class/constant
   - For TypeScript, also check if the type is used in `.d.ts` files

   ### Circular Dependencies
   - Identify the cycle from the mermaid graph: `statico analyze . --format mermaid`
   - Break the cycle by extracting shared logic to a third file
   - Or use dependency injection / event patterns

   ### Code Duplication
   - Extract duplicated code into a shared utility
   - If the duplication is in tests, consider test helpers

3. After fixing, re-run to verify:
   ```bash
   statico analyze . --format ai
   ```
   Health score should improve.
"#.to_string()
}

fn generate_skill_plugin() -> String {
    r#"# statico Plugin Development

Use when the user wants to create, modify, or debug a statico plugin.

## Quick Start

```bash
# Scaffold a new TypeScript plugin
statico plugin init my-plugin --lang typescript

# Scaffold a new Rust plugin
statico plugin init my-plugin --lang rust

# Run a plugin against a test file
statico plugin run my-plugin --file fixtures/sample.ts

# Check runtime readiness
statico plugin doctor
```

## Plugin Protocol

Plugins communicate via JSON-RPC 2.0 over stdin/stdout (newline-delimited).

### Lifecycle

1. statico spawns the plugin subprocess
2. Sends `init` request → plugin responds with capabilities
3. Sends hook requests (`analyze_file`, `discover_entries`, etc.)
4. Sends `shutdown` → plugin exits

### Hooks

| Hook | When | Mode |
|------|------|------|
| `analyze_file` | Per-file analysis | add |
| `discover_entries` | Find entry points | add |
| `resolve_import` | Resolve import specifiers | override |
| `post_analysis` | After full analysis | add |
| `format_output` | Before displaying results | override |

### Modes

- **add**: Contribute alongside built-in analysis and other plugins
- **override**: Replace the built-in stage entirely (only one plugin per hook)

## TypeScript SDK

```typescript
import { Plugin } from "@statico/plugin-sdk";

const plugin = Plugin.create("my-rule", {
  hooks: { analyze_file: "add" },
  languages: ["typescript"],
  rules: [{ id: "my-rule", severity: "warning", description: "..." }],
});

plugin.onAnalyzeFile((params) => {
  const issues = [];
  // params.path, params.source, params.language
  // Detect patterns in params.source
  return { issues };
});

plugin.start();
```

## Rust SDK

```rust
use statico_plugin_sdk::{Plugin, PluginManifest, HookName, HookMode};
use std::collections::HashMap;

fn main() {
    let mut plugin = Plugin::create("my-rule", PluginManifest {
        version: Some("0.1.0".to_string()),
        hooks: HashMap::from([(HookName::AnalyzeFile, HookMode::Add)]),
        languages: vec!["rust".to_string()],
        rules: vec![],
    });

    plugin.on_analyze_file(|params| {
        // params.path, params.source, params.language
        statico_plugin_sdk::AnalyzeFileResult::default()
    });

    plugin.start();
}
```

## Protocol Reference

Run `statico plugin docs` for the full protocol reference.
Run `statico plugin schema --format json` for the JSON schema.
"#.to_string()
}

fn generate_cursor_rules() -> String {
    format!(r#"---
description: statico code analysis rules and patterns for the AI assistant
---

# statico Code Quality

## Commands

- `statico analyze . --format ai` — Analyze project (AI-optimized output)
- `statico analyze . --format fix` — Get fix suggestions
- `statico analyze . --format mermaid` — Dependency graph
- `statico diff before.json after.json` — Compare analyses
- `statico plugin init <name> --lang typescript` — Create a plugin
- `statico plugin run <name> --file <path>` — Test a plugin
- `statico plugin doctor` — Check runtime readiness
- `statico plugin docs` — Full protocol reference

## Issue Types

1. **Dead code**: Files nothing imports. Safe to delete after verification.
2. **Unused exports**: Exports never imported. Make internal or remove.
3. **Circular dependencies**: Files importing each other. Break with extraction.
4. **Code duplication**: Duplicated blocks. Extract shared utilities.
5. **Framework gotchas**: Next.js, Payload, Angular anti-patterns.

## Workflow

When the user asks about code quality:
1. Run `statico analyze . --format ai`
2. Summarize the health score and top issues
3. For each issue category, explain what to fix and why
4. After fixes, re-run to verify improvement

## Health Score Guide

- 80–100: Good shape
- 60–79: Needs attention (plan cleanup)
- 0–59: Critical (prioritize fixes)
"#)
}
