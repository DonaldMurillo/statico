//! statico CLI — Static code analyzer for TypeScript projects.

use clap::Parser;
use statico::output::OutputFormatter;
use std::process;

/// Static code analyzer for TypeScript projects.
#[derive(Parser)]
#[command(name = "statico", version, about = "Static code analyzer for TypeScript projects")]
struct Cli {
    /// Suppress progress output.
    #[arg(long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Analyze a TypeScript project.
    Analyze {
        /// Path to the TypeScript project directory.
        path: String,

        /// Output format: json, sarif, markdown, html, ai, context, mermaid, pr-comment, fix.
        /// Defaults to config file value or "json".
        #[arg(long)]
        format: Option<String>,

        /// Minimum confidence threshold (0.0–1.0) for filtering issues.
        /// Defaults to config file value or 0.0.
        #[arg(long)]
        min_confidence: Option<f64>,

        /// Exit with code 1 if issues are found above min-confidence.
        #[arg(long)]
        exit_code: bool,
    },

    /// Show interactive terminal dashboard.
    Tui {
        /// Path to the TypeScript project directory.
        path: String,

        /// Minimum confidence threshold (0.0–1.0).
        #[arg(long, default_value_t = 0.5)]
        min_confidence: f64,
    },

    /// Compare two analysis outputs.
    Diff {
        /// Path to the before.json file.
        before: String,

        /// Path to the after.json file.
        after: String,

        /// Output format: json, markdown.
        #[arg(long, default_value = "json")]
        format: String,
    },
}

fn main() {
    let cli = Cli::parse();
    let quiet = cli.quiet;

    match cli.command {
        Commands::Analyze { path, format, min_confidence, exit_code } => {
            run_analyze(&path, format.as_deref(), min_confidence, exit_code, quiet);
        }
        Commands::Tui { path, min_confidence } => {
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
        Commands::Diff { before, after, format } => {
            run_diff(&before, &after, &format);
        }
    }
}

fn run_analyze(path: &str, format: Option<&str>, min_confidence: Option<f64>, exit_code: bool, quiet: bool) {
    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(_) => root.to_path_buf(),
    };

    // Load config from project root and merge with CLI args.
    let config = statico::config::StaticoConfig::load(&root)
        .merge_cli(format, min_confidence, exit_code, quiet);

    if !config.quiet && !config.exclude.is_empty() {
        eprintln!("info: exclude patterns: {:?}", config.exclude);
    }
    if !config.quiet && !config.include.is_empty() {
        eprintln!("info: include patterns: {:?}", config.include);
    }

    let output = match statico::analyzer::analyze_with_excludes(&root, &config.exclude) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("error: {}", msg);
            process::exit(1);
        }
    };

    // Apply confidence filter if threshold > 0.
    let filtered = if config.min_confidence > 0.0 {
        statico::output::filter_by_confidence(&output, config.min_confidence)
    } else {
        output
    };

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
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path, e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("failed to parse {}: {}", path, e))
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
