//! `statico analyze` command.

use std::process;

pub fn run_analyze(
    path: &str,
    format: Option<&str>,
    min_confidence: Option<f64>,
    exit_code: bool,
    quiet: bool,
    no_cache: bool,
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

    let mut output = match statico::analyzer::analyze_with_options(&root, &config.exclude, no_cache) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("error: {}", msg);
            process::exit(1);
        }
    };

    // Plugin per-file analysis: analyze_file hook.
    if !plugin_pipeline.is_empty() {
        let source_files = &output.structure.source_files;
        let root_path = std::path::Path::new(&root);
        for file_entry in source_files {
            let rel_path = &file_entry.path;
            let abs_path = root_path.join(rel_path);
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
            for mut result in results {
                if !result.issues.is_empty() {
                    for issue in &mut result.issues {
                        issue.file = issue.file.chars()
                            .filter(|c| !c.is_control())
                            .collect::<String>();
                        if issue.file.starts_with('/') || issue.file.starts_with("..") {
                            issue.file = issue.file
                                .trim_start_matches('/')
                            .trim_start_matches("../")
                            .to_string();
                        }
                    }
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
        use statico::output::OutputFormatter;

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

pub fn load_analysis(path: &str) -> Result<statico::types::AnalysisOutput, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("failed to read {}: {}", path, e))?;
    serde_json::from_str(&content).map_err(|e| format!("failed to parse {}: {}", path, e))
}

/// Check if there are any significant issues in the output.
pub fn has_issues_above_confidence(output: &statico::types::AnalysisOutput, _min_confidence: f64) -> bool {
    let issues = &output.issues;
    !issues.dead_code.is_empty()
        || !issues.unused_exports.is_empty()
        || !issues.circular_dependencies.is_empty()
        || !issues.gotchas.is_empty()
        || !issues.unresolved_imports.is_empty()
        || !issues.duplicate_exports.is_empty()
}
