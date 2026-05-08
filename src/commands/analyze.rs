//! `statico analyze` command.

use std::process;

#[allow(clippy::too_many_arguments)]
pub fn run_analyze(
    path: &str,
    format: Option<&str>,
    min_confidence: Option<f64>,
    exit_code: bool,
    quiet: bool,
    no_cache: bool,
    baseline: Option<&str>,
    update_baseline: Option<&str>,
    watch: bool,
) {
    let root = std::path::Path::new(path);
    let root = match std::fs::canonicalize(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: cannot resolve path '{}': {}", path, e);
            process::exit(1);
        }
    };

    if watch && update_baseline.is_some() {
        eprintln!("error: --watch is incompatible with --update-baseline");
        process::exit(1);
    }

    if watch {
        run_watch_loop(&root, path, format, min_confidence, exit_code, quiet, no_cache, baseline);
        return;
    }

    run_analyze_inner(&root, format, min_confidence, exit_code, quiet, no_cache, baseline, update_baseline);
}

/// Watch loop: re-run `run_analyze_once` whenever a source file under `root`
/// changes. Debounces bursts (e.g. saves from editors that touch many files)
/// to ~150ms.
#[allow(clippy::too_many_arguments)]
fn run_watch_loop(
    root: &std::path::Path,
    raw_path: &str,
    format: Option<&str>,
    min_confidence: Option<f64>,
    exit_code: bool,
    quiet: bool,
    no_cache: bool,
    baseline: Option<&str>,
) {
    use notify::{RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    eprintln!("statico: watching {} (Ctrl-C to exit)", root.display());

    // Run once up front so users see the initial state.
    run_analyze_inner(root, format, min_confidence, exit_code, quiet, no_cache, baseline, None);

    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher = match notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    }) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("error: cannot start file watcher: {}", e);
            process::exit(1);
        }
    };
    if let Err(e) = watcher.watch(root, RecursiveMode::Recursive) {
        eprintln!("error: cannot watch {}: {}", root.display(), e);
        process::exit(1);
    }

    let debounce = Duration::from_millis(150);
    loop {
        let event = match rx.recv() {
            Ok(Ok(ev)) => ev,
            Ok(Err(e)) => {
                eprintln!("watch error: {}", e);
                continue;
            }
            Err(_) => break, // sender dropped
        };
        if !event_is_relevant(&event, root) {
            continue;
        }

        // Debounce: drain any further events that arrive within the window.
        let deadline = Instant::now() + debounce;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match rx.recv_timeout(remaining) {
                Ok(_) => continue,
                Err(_) => break,
            }
        }

        eprintln!("\nstatico: change detected in {} — re-analyzing…", raw_path);
        run_analyze_inner(root, format, min_confidence, exit_code, quiet, no_cache, baseline, None);
    }
}

/// Filter notify events to those that should trigger a re-analyze.
/// Skips events inside skipped dirs (node_modules, .git, etc) and non-source
/// extensions to keep CPU low on busy projects.
fn event_is_relevant(event: &notify::Event, root: &std::path::Path) -> bool {
    use notify::EventKind;
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {}
        _ => return false,
    }
    for path in &event.paths {
        // Skip events under any directory we don't analyze.
        let mut skipped = false;
        for ancestor in path.ancestors().take_while(|a| *a != root) {
            if statico::discovery::is_skipped_dir(ancestor) {
                skipped = true;
                break;
            }
        }
        if skipped {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if matches!(ext, "ts" | "tsx" | "js" | "jsx" | "rs" | "py" | "json" | "toml") {
            return true;
        }
    }
    false
}

/// Inner body of `run_analyze` minus the watch / update-baseline branches.
/// Extracted so the watch loop can call it without duplicating logic.
#[allow(clippy::too_many_arguments)]
fn run_analyze_inner(
    root: &std::path::Path,
    format: Option<&str>,
    min_confidence: Option<f64>,
    exit_code: bool,
    quiet: bool,
    no_cache: bool,
    baseline: Option<&str>,
    update_baseline: Option<&str>,
) {
    // Load config from project root and merge with CLI args.
    let mut config = statico::config::StaticoConfig::load(root).merge_cli(format, min_confidence, exit_code, quiet);

    if format.is_none() && config.format == "json" && std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        config.format = "markdown".to_string();
    }

    if !config.quiet && !config.exclude.is_empty() {
        eprintln!("info: exclude patterns: {:?}", config.exclude);
    }
    if !config.quiet && !config.include.is_empty() {
        eprintln!("info: include patterns: {:?}", config.include);
    }

    let plugin_pipeline = std::sync::Arc::new(std::sync::Mutex::new(statico::plugin::PluginPipeline::new(root)));
    {
        let pipeline = plugin_pipeline.lock().unwrap();
        if !config.quiet && !pipeline.is_empty() {
            eprintln!("info: {} plugin(s) active", pipeline.len());
        }
    }

    // Wire plugin resolve_import hook so plugins can override import resolution
    // during the analysis phase (e.g. tsconfig path mappings, custom aliases).
    {
        let pipeline_clone = std::sync::Arc::clone(&plugin_pipeline);
        let root_owned = root.to_path_buf();
        statico::resolution::set_plugin_resolver(Box::new(move |from_dir: &std::path::Path, spec: &str| {
            let mut pipeline = pipeline_clone.lock().unwrap();
            let from_file = from_dir.to_string_lossy();
            pipeline.resolve_import(&from_file, spec, &root_owned.to_string_lossy()).and_then(|r| {
                if r.external {
                    return None;
                }
                let path = std::path::PathBuf::from(&r.resolved_path);
                if path.is_absolute() && path.exists() {
                    Some(path)
                } else {
                    let abs = root_owned.join(&r.resolved_path);
                    if abs.exists() { Some(abs) } else { None }
                }
            })
        }));
    }

    // Clear the plugin resolver hook — analysis is done, we don't want
    // stale closures hanging around (especially in watch mode).
    statico::resolution::clear_plugin_resolver();

    let mut output = match statico::analyzer::analyze_with_options(root, &config.exclude, no_cache) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return; // in watch mode we want to keep the loop alive
        }
    };

    {
        let mut pipeline = plugin_pipeline.lock().unwrap();
        if !pipeline.is_empty() {
            let source_files = &output.structure.source_files;
            for file_entry in source_files {
                let rel_path = &file_entry.path;
                let abs_path = root.join(rel_path);
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
                let results = pipeline.analyze_file(rel_path, &source, language);
                for mut result in results {
                    if !result.issues.is_empty() {
                        for issue in &mut result.issues {
                            issue.file = issue.file.chars().filter(|c| !c.is_control()).collect::<String>();
                            if issue.file.starts_with('/') || issue.file.starts_with("..") {
                                issue.file = issue.file.trim_start_matches('/').trim_start_matches("../").to_string();
                            }
                        }
                        output.issues.plugin_issues.extend(result.issues);
                    }
                }
            }
        }
    }

    {
        let mut pipeline = plugin_pipeline.lock().unwrap();
        if !pipeline.is_empty() {
            let plugin_results = pipeline.post_analysis(&output);
            if !config.quiet && !plugin_results.is_empty() {
                eprintln!("info: plugins contributed {} post-analysis results", plugin_results.len());
            }
        }
    }

    if let Some(out_path) = update_baseline {
        let pre_baseline = if config.min_confidence > 0.0 {
            statico::output::filter_by_confidence(&output, config.min_confidence)
        } else {
            output.clone()
        };
        let bl = statico::baseline::Baseline::from_output(&pre_baseline);
        if let Err(e) = bl.write(std::path::Path::new(out_path)) {
            eprintln!("error: {}", e);
            return;
        }
        if !config.quiet {
            eprintln!("info: wrote baseline with {} fingerprints to {}", bl.len(), out_path);
        }
        return;
    }

    if let Some(in_path) = baseline {
        match statico::baseline::Baseline::load(std::path::Path::new(in_path)) {
            Ok(bl) => {
                let suppressed = bl.apply(&mut output);
                if !config.quiet {
                    eprintln!("info: baseline {} suppressed {} known issue(s)", in_path, suppressed);
                }
            }
            Err(e) => {
                eprintln!("error: {}", e);
                return;
            }
        }
    }

    let filtered = if config.min_confidence > 0.0 {
        statico::output::filter_by_confidence(&output, config.min_confidence)
    } else {
        output
    };

    if let Some(plugin_output) = plugin_pipeline.lock().unwrap().format_output(&filtered, &config.format) {
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

    if config.exit_code && has_issues_above_confidence(&filtered, config.min_confidence) {
        process::exit(1);
    }
}

pub fn load_analysis(path: &str) -> Result<statico::types::AnalysisOutput, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("failed to read {}: {}", path, e))?;
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
