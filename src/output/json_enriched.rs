//! Enriched JSON output with schema, version, and summary.

use crate::output::{OutputFormatter, compute_summary, detect_framework_names};
use crate::types::AnalysisOutput;
use serde_json::Value;

/// Enriched JSON formatter that adds `$schema`, `version`, `summary`, etc.
pub struct EnrichedJsonFormatter;

impl OutputFormatter for EnrichedJsonFormatter {
    fn format(&self, output: &AnalysisOutput) -> Result<String, String> {
        let mut value = serde_json::to_value(output).map_err(|e| format!("failed to serialize output: {}", e))?;

        let obj = value.as_object_mut().ok_or("expected AnalysisOutput to be a JSON object")?;

        // Prepend schema and version at the top.
        let mut enriched = serde_json::Map::new();
        enriched
            .insert("$schema".to_string(), Value::String("https://statico.dev/schema/analysis-0.2.0.json".to_string()));
        enriched.insert("version".to_string(), Value::String("0.2.0".to_string()));

        // Compute and insert summary.
        let summary = compute_summary(output);
        enriched.insert(
            "summary".to_string(),
            serde_json::to_value(&summary).map_err(|e| format!("failed to serialize summary: {}", e))?,
        );

        // Detect frameworks.
        let frameworks = detect_framework_names(output);
        enriched.insert(
            "detected_frameworks".to_string(),
            serde_json::to_value(&frameworks).map_err(|e| format!("failed to serialize frameworks: {}", e))?,
        );

        // Copy remaining fields (skip version/summary/detected_frameworks if present).
        let keys: Vec<String> = obj.keys().cloned().collect();
        for key in keys {
            if let Some(val) = obj.remove(&key) {
                enriched.insert(key, val);
            }
        }

        // Use compact JSON for very large outputs (avoids the O(n) cost of
        // pretty-printing 10MB+ payloads). Pretty-print is only useful for
        // human reading; machines prefer compact.
        let total_issues = output.issues.dead_code.len()
            + output.issues.unused_exports.len()
            + output.issues.gotchas.len()
            + output.issues.duplicate_code.len();

        if total_issues > 1000 {
            // Compact output for large results — significantly faster.
            serde_json::to_string(&enriched).map_err(|e| format!("failed to format JSON: {}", e))
        } else {
            serde_json::to_string_pretty(&enriched).map_err(|e| format!("failed to format JSON: {}", e))
        }
    }
}
