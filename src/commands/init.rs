//! `statico init` and `statico setup` commands.

use std::io::Write;
use std::process;

use clap_complete::{Shell, generate};

pub fn run_init(shell: Option<&str>, cli_command: &mut clap::Command) {
    let shell =
        shell.map(|s| s.to_string()).or_else(|| std::env::var("SHELL").ok()).unwrap_or_else(|| "bash".to_string());

    let is_zsh = shell.contains("zsh");
    let _is_bash = shell.contains("bash");
    let is_fish = shell.contains("fish");

    let rc_file = if is_zsh {
        dirs::home_dir().map(|h| h.join(".zshrc"))
    } else if is_fish {
        dirs::home_dir().map(|h| h.join(".config/fish/config.fish"))
    } else {
        dirs::home_dir().map(|h| h.join(".bashrc"))
    };

    let Some(rc_file) = rc_file else {
        eprintln!("error: cannot determine home directory");
        process::exit(1);
    };

    // Ensure data dir exists.
    let data_dir = statico::update::data_dir();
    let completions_dir = data_dir.join("completions");
    std::fs::create_dir_all(&completions_dir).ok();

    // Generate completion file.
    let completion_file =
        if is_fish { completions_dir.join("statico.fish") } else { completions_dir.join("statico.bash") };

    let shell_type = if is_fish {
        Shell::Fish
    } else if is_zsh {
        Shell::Zsh
    } else {
        Shell::Bash
    };

    {
        let mut file = std::fs::File::create(&completion_file).expect("failed to create completion file");
        generate(shell_type, cli_command, "statico", &mut file);
    }

    // Build the rc snippet — PATH + alias + completions.
    let exe = std::env::current_exe().expect("cannot determine current executable");
    let bin_dir = exe.parent().unwrap_or_else(|| std::path::Path::new("/usr/local/bin"));
    let bin_dir_escaped = statico::shell::shell_escape(&bin_dir.display().to_string());
    let completion_escaped = statico::shell::shell_escape(&completion_file.display().to_string());

    let snippet = if is_fish {
        // Fish does not interpret `\$` / `\\`` inside double quotes the way bash does;
        // single-quote the escaped value so paths containing spaces or shell metachars
        // survive intact (audit S4.4).
        let bin_dir_fish = fish_single_quote(&bin_dir.display().to_string());
        let completion_fish = fish_single_quote(&completion_file.display().to_string());
        format!("\n# statico\nset -gx PATH {bin_dir_fish} $PATH\nalias st statico\nsource {completion_fish}\n")
    } else {
        format!(
            "\n# statico\nexport PATH=\"{bin_dir_escaped}:$PATH\"\nalias st='statico'\nsource \"{completion_escaped}\"\n"
        )
    };

    // Check if already configured.
    let rc_content = std::fs::read_to_string(&rc_file).unwrap_or_default();
    if rc_content.contains("# statico") {
        println!("Shell integration already configured in {}", rc_file.display());
        println!("  alias: st='statico'");
        println!("  completions: {}", completion_file.display());
        return;
    }

    // Append to rc file.
    let mut file = std::fs::OpenOptions::new().append(true).open(&rc_file).expect("failed to open rc file");
    file.write_all(snippet.as_bytes()).expect("failed to write rc file");

    println!("\x1b[32m✓ Shell integration configured!\x1b[0m");
    println!(
        "  Shell:    {}",
        if is_zsh {
            "zsh"
        } else if is_fish {
            "fish"
        } else {
            "bash"
        }
    );
    println!("  Config:   {}", rc_file.display());
    println!("  Alias:    st → statico");
    println!("  Complete: {}", completion_file.display());
    println!();
    println!("Restart your shell or run:");
    println!("  source {}", rc_file.display());
}

/// Wrap a string in fish single-quotes, escaping `\` and `'`.
///
/// Inside fish single-quotes only `\\` and `\'` are special — every other
/// character is literal, which makes single-quote wrapping the safest way to
/// embed user-supplied paths in a fish script.
fn fish_single_quote(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{}'", escaped)
}

pub fn run_setup(target: &str, path: &str, force: bool) {
    let root = std::path::Path::new(path);
    if !root.exists() {
        eprintln!("error: path {} does not exist", path);
        process::exit(1);
    }

    let generate_all = target == "all";
    let generate_claude = generate_all || target == "claude";
    let generate_cursor = generate_all || target == "cursor";
    let generate_pi = generate_all || target == "pi";

    if !generate_claude && !generate_cursor && !generate_pi {
        eprintln!("error: unknown target '{}'. Use: claude, cursor, pi, or all", target);
        process::exit(1);
    }

    let mut files_written = 0usize;

    // Update .gitignore for user-specific AI config dirs.
    let gitignore = root.join(".gitignore");
    if gitignore.exists() && !gitignore.is_symlink() {
        let content = std::fs::read_to_string(&gitignore).unwrap_or_default();
        let mut additions = Vec::new();
        if generate_claude && !content.lines().any(|l| l == ".claude/") {
            additions.push(".claude/");
        }
        if generate_pi && !content.lines().any(|l| l == ".pi/") {
            additions.push(".pi/");
        }
        if !additions.is_empty() {
            let mut f = std::fs::OpenOptions::new().append(true).open(&gitignore).expect("open .gitignore");
            for a in &additions {
                let _ = f.write_all(format!("\n{}\n", a).as_bytes());
            }
        }
    }

    // --- Claude Code setup ---
    if generate_claude {
        let claude_dir = root.join(".claude");

        let claude_md = claude_dir.join("CLAUDE.md");
        if claude_md.exists() && !force {
            println!("  skipping {} (already exists, use --force to overwrite)", claude_md.display());
        } else {
            std::fs::create_dir_all(&claude_dir).expect("create .claude dir");
            std::fs::write(&claude_md, CLAUDE_MD).expect("write CLAUDE.md");
            println!("  wrote {}", claude_md.display());
            files_written += 1;
        }

        files_written +=
            write_skill(&claude_dir.join("skills").join("statico-analyze"), "statico-analyze", SKILL_ANALYZE, force);
        files_written += write_skill(&claude_dir.join("skills").join("statico-fix"), "statico-fix", SKILL_FIX, force);
        files_written +=
            write_skill(&claude_dir.join("skills").join("statico-plugin"), "statico-plugin", SKILL_PLUGIN, force);
    }

    // --- Pi setup ---
    if generate_pi {
        let pi_dir = root.join(".pi");
        files_written +=
            write_skill(&pi_dir.join("skills").join("statico-analyze"), "statico-analyze", SKILL_ANALYZE, force);
        files_written += write_skill(&pi_dir.join("skills").join("statico-fix"), "statico-fix", SKILL_FIX, force);
        files_written +=
            write_skill(&pi_dir.join("skills").join("statico-plugin"), "statico-plugin", SKILL_PLUGIN, force);
    }

    // --- Cursor setup ---
    if generate_cursor {
        let rules_file = root.join(".cursor").join("rules").join("statico.mdc");
        if rules_file.exists() && !force {
            println!("  skipping {} (already exists)", rules_file.display());
        } else {
            std::fs::create_dir_all(rules_file.parent().unwrap()).expect("create cursor rules dir");
            std::fs::write(&rules_file, CURSOR_RULES).expect("write cursor rules");
            println!("  wrote {}", rules_file.display());
            files_written += 1;
        }
    }

    if files_written > 0 {
        println!("\n\x1b[32m✓ AI integration set up!\x1b[0m {} file(s) generated.", files_written);
    } else {
        println!("All files already exist. Use --force to overwrite.");
    }
}

/// Write a skill directory with SKILL.md. Returns 1 if written, 0 if skipped.
fn write_skill(dir: &std::path::Path, _name: &str, content: &str, force: bool) -> usize {
    let skill_file = dir.join("SKILL.md");
    if skill_file.exists() && !force {
        println!("  skipping {} (already exists)", skill_file.display());
        return 0;
    }
    std::fs::create_dir_all(dir).unwrap_or_else(|_| panic!("create {} dir", dir.display()));
    std::fs::write(&skill_file, content).unwrap_or_else(|_| panic!("write {}", skill_file.display()));
    println!("  wrote {}", skill_file.display());
    1
}

// Source-of-truth markdown templates live under `templates/` and are embedded
// in the binary at build time via `include_str!`. To edit what `statico setup`
// writes into a user's project, edit the file under `templates/` and rebuild.
const CLAUDE_MD: &str = include_str!("../../templates/CLAUDE.md");
const SKILL_ANALYZE: &str = include_str!("../../templates/skills/statico-analyze/SKILL.md");
const SKILL_FIX: &str = include_str!("../../templates/skills/statico-fix/SKILL.md");
const SKILL_PLUGIN: &str = include_str!("../../templates/skills/statico-plugin/SKILL.md");
const CURSOR_RULES: &str = include_str!("../../templates/cursor/statico.mdc");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sec_fish_single_quote_wraps_normal_path() {
        assert_eq!(fish_single_quote("/usr/local/bin"), "'/usr/local/bin'");
    }

    #[test]
    fn sec_fish_single_quote_handles_path_with_space() {
        assert_eq!(fish_single_quote("/Users/Alice Smith/.statico/bin"), "'/Users/Alice Smith/.statico/bin'");
    }

    #[test]
    fn sec_fish_single_quote_escapes_single_quote() {
        // path contains a literal single quote (yes, this is legal on POSIX)
        assert_eq!(fish_single_quote("/tmp/o'reilly/bin"), "'/tmp/o\\'reilly/bin'");
    }

    #[test]
    fn sec_fish_single_quote_escapes_backslash() {
        assert_eq!(fish_single_quote(r"C:\stat\bin"), r"'C:\\stat\\bin'");
    }
}
