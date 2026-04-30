//! Compact LLM-optimized JSON formatter.
//!
//! Produces a schema-versioned JSON payload stripped of verbose details,
//! designed to fit within ~500 tokens while remaining actionable.

use crate::output::{OutputFormatter, compute_summary};
use crate::types::AnalysisOutput;
use serde_json::{Value, json};

/// Compact LLM-optimized JSON formatter (`--format ai`).
pub struct AiFormatter;

impl OutputFormatter for AiFormatter {
    fn format(&self, output: &AnalysisOutput) -> Result<String, String> {
        let summary = compute_summary(output);

        let top_issues = build_top_issues(output);
        let files_at_risk = build_files_at_risk(output);

        let payload = json!({
            "schema": "statico-ai-v1",
            "summary": {
                "health_score": summary.health_score,
                "total_files": summary.total_files,
                "total_lines": summary.total_lines,
                "issue_counts": {
                    "dead_code": summary.issue_counts.dead_code,
                    "unused_exports": summary.issue_counts.unused_exports,
                    "unused_types": summary.issue_counts.unused_types,
                    "duplicate_code": summary.issue_counts.duplicate_code,
                    "gotchas": summary.issue_counts.gotchas,
                    "circular_dependencies": summary.issue_counts.circular_dependencies,
                }
            },
            "top_issues": top_issues,
            "files_at_risk": files_at_risk,
        });

        serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())
    }
}

/// A unified issue entry for the AI payload, sorted by impact.
struct AiIssue {
    category: &'static str,
    file: String,
    line: Option<usize>,
    impact: f64,
    confidence: f64,
    suggested_action: &'static str,
    reason: String,
}

fn build_top_issues(output: &AnalysisOutput) -> Vec<Value> {
    let mut issues: Vec<AiIssue> = Vec::new();

    // Dead code: impact = LOC wasted
    for dc in &output.issues.dead_code {
        issues.push(AiIssue {
            category: "dead_code",
            file: dc.path.clone(),
            line: None,
            impact: dc.lines_of_code as f64,
            confidence: dc.confidence,
            suggested_action: if dc.confidence >= 0.9 { "safe-to-delete" } else { "investigate" },
            reason: dc.reason.clone(),
        });
    }

    // Unused exports: impact = 1 per export, sort by file grouping later
    for ue in &output.issues.unused_exports {
        issues.push(AiIssue {
            category: "unused_exports",
            file: ue.path.clone(),
            line: None,
            impact: 1.0,
            confidence: 0.8,
            suggested_action: "remove",
            reason: format!("export '{}' is never imported", ue.name),
        });
    }

    // Unused types
    for ut in &output.issues.unused_types {
        issues.push(AiIssue {
            category: "unused_types",
            file: ut.path.clone(),
            line: None,
            impact: 1.0,
            confidence: 0.8,
            suggested_action: "remove",
            reason: format!("{} '{}' is never imported", ut.kind, ut.name),
        });
    }

    // Duplicate code: impact = duplicated lines
    for dup in &output.issues.duplicate_code {
        let loc = (dup.location_a.end_line - dup.location_a.start_line + 1) as f64;
        issues.push(AiIssue {
            category: "duplicate_code",
            file: dup.location_a.file.clone(),
            line: Some(dup.location_a.start_line),
            impact: loc,
            confidence: dup.confidence,
            suggested_action: "investigate",
            reason: format!(
                "duplicates {} (L{}-L{})",
                dup.location_b.file, dup.location_b.start_line, dup.location_b.end_line
            ),
        });
    }

    // Gotchas: impact = confidence
    for g in &output.issues.gotchas {
        issues.push(AiIssue {
            category: "gotchas",
            file: g.file.clone(),
            line: Some(g.line),
            impact: g.confidence,
            confidence: g.confidence,
            suggested_action: "investigate",
            reason: format!("{}: {}", g.rule, g.message),
        });
    }

    // Circular deps: impact = chain length
    for cd in &output.issues.circular_dependencies {
        let chain = cd.files.join(" → ");
        issues.push(AiIssue {
            category: "circular_dependencies",
            file: cd.files.first().cloned().unwrap_or_default(),
            line: None,
            impact: cd.files.len() as f64,
            confidence: 1.0,
            suggested_action: "investigate",
            reason: format!("cycle: {}", chain),
        });
    }

    // Sort by impact descending
    issues.sort_by(|a, b| b.impact.partial_cmp(&a.impact).unwrap_or(std::cmp::Ordering::Equal));

    issues.truncate(20);

    issues
        .into_iter()
        .map(|i| {
            let mut obj = json!({
                "category": i.category,
                "file": i.file,
                "impact": i.impact,
                "confidence": (i.confidence * 100.0).round() / 100.0,
                "suggested_action": i.suggested_action,
                "reason": i.reason,
            });
            if let Some(line) = i.line {
                obj.as_object_mut().unwrap().insert("line".to_string(), Value::Number(line.into()));
            }
            obj
        })
        .collect()
}

/// Files with the most issues, with counts per category.
fn build_files_at_risk(output: &AnalysisOutput) -> Vec<Value> {
    use std::collections::HashMap;

    #[derive(Default)]
    struct FileStats {
        dead_code: usize,
        unused_exports: usize,
        unused_types: usize,
        gotchas: usize,
        duplicate_code: usize,
        circular_dependencies: usize,
    }

    let mut file_stats: HashMap<String, FileStats> = HashMap::new();

    for dc in &output.issues.dead_code {
        file_stats.entry(dc.path.clone()).or_default().dead_code += 1;
    }
    for ue in &output.issues.unused_exports {
        file_stats.entry(ue.path.clone()).or_default().unused_exports += 1;
    }
    for ut in &output.issues.unused_types {
        file_stats.entry(ut.path.clone()).or_default().unused_types += 1;
    }
    for g in &output.issues.gotchas {
        file_stats.entry(g.file.clone()).or_default().gotchas += 1;
    }
    for dup in &output.issues.duplicate_code {
        file_stats.entry(dup.location_a.file.clone()).or_default().duplicate_code += 1;
    }
    for cd in &output.issues.circular_dependencies {
        for f in &cd.files {
            file_stats.entry(f.clone()).or_default().circular_dependencies += 1;
        }
    }

    let mut entries: Vec<(String, FileStats)> = file_stats.into_iter().collect();
    entries.sort_by(|a, b| {
        let total_a = a.1.dead_code + a.1.unused_exports + a.1.unused_types + a.1.gotchas;
        let total_b = b.1.dead_code + b.1.unused_exports + b.1.unused_types + b.1.gotchas;
        total_b.cmp(&total_a)
    });
    entries.truncate(10);

    entries
        .into_iter()
        .map(|(file, stats)| {
            json!({
                "file": file,
                "dead_code": stats.dead_code,
                "unused_exports": stats.unused_exports,
                "unused_types": stats.unused_types,
                "gotchas": stats.gotchas,
                "duplicate_code": stats.duplicate_code,
                "circular_dependencies": stats.circular_dependencies,
            })
        })
        .collect()
}
