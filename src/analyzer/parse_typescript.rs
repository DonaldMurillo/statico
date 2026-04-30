//! TypeScript file parsing — now handled by `languages::TypeScriptPlugin`.
//!
//! This module retained for backward compatibility. The plugin system
//! (Phase 3) replaced the direct call path.

// All TypeScript parsing is now done via:
//   crate::languages::typescript::TypeScriptPlugin::analyze_file()
//
// The legacy FileResult / parse_all_files_parallel / parse_single_file types
// have been removed. If you need per-file TypeScript analysis, use the plugin:
//
//   use crate::languages::plugin_for_extension;
//   let plugin = plugin_for_extension("ts").unwrap();
//   let analysis = plugin.analyze_file(root, "src/app.ts", &source);
