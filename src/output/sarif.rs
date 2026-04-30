//! SARIF 2.1.0 output formatter.
//!
//! Maps each issue to a SARIF result with rule metadata, locations, and severity.

use crate::output::OutputFormatter;
use crate::types::AnalysisOutput;
use serde_json::{Value, json};

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
                "message": { "text": dc.reason },
                "locations": [{ "physicalLocation": {
                    "artifactLocation": { "uri": dc.path },
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
                "message": { "text": format!("Export '{}' is never imported", ue.name) },
                "locations": [{ "physicalLocation": {
                    "artifactLocation": { "uri": ue.path },
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
                "message": { "text": format!("{} '{}' is never imported", ut.kind, ut.name) },
                "locations": [{ "physicalLocation": {
                    "artifactLocation": { "uri": ut.path },
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
                "message": { "text": format!("Export '{}' defined in {} locations", de.name, de.locations.len()) },
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
                "message": { "text": format!("Unresolved import '{}' in {}", ui.import_spec, ui.source_file) },
                "locations": [{ "physicalLocation": {
                    "artifactLocation": { "uri": ui.source_file },
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
                "message": { "text": format!("Package '{}' is listed but never imported", ud.package_name) },
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
                "message": { "text": format!("'{}' imported by {} but not in package.json", ud.package_name, ud.imported_by) },
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
            "message": { "text": format!(
                "Similar code in {} (L{}-L{}) and {} (L{}-L{})",
                dc.location_a.file, dc.location_a.start_line, dc.location_a.end_line,
                dc.location_b.file, dc.location_b.start_line, dc.location_b.end_line
            )},
            "locations": [
                { "physicalLocation": { "artifactLocation": { "uri": dc.location_a.file }, "region": { "startLine": dc.location_a.start_line } }},
                { "physicalLocation": { "artifactLocation": { "uri": dc.location_b.file }, "region": { "startLine": dc.location_b.start_line } }},
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
            "message": { "text": g.message },
            "locations": [{ "physicalLocation": {
                "artifactLocation": { "uri": g.file },
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
                "message": { "text": format!("Circular dependency: {} → {}", cd.files.join(" → "), first) },
                "locations": [{ "physicalLocation": {
                    "artifactLocation": { "uri": first },
                    "region": { "startLine": 1 }
                }}],
            }));
        }
    }
}
