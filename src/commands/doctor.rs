//! `statico doctor` command.

pub fn run_doctor() {
    let mut ok = true;

    println!("statico doctor — checking installation...\n");

    // 1. Binary location.
    let exe = std::env::current_exe().unwrap_or_default();
    println!("  Binary:   {}", exe.display());

    // 2. Version.
    let version = env!("CARGO_PKG_VERSION");
    let git = git_version::git_version!(fallback = "unknown");
    println!("  Version:  v{} ({})", version, git);

    // 3. PATH check.
    let in_path = which_statico();
    match in_path {
        Some(p) => println!("  PATH:     {} \x1b[32m✓\x1b[0m", p.display()),
        None => {
            println!("  PATH:     \x1b[31m✗ statico not found on PATH\x1b[0m");
            ok = false;
        }
    }

    // 4. Shell alias.
    let alias_check = std::process::Command::new("alias").arg("st").output();
    let alias_ok = alias_check
        .map(|o| {
            String::from_utf8_lossy(&o.stdout).contains("statico") || String::from_utf8_lossy(&o.stdout).contains("st")
        })
        .unwrap_or(false);
    if alias_ok {
        println!("  Alias:    st='statico' \x1b[32m✓\x1b[0m");
    } else {
        println!("  Alias:    \x1b[33mst alias not set (run `statico init`)\x1b[0m");
    }

    // 5. Completions.
    let data_dir = statico::update::data_dir();
    let completions = data_dir.join("completions");
    if completions.exists() {
        println!("  Complete: {} \x1b[32m✓\x1b[0m", completions.display());
    } else {
        println!("  Complete: \x1b[33mnot installed (run `statico init`)\x1b[0m");
    }

    // 6. Update check.
    let last_version = data_dir.join("last-version");
    if last_version.exists() {
        let cached = std::fs::read_to_string(&last_version).unwrap_or_default();
        let status = if statico::update::is_newer(version, &cached) {
            format!("\x1b[33mv{} available\x1b[0m", cached.trim())
        } else {
            "\x1b[32mup to date\x1b[0m".to_string()
        };
        println!("  Updates:  {}", status);
    } else {
        println!("  Updates:  \x1b[33mnever checked (run `statico update --check`)\x1b[0m");
    }

    println!();
    if ok {
        println!("\x1b[32mAll checks passed.\x1b[0m");
    } else {
        println!("\x1b[33mSome issues found. Run `statico init` to set up shell integration.\x1b[0m");
    }
}

/// Find statico on PATH.
fn which_statico() -> Option<std::path::PathBuf> {
    let path_env = std::env::var("PATH").unwrap_or_default();
    for dir in path_env.split(':') {
        let candidate = std::path::Path::new(dir).join("statico");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

pub fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn print_status(label: &str, ok: bool) {
    let mark = if ok { "\u{2713}" } else { "\u{2717}" };
    println!("  {} {}", mark, label);
}
