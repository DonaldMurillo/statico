//! Mermaid flowchart diagram formatter.
//!
//! Renders the dependency graph as a Mermaid flowchart suitable for Markdown
//! rendering (GitHub, GitLab, Notion, etc.).
//!
//! Visual encoding:
//! - Entry points  → green nodes   (`fill:#66bb6a`)
//! - Dead code     → red nodes     (`fill:#ff6b6b`)
//! - Issue hotspots → orange nodes (`fill:#ffa726`)
//! - Circular deps  → thick red arrows (`stroke:red,stroke-width:3px`)

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::OutputFormatter;
use crate::types::AnalysisOutput;

/// Mermaid flowchart diagram formatter.
pub struct MermaidFormatter;

/// Maximum number of files to show before triggering complexity reduction.
const FILE_LIMIT: usize = 30;

impl OutputFormatter for MermaidFormatter {
    fn format(&self, output: &AnalysisOutput) -> Result<String, String> {
        let mut buf = String::from("graph TD\n");

        // ── 1. Collect all files appearing in the dependency graph ──
        let mut all_files: BTreeSet<String> = BTreeSet::new();
        for fi in &output.dependencies.imports {
            all_files.insert(fi.source.clone());
            for t in &fi.targets {
                all_files.insert(t.clone());
            }
        }

        // ── 2. Classify files ──
        let dead_set: HashSet<String> = output.issues.dead_code.iter().map(|d| d.path.clone()).collect();

        let entry_set: HashSet<String> = output.structure.entry_points.iter().cloned().collect();

        let circular_set: HashSet<String> =
            output.issues.circular_dependencies.iter().flat_map(|cd| cd.files.iter().cloned()).collect();

        // Build the set of edges that participate in a circular dep.
        // For each circular dep chain A→B→C→A, mark every adjacent pair.
        let mut circular_edges: HashSet<(String, String)> = HashSet::new();
        for cd in &output.issues.circular_dependencies {
            for window in cd.files.windows(2) {
                circular_edges.insert((window[0].clone(), window[1].clone()));
            }
            // Close the cycle: last → first
            if cd.files.len() >= 2 {
                circular_edges.insert((cd.files[cd.files.len() - 1].clone(), cd.files[0].clone()));
            }
        }

        // ── 3. Count issues per file for hotspot detection ──
        let issue_counts = count_issues_per_file(output);

        // ── 4. Complexity management: trim to important files if needed ──
        let visible_files: BTreeSet<String> = if all_files.len() > FILE_LIMIT {
            select_important_files(
                &all_files,
                &dead_set,
                &entry_set,
                &circular_set,
                &issue_counts,
                &output.dependencies.imports,
            )
        } else {
            all_files
        };

        // ── 5. Assign stable short IDs (N0, N1, …) ──
        let id_map: HashMap<String, String> =
            visible_files.iter().enumerate().map(|(i, path)| (path.clone(), format!("N{}", i))).collect();

        // ── 6. Determine the common prefix to strip for display names ──
        let common_prefix = common_path_prefix(visible_files.iter());

        // ── 7. Group files by directory (for subgraphs) ──
        let dir_groups = group_by_directory(&visible_files, &common_prefix);

        // ── 8. Emit subgraphs ──
        for (dir, paths) in &dir_groups {
            // Mermaid subgraph label: escape quotes if present
            let label = if dir.is_empty() { "(root)".to_string() } else { escape_mermaid_label(dir) };
            buf.push_str(&format!("    subgraph {}\n", label));
            for path in paths {
                let id = &id_map[path];
                let display = display_name(path, &common_prefix);
                buf.push_str(&format!("        {}[\"{}\"]\n", id, display));
            }
            buf.push_str("    end\n");
        }

        // ── 9. Emit edges ──
        let mut edge_index: usize = 0;
        let mut circular_edge_indices: Vec<usize> = Vec::new();

        for fi in &output.dependencies.imports {
            if !id_map.contains_key(&fi.source) {
                continue;
            }
            for target in &fi.targets {
                if !id_map.contains_key(target) {
                    continue;
                }
                let src_id = &id_map[&fi.source];
                let tgt_id = &id_map[target];

                if circular_edges.contains(&(fi.source.clone(), target.clone())) {
                    buf.push_str(&format!("    {} -.->|circular| {}\n", src_id, tgt_id));
                    circular_edge_indices.push(edge_index);
                } else {
                    buf.push_str(&format!("    {} --> {}\n", src_id, tgt_id));
                }
                edge_index += 1;
            }
        }

        // ── 10. Style: entry points (green) ──
        for path in &entry_set {
            if let Some(id) = id_map.get(path) {
                buf.push_str(&format!("    style {} fill:#66bb6a\n", id));
            }
        }

        // ── 11. Style: dead code (red) ──
        for path in &dead_set {
            if let Some(id) = id_map.get(path) {
                buf.push_str(&format!("    style {} fill:#ff6b6b\n", id));
            }
        }

        // ── 12. Style: issue hotspots — top 5 by issue count (orange) ──
        // Don't override entry/dead styling; only style uncoloured files.
        let mut hotspot_candidates: Vec<(String, usize)> = issue_counts
            .iter()
            .filter(|(p, _)| visible_files.contains(*p) && !entry_set.contains(*p) && !dead_set.contains(*p))
            .map(|(p, c)| (p.clone(), *c))
            .collect();
        hotspot_candidates.sort_by(|a, b| b.1.cmp(&a.1));
        for (path, _count) in hotspot_candidates.iter().take(5) {
            if let Some(id) = id_map.get(path) {
                buf.push_str(&format!("    style {} fill:#ffa726\n", id));
            }
        }

        // ── 13. Style: circular dep edges (thick red) ──
        for idx in &circular_edge_indices {
            buf.push_str(&format!("    linkStyle {} stroke:red,stroke-width:3px\n", idx));
        }

        Ok(buf)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Count the total number of issues that reference each file path.
fn count_issues_per_file(output: &AnalysisOutput) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    let bump = |m: &mut HashMap<String, usize>, path: &str| {
        *m.entry(path.to_string()).or_insert(0) += 1;
    };

    for d in &output.issues.dead_code {
        bump(&mut counts, &d.path);
    }
    for u in &output.issues.unused_exports {
        bump(&mut counts, &u.path);
    }
    for u in &output.issues.unused_types {
        bump(&mut counts, &u.path);
    }
    for g in &output.issues.gotchas {
        bump(&mut counts, &g.file);
    }
    for cd in &output.issues.circular_dependencies {
        for f in &cd.files {
            bump(&mut counts, f);
        }
    }
    for dc in &output.issues.duplicate_code {
        bump(&mut counts, &dc.location_a.file);
        bump(&mut counts, &dc.location_b.file);
    }
    for ui in &output.issues.unresolved_imports {
        bump(&mut counts, &ui.source_file);
    }
    for ud in &output.issues.unlisted_dependencies {
        bump(&mut counts, &ud.imported_by);
    }

    counts
}

/// When the graph exceeds [`FILE_LIMIT`] files, select only the most important
/// ones and their direct neighbours so the diagram stays readable.
fn select_important_files(
    all_files: &BTreeSet<String>,
    dead_set: &HashSet<String>,
    entry_set: &HashSet<String>,
    circular_set: &HashSet<String>,
    issue_counts: &HashMap<String, usize>,
    imports: &[crate::types::FileImports],
) -> BTreeSet<String> {
    let mut important: BTreeSet<String> = BTreeSet::new();

    // 1. All dead code files
    for f in dead_set {
        if all_files.contains(f) {
            important.insert(f.clone());
        }
    }

    // 2. All entry points
    for f in entry_set {
        if all_files.contains(f) {
            important.insert(f.clone());
        }
    }

    // 3. Files involved in circular deps
    for f in circular_set {
        if all_files.contains(f) {
            important.insert(f.clone());
        }
    }

    // 4. Top 10 files by issue count
    let mut by_issues: Vec<(String, usize)> =
        issue_counts.iter().filter(|(p, _)| all_files.contains(*p)).map(|(p, c)| (p.clone(), *c)).collect();
    by_issues.sort_by(|a, b| b.1.cmp(&a.1));
    for (path, _) in by_issues.iter().take(10) {
        important.insert(path.clone());
    }

    // 5. Direct import connections of important files
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for fi in imports {
        for t in &fi.targets {
            adjacency.entry(fi.source.clone()).or_default().push(t.clone());
            adjacency.entry(t.clone()).or_default().push(fi.source.clone());
        }
    }

    let mut with_neighbours: BTreeSet<String> = important.clone();
    for f in &important {
        if let Some(neighbours) = adjacency.get(f) {
            for n in neighbours {
                if all_files.contains(n) {
                    with_neighbours.insert(n.clone());
                }
            }
        }
    }

    with_neighbours
}

/// Compute the longest common directory prefix across a set of file paths.
fn common_path_prefix<'a, I>(paths: I) -> String
where
    I: Iterator<Item = &'a String>,
{
    let mut iter = paths.peekable();
    let first = match iter.peek() {
        Some(s) => s.as_str(),
        None => return String::new(),
    };
    let mut prefix = first.to_string();
    for path in iter {
        while !path.starts_with(&prefix) {
            // Trim to parent directory: strip trailing '/' first, then find last '/'
            let trimmed = prefix.trim_end_matches('/');
            if let Some(pos) = trimmed.rfind('/') {
                prefix.truncate(pos + 1);
            } else {
                return String::new();
            }
        }
    }
    // Keep trailing slash if it's a directory prefix
    if !prefix.ends_with('/') && !prefix.is_empty() {
        if let Some(pos) = prefix.rfind('/') {
            prefix.truncate(pos + 1);
        } else {
            prefix.clear();
        }
    }
    prefix
}

/// Escape characters that could break Mermaid syntax in subgraph labels
/// and other structural elements.
fn escape_mermaid_label(s: &str) -> String {
    s.replace('\n', " ")
     .replace('\r', " ")
     .replace('&', "&amp;")
     .replace('#', "&#35;")
     .replace('"', "&quot;")
     .replace('[', "&#91;")
     .replace(']', "&#93;")
     .replace('{', "&#123;")
     .replace('}', "&#125;")
}

/// Return a short display name by stripping the common prefix.
fn display_name(path: &str, prefix: &str) -> String {
    let raw = if prefix.is_empty() { path.to_string() } else { path.strip_prefix(prefix).unwrap_or(path).to_string() };
    // Escape characters that could break Mermaid node labels.
    // V7-3: `#` must be escaped because Mermaid interprets `#quot;` etc. as
    // entity references, allowing injection of quotes and other chars.
    // V7-9: `&` must be escaped FIRST to prevent injection via HTML entities.
    // Order matters: `&` and `#` must be escaped before `"`, `[]`, `{}`
    // whose replacement strings contain `#` (e.g. `&#123;`).
    raw.replace('\n', " ")
       .replace('\r', " ")
       .replace('&', "&amp;")
       .replace('#', "&#35;")
       .replace('"', "&quot;")
       .replace(']', "&#93;")
       .replace('[', "&#91;")
       .replace('{', "&#123;")
       .replace('}', "&#125;")
}

/// Group files into directory buckets for Mermaid subgraphs.
///
/// Returns a `BTreeMap` keyed by directory (relative to `prefix`), preserving
/// a stable ordering.
fn group_by_directory(files: &BTreeSet<String>, prefix: &str) -> BTreeMap<String, Vec<String>> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in files {
        let relative = if prefix.is_empty() { path.as_str() } else { path.strip_prefix(prefix).unwrap_or(path) };
        let dir = relative.rfind('/').map(|pos| relative[..pos].to_string()).unwrap_or_default();
        groups.entry(dir).or_default().push(path.clone());
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::path::PathBuf;

    fn make_minimal_output() -> AnalysisOutput {
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
                dead_code: vec![], unused_exports: vec![], duplicate_exports: vec![],
                duplicate_code: vec![], gotchas: vec![], unused_types: vec![],
                circular_dependencies: vec![], unused_dependencies: vec![],
                unresolved_imports: vec![], unlisted_dependencies: vec![], plugin_issues: vec![],
            },
            duplication: DuplicationSection {
                stats: DuplicationStats {
                    total_lines: 0, duplicated_lines: 0, duplication_percentage: 0.0,
                    clone_groups: 0, clone_instances: 0, clone_families: 0,
                },
                clone_groups: vec![], clone_families: vec![], mirrored_directories: vec![],
            },
        }
    }

    #[test]
    fn common_prefix_finds_shared_dir() {
        let files = vec!["src/main.rs".to_string(), "src/utils.rs".to_string(), "src/lib/mod.rs".to_string()];
        let prefix = common_path_prefix(files.iter());
        assert_eq!(prefix, "src/");
    }

    #[test]
    fn common_prefix_no_common() {
        let files = vec!["a.rs".to_string(), "b.rs".to_string()];
        let prefix = common_path_prefix(files.iter());
        assert_eq!(prefix, "");
    }

    #[test]
    fn display_name_strips_prefix() {
        assert_eq!(display_name("src/main.rs", "src/"), "main.rs");
        assert_eq!(display_name("src/main.rs", ""), "src/main.rs");
    }

    #[test]
    fn group_by_directory_works() {
        let mut files: BTreeSet<String> = BTreeSet::new();
        files.insert("src/main.rs".to_string());
        files.insert("src/utils.rs".to_string());
        files.insert("src/lib/a.rs".to_string());
        files.insert("src/lib/b.rs".to_string());

        let groups = group_by_directory(&files, "src/");
        assert_eq!(groups.get("").unwrap().len(), 2); // main.rs, utils.rs
        assert_eq!(groups.get("lib").unwrap().len(), 2); // lib/a.rs, lib/b.rs
    }

    #[test]
    fn sec_mermaid_escapes_quotes() {
        // File paths with quotes or "] chars could break mermaid syntax
        let evil_path = "src/evil\"] --> evilNode[\"evil";
        let output = AnalysisOutput {
            version: None,
            summary: None,
            detected_frameworks: None,
            monorepo: None,
            structure: Structure {
                root: PathBuf::from("/project"),
                entry_points: vec![evil_path.to_string()],
                implicit_entries: vec![],
                source_files: vec![],
                config_files: vec![],
            },
            dependencies: Dependencies {
                imports: vec![FileImports {
                    source: evil_path.to_string(),
                    targets: vec!["src/other.ts".to_string()],
                }],
                external: vec![],
            },
            quality: Quality { files: vec![] },
            issues: Issues {
                dead_code: vec![],
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
            },
        };
        let formatter = MermaidFormatter;
        let result = formatter.format(&output).unwrap();
        // Raw "] should not appear to close a node label prematurely
        assert!(!result.contains("evil\"] --> evilNode"),
            "mermaid should escape quotes/brackets in node labels, got:\n{}", result);
    }

    // ── V4-4: Subgraph label injection via directory names with quotes ──
    #[test]
    fn sec_mermaid_subgraph_label_escapes_special() {
        // Directory name containing quotes/brackets should not break Mermaid syntax
        let evil_dir = "src/evil\"dir";
        let mut output = make_minimal_output();
        output.dependencies.imports = vec![FileImports {
            source: format!("{}/a.ts", evil_dir),
            targets: vec![format!("{}/b.ts", evil_dir)],
        }];
        let formatter = MermaidFormatter;
        let result = formatter.format(&output).unwrap();
        // Raw unescaped quote in subgraph label would break mermaid parsing
        assert!(!result.contains("subgraph src/evil\"dir"),
            "subgraph label should escape quotes, got:\n{}", result);
    }

    // ── V4-9: display_name doesn't escape curly braces ──
    #[test]
    fn sec_mermaid_escapes_curly_braces() {
        let name = display_name("src/file{evil}.ts", "");
        assert!(!name.contains('{') && !name.contains('}'),
            "curly braces should be escaped in display names, got: {}", name);
        assert!(name.contains("&#123;") && name.contains("&#125;"),
            "should use HTML entities for curly braces, got: {}", name);
    }

    // ── V5-10: display_name sanitizes newlines to prevent chart breakage ──
    #[test]
    fn sec_mermaid_escapes_newlines() {
        let name = display_name("src/evil\nINJECTED.ts", "");
        assert!(!name.contains('\n'),
            "newlines should be replaced with spaces, got: {}", name);
        assert!(name.contains("INJECTED"),
            "content should be preserved, got: {}", name);
    }

    // ── V6-1: escape_mermaid_label must escape newlines to prevent subgraph injection ──
    #[test]
    fn sec_mermaid_label_escapes_newlines() {
        let evil = "src/evil\nend\nsubgraph fake";
        let escaped = escape_mermaid_label(evil);
        assert!(!escaped.contains('\n'),
            "newlines should be replaced with spaces, got: {:?}", escaped);
        assert!(escaped.contains("fake"),
            "content should be preserved, got: {:?}", escaped);
    }

    // ── V6-7: escape_mermaid_label must escape # to prevent Mermaid entity injection ──
    #[test]
    fn sec_mermaid_label_escapes_hash() {
        let evil = "dir#name";
        let escaped = escape_mermaid_label(evil);
        // After escaping, the raw '#' should be gone, replaced with entity
        assert!(!escaped.contains("#n"),
            "# followed by text should be escaped, got: {:?}", escaped);
        assert!(escaped.contains("&#35;"),
            "# should become &#35;, got: {:?}", escaped);
    }

    // ── V7-3: display_name must escape # to prevent Mermaid entity injection ──
    #[test]
    fn sec_mermaid_display_name_escapes_hash() {
        // In Mermaid, `#quot;` is interpreted as a literal `"`. A file path
        // containing `#quot;` would inject a quote into the node label, breaking
        // the chart structure and allowing label injection.
        let name = display_name("src/file#quot;.ts", "");
        // After escaping, `#` should become `&#35;`
        assert!(name.contains("&#35;"),
            "# should become &#35;, got: {}", name);
        // No raw `#` should remain (it's been replaced with entity)
        let without_entities = name.replace("&amp;", "").replace("&#35;", "").replace("&quot;", "");
        assert!(!without_entities.contains('#'),
            "no raw # should remain, got: {}", name);
    }

    // ── V7-9: display_name and escape_mermaid_label must escape & ──
    #[test]
    fn sec_mermaid_display_name_escapes_ampersand() {
        // In Mermaid, `&` starts HTML entities. A path like `foo&quot;.ts` would
        // have `&quot;` decoded to `"`, injecting a quote into the node label.
        let name = display_name("src/foo&quot;.ts", "");
        // After escaping, `&` should be `&amp;` so Mermaid doesn't decode the entity
        assert!(name.contains("&amp;"),
            "& should become &amp;, got: {}", name);
        // Verify no raw `&` followed by a letter remains (which would be an entity)
        let raw_amp = name.replace("&amp;", "").replace("&quot;", "").replace("&#35;", "").replace("&#91;", "").replace("&#93;", "").replace("&#123;", "").replace("&#125;", "");
        assert!(!raw_amp.contains('&'),
            "no unescaped & should remain, got: {}", name);
    }

    #[test]
    fn sec_mermaid_label_escapes_ampersand() {
        let escaped = escape_mermaid_label("foo&amp;evil");
        // After escaping, every original `&` should be `&amp;`
        assert!(escaped.contains("&amp;"),
            "& should become &amp;, got: {:?}", escaped);
        // The result should contain &amp;amp; because the original & in &amp; gets escaped too
        assert!(escaped.contains("&amp;amp;"),
            "nested & should be double-escaped, got: {:?}", escaped);
    }
}
