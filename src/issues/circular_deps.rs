//! Circular dependency detection.
//!
//! Uses DFS with three-state coloring (WHITE/GRAY/BLACK) to find cycles
//! in the import dependency graph.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::types::CircularDepIssue;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    White,
    Gray,
    Black,
}

/// Detect cycles in the import dependency graph.
///
/// `dep_graph` maps each file to the list of files it imports.
/// Returns one `CircularDepIssue` per distinct cycle found.
pub fn detect(dep_graph: &BTreeMap<String, Vec<String>>) -> Vec<CircularDepIssue> {
    let mut color: HashMap<String, Color> = HashMap::new();
    for key in dep_graph.keys() {
        color.insert(key.clone(), Color::White);
    }

    let mut cycles: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut stack: Vec<String> = Vec::new();

    for start in dep_graph.keys() {
        if color.get(start).copied() == Some(Color::White) {
            dfs(start, dep_graph, &mut color, &mut stack, &mut cycles);
        }
    }

    cycles
        .into_iter()
        .map(|files| CircularDepIssue { files })
        .collect()
}

fn dfs(
    node: &str,
    dep_graph: &BTreeMap<String, Vec<String>>,
    color: &mut HashMap<String, Color>,
    stack: &mut Vec<String>,
    cycles: &mut BTreeSet<Vec<String>>,
) {
    color.insert(node.to_string(), Color::Gray);
    stack.push(node.to_string());

    if let Some(deps) = dep_graph.get(node) {
        for dep in deps {
            // Skip self-loops.
            if dep == node {
                continue;
            }

            let dep_color = color.get(dep).copied().unwrap_or(Color::White);

            match dep_color {
                Color::Gray => {
                    // Back-edge found — reconstruct the cycle.
                    if let Some(pos) = stack.iter().position(|s| s == dep) {
                        let cycle: Vec<String> = stack[pos..].to_vec();
                        // Normalize: rotate so the lexicographically smallest element is first.
                        cycles.insert(normalize_cycle(cycle));
                    }
                }
                Color::White => {
                    dfs(dep, dep_graph, color, stack, cycles);
                }
                Color::Black => {}
            }
        }
    }

    stack.pop();
    color.insert(node.to_string(), Color::Black);
}

/// Normalize a cycle by rotating it so the smallest path comes first.
fn normalize_cycle(mut cycle: Vec<String>) -> Vec<String> {
    if cycle.is_empty() {
        return cycle;
    }
    let min_pos = cycle
        .iter()
        .enumerate()
        .min_by_key(|(_, v)| *v)
        .map(|(i, _)| i)
        .unwrap_or(0);
    cycle.rotate_left(min_pos);
    cycle
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_cycles() {
        let graph = BTreeMap::from([
            ("a.ts".into(), vec!["b.ts".into()]),
            ("b.ts".into(), vec!["c.ts".into()]),
            ("c.ts".into(), vec![]),
        ]);
        let cycles = detect(&graph);
        assert!(cycles.is_empty());
    }

    #[test]
    fn simple_cycle() {
        let graph = BTreeMap::from([
            ("a.ts".into(), vec!["b.ts".into()]),
            ("b.ts".into(), vec!["a.ts".into()]),
        ]);
        let cycles = detect(&graph);
        assert_eq!(cycles.len(), 1);
        // Cycle should contain both files.
        let files = &cycles[0].files;
        assert!(files.contains(&"a.ts".to_string()));
        assert!(files.contains(&"b.ts".to_string()));
    }

    #[test]
    fn three_node_cycle() {
        let graph = BTreeMap::from([
            ("a.ts".into(), vec!["b.ts".into()]),
            ("b.ts".into(), vec!["c.ts".into()]),
            ("c.ts".into(), vec!["a.ts".into()]),
        ]);
        let cycles = detect(&graph);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].files.len(), 3);
    }

    #[test]
    fn self_loop_skipped() {
        let graph = BTreeMap::from([
            ("a.ts".into(), vec!["a.ts".into()]),
        ]);
        let cycles = detect(&graph);
        assert!(cycles.is_empty());
    }

    #[test]
    fn empty_graph() {
        let graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let cycles = detect(&graph);
        assert!(cycles.is_empty());
    }

    #[test]
    fn cycle_with_branch() {
        // a → b → c → a, and b → d (no cycle through d).
        let graph = BTreeMap::from([
            ("a.ts".into(), vec!["b.ts".into()]),
            ("b.ts".into(), vec!["c.ts".into(), "d.ts".into()]),
            ("c.ts".into(), vec!["a.ts".into()]),
            ("d.ts".into(), vec![]),
        ]);
        let cycles = detect(&graph);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].files.len(), 3);
    }
}
