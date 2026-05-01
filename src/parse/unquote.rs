//! String unquoting utility — strips matching quote delimiters.

/// Remove matching surrounding quotes from a string.
/// Handles `"..."`, `'...'`, and `` `...` ``.
/// Returns the string as-is if not properly quoted.
pub fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\''))
            || (s.starts_with('`') && s.ends_with('`')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unquote() {
        assert_eq!(unquote("'./foo'"), "./foo");
        assert_eq!(unquote("\"bar\""), "bar");
        assert_eq!(unquote("`baz`"), "baz");
        assert_eq!(unquote("naked"), "naked");
    }

    #[test]
    fn sec_parse_unquote_no_panic_short_string() {
        assert_eq!(unquote("\""), "\"");
        assert_eq!(unquote("'"), "'");
        assert_eq!(unquote("`"), "`");
        assert_eq!(unquote(""), "");
    }
}
