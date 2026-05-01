use serde::{Deserialize, Serialize};
use std::path::Path;

/// Configuration loaded from .statico.toml
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StaticoConfig {
    /// Default output format: json, sarif, markdown, html
    #[serde(default = "default_format")]
    pub format: String,
    /// Minimum confidence threshold (0.0–1.0)
    #[serde(default)]
    pub min_confidence: f64,
    /// Exit with code 1 on issues
    #[serde(default)]
    pub exit_code: bool,
    /// Suppress progress output
    #[serde(default)]
    pub quiet: bool,
    /// Glob patterns to exclude from analysis
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Glob patterns to include (overrides exclude)
    #[serde(default)]
    pub include: Vec<String>,
    /// Maximum file size in bytes to analyze (skip large files)
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,
    /// Number of threads (0 = auto)
    #[serde(default)]
    pub threads: usize,
    /// Disable auto-discovery of plugins in .statico/plugins/
    #[serde(default = "default_true")]
    pub plugin_auto_discover: bool,
    /// Plugin declarations (merged with auto-discovery).
    #[serde(default)]
    pub plugin: Vec<PluginEntry>,
}

fn default_format() -> String {
    "json".to_string()
}
fn default_max_file_size() -> u64 {
    1_000_000
}

/// Maximum allowed file size (50 MB) — prevents OOM from malicious config.
const MAX_ALLOWED_FILE_SIZE: u64 = 50_000_000;
fn default_true() -> bool {
    true
}

/// A plugin entry in .statico.toml.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PluginEntry {
    pub name: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// If true, override ALL hooks this plugin registers.
    #[serde(default, rename = "override")]
    pub r#override: bool,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default = "default_toml_table")]
    pub settings: toml::Value,
}

fn default_toml_table() -> toml::Value {
    toml::Value::Table(toml::map::Map::new())
}

impl Default for StaticoConfig {
    fn default() -> Self {
        Self {
            format: default_format(),
            min_confidence: 0.0,
            exit_code: false,
            quiet: false,
            exclude: Vec::new(),
            include: Vec::new(),
            max_file_size: default_max_file_size(),
            threads: 0,
            plugin_auto_discover: true,
            plugin: Vec::new(),
        }
    }
}

impl StaticoConfig {
    /// Load config from a .statico.toml file in the given directory.
    pub fn load(project_root: &Path) -> Self {
        let config_path = project_root.join(".statico.toml");
        if !config_path.exists() {
            return Self::default();
        }
        let content = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warning: failed to read {}: {}", config_path.display(), e);
                return Self::default();
            }
        };
        match toml::from_str::<StaticoConfig>(&content) {
            Ok(mut c) => {
                // Clamp max_file_size to prevent OOM from malicious config (S2-08).
                if c.max_file_size > MAX_ALLOWED_FILE_SIZE {
                    eprintln!(
                        "warning: max_file_size ({}) exceeds limit ({}), clamping",
                        c.max_file_size, MAX_ALLOWED_FILE_SIZE
                    );
                    c.max_file_size = MAX_ALLOWED_FILE_SIZE;
                }
                // V7-10: Clamp min_confidence to [0.0, 1.0].
                // NaN, negative, or >1.0 values would cause filter_by_confidence
                // to produce misleading results (e.g. NaN drops ALL issues,
                // giving a false 100/100 health score).
                if c.min_confidence.is_nan() || c.min_confidence < 0.0 {
                    c.min_confidence = 0.0;
                } else if c.min_confidence > 1.0 {
                    c.min_confidence = 1.0;
                }
                c
            }
            Err(e) => {
                eprintln!("warning: failed to parse {}: {}", config_path.display(), e);
                Self::default()
            }
        }
    }

    /// Merge CLI arguments over config defaults.
    pub fn merge_cli(&self, format: Option<&str>, min_confidence: Option<f64>, exit_code: bool, quiet: bool) -> Self {
        let mut merged = self.clone();
        if let Some(f) = format {
            merged.format = f.to_string();
        }
        if let Some(c) = min_confidence {
            // V7-10: Clamp min_confidence to [0.0, 1.0] — same as load().
            merged.min_confidence = if c.is_nan() || c < 0.0 { 0.0 } else if c > 1.0 { 1.0 } else { c };
        }
        if exit_code {
            merged.exit_code = true;
        }
        if quiet {
            merged.quiet = true;
        }
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("statico_test_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_default_config() {
        let c = StaticoConfig::default();
        assert_eq!(c.format, "json");
        assert_eq!(c.min_confidence, 0.0);
        assert!(!c.exit_code);
        assert!(!c.quiet);
        assert!(c.exclude.is_empty());
        assert!(c.include.is_empty());
        assert_eq!(c.max_file_size, 1_000_000);
        assert_eq!(c.threads, 0);
    }

    #[test]
    fn test_parse_valid_toml() {
        let c: StaticoConfig = toml::from_str(
            r#"
format = "markdown"
min_confidence = 0.7
exit_code = true
quiet = true
exclude = ["node_modules", "dist"]
include = ["src/**/*.ts"]
max_file_size = 500000
threads = 4
"#,
        )
        .unwrap();
        assert_eq!(c.format, "markdown");
        assert!((c.min_confidence - 0.7).abs() < f64::EPSILON);
        assert!(c.exit_code);
        assert!(c.quiet);
        assert_eq!(c.exclude, vec!["node_modules", "dist"]);
        assert_eq!(c.include, vec!["src/**/*.ts"]);
        assert_eq!(c.max_file_size, 500_000);
        assert_eq!(c.threads, 4);
    }

    #[test]
    fn test_parse_partial_toml_uses_defaults() {
        let c: StaticoConfig = toml::from_str("format = \"sarif\"").unwrap();
        assert_eq!(c.format, "sarif");
        assert!(!c.exit_code);
        assert!(c.exclude.is_empty());
        assert_eq!(c.max_file_size, 1_000_000);
    }

    #[test]
    fn test_merge_cli_overrides_format() {
        let config = StaticoConfig { format: "markdown".into(), ..StaticoConfig::default() };
        let merged = config.merge_cli(Some("html"), None, false, false);
        assert_eq!(merged.format, "html");
    }

    #[test]
    fn test_merge_cli_preserves_config_defaults() {
        let config = StaticoConfig { format: "sarif".into(), min_confidence: 0.5, ..StaticoConfig::default() };
        let merged = config.merge_cli(None, None, false, false);
        assert_eq!(merged.format, "sarif");
        assert!((merged.min_confidence - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_merge_cli_sets_exit_code_and_quiet() {
        let merged = StaticoConfig::default().merge_cli(None, None, true, true);
        assert!(merged.exit_code);
        assert!(merged.quiet);
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let dir = make_temp_dir("missing");
        let c = StaticoConfig::load(&dir);
        assert_eq!(c.format, "json");
        assert!(c.exclude.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_valid_config_file() {
        let dir = make_temp_dir("valid");
        std::fs::write(
            dir.join(".statico.toml"),
            r#"format = "markdown"
min_confidence = 0.8
exclude = ["vendor"]
"#,
        )
        .unwrap();
        let c = StaticoConfig::load(&dir);
        assert_eq!(c.format, "markdown");
        assert!((c.min_confidence - 0.8).abs() < f64::EPSILON);
        assert_eq!(c.exclude, vec!["vendor"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_invalid_toml_returns_default() {
        let dir = make_temp_dir("invalid");
        std::fs::write(dir.join(".statico.toml"), "not valid toml [[[=[").unwrap();
        let c = StaticoConfig::load(&dir);
        assert_eq!(c.format, "json");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let c = StaticoConfig {
            format: "html".into(),
            min_confidence: 0.3,
            exit_code: true,
            quiet: false,
            exclude: vec!["build".into()],
            include: vec!["src/**/*.tsx".into()],
            max_file_size: 2_000_000,
            threads: 8,
            plugin_auto_discover: true,
            plugin: vec![],
        };
        let toml_str = toml::to_string(&c).unwrap();
        let parsed: StaticoConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.format, c.format);
        assert!((parsed.min_confidence - c.min_confidence).abs() < f64::EPSILON);
        assert_eq!(parsed.exit_code, c.exit_code);
        assert_eq!(parsed.exclude, c.exclude);
        assert_eq!(parsed.max_file_size, c.max_file_size);
        assert_eq!(parsed.threads, c.threads);
    }

    #[test]
    fn test_parse_plugin_config() {
        let c: StaticoConfig = toml::from_str(
            r#"
format = "json"
plugin_auto_discover = false

[[plugin]]
name = "my-rule"
path = "./plugins/my-rule"
enabled = true
languages = ["typescript"]

[[plugin]]
name = "acme-fork"
override = true
"#,
        )
        .unwrap();
        assert!(!c.plugin_auto_discover);
        assert_eq!(c.plugin.len(), 2);
        assert_eq!(c.plugin[0].name, "my-rule");
        assert_eq!(c.plugin[0].path, Some("./plugins/my-rule".to_string()));
        assert!(c.plugin[0].enabled);
        assert!(c.plugin[1].r#override);
    }

    // ── Security tests ──────────────────────────────────────────────────

    #[test]
    fn sec_max_file_size_capped_at_50mb() {
        let dir = make_temp_dir("cap");
        std::fs::write(
            dir.join(".statico.toml"),
            r#"max_file_size = 999999999999"#,
        )
        .unwrap();
        let c = StaticoConfig::load(&dir);
        assert_eq!(c.max_file_size, MAX_ALLOWED_FILE_SIZE,
            "max_file_size should be clamped to {}, got {}", MAX_ALLOWED_FILE_SIZE, c.max_file_size);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sec_max_file_size_normal_values_pass() {
        let dir = make_temp_dir("normal");
        std::fs::write(
            dir.join(".statico.toml"),
            r#"max_file_size = 500000"#,
        )
        .unwrap();
        let c = StaticoConfig::load(&dir);
        assert_eq!(c.max_file_size, 500_000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sec_max_file_size_default_is_1mb() {
        let c = StaticoConfig::default();
        assert_eq!(c.max_file_size, 1_000_000);
        assert!(c.max_file_size <= MAX_ALLOWED_FILE_SIZE);
    }

    // ── V7-10: min_confidence must be clamped to [0.0, 1.0] ──
    #[test]
    fn sec_v7_10_min_confidence_clamped_nan() {
        // NaN causes ALL issues to be filtered (NaN >= x is always false),
        // giving a false 100/100 health score.
        let merged = StaticoConfig::default().merge_cli(None, Some(f64::NAN), false, false);
        assert!(!merged.min_confidence.is_nan(),
            "NaN min_confidence should be clamped to 0.0");
        assert_eq!(merged.min_confidence, 0.0);
    }

    #[test]
    fn sec_v7_10_min_confidence_clamped_negative() {
        let merged = StaticoConfig::default().merge_cli(None, Some(-0.5), false, false);
        assert_eq!(merged.min_confidence, 0.0,
            "negative min_confidence should be clamped to 0.0");
    }

    #[test]
    fn sec_v7_10_min_confidence_clamped_above_one() {
        let merged = StaticoConfig::default().merge_cli(None, Some(2.0), false, false);
        assert_eq!(merged.min_confidence, 1.0,
            "min_confidence > 1.0 should be clamped to 1.0");
    }
}
