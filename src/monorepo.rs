//! Monorepo and workspace detection.
//!
//! Re-exported from `frameworks` module. All monorepo profiles now live in
//! `src/frameworks/monorepo_*.rs` alongside framework profiles.

pub use crate::frameworks::{MonorepoInfo, detect_monorepo, discover_workspace_roots, is_workspace_package_file};
