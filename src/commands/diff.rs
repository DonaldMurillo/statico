//! `statico diff` command.

use std::process;

pub fn run_diff(before_path: &str, after_path: &str, format: &str) {
    let before = super::analyze::load_analysis(before_path);
    let after = super::analyze::load_analysis(after_path);

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
