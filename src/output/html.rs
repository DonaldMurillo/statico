//! Self-contained HTML interactive report formatter.

use crate::output::{OutputFormatter, compute_summary};
use crate::types::AnalysisOutput;

/// HTML interactive report formatter.
pub struct HtmlFormatter;

impl OutputFormatter for HtmlFormatter {
    fn format(&self, output: &AnalysisOutput) -> Result<String, String> {
        let summary = compute_summary(output);
        let json_data = serde_json::to_string(output).map_err(|e| format!("failed to serialize: {}", e))?
            .replace("</", "<\\/")  // Prevent </script> injection (S3-01)
            .replace("<!--", "<\\x21--"); // Prevent HTML comment injection in script
        let summary_json =
            serde_json::to_string(&summary).map_err(|e| format!("failed to serialize summary: {}", e))?
                .replace("</", "<\\/")
                .replace("<!--", "<\\x21--");

        Ok(format!(
            r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>statico Report</title>
<style>
:root {{ --bg:#1a1a2e; --card:#16213e; --text:#e0e0e0; --accent:#0f3460; --warn:#f0a500;
--danger:#e94560; --success:#4ecca3; --border:#2a2a4a; --muted:#888; }}
[data-theme="light"] {{ --bg:#f5f5f5; --card:#fff; --text:#222; --accent:#3a86ff;
--warn:#ff9f1c; --danger:#ef476f; --success:#06d6a0; --border:#ddd; --muted:#666; }}
* {{ margin:0; padding:0; box-sizing:border-box; }}
body {{ font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;
background:var(--bg); color:var(--text); line-height:1.6; padding:2rem; }}
.container {{ max-width:1200px; margin:0 auto; }}
h1 {{ margin-bottom:1rem; }}
.toggle {{ position:fixed; top:1rem; right:1rem; background:var(--card); color:var(--text);
border:1px solid var(--border); padding:0.5rem 1rem; border-radius:6px; cursor:pointer; }}
.cards {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(200px,1fr)); gap:1rem; margin:1.5rem 0; }}
.card {{ background:var(--card); border:1px solid var(--border); border-radius:8px; padding:1.2rem; }}
.card h3 {{ font-size:0.85rem; color:var(--muted); text-transform:uppercase; margin-bottom:0.5rem; }}
.card .value {{ font-size:1.8rem; font-weight:700; }}
.card.danger .value {{ color:var(--danger); }}
.card.warn .value {{ color:var(--warn); }}
.card.success .value {{ color:var(--success); }}
.section {{ background:var(--card); border:1px solid var(--border); border-radius:8px;
margin:1rem 0; overflow:hidden; }}
.section-header {{ padding:1rem 1.2rem; cursor:pointer; display:flex; justify-content:space-between;
align-items:center; font-weight:600; border-bottom:1px solid var(--border); }}
.section-header::after {{ content:"▼"; transition:transform 0.2s; }}
.section-header.collapsed::after {{ transform:rotate(-90deg); }}
.section-body {{ padding:1rem 1.2rem; }}
.section-body.hidden {{ display:none; }}
table {{ width:100%; border-collapse:collapse; font-size:0.9rem; }}
th,td {{ text-align:left; padding:0.5rem 0.75rem; border-bottom:1px solid var(--border); }}
th {{ color:var(--muted); font-size:0.8rem; text-transform:uppercase; }}
.heatmap {{ display:grid; gap:0.5rem; }}
.heat-item {{ display:flex; justify-content:space-between; padding:0.4rem 0.6rem;
border-radius:4px; font-size:0.85rem; }}
.heat-low {{ background:rgba(78,204,163,0.15); }}
.heat-med {{ background:rgba(240,165,0,0.15); }}
.heat-high {{ background:rgba(233,69,96,0.15); }}
code {{ background:var(--accent); padding:0.1rem 0.3rem; border-radius:3px; font-size:0.85em; }}
</style>
</head>
<body data-theme="dark">
<button class="toggle" onclick="toggleTheme()">🌓 Toggle Theme</button>
<div class="container">
<h1>🔍 statico Report</h1>
<div id="cards" class="cards"></div>
<div id="sections"></div>
</div>
<script>
const DATA = {json_data};
const SUMMARY = {summary_json};
function toggleTheme() {{
  const b = document.body;
  b.setAttribute('data-theme', b.getAttribute('data-theme') === 'dark' ? 'light' : 'dark');
}}
function esc(s) {{ const d = document.createElement('div'); d.textContent = s; return d.innerHTML; }}
function render() {{
  const issues = DATA.issues;
  const dup = DATA.duplication;
  // Cards
  const cardsEl = document.getElementById('cards');
  const cards = [
    {{ label:'Files', value:SUMMARY.total_files, cls:'' }},
    {{ label:'Lines of Code', value:SUMMARY.total_lines, cls:'' }},
    {{ label:'Duplication', value:SUMMARY.duplication_percentage.toFixed(1)+'%', cls:SUMMARY.duplication_percentage>10?'warn':'success' }},
    {{ label:'Health Score', value:SUMMARY.health_score.toFixed(1), cls:SUMMARY.health_score>=80?'success':SUMMARY.health_score>=50?'warn':'danger' }},
    {{ label:'Dead Code', value:issues.dead_code.length, cls:issues.dead_code.length>0?'danger':'success' }},
    {{ label:'Unused Exports', value:issues.unused_exports.length, cls:issues.unused_exports.length>0?'warn':'success' }},
    {{ label:'Gotchas', value:issues.gotchas.length, cls:issues.gotchas.length>0?'warn':'success' }},
    {{ label:'Circular Deps', value:issues.circular_dependencies.length, cls:issues.circular_dependencies.length>0?'danger':'success' }},
  ];
  cardsEl.innerHTML = cards.map(c =>
    '<div class="card '+c.cls+'"><h3>'+c.label+'</h3><div class="value">'+c.value+'</div></div>'
  ).join('');
  // Sections
  const sectionsEl = document.getElementById('sections');
  let html = '';
  // Heat map
  const fileIssueCount = {{}};
  issues.dead_code.forEach(i => fileIssueCount[i.path] = (fileIssueCount[i.path]||0)+1);
  issues.gotchas.forEach(i => fileIssueCount[i.file] = (fileIssueCount[i.file]||0)+1);
  issues.unused_exports.forEach(i => fileIssueCount[i.path] = (fileIssueCount[i.path]||0)+1);
  const heatEntries = Object.entries(fileIssueCount).sort((a,b)=>b[1]-a[1]).slice(0,20);
  if (heatEntries.length) {{
    html += section('file-heat','File Heat Map', '<div class="heatmap">' +
      heatEntries.map(([f,c]) => {{
        const cls = c >= 5 ? 'heat-high' : c >= 2 ? 'heat-med' : 'heat-low';
        return '<div class="heat-item '+cls+'"><code>'+esc(f)+'</code><span>'+c+' issues</span></div>';
      }}).join('') + '</div>');
  }}
  // Dead code table
  if (issues.dead_code.length) {{
    const sorted = [...issues.dead_code].sort((a,b)=>b.lines_of_code-a.lines_of_code).slice(0,30);
    html += section('dead','Dead Code ('+issues.dead_code.length+')', '<table><tr><th>File</th><th>Lines</th><th>Confidence</th></tr>' +
      sorted.map(i => '<tr><td><code>'+esc(i.path)+'</code></td><td>'+i.lines_of_code+'</td><td>'+( i.confidence*100).toFixed(0)+'%</td></tr>').join('') + '</table>');
  }}
  // Unused exports
  if (issues.unused_exports.length) {{
    html += section('uexp','Unused Exports ('+issues.unused_exports.length+')', '<table><tr><th>Export</th><th>File</th></tr>' +
      issues.unused_exports.slice(0,30).map(i => '<tr><td>'+esc(i.name)+'</td><td><code>'+esc(i.path)+'</code></td></tr>').join('') + '</table>');
  }}
  // Clone groups
  if (dup.clone_groups.length) {{
    const sorted = [...dup.clone_groups].sort((a,b)=>b.line_count-a.line_count).slice(0,15);
    html += section('dup','Duplication ('+dup.clone_groups.length+' groups)', '<table><tr><th>Files</th><th>Lines</th></tr>' +
      sorted.map(g => '<tr><td>'+g.instances.map(i=>'<code>'+esc(i.file)+':L'+i.start_line+'</code>').join('<br>')+'</td><td>'+g.line_count+'</td></tr>').join('') + '</table>');
  }}
  // Circular deps
  if (issues.circular_dependencies.length) {{
    html += section('circ','Circular Dependencies ('+issues.circular_dependencies.length+')',
      issues.circular_dependencies.map(i => '<div><code>'+i.files.map(esc).join(' → ')+'</code></div>').join(''));
  }}
  sectionsEl.innerHTML = html;
}}
function section(id, title, body) {{
  return '<div class="section"><div class="section-header" onclick="toggleSection(this)">'+esc(title)+'</div><div class="section-body">'+body+'</div></div>';
}}
function toggleSection(el) {{
  el.classList.toggle('collapsed');
  el.nextElementSibling.classList.toggle('hidden');
}}
render();
</script>
</body>
</html>"##
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::path::PathBuf;

    fn make_output_with_path(path: &str) -> AnalysisOutput {
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
                dead_code: vec![DeadCodeIssue {
                    path: path.to_string(),
                    lines_of_code: 42,
                    confidence: 0.9,
                    reason: "unused".to_string(),
                }],
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
                    total_lines: 0,
                    duplicated_lines: 0,
                    duplication_percentage: 0.0,
                    clone_groups: 0,
                    clone_instances: 0,
                    clone_families: 0,
                },
                clone_groups: vec![],
                clone_families: vec![],
                mirrored_directories: vec![],
            },
        }
    }

    #[test]
    fn sec_html_escapes_script_injection() {
        // A file path containing </script> should not break out of the script tag
        let output = make_output_with_path("test/</script><script>alert(1)//");
        let formatter = HtmlFormatter;
        let html = formatter.format(&output).unwrap();
        // The raw </script> string should NOT appear in the output
        assert!(!html.contains("</script><script>alert(1)"),
            "HTML output should not contain raw </script> injection");
        // The escaped version <\/ should be used instead
        assert!(html.contains("<\\/") || !html.contains("</script><script>"),
            "JSON should have escaped forward slashes");
    }

    #[test]
    fn sec_html_no_raw_script_close_in_json() {
        let output = make_output_with_path("normal/path.ts");
        let formatter = HtmlFormatter;
        let html = formatter.format(&output).unwrap();
        // Find the JSON data assignment
        let script_start = html.find("const DATA = ").expect("should find DATA assignment");
        let script_end = html.find(";\nfunction toggleTheme").expect("should find end of DATA");
        let json_fragment = &html[script_start..script_end];
        // No raw </ sequence in the JSON data (should be <\/ instead)
        assert!(!json_fragment.contains("</script"),
            "JSON in HTML should not contain unescaped </script");
    }

    #[test]
    fn sec_html_escapes_comment_injection() {
        // A file path containing <!-- should not break out of script tag
        let output = make_output_with_path("test/<!--<script>alert(1)//");
        let formatter = HtmlFormatter;
        let html = formatter.format(&output).unwrap();
        assert!(!html.contains("<!--<script>"),
            "HTML comment should be escaped, not raw in output");
    }
}
