//! statico — static code analyzer for TypeScript and Rust.
//!
//! This crate is primarily a CLI binary. The library surface exists so the
//! binary, tests, examples, and benchmarks can share modules — it is **not**
//! a stable public API. Every module below is marked `#[doc(hidden)]` and
//! may change in any release prior to `1.0.0`.
//!
//! If you want to depend on statico programmatically, please open an issue
//! describing your use case so we can carve out a stable surface for it.

#[doc(hidden)]
pub mod analyzer;
#[doc(hidden)]
pub mod baseline;
#[doc(hidden)]
pub mod cache;
#[doc(hidden)]
pub mod config;
#[doc(hidden)]
pub mod discovery;
#[doc(hidden)]
pub mod duplication;
#[doc(hidden)]
pub mod frameworks;
#[doc(hidden)]
pub mod issues;
#[doc(hidden)]
pub mod languages;
#[doc(hidden)]
pub mod output;
#[doc(hidden)]
pub mod parse;
#[doc(hidden)]
pub mod path_safety;
#[doc(hidden)]
pub mod plugin;
#[doc(hidden)]
pub mod progress;
#[doc(hidden)]
pub mod resolution;
#[doc(hidden)]
pub mod shell;
#[doc(hidden)]
pub mod strip_ansi;
#[doc(hidden)]
pub mod tui;
#[doc(hidden)]
pub mod types;
#[doc(hidden)]
pub mod update;
#[doc(hidden)]
pub mod workspace;

// Re-export so existing callers don't break.
#[doc(hidden)]
pub use path_safety::ensure_within_root;
