/// Ensure a path is within a root directory (prevents path traversal attacks).
/// Canonicalizes both paths and checks that `path` starts with `root`.
pub fn ensure_within_root(path: &std::path::Path, root: &std::path::Path) -> Result<(), String> {
    // Try canonicalize first (works for existing paths, resolves symlinks).
    // Fall back to lexical normalization for non-existent paths.
    if let (Ok(canonical), Ok(canonical_root)) =
        (std::fs::canonicalize(path), std::fs::canonicalize(root))
    {
        if !canonical.starts_with(&canonical_root) {
            return Err(format!("path '{}' escapes project root", path.display()));
        }
        return Ok(());
    }

    // Fallback: path or root doesn't exist on disk.
    // Check the relative suffix after stripping the root prefix.
    // If path starts with root, check the remaining components for `..` traversal.
    let suffix = path.strip_prefix(root).unwrap_or(path);
    let mut depth = 0i32;
    for component in suffix.components() {
        match component {
            std::path::Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return Err(format!(
                        "path '{}' escapes project root",
                        path.display()
                    ));
                }
            }
            std::path::Component::Normal(_) => depth += 1,
            std::path::Component::RootDir => {
                // Absolute path that isn't under root — only ok if path == root.
                if path != root {
                    return Err(format!(
                        "path '{}' is absolute outside project root",
                        path.display()
                    ));
                }
            }
            std::path::Component::CurDir => {
                // `.` doesn't change depth — no action needed.
            }
            std::path::Component::Prefix(_) => {}
        }
    }
    Ok(())
}

pub mod analyzer;
pub mod cache;
pub mod config;
pub mod discovery;
pub mod duplication;
pub mod frameworks;
pub mod issues;
pub mod languages;
pub mod monorepo;
pub mod output;
pub mod plugin;
pub mod parse;
pub mod progress;
pub mod resolution;
pub mod tui;
pub mod types;
pub mod update;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn sec_ensure_within_root_allows_child() {
        let root = Path::new("/project");
        let child = Path::new("/project/src/index.ts");
        // These may not exist, so only the lexical path runs
        assert!(ensure_within_root(child, root).is_ok());
    }

    #[test]
    fn sec_ensure_within_root_rejects_parent_traversal() {
        let root = Path::new("/project");
        let evil = Path::new("/project/../../../etc/passwd");
        let result = ensure_within_root(evil, root);
        assert!(result.is_err(), "should reject parent dir traversal");
    }

    #[test]
    fn sec_ensure_within_root_rejects_absolute_escape() {
        let root = Path::new("/project");
        let evil = Path::new("/etc/passwd");
        let result = ensure_within_root(evil, root);
        assert!(result.is_err(), "should reject absolute path outside root");
    }

    #[test]
    fn sec_ensure_within_root_rejects_dotdot_in_middle() {
        let root = Path::new("/project");
        let evil = Path::new("/project/src/../../etc/passwd");
        let result = ensure_within_root(evil, root);
        assert!(result.is_err(), "should reject ../ in middle");
    }

    #[test]
    fn sec_ensure_within_root_allows_valid_subpath() {
        let root = Path::new("/project");
        let child = Path::new("/project/.statico/plugins/my-rule/index.ts");
        assert!(ensure_within_root(child, root).is_ok());
    }
}

/// Strip ANSI escape sequences and control characters from a string.
/// Used to sanitize plugin-provided text before printing to terminal.
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                // CSI sequence: ESC [ ... (final byte 0x40-0x7E)
                Some('[') => {
                    chars.next(); // consume '['
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next >= '\x40' && next <= '\x7e' { break; }
                    }
                }
                // OSC sequence: ESC ] ... (terminated by BEL/0x07 or ST/ESC\)
                Some(']') => {
                    chars.next(); // consume ']'
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next == '\x07' { break; } // BEL terminator
                        if next == '\x1b' {
                            // ST is ESC backslash
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                                break;
                            }
                        }
                    }
                }
                // Other two-character ESC sequences (ESC c, ESC 7, etc.)
                _ => {
                    // Consume the single following character
                    chars.next();
                }
            }
            continue;
        }
        if c.is_control() && c != '\n' && c != '\r' && c != '\t' {
            continue; // strip other control chars
        }
        result.push(c);
    }
    result
}

#[cfg(test)]
mod strip_ansi_tests {
    use super::*;

    #[test]
    fn sec_v4_1_strips_ansi_escape_from_plugin_message() {
        // A plugin could inject ANSI escapes to change terminal color, move cursor, etc.
        let evil = "\x1b[31mCRITICAL ERROR\x1b[0m";
        let clean = strip_ansi(evil);
        assert_eq!(clean, "CRITICAL ERROR",
            "ANSI color codes should be stripped, got: {:?}", clean);
    }

    #[test]
    fn sec_v4_1_strips_cursor_movement_ansi() {
        let evil = "\x1b[2J\x1b[H\x1b[31mFAKE ERROR\x1b[0m";
        let clean = strip_ansi(evil);
        assert_eq!(clean, "FAKE ERROR",
            "ANSI cursor movement should be stripped, got: {:?}", clean);
    }

    #[test]
    fn sec_v4_8_strips_ansi_from_plugin_name() {
        // A plugin directory named with ANSI escapes
        let evil_name = "\x1b[32m\x1b[1mmalicious\x1b[0m";
        let clean = strip_ansi(evil_name);
        assert_eq!(clean, "malicious",
            "ANSI in plugin names should be stripped, got: {:?}", clean);
    }

    #[test]
    fn strip_ansi_preserves_normal_text() {
        assert_eq!(strip_ansi("hello world"), "hello world");
        assert_eq!(strip_ansi("file.ts:42"), "file.ts:42");
    }

    #[test]
    fn strip_ansi_strips_control_chars() {
        let clean = strip_ansi("hello\x07world\x00test");
        assert_eq!(clean, "helloworldtest",
            "control chars should be stripped, got: {:?}", clean);
    }

    #[test]
    fn strip_ansi_preserves_newlines() {
        assert_eq!(strip_ansi("hello\nworld"), "hello\nworld");
    }

    // ── V6-6: strip_ansi must handle OSC sequences ──
    #[test]
    fn sec_v6_6_strip_ansi_handles_osc_sequences() {
        // OSC sequence: ESC ] 0 ; title BEL — sets terminal title
        let evil = "\x1b]0;evil-title\x07visible";
        let clean = strip_ansi(evil);
        assert_eq!(clean, "visible",
            "OSC sequence should be fully stripped, got: {:?}", clean);
    }

    #[test]
    fn sec_v6_6_strip_ansi_handles_osc_st_terminator() {
        // OSC sequence with ST terminator: ESC ] 0 ; title ESC \
        let evil = "\x1b]0;evil-title\x1b\\visible";
        let clean = strip_ansi(evil);
        assert_eq!(clean, "visible",
            "OSC sequence with ST terminator should be fully stripped, got: {:?}", clean);
    }

    #[test]
    fn sec_v6_6_strip_ansi_handles_two_char_esc() {
        // Two-character ESC sequence: ESC c (terminal reset)
        let evil = "\x1bcremaining";
        let clean = strip_ansi(evil);
        assert_eq!(clean, "remaining",
            "two-char ESC sequence should be fully stripped, got: {:?}", clean);
    }
}
