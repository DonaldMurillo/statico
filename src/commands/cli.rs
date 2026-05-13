//! CLI argument definitions and dispatch.

use clap::builder::styling::{AnsiColor, Styles};
use clap::{CommandFactory, Parser};
use clap_complete::{Shell, generate};
use std::process;

static VERSION_FULL: &str = concat!(env!("CARGO_PKG_VERSION"));

fn version_with_git() -> &'static str {
    let git = git_version::git_version!(fallback = "unknown");
    if git == "unknown" { VERSION_FULL } else { Box::leak(format!("{} ({})", VERSION_FULL, git).into_boxed_str()) }
}

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default())
    .placeholder(AnsiColor::Cyan.on_default())
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Yellow.on_default());

/// Static code analyzer for TypeScript and Rust projects.
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
pub struct Cli {
    /// Suppress non-essential output (progress bars, info messages).
    ///
    /// Useful in CI pipelines or when piping output to other commands.
    #[arg(long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand)]
pub enum Commands {
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

        /// Disable incremental cache. Forces a full re-parse of all files.
        ///
        /// By default, statico caches parsed results keyed by file content hash.
        /// Unchanged files are served from cache on subsequent runs, making
        /// warm analysis ~5-10x faster than cold.
        #[arg(long)]
        no_cache: bool,

        /// Filter out issues whose fingerprints are listed in this baseline file.
        ///
        /// Use --update-baseline to write a new baseline. Combined with --exit-code
        /// this lets teams ratchet down issues over time without each PR drowning in
        /// pre-existing noise.
        #[arg(long, value_name = "PATH")]
        baseline: Option<String>,

        /// Write the current set of issues as a baseline file at this path,
        /// then exit. Existing baselines at that path are overwritten atomically.
        #[arg(long, value_name = "PATH", conflicts_with = "baseline")]
        update_baseline: Option<String>,

        /// Re-run analysis whenever a source file changes. Press Ctrl-C to exit.
        ///
        /// Uses the existing incremental cache so successive runs only re-parse
        /// files that actually changed. Ignored when used with --update-baseline.
        #[arg(long)]
        watch: bool,
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

    /// Apply safe automated fixes to a project.
    ///
    /// Supported transforms (each opt-in via flags, all on by default):
    ///   --unused-exports  Drop the `export` keyword from declarations whose
    ///                     export is unused (only well-formed
    ///                     const/let/var/function/class/type/interface).
    ///   --unused-deps     Remove unused entries from package.json
    ///                     dependencies / devDependencies / peerDependencies
    ///                     / optionalDependencies.
    ///
    /// Default mode is dry-run; pass --apply to actually edit files.
    Fix {
        /// Project path (defaults to current directory).
        #[arg(default_value = ".")]
        path: String,

        /// Actually rewrite files. Without this flag, statico prints the
        /// fixes it *would* make and exits 0.
        #[arg(long)]
        apply: bool,

        /// Apply the unused-exports fix.
        #[arg(long, default_value_t = true)]
        unused_exports: bool,

        /// Skip the unused-exports fix (overrides --unused-exports).
        #[arg(long, conflicts_with = "unused_exports")]
        no_unused_exports: bool,

        /// Apply the unused-deps fix.
        #[arg(long, default_value_t = true)]
        unused_deps: bool,

        /// Skip the unused-deps fix (overrides --unused-deps).
        #[arg(long, conflicts_with = "unused_deps")]
        no_unused_deps: bool,
    },

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

    /// Protect critical files from unintended modification.
    ///
    /// Maintains a guard manifest (.statico/guard.json) with SHA-256 hashes
    /// of registered files. Use `check` in CI or pre-commit hooks to verify
    /// that guarded files haven't been tampered with.
    ///
    /// Quick start:
    ///   statico guard add src/config.rs     # Register a file
    ///   statico guard check --exit-code     # Verify (fails on mismatch)
    ///   statico guard update                # Re-hash after intentional change
    Guard {
        #[command(subcommand)]
        action: GuardAction,
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
pub enum GuardAction {
    /// Register files for integrity protection.
    ///
    /// Computes SHA-256 hashes and adds them to the guard manifest.
    /// The manifest is stored in .statico/guard.json and should be committed.
    Add {
        /// Files to guard (relative to project root).
        files: Vec<String>,

        /// Optional description for all added files.
        #[arg(long, value_name = "MSG")]
        description: Option<String>,

        /// Project path (defaults to current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },

    /// Remove files from the guard manifest.
    Remove {
        /// Files to remove from guarding.
        files: Vec<String>,

        /// Project path (defaults to current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },

    /// List all guarded files and their hashes.
    List {
        /// Project path (defaults to current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },

    /// Verify guarded files match their recorded hashes.
    ///
    /// Use --exit-code to fail (exit 1) if any file was modified.
    /// Ideal for CI pipelines and pre-commit hooks.
    Check {
        /// Exit with code 1 if any guarded file was modified or is missing.
        #[arg(long)]
        exit_code: bool,

        /// Project path (defaults to current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },

    /// Re-hash files after intentional changes.
    ///
    /// Without explicit files, updates all guarded files.
    Update {
        /// Specific files to re-hash (defaults to all guarded files).
        files: Vec<String>,

        /// Project path (defaults to current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },
}

#[derive(clap::Subcommand)]
pub enum PluginAction {
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

/// Parse argv and dispatch to the matching command implementation.
pub fn parse_and_dispatch() {
    let cli = Cli::parse();
    let quiet = cli.quiet;

    // Non-blocking version check (rate-limited to once per day).
    if !quiet {
        statico::update::check_and_notify();
    }

    match cli.command {
        Commands::Analyze { path, format, min_confidence, exit_code, no_cache, baseline, update_baseline, watch } => {
            super::analyze::run_analyze(
                &path,
                format.as_deref(),
                min_confidence,
                exit_code,
                quiet,
                no_cache,
                baseline.as_deref(),
                update_baseline.as_deref(),
                watch,
            );
        }
        Commands::Tui { path, min_confidence } => {
            let root = std::path::Path::new(&path);
            let root = match std::fs::canonicalize(root) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("error: cannot resolve path '{}': {}", path, e);
                    process::exit(1);
                }
            };
            if let Err(e) = statico::tui::run_tui(&root, min_confidence) {
                eprintln!("error: {}", e);
                process::exit(1);
            }
        }
        Commands::Diff { before, after, format } => {
            super::diff::run_diff(&before, &after, &format);
        }
        Commands::Completions { shell } => {
            generate(shell, &mut Cli::command(), "statico", &mut std::io::stdout());
        }
        Commands::Update { check } => {
            super::self_update::run_update(check);
        }
        Commands::Init { shell } => {
            super::init::run_init(shell.as_deref(), &mut Cli::command());
        }
        Commands::Doctor => {
            super::doctor::run_doctor();
        }
        Commands::Fix { path, apply, unused_exports, no_unused_exports, unused_deps, no_unused_deps } => {
            let selection = super::fix::FixSelection {
                unused_exports: unused_exports && !no_unused_exports,
                unused_deps: unused_deps && !no_unused_deps,
            };
            super::fix::run_fix(&path, apply, selection);
        }
        Commands::Guard { action } => match action {
            GuardAction::Add { files, description, path } => {
                super::guard::run_guard_add(&files, description.as_deref(), &path);
            }
            GuardAction::Remove { files, path } => {
                super::guard::run_guard_remove(&files, &path);
            }
            GuardAction::List { path } => {
                super::guard::run_guard_list(&path);
            }
            GuardAction::Check { exit_code, path } => {
                super::guard::run_guard_check(&path, exit_code);
            }
            GuardAction::Update { files, path } => {
                super::guard::run_guard_update(&files, &path);
            }
        },
        Commands::Setup { target, path, force } => {
            super::init::run_setup(&target, &path, force);
        }
        Commands::Plugin { action } => match action {
            PluginAction::List { path } => super::plugin::run_plugin_list(&path),
            PluginAction::Schema { format } => super::plugin::run_plugin_schema(&format),
            PluginAction::Docs => super::plugin::run_plugin_docs(),
            PluginAction::Init { name, lang, path } => super::plugin::run_plugin_init(&name, &lang, &path),
            PluginAction::Build { name, path } => super::plugin::run_plugin_build(name.as_deref(), &path),
            PluginAction::Run { name, file, path } => super::plugin::run_plugin_run(&name, &file, &path),
            PluginAction::Doctor { path } => super::plugin::run_plugin_doctor(&path),
        },
    }
}
