//! ANSI escape sequence stripping for sanitizing plugin-provided text.

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
                        if ('\x40'..='\x7e').contains(&next) {
                            break;
                        }
                    }
                }
                // OSC sequence: ESC ] ... (terminated by BEL/0x07 or ST/ESC\)
                Some(']') => {
                    chars.next(); // consume ']'
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next == '\x07' {
                            break;
                        } // BEL terminator
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
mod tests {
    use super::*;

    #[test]
    fn sec_ansi_strips_escape_from_plugin_message() {
        let evil = "\x1b[31mCRITICAL ERROR\x1b[0m";
        let clean = strip_ansi(evil);
        assert_eq!(clean, "CRITICAL ERROR", "ANSI color codes should be stripped, got: {:?}", clean);
    }

    #[test]
    fn sec_ansi_strips_cursor_movement() {
        let evil = "\x1b[2J\x1b[H\x1b[31mFAKE ERROR\x1b[0m";
        let clean = strip_ansi(evil);
        assert_eq!(clean, "FAKE ERROR", "ANSI cursor movement should be stripped, got: {:?}", clean);
    }

    #[test]
    fn sec_ansi_strips_from_plugin_name() {
        let evil_name = "\x1b[32m\x1b[1mmalicious\x1b[0m";
        let clean = strip_ansi(evil_name);
        assert_eq!(clean, "malicious", "ANSI in plugin names should be stripped, got: {:?}", clean);
    }

    #[test]
    fn preserves_normal_text() {
        assert_eq!(strip_ansi("hello world"), "hello world");
        assert_eq!(strip_ansi("file.ts:42"), "file.ts:42");
    }

    #[test]
    fn strips_control_chars() {
        let clean = strip_ansi("hello\x07world\x00test");
        assert_eq!(clean, "helloworldtest", "control chars should be stripped, got: {:?}", clean);
    }

    #[test]
    fn preserves_newlines() {
        assert_eq!(strip_ansi("hello\nworld"), "hello\nworld");
    }

    #[test]
    fn sec_ansi_handles_osc_sequences() {
        let evil = "\x1b]0;evil-title\x07visible";
        let clean = strip_ansi(evil);
        assert_eq!(clean, "visible", "OSC sequence should be fully stripped, got: {:?}", clean);
    }

    #[test]
    fn sec_ansi_handles_osc_st_terminator() {
        let evil = "\x1b]0;evil-title\x1b\\visible";
        let clean = strip_ansi(evil);
        assert_eq!(clean, "visible", "OSC sequence with ST terminator should be fully stripped, got: {:?}", clean);
    }

    #[test]
    fn sec_ansi_handles_two_char_esc() {
        let evil = "\x1bcremaining";
        let clean = strip_ansi(evil);
        assert_eq!(clean, "remaining", "two-char ESC sequence should be fully stripped, got: {:?}", clean);
    }
}
