//! SARIF 2.1.0 output formatter.
//!
//! Maps each issue to a SARIF result with rule metadata, locations, and severity.

use crate::output::OutputFormatter;
use crate::types::AnalysisOutput;
use serde_json::{Value, json};

/// Sanitize a file path for use as a SARIF artifactLocation URI.
/// Removes control characters and normalizes backslashes to forward slashes.
fn sanitize_uri(path: &str) -> String {
    path.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .replace('\\', "/")
}

/// Sanitize a message string for SARIF message.text fields.
/// Strips control characters (except common whitespace) to prevent
/// injection of misleading content via file paths and names.
fn sanitize_message(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else if c.is_control() { ' ' } else { c })
        .collect()
}

/// SARIF 2.1.0 formatter.
pub struct SarifFormatter;

impl OutputFormatter for SarifFormatter {
    fn format(&self, output: &AnalysisOutput) -> Result<String, String> {
        let mut results = Vec::new();
        let mut rules = Vec::new();

        // Dead code
        rules.push(make_rule("dead_code", "Dead code", "File is not reachable from any entry point.", "warning"));
        for dc in &output.issues.dead_code {
            results.push(json!({
                "ruleId": "dead_code",
                "level": if dc.confidence >= 0.8 { "warning" } else { "note" },
                "message": { "text": sanitize_message(&dc.reason) },
                "locations": [{ "physicalLocation": {
                    "artifactLocation": { "uri": sanitize_uri(&dc.path) },
                    "region": { "startLine": 1 }
                }}],
                "properties": { "confidence": dc.confidence, "lines_of_code": dc.lines_of_code }
            }));
        }

        // Unused exports
        rules.push(make_rule("unused_export", "Unused export", "Export is never imported by any file.", "note"));
        for ue in &output.issues.unused_exports {
            results.push(json!({
                "ruleId": "unused_export",
                "level": "note",
                "message": { "text": sanitize_message(&format!("Export '{}' is never imported", ue.name)) },
                "locations": [{ "physicalLocation": {
                    "artifactLocation": { "uri": sanitize_uri(&ue.path) },
                    "region": { "startLine": 1 }
                }}],
            }));
        }

        // Unused types
        rules.push(make_rule("unused_type", "Unused type", "Type/interface is exported but never imported.", "note"));
        for ut in &output.issues.unused_types {
            results.push(json!({
                "ruleId": "unused_type",
                "level": "note",
                "message": { "text": sanitize_message(&format!("{} '{}' is never imported", ut.kind, ut.name)) },
                "locations": [{ "physicalLocation": {
                    "artifactLocation": { "uri": sanitize_uri(&ut.path) },
                    "region": { "startLine": 1 }
                }}],
            }));
        }

        // Duplicate code
        rules.push(make_rule(
            "duplicate_code",
            "Duplicate code",
            "Similar code blocks found in multiple locations.",
            "note",
        ));
        append_dup_code_results(&mut results, output);

        // Gotchas
        rules.push(make_rule("gotcha", "Gotcha", "Common error-prone pattern detected.", "warning"));
        append_gotcha_results(&mut results, output);

        // Circular dependencies
        rules.push(make_rule(
            "circular_dependency",
            "Circular dependency",
            "Circular import chain detected.",
            "warning",
        ));
        append_circular_dep_results(&mut results, output);

        // Duplicate exports
        rules.push(make_rule(
            "duplicate_export",
            "Duplicate export",
            "Same export name defined in multiple files.",
            "warning",
        ));
        for de in &output.issues.duplicate_exports {
            results.push(json!({
                "ruleId": "duplicate_export",
                "level": "warning",
                "message": { "text": sanitize_message(&format!("Export '{}' defined in {} locations", de.name, de.locations.len())) },
            }));
        }

        // Unresolved imports
        rules.push(make_rule(
            "unresolved_import",
            "Unresolved import",
            "Import could not be resolved to a file.",
            "warning",
        ));
        for ui in &output.issues.unresolved_imports {
            results.push(json!({
                "ruleId": "unresolved_import",
                "level": "warning",
                "message": { "text": sanitize_message(&format!("Unresolved import '{}' in {}", ui.import_spec, ui.source_file)) },
                "locations": [{ "physicalLocation": {
                    "artifactLocation": { "uri": sanitize_uri(&ui.source_file) },
                    "region": { "startLine": 1 }
                }}],
            }));
        }

        // Unused / unlisted dependencies
        rules.push(make_rule(
            "unused_dependency",
            "Unused dependency",
            "Package listed in package.json but never imported.",
            "note",
        ));
        for ud in &output.issues.unused_dependencies {
            results.push(json!({
                "ruleId": "unused_dependency",
                "level": "note",
                "message": { "text": sanitize_message(&format!("Package '{}' is listed but never imported", ud.package_name)) },
            }));
        }

        rules.push(make_rule(
            "unlisted_dependency",
            "Unlisted dependency",
            "External import not in package.json.",
            "warning",
        ));
        for ud in &output.issues.unlisted_dependencies {
            results.push(json!({
                "ruleId": "unlisted_dependency",
                "level": "warning",
                "message": { "text": sanitize_message(&format!("'{}' imported by {} but not in package.json", ud.package_name, ud.imported_by)) },
            }));
        }

        let sarif = json!({
            "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "statico",
                        "version": env!("CARGO_PKG_VERSION"),
                        "rules": rules,
                    }
                },
                "results": results,
            }]
        });

        serde_json::to_string_pretty(&sarif).map_err(|e| format!("failed to serialize SARIF: {}", e))
    }
}

fn make_rule(id: &str, name: &str, desc: &str, default_level: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "shortDescription": { "text": desc },
        "defaultConfiguration": { "level": default_level },
    })
}

fn append_dup_code_results(results: &mut Vec<Value>, output: &AnalysisOutput) {
    for dc in &output.issues.duplicate_code {
        results.push(json!({
            "ruleId": "duplicate_code",
            "level": if dc.confidence >= 0.8 { "warning" } else { "note" },
            "message": { "text": sanitize_message(&format!(
                "Similar code in {} (L{}-L{}) and {} (L{}-L{})",
                dc.location_a.file, dc.location_a.start_line, dc.location_a.end_line,
                dc.location_b.file, dc.location_b.start_line, dc.location_b.end_line
            ))},
            "locations": [
                { "physicalLocation": { "artifactLocation": { "uri": sanitize_uri(&dc.location_a.file) }, "region": { "startLine": dc.location_a.start_line } }},
                { "physicalLocation": { "artifactLocation": { "uri": sanitize_uri(&dc.location_b.file) }, "region": { "startLine": dc.location_b.start_line } }},
            ],
            "properties": { "confidence": dc.confidence }
        }));
    }
}

fn append_gotcha_results(results: &mut Vec<Value>, output: &AnalysisOutput) {
    for g in &output.issues.gotchas {
        results.push(json!({
            "ruleId": "gotcha",
            "level": match g.severity.as_str() {
                "error" => "error",
                "warning" => "warning",
                _ => "note",
            },
            "message": { "text": sanitize_message(&g.message) },
            "locations": [{ "physicalLocation": {
                "artifactLocation": { "uri": sanitize_uri(&g.file) },
                "region": { "startLine": g.line }
            }}],
            "properties": { "confidence": g.confidence, "rule": g.rule }
        }));
    }
}

fn append_circular_dep_results(results: &mut Vec<Value>, output: &AnalysisOutput) {
    for cd in &output.issues.circular_dependencies {
        if let Some(first) = cd.files.first() {
            results.push(json!({
                "ruleId": "circular_dependency",
                "level": "warning",
                "message": { "text": sanitize_message(&format!("Circular dependency: {} → {}", cd.files.join(" → "), first)) },
                "locations": [{ "physicalLocation": {
                    "artifactLocation": { "uri": sanitize_uri(first) },
                    "region": { "startLine": 1 }
                }}],
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::path::PathBuf;

    fn evil_output() -> AnalysisOutput {
        AnalysisOutput {
            version: None,
            summary: None,
            detected_frameworks: None,
            monorepo: None,
            structure: Structure {
                root: PathBuf::from("/project"),
                entry_points: vec![],
                implicit_entries: vec![],
                source_files: vec![],
                config_files: vec![],
            },
            dependencies: Dependencies { imports: vec![], external: vec![] },
            quality: Quality { files: vec![] },
            issues: Issues {
                dead_code: vec![DeadCodeIssue {
                    path: "src/evil\r\nfile.ts".to_string(),
                    lines_of_code: 100,
                    confidence: 0.9,
                    reason: "test".to_string(),
                }],
                unused_exports: vec![],
                duplicate_exports: vec![],
                duplicate_code: vec![],
                gotchas: vec![],
                unused_types: vec![],
                circular_dependencies: vec![],
                unused_dependencies: vec![],
                unresolved_imports: vec![],
                unlisted_dependencies: vec![],
                plugin_issues: vec![],
            },
            duplication: DuplicationSection {
                stats: DuplicationStats {
                    total_lines: 0, duplicated_lines: 0,
                    duplication_percentage: 0.0, clone_groups: 0,
                    clone_instances: 0, clone_families: 0,
                },
                clone_groups: vec![], clone_families: vec![],
                mirrored_directories: vec![],
                repetitive_patterns: vec![],
            },
        }
    }

    #[test]
    fn sec_sarif_uri_no_control_chars() {
        let output = evil_output();
        let formatter = SarifFormatter;
        let json = formatter.format(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let results = parsed["runs"][0]["results"].as_array().unwrap();
        let uri = results[0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
            .as_str().unwrap();
        assert!(!uri.contains('\r'), "SARIF URI should not contain CR: {}", uri);
        assert!(!uri.contains('\n'), "SARIF URI should not contain LF: {}", uri);
    }

    // ── V6-4: SARIF message.text must not contain control chars from user data ──
    #[test]
    fn sec_sarif_message_no_control_chars() {
        let mut output = evil_output();
        // Inject control chars into gotcha message
        output.issues.gotchas.push(crate::types::GotchaIssue {
            rule: "test-rule".into(),
            message: "evil\x07message\nINJECTION".into(),
            file: "src/a.ts".into(),
            line: 1,
            severity: "warning".into(),
            confidence: 0.9,
            snippet: "".into(),
        });
        // Inject control chars into circular dep
        output.issues.circular_dependencies.push(crate::types::CircularDepIssue {
            files: vec!["src/a\nts".into(), "src/b.ts".into()],
        });
        let formatter = SarifFormatter;
        let json = formatter.format(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let results = parsed["runs"][0]["results"].as_array().unwrap();
        // Find the gotcha result
        let gotcha = results.iter().find(|r| r["ruleId"] == "gotcha").unwrap();
        let msg = gotcha["message"]["text"].as_str().unwrap();
        assert!(!msg.contains('\x07'), "BEL should be stripped from message, got: {:?}", msg);
        assert!(!msg.contains('\n'), "LF should be replaced with space in message, got: {:?}", msg);
        assert!(msg.contains("INJECTION"), "content should be preserved, got: {:?}", msg);
        // Find the circular dep result
        let circ = results.iter().find(|r| r["ruleId"] == "circular_dependency").unwrap();
        let circ_msg = circ["message"]["text"].as_str().unwrap();
        assert!(!circ_msg.contains('\n'), "LF should be replaced with space in circular dep message, got: {:?}", circ_msg);
    }
}
