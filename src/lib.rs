pub mod analyzer;
pub mod baseline;
pub mod cache;
pub mod config;
pub mod discovery;
pub mod duplication;
pub mod frameworks;
pub mod issues;
pub mod languages;
pub mod output;
pub mod parse;
pub mod path_safety;
pub mod plugin;
pub mod progress;
pub mod resolution;
pub mod shell;
pub mod strip_ansi;
pub mod tui;
pub mod types;
pub mod update;
pub mod workspace;

// Re-export so existing callers don't break.
pub use path_safety::ensure_within_root;
