# Plan: Test Reorganization & File Splits

## Goals
1. Split huge files into focused modules
2. Rename all `sec_vN_M_*` tests to `sec_{domain}_*` (domain-based, not pass-number-based)
3. Extract pure functions into separate testable modules

## Guiding Principles
- Every test name answers "what is being tested", not "when was it found"
- Pure functions (no filesystem, no network, no side effects) live in their own modules
- File size target: ≤400 lines for source files, ≤300 for test modules
- Keep tests co-located with code (Rust convention) but extracted pure functions get their own test files

---

## Phase 1: Split `main.rs` (1940 → ~5 files)

### New structure
```
src/main.rs          — CLI entry, clap args, dispatch only (~300 lines)
src/cli/setup.rs     — run_init, run_setup, generate_claude_md, generate_skill_*, generate_cursor_rules (~400 lines)
src/cli/plugin.rs    — run_plugin_*, scaffold_* (~550 lines)
src/cli/doctor.rs    — run_doctor, which_statico, which_exists, print_status (~150 lines)
src/cli/mod.rs       — run_analyze, run_diff, run_update, shell_escape, load_analysis (~350 lines)
```

### Functions to extract
| Function | Destination | Is pure? |
|---|---|---|
| `shell_escape` | `src/cli/mod.rs` (or `src/cli/shell.rs`) | ✅ pure |
| `version_with_git` | `src/main.rs` (stays) | ✅ pure |
| `load_analysis` | `src/cli/mod.rs` | ❌ reads file |
| `has_issues_above_confidence` | `src/cli/mod.rs` | ✅ pure |
| `generate_claude_md` | `src/cli/setup.rs` | ✅ pure |
| `generate_skill_analyze` | `src/cli/setup.rs` | ✅ pure |
| `generate_skill_fix` | `src/cli/setup.rs` | ✅ pure |
| `generate_skill_plugin` | `src/cli/setup.rs` | ✅ pure |
| `generate_cursor_rules` | `src/cli/setup.rs` | ✅ pure |
| `scaffold_typescript_plugin` | `src/cli/plugin.rs` | ❌ writes files |
| `scaffold_rust_plugin` | `src/cli/plugin.rs` | ❌ writes files |
| `scaffold_python_plugin` | `src/cli/plugin.rs` | ❌ writes files |

---

## Phase 2: Rename All Security Tests

### Mapping: old name → new name

Tests are renamed based on **what they test** (the module/domain), not which pass found them.

#### `src/lib.rs` — path safety & ANSI stripping
| Old | New |
|---|---|
| `sec_ensure_within_root_allows_child` | `sec_path_within_root_allows_child` |
| `sec_ensure_within_root_rejects_parent_traversal` | `sec_path_within_root_rejects_parent_traversal` |
| `sec_ensure_within_root_rejects_absolute_escape` | `sec_path_within_root_rejects_absolute` |
| `sec_ensure_within_root_rejects_dotdot_in_middle` | `sec_path_within_root_rejects_dotdot` |
| `sec_ensure_within_root_allows_valid_subpath` | `sec_path_within_root_allows_subpath` |
| `sec_v4_1_strips_ansi_escape_from_plugin_message` | `sec_ansi_strips_escape_from_plugin_message` |
| `sec_v4_1_strips_cursor_movement_ansi` | `sec_ansi_strips_cursor_movement` |
| `sec_v4_8_strips_ansi_from_plugin_name` | `sec_ansi_strips_from_plugin_name` |
| `sec_v6_6_strip_ansi_handles_osc_sequences` | `sec_ansi_handles_osc_sequences` |
| `sec_v6_6_strip_ansi_handles_osc_st_terminator` | `sec_ansi_handles_osc_st_terminator` |
| `sec_v6_6_strip_ansi_handles_two_char_esc` | `sec_ansi_handles_two_char_esc` |

#### `src/main.rs` → `src/cli/mod.rs` — shell escaping
| Old | New |
|---|---|
| `sec_v34_shell_escape_dollar` | `sec_shell_escape_dollar` |
| `sec_v34_shell_escape_backtick` | `sec_shell_escape_backtick` |
| `sec_v34_shell_escape_double_quote` | `sec_shell_escape_double_quote` |
| `sec_v34_shell_escape_backslash` | `sec_shell_escape_backslash` |
| `sec_v34_shell_escape_normal_path` | `sec_shell_escape_normal_path` |
| `sec_v6_6_source_path_shell_escaped` | `sec_shell_source_path_escaped` |

#### `src/cache.rs` — cache integrity
| Old | New |
|---|---|
| `sec_cache_atomic_write_no_partial_file` | (already good) |
| `sec_cache_file_permissions_restricted` | (already good) |
| `sec_content_hash_no_trivial_collision` | (already good) |
| `sec_v8_gitignore_rejects_symlink` | `sec_gitignore_rejects_symlink` |
| `sec_v7_4_gitignore_adds_entry_when_only_comment_present` | `sec_gitignore_adds_when_only_comment` |
| `sec_v7_4_gitignore_skips_when_real_pattern_present` | `sec_gitignore_skips_when_pattern_present` |

#### `src/config.rs` — config validation
| Old | New |
|---|---|
| `sec_max_file_size_capped_at_50mb` | `sec_config_max_file_size_capped` |
| `sec_max_file_size_normal_values_pass` | `sec_config_max_file_size_normal` |
| `sec_max_file_size_default_is_1mb` | `sec_config_max_file_size_default` |
| `sec_v7_10_min_confidence_clamped_nan` | `sec_config_min_confidence_clamped_nan` |
| `sec_v7_10_min_confidence_clamped_negative` | `sec_config_min_confidence_clamped_negative` |
| `sec_v7_10_min_confidence_clamped_above_one` | `sec_config_min_confidence_clamped_above_one` |

#### `src/monorepo.rs` — monorepo detection
| Old | New |
|---|---|
| `sec_v7_6_glob_to_prefix_double_star` | `sec_monorepo_glob_to_prefix_double_star` |
| `sec_v310_workspace_roots_reject_traversal` | `sec_monorepo_workspace_roots_reject_traversal` |
| `sec_v310_workspace_roots_reject_absolute` | `sec_monorepo_workspace_roots_reject_absolute` |

#### `src/discovery/mod.rs` — file discovery
| Old | New |
|---|---|
| `sec_discovery_no_symlink_follow` | (already good) |
| `sec_discovery_respects_max_depth` | (already good) |
| `sec_v7_7_star_does_not_match_slash` | `sec_glob_star_does_not_match_slash` |

#### `src/update.rs` — self-update
| Old | New |
|---|---|
| `sec_today_string_is_valid_date` | `sec_update_today_is_valid_date` |
| `sec_today_string_no_shell_out` | `sec_update_today_no_shell_out` |
| `sec_extract_tar_gz_rejects_path_traversal` | `sec_update_tar_rejects_path_traversal` |
| `sec_v11_download_size_limit_exists` | `sec_update_download_size_limit` |
| `sec_v12_find_binary_rejects_symlink` | `sec_update_binary_rejects_symlink` |
| `sec_v6_5_is_newer_distinguishes_prerelease` | `sec_update_is_newer_distinguishes_prerelease` |

#### `src/parse/mod.rs` — parsing
| Old | New |
|---|---|
| `sec_v7_1_unquote_no_panic_on_single_char` | `sec_parse_unquote_no_panic_short_string` |
| `sec_v7_2_parser_recovers_from_poisoned_mutex` | `sec_parse_recovers_from_poisoned_mutex` |

#### `src/parse/imports.rs` — import analysis
| Old | New |
|---|---|
| `sec_v7_5_extract_package_name_bare_at` | `sec_imports_bare_at_returns_empty` |
| `sec_v7_8_classify_import_no_empty_external` | `sec_imports_no_empty_external_packages` |

#### `src/resolution/paths.rs` — path resolution
| Old | New |
|---|---|
| `sec_v3_resolve_relative_rejects_path_traversal` | `sec_paths_rejects_path_traversal` |
| `sec_v3_resolve_relative_rejects_absolute_spec` | `sec_paths_rejects_absolute_spec` |

#### `src/resolution/tsconfig.rs` — tsconfig parsing
| Old | New |
|---|---|
| `sec_v32_strip_jsonc_normal_block_comment` | `sec_jsonc_strips_block_comment` |
| `sec_v32_strip_jsonc_unterminated_block_comment` | `sec_jsonc_handles_unterminated_comment` |
| `sec_v36_tsconfig_target_traversal_rejected` | `sec_tsconfig_target_rejects_traversal` |
| `sec_v39_tsconfig_non_relative_target_traversal_rejected` | `sec_tsconfig_non_relative_rejects_traversal` |

#### `src/output/mermaid.rs` — Mermaid output
| Old | New |
|---|---|
| `sec_v4_9_display_name_escapes_curly_braces` | `sec_mermaid_escapes_curly_braces` |
| `sec_v5_mermaid_escapes_quotes_in_labels` | `sec_mermaid_escapes_quotes` |
| `sec_v5_10_display_name_escapes_newlines` | `sec_mermaid_escapes_newlines` |
| `sec_v6_1_escape_mermaid_label_escapes_newlines` | `sec_mermaid_label_escapes_newlines` |
| `sec_v6_7_escape_mermaid_label_escapes_hash` | `sec_mermaid_label_escapes_hash` |
| `sec_v7_3_display_name_escapes_hash` | `sec_mermaid_display_name_escapes_hash` |
| `sec_v7_9_display_name_escapes_ampersand` | `sec_mermaid_display_name_escapes_ampersand` |
| `sec_v7_9_escape_mermaid_label_escapes_ampersand` | `sec_mermaid_label_escapes_ampersand` |

#### `src/output/markdown.rs` — Markdown output
| Old | New |
|---|---|
| `sec_v4_markdown_escapes_markdown_links` | `sec_markdown_escapes_links` |
| `sec_v4_markdown_escapes_newlines_in_cells` | `sec_markdown_escapes_newlines_in_cells` |
| `sec_v4_markdown_escapes_pipe_in_tables` | `sec_markdown_escapes_pipe_in_tables` |

#### `src/output/fix.rs` — fix suggestions
| Old | New |
|---|---|
| `sec_v4_2_fix_formatter_strips_newlines_in_path` | `sec_fix_strips_newlines_in_path` |
| `sec_v4_3_fix_formatter_strips_newlines_in_reason` | `sec_fix_strips_newlines_in_reason` |
| `sec_v6_2_fix_formatter_strips_newlines_in_export_path` | `sec_fix_strips_newlines_in_export_path` |
| `sec_v6_3_fix_formatter_strips_newlines_in_export_name` | `sec_fix_strips_newlines_in_export_name` |

#### `src/output/sarif.rs` — SARIF output
| Old | New |
|---|---|
| `sec_sarif_uri_no_control_chars` | (already good) |
| `sec_v6_4_sarif_message_no_control_chars` | `sec_sarif_message_no_control_chars` |

#### `src/output/html.rs` — HTML output
| Old | New |
|---|---|
| `sec_v3_html_escapes_comment_injection` | `sec_html_escapes_comment_injection` |
| `sec_html_escapes_script_injection` | (already good) |
| `sec_html_no_raw_script_close_in_json` | (already good) |

#### `src/output/pr_comment.rs` — PR comment output
| Old | New |
|---|---|
| `sec_pr_comment_escapes_circular_dep_files` | (already good) |
| `sec_pr_comment_escapes_markdown_links` | (already good) |
| `sec_pr_comment_escapes_newlines_in_cells` | (already good) |
| `sec_pr_comment_escapes_pipe_in_tables` | (already good) |
| `sec_v5_4_pr_comment_escapes_backticks_and_angle_brackets` | `sec_pr_comment_escapes_backticks_and_angle_brackets` |

#### `src/output/diff.rs` — diff output
| Old | New |
|---|---|
| `sec_v31_diff_markdown_escapes_newlines_in_detail` | `sec_diff_escapes_newlines_in_detail` |
| `sec_v31_diff_markdown_escapes_pipe_in_detail` | `sec_diff_escapes_pipe_in_detail` |
| `sec_v5_7_diff_escapes_backticks_and_angle_brackets` | `sec_diff_escapes_backticks_and_angle_brackets` |

#### `src/output/context.rs` — context output
| Old | New |
|---|---|
| `sec_v5_6_context_newline_in_path_sanitized` | `sec_context_newline_in_path_sanitized` |

#### `src/plugin/discovery.rs` — plugin discovery
| Old | New |
|---|---|
| `sec_config_plugin_path_traversal_rejected` | `sec_plugin_path_traversal_rejected` |
| `sec_discovery_skips_hidden_and_temp` | (already good — kept under plugin) |

#### `src/plugin/manager.rs` — plugin manager
| Old | New |
|---|---|
| `sec_v35_python_entry_rejects_traversal` | `sec_plugin_python_entry_rejects_traversal` |
| `sec_v35_python_entry_rejects_absolute` | `sec_plugin_python_entry_rejects_absolute` |
| `sec_v37_toml_depth_limit` | `sec_plugin_toml_depth_limit` |
| `sec_v38_settings_string_size_limit` | `sec_plugin_settings_string_size_limit` |
| `sec_v38_settings_array_size_limit` | `sec_plugin_settings_array_size_limit` |
| `sec_v38_settings_table_size_limit` | `sec_plugin_settings_table_size_limit` |

#### `src/analyzer/mod.rs` — analyzer
| Old | New |
|---|---|
| `sec_analyzer_skips_oversized_file` | (already good) |

#### `src/tui.rs` — TUI
| Old | New |
|---|---|
| `sec_v5_2_shorten_str_no_panic_on_multibyte` | `sec_tui_shorten_str_no_panic_on_multibyte` |
| `sec_v5_3_shorten_path_no_panic_on_multibyte` | `sec_tui_shorten_path_no_panic_on_multibyte` |

#### `src/output/markdown.rs` / `src/output/*.rs` — other output tests
| Old | New |
|---|---|
| `sec_v4_4_subgraph_label_escapes_special_chars` | `sec_mermaid_subgraph_label_escapes_special` |
| `sec_v4_5_escapes_backticks_in_cells` | `sec_markdown_escapes_backticks_in_cells` |
| `sec_v4_6_escapes_circular_dep_file_names` | `sec_markdown_escapes_circular_dep_files` |
| `sec_v4_7_escapes_duplication_file_names` | `sec_markdown_escapes_duplication_files` |
| `sec_v4_10_escapes_html_chars_in_cells` | `sec_markdown_escapes_html_chars_in_cells` |
| `sec_v5_9_config_files_escaped_in_markdown` | `sec_markdown_config_files_escaped` |

---

## Phase 3: Extract Pure Functions

Functions that are pure (no side effects, deterministic) should be extracted into focused modules:

### `src/cli/shell.rs` (new)
- `shell_escape(s: &str) -> String` — pure string transformation
- Tests: `sec_shell_escape_*`

### `src/output/mermaid/escape.rs` (new — or just `src/output/mermaid_escape.rs`)
- `escape_mermaid_label(s: &str) -> String` — pure
- `display_name(path, prefix) -> String` — pure
- Tests: all `sec_mermaid_*` tests

### `src/parse/unquote.rs` (new)
- `unquote(s: &str) -> String` — pure
- Test: `sec_parse_unquote_no_panic_short_string`

### `src/monorepo/glob.rs` (new — or inline)
- `glob_to_prefix(patterns) -> Vec<String>` — pure
- `match_simple_glob(pattern, path) -> bool` — pure
- Tests: `sec_monorepo_*`, `sec_glob_*`

### `src/strip_ansi.rs` (new top-level)
- `strip_ansi(s: &str) -> String` — pure
- Tests: all `sec_ansi_*` tests
- Currently embedded in `src/lib.rs` — should be its own module

---

## Execution Order

1. **Phase 2 first** (rename tests) — mechanical, no logic changes, easy to verify
2. **Phase 3** (extract pure functions) — move functions + their tests to new modules
3. **Phase 1** (split main.rs) — largest change, do last when patterns are established

Each phase: edit → `cargo test` → commit.

## Files Created
```
src/cli/mod.rs
src/cli/setup.rs
src/cli/plugin.rs
src/cli/doctor.rs
src/cli/shell.rs
src/output/mermaid_escape.rs
src/parse/unquote.rs
src/monorepo/glob.rs
src/strip_ansi.rs
```

## Verification
- `cargo test` — all 517+ tests pass with new names
- `cargo clippy` — no new warnings
- File line counts all ≤400
