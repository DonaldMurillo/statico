//! statico CLI — Static code analyzer for TypeScript and Rust projects.

use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use statico::output::OutputFormatter;
use std::process;

static VERSION_FULL: &str = concat!(env!("CARGO_PKG_VERSION"));

fn version_with_git() -> &'static str {
    // git-version resolves at compile time but returns a &str.
    // We format it into a leaked Box<str> to get a 'static reference.
    // This is fine since it's called at most once.
    let git = git_version::git_version!(fallback = "unknown");
    if git == "unknown" {
        VERSION_FULL
    } else {
        Box::leak(format!("{} ({})", VERSION_FULL, git).into_boxed_str())
    }
}

/// Static code analyzer for TypeScript and Rust projects.
///
/// Detects dead code, unused exports, circular dependencies, and other
/// issues in TypeScript codebases. Supports multiple output formats
/// including JSON, SARIF, Markdown, HTML, and AI-friendly formats.
use clap::builder::styling::{AnsiColor, Styles};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default())
    .placeholder(AnsiColor::Cyan.on_default())
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Yellow.on_default());

#[derive(Parser)]
#[command(
    name = "statico",
    version = version_with_git(),
    about = "Static code analyzer for TypeScript and Rust projects",
    long_about = "Static code analyzer for TypeScript and Rust projects.\n\n\
        Detects dead code, unused exports, circular dependencies, and other \
        quality issues. Supports multiple output formats for CI integration, \
        code review, and AI-assisted workflows.\n\n\
        Quick start:\n\
          statico analyze .           # Analyze current directory\n\
          statico analyze . --format markdown  # Markdown output\n\
          statico update              # Self-update to latest version\n\
          statico init                # Set up shell alias & completions",
    arg_required_else_help = true,
    styles = STYLES,
    help_template = "{name} {version}\n\n{about-with-newline}{usage-heading} {usage}\n\n{all-args}{after-help}",
)]
struct Cli {
    /// Suppress non-essential output (progress bars, info messages).
    ///
    /// Useful in CI pipelines or when piping output to other commands.
    #[arg(long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Analyze a project for code quality issues.
    Analyze {
        /// Path to the project directory.
        #[arg(default_value = ".")]
        path: String,

        /// Output format for analysis results.
        ///
        /// Supported: json, sarif, markdown, html, ai, context, mermaid, pr-comment, fix.
        /// Defaults to config file value or "json".
        #[arg(long, value_name = "FORMAT")]
        format: Option<String>,

        /// Minimum confidence threshold (0.0–1.0) for filtering issues.
        ///
        /// Only report issues with confidence at or above this value.
        /// Defaults to config file value or 0.0 (show all).
        #[arg(long, value_name = "THRESHOLD")]
        min_confidence: Option<f64>,

        /// Exit with code 1 if any issues are found above the confidence threshold.
        ///
        /// Useful for CI pipelines where you want builds to fail on code quality issues.
        #[arg(long)]
        exit_code: bool,
    },

    /// Show an interactive terminal dashboard for exploring analysis results.
    Tui {
        /// Path to the project directory.
        #[arg(default_value = ".")]
        path: String,

        /// Minimum confidence threshold (0.0–1.0) for filtering displayed issues.
        #[arg(long, default_value_t = 0.5, value_name = "THRESHOLD")]
        min_confidence: f64,
    },

    /// Compare two analysis outputs and show the differences.
    ///
    /// Useful for tracking code quality changes between commits or branches.
    Diff {
        /// Path to the baseline (before) analysis JSON file.
        before: String,

        /// Path to the current (after) analysis JSON file.
        after: String,

        /// Output format for the diff results.
        ///
        /// Supported: json, markdown. Defaults to "json".
        #[arg(long, default_value = "json", value_name = "FORMAT")]
        format: String,
    },

    /// Generate shell completion scripts for statico.
    ///
    /// Print completions to stdout. Redirect to the appropriate file for your shell.
    /// Example: statico completions bash > /etc/bash_completion.d/statico
    Completions {
        /// The shell to generate completions for.
        #[arg(value_name = "SHELL")]
        shell: Shell,
    },

    /// Update statico to the latest version.
    ///
    /// Checks GitHub releases for a newer version and performs an in-place
    /// binary update. No sudo required.
    Update {
        /// Only check for updates, don't install.
        #[arg(long)]
        check: bool,
    },

    /// Set up shell integration (alias, completions, PATH).
    ///
    /// Installs the `st` alias, shell completions, and ensures statico
    /// is on your PATH. Run this once after installing.
    Init {
        /// Shell to configure (auto-detected if not specified).
        #[arg(long, value_name = "SHELL")]
        shell: Option<String>,
    },

    /// Diagnose common installation issues.
    ///
    /// Checks PATH, shell integration, binary location, and version.
    Doctor,

    /// Set up AI integration for the current project.
    ///
    /// Generates skills for Claude Code, pi, and Cursor so AI assistants
    /// can run statico analysis and understand the results.
    Setup {
        /// What to generate.
        ///
        /// - claude: .claude/ directory with skills + CLAUDE.md
        /// - pi: .pi/skills/ with SKILL.md files
        /// - cursor: .cursor/rules with statico context
        /// - all: everything (default)
        #[arg(long, default_value = "all", value_name = "TARGET")]
        target: String,

        /// Project path (defaults to current directory).
        #[arg(long, default_value = ".")]
        path: String,

        /// Overwrite existing files without prompting.
        #[arg(long)]
        force: bool,
    },

    /// Manage statico plugins.
    ///
    /// Discover, inspect, and manage plugins that extend statico's
    /// analysis pipeline. Plugins communicate via JSON-RPC over stdin/stdout.
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
}

#[derive(clap::Subcommand)]
enum PluginAction {
    /// List discovered plugins and their status.
    List {
        /// Project path (defaults to current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },

    /// Print the JSON schema for the plugin protocol.
    ///
    /// Useful for LLMs and plugin developers to understand the exact contract.
    Schema {
        /// Output format (text or json).
        #[arg(long, default_value = "text", value_name = "FORMAT")]
        format: String,
    },

    /// Print the full plugin protocol reference documentation.
    ///
    /// Human-readable guide to building plugins. Covers all hooks,
    /// message types, and lifecycle.
    Docs,

    /// Scaffold a new plugin project.
    ///
    /// Creates a plugin directory in .statico/plugins/ with all
    /// necessary files to get started.
    Init {
        /// Plugin name (used as directory name).
        name: String,

        /// Plugin language: typescript, rust, or python.
        #[arg(long, default_value = "typescript", value_name = "LANG")]
        lang: String,

        /// Project path (defaults to current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },

    /// Build one or all plugins.
    ///
    /// Compiles Rust plugins via cargo, bundles TypeScript plugins via bun.
    Build {
        /// Build only this plugin (by name).
        #[arg(long, value_name = "NAME")]
        name: Option<String>,

        /// Project path (defaults to current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },

    /// Run a single plugin against a file.
    ///
    /// Spawns the plugin, sends init + analyze_file, prints the result.
    /// Useful for development and debugging.
    Run {
        /// Plugin name to run.
        name: String,

        /// File to analyze.
        #[arg(long, value_name = "FILE")]
        file: String,

        /// Project path (defaults to current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },

    /// Check runtime readiness for plugin development.
    ///
    /// Verifies Bun (for TS plugins) and cargo (for Rust plugins)
    /// are available and reports any issues.
    Doctor {
        /// Project path (defaults to current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },
}

fn main() {
    let cli = Cli::parse();
    let quiet = cli.quiet;

    // Non-blocking version check (rate-limited to once per day).
    if !quiet {
        statico::update::check_and_notify();
    }

    match cli.command {
        Commands::Analyze {
            path,
            format,
            min_confidence,
            exit_code,
        } => {
            run_analyze(&path, format.as_deref(), min_confidence, exit_code, quiet);
        }
        Commands::Tui {
            path,
            min_confidence,
        } => {
            let root = std::path::Path::new(&path);
            let root = match std::fs::canonicalize(root) {
                Ok(c) => c,
                Err(_) => root.to_path_buf(),
            };
            if let Err(e) = statico::tui::run_tui(&root, min_confidence) {
                eprintln!("error: {}", e);
                process::exit(1);
            }
        }
        Commands::Diff {
            before,
            after,
            format,
        } => {
            run_diff(&before, &after, &format);
        }
        Commands::Completions { shell } => {
            generate(shell, &mut Cli::command(), "statico", &mut std::io::stdout());
        }
        Commands::Update { check } => {
            run_update(check);
        }
        Commands::Init { shell } => {
            run_init(shell.as_deref());
        }
        Commands::Doctor => {
            run_doctor();
        }
        Commands::Setup {
            target,
            path,
            force,
        } => {
            run_setup(&target, &path, force);
        }
        Commands::Plugin { action } => {
            match action {
                PluginAction::List { path } => run_plugin_list(&path),
                PluginAction::Schema { format } => run_plugin_schema(&format),
                PluginAction::Docs => run_plugin_docs(),
                PluginAction::Init { name, lang, path } => run_plugin_init(&name, &lang, &path),
                PluginAction::Build { name, path } => run_plugin_build(name.as_deref(), &path),
                PluginAction::Run { name, file, path } => run_plugin_run(&name, &file, &path),
                PluginAction::Doctor { path } => run_plugin_doctor(&path),
            }
        }
    }
}

fn run_analyze(
    path: &str,
    format: Option<&str>,
    min_confidence: Option<f64>,
    exit_code: bool,
    quiet: bool,
) {
    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: cannot resolve path '{}': {}", path, e);
            process::exit(1);
        }
    };

    // Load config from project root and merge with CLI args.
    let config =
        statico::config::StaticoConfig::load(&root).merge_cli(format, min_confidence, exit_code, quiet);

    if !config.quiet && !config.exclude.is_empty() {
        eprintln!("info: exclude patterns: {:?}", config.exclude);
    }
    if !config.quiet && !config.include.is_empty() {
        eprintln!("info: include patterns: {:?}", config.include);
    }

    // Initialize plugin pipeline.
    let mut plugin_pipeline = statico::plugin::PluginPipeline::new(&root);
    if !config.quiet && !plugin_pipeline.is_empty() {
        eprintln!("info: {} plugin(s) active", plugin_pipeline.len());
    }

    let mut output = match statico::analyzer::analyze_with_excludes(&root, &config.exclude) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("error: {}", msg);
            process::exit(1);
        }
    };

    // Plugin per-file analysis: analyze_file hook.
    // This is the primary hook for plugins that detect per-file issues.
    if !plugin_pipeline.is_empty() {
        let source_files = &output.structure.source_files;
        let root_path = std::path::Path::new(&root);
        for file_entry in source_files {
            let rel_path = &file_entry.path;
            let abs_path = root_path.join(rel_path);
                // Skip files exceeding max_file_size (S3-04).
            let file_size = match std::fs::metadata(&abs_path) {
                Ok(m) => m.len(),
                Err(_) => continue,
            };
            if file_size > config.max_file_size {
                continue;
            }
            let source = match std::fs::read_to_string(&abs_path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let language = &file_entry.language;
            let results = plugin_pipeline.analyze_file(rel_path, &source, language);
            for result in results {
                if !result.issues.is_empty() {
                    output.issues.plugin_issues.extend(result.issues);
                }
            }
        }
    }

    // Plugin post_analysis hook.
    if !plugin_pipeline.is_empty() {
        let plugin_results = plugin_pipeline.post_analysis(&output);
        if !config.quiet && !plugin_results.is_empty() {
            eprintln!("info: plugins contributed {} post-analysis results", plugin_results.len());
        }
    }

    // Apply confidence filter if threshold > 0.
    let filtered = if config.min_confidence > 0.0 {
        statico::output::filter_by_confidence(&output, config.min_confidence)
    } else {
        output
    };

    // Plugin format_output hook — if any plugin handles it, skip built-in formatting.
    if let Some(plugin_output) = plugin_pipeline.format_output(&filtered, &config.format) {
        println!("{}", plugin_output);
    } else {
        let result = match config.format.as_str() {
            "json" => statico::output::json_enriched::EnrichedJsonFormatter.format(&filtered),
            "sarif" => statico::output::sarif::SarifFormatter.format(&filtered),
            "markdown" | "md" => statico::output::markdown::MarkdownFormatter.format(&filtered),
            "html" => statico::output::html::HtmlFormatter.format(&filtered),
            "ai" => statico::output::ai::AiFormatter.format(&filtered),
            "context" => statico::output::context::ContextFormatter.format(&filtered),
            "mermaid" => statico::output::mermaid::MermaidFormatter.format(&filtered),
            "pr-comment" | "pr_comment" => statico::output::pr_comment::PrCommentFormatter.format(&filtered),
            "fix" => statico::output::fix::FixFormatter.format(&filtered),
            other => Err(format!(
                "unknown format: '{}'. Use json, sarif, markdown, html, ai, context, mermaid, pr-comment, or fix.",
                other
            )),
        };

        match result {
            Ok(text) => println!("{}", text),
            Err(e) => {
                eprintln!("error: {}", e);
                process::exit(1);
            }
        }
    }

    // Exit code logic: exit 1 if there are issues above threshold.
    if config.exit_code && has_issues_above_confidence(&filtered, config.min_confidence) {
        process::exit(1);
    }
}

fn run_diff(before_path: &str, after_path: &str, format: &str) {
    let before = load_analysis(before_path);
    let after = load_analysis(after_path);

    let (before, after) = match (before, after) {
        (Ok(b), Ok(a)) => (b, a),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    };

    let diff = statico::output::diff::compute_diff(&before, &after);

    let result = match format {
        "json" => statico::output::diff::format_diff_json(&diff),
        "markdown" | "md" => statico::output::diff::format_diff_markdown(&diff),
        other => Err(format!("unknown format: '{}'. Use json or markdown.", other)),
    };

    match result {
        Ok(text) => println!("{}", text),
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    }

    // Exit 1 if there are new issues.
    if !diff.new_issues.is_empty() {
        process::exit(1);
    }
}

fn load_analysis(path: &str) -> Result<statico::types::AnalysisOutput, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("failed to read {}: {}", path, e))?;
    serde_json::from_str(&content).map_err(|e| format!("failed to parse {}: {}", path, e))
}

/// Check if there are any significant issues in the output.
fn has_issues_above_confidence(output: &statico::types::AnalysisOutput, _min_confidence: f64) -> bool {
    let issues = &output.issues;
    !issues.dead_code.is_empty()
        || !issues.unused_exports.is_empty()
        || !issues.circular_dependencies.is_empty()
        || !issues.gotchas.is_empty()
        || !issues.unresolved_imports.is_empty()
        || !issues.duplicate_exports.is_empty()
}

// ---------------------------------------------------------------------------
// Self-update
// ---------------------------------------------------------------------------

fn run_update(check_only: bool) {
    match statico::update::run_update(check_only) {
        Ok(msg) => println!("{}", msg),
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Shell init (alias + completions + PATH)
// ---------------------------------------------------------------------------

fn run_init(shell: Option<&str>) {
    use std::io::Write;

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
        generate(shell_type, &mut Cli::command(), "statico", &mut file);
    }

    // Build the rc snippet — PATH + alias + completions.
    let exe = std::env::current_exe().expect("cannot determine current executable");
    let bin_dir = exe.parent().unwrap_or_else(|| std::path::Path::new("/usr/local/bin"));
    let bin_dir_str = bin_dir.display();

    let snippet = if is_fish {
        format!(
            "\n# statico\nset -gx PATH {bin_dir_str} $PATH\nalias st statico\nsource {}\n",
            completion_file.display()
        )
    } else if is_zsh {
        format!(
            "\n# statico\nexport PATH=\"{bin_dir_str}:$PATH\"\nalias st='statico'\nsource {}\n",
            completion_file.display()
        )
    } else {
        format!(
            "\n# statico\nexport PATH=\"{bin_dir_str}:$PATH\"\nalias st='statico'\nsource {}\n",
            completion_file.display()
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

// ---------------------------------------------------------------------------
// Doctor — diagnose installation
// ---------------------------------------------------------------------------
// AI setup — generate skills and context for AI assistants
// ---------------------------------------------------------------------------

fn run_setup(target: &str, path: &str, force: bool) {
    use std::io::Write;

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
    if gitignore.exists() {
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

        // CLAUDE.md — project-level context
        let claude_md = claude_dir.join("CLAUDE.md");
        if claude_md.exists() && !force {
            println!("  skipping {} (already exists, use --force to overwrite)", claude_md.display());
        } else {
            std::fs::create_dir_all(&claude_dir).expect("create .claude dir");
            std::fs::write(&claude_md, generate_claude_md()).expect("write CLAUDE.md");
            println!("  wrote {}", claude_md.display());
            files_written += 1;
        }

        // Skill: .claude/skills/statico-analyze/SKILL.md
        files_written += write_skill(&claude_dir.join("skills").join("statico-analyze"), "statico-analyze", generate_skill_analyze(), force);

        // Skill: .claude/skills/statico-fix/SKILL.md
        files_written += write_skill(&claude_dir.join("skills").join("statico-fix"), "statico-fix", generate_skill_fix(), force);

        // Skill: .claude/skills/statico-plugin/SKILL.md
        files_written += write_skill(&claude_dir.join("skills").join("statico-plugin"), "statico-plugin", generate_skill_plugin(), force);
    }

    // --- Pi setup ---
    if generate_pi {
        let pi_dir = root.join(".pi");

        // Skill: .pi/skills/statico-analyze/SKILL.md
        files_written += write_skill(&pi_dir.join("skills").join("statico-analyze"), "statico-analyze", generate_skill_analyze(), force);

        // Skill: .pi/skills/statico-fix/SKILL.md
        files_written += write_skill(&pi_dir.join("skills").join("statico-fix"), "statico-fix", generate_skill_fix(), force);

        // Skill: .pi/skills/statico-plugin/SKILL.md
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

// ---------------------------------------------------------------------------

fn run_doctor() {
    let mut ok = true;

    println!("statico doctor — checking installation...\n");

    // 1. Binary location.
    let exe = std::env::current_exe().unwrap_or_default();
    println!("  Binary:   {}", exe.display());

    // 2. Version.
    let version = env!("CARGO_PKG_VERSION");
    let git = git_version::git_version!(fallback = "unknown");
    println!("  Version:  v{} ({})", version, git);

    // 3. PATH check.
    let in_path = which_statico();
    match in_path {
        Some(p) => println!("  PATH:     {} \x1b[32m✓\x1b[0m", p.display()),
        None => {
            println!("  PATH:     \x1b[31m✗ statico not found on PATH\x1b[0m");
            ok = false;
        }
    }

    // 4. Shell alias.
    let alias_check = std::process::Command::new("alias")
        .arg("st")
        .output();
    let alias_ok = alias_check
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("statico") || String::from_utf8_lossy(&o.stdout).contains("st"))
        .unwrap_or(false);
    if alias_ok {
        println!("  Alias:    st='statico' \x1b[32m✓\x1b[0m");
    } else {
        println!("  Alias:    \x1b[33mst alias not set (run `statico init`)\x1b[0m");
    }

    // 5. Completions.
    let data_dir = statico::update::data_dir();
    let completions = data_dir.join("completions");
    if completions.exists() {
        println!("  Complete: {} \x1b[32m✓\x1b[0m", completions.display());
    } else {
        println!("  Complete: \x1b[33mnot installed (run `statico init`)\x1b[0m");
    }

    // 6. Update check.
    let last_version = data_dir.join("last-version");
    if last_version.exists() {
        let cached = std::fs::read_to_string(&last_version).unwrap_or_default();
        let status = if statico::update::is_newer(version, &cached) {
            format!("\x1b[33mv{} available\x1b[0m", cached.trim())
        } else {
            "\x1b[32mup to date\x1b[0m".to_string()
        };
        println!("  Updates:  {}", status);
    } else {
        println!("  Updates:  \x1b[33mnever checked (run `statico update --check`)\x1b[0m");
    }

    println!();
    if ok {
        println!("\x1b[32mAll checks passed.\x1b[0m");
    } else {
        println!("\x1b[33mSome issues found. Run `statico init` to set up shell integration.\x1b[0m");
    }
}

/// Find statico on PATH.
fn which_statico() -> Option<std::path::PathBuf> {
    let path_env = std::env::var("PATH").unwrap_or_default();
    for dir in path_env.split(':') {
        let candidate = std::path::Path::new(dir).join("statico");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn run_plugin_list(path: &str) {
    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(_) => root.to_path_buf(),
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

fn run_plugin_schema(format: &str) {
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

fn run_plugin_docs() {
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

fn run_plugin_init(name: &str, lang: &str, path: &str) {
    // Validate plugin name: only alphanumeric, hyphens, underscores.
    let valid = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid || name.is_empty() {
        eprintln!(
            "Error: invalid plugin name '{}'. Must match ^[a-zA-Z0-9_-]+$",
            name
        );
        std::process::exit(1);
    }

    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(_) => root.to_path_buf(),
    };

    let plugin_dir = root.join(".statico/plugins").join(name);
    if plugin_dir.exists() {
        eprintln!("Error: plugin '{}' already exists at {}", name, plugin_dir.display());
        std::process::exit(1);
    }

    match lang {
        "typescript" | "ts" => scaffold_typescript_plugin(name, &plugin_dir),
        "rust" | "rs" => scaffold_rust_plugin(name, &plugin_dir),
        "python" | "py" => scaffold_python_plugin(name, &plugin_dir),
        other => {
            eprintln!("Error: unsupported language '{}'. Use 'typescript', 'rust', or 'python'.", other);
            std::process::exit(1);
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
    ).unwrap();

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
    ).unwrap();

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
    ).unwrap();

    std::fs::write(
        dir.join("fixtures").join("sample.ts"),
        "// Test fixture for plugin development\nexport function hello() {\n  console.log('hello');\n}\n",
    ).unwrap();

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
    ).unwrap();

    std::fs::write(
        dir.join("fixtures").join("sample.rs"),
        "// Test fixture for plugin development\nfn main() {\n    println!(\"hello\");\n}\n",
    ).unwrap();

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
    ).unwrap();

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
    ).unwrap();

    std::fs::write(
        dir.join("fixtures").join("sample.py"),
        "# Test fixture for plugin development\ndef hello():\n    pass\n",
    ).unwrap();

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

fn run_plugin_build(name: Option<&str>, path: &str) {
    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(_) => root.to_path_buf(),
    };

    let plugins = statico::plugin::discovery::discover_plugins(&root);
    let targets: Vec<_> = match name {
        Some(n) => plugins.into_iter().filter(|p| &p.name == n).collect(),
        None => plugins,
    };

    if targets.is_empty() {
        if name.is_some() {
            eprintln!("Plugin '{}' not found.", name.unwrap());
            std::process::exit(1);
        } else {
            println!("No plugins found to build.");
            return;
        }
    }

    for plugin in &targets {
        match plugin.kind {
            statico::plugin::discovery::PluginKind::Rust => {
                print!("Building Rust plugin '{}'... ", plugin.name);
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
                    std::process::exit(1);
                }
            }
            statico::plugin::discovery::PluginKind::TypeScript => {
                print!("TypeScript plugin '{}' (no build needed with Bun)... ", plugin.name);
                println!("ok");
            }
            statico::plugin::discovery::PluginKind::Executable => {
                println!("Skipping executable plugin '{}' (no build step)", plugin.name);
            }
            statico::plugin::discovery::PluginKind::Python => {
                print!("Python plugin '{}' (no build needed)... ", plugin.name);
                println!("ok");
            }
        }
    }
}

fn run_plugin_run(name: &str, file: &str, path: &str) {
    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(_) => root.to_path_buf(),
    };

    let source_path = root.join(file);

    // Verify the file path is within the project root.
    if let Err(e) = statico::ensure_within_root(&source_path, &root) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    let source = match std::fs::read_to_string(&source_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", source_path.display(), e);
            std::process::exit(1);
        }
    };

    let plugins = statico::plugin::discovery::discover_plugins(&root);
    let plugin = match plugins.into_iter().find(|p| p.name == name) {
        Some(p) => p,
        None => {
            eprintln!("Plugin '{}' not found.", name);
            std::process::exit(1);
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
            std::process::exit(1);
        }
    };

    let lang = std::path::Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("unknown")
        .to_string();

    let params = statico::plugin::protocol::AnalyzeFileParams {
        path: file.to_string(),
        source,
        language: lang,
        existing_issues: vec![],
    };

    let result: statico::plugin::protocol::AnalyzeFileResult =
        match active.send_request("analyze_file", &params) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error calling analyze_file: {}", e);
                active.shutdown().ok();
                std::process::exit(1);
            }
        };

    println!("\nResults:");
    println!("  Issues: {}", result.issues.len());
    for issue in &result.issues {
        println!("    [{}] {} ({}:{})", issue.severity.as_ref(), issue.message, issue.file, issue.line);
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

fn run_plugin_doctor(path: &str) {
    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(_) => root.to_path_buf(),
    };

    println!("statico Plugin Doctor");
    println!("====================\n");

    // Check main binary.
    let bin_ok = which_exists("statico");
    print_status("statico binary", bin_ok);

    // Check runtimes.
    let bun_system = which_exists("bun");
    let bun_managed = statico::plugin::runtime::bun_is_installed();
    let bun_ok = bun_system || bun_managed;
    let bun_label = if bun_system {
        "bun (system)"
    } else if bun_managed {
        "bun (managed)"
    } else {
        "bun (TypeScript plugins)"
    };
    print_status(bun_label, bun_ok);
    if bun_ok {
        if let Some(bun_path) = statico::plugin::runtime::find_bun() {
            if let Ok(ver) = statico::plugin::runtime::check_bun_version(&bun_path) {
                println!("    version: {}", ver);
            }
        }
    }

    let cargo_ok = which_exists("cargo");
    print_status("cargo (Rust plugins)", cargo_ok);
    if cargo_ok {
        let output = std::process::Command::new("cargo").arg("--version").output().ok();
        if let Some(out) = output {
            let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
            println!("    {}", ver);
        }
    }

    // Check managed runtime dir.
    let runtime_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".statico/runtimes");
    println!("\nRuntime directory: {}", runtime_dir.display());
    if runtime_dir.exists() {
        for entry in std::fs::read_dir(&runtime_dir).unwrap_or_else(|_| panic!("read_dir")) {
            if let Ok(e) = entry {
                println!("  {}", e.file_name().to_string_lossy());
            }
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

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn print_status(label: &str, ok: bool) {
    let mark = if ok { "\u{2713}" } else { "\u{2717}" };
    println!("  {} {}", mark, label);
}
