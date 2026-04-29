//! Terminal UI mode — interactive dashboard for analysis results.

use colored::*;
use indicatif::{ProgressBar, ProgressStyle};

use crate::types::AnalysisOutput;

/// Run the TUI dashboard for a project at `root`.
pub fn run_tui(root: &std::path::Path, min_confidence: f64) -> Result<(), String> {
    // Respect terminal vs pipe: only colourise when stdout is a tty.
    colored::control::set_override(atty::is(atty::Stream::Stdout));

    // 1. Progress spinner while analysing.
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .map_err(|e| e.to_string())?,
    );
    pb.set_message("Analyzing project…");
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    let output = crate::analyzer::analyze(root)?;
    pb.finish_with_message("done".green().to_string());

    // 2. Dashboard sections.
    let summary = crate::output::compute_summary(&output);
    let frameworks = crate::output::detect_framework_names(&output);

    print_header(root);
    print_summary_cards(&summary);
    print_issues(&output, &summary.issue_counts, min_confidence);
    print_top_issues(&output, min_confidence);
    print_duplication(&output);
    print_frameworks(&frameworks);

    Ok(())
}

// ---------------------------------------------------------------------------
// Section printers
// ---------------------------------------------------------------------------

fn print_header(root: &std::path::Path) {
    let version = env!("CARGO_PKG_VERSION");
    println!();
    println!(
        "  {} {}  {}",
        "statico".bold().white(),
        format!("v{}", version).dimmed(),
        root.display().to_string().cyan()
    );
    println!("  {}\n", "─".repeat(60).dimmed());
}

fn print_summary_cards(summary: &crate::types::Summary) {
    let score = summary.health_score;
    let score_color = if score >= 80.0 {
        "green"
    } else if score >= 50.0 {
        "yellow"
    } else {
        "red"
    };

    let dup = summary.duplication_percentage;
    let dup_color = if dup < 10.0 {
        "green"
    } else if dup < 20.0 {
        "yellow"
    } else {
        "red"
    };

    println!("  {}  {}", "Total Files:".dimmed(), summary.total_files.to_string().cyan());
    println!("  {}  {}", "Total Lines:".dimmed(), summary.total_lines.to_string().cyan());
    println!(
        "  {}  {}/100",
        "Health Score:".dimmed(),
        format!("{:.1}", score).color(score_color)
    );
    println!(
        "  {}  {}%",
        "Duplication:".dimmed(),
        format!("{:.1}", dup).color(dup_color)
    );
    println!();
}

fn print_issues(
    output: &AnalysisOutput,
    counts: &crate::types::IssueCounts,
    min_confidence: f64,
) {
    println!("  {}", "Issues by Category".bold());
    println!("  {}\n", "─".repeat(40).dimmed());

    // Count items above confidence threshold.
    let dead: usize = output
        .issues
        .dead_code
        .iter()
        .filter(|i| i.confidence >= min_confidence)
        .count();
    let gotchas: usize = output
        .issues
        .gotchas
        .iter()
        .filter(|i| i.confidence >= min_confidence)
        .count();

    print_category_line("🔴", "Dead Code", dead);
    print_category_line("🟡", "Unused Exports", counts.unused_exports);
    print_category_line("🟠", "Circular Dependencies", counts.circular_dependencies);
    print_category_line("🔵", "Gotchas", gotchas);
    print_category_line("🟣", "Unused Types", counts.unused_types);
    print_category_line("⚪", "Unused Dependencies", counts.unused_dependencies);
    print_category_line("🟤", "Duplicate Code", counts.duplicate_code);
    print_category_line("🔺", "Unresolved Imports", counts.unresolved_imports);
    println!();
}

fn print_category_line(icon: &str, label: &str, count: usize) {
    let count_str = if count == 0 {
        count.to_string().green().to_string()
    } else {
        count.to_string().yellow().to_string()
    };
    println!("  {} {:<25} {}", icon, label, count_str);
}

fn print_top_issues(output: &AnalysisOutput, min_confidence: f64) {
    println!("  {}", "Top Issues".bold());
    println!("  {}\n", "─".repeat(40).dimmed());

    let mut issues: Vec<(String, String, f64)> = Vec::new();

    for dc in &output.issues.dead_code {
        if dc.confidence >= min_confidence {
            issues.push((dc.path.clone(), "dead code".into(), dc.confidence));
        }
    }
    for g in &output.issues.gotchas {
        if g.confidence >= min_confidence {
            issues.push((g.file.clone(), format!("{}: {}", g.rule, g.message), g.confidence));
        }
    }
    for ue in output.issues.unused_exports.iter().take(10) {
        issues.push((ue.path.clone(), format!("unused export '{}'", ue.name), 0.7));
    }

    // Sort by confidence descending, take top 10.
    issues.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    issues.truncate(10);

    if issues.is_empty() {
        println!("  {}", "No issues found! 🎉".green());
        println!();
        return;
    }

    // Table header.
    println!(
        "  {:<40} {:<30} {}",
        "File".dimmed(),
        "Issue".dimmed(),
        "Confidence".dimmed()
    );

    for (file, issue, conf) in &issues {
        let short_file = shorten_path(file, 38);
        let short_issue = shorten_str(issue, 28);
        let conf_str = format!("{:.0}%", conf * 100.0);
        let conf_colored = if *conf >= 0.8 {
            conf_str.red().to_string()
        } else if *conf >= 0.5 {
            conf_str.yellow().to_string()
        } else {
            conf_str.normal().to_string()
        };
        println!("  {:<40} {:<30} {}", short_file, short_issue, conf_colored);
    }
    println!();
}

fn print_duplication(output: &AnalysisOutput) {
    let stats = &output.duplication.stats;
    println!(
        "  {} {} clone groups, {} clone instances, {:.1}% duplication",
        "Duplication:".dimmed(),
        stats.clone_groups.to_string().cyan(),
        stats.clone_instances.to_string().cyan(),
        stats.duplication_percentage
    );
}

fn print_frameworks(frameworks: &[String]) {
    if frameworks.is_empty() {
        return;
    }
    println!(
        "  {} {}",
        "Frameworks:".dimmed(),
        frameworks.join(", ").cyan()
    );
    println!();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn shorten_path(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    // Keep filename and show "…" prefix.
    let truncate_to = max_len - 1;
    format!("…{}", &s[s.len() - truncate_to..])
}

fn shorten_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    format!("{}…", &s[..max_len - 1])
}
