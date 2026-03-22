use assert_cmd::Command;
use axum::{routing::get, routing::post, Json, Router};
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::mpsc;
use std::time::Duration;

fn sxmc() -> Command {
    Command::cargo_bin("sxmc").unwrap()
}

fn sxmc_with_config_home(home: &Path) -> Command {
    let mut cmd = sxmc();
    cmd.env("HOME", home);
    cmd.env("USERPROFILE", home);
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd.env("APPDATA", home.join("AppData").join("Roaming"));
    cmd.env("LOCALAPPDATA", home.join("AppData").join("Local"));
    cmd
}

fn sxmc_bin_string() -> String {
    assert_cmd::cargo::cargo_bin("sxmc")
        .to_string_lossy()
        .into_owned()
}

fn stateful_mcp_command_spec() -> String {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("stateful_mcp_server.py");

    #[cfg(windows)]
    let parts = vec![
        "py".to_string(),
        "-3".to_string(),
        script.to_string_lossy().into_owned(),
    ];

    #[cfg(not(windows))]
    let parts = vec!["python3".to_string(), script.to_string_lossy().into_owned()];

    serde_json::to_string(&parts).unwrap()
}

fn wait_for_http_server(port: u16) {
    let addr = format!("127.0.0.1:{port}")
        .parse()
        .expect("valid socket address");
    for _ in 0..40 {
        if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok() {
            std::thread::sleep(Duration::from_millis(100));
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out waiting for HTTP server on port {}", port);
}

fn spawn_http_server(extra_args: &[&str]) -> (Child, u16) {
    let mut child = ProcessCommand::new(sxmc_bin_string())
        .args([
            "serve",
            "--transport",
            "http",
            "--host",
            "127.0.0.1",
            "--port",
            "0",
        ])
        .args(extra_args)
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stderr = child.stderr.take().expect("child stderr should be piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut sent = false;
        for line in reader.lines().map_while(Result::ok) {
            if !sent {
                if let Some(port) = line
                    .split("http://127.0.0.1:")
                    .nth(1)
                    .and_then(|tail| tail.split("/mcp").next())
                    .and_then(|port| port.parse::<u16>().ok())
                {
                    let _ = sender.send(port);
                    sent = true;
                }
            }
        }
    });

    let port = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("timed out waiting for HTTP server port");
    wait_for_http_server(port);
    (child, port)
}

fn command_stdout(args: &[&str]) -> String {
    let output = ProcessCommand::new(sxmc_bin_string())
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "command failed: {}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn command_json(args: &[&str]) -> Value {
    serde_json::from_str(&command_stdout(args)).unwrap()
}

#[cfg(not(windows))]
fn write_fake_cli(dir: &Path, help_text: &str) -> std::path::PathBuf {
    let script = dir.join("fake-cli");
    let body = format!("#!/bin/sh\ncat <<'EOF'\n{help_text}\nEOF\n");
    fs::write(&script, body).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();
    script
}

#[cfg(not(windows))]
fn write_fake_nested_cli(dir: &Path) -> std::path::PathBuf {
    let script = dir.join("fake-nested-cli");
    let body = r#"#!/bin/sh
if [ "$1" = "alpha" ] && [ "$2" = "beta" ]; then
    cat <<'EOF'
fake-nested-cli alpha beta

Deep beta command.

Usage:
  fake-nested-cli alpha beta [OPTIONS]

Options:
  --deep  Use the deepest path.
EOF
elif [ "$1" = "alpha" ]; then
    cat <<'EOF'
fake-nested-cli alpha

Alpha command group.

Commands:
  beta  Run the beta workflow
EOF
else
    cat <<'EOF'
fake-nested-cli

Nested demo CLI.

Commands:
  alpha  Run the alpha workflow
EOF
fi
"#;
    fs::write(&script, body).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();
    script
}

#[test]
fn test_version() {
    sxmc()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("sxmc"));
}

#[test]
fn test_help() {
    sxmc()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Skills × MCP × CLI"))
        .stdout(predicate::str::contains("serve"))
        .stdout(predicate::str::contains("skills"))
        .stdout(predicate::str::contains("stdio"))
        .stdout(predicate::str::contains("http"))
        .stdout(predicate::str::contains("mcp"))
        .stdout(predicate::str::contains("api"))
        .stdout(predicate::str::contains("inspect"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("scaffold"))
        .stdout(predicate::str::contains("scan"))
        .stdout(predicate::str::contains("bake"))
        .stdout(predicate::str::contains("completions"))
        .stdout(predicate::str::contains("doctor"));
}

#[test]
fn test_completions_bash() {
    sxmc()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_sxmc"));
}

#[test]
fn test_doctor_reports_recommended_first_moves_as_json_off_tty() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("AGENTS.md"), "# Existing\n").unwrap();

    let value = command_json(&["doctor", "--root", temp.path().to_str().unwrap()]);
    assert_eq!(value["root"], temp.path().to_string_lossy().as_ref());
    assert_eq!(
        value["startup_files"]["portable_agent_doc"]["present"],
        Value::Bool(true)
    );
    let moves = value["recommended_first_moves"].as_array().unwrap();
    assert!(moves.iter().any(|entry| entry["surface"] == "unknown_cli"
        && entry["command"]
            .as_str()
            .unwrap_or_default()
            .contains("sxmc inspect cli")));
    assert!(moves.iter().any(|entry| entry["surface"] == "unknown_api"
        && entry["command"]
            .as_str()
            .unwrap_or_default()
            .contains("sxmc api <url-or-spec> --list")));
    assert!(moves
        .iter()
        .any(|entry| entry["surface"] == "local_skills_or_prompts"
            && entry["command"]
                .as_str()
                .unwrap_or_default()
                .contains("sxmc serve --paths <dir>")));
}

#[test]
fn test_inspect_cli_compact_output_reduces_profile_shape() {
    let value = command_json(&["inspect", "cli", "cargo", "--compact"]);
    assert_eq!(value["command"], "cargo");
    assert!(value["subcommand_count"].as_u64().unwrap_or(0) >= 10);
    assert!(value["option_count"].as_u64().unwrap_or(0) >= 5);
    assert!(value["subcommands"].as_array().unwrap().len() <= 12);
    assert!(value["options"].as_array().unwrap().len() <= 15);
    assert!(value.get("provenance").is_none());
}

#[cfg(not(windows))]
#[test]
fn test_inspect_cli_cache_invalidates_when_binary_changes() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_cli(
        temp.path(),
        "fake-cli\n\nA first summary.\n\nUsage:\n  fake-cli [OPTIONS]\n",
    );

    let first = command_json(&["inspect", "cli", fake.to_str().unwrap(), "--pretty"]);
    assert_eq!(first["summary"], "A first summary.");

    std::thread::sleep(Duration::from_millis(1100));
    fs::write(
        &fake,
        "#!/bin/sh\ncat <<'EOF'\nfake-cli\n\nA second summary after change.\n\nUsage:\n  fake-cli [OPTIONS]\nEOF\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&fake).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake, perms).unwrap();

    let second = command_json(&["inspect", "cli", fake.to_str().unwrap(), "--pretty"]);
    assert_eq!(second["summary"], "A second summary after change.");
}

#[test]
fn test_inspect_cli_depth_one_collects_nested_profiles() {
    let output = sxmc()
        .args(["inspect", "cli", "cargo", "--depth", "1"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let nested = value["subcommand_profiles"].as_array().unwrap();
    assert!(!nested.is_empty());
    assert!(nested.iter().any(|profile| profile["command"] == "build"));
}

#[cfg(not(windows))]
#[test]
fn test_inspect_cli_depth_two_collects_grandchild_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_nested_cli(temp.path());
    let value = command_json(&["inspect", "cli", fake.to_str().unwrap(), "--depth", "2"]);

    let nested = value["subcommand_profiles"].as_array().unwrap();
    let alpha = nested
        .iter()
        .find(|profile| profile["command"] == "alpha")
        .expect("alpha nested profile");
    let grandchild = alpha["subcommand_profiles"].as_array().unwrap();
    assert!(grandchild
        .iter()
        .any(|profile| profile["command"] == "beta"));
}

#[cfg(not(windows))]
#[test]
fn test_inspect_cli_uses_man_page_fallback_for_bsd_tools() {
    let output = sxmc().args(["inspect", "cli", "ls"]).output().unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_ne!(value["summary"], "ls command-line interface");
    assert!(
        value["options"]
            .as_array()
            .map(|options| !options.is_empty())
            .unwrap_or(false),
        "expected man-page fallback to recover options for ls"
    );
}

#[cfg(not(windows))]
#[test]
fn test_init_ai_blocks_low_confidence_profiles_without_override() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_cli(temp.path(), "usage: fake-cli [options]");

    sxmc()
        .args([
            "init",
            "ai",
            "--from-cli",
            fake.to_str().unwrap(),
            "--client",
            "claude-code",
            "--mode",
            "preview",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("low-confidence CLI profile"));

    sxmc()
        .args([
            "init",
            "ai",
            "--from-cli",
            fake.to_str().unwrap(),
            "--client",
            "claude-code",
            "--mode",
            "preview",
            "--allow-low-confidence",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("sxmc CLI Surface"));
}

#[test]
fn test_http_help_mentions_timeout() {
    sxmc()
        .args(["http", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--timeout-seconds"));
}

#[test]
fn test_api_help_mentions_timeout() {
    sxmc()
        .args(["api", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--timeout-seconds"));
}

#[test]
fn test_serve_help_mentions_http_limits() {
    sxmc()
        .args(["serve", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--max-concurrency"))
        .stdout(predicate::str::contains("--max-request-bytes"));
}

#[test]
fn test_bake_timeout_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    sxmc_with_config_home(temp.path())
        .args([
            "bake",
            "create",
            "demo-http",
            "--type",
            "http",
            "--source",
            "http://127.0.0.1:8000/mcp",
            "--timeout-seconds",
            "9",
            "--skip-validate",
        ])
        .assert()
        .success();

    sxmc_with_config_home(temp.path())
        .args(["bake", "show", "demo-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Timeout: 9s"));
}

#[test]
fn test_bake_create_validates_stdio_source_by_default() {
    let temp = tempfile::tempdir().unwrap();
    sxmc_with_config_home(temp.path())
        .args([
            "bake",
            "create",
            "broken",
            "--type",
            "stdio",
            "--source",
            r#"["definitely-not-a-real-command-for-sxmc-tests"]"#,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "could not connect during validation",
        ))
        .stderr(predicate::str::contains(
            "Run the stdio command directly once",
        ))
        .stderr(predicate::str::contains("--skip-validate"));
}

#[test]
fn test_bake_create_http_validation_includes_guided_hints() {
    let temp = tempfile::tempdir().unwrap();
    sxmc_with_config_home(temp.path())
        .args([
            "bake",
            "create",
            "offline-http",
            "--type",
            "http",
            "--source",
            "http://127.0.0.1:9/mcp",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "points at its streamable MCP endpoint",
        ))
        .stderr(predicate::str::contains("--skip-validate"));
}

#[test]
fn test_bake_create_can_skip_validation() {
    let temp = tempfile::tempdir().unwrap();
    sxmc_with_config_home(temp.path())
        .args([
            "bake",
            "create",
            "broken",
            "--type",
            "stdio",
            "--source",
            r#"["definitely-not-a-real-command-for-sxmc-tests"]"#,
            "--skip-validate",
        ])
        .assert()
        .success();

    sxmc_with_config_home(temp.path())
        .args(["bake", "show", "broken"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "definitely-not-a-real-command-for-sxmc-tests",
        ));
}

#[test]
fn test_bake_stdio_base_dir_round_trip_and_relative_source() {
    let temp = tempfile::tempdir().unwrap();
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(skills_dir.join("mini")).unwrap();
    fs::write(
        skills_dir.join("mini").join("SKILL.md"),
        r#"---
name: mini
description: "Mini skill"
---

Hello
"#,
    )
    .unwrap();

    sxmc_with_config_home(temp.path())
        .args([
            "bake",
            "create",
            "relative-stdio",
            "--type",
            "stdio",
            "--source",
            r#"["sxmc","serve","--paths","."]"#,
            "--base-dir",
            skills_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    sxmc_with_config_home(temp.path())
        .args(["bake", "show", "relative-stdio"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Base dir:"))
        .stdout(predicate::str::contains(
            skills_dir.to_string_lossy().as_ref(),
        ));

    sxmc_with_config_home(temp.path())
        .args(["mcp", "prompts", "relative-stdio", "--limit", "5"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mini"));
}

#[test]
fn test_skills_list() {
    sxmc()
        .args(["skills", "list", "--paths", "tests/fixtures"])
        .assert()
        .success()
        .stdout(predicate::str::contains("simple-skill"))
        .stdout(predicate::str::contains("skill-with-scripts"))
        .stdout(predicate::str::contains("skill-with-references"));
}

#[test]
fn test_skills_list_json() {
    sxmc()
        .args(["skills", "list", "--paths", "tests/fixtures", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\""))
        .stdout(predicate::str::contains("simple-skill"));
}

#[test]
fn test_skills_info() {
    sxmc()
        .args([
            "skills",
            "info",
            "simple-skill",
            "--paths",
            "tests/fixtures",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Name: simple-skill"))
        .stdout(predicate::str::contains("Description:"));
}

#[test]
fn test_skills_info_not_found() {
    sxmc()
        .args([
            "skills",
            "info",
            "nonexistent-skill",
            "--paths",
            "tests/fixtures",
        ])
        .assert()
        .failure();
}

#[test]
fn test_skills_run() {
    sxmc()
        .args(["skills", "run", "simple-skill", "--paths", "tests/fixtures"])
        .assert()
        .success();
}

#[test]
fn test_inspect_profile_toon() {
    sxmc()
        .args([
            "inspect",
            "profile",
            "examples/profiles/from_cli.json",
            "--format",
            "toon",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("profile_schema:"))
        .stdout(predicate::str::contains("command:"))
        .stdout(predicate::str::contains("subcommands["));
}

#[test]
fn test_inspect_profile_json_pretty() {
    sxmc()
        .args([
            "inspect",
            "profile",
            "examples/profiles/from_generated_cli.json",
            "--pretty",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"profile_schema\":"))
        .stdout(predicate::str::contains("\"generation_depth\": 1"));
}

#[test]
fn test_inspect_cli_requires_allow_self_for_sxmc() {
    sxmc()
        .args(["inspect", "cli", &sxmc_bin_string()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Refusing to inspect sxmc itself"));
}

#[test]
fn test_inspect_cli_self_with_allow_self() {
    sxmc()
        .args([
            "inspect",
            "cli",
            &sxmc_bin_string(),
            "--allow-self",
            "--pretty",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"profile_schema\""))
        .stdout(predicate::str::contains("\"command\": \"sxmc\""))
        .stdout(predicate::str::contains("\"subcommands\""));
}

#[test]
fn test_inspect_cli_git_detects_common_subcommands() {
    let profile = command_json(&["inspect", "cli", "git", "--pretty"]);
    assert_eq!(profile["command"], "git");
    let summary = profile["summary"].as_str().unwrap_or_default();
    assert!(!summary.to_ascii_lowercase().starts_with("usage:"));
    assert!(!summary.contains("--exec-path"));
    assert_ne!(
        summary,
        "These are common Git commands used in various situations:"
    );

    let subcommands = profile["subcommands"].as_array().unwrap();
    assert!(subcommands.iter().any(|entry| entry["name"] == "clone"
        && entry["summary"] == "Clone a repository into a new directory"));
    assert!(subcommands.iter().any(|entry| entry["name"] == "fetch"));
    assert!(!subcommands.iter().any(|entry| entry["name"] == "grow"));
    let options = profile["options"].as_array().unwrap();
    assert!(!options.is_empty());
    assert!(options.iter().any(|entry| entry["name"] == "--version"));
}

#[test]
fn test_inspect_cli_cargo_uses_primary_subcommand_names() {
    let profile = command_json(&["inspect", "cli", "cargo", "--pretty"]);
    let subcommands = profile["subcommands"].as_array().unwrap();
    assert!(subcommands.iter().any(|entry| entry["name"] == "build"));
    assert!(!subcommands.iter().any(|entry| entry["name"] == "build, b"));
}

#[test]
fn test_inspect_cli_node_avoids_option_shaped_subcommands() {
    let profile = command_json(&["inspect", "cli", "node", "--pretty"]);
    let subcommands = profile["subcommands"].as_array().unwrap();
    assert!(subcommands.iter().any(|entry| entry["name"] == "inspect"));
    assert!(!subcommands
        .iter()
        .any(|entry| { entry["name"].as_str().unwrap_or_default().starts_with("--") }));
    let summary = profile["summary"].as_str().unwrap_or_default();
    assert!(!summary.contains("interactive mode"));
    assert!(summary.contains("JavaScript") || summary.contains("runtime"));
}

#[test]
fn test_inspect_cli_gh_recovers_top_level_flags() {
    let profile = command_json(&["inspect", "cli", "gh", "--pretty"]);
    let options = profile["options"].as_array().unwrap();
    assert!(options.iter().any(|entry| entry["name"] == "--help"));
    assert!(options.iter().any(|entry| entry["name"] == "--version"));
}

#[test]
fn test_inspect_cli_rustup_recovers_top_level_flags() {
    let profile = command_json(&["inspect", "cli", "rustup", "--pretty"]);
    let options = profile["options"].as_array().unwrap();
    assert!(options.iter().any(|entry| entry["name"] == "--verbose"));
    assert!(options.iter().any(|entry| entry["name"] == "--quiet"));
    assert!(options.iter().any(|entry| entry["name"] == "--help"));
}

#[test]
fn test_inspect_cli_python3_avoids_env_vars_as_subcommands() {
    let profile = command_json(&["inspect", "cli", "python3", "--pretty"]);
    let summary = profile["summary"].as_str().unwrap_or_default();
    assert!(summary.contains("Python") || summary.contains("language"));
    let subcommands = profile["subcommands"].as_array().unwrap();
    assert!(!subcommands.iter().any(|entry| {
        entry["name"]
            .as_str()
            .unwrap_or_default()
            .starts_with("PYTHON")
    }));
    let options = profile["options"].as_array().unwrap();
    assert!(options.iter().any(|entry| entry["name"] == "--help-all"));
}

#[test]
fn test_inspect_cli_npm_uses_better_summary_and_usage_options() {
    let profile = command_json(&["inspect", "cli", "npm", "--pretty"]);
    let summary = profile["summary"].as_str().unwrap_or_default();
    assert!(summary.contains("package manager"));
    let options = profile["options"].as_array().unwrap();
    assert!(options.iter().any(|entry| entry["name"] == "-h"));
    assert!(options.iter().any(|entry| entry["name"] == "-l"));
}

#[test]
fn test_init_ai_preview_for_claude() {
    sxmc()
        .args([
            "init",
            "ai",
            "--from-cli",
            &sxmc_bin_string(),
            "--client",
            "claude-code",
            "--mode",
            "preview",
            "--allow-self",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Target:"))
        .stdout(predicate::str::contains("CLAUDE.md"))
        .stdout(predicate::str::contains("sxmc CLI Surface"))
        .stdout(predicate::str::contains(
            "sxmc inspect cli <tool> --depth 1 --format json-pretty",
        ))
        .stdout(predicate::str::contains("sxmc api <url-or-spec> --list"));
}

#[test]
fn test_init_ai_full_preview_lists_multi_host_targets() {
    sxmc()
        .args([
            "init",
            "ai",
            "--from-cli",
            &sxmc_bin_string(),
            "--coverage",
            "full",
            "--mode",
            "preview",
            "--allow-self",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("AGENTS.md"))
        .stdout(predicate::str::contains("CLAUDE.md"))
        .stdout(predicate::str::contains(".cursor/rules/sxmc-cli-ai.md"))
        .stdout(predicate::str::contains("GEMINI.md"))
        .stdout(predicate::str::contains(".github/copilot-instructions.md"))
        .stdout(predicate::str::contains(".continue/rules/sxmc-cli-ai.md"))
        .stdout(predicate::str::contains("opencode.json"))
        .stdout(predicate::str::contains(
            ".aiassistant/rules/sxmc-cli-ai.md",
        ))
        .stdout(predicate::str::contains(".junie/guidelines.md"))
        .stdout(predicate::str::contains(".windsurf/rules/sxmc-cli-ai.md"))
        .stdout(predicate::str::contains(".cursor/mcp.json"))
        .stdout(predicate::str::contains(".gemini/settings.json"))
        .stdout(predicate::str::contains(".codex/mcp.toml"));
}

#[test]
fn test_init_ai_full_apply_requires_hosts() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "init",
            "ai",
            "--from-cli",
            &sxmc_bin_string(),
            "--coverage",
            "full",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
            "--allow-self",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Full-coverage apply requires at least one --host",
        ));
}

#[test]
fn test_init_ai_full_apply_updates_selected_hosts_and_sidecars_rest() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "init",
            "ai",
            "--from-cli",
            &sxmc_bin_string(),
            "--coverage",
            "full",
            "--host",
            "claude-code,cursor",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
            "--allow-self",
        ])
        .assert()
        .success();

    assert!(temp.path().join("AGENTS.md").exists());
    assert!(temp.path().join("CLAUDE.md").exists());
    assert!(temp.path().join(".cursor/rules/sxmc-cli-ai.md").exists());
    assert!(temp.path().join(".cursor/mcp.json").exists());
    assert!(temp
        .path()
        .join(".sxmc/ai/github-copilot/copilot-instructions.md.sxmc.snippet")
        .exists());
    assert!(temp
        .path()
        .join(".sxmc/ai/continue/sxmc-cli-ai.md.sxmc.snippet")
        .exists());
    assert!(temp
        .path()
        .join(".sxmc/ai/opencode/opencode.json.sxmc.snippet")
        .exists());
    assert!(temp
        .path()
        .join(".sxmc/ai/jetbrains-ai-assistant/sxmc-cli-ai.md.sxmc.snippet")
        .exists());
    assert!(temp
        .path()
        .join(".sxmc/ai/junie/guidelines.md.sxmc.snippet")
        .exists());
    assert!(temp
        .path()
        .join(".sxmc/ai/windsurf/sxmc-cli-ai.md.sxmc.snippet")
        .exists());
    assert!(temp
        .path()
        .join(".sxmc/ai/gemini-cli/GEMINI.md.sxmc.snippet")
        .exists());
    assert!(temp
        .path()
        .join(".sxmc/ai/openai-codex/mcp.toml.sxmc.snippet")
        .exists());
    assert!(temp
        .path()
        .join(".sxmc/ai/openai-codex/AGENTS.md.sxmc.snippet")
        .exists());
}

#[test]
fn test_scaffold_agent_doc_apply_preserves_existing_content() {
    let temp = tempfile::tempdir().unwrap();
    let agents = temp.path().join("AGENTS.md");
    fs::write(&agents, "# Existing\n\nKeep me.\n").unwrap();

    sxmc()
        .args([
            "scaffold",
            "agent-doc",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--client",
            "openai-codex",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(&agents).unwrap();
    assert!(contents.contains("Keep me."));
    assert!(contents.contains("<!-- sxmc:begin cli-ai:openai-codex -->"));
    assert!(contents.contains("sxmc CLI Surface: `gh`"));
}

#[test]
fn test_init_ai_full_apply_keeps_multiple_agents_blocks_for_shared_targets() {
    let temp = tempfile::tempdir().unwrap();
    let agents = temp.path().join("AGENTS.md");
    fs::write(&agents, "# Existing\n").unwrap();

    sxmc()
        .args([
            "init",
            "ai",
            "--from-cli",
            "gh",
            "--coverage",
            "full",
            "--host",
            "open-code,openai-codex",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(&agents).unwrap();
    assert!(contents.contains("<!-- sxmc:begin cli-ai:portable -->"));
    assert!(contents.contains("<!-- sxmc:begin cli-ai:opencode -->"));
    assert!(contents.contains("<!-- sxmc:begin cli-ai:openai-codex -->"));
    assert!(contents.contains("OpenCode"));
    assert!(contents.contains("OpenAI/Codex"));
}

#[test]
fn test_scaffold_agent_doc_apply_for_gemini_writes_gemini_md() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "scaffold",
            "agent-doc",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--client",
            "gemini-cli",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(temp.path().join("GEMINI.md")).unwrap();
    assert!(contents.contains("sxmc CLI Surface: `gh`"));
}

#[test]
fn test_scaffold_agent_doc_apply_for_github_copilot_writes_native_instructions() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "scaffold",
            "agent-doc",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--client",
            "github-copilot",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(temp.path().join(".github/copilot-instructions.md")).unwrap();
    assert!(contents.contains("sxmc CLI Surface: `gh`"));
}

#[test]
fn test_scaffold_agent_doc_apply_for_continue_writes_rules_doc() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "scaffold",
            "agent-doc",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--client",
            "continue-dev",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(temp.path().join(".continue/rules/sxmc-cli-ai.md")).unwrap();
    assert!(contents.contains("sxmc CLI Surface: `gh`"));
}

#[test]
fn test_scaffold_agent_doc_apply_for_jetbrains_ai_assistant_writes_rules_doc() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "scaffold",
            "agent-doc",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--client",
            "jetbrains-ai-assistant",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let contents =
        fs::read_to_string(temp.path().join(".aiassistant/rules/sxmc-cli-ai.md")).unwrap();
    assert!(contents.contains("sxmc CLI Surface: `gh`"));
}

#[test]
fn test_scaffold_agent_doc_apply_for_junie_writes_guidelines() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "scaffold",
            "agent-doc",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--client",
            "junie",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(temp.path().join(".junie/guidelines.md")).unwrap();
    assert!(contents.contains("sxmc CLI Surface: `gh`"));
}

#[test]
fn test_scaffold_agent_doc_apply_for_windsurf_writes_rules_doc() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "scaffold",
            "agent-doc",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--client",
            "windsurf",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(temp.path().join(".windsurf/rules/sxmc-cli-ai.md")).unwrap();
    assert!(contents.contains("sxmc CLI Surface: `gh`"));
}

#[test]
fn test_scaffold_client_config_apply_merges_cursor_json() {
    let temp = tempfile::tempdir().unwrap();
    let cursor_dir = temp.path().join(".cursor");
    fs::create_dir_all(&cursor_dir).unwrap();
    let config_path = cursor_dir.join("mcp.json");
    fs::write(
        &config_path,
        r#"{"mcpServers":{"existing":{"command":"foo","args":[]}}}"#,
    )
    .unwrap();

    sxmc()
        .args([
            "scaffold",
            "client-config",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--client",
            "cursor",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(&config_path).unwrap();
    assert!(contents.contains("\"existing\""));
    assert!(contents.contains("\"sxmc-cli-ai-gh\""));
    assert!(contents.contains("\"command\": \"sxmc\""));
}

#[test]
fn test_scaffold_client_config_for_github_copilot_is_rejected() {
    sxmc()
        .args([
            "scaffold",
            "client-config",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--client",
            "github-copilot",
            "--mode",
            "preview",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "GitHub Copilot does not have a native MCP config target",
        ));
}

#[test]
fn test_scaffold_agent_doc_invalid_profile_has_friendly_error() {
    let temp = tempfile::tempdir().unwrap();
    let bad_profile = temp.path().join("bad-profile.json");
    fs::write(&bad_profile, "{not-json").unwrap();

    sxmc()
        .args([
            "scaffold",
            "agent-doc",
            "--from-profile",
            bad_profile.to_str().unwrap(),
            "--client",
            "claude-code",
            "--mode",
            "preview",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not valid JSON"))
        .stderr(predicate::str::contains(
            "sxmc inspect cli <tool> --format json-pretty",
        ));
}

#[test]
fn test_inspect_profile_invalid_schema_has_friendly_error() {
    let temp = tempfile::tempdir().unwrap();
    let bad_profile = temp.path().join("not-a-cli-profile.json");
    fs::write(&bad_profile, r#"{"hello":"world"}"#).unwrap();

    sxmc()
        .args([
            "inspect",
            "profile",
            bad_profile.to_str().unwrap(),
            "--format",
            "json-pretty",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "is not a valid sxmc CLI surface profile",
        ))
        .stderr(predicate::str::contains("profile_schema"));
}

#[test]
fn test_scaffold_client_config_apply_merges_opencode_json() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("opencode.json");
    fs::write(
        &config_path,
        r#"{"mcp":{"existing":{"type":"local","command":["foo"]}}}"#,
    )
    .unwrap();

    sxmc()
        .args([
            "scaffold",
            "client-config",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--client",
            "open-code",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(&config_path).unwrap();
    assert!(contents.contains("\"existing\""));
    assert!(contents.contains("\"sxmc-cli-ai-gh\""));
    assert!(contents.contains("\"mcp\""));
}

#[test]
fn test_scaffold_skill_apply_writes_skill_markdown() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "scaffold",
            "skill",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let skill_path = temp.path().join(".claude/skills/gh-cli/SKILL.md");
    let contents = fs::read_to_string(&skill_path).unwrap();
    assert!(contents.contains("name: gh-cli"));
    assert!(contents.contains("# gh CLI workflow"));
    assert!(contents.contains("Recommended commands:"));
}

#[test]
fn test_scaffold_mcp_wrapper_apply_writes_wrapper_files() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "scaffold",
            "mcp-wrapper",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let wrapper_dir = temp.path().join(".sxmc/mcp-wrappers/gh-mcp-wrapper");
    let readme = fs::read_to_string(wrapper_dir.join("README.md")).unwrap();
    let manifest = fs::read_to_string(wrapper_dir.join("manifest.json")).unwrap();
    assert!(readme.contains("# gh MCP wrapper scaffold"));
    assert!(manifest.contains("\"source_command\": \"gh\""));
    assert!(manifest.contains("\"suggested_tools\""));
}

#[test]
fn test_scaffold_llms_txt_apply_writes_export() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "scaffold",
            "llms-txt",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(temp.path().join("llms.txt")).unwrap();
    assert!(contents.contains("# gh"));
    assert!(contents.contains("## Recommended Commands"));
}

#[test]
fn test_init_ai_remove_cleans_up_applied_files() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "init",
            "ai",
            "--from-cli",
            "cargo",
            "--client",
            "claude-code",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
            "--depth",
            "1",
        ])
        .assert()
        .success();

    let claude_path = temp.path().join("CLAUDE.md");
    assert!(claude_path.exists());

    sxmc()
        .args([
            "init",
            "ai",
            "--from-cli",
            "cargo",
            "--client",
            "claude-code",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
            "--depth",
            "1",
            "--remove",
        ])
        .assert()
        .success();

    assert!(
        !claude_path.exists()
            || !fs::read_to_string(&claude_path)
                .unwrap()
                .contains("sxmc CLI Surface"),
        "expected CLI->AI remove to clean up the managed CLAUDE.md block"
    );
}

#[test]
fn test_scan_clean_skills() {
    sxmc()
        .args([
            "scan",
            "--paths",
            "tests/fixtures",
            "--skill",
            "simple-skill",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn test_scan_malicious_skill() {
    sxmc()
        .args([
            "scan",
            "--paths",
            "tests/fixtures",
            "--skill",
            "malicious-skill",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("CRITICAL"))
        .stdout(predicate::str::contains("SL-INJ-001"));
}

#[test]
fn test_scan_json_output() {
    sxmc()
        .args([
            "scan",
            "--paths",
            "tests/fixtures",
            "--skill",
            "malicious-skill",
            "--json",
        ])
        .assert()
        .stdout(predicate::str::contains("\"findings\""))
        .stdout(predicate::str::contains("\"severity\""))
        .stdout(predicate::str::contains("\"critical\": 2"));
}

#[test]
fn test_scan_json_output_is_single_document_for_multiple_targets() {
    let stdout = command_stdout(&["scan", "--paths", "tests/fixtures", "--json"]);
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(value.get("reports").is_some());
    assert_eq!(value["count"].as_u64(), Some(4));
}

#[test]
fn test_scan_severity_filter() {
    sxmc()
        .args([
            "scan",
            "--paths",
            "tests/fixtures",
            "--skill",
            "malicious-skill",
            "--severity",
            "critical",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("CRITICAL"))
        .stdout(predicate::str::contains("ERROR").not())
        .stdout(predicate::str::contains("WARN").not());
}

#[test]
fn test_scan_all_fixtures() {
    sxmc()
        .args(["scan", "--paths", "tests/fixtures"])
        .assert()
        // Should find issues in malicious-skill
        .stdout(predicate::str::contains("PASS").or(predicate::str::contains("SCAN")));
}

#[test]
fn test_bake_lifecycle() {
    // Create
    sxmc()
        .args([
            "bake",
            "create",
            "test-bake",
            "--type",
            "stdio",
            "--source",
            "echo hello",
            "--description",
            "Test bake config",
            "--skip-validate",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created bake: test-bake"));

    // List
    sxmc()
        .args(["bake", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-bake"));

    // Show
    sxmc()
        .args(["bake", "show", "test-bake"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Name: test-bake"))
        .stdout(predicate::str::contains("Source: echo hello"));

    // Update
    sxmc()
        .args([
            "bake",
            "update",
            "test-bake",
            "--source",
            "echo updated",
            "--description",
            "Updated bake config",
            "--skip-validate",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated bake: test-bake"));

    sxmc()
        .args(["bake", "show", "test-bake"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Source: echo updated"))
        .stdout(predicate::str::contains("Description: Updated bake config"));

    // Remove
    sxmc()
        .args(["bake", "remove", "test-bake"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed bake: test-bake"));

    // Verify removed
    sxmc()
        .args(["bake", "show", "test-bake"])
        .assert()
        .failure();
}

#[test]
fn test_mcp_servers_and_tools_via_bake() {
    let temp = tempfile::tempdir().unwrap();
    let bake_name = "fixture-mcp-tools";
    let inner = serde_json::to_string(&vec![
        sxmc_bin_string(),
        "serve".to_string(),
        "--paths".to_string(),
        "tests/fixtures".to_string(),
    ])
    .unwrap();

    sxmc_with_config_home(temp.path())
        .args([
            "bake",
            "create",
            bake_name,
            "--type",
            "stdio",
            "--source",
            &inner,
            "--description",
            "Fixture MCP server",
        ])
        .assert()
        .success();

    sxmc_with_config_home(temp.path())
        .args(["mcp", "servers"])
        .assert()
        .success();

    let output = sxmc_with_config_home(temp.path())
        .args(["mcp", "servers"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value[0]["name"], bake_name);
    assert_eq!(value[0]["transport"], "stdio");
    assert_eq!(value[0]["description"], "Fixture MCP server");

    sxmc_with_config_home(temp.path())
        .args(["mcp", "tools", bake_name, "--limit", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tools (2 shown of"))
        .stdout(predicate::str::contains("get_available_skills"));
}

#[test]
fn test_mcp_grep_via_bake() {
    let temp = tempfile::tempdir().unwrap();
    let inner = serde_json::to_string(&vec![
        sxmc_bin_string(),
        "serve".to_string(),
        "--paths".to_string(),
        "tests/fixtures".to_string(),
    ])
    .unwrap();

    sxmc_with_config_home(temp.path())
        .args([
            "bake",
            "create",
            "fixture-mcp",
            "--type",
            "stdio",
            "--source",
            &inner,
        ])
        .assert()
        .success();

    sxmc_with_config_home(temp.path())
        .args(["mcp", "grep", "skill", "--limit", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Matches for 'skill'"))
        .stdout(predicate::str::contains("fixture-mcp/get_available_skills"))
        .stdout(predicate::str::contains("fixture-mcp/get_skill_details"));
}

#[test]
fn test_stdio_missing_command_has_install_hint() {
    sxmc()
        .args(["stdio", "definitely-not-a-real-command-xyz", "--list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("command not found on PATH"))
        .stderr(predicate::str::contains("npx"));
}

#[test]
fn test_mcp_info_call_prompt_and_read_via_bake() {
    let temp = tempfile::tempdir().unwrap();
    let bake_name = "fixture-mcp-info";
    let inner = serde_json::to_string(&vec![
        sxmc_bin_string(),
        "serve".to_string(),
        "--paths".to_string(),
        "tests/fixtures".to_string(),
    ])
    .unwrap();

    sxmc_with_config_home(temp.path())
        .args([
            "bake", "create", bake_name, "--type", "stdio", "--source", &inner,
        ])
        .assert()
        .success();

    sxmc_with_config_home(temp.path())
        .args([
            "mcp",
            "info",
            "fixture-mcp-info/get_skill_details",
            "--format",
            "toon",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#"name: "get_skill_details""#))
        .stdout(predicate::str::contains("input_schema:"));

    sxmc_with_config_home(temp.path())
        .args([
            "mcp",
            "call",
            "fixture-mcp-info/get_skill_details",
            r#"{"name":"simple-skill","return_type":"both"}"#,
            "--pretty",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"simple-skill\""));

    sxmc_with_config_home(temp.path())
        .args([
            "mcp",
            "prompt",
            "fixture-mcp-info/simple-skill",
            "arguments=friend",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello friend, welcome to sxmc!"));

    sxmc_with_config_home(temp.path())
        .args([
            "mcp",
            "read",
            "fixture-mcp-info/skill://skill-with-references/references/style-guide.md",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Style Guide"))
        .stdout(predicate::str::contains("Use clear, concise language"));
}

#[test]
fn test_mcp_session_preserves_stateful_tool_memory() {
    let temp = tempfile::tempdir().unwrap();
    let bake_name = "stateful-mcp";
    let source = stateful_mcp_command_spec();

    sxmc_with_config_home(temp.path())
        .args([
            "bake", "create", bake_name, "--type", "stdio", "--source", &source,
        ])
        .assert()
        .success();

    sxmc_with_config_home(temp.path())
        .args(["mcp", "session", bake_name, "--quiet"])
        .write_stdin(
            "call remember_state '{\"key\":\"topic\",\"value\":\"alpha\"}' --pretty\n\
             call read_state '{\"key\":\"topic\"}' --pretty\n\
             exit\n",
        )
        .assert()
        .success()
        .stdout(predicate::str::contains("\"stored\": true"))
        .stdout(predicate::str::contains("\"value\": \"alpha\""));
}

#[test]
fn test_no_subcommand_shows_help() {
    sxmc()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn test_stdio_lists_hybrid_skill_tools() {
    let inner = format!("{} serve --paths tests/fixtures", sxmc_bin_string());

    sxmc()
        .args(["stdio", &inner, "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("get_available_skills"))
        .stdout(predicate::str::contains("get_skill_details"))
        .stdout(predicate::str::contains("get_skill_related_file"))
        .stdout(predicate::str::contains("skill_with_scripts__hello"));
}

#[test]
fn test_stdio_accepts_json_array_command_spec() {
    let inner = serde_json::to_string(&vec![
        sxmc_bin_string(),
        "serve".to_string(),
        "--paths".to_string(),
        "tests/fixtures".to_string(),
    ])
    .unwrap();

    sxmc()
        .args(["stdio", &inner, "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("get_available_skills"))
        .stdout(predicate::str::contains("skill_with_scripts__hello"));
}

#[test]
fn test_stdio_lists_prompts_explicitly() {
    let inner = format!("{} serve --paths tests/fixtures", sxmc_bin_string());

    sxmc()
        .args(["stdio", &inner, "--list-prompts"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Prompts"))
        .stdout(predicate::str::contains("simple-skill"));
}

#[test]
fn test_stdio_describe_reports_capabilities_and_counts() {
    let inner = format!("{} serve --paths tests/fixtures", sxmc_bin_string());

    sxmc()
        .args(["stdio", &inner, "--describe", "--pretty"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"protocol_version\""))
        .stdout(predicate::str::contains("\"capabilities\""))
        .stdout(predicate::str::contains("\"tools\": true"))
        .stdout(predicate::str::contains("\"prompts\": true"))
        .stdout(predicate::str::contains("\"resources\": true"))
        .stdout(predicate::str::contains("\"counts\""));
}

#[test]
fn test_stdio_describe_is_summary_oriented_and_respects_limit() {
    let inner = format!("{} serve --paths tests/fixtures", sxmc_bin_string());

    sxmc()
        .args(["stdio", &inner, "--describe", "--pretty", "--limit", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"detail_mode\": \"summary\""))
        .stdout(predicate::str::contains("\"limit\": 1"))
        .stdout(predicate::str::contains("\"shown\""))
        .stdout(predicate::str::contains("\"parameter_count\""))
        .stdout(predicate::str::contains("\"parameter_names\""))
        .stdout(predicate::str::contains("\"truncated\""))
        .stdout(predicate::str::contains("\"tools\": true"))
        .stdout(predicate::str::contains("\"prompts\": true"))
        .stdout(predicate::str::contains("\"resources\": true"))
        .stdout(predicate::str::contains("\"input_schema\"").not());
}

#[test]
fn test_stdio_list_tools_respects_limit() {
    let inner = format!("{} serve --paths tests/fixtures", sxmc_bin_string());

    sxmc()
        .args(["stdio", &inner, "--list-tools", "--limit", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tools (1 shown of"))
        .stdout(predicate::str::contains("get_available_skills"));
}

#[test]
fn test_stdio_describe_tool_shows_schema_summary() {
    let inner = format!("{} serve --paths tests/fixtures", sxmc_bin_string());

    sxmc()
        .args(["stdio", &inner, "--describe-tool", "get_skill_details"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tool: get_skill_details"))
        .stdout(predicate::str::contains("name [required]"))
        .stdout(predicate::str::contains("Parameters"));
}

#[test]
fn test_stdio_describe_tool_supports_toon_format() {
    let inner = format!("{} serve --paths tests/fixtures", sxmc_bin_string());

    sxmc()
        .args([
            "stdio",
            &inner,
            "--describe-tool",
            "get_skill_details",
            "--format",
            "toon",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#"name: "get_skill_details""#))
        .stdout(predicate::str::contains("parameters:"))
        .stdout(predicate::str::contains("input_schema:"));
}

#[test]
fn test_stdio_hybrid_get_skill_details() {
    let inner = format!("{} serve --paths tests/fixtures", sxmc_bin_string());

    sxmc()
        .args([
            "stdio",
            &inner,
            "get_skill_details",
            "name=simple-skill",
            "return_type=both",
            "--pretty",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"simple-skill\""))
        .stdout(predicate::str::contains(
            "\"prompt_name\": \"simple-skill\"",
        ))
        .stdout(predicate::str::contains(
            "Hello $ARGUMENTS, welcome to sxmc!",
        ));
}

#[test]
fn test_stdio_reads_prompt() {
    let inner = format!("{} serve --paths tests/fixtures", sxmc_bin_string());

    sxmc()
        .args([
            "stdio",
            &inner,
            "--prompt",
            "simple-skill",
            "arguments=friend",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello friend, welcome to sxmc!"));
}

#[test]
fn test_stdio_hybrid_get_skill_related_file() {
    let inner = format!("{} serve --paths tests/fixtures", sxmc_bin_string());

    sxmc()
        .args([
            "stdio",
            &inner,
            "get_skill_related_file",
            "skill_name=skill-with-references",
            "relative_path=references/style-guide.md",
            "return_type=content",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Style Guide"))
        .stdout(predicate::str::contains("Use clear, concise language"));
}

#[test]
fn test_stdio_reads_resource() {
    let inner = format!("{} serve --paths tests/fixtures", sxmc_bin_string());

    sxmc()
        .args([
            "stdio",
            &inner,
            "--resource",
            "skill://skill-with-references/references/style-guide.md",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Style Guide"))
        .stdout(predicate::str::contains("Use clear, concise language"));
}

#[test]
fn test_stdio_executes_project_local_skill_script_without_explicit_paths() {
    let temp = tempfile::tempdir().unwrap();
    let skill_dst = temp
        .path()
        .join(".claude")
        .join("skills")
        .join("project-local-skill");
    let scripts_dir = skill_dst.join("scripts");

    fs::create_dir_all(&scripts_dir).unwrap();
    fs::write(
        skill_dst.join("SKILL.md"),
        "---\nname: project-local-skill\ndescription: Project-local regression skill\n---\nThis skill has tools available.\n",
    )
    .unwrap();

    #[cfg(windows)]
    let script_name = "hello.cmd";
    #[cfg(not(windows))]
    let script_name = "hello.sh";

    let script_path = scripts_dir.join(script_name);

    #[cfg(windows)]
    fs::write(
        &script_path,
        "@echo off\r\necho Hello from script! Args: %*\r\n",
    )
    .unwrap();

    #[cfg(not(windows))]
    {
        fs::write(
            &script_path,
            "#!/bin/sh\necho \"Hello from script! Args: $@\"\n",
        )
        .unwrap();
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let inner = format!("{} serve", sxmc_bin_string());

    sxmc()
        .current_dir(temp.path())
        .args([
            "stdio",
            &inner,
            "project_local_skill__hello",
            "args=from-regression-test",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Hello from script! Args: from-regression-test",
        ));
}

#[test]
fn test_http_lists_hybrid_skill_tools() {
    let (mut child, port) = spawn_http_server(&["--paths", "tests/fixtures"]);

    sxmc()
        .args(["http", &format!("http://127.0.0.1:{port}/mcp"), "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("get_available_skills"))
        .stdout(predicate::str::contains("get_skill_details"))
        .stdout(predicate::str::contains("skill_with_scripts__hello"));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_http_lists_resources_explicitly() {
    let (mut child, port) = spawn_http_server(&["--paths", "tests/fixtures"]);

    sxmc()
        .args([
            "http",
            &format!("http://127.0.0.1:{port}/mcp"),
            "--list-resources",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Resources"))
        .stdout(predicate::str::contains("style-guide.md"));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_http_reads_prompt() {
    let (mut child, port) = spawn_http_server(&["--paths", "tests/fixtures"]);

    sxmc()
        .args([
            "http",
            &format!("http://127.0.0.1:{port}/mcp"),
            "--prompt",
            "simple-skill",
            "arguments=friend",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello friend, welcome to sxmc!"));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_http_reads_resource() {
    let (mut child, port) = spawn_http_server(&["--paths", "tests/fixtures"]);

    sxmc()
        .args([
            "http",
            &format!("http://127.0.0.1:{port}/mcp"),
            "--resource",
            "skill://skill-with-references/references/style-guide.md",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Style Guide"))
        .stdout(predicate::str::contains("Use clear, concise language"));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_http_lists_hybrid_skill_tools_with_required_header() {
    let (mut child, port) = spawn_http_server(&[
        "--require-header",
        "Authorization: Bearer integration-token",
        "--paths",
        "tests/fixtures",
    ]);

    sxmc()
        .args([
            "http",
            &format!("http://127.0.0.1:{port}/mcp"),
            "--auth-header",
            "Authorization: Bearer integration-token",
            "--list",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("get_available_skills"))
        .stdout(predicate::str::contains("get_skill_details"))
        .stdout(predicate::str::contains("skill_with_scripts__hello"));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_http_lists_hybrid_skill_tools_with_bearer_token() {
    let mut child = ProcessCommand::new(sxmc_bin_string())
        .env("SXMC_TEST_BEARER_TOKEN", "integration-bearer-token")
        .args([
            "serve",
            "--transport",
            "http",
            "--host",
            "127.0.0.1",
            "--port",
            "0",
            "--bearer-token",
            "env:SXMC_TEST_BEARER_TOKEN",
            "--paths",
            "tests/fixtures",
        ])
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().expect("child stderr should be piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut sent = false;
        for line in reader.lines().map_while(Result::ok) {
            if !sent {
                if let Some(port) = line
                    .split("http://127.0.0.1:")
                    .nth(1)
                    .and_then(|tail| tail.split("/mcp").next())
                    .and_then(|port| port.parse::<u16>().ok())
                {
                    let _ = sender.send(port);
                    sent = true;
                }
            }
        }
    });
    let port = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("timed out waiting for bearer HTTP server port");
    wait_for_http_server(port);

    sxmc()
        .args([
            "http",
            &format!("http://127.0.0.1:{port}/mcp"),
            "--auth-header",
            "Authorization: Bearer integration-bearer-token",
            "--list",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("get_available_skills"))
        .stdout(predicate::str::contains("get_skill_details"))
        .stdout(predicate::str::contains("skill_with_scripts__hello"));

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn test_http_health_endpoint_reports_auth_mode() {
    let (mut child, port) = spawn_http_server(&[
        "--bearer-token",
        "health-token",
        "--paths",
        "tests/fixtures",
    ]);

    let response: serde_json::Value = reqwest::get(format!("http://127.0.0.1:{port}/healthz"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(response["status"], "ok");
    assert_eq!(response["transport"], "streamable-http");
    assert_eq!(response["auth"]["enabled"], true);
    assert_eq!(response["auth"]["schemes"], serde_json::json!(["bearer"]));
    assert_eq!(response["inventory"]["skills"], 4);
    assert_eq!(response["inventory"]["tools"], 1);
    assert_eq!(response["inventory"]["resources"], 1);

    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_spec_supports_toon_output_format() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let app = Router::new().route(
            "/pets",
            get(|| async {
                Json(serde_json::json!({
                    "pets": [
                        {"id": 1, "name": "Mochi"},
                        {"id": 2, "name": "Pixel"}
                    ]
                }))
            }),
        );
        let _ = axum::serve(listener, app).await;
    });

    let temp = tempfile::tempdir().unwrap();
    let spec_path = temp.path().join("petstore.json");
    fs::write(
        &spec_path,
        serde_json::json!({
            "openapi": "3.0.0",
            "info": { "title": "Local Pets" },
            "servers": [{ "url": format!("http://{addr}") }],
            "paths": {
                "/pets": {
                    "get": {
                        "operationId": "listPets",
                        "summary": "List pets",
                        "responses": {
                            "200": { "description": "ok" }
                        }
                    }
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    sxmc()
        .args([
            "spec",
            spec_path.to_str().unwrap(),
            "listPets",
            "--format",
            "toon",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("pets[2]{id,name}:"))
        .stdout(predicate::str::contains(r#"  1,"Mochi""#))
        .stdout(predicate::str::contains(r#"  2,"Pixel""#));

    handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_api_autodetect_openapi_local_list_and_call() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let spec = serde_json::json!({
        "openapi": "3.0.0",
        "info": { "title": "Local Pets", "version": "1.0.0" },
        "servers": [{ "url": format!("http://{addr}") }],
        "paths": {
            "/pets": {
                "get": {
                    "operationId": "listPets",
                    "summary": "List pets",
                    "parameters": [
                        { "name": "limit", "in": "query", "schema": { "type": "integer" } }
                    ],
                    "responses": { "200": { "description": "ok" } }
                }
            }
        }
    });
    let spec_clone = spec.clone();
    let handle = tokio::spawn(async move {
        let app = Router::new()
            .route(
                "/openapi.json",
                get(move || {
                    let spec = spec_clone.clone();
                    async move { Json(spec) }
                }),
            )
            .route(
                "/pets",
                get(|| async {
                    Json(serde_json::json!({
                        "pets": [
                            {"id": 1, "name": "Mochi"},
                            {"id": 2, "name": "Pixel"}
                        ]
                    }))
                }),
            );
        let _ = axum::serve(listener, app).await;
    });

    let base = format!("http://{addr}/openapi.json");

    sxmc()
        .args(["api", &base, "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("listPets"));

    sxmc()
        .args(["api", &base, "listPets", "limit=2", "--format", "toon"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pets[2]{id,name}:"))
        .stdout(predicate::str::contains(r#"  1,"Mochi""#));

    handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_api_list_supports_json_output() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let spec = serde_json::json!({
        "openapi": "3.0.0",
        "info": { "title": "Local Pets", "version": "1.0.0" },
        "servers": [{ "url": format!("http://{addr}") }],
        "paths": {
            "/pets": {
                "get": {
                    "operationId": "listPets",
                    "summary": "List pets",
                    "parameters": [
                        { "name": "limit", "in": "query", "schema": { "type": "integer" } }
                    ],
                    "responses": { "200": { "description": "ok" } }
                }
            }
        }
    });
    let spec_clone = spec.clone();
    let handle = tokio::spawn(async move {
        let app = Router::new().route(
            "/openapi.json",
            get(move || {
                let spec = spec_clone.clone();
                async move { Json(spec) }
            }),
        );
        let _ = axum::serve(listener, app).await;
    });

    let base = format!("http://{addr}/openapi.json");
    let output = ProcessCommand::new(sxmc_bin_string())
        .args(["api", &base, "--list", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "command failed: {}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("api --list should emit JSON");
    assert_eq!(value["api_type"], "OpenAPI");
    assert_eq!(value["count"], 1);
    assert_eq!(value["operations"][0]["name"], "listPets");

    handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_graphql_local_list_and_call() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let app = Router::new().route(
            "/graphql",
            post(|Json(payload): Json<serde_json::Value>| async move {
                let query = payload
                    .get("query")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                if query.contains("__schema") {
                    Json(serde_json::json!({
                        "data": {
                            "__schema": {
                                "queryType": { "name": "Query" },
                                "mutationType": null,
                                "types": [
                                    {
                                        "kind": "OBJECT",
                                        "name": "Query",
                                        "fields": [
                                            {
                                                "name": "hello",
                                                "args": [],
                                                "type": { "kind": "SCALAR", "name": "String", "ofType": null }
                                            },
                                            {
                                                "name": "echo",
                                                "args": [
                                                    {
                                                        "name": "message",
                                                        "type": { "kind": "SCALAR", "name": "String", "ofType": null },
                                                        "defaultValue": null
                                                    }
                                                ],
                                                "type": { "kind": "SCALAR", "name": "String", "ofType": null }
                                            }
                                        ]
                                    },
                                    {
                                        "kind": "SCALAR",
                                        "name": "String",
                                        "fields": null,
                                        "inputFields": null,
                                        "interfaces": null,
                                        "enumValues": null,
                                        "possibleTypes": null
                                    }
                                ],
                                "directives": []
                            }
                        }
                    }))
                } else if query.contains("echo") {
                    let message = payload
                        .get("variables")
                        .and_then(|value| value.get("message"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    Json(serde_json::json!({ "data": { "echo": message } }))
                } else {
                    Json(serde_json::json!({ "data": { "hello": "world" } }))
                }
            }),
        );
        let _ = axum::serve(listener, app).await;
    });

    let base = format!("http://{addr}/graphql");

    sxmc()
        .args(["graphql", &base, "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"))
        .stdout(predicate::str::contains("echo"));

    sxmc()
        .args(["graphql", &base, "echo", "message=hello", "--pretty"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"echo\": \"hello\""));

    handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_skills_create_from_local_spec() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let spec = serde_json::json!({
        "openapi": "3.0.0",
        "info": { "title": "Local Pets API", "version": "1.0.0" },
        "servers": [{ "url": format!("http://{addr}") }],
        "paths": {
            "/pets": {
                "get": {
                    "operationId": "listPets",
                    "summary": "List pets",
                    "responses": { "200": { "description": "ok" } }
                }
            }
        }
    });
    let spec_clone = spec.clone();
    let handle = tokio::spawn(async move {
        let app = Router::new().route(
            "/openapi.json",
            get(move || {
                let spec = spec_clone.clone();
                async move { Json(spec) }
            }),
        );
        let _ = axum::serve(listener, app).await;
    });

    let temp = tempfile::tempdir().unwrap();
    let output_dir = temp.path().join("generated-skills");

    sxmc()
        .args([
            "skills",
            "create",
            &format!("http://{addr}/openapi.json"),
            "--output-dir",
            output_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Generated skill at:"));

    let skill_path = output_dir.join("local-pets-api").join("SKILL.md");
    assert!(skill_path.exists());
    let skill_body = fs::read_to_string(&skill_path).unwrap();
    assert!(skill_body.contains("listPets"));

    handle.abort();
}

#[test]
fn test_serve_watch_reloads_skill_prompt_over_http() {
    let temp = tempfile::tempdir().unwrap();
    let skill_dir = temp.path().join("watch-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_path = skill_dir.join("SKILL.md");
    fs::write(
        &skill_path,
        r#"---
name: watch-skill
description: "Watch reload test"
argument-hint: "[name]"
---

Hello version one, $ARGUMENTS!
"#,
    )
    .unwrap();

    let (mut child, port) =
        spawn_http_server(&["--watch", "--paths", temp.path().to_str().unwrap()]);

    let before = command_stdout(&[
        "http",
        &format!("http://127.0.0.1:{port}/mcp"),
        "--prompt",
        "watch-skill",
        "arguments=friend",
    ]);
    assert!(before.contains("Hello version one, friend!"));

    fs::write(
        &skill_path,
        r#"---
name: watch-skill
description: "Watch reload test"
argument-hint: "[name]"
---

Hello version two, $ARGUMENTS!
"#,
    )
    .unwrap();

    let mut saw_reload = false;
    for _ in 0..12 {
        std::thread::sleep(Duration::from_millis(300));
        let after = command_stdout(&[
            "http",
            &format!("http://127.0.0.1:{port}/mcp"),
            "--prompt",
            "watch-skill",
            "arguments=friend",
        ]);
        if after.contains("Hello version two, friend!") {
            saw_reload = true;
            break;
        }
    }

    let _ = child.kill();
    let _ = child.wait();

    assert!(
        saw_reload,
        "watch mode did not reload the updated skill body"
    );
}
