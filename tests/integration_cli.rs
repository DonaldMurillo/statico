//! Integration tests: CLI surface — version/help/init/setup/doctor/quiet/update.

mod common;
use common::*;
use std::path::Path;
use std::process::Command;

#[test]
fn cli_version_flag_works() {
    let output = Command::new(statico_bin()).arg("--version").output().expect("failed to execute statico --version");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "--version should exit 0");
    // Don't pin a specific version — the release script bumps `Cargo.toml`
    // and runs the test suite before committing, so any literal version
    // here would break every cut. Just check the output looks like a
    // version line.
    let expected = env!("CARGO_PKG_VERSION");
    assert!(stdout.contains(expected), "version output should contain Cargo.toml version `{expected}`, got: {stdout}");
    assert!(stdout.starts_with("statico "), "version line should start with 'statico ', got: {stdout}");
}

#[test]
fn cli_help_lists_new_commands() {
    let output = Command::new(statico_bin()).arg("--help").output().expect("failed to execute statico --help");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success());
    assert!(stdout.contains("update"), "help should mention 'update' command");
    assert!(stdout.contains("init"), "help should mention 'init' command");
    assert!(stdout.contains("doctor"), "help should mention 'doctor' command");
    assert!(stdout.contains("setup"), "help should mention 'setup' command");
}

#[test]
fn cli_update_check_does_not_crash() {
    // Originally this test asserted on non-zero exit because no releases
    // existed yet. Now that v0.1.0+ is published, the real GitHub API
    // succeeds and `--check` exits 0. The relevant guarantee — and the
    // one we still want to enforce — is that the command never panics.
    // For a deterministic offline check use STATICO_UPDATE_API_URL in
    // a separate test that runs against a mock server; that already
    // exists as `cli_update_downloads_and_extracts_from_mock_server`.
    let output = Command::new(statico_bin())
        .args(["update", "--check"])
        .output()
        .expect("failed to execute statico update --check");

    let combined = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),);
    assert!(!combined.contains("panicked at"), "should not panic, got: {combined}");
    assert!(!combined.contains("RUST_BACKTRACE"), "should not panic, got: {combined}");
}

#[test]
fn cli_doctor_runs_without_crash() {
    let output = Command::new(statico_bin()).arg("doctor").output().expect("failed to execute statico doctor");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "doctor should exit 0, stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("Binary:"), "doctor should report binary location");
    assert!(stdout.contains("Version:"), "doctor should report version");
    assert!(stdout.contains("PATH:"), "doctor should check PATH");
    assert!(stdout.contains("Alias:"), "doctor should check alias");
    assert!(stdout.contains("Complete:"), "doctor should check completions");
    assert!(stdout.contains("Updates:"), "doctor should check update status");
}

#[test]
fn cli_init_writes_shell_rc_file() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let home = tmp.path().to_path_buf();

    // Create a fake .zshrc
    let zshrc = home.join(".zshrc");
    std::fs::write(&zshrc, "# existing content\n").expect("write .zshrc");

    let output = Command::new(statico_bin())
        .args(["init", "--shell", "zsh"])
        .env("HOME", &home)
        .env("SHELL", "/bin/zsh")
        .output()
        .expect("failed to execute statico init");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(output.status.success(), "init should exit 0, stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("Shell integration configured"), "expected success message, got: {stdout}");

    // Verify .zshrc was modified.
    let rc_content = std::fs::read_to_string(&zshrc).expect("read .zshrc");
    assert!(rc_content.contains("# statico"), "rc file should contain statico marker, got: {rc_content}");
    assert!(rc_content.contains("export PATH="), "rc file should contain PATH export, got: {rc_content}");
    assert!(rc_content.contains("alias st='statico'"), "rc file should contain alias, got: {rc_content}");
    assert!(rc_content.contains("source"), "rc file should source completions, got: {rc_content}");
    // Should preserve existing content.
    assert!(rc_content.contains("# existing content"), "rc file should preserve existing content, got: {rc_content}");
}

#[test]
fn cli_init_is_idempotent() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let home = tmp.path().to_path_buf();
    let zshrc = home.join(".zshrc");
    std::fs::write(&zshrc, "").expect("write .zshrc");

    // Run init twice.
    for _ in 0..2 {
        let output = Command::new(statico_bin())
            .args(["init", "--shell", "zsh"])
            .env("HOME", &home)
            .env("SHELL", "/bin/zsh")
            .output()
            .expect("failed to execute statico init");
        assert!(output.status.success());
    }

    let rc_content = std::fs::read_to_string(&zshrc).expect("read .zshrc");
    let count = rc_content.matches("# statico").count();
    assert_eq!(count, 1, "init should be idempotent, but # statico appeared {} times:\n{}", count, rc_content);
    let alias_count = rc_content.matches("alias st='statico'").count();
    assert_eq!(alias_count, 1, "alias should appear once, appeared {} times", alias_count);
}

#[test]
fn cli_quiet_suppresses_update_notification() {
    let output = Command::new(statico_bin())
        .args(["--quiet", "analyze", "."])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to execute");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(!stderr.contains("panic"), "should not panic with --quiet");
}

// ---------------------------------------------------------------------------
// Mock server test for self-update
// ---------------------------------------------------------------------------

/// Spin up a tiny HTTP server, serve a mock release + tarball, and verify
/// the full download-extract-swap flow.
#[test]
fn cli_update_downloads_and_extracts_from_mock_server() {
    use std::io::Write as IoWrite;

    let tmp = tempfile::tempdir().expect("temp dir");
    let tmp_path = tmp.path().to_path_buf();

    // 1. Build the mock tarball with a fake "statico" binary inside.
    let fake_binary_content = "#!/bin/sh\necho 'statico 99.0.0'\n";
    let tarball_path = tmp_path.join("archive.tar.gz");
    {
        let mut tar_builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_path("statico").expect("set path");
        header.set_size(fake_binary_content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar_builder.append(&header, fake_binary_content.as_bytes()).expect("append");
        let tar_data = tar_builder.into_inner().expect("tar data");
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar_data).expect("gzip");
        let gz_data = gz.finish().expect("finish gzip");
        std::fs::write(&tarball_path, &gz_data).expect("write tarball");
    }
    let tarball_bytes = std::fs::read(&tarball_path).expect("read tarball");

    // 2. Build the mock release JSON.
    let platform = if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "macos-aarch64"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        "macos-x86_64"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        "linux-aarch64"
    } else {
        "linux-x86_64"
    };
    let release_json = format!(
        r#"{{"tag_name":"v99.0.0","assets":[{{"name":"statico-{}.tar.gz","browser_download_url":"http://MOCK/releases/download/v99.0.0/statico-{}.tar.gz"}}]}}"#,
        platform, platform
    );

    // 3. Start a tiny HTTP server in a background thread.
    let server = tiny_http::ServerBuilder::new().with_random_port().build().expect("start mock server");
    let port = server.server_addr().port();
    let base_url = format!("http://127.0.0.1:{}", port);

    let tarball_bytes_clone = tarball_bytes.clone();
    let release_json_clone = release_json.clone();
    let server_handle = std::thread::spawn(move || {
        // Handle exactly 2 requests from the update command:
        // 1. version check: GET /releases/latest
        // 2. download:      GET /releases/download/v99.0.0/...
        //
        // NOTE: We pass --quiet to suppress the startup check_and_notify()
        //       which would otherwise make a 3rd request.
        for _ in 0..2 {
            if let Ok(req) = server.recv() {
                let path = req.url();
                if path.contains("/releases/latest") && !path.contains("download") {
                    let resp = tiny_http::Response::from_string(&release_json_clone).with_header(
                        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).expect("header"),
                    );
                    req.respond(resp);
                } else {
                    let resp = tiny_http::Response::from_data(tarball_bytes_clone.clone()).with_header(
                        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/gzip"[..]).expect("header"),
                    );
                    req.respond(resp);
                }
            }
        }
    });

    // 4. Create a fake current binary (copy the real one to temp location).
    let fake_exe_dir = tmp_path.join("bin");
    std::fs::create_dir_all(&fake_exe_dir).expect("create bin dir");
    let fake_exe = fake_exe_dir.join("statico");
    std::fs::copy(statico_bin(), &fake_exe).expect("copy real binary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_exe, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    // 5. Run the fake binary with `--quiet update`, pointing at our mock server.
    //    --quiet suppresses check_and_notify() which would otherwise hit the mock server.
    let output = std::process::Command::new(&fake_exe)
        .args(["--quiet", "update"])
        .env("STATICO_UPDATE_API_URL", &base_url)
        .env("STATICO_UPDATE_DL_URL", &base_url)
        .output()
        .expect("run fake binary");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Wait for server to finish.
    let _ = server_handle.join();

    // The update should succeed — binary swapped.
    if !output.status.success() {
        panic!("update command failed.\nstdout: {stdout}\nstderr: {stderr}");
    }

    assert!(stdout.contains("Updated statico"), "expected update success message, got: {stdout}");
    assert!(stdout.contains("99.0.0"), "expected new version in output, got: {stdout}");

    // 6. Verify the binary was replaced — it should now print our fake version.
    let verify = std::process::Command::new(&fake_exe).arg("--version").output().expect("verify replaced binary");
    let verify_stdout = String::from_utf8_lossy(&verify.stdout).to_string();
    assert!(verify_stdout.contains("99.0.0"), "binary should have been replaced with mock, got: {verify_stdout}");
}

// ---------------------------------------------------------------------------
// setup command tests
// ---------------------------------------------------------------------------

#[test]
fn cli_setup_generates_claude_files() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let tmp_path = tmp.path();

    // Init a git repo so .gitignore works.
    std::process::Command::new("git").arg("init").current_dir(tmp_path).output().expect("git init");
    std::fs::write(tmp_path.join(".gitignore"), "node_modules/\n").expect("gitignore");

    let output = std::process::Command::new(statico_bin())
        .args(["setup", "--target", "claude"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("run setup");

    assert!(output.status.success(), "setup should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("wrote"), "should report files written: {stdout}");

    assert!(tmp_path.join(".claude/CLAUDE.md").exists(), "CLAUDE.md should exist");
    assert!(tmp_path.join(".claude/skills/statico-analyze/SKILL.md").exists(), "analyze skill should exist");
    assert!(tmp_path.join(".claude/skills/statico-fix/SKILL.md").exists(), "fix skill should exist");
    assert!(tmp_path.join(".claude/skills/statico-plugin/SKILL.md").exists(), "plugin skill should exist");

    // Verify .gitignore updated.
    let gitignore = std::fs::read_to_string(tmp_path.join(".gitignore")).expect("read gitignore");
    assert!(gitignore.contains(".claude/"), ".gitignore should contain .claude/: {gitignore}");
}

/// Snapshot test: every shipped skill / config file in the user repo must be
/// byte-identical to the source-of-truth under `templates/`. Catches drift
/// where someone edits the inline `include_str!` reference without updating
/// the template, or vice-versa.
#[test]
fn cli_setup_output_matches_templates_byte_for_byte() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let tmp_path = tmp.path();
    std::process::Command::new("git").arg("init").current_dir(tmp_path).output().expect("git init");

    let output = std::process::Command::new(statico_bin())
        .args(["setup", "--target", "all"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("run setup");
    assert!(output.status.success(), "setup --target all should succeed");

    let templates_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");

    let pairs: &[(&str, &str)] = &[
        (".claude/CLAUDE.md", "CLAUDE.md"),
        (".claude/skills/statico-analyze/SKILL.md", "skills/statico-analyze/SKILL.md"),
        (".claude/skills/statico-fix/SKILL.md", "skills/statico-fix/SKILL.md"),
        (".claude/skills/statico-plugin/SKILL.md", "skills/statico-plugin/SKILL.md"),
        (".cursor/rules/statico.mdc", "cursor/statico.mdc"),
    ];

    for (written, source) in pairs {
        let written_path = tmp_path.join(written);
        let source_path = templates_root.join(source);
        let written_bytes = std::fs::read(&written_path).unwrap_or_else(|e| panic!("read {written}: {e}"));
        let source_bytes = std::fs::read(&source_path).unwrap_or_else(|e| panic!("read {source}: {e}"));
        assert_eq!(
            written_bytes, source_bytes,
            "{written} drifted from templates/{source} — `statico setup` writes content that no longer matches the source-of-truth",
        );
    }

    // Every shipped SKILL.md must carry frontmatter so Claude Code / pi
    // auto-discovery picks it up. Specifically check the `description:`
    // line — without it the skill is invisible.
    for skill_rel in [
        ".claude/skills/statico-analyze/SKILL.md",
        ".claude/skills/statico-fix/SKILL.md",
        ".claude/skills/statico-plugin/SKILL.md",
    ] {
        let body = std::fs::read_to_string(tmp_path.join(skill_rel)).expect("read skill");
        assert!(body.starts_with("---\n"), "{skill_rel} missing YAML frontmatter");
        assert!(body.contains("\nname: statico-"), "{skill_rel} missing name field");
        assert!(body.contains("\ndescription: "), "{skill_rel} missing description field");
    }
}

#[test]
fn cli_setup_generates_cursor_rules() {
    let tmp = tempfile::tempdir().expect("temp dir");

    let output = std::process::Command::new(statico_bin())
        .args(["setup", "--target", "cursor"])
        .arg("--path")
        .arg(tmp.path())
        .output()
        .expect("run setup");

    assert!(output.status.success(), "setup should succeed");
    assert!(tmp.path().join(".cursor/rules/statico.mdc").exists(), "cursor rules should exist");
}

#[test]
fn cli_setup_is_idempotent() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let tmp_path = tmp.path();
    std::process::Command::new("git").arg("init").current_dir(tmp_path).output().ok();

    // Run twice.
    let first = std::process::Command::new(statico_bin())
        .args(["setup"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("first setup");
    assert!(first.status.success());

    let second = std::process::Command::new(statico_bin())
        .args(["setup"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("second setup");
    assert!(second.status.success());
    let stdout = String::from_utf8_lossy(&second.stdout);
    assert!(stdout.contains("already exist"), "second run should say already exist: {stdout}");
}

#[test]
fn cli_setup_force_overwrites() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let tmp_path = tmp.path();
    std::process::Command::new("git").arg("init").current_dir(tmp_path).output().ok();

    // Run once, then modify a file, then --force.
    let first = std::process::Command::new(statico_bin())
        .args(["setup"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("first setup");
    assert!(first.status.success());

    // Overwrite CLAUDE.md with custom content.
    std::fs::write(tmp_path.join(".claude/CLAUDE.md"), "CUSTOM CONTENT").expect("overwrite");

    let force = std::process::Command::new(statico_bin())
        .args(["setup", "--force"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("force setup");
    assert!(force.status.success());

    // File should be overwritten.
    let content = std::fs::read_to_string(tmp_path.join(".claude/CLAUDE.md")).expect("read");
    assert!(content.contains("statico"), "should be regenerated: {content}");
    assert!(!content.contains("CUSTOM CONTENT"), "custom content should be gone");
}

#[test]
fn cli_setup_generates_pi_skills() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let tmp_path = tmp.path();
    std::process::Command::new("git").arg("init").current_dir(tmp_path).output().ok();
    std::fs::write(tmp_path.join(".gitignore"), "node_modules/\n").expect("gitignore");

    let output = std::process::Command::new(statico_bin())
        .args(["setup", "--target", "pi"])
        .arg("--path")
        .arg(tmp_path)
        .output()
        .expect("run setup");

    assert!(output.status.success(), "setup should succeed");
    assert!(tmp_path.join(".pi/skills/statico-analyze/SKILL.md").exists(), "pi analyze skill should exist");
    assert!(tmp_path.join(".pi/skills/statico-fix/SKILL.md").exists(), "pi fix skill should exist");
    assert!(tmp_path.join(".pi/skills/statico-plugin/SKILL.md").exists(), "pi plugin skill should exist");

    // Verify frontmatter has correct name field.
    let content = std::fs::read_to_string(tmp_path.join(".pi/skills/statico-analyze/SKILL.md")).expect("read");
    assert!(content.contains("name: statico-analyze"), "SKILL.md should have correct name frontmatter: {content}");

    // Verify .gitignore updated.
    let gitignore = std::fs::read_to_string(tmp_path.join(".gitignore")).expect("read gitignore");
    assert!(gitignore.contains(".pi/"), ".gitignore should contain .pi/: {gitignore}");
}

// ─── Plugin integration tests ────────────────────────────────────
