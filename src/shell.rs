//! Shell escaping utility — safe embedding of strings in bash/zsh/fish.

/// Shell-escape a string for safe embedding in bash/zsh/fish PATH assignment.
/// Escapes characters that would be interpreted by the shell: `$`, `` ` ``, `"`, `\`.
pub fn shell_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('"', "\\\"")
     .replace('$', "\\$")
     .replace('`', "\\`")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sec_shell_escape_dollar() {
        let result = shell_escape("/path/$HOME/bin");
        assert_eq!(result, "/path/\\$HOME/bin",
            "dollar sign should be escaped: got {}", result);
    }

    #[test]
    fn sec_shell_escape_backtick() {
        let result = shell_escape("/path/`whoami`/bin");
        assert_eq!(result, "/path/\\`whoami\\`/bin",
            "backtick should be escaped: got {}", result);
    }

    #[test]
    fn sec_shell_escape_double_quote() {
        let result = shell_escape("/path/\"evil\"/bin");
        assert_eq!(result, "/path/\\\"evil\\\"/bin",
            "double quote should be escaped: got {}", result);
    }

    #[test]
    fn sec_shell_escape_backslash() {
        let result = shell_escape("/path/\\evil/bin");
        assert_eq!(result, "/path/\\\\evil/bin",
            "backslash should be escaped: got {}", result);
    }

    #[test]
    fn sec_shell_escape_normal_path() {
        let result = shell_escape("/usr/local/bin");
        assert_eq!(result, "/usr/local/bin",
            "normal path should be unchanged: got {}", result);
    }

    #[test]
    fn sec_shell_source_path_escaped() {
        let path_with_dollar = "/home/$USER/.statico/completions/statico.bash";
        let escaped = shell_escape(path_with_dollar);
        assert!(escaped.contains("\\$"),
            "dollar in path should be escaped, got: {}", escaped);
        let path_with_backtick = "/home/`whoami`/.statico/completions/statico.bash";
        let escaped2 = shell_escape(path_with_backtick);
        assert!(escaped2.contains("\\`"),
            "backtick in path should be escaped, got: {}", escaped2);
    }
}
