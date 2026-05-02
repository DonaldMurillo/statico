//! Pure functions for Mermaid output escaping.
//!
//! Extracted for testability — these functions have no side effects.

/// Find the common path prefix shared by a set of file paths.
/// Returns "" if no common prefix exists (other than the empty string).
pub fn common_path_prefix<'a>(paths: impl Iterator<Item = &'a String>) -> String {
    let mut prefix = String::new();
    let mut first = true;
    for path in paths {
        if first {
            // Find the directory portion of the first path
            prefix = path.rfind('/').map(|pos| path[..=pos].to_string()).unwrap_or_default();
            first = false;
            continue;
        }
        // Shorten prefix until it matches the start of this path
        loop {
            if path.starts_with(&prefix) {
                break;
            }
            // Remove last component from prefix
            if let Some(pos) = prefix[..prefix.len().saturating_sub(1)].rfind('/') {
                prefix.truncate(pos + 1);
            } else {
                prefix.clear();
                break;
            }
        }
        if prefix.is_empty() {
            break;
        }
    }
    prefix
}

/// Escape characters that could break Mermaid syntax in subgraph labels
/// and other structural elements.
pub fn escape_mermaid_label(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
     .replace('&', "&amp;")
     .replace('#', "&#35;")
     .replace('"', "&quot;")
     .replace('[', "&#91;")
     .replace(']', "&#93;")
     .replace('{', "&#123;")
     .replace('}', "&#125;")
}

/// Return a short display name by stripping the common prefix.
pub fn display_name(path: &str, prefix: &str) -> String {
    let raw = if prefix.is_empty() { path.to_string() } else { path.strip_prefix(prefix).unwrap_or(path).to_string() };
    raw.replace(['\n', '\r'], " ")
       .replace('&', "&amp;")
       .replace('#', "&#35;")
       .replace('"', "&quot;")
       .replace(']', "&#93;")
       .replace('[', "&#91;")
       .replace('{', "&#123;")
       .replace('}', "&#125;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_prefix_finds_shared_dir() {
        let files = ["src/main.rs".to_string(), "src/utils.rs".to_string(), "src/lib/mod.rs".to_string()];
        let prefix = common_path_prefix(files.iter());
        assert_eq!(prefix, "src/");
    }

    #[test]
    fn common_prefix_no_common() {
        let files = ["a.rs".to_string(), "b.rs".to_string()];
        let prefix = common_path_prefix(files.iter());
        assert_eq!(prefix, "");
    }

    #[test]
    fn display_name_strips_prefix() {
        assert_eq!(display_name("src/main.rs", "src/"), "main.rs");
        assert_eq!(display_name("src/main.rs", ""), "src/main.rs");
    }

    #[test]
    fn sec_mermaid_escapes_curly_braces() {
        let name = display_name("src/file{evil}.ts", "");
        assert!(!name.contains('{'), "should escape {{, got: {}", name);
        assert!(!name.contains('}'), "should escape }}, got: {}", name);
        assert!(name.contains("&#123;"), "should use HTML entities for curly braces, got: {}", name);
    }

    #[test]
    fn sec_mermaid_escapes_newlines() {
        let name = display_name("src/file\nname.ts", "");
        assert!(!name.contains('\n'), "should not contain literal newline, got: {:?}", name);
    }

    #[test]
    fn sec_mermaid_label_escapes_newlines() {
        let escaped = escape_mermaid_label("hello\nworld");
        assert_eq!(escaped, "hello world", "should replace newlines with spaces");
        let escaped2 = escape_mermaid_label("hello\r\nworld");
        assert_eq!(escaped2, "hello  world", "should replace \\r\\n with two spaces");
    }

    #[test]
    fn sec_mermaid_label_escapes_hash() {
        let escaped = escape_mermaid_label("file#name.ts");
        assert!(escaped.contains("&#35;"), "# should be escaped, got: {}", escaped);
    }

    #[test]
    fn sec_mermaid_display_name_escapes_hash() {
        let name = display_name("src/file#quot;.ts", "");
        assert!(name.contains("&#35;"),
            "# should become &#35;, got: {}", name);
        let without_entities = name.replace("&amp;", "").replace("&#35;", "").replace("&quot;", "");
        assert!(!without_entities.contains('#'),
            "no raw # should remain, got: {}", name);
    }

    #[test]
    fn sec_mermaid_display_name_escapes_ampersand() {
        let name = display_name("src/foo&quot;.ts", "");
        assert!(name.contains("&amp;"),
            "& should become &amp;, got: {}", name);
        let raw_amp = name.replace("&amp;", "").replace("&quot;", "").replace("&#35;", "").replace("&#91;", "").replace("&#93;", "").replace("&#123;", "").replace("&#125;", "");
        assert!(!raw_amp.contains('&'),
            "no unescaped & should remain, got: {}", name);
    }

    #[test]
    fn sec_mermaid_label_escapes_ampersand() {
        let escaped = escape_mermaid_label("foo&amp;evil");
        assert!(escaped.contains("&amp;"),
            "& should become &amp;, got: {:?}", escaped);
        assert!(escaped.contains("&amp;amp;"),
            "nested & should be double-escaped, got: {:?}", escaped);
    }
}
