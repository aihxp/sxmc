use assert_cmd::Command;
use axum::{extract::State, routing::get, routing::post, routing::put, Json, Router};
use predicates::prelude::*;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

fn sxmc() -> Command {
    Command::cargo_bin("sxmc").unwrap()
}

fn has_command(name: &str) -> bool {
    ProcessCommand::new(name)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
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

fn spawn_wrap_http_server(command_spec: &str, extra_args: &[&str]) -> (Child, u16) {
    let mut child = ProcessCommand::new(sxmc_bin_string())
        .args([
            "wrap",
            command_spec,
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
        .expect("timed out waiting for wrapped HTTP server port");
    wait_for_http_server(port);
    (child, port)
}

fn spawn_registry_http_server(registry_dir: &Path, extra_args: &[&str]) -> (Child, u16) {
    let mut child = ProcessCommand::new(sxmc_bin_string())
        .args([
            "inspect",
            "registry-serve",
            "--registry",
            registry_dir.to_str().unwrap(),
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
                    .and_then(|tail| tail.split("/index.json").next())
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
        .expect("timed out waiting for registry HTTP server port");
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

fn command_json_with_config_home(home: &Path, args: &[&str]) -> Value {
    let output = sxmc_with_config_home(home).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "command failed: {}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
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

#[cfg(not(windows))]
fn write_fake_wrappable_cli(dir: &Path) -> std::path::PathBuf {
    let script = dir.join("fake-wrap-cli");
    let body = r#"#!/bin/sh
if [ "$1" = "hello" ] && [ "$2" = "--help" ]; then
    cat <<'EOF'
fake-wrap-cli hello

Say hello.

Usage:
  fake-wrap-cli hello [OPTIONS] <target>

Options:
  --name <NAME>     Override the target name.
  --excited         Add emphasis.
EOF
elif [ "$1" = "goodbye" ] && [ "$2" = "--help" ]; then
    cat <<'EOF'
fake-wrap-cli goodbye

Say goodbye.

Usage:
  fake-wrap-cli goodbye [OPTIONS]

Options:
  --name <NAME>     Person to say goodbye to.
EOF
elif [ "$1" = "pwd" ] && [ "$2" = "--help" ]; then
    cat <<'EOF'
fake-wrap-cli pwd

Report the current working directory.

Usage:
  fake-wrap-cli pwd
EOF
elif [ "$1" = "spam" ] && [ "$2" = "--help" ]; then
    cat <<'EOF'
fake-wrap-cli spam

Emit repeated output.

Usage:
  fake-wrap-cli spam [OPTIONS]

Options:
  --count <COUNT>   Number of characters to emit.
EOF
elif [ "$1" = "slow" ] && [ "$2" = "--help" ]; then
    cat <<'EOF'
fake-wrap-cli slow

Sleep briefly, then report completion.

Usage:
  fake-wrap-cli slow [OPTIONS]

Options:
  --seconds <SECONDS>   Seconds to sleep before completing.
EOF
elif [ "$1" = "hello" ]; then
    shift
    target=""
    name=""
    excited="false"
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --name)
                name="$2"
                shift 2
                ;;
            --excited)
                excited="true"
                shift
                ;;
            *)
                if [ -z "$target" ]; then
                    target="$1"
                fi
                shift
                ;;
        esac
    done
    if [ -n "$name" ]; then
        target="$name"
    fi
    if [ -z "$target" ]; then
        target="world"
    fi
    suffix=""
    if [ "$excited" = "true" ]; then
        suffix="!"
    fi
    printf '{"message":"hello %s%s"}\n' "$target" "$suffix"
elif [ "$1" = "goodbye" ]; then
    shift
    target="world"
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --name)
                target="$2"
                shift 2
                ;;
            *)
                shift
                ;;
        esac
    done
    printf '{"message":"goodbye %s"}\n' "$target"
elif [ "$1" = "pwd" ]; then
    printf '{"cwd":"%s"}\n' "$PWD"
elif [ "$1" = "spam" ]; then
    shift
    count="2048"
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --count)
                count="$2"
                shift 2
                ;;
            *)
                shift
                ;;
        esac
    done
    python3 - <<'PY' "$count"
import sys
count = int(sys.argv[1])
print("x" * count)
PY
elif [ "$1" = "slow" ]; then
    shift
    seconds="2"
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --seconds)
                seconds="$2"
                shift 2
                ;;
            *)
                shift
                ;;
        esac
    done
    sleep "$seconds"
    printf '{"status":"done","slept":"%s"}\n' "$seconds"
else
    cat <<'EOF'
fake-wrap-cli

CLI wrapping fixture.

Commands:
  hello    Say hello
  goodbye  Say goodbye
  pwd      Report the current working directory
  spam     Emit repeated output
  slow     Sleep briefly, then report completion
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
        .stdout(predicate::str::contains("Sumac"))
        .stdout(predicate::str::contains("serve"))
        .stdout(predicate::str::contains("wrap"))
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

#[cfg(not(windows))]
#[test]
fn test_wrap_stdio_describe_and_call_work_for_fake_cli() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_wrappable_cli(temp.path());
    let spec = serde_json::to_string(&vec![
        sxmc_bin_string(),
        "wrap".to_string(),
        fake.to_string_lossy().into_owned(),
    ])
    .unwrap();

    sxmc()
        .args(["stdio", &spec, "--list-tools"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"))
        .stdout(predicate::str::contains("goodbye"));

    sxmc()
        .args(["stdio", &spec, "--describe-tool", "hello"])
        .assert()
        .success()
        .stdout(predicate::str::contains("name"))
        .stdout(predicate::str::contains("target"))
        .stdout(predicate::str::contains("excited"));

    let output = ProcessCommand::new(sxmc_bin_string())
        .args([
            "stdio",
            &spec,
            "hello",
            "name=Sam",
            "excited=true",
            "--pretty",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["tool"], "hello");
    assert_eq!(value["stdout"], "{\"message\":\"hello Sam!\"}\n");
}

#[cfg(not(windows))]
#[test]
fn test_wrap_respects_allow_tool_and_output_limits() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_wrappable_cli(temp.path());
    let spec = serde_json::to_string(&vec![
        sxmc_bin_string(),
        "wrap".to_string(),
        fake.to_string_lossy().into_owned(),
        "--allow-tool".to_string(),
        "hello,spam".to_string(),
        "--max-stdout-bytes".to_string(),
        "64".to_string(),
    ])
    .unwrap();

    let tools = command_stdout(&["stdio", &spec, "--list-tools"]);
    assert!(tools.contains("hello"));
    assert!(tools.contains("spam"));
    assert!(!tools.contains("goodbye"));

    let output = ProcessCommand::new(sxmc_bin_string())
        .args(["stdio", &spec, "spam", "count=512", "--pretty"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["tool"], "spam");
    assert_eq!(value["stdout_truncated"], Value::Bool(true));
    assert!(value["stdout_bytes"].as_u64().unwrap_or(0) > 64);
}

#[cfg(not(windows))]
#[test]
fn test_wrap_argument_filters_shrink_contract_and_reject_blocked_inputs() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_wrappable_cli(temp.path());
    let spec = serde_json::to_string(&vec![
        sxmc_bin_string(),
        "wrap".to_string(),
        fake.to_string_lossy().into_owned(),
        "--deny-option=--name".to_string(),
        "--deny-positional".to_string(),
        "target".to_string(),
    ])
    .unwrap();

    let described = command_stdout(&["stdio", &spec, "--describe-tool", "hello"]);
    assert!(described.contains("excited"));
    assert!(!described.contains("Override the target name."));
    assert!(!described.contains("Missing required positional 'target'"));

    let output = ProcessCommand::new(sxmc_bin_string())
        .args(["stdio", &spec, "hello", "name=Sam", "--pretty"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unknown argument 'name'"));

    let allowed = ProcessCommand::new(sxmc_bin_string())
        .args(["stdio", &spec, "hello", "excited=true", "--pretty"])
        .output()
        .unwrap();
    assert!(allowed.status.success());
    let value: Value = serde_json::from_slice(&allowed.stdout).unwrap();
    assert_eq!(value["tool"], "hello");
    assert!(value["stdout"]
        .as_str()
        .unwrap_or_default()
        .contains("hello world!"));
}

#[cfg(not(windows))]
#[test]
fn test_wrap_respects_working_dir() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_wrappable_cli(temp.path());
    let spec = serde_json::to_string(&vec![
        sxmc_bin_string(),
        "wrap".to_string(),
        fake.to_string_lossy().into_owned(),
        "--allow-tool".to_string(),
        "pwd".to_string(),
        "--working-dir".to_string(),
        temp.path().to_string_lossy().into_owned(),
    ])
    .unwrap();

    let output = ProcessCommand::new(sxmc_bin_string())
        .args(["stdio", &spec, "pwd", "--pretty"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["tool"], "pwd");
    let expected = fs::canonicalize(temp.path()).unwrap();
    let actual = fs::canonicalize(value["stdout_json"]["cwd"].as_str().unwrap()).unwrap();
    assert_eq!(actual, expected);
}

#[cfg(not(windows))]
#[test]
fn test_wrap_reports_progress_events_and_structured_timeout_errors() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_wrappable_cli(temp.path());
    let spec = serde_json::to_string(&vec![
        sxmc_bin_string(),
        "wrap".to_string(),
        fake.to_string_lossy().into_owned(),
        "--allow-tool".to_string(),
        "slow".to_string(),
        "--progress-seconds".to_string(),
        "1".to_string(),
        "--timeout-seconds".to_string(),
        "1".to_string(),
    ])
    .unwrap();

    let output = ProcessCommand::new(sxmc_bin_string())
        .args(["stdio", &spec, "slow", "seconds=2", "--pretty"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["tool"], "slow");
    assert_eq!(value["timeout"], Value::Bool(true));
    assert!(value["progress_event_count"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(value["long_running"], Value::Bool(true));
    assert!(value["progress_events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| {
            entry["message"]
                .as_str()
                .unwrap_or_default()
                .contains("still running")
        }));
}

#[cfg(not(windows))]
#[test]
fn test_wrap_http_exposes_execution_resources() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_wrappable_cli(temp.path());
    let (mut child, port) = spawn_wrap_http_server(
        fake.to_string_lossy().as_ref(),
        &["--allow-tool", "hello", "--execution-history-limit", "5"],
    );
    let url = format!("http://127.0.0.1:{port}/mcp");

    let output = ProcessCommand::new(sxmc_bin_string())
        .args(["http", &url, "hello", "name=Sam", "--pretty"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let resource_uri = value["execution_resource_uri"]
        .as_str()
        .unwrap()
        .to_string();

    let history: Value = serde_json::from_str(&command_stdout(&[
        "http",
        &url,
        "--resource",
        "sxmc-wrap://executions",
    ]))
    .unwrap();
    assert!(history["count"].as_u64().unwrap_or(0) >= 1);

    let detail: Value = serde_json::from_str(&command_stdout(&[
        "http",
        &url,
        "--resource",
        &resource_uri,
    ]))
    .unwrap();
    assert_eq!(detail["tool"], "hello");
    assert_eq!(detail["execution_resource_uri"], resource_uri);
    let events_uri = detail["events_resource_uri"].as_str().unwrap().to_string();
    let events: Value =
        serde_json::from_str(&command_stdout(&["http", &url, "--resource", &events_uri])).unwrap();
    assert!(events["stdout_event_count"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(events["count"].as_u64().unwrap_or(0), 1);

    let _ = child.kill();
    let _ = child.wait();
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

    let value = command_json_with_config_home(
        temp.path(),
        &["doctor", "--root", temp.path().to_str().unwrap()],
    );
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
fn test_doctor_human_flag_renders_report() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("AGENTS.md"), "# Existing\n").unwrap();

    sxmc_with_config_home(temp.path())
        .args(["doctor", "--human", "--root", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Startup files present"))
        .stdout(predicate::str::contains("Recommended first moves"))
        .stdout(predicate::str::contains("CLI profile cache"));
}

#[test]
fn test_status_reports_saved_profile_drift() {
    let temp = tempfile::tempdir().unwrap();
    let profiles_dir = temp.path().join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();
    let mut profile = command_json_with_config_home(
        temp.path(),
        &["inspect", "cli", "cargo", "--format", "json-pretty"],
    );
    profile["summary"] = Value::from("An older cargo summary");
    fs::write(
        profiles_dir.join("cargo.json"),
        serde_json::to_string_pretty(&profile).unwrap(),
    )
    .unwrap();

    let value = command_json_with_config_home(
        temp.path(),
        &[
            "status",
            "--root",
            temp.path().to_str().unwrap(),
            "--format",
            "json-pretty",
        ],
    );
    assert_eq!(value["saved_profiles"]["drift"]["count"], Value::from(1));
    assert_eq!(
        value["saved_profiles"]["drift"]["changed_count"],
        Value::from(1)
    );
}

#[test]
fn test_status_reports_saved_profile_inventory_freshness() {
    let temp = tempfile::tempdir().unwrap();
    let profiles_dir = temp.path().join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();
    let mut profile = command_json_with_config_home(
        temp.path(),
        &[
            "inspect",
            "cli",
            &sxmc_bin_string(),
            "--allow-self",
            "--format",
            "json-pretty",
        ],
    );
    profile["provenance"]["generated_at"] = Value::from("2025-01-01T00:00:00Z");
    fs::write(
        profiles_dir.join("sxmc.json"),
        serde_json::to_string_pretty(&profile).unwrap(),
    )
    .unwrap();

    let value = command_json_with_config_home(
        temp.path(),
        &[
            "status",
            "--root",
            temp.path().to_str().unwrap(),
            "--format",
            "json-pretty",
        ],
    );
    assert_eq!(
        value["saved_profiles"]["inventory"]["count"],
        Value::from(1)
    );
    assert_eq!(
        value["saved_profiles"]["inventory"]["stale_count"],
        Value::from(1)
    );
    assert_eq!(
        value["saved_profiles"]["inventory"]["entries"][0]["freshness"]["known"],
        Value::Bool(true)
    );
    assert!(
        value["saved_profiles"]["inventory"]["entries"][0]["quality"]["ready_for_agent_docs"]
            .is_boolean()
    );
}

#[test]
fn test_status_reports_host_capabilities_and_baked_health() {
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
            "fixture-health",
            "--type",
            "stdio",
            "--source",
            &inner,
        ])
        .assert()
        .success();

    let output = sxmc_with_config_home(temp.path())
        .args(["status", "--health", "--pretty"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["host_capabilities"]["claude-code"]["label"]
        .as_str()
        .unwrap_or_default()
        .contains("Claude"));
    assert_eq!(value["baked_health"]["count"], Value::from(1));
    assert_eq!(value["baked_health"]["healthy_count"], Value::from(1));
    assert!(value["baked_health"]["avg_latency_ms"].as_u64().is_some());
    assert_eq!(
        value["baked_health"]["slow_threshold_ms"],
        Value::from(1000)
    );
    assert_eq!(
        value["baked_health"]["by_source_type"]["stdio"]["count"],
        Value::from(1)
    );
    assert!(value["baked_health"]["panels"]["mcp"]["avg_latency_ms"]
        .as_u64()
        .is_some());
}

#[test]
fn test_status_health_exit_code_fails_for_unhealthy_bakes() {
    let temp = tempfile::tempdir().unwrap();

    sxmc_with_config_home(temp.path())
        .args([
            "bake",
            "create",
            "fixture-unhealthy",
            "--type",
            "stdio",
            "--source",
            "definitely-not-a-real-command",
            "--skip-validate",
        ])
        .assert()
        .success();

    sxmc_with_config_home(temp.path())
        .args(["status", "--health", "--exit-code"])
        .assert()
        .failure();
}

#[test]
fn test_watch_exit_on_unhealthy_fails_for_unhealthy_bakes() {
    let temp = tempfile::tempdir().unwrap();

    sxmc_with_config_home(temp.path())
        .args([
            "bake",
            "create",
            "fixture-unhealthy-watch",
            "--type",
            "stdio",
            "--source",
            "definitely-not-a-real-command",
            "--skip-validate",
        ])
        .assert()
        .success();

    sxmc_with_config_home(temp.path())
        .args([
            "watch",
            "--health",
            "--exit-on-unhealthy",
            "--format",
            "ndjson",
        ])
        .assert()
        .failure();
}

#[test]
fn test_status_can_compare_hosts() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".cursor")).unwrap();
    fs::write(temp.path().join("CLAUDE.md"), "# Claude\n").unwrap();
    fs::write(
        temp.path().join(".cursor").join("mcp.json"),
        "{\"mcpServers\":{}}\n",
    )
    .unwrap();

    let value = command_json_with_config_home(
        temp.path(),
        &[
            "status",
            "--root",
            temp.path().to_str().unwrap(),
            "--compare-hosts",
            "claude-code,cursor",
            "--pretty",
        ],
    );
    assert_eq!(
        value["host_capability_diff"]["difference_count"],
        Value::from(2)
    );
    let differences = value["host_capability_diff"]["differences"]
        .as_array()
        .unwrap();
    assert!(differences
        .iter()
        .any(|entry| entry["field"] == "doc_present"));
    assert!(differences
        .iter()
        .any(|entry| entry["field"] == "config_present"));
}

#[test]
fn test_inspect_cache_stats_reports_entries() {
    let temp = tempfile::tempdir().unwrap();
    let value = command_json_with_config_home(temp.path(), &["inspect", "cache-stats"]);
    assert!(value["path"].as_str().unwrap_or_default().contains("sxmc"));
    assert!(value["default_ttl_secs"].as_u64().unwrap_or(0) > 0);
}

#[test]
fn test_inspect_batch_returns_profiles_and_failures() {
    let value = command_json(&[
        "inspect",
        "batch",
        "cargo",
        "this-command-should-not-exist-xyz",
    ]);
    assert_eq!(value["count"], Value::from(2));
    assert!(value["success_count"].as_u64().unwrap_or(0) >= 1);
    assert!(value["failed_count"].as_u64().unwrap_or(0) >= 1);
    assert!(value["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .any(|profile| profile["command"] == "cargo"));
    assert!(value["parallelism"].as_u64().unwrap_or(0) >= 1);
}

#[test]
fn test_inspect_drift_detects_changed_saved_profile() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("cargo-before.json");
    let mut profile = command_json(&["inspect", "cli", "cargo", "--format", "json-pretty"]);
    profile["summary"] = Value::from("An older cargo summary");
    fs::write(&path, serde_json::to_string_pretty(&profile).unwrap()).unwrap();

    let value = command_json(&[
        "inspect",
        "drift",
        path.to_str().unwrap(),
        "--format",
        "json-pretty",
    ]);
    assert_eq!(value["count"], Value::from(1));
    assert_eq!(value["changed_count"], Value::from(1));
    assert_eq!(value["entries"][0]["command"], Value::from("cargo"));
}

#[test]
fn test_inspect_bundle_export_and_import_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles_dir = root.join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();

    let git = command_json(&["inspect", "cli", "git", "--pretty"]);
    let cargo = command_json(&["inspect", "cli", "cargo", "--pretty"]);
    fs::write(
        profiles_dir.join("git.json"),
        serde_json::to_string_pretty(&git).unwrap(),
    )
    .unwrap();
    fs::write(
        profiles_dir.join("cargo.json"),
        serde_json::to_string_pretty(&cargo).unwrap(),
    )
    .unwrap();

    let bundle_path = root.join("profiles.bundle.json");
    let export = command_json(&[
        "inspect",
        "bundle-export",
        "--root",
        root.to_str().unwrap(),
        "--output",
        bundle_path.to_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(export["profile_count"], Value::from(2));
    assert!(bundle_path.exists());

    let import_dir = root.join("imported-profiles");
    let import = command_json(&[
        "inspect",
        "bundle-import",
        bundle_path.to_str().unwrap(),
        "--output-dir",
        import_dir.to_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(import["imported_count"], Value::from(2));
    assert!(import_dir.join("git.json").exists());
    assert!(import_dir.join("cargo.json").exists());
}

#[test]
fn test_inspect_bundle_export_and_import_preserve_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles_dir = root.join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();

    let git = command_json(&["inspect", "cli", "git", "--pretty"]);
    fs::write(
        profiles_dir.join("git.json"),
        serde_json::to_string_pretty(&git).unwrap(),
    )
    .unwrap();

    let bundle_path = root.join("team.bundle.json");
    let export = command_json(&[
        "inspect",
        "bundle-export",
        "--root",
        root.to_str().unwrap(),
        "--bundle-name",
        "Platform Bundle",
        "--description",
        "Blessed internal tool set",
        "--role",
        "platform",
        "--hosts",
        "claude-code,cursor",
        "--output",
        bundle_path.to_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(export["metadata"]["name"], Value::from("Platform Bundle"));
    assert_eq!(export["metadata"]["role"], Value::from("platform"));
    assert_eq!(
        export["metadata"]["hosts"],
        Value::from(vec!["claude-code", "cursor"])
    );

    let import_dir = root.join("imported-with-metadata");
    let import = command_json(&[
        "inspect",
        "bundle-import",
        bundle_path.to_str().unwrap(),
        "--output-dir",
        import_dir.to_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(
        import["metadata"]["description"],
        Value::from("Blessed internal tool set")
    );
    assert_eq!(import["imported_count"], Value::from(1));
}

#[test]
fn test_inspect_export_corpus_round_trips_saved_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles_dir = root.join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();

    let git = command_json(&["inspect", "cli", "git", "--pretty"]);
    fs::write(
        profiles_dir.join("git.json"),
        serde_json::to_string_pretty(&git).unwrap(),
    )
    .unwrap();

    let value = command_json(&[
        "inspect",
        "export-corpus",
        "--root",
        root.to_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(
        value["corpus_schema"],
        Value::from("sxmc_profile_corpus_v1")
    );
    assert_eq!(value["count"], Value::from(1));
    assert_eq!(value["entries"][0]["type"], Value::from("profile"));
    assert_eq!(
        value["entries"][0]["profile"]["command"],
        Value::from("git")
    );
    assert!(
        value["entries"][0]["quality"]["score"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );
}

#[test]
fn test_inspect_corpus_stats_and_query() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles_dir = root.join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();

    let git = command_json(&["inspect", "cli", "git", "--pretty"]);
    let cargo = command_json(&["inspect", "cli", "cargo", "--pretty"]);
    fs::write(
        profiles_dir.join("git.json"),
        serde_json::to_string_pretty(&git).unwrap(),
    )
    .unwrap();
    fs::write(
        profiles_dir.join("cargo.json"),
        serde_json::to_string_pretty(&cargo).unwrap(),
    )
    .unwrap();

    let corpus_path = root.join("corpus.json");
    let _ = command_json(&[
        "inspect",
        "export-corpus",
        "--root",
        root.to_str().unwrap(),
        "--output",
        corpus_path.to_str().unwrap(),
        "--pretty",
    ]);

    let stats = command_json(&[
        "inspect",
        "corpus-stats",
        corpus_path.to_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(stats["profile_count"], Value::from(2));
    assert_eq!(stats["command_count"], Value::from(2));
    assert!(stats["average_quality_score"].as_f64().unwrap_or(0.0) > 0.0);

    let query = command_json(&[
        "inspect",
        "corpus-query",
        corpus_path.to_str().unwrap(),
        "--search",
        "git",
        "--limit",
        "5",
        "--pretty",
    ]);
    assert_eq!(query["query"]["search"], Value::from("git"));
    assert_eq!(query["query"]["limit"], Value::from(5));
    assert!(query["match_count"].as_u64().unwrap_or(0) >= 1);
    assert!(query["entries"].as_array().unwrap().iter().any(|entry| {
        entry["command"].as_str().unwrap_or_default() == "git"
            || entry["summary"]
                .as_str()
                .unwrap_or_default()
                .to_lowercase()
                .contains("content")
    }));
    assert!(query["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["quality"]["score"].as_u64().unwrap_or(0) > 0));
}

#[test]
fn test_publish_and_pull_round_trip_via_local_bundle_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles_dir = root.join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();

    let git = command_json(&["inspect", "cli", "git", "--pretty"]);
    fs::write(
        profiles_dir.join("git.json"),
        serde_json::to_string_pretty(&git).unwrap(),
    )
    .unwrap();

    let bundle_path = root.join("published.bundle.json");
    let publish = command_json(&[
        "publish",
        bundle_path.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--bundle-name",
        "Team Bundle",
        "--role",
        "platform",
        "--pretty",
    ]);
    assert_eq!(publish["profile_count"], Value::from(1));
    assert_eq!(publish["transport"], Value::from("file"));
    assert!(publish["sha256"].as_str().unwrap_or_default().len() == 64);
    assert!(bundle_path.exists());

    let verify = command_json(&[
        "inspect",
        "bundle-verify",
        bundle_path.to_str().unwrap(),
        "--expected-sha256",
        publish["sha256"].as_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(verify["verified"], Value::Bool(true));
    assert_eq!(verify["sha256"], publish["sha256"]);

    let pull_dir = root.join("pulled-profiles");
    let pull = command_json(&[
        "pull",
        bundle_path.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--output-dir",
        pull_dir.to_str().unwrap(),
        "--expected-sha256",
        publish["sha256"].as_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(pull["imported_count"], Value::from(1));
    assert_eq!(pull["metadata"]["name"], Value::from("Team Bundle"));
    assert_eq!(pull["sha256"], publish["sha256"]);
    assert!(pull_dir.join("git.json").exists());

    sxmc()
        .args([
            "pull",
            bundle_path.to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
            "--output-dir",
            root.join("wrong-sha-pull").to_str().unwrap(),
            "--expected-sha256",
            "deadbeef",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "did not match the expected SHA-256",
        ));
}

#[test]
fn test_signed_bundle_export_verify_and_pull_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles_dir = root.join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();

    let git = command_json(&["inspect", "cli", "git", "--pretty"]);
    fs::write(
        profiles_dir.join("git.json"),
        serde_json::to_string_pretty(&git).unwrap(),
    )
    .unwrap();

    let secret = "team-secret";
    let bundle_path = root.join("signed.bundle.json");
    let export = command_json(&[
        "inspect",
        "bundle-export",
        "--root",
        root.to_str().unwrap(),
        "--signature-secret",
        secret,
        "--output",
        bundle_path.to_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(export["signature"]["present"], Value::Bool(true));
    assert_eq!(export["signature"]["algorithm"], Value::from("hmac-sha256"));

    let verify = command_json(&[
        "inspect",
        "bundle-verify",
        bundle_path.to_str().unwrap(),
        "--signature-secret",
        secret,
        "--pretty",
    ]);
    assert_eq!(verify["signature"]["present"], Value::Bool(true));
    assert_eq!(verify["signature"]["verified"], Value::Bool(true));

    let pull_dir = root.join("signed-pulled");
    let pull = command_json(&[
        "pull",
        bundle_path.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--output-dir",
        pull_dir.to_str().unwrap(),
        "--signature-secret",
        secret,
        "--pretty",
    ]);
    assert_eq!(pull["imported_count"], Value::from(1));
    assert_eq!(pull["signature"]["verified"], Value::Bool(true));
    assert!(pull_dir.join("git.json").exists());

    sxmc()
        .args([
            "inspect",
            "bundle-verify",
            bundle_path.to_str().unwrap(),
            "--signature-secret",
            "wrong-secret",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "did not match the expected embedded signature",
        ));
}

#[test]
fn test_trust_policy_enforces_signature_quality_and_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles_dir = root.join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();

    let git = command_json(&["inspect", "cli", "git", "--pretty"]);
    fs::write(
        profiles_dir.join("git.json"),
        serde_json::to_string_pretty(&git).unwrap(),
    )
    .unwrap();

    let secret = "team-secret";
    let bundle_path = root.join("policy.bundle.json");
    command_json(&[
        "inspect",
        "bundle-export",
        "--root",
        root.to_str().unwrap(),
        "--bundle-name",
        "Platform Bundle",
        "--description",
        "Team blessed profiles",
        "--role",
        "platform",
        "--hosts",
        "claude-code,cursor",
        "--signature-secret",
        secret,
        "--output",
        bundle_path.to_str().unwrap(),
        "--pretty",
    ]);

    let value = command_json(&[
        "inspect",
        "trust-policy",
        bundle_path.to_str().unwrap(),
        "--signature-secret",
        secret,
        "--require-signature",
        "--require-verified-signature",
        "--min-average-quality",
        "1",
        "--min-ready-count",
        "1",
        "--max-stale-count",
        "10",
        "--require-role",
        "platform",
        "--require-host",
        "claude_code,cursor",
        "--pretty",
    ]);
    assert_eq!(value["passed"], Value::Bool(true));
    assert_eq!(value["report"]["signature"]["verified"], Value::Bool(true));
    assert_eq!(value["report"]["metadata"]["role"], Value::from("platform"));
    assert!(value["report"]["metadata"]["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry.as_str() == Some("cursor")));
    assert!(value["checks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["passed"].as_bool() == Some(true)));
    assert!(value["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"].as_str() == Some("require_hosts")));
}

#[test]
fn test_registry_sync_mirrors_entries_from_another_registry() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles_dir = root.join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();

    let git = command_json(&["inspect", "cli", "git", "--pretty"]);
    fs::write(
        profiles_dir.join("git.json"),
        serde_json::to_string_pretty(&git).unwrap(),
    )
    .unwrap();

    let bundle_path = root.join("sync.bundle.json");
    command_json(&[
        "inspect",
        "bundle-export",
        "--root",
        root.to_str().unwrap(),
        "--bundle-name",
        "Sync Bundle",
        "--output",
        bundle_path.to_str().unwrap(),
        "--pretty",
    ]);

    let source_registry = root.join("source-registry");
    command_json(&[
        "inspect",
        "registry-init",
        source_registry.to_str().unwrap(),
        "--pretty",
    ]);
    command_json(&[
        "inspect",
        "registry-add",
        bundle_path.to_str().unwrap(),
        "--registry",
        source_registry.to_str().unwrap(),
        "--pretty",
    ]);

    let target_registry = root.join("target-registry");
    let sync = command_json(&[
        "inspect",
        "registry-sync",
        source_registry.to_str().unwrap(),
        "--registry",
        target_registry.to_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(sync["imported_count"], Value::from(1));
    assert_eq!(sync["error_count"], Value::from(0));

    let target = command_json(&[
        "inspect",
        "registry-list",
        target_registry.to_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(target["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn test_registry_serve_and_push_support_remote_registry_flow() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles_dir = root.join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();

    let git = command_json(&["inspect", "cli", "git", "--pretty"]);
    fs::write(
        profiles_dir.join("git.json"),
        serde_json::to_string_pretty(&git).unwrap(),
    )
    .unwrap();

    let bundle_path = root.join("remote-registry.bundle.json");
    command_json(&[
        "inspect",
        "bundle-export",
        "--root",
        root.to_str().unwrap(),
        "--bundle-name",
        "Remote Bundle",
        "--output",
        bundle_path.to_str().unwrap(),
        "--pretty",
    ]);

    let registry_dir = root.join("served-registry");
    let (mut child, port) = spawn_registry_http_server(&registry_dir, &[]);
    let index_url = format!("http://127.0.0.1:{port}/index.json");
    let base_url = format!("http://127.0.0.1:{port}");

    let push = command_json(&[
        "inspect",
        "registry-push",
        bundle_path.to_str().unwrap(),
        "--registry",
        &base_url,
        "--pretty",
    ]);
    assert_eq!(push["transport"], Value::from("http"));
    assert_eq!(
        push["result"]["result"]["entry"]["name"],
        Value::from("Remote Bundle")
    );

    let sync_target = root.join("synced-registry");
    let sync = command_json(&[
        "inspect",
        "registry-sync",
        &index_url,
        "--registry",
        sync_target.to_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(sync["imported_count"], Value::from(1));
    assert_eq!(sync["error_count"], Value::from(0));

    let pulled_dir = root.join("remote-registry-pulled");
    let pulled = command_json(&[
        "inspect",
        "registry-pull",
        "Remote Bundle",
        "--registry",
        sync_target.to_str().unwrap(),
        "--output-dir",
        pulled_dir.to_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(pulled["import"]["imported_count"], Value::from(1));
    assert!(pulled_dir.join("git.json").exists());

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_known_good_prefers_current_bundle_candidate_with_rank_details() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles_dir = root.join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();

    let mut git = command_json(&["inspect", "cli", "git", "--pretty"]);
    git["summary"] = Value::from("the stupid content tracker");
    fs::write(
        profiles_dir.join("git.json"),
        serde_json::to_string_pretty(&git).unwrap(),
    )
    .unwrap();

    let bundle_path = root.join("known-good.bundle.json");
    command_json(&[
        "inspect",
        "bundle-export",
        "--root",
        root.to_str().unwrap(),
        "--bundle-name",
        "Known Good Bundle",
        "--output",
        bundle_path.to_str().unwrap(),
        "--pretty",
    ]);

    let known_good = command_json(&[
        "inspect",
        "known-good",
        bundle_path.to_str().unwrap(),
        "--command",
        "git",
        "--pretty",
    ]);
    assert_eq!(known_good["command"], Value::from("git"));
    assert!(known_good["candidate_count"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(known_good["selected"]["command"], Value::from("git"));
    assert!(known_good["selected"]["rank_score"].as_i64().unwrap_or(0) > 0);
    assert_eq!(
        known_good["selected"]["quality"]["ready_for_agent_docs"],
        Value::Bool(true)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_publish_and_pull_support_http_bundle_endpoints() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles_dir = root.join(".sxmc").join("ai").join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();

    let git = command_json(&["inspect", "cli", "git", "--pretty"]);
    fs::write(
        profiles_dir.join("git.json"),
        serde_json::to_string_pretty(&git).unwrap(),
    )
    .unwrap();

    let stored_bundle = Arc::new(Mutex::new(None::<Value>));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app =
        Router::new()
            .route(
                "/bundle",
                put(
                    |State(stored): State<Arc<Mutex<Option<Value>>>>,
                     Json(payload): Json<Value>| async move {
                        *stored.lock().unwrap() = Some(payload);
                        Json(serde_json::json!({"ok": true}))
                    },
                )
                .get(
                    |State(stored): State<Arc<Mutex<Option<Value>>>>| async move {
                        Json(stored.lock().unwrap().clone().unwrap_or_else(|| {
                            serde_json::json!({
                                "bundle_schema": "sxmc_profile_bundle_v1",
                                "profiles": []
                            })
                        }))
                    },
                ),
            )
            .with_state(Arc::clone(&stored_bundle));
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let url = format!("http://{addr}/bundle");
    let publish = command_json(&[
        "publish",
        &url,
        "--root",
        root.to_str().unwrap(),
        "--bundle-name",
        "Remote Bundle",
        "--pretty",
    ]);
    assert_eq!(publish["transport"], Value::from("http"));
    assert_eq!(publish["profile_count"], Value::from(1));
    assert!(publish["sha256"].as_str().unwrap_or_default().len() == 64);
    assert_eq!(
        stored_bundle.lock().unwrap().as_ref().unwrap()["metadata"]["name"],
        Value::from("Remote Bundle")
    );

    let pull_dir = root.join("remote-pulled-profiles");
    let pull = command_json(&[
        "pull",
        &url,
        "--root",
        root.to_str().unwrap(),
        "--output-dir",
        pull_dir.to_str().unwrap(),
        "--expected-sha256",
        publish["sha256"].as_str().unwrap(),
        "--pretty",
    ]);
    assert_eq!(pull["imported_count"], Value::from(1));
    assert_eq!(pull["sha256"], publish["sha256"]);
    assert!(pull_dir.join("git.json").exists());

    handle.abort();
}

#[test]
fn test_inspect_batch_from_file_loads_commands() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tools.txt");
    fs::write(&path, "cargo\n# comment\n   \n git \n  # spaced comment\n").unwrap();

    let value = command_json(&[
        "inspect",
        "batch",
        "--from-file",
        path.to_str().unwrap(),
        "--compact",
    ]);
    assert_eq!(value["count"], Value::from(2));
    assert_eq!(value["failed_count"], Value::from(0));
}

#[test]
#[cfg(not(windows))]
fn test_inspect_batch_from_yaml_supports_depth_overrides() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_nested_cli(temp.path());
    let path = temp.path().join("tools.yaml");
    fs::write(
        &path,
        format!(
            "tools:\n  - command: \"{}\"\n    depth: 1\n",
            fake.to_string_lossy()
        ),
    )
    .unwrap();

    let value = command_json(&["inspect", "batch", "--from-file", path.to_str().unwrap()]);
    assert_eq!(value["count"], Value::from(1));
    let profiles = value["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 1);
    assert!(!profiles[0]["subcommand_profiles"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn test_inspect_batch_from_toml_supports_structured_tools() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tools.toml");
    fs::write(
        &path,
        "tools = [\n  { command = \"cargo\", depth = 1 },\n  { command = \"git\" }\n]\n",
    )
    .unwrap();

    let value = command_json(&[
        "inspect",
        "batch",
        "--from-file",
        path.to_str().unwrap(),
        "--compact",
    ]);
    assert_eq!(value["count"], Value::from(2));
    assert_eq!(value["failed_count"], Value::from(0));
}

#[test]
fn test_inspect_batch_since_accepts_rfc3339_timestamp() {
    let value = command_json(&[
        "inspect",
        "batch",
        "cargo",
        "--since",
        "1970-01-01T00:00:00Z",
    ]);
    assert_eq!(value["count"], Value::from(1));
    assert_eq!(value["inspected_count"], Value::from(1));
}

#[test]
fn test_inspect_batch_toon_is_summary_oriented() {
    sxmc()
        .args(["inspect", "batch", "cargo", "--format", "toon"])
        .assert()
        .success()
        .stdout(predicate::str::contains("profiles:"))
        .stdout(predicate::str::contains("- cargo:"))
        .stdout(predicate::str::contains("parallelism:"));
}

#[test]
fn test_inspect_batch_output_dir_writes_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let output_dir = temp.path().join("profiles");

    let value = command_json(&[
        "inspect",
        "batch",
        "cargo",
        "git",
        "--output-dir",
        output_dir.to_str().unwrap(),
    ]);
    assert_eq!(value["written_profile_count"], Value::from(2));
    assert_eq!(
        value["output_dir"],
        Value::from(output_dir.display().to_string())
    );
    assert!(output_dir.join("cargo.json").exists());
    assert!(output_dir.join("git.json").exists());
    assert!(output_dir.join("batch-summary.json").exists());
    assert_eq!(
        value["written_manifest_path"],
        Value::from(output_dir.join("batch-summary.json").display().to_string())
    );
}

#[test]
fn test_inspect_batch_output_dir_skip_existing_preserves_existing_file() {
    let temp = tempfile::tempdir().unwrap();
    let output_dir = temp.path().join("profiles");
    fs::create_dir_all(&output_dir).unwrap();
    let existing = output_dir.join("cargo.json");
    fs::write(&existing, "{\"sentinel\":true}\n").unwrap();

    let value = command_json(&[
        "inspect",
        "batch",
        "cargo",
        "--output-dir",
        output_dir.to_str().unwrap(),
        "--skip-existing",
    ]);
    assert_eq!(value["written_profile_count"], Value::from(0));
    assert_eq!(value["skipped_existing_count"], Value::from(1));
    assert_eq!(
        fs::read_to_string(existing).unwrap(),
        "{\"sentinel\":true}\n"
    );
}

#[test]
fn test_inspect_batch_retry_failed_loads_commands_from_saved_batch_json() {
    let temp = tempfile::tempdir().unwrap();
    let batch_path = temp.path().join("batch.json");
    let batch = command_json(&[
        "inspect",
        "batch",
        "cargo",
        "this-command-should-not-exist-xyz",
    ]);
    fs::write(&batch_path, serde_json::to_string_pretty(&batch).unwrap()).unwrap();

    let value = command_json(&[
        "inspect",
        "batch",
        "--retry-failed",
        batch_path.to_str().unwrap(),
    ]);
    assert_eq!(value["count"], Value::from(1));
    assert_eq!(value["failed_count"], Value::from(1));
    assert_eq!(
        value["failures"][0]["command"],
        Value::from("this-command-should-not-exist-xyz")
    );
}

#[test]
fn test_inspect_batch_ndjson_streams_events_and_summary() {
    let output = sxmc()
        .args([
            "inspect",
            "batch",
            "cargo",
            "this-command-should-not-exist-xyz",
            "--format",
            "ndjson",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    assert!(lines.len() >= 3);
    let first: Value = serde_json::from_str(lines[0]).unwrap();
    let last: Value = serde_json::from_str(lines.last().unwrap()).unwrap();
    assert!(matches!(
        first["type"].as_str().unwrap_or_default(),
        "profile" | "failure" | "skipped"
    ));
    assert_eq!(last["type"], Value::from("summary"));
}

#[test]
fn test_inspect_batch_toon_includes_failure_details() {
    sxmc()
        .args([
            "inspect",
            "batch",
            "cargo",
            "this-command-should-not-exist-xyz",
            "--format",
            "toon",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("failures:"))
        .stdout(predicate::str::contains(
            "this-command-should-not-exist-xyz",
        ));
}

#[test]
#[cfg(not(windows))]
fn test_inspect_batch_since_skips_unchanged_tools() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_cli(
        temp.path(),
        "fake-cli\n\nSummary.\n\nUsage:\n  fake-cli [OPTIONS]\n",
    );
    let future = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;

    let value = command_json(&[
        "inspect",
        "batch",
        fake.to_str().unwrap(),
        "--since",
        &future.to_string(),
    ]);
    assert_eq!(value["count"], Value::from(1));
    assert_eq!(value["inspected_count"], Value::from(0));
    assert_eq!(value["skipped_count"], Value::from(1));
    assert_eq!(value["failed_count"], Value::from(0));
}

#[test]
fn test_inspect_cache_invalidate_and_clear() {
    let temp = tempfile::tempdir().unwrap();
    let _ = command_json_with_config_home(temp.path(), &["inspect", "cli", "cargo", "--compact"]);
    let _ = command_json_with_config_home(temp.path(), &["inspect", "cli", "git", "--compact"]);

    let before = command_json_with_config_home(temp.path(), &["inspect", "cache-stats"]);
    let before_entries = before["entry_count"].as_u64().unwrap_or(0);
    assert!(before_entries >= 2);

    let invalidate =
        command_json_with_config_home(temp.path(), &["inspect", "cache-invalidate", "cargo"]);
    assert_eq!(invalidate["command"], "cargo");
    assert_eq!(invalidate["match_mode"], "exact");
    assert!(invalidate["removed_entries"].as_u64().unwrap_or(0) >= 1);
    assert!(invalidate["remaining_entries"].as_u64().unwrap_or(0) >= 1);

    let _ = command_json_with_config_home(temp.path(), &["inspect", "cli", "cargo", "--compact"]);
    let dry_run = command_json_with_config_home(
        temp.path(),
        &["inspect", "cache-invalidate", "c*", "--dry-run"],
    );
    assert_eq!(dry_run["dry_run"], Value::Bool(true));
    assert_eq!(dry_run["match_mode"], "glob");
    assert!(dry_run["matched_entries"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(dry_run["removed_entries"], Value::from(0));

    let _ = command_json_with_config_home(temp.path(), &["inspect", "cli", "cargo", "--compact"]);
    let wildcard =
        command_json_with_config_home(temp.path(), &["inspect", "cache-invalidate", "g*"]);
    assert_eq!(wildcard["match_mode"], "glob");
    assert!(wildcard["removed_entries"].as_u64().unwrap_or(0) >= 1);

    let cleared = command_json_with_config_home(temp.path(), &["inspect", "cache-clear"]);
    assert_eq!(cleared["cleared"], Value::Bool(true));
    assert_eq!(cleared["entry_count"], Value::from(0));
}

#[test]
fn test_inspect_cache_warm_returns_summary() {
    let value = command_json(&["inspect", "cache-warm", "cargo", "git", "--parallel", "2"]);
    assert_eq!(value["count"], Value::from(2));
    assert!(value["warmed_count"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(value["failed_count"], Value::from(0));
}

#[test]
fn test_doctor_check_exits_non_zero_when_startup_files_missing() {
    let temp = tempfile::tempdir().unwrap();
    sxmc()
        .args(["doctor", "--check", "--root", temp.path().to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
#[cfg(not(windows))]
fn test_doctor_fix_repairs_selected_hosts() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_cli(
        temp.path(),
        "fake-cli\n\nA repairable CLI.\n\nUsage:\n  fake-cli [OPTIONS]\n\nOptions:\n  --json  Emit json\n",
    );

    sxmc()
        .args([
            "doctor",
            "--check",
            "--fix",
            "--allow-low-confidence",
            "--only",
            "claude-code,cursor",
            "--from-cli",
            fake.to_str().unwrap(),
            "--root",
            temp.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(temp.path().join("CLAUDE.md").exists());
    assert!(temp
        .path()
        .join(".cursor")
        .join("rules")
        .join("sxmc-cli-ai.md")
        .exists());

    let rerun = sxmc()
        .args([
            "doctor",
            "--check",
            "--fix",
            "--allow-low-confidence",
            "--only",
            "claude-code,cursor",
            "--from-cli",
            fake.to_str().unwrap(),
            "--root",
            temp.path().to_str().unwrap(),
            "--human",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let rerun_stdout = String::from_utf8_lossy(&rerun);
    assert!(rerun_stdout.contains("Skipped unchanged"));
    assert!(rerun_stdout.contains("Summary:"));
}

#[test]
#[cfg(not(windows))]
fn test_doctor_fix_dry_run_does_not_write_files() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_cli(
        temp.path(),
        "fake-cli\n\nA repairable CLI.\n\nUsage:\n  fake-cli [OPTIONS]\n\nOptions:\n  --json  Emit json\n",
    );

    sxmc()
        .args([
            "doctor",
            "--check",
            "--fix",
            "--dry-run",
            "--allow-low-confidence",
            "--only",
            "claude-code",
            "--from-cli",
            fake.to_str().unwrap(),
            "--root",
            temp.path().to_str().unwrap(),
            "--human",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("Created"))
        .stdout(predicate::str::contains("Summary:"));

    assert!(!temp.path().join("CLAUDE.md").exists());
}

#[test]
#[cfg(not(windows))]
fn test_doctor_remove_cleans_selected_hosts() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_cli(
        temp.path(),
        "fake-cli\n\nA repairable CLI.\n\nUsage:\n  fake-cli [OPTIONS]\n\nOptions:\n  --json  Emit json\n",
    );

    sxmc()
        .args([
            "doctor",
            "--check",
            "--fix",
            "--allow-low-confidence",
            "--only",
            "claude-code",
            "--from-cli",
            fake.to_str().unwrap(),
            "--root",
            temp.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(temp.path().join("CLAUDE.md").exists());

    sxmc()
        .args([
            "doctor",
            "--remove",
            "--only",
            "claude-code",
            "--from-cli",
            fake.to_str().unwrap(),
            "--root",
            temp.path().to_str().unwrap(),
            "--human",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"))
        .stdout(predicate::str::contains("Summary:"));
}

#[test]
fn test_doctor_check_only_hosts_limits_scope() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".cursor").join("rules")).unwrap();
    fs::create_dir_all(temp.path().join(".sxmc").join("ai")).unwrap();
    fs::write(temp.path().join("CLAUDE.md"), "# Claude\n").unwrap();
    fs::write(
        temp.path()
            .join(".sxmc")
            .join("ai")
            .join("claude-code-mcp.json"),
        "{\"mcpServers\":{}}\n",
    )
    .unwrap();
    fs::write(
        temp.path()
            .join(".cursor")
            .join("rules")
            .join("sxmc-cli-ai.md"),
        "# Cursor\n",
    )
    .unwrap();
    fs::write(
        temp.path().join(".cursor").join("mcp.json"),
        "{\"mcpServers\":{}}\n",
    )
    .unwrap();

    sxmc_with_config_home(temp.path())
        .args([
            "doctor",
            "--check",
            "--only",
            "claude-code,cursor",
            "--root",
            temp.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let value = command_json_with_config_home(
        temp.path(),
        &[
            "doctor",
            "--root",
            temp.path().to_str().unwrap(),
            "--only",
            "claude-code,cursor",
        ],
    );
    assert_eq!(
        value["checked_hosts"],
        Value::from(vec!["claude-code", "cursor"])
    );
    let startup_files = value["startup_files"].as_object().unwrap();
    assert!(startup_files.contains_key("claude_code"));
    assert!(startup_files.contains_key("cursor_rules"));
    assert!(startup_files.contains_key("cursor_mcp"));
    assert!(!startup_files.contains_key("github_copilot"));
}

#[test]
#[cfg(not(windows))]
fn test_inspect_diff_reports_changes_against_saved_profile() {
    let temp = tempfile::tempdir().unwrap();
    let fake = write_fake_cli(
        temp.path(),
        "fake-cli\n\nA first summary.\n\nUsage:\n  fake-cli [OPTIONS]\n",
    );
    let before = command_stdout(&["inspect", "cli", fake.to_str().unwrap(), "--pretty"]);
    let before_path = temp.path().join("before.json");
    fs::write(&before_path, before).unwrap();

    std::thread::sleep(Duration::from_millis(1100));
    fs::write(
        &fake,
        "#!/bin/sh\ncat <<'EOF'\nfake-cli\n\nA second summary.\n\nUsage:\n  fake-cli [OPTIONS]\n\nOptions:\n  --json  Emit json.\nEOF\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&fake).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake, perms).unwrap();

    let value = command_json(&[
        "inspect",
        "diff",
        fake.to_str().unwrap(),
        "--before",
        before_path.to_str().unwrap(),
    ]);
    assert_eq!(value["summary_changed"], Value::Bool(true));
    assert!(value["options_added"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "--json"));
}

#[test]
fn test_inspect_diff_can_compare_two_saved_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let before = command_json(&["inspect", "cli", "cargo", "--format", "json-pretty"]);
    let mut after = before.clone();
    after["summary"] = Value::from("A changed cargo summary");

    let before_path = temp.path().join("before.json");
    let after_path = temp.path().join("after.json");
    fs::write(&before_path, serde_json::to_string_pretty(&before).unwrap()).unwrap();
    fs::write(&after_path, serde_json::to_string_pretty(&after).unwrap()).unwrap();

    let value = command_json(&[
        "inspect",
        "diff",
        "--before",
        before_path.to_str().unwrap(),
        "--after",
        after_path.to_str().unwrap(),
    ]);
    assert_eq!(value["summary_changed"], Value::Bool(true));
    assert_eq!(
        value["after_summary"],
        Value::from("A changed cargo summary")
    );
}

#[test]
fn test_inspect_diff_rejects_compact_profiles_with_specific_guidance() {
    let temp = tempfile::tempdir().unwrap();
    let before = command_stdout(&["inspect", "cli", "cargo", "--compact"]);
    let before_path = temp.path().join("before-compact.json");
    fs::write(&before_path, before).unwrap();

    sxmc()
        .args([
            "inspect",
            "diff",
            "cargo",
            "--before",
            before_path.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Compact profiles cannot be diffed",
        ))
        .stderr(predicate::str::contains("without `--compact`"));
}

#[test]
fn test_inspect_diff_toon_is_human_oriented() {
    let temp = tempfile::tempdir().unwrap();
    let before = command_stdout(&["inspect", "cli", "cargo", "--pretty"]);
    let before_path = temp.path().join("before.json");
    fs::write(&before_path, before).unwrap();

    sxmc()
        .args([
            "inspect",
            "diff",
            "cargo",
            "--before",
            before_path.to_str().unwrap(),
            "--format",
            "toon",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("command: cargo"))
        .stdout(predicate::str::contains("summary_changed:"));
}

#[test]
fn test_inspect_diff_toon_includes_removed_deltas() {
    let temp = tempfile::tempdir().unwrap();
    let before = command_json(&[
        "inspect",
        "cli",
        &sxmc_bin_string(),
        "--allow-self",
        "--format",
        "json-pretty",
    ]);
    let mut after = before.clone();
    let subcommands = after["subcommands"].as_array_mut().unwrap();
    assert!(!subcommands.is_empty());
    subcommands.remove(0);
    let options = after["options"].as_array_mut().unwrap();
    assert!(!options.is_empty());
    options.remove(0);

    let before_path = temp.path().join("before.json");
    let after_path = temp.path().join("after.json");
    fs::write(&before_path, serde_json::to_string_pretty(&before).unwrap()).unwrap();
    fs::write(&after_path, serde_json::to_string_pretty(&after).unwrap()).unwrap();

    sxmc()
        .args([
            "inspect",
            "diff",
            "--before",
            before_path.to_str().unwrap(),
            "--after",
            after_path.to_str().unwrap(),
            "--format",
            "toon",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("subcommands_removed:"))
        .stdout(predicate::str::contains("options_removed:"));
}

#[test]
fn test_inspect_diff_tolerates_missing_legacy_fields_in_saved_profile() {
    let temp = tempfile::tempdir().unwrap();
    let mut before = command_json(&["inspect", "cli", "cargo", "--format", "json-pretty"]);

    if let Some(subcommands) = before["subcommands"].as_array_mut() {
        if let Some(first) = subcommands.first_mut() {
            first.as_object_mut().unwrap().remove("confidence");
        }
    }
    if let Some(options) = before["options"].as_array_mut() {
        if let Some(first) = options.first_mut() {
            first.as_object_mut().unwrap().remove("confidence");
        }
    }
    before["provenance"]
        .as_object_mut()
        .unwrap()
        .remove("generated_at");

    let before_path = temp.path().join("before-legacyish.json");
    fs::write(&before_path, serde_json::to_string_pretty(&before).unwrap()).unwrap();

    sxmc()
        .args([
            "inspect",
            "diff",
            "cargo",
            "--before",
            before_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"summary_changed\""));
}

#[test]
fn test_inspect_diff_reports_migration_note_for_version_mismatch() {
    let temp = tempfile::tempdir().unwrap();
    let mut before = command_json(&["inspect", "cli", "cargo", "--format", "json-pretty"]);
    before["provenance"]["generator_version"] = Value::from("0.1.0");
    let before_path = temp.path().join("before-old-version.json");
    fs::write(&before_path, serde_json::to_string_pretty(&before).unwrap()).unwrap();

    let value = command_json(&[
        "inspect",
        "diff",
        "cargo",
        "--before",
        before_path.to_str().unwrap(),
    ]);
    assert!(value["migration_note"]
        .as_str()
        .unwrap_or_default()
        .contains("generated by sxmc 0.1.0"));
}

#[test]
fn test_inspect_diff_markdown_is_human_readable() {
    let temp = tempfile::tempdir().unwrap();
    let mut before = command_json(&["inspect", "cli", "cargo", "--format", "json-pretty"]);
    before["summary"] = Value::from("An older cargo summary");
    let before_path = temp.path().join("before-old.json");
    fs::write(&before_path, serde_json::to_string_pretty(&before).unwrap()).unwrap();

    sxmc()
        .args([
            "inspect",
            "diff",
            "cargo",
            "--before",
            before_path.to_str().unwrap(),
            "--format",
            "markdown",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("# CLI Diff: `cargo`"))
        .stdout(predicate::str::contains("Summary changed: `true`"));
}

#[test]
fn test_inspect_diff_exit_code_fails_when_changed() {
    let temp = tempfile::tempdir().unwrap();
    let mut before = command_json(&["inspect", "cli", "cargo", "--format", "json-pretty"]);
    before["summary"] = Value::from("An older cargo summary");
    let before_path = temp.path().join("before-old.json");
    fs::write(&before_path, serde_json::to_string_pretty(&before).unwrap()).unwrap();

    sxmc()
        .args([
            "inspect",
            "diff",
            "cargo",
            "--before",
            before_path.to_str().unwrap(),
            "--exit-code",
        ])
        .assert()
        .failure();
}

#[test]
fn test_inspect_diff_exit_code_succeeds_when_identical() {
    let temp = tempfile::tempdir().unwrap();
    let before = command_stdout(&["inspect", "cli", "cargo", "--pretty"]);
    let before_path = temp.path().join("before.json");
    fs::write(&before_path, before).unwrap();

    sxmc()
        .args([
            "inspect",
            "diff",
            "cargo",
            "--before",
            before_path.to_str().unwrap(),
            "--exit-code",
        ])
        .assert()
        .success();
}

#[test]
#[cfg(not(windows))]
fn test_inspect_diff_watch_flushes_first_frame_for_piped_stdout() {
    let temp = tempfile::tempdir().unwrap();
    let before = command_stdout(&["inspect", "cli", "cargo", "--pretty"]);
    let before_path = temp.path().join("before.json");
    fs::write(&before_path, before).unwrap();

    let mut child = ProcessCommand::new(sxmc_bin_string())
        .args([
            "inspect",
            "diff",
            "cargo",
            "--before",
            before_path.to_str().unwrap(),
            "--watch",
            "3",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let _ = sender.send(line);
    });

    let first_line = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("timed out waiting for watch output");
    child.kill().ok();
    let _ = child.wait();

    assert!(first_line.contains("\"summary_changed\""));
}

#[test]
#[cfg(not(windows))]
fn test_inspect_diff_watch_ndjson_flushes_first_frame_for_piped_stdout() {
    let temp = tempfile::tempdir().unwrap();
    let before = command_stdout(&["inspect", "cli", "cargo", "--pretty"]);
    let before_path = temp.path().join("before.json");
    fs::write(&before_path, before).unwrap();

    let mut child = ProcessCommand::new(sxmc_bin_string())
        .args([
            "inspect",
            "diff",
            "cargo",
            "--before",
            before_path.to_str().unwrap(),
            "--watch",
            "3",
            "--format",
            "ndjson",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let _ = sender.send(line);
    });

    let first_line = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("timed out waiting for watch ndjson output");
    child.kill().ok();
    let _ = child.wait();

    assert!(first_line.contains("\"summary_changed\""));
    let parsed: Value = serde_json::from_str(first_line.trim()).unwrap();
    assert!(parsed.get("command").is_some());
}

#[test]
#[cfg(not(windows))]
fn test_watch_flushes_first_frame_for_piped_stdout() {
    let temp = tempfile::tempdir().unwrap();
    let mut child = ProcessCommand::new(sxmc_bin_string())
        .args([
            "watch",
            "--root",
            temp.path().to_str().unwrap(),
            "--interval-seconds",
            "3",
            "--format",
            "ndjson",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let _ = sender.send(line);
    });

    let first_line = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("timed out waiting for watch output");
    child.kill().ok();
    let _ = child.wait();

    assert!(first_line.contains("\"saved_profiles\""));
    let parsed: Value = serde_json::from_str(first_line.trim()).unwrap();
    assert!(parsed.get("root").is_some());
}

#[test]
fn test_inspect_cli_compact_output_reduces_profile_shape() {
    let value = command_json(&[
        "inspect",
        "cli",
        &sxmc_bin_string(),
        "--allow-self",
        "--compact",
    ]);
    assert_eq!(value["command"], "sxmc");
    assert!(value["subcommand_count"].as_u64().unwrap_or(0) >= 3);
    assert!(value["option_count"].as_u64().unwrap_or(0) >= 1);
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
        .args([
            "inspect",
            "cli",
            &sxmc_bin_string(),
            "--allow-self",
            "--depth",
            "1",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let nested = value["subcommand_profiles"].as_array().unwrap();
    assert!(!nested.is_empty());
    assert!(nested.iter().any(|profile| profile["command"] == "serve"));
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

    let source = serde_json::to_string(&vec![
        sxmc_bin_string(),
        "serve".to_string(),
        "--paths".to_string(),
        ".".to_string(),
    ])
    .unwrap();

    sxmc_with_config_home(temp.path())
        .args([
            "bake",
            "create",
            "relative-stdio",
            "--type",
            "stdio",
            "--source",
            &source,
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

#[cfg(not(windows))]
#[test]
fn test_skills_run_executes_single_script_with_args() {
    sxmc()
        .args([
            "skills",
            "run",
            "skill-with-scripts",
            "--paths",
            "tests/fixtures",
            "--",
            "alpha",
            "beta",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Hello from script! Args: alpha beta",
        ));
}

#[cfg(not(windows))]
#[test]
fn test_skills_run_passes_env_vars_to_script() {
    let temp = tempfile::tempdir().unwrap();
    let skill_dir = temp.path().join("env-skill");
    fs::create_dir_all(skill_dir.join("scripts")).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: env-skill\ndescription: env skill\n---\nBody output.\n",
    )
    .unwrap();
    let script_path = skill_dir.join("scripts").join("show-env.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nprintf 'env=%s\\n' \"$GREETING\"\nprintf 'sxmc=%s\\n' \"$SXMC_SKILL_NAME\"\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();

    sxmc()
        .args([
            "skills",
            "run",
            "env-skill",
            "--paths",
            temp.path().to_str().unwrap(),
            "--env",
            "GREETING=hello",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("env=hello"))
        .stdout(predicate::str::contains("sxmc=env-skill"));
}

#[cfg(not(windows))]
#[test]
fn test_skills_run_can_print_body_for_script_skills() {
    sxmc()
        .args([
            "skills",
            "run",
            "skill-with-scripts",
            "--paths",
            "tests/fixtures",
            "--print-body",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("This skill has tools available."));
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
fn test_inspect_migrate_profile_writes_canonical_output() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("legacyish.json");
    let output = temp.path().join("migrated.json");
    let mut profile = command_json(&[
        "inspect",
        "cli",
        &sxmc_bin_string(),
        "--allow-self",
        "--format",
        "json-pretty",
    ]);
    let options = profile["options"].as_array_mut().unwrap();
    assert!(!options.is_empty());
    options[0].as_object_mut().unwrap().remove("confidence");
    profile["provenance"]
        .as_object_mut()
        .unwrap()
        .remove("generated_at");
    fs::write(&input, serde_json::to_string_pretty(&profile).unwrap()).unwrap();

    let report = command_json(&[
        "inspect",
        "migrate-profile",
        input.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ]);
    assert_eq!(report["output"], Value::from(output.display().to_string()));
    assert!(output.exists());

    let migrated: Value = serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
    assert_eq!(
        migrated["profile_schema"],
        Value::from("sxmc_cli_surface_profile_v1")
    );
    assert_eq!(migrated["command"], Value::from("sxmc"));
    sxmc()
        .args(["inspect", "profile", output.to_str().unwrap(), "--pretty"])
        .assert()
        .success();
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
    // On Windows/MINGW, git --help output may not expose standalone options
    if !cfg!(windows) {
        assert!(!options.is_empty());
        assert!(options.iter().any(|entry| entry["name"] == "--version"));
    }
}

#[test]
fn test_inspect_cli_primary_subcommand_names_avoid_alias_pairs() {
    let profile = command_json(&[
        "inspect",
        "cli",
        &sxmc_bin_string(),
        "--allow-self",
        "--pretty",
    ]);
    let subcommands = profile["subcommands"].as_array().unwrap();
    assert!(subcommands.iter().any(|entry| entry["name"] == "serve"));
    assert!(!subcommands
        .iter()
        .any(|entry| { entry["name"].as_str().unwrap_or_default().contains(',') }));
}

#[test]
fn test_inspect_cli_node_avoids_option_shaped_subcommands() {
    let output = ProcessCommand::new(sxmc_bin_string())
        .args(["inspect", "cli", "node", "--pretty"])
        .output()
        .unwrap();
    if !output.status.success() {
        eprintln!("skipping: node help output could not be parsed on this platform");
        return;
    }
    let profile: Value = serde_json::from_slice(&output.stdout).unwrap();
    let subcommands = profile["subcommands"].as_array().unwrap();
    assert!(subcommands.iter().any(|entry| entry["name"] == "inspect"));
    assert!(!subcommands
        .iter()
        .any(|entry| { entry["name"].as_str().unwrap_or_default().starts_with("--") }));
    let summary = profile["summary"].as_str().unwrap_or_default();
    assert!(!summary.contains("interactive mode"));
    assert!(
        summary.contains("JavaScript") || summary.contains("runtime") || summary.contains("node"),
        "unexpected node summary: {summary}"
    );
}

#[test]
fn test_inspect_cli_gh_recovers_top_level_flags() {
    if !has_command("gh") {
        eprintln!("skipping: gh not installed");
        return;
    }
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
    let output = ProcessCommand::new(sxmc_bin_string())
        .args(["inspect", "cli", "python3", "--pretty"])
        .output()
        .unwrap();
    if !output.status.success() {
        eprintln!("skipping: python3 help output could not be parsed on this platform");
        return;
    }
    let profile: Value = serde_json::from_slice(&output.stdout).unwrap();
    let summary = profile["summary"].as_str().unwrap_or_default();
    // On Windows, python3 may resolve to a stub; skip if summary looks wrong
    if summary.contains("not found") || summary.contains("Microsoft Store") {
        eprintln!("skipping: python3 resolves to Windows Store stub");
        return;
    }
    assert!(!summary.is_empty());
    assert!(!summary.to_ascii_lowercase().starts_with("usage:"));
    assert!(summary.contains("Python") || summary.contains("language"));
    let subcommands = profile["subcommands"].as_array().unwrap();
    assert!(!subcommands.iter().any(|entry| {
        entry["name"]
            .as_str()
            .unwrap_or_default()
            .starts_with("PYTHON")
    }));
    let options = profile["options"].as_array().unwrap();
    assert!(options.iter().any(|entry| {
        matches!(
            entry["name"].as_str().unwrap_or_default(),
            "--help-all" | "--help" | "-h"
        )
    }));
}

#[test]
fn test_inspect_cli_npm_uses_better_summary_and_usage_options() {
    let output = ProcessCommand::new(sxmc_bin_string())
        .args(["inspect", "cli", "npm", "--pretty"])
        .output()
        .unwrap();
    if !output.status.success() {
        eprintln!("skipping: npm help output could not be parsed on this platform");
        return;
    }
    let profile: Value = serde_json::from_slice(&output.stdout).unwrap();
    let summary = profile["summary"].as_str().unwrap_or_default();
    assert!(!summary.is_empty());
    assert!(!summary.to_ascii_lowercase().starts_with("usage:"));
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
    if !has_command("gh") {
        eprintln!("skipping: gh not installed");
        return;
    }
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
fn test_scaffold_ci_apply_writes_github_actions_workflow() {
    let temp = tempfile::tempdir().unwrap();

    sxmc()
        .args([
            "scaffold",
            "ci",
            "--from-profile",
            "examples/profiles/from_cli.json",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
        ])
        .assert()
        .success();

    let workflow_path = temp.path().join(".github/workflows/sxmc-drift-gh.yml");
    let contents = fs::read_to_string(&workflow_path).unwrap();
    assert!(contents.contains("name: sxmc drift (gh)"));
    assert!(contents.contains("sxmc inspect diff gh"));
    assert!(contents.contains("--exit-code"));
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
            &sxmc_bin_string(),
            "--client",
            "claude-code",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
            "--depth",
            "1",
            "--allow-self",
            "--allow-low-confidence",
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
            &sxmc_bin_string(),
            "--client",
            "claude-code",
            "--root",
            temp.path().to_str().unwrap(),
            "--mode",
            "apply",
            "--depth",
            "1",
            "--remove",
            "--allow-self",
            "--allow-low-confidence",
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
    let temp = tempfile::tempdir().unwrap();

    // Create
    sxmc_with_config_home(temp.path())
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
    sxmc_with_config_home(temp.path())
        .args(["bake", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-bake"));

    // Show
    sxmc_with_config_home(temp.path())
        .args(["bake", "show", "test-bake"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Name: test-bake"))
        .stdout(predicate::str::contains("Source: echo hello"));

    // Update
    sxmc_with_config_home(temp.path())
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

    sxmc_with_config_home(temp.path())
        .args(["bake", "show", "test-bake"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Source: echo updated"))
        .stdout(predicate::str::contains("Description: Updated bake config"));

    // Remove
    sxmc_with_config_home(temp.path())
        .args(["bake", "remove", "test-bake"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed bake: test-bake"));

    // Verify removed
    sxmc_with_config_home(temp.path())
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

#[test]
fn test_discover_cli_self_alias_emits_profile() {
    sxmc()
        .args([
            "discover",
            "cli",
            &sxmc_bin_string(),
            "--allow-self",
            "--pretty",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"profile_schema\""))
        .stdout(predicate::str::contains("\"command\": \"sxmc\""));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_discover_api_auto_detects_openapi_list() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let spec = serde_json::json!({
        "openapi": "3.0.0",
        "info": { "title": "Discover Pets API", "version": "1.0.0" },
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

    let base = format!("http://{addr}/openapi.json");
    let value = command_json(&["discover", "api", &base, "--list", "--format", "json"]);
    assert_eq!(value["api_type"], "OpenAPI");
    assert_eq!(value["count"], 1);
    assert_eq!(value["operations"][0]["name"], "listPets");

    handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_discover_graphql_local_list_and_call() {
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
                                        "description": "Root query type",
                                        "fields": [
                                            {
                                                "name": "hello",
                                                "description": "Say hello",
                                                "args": [],
                                                "type": { "kind": "SCALAR", "name": "String", "ofType": null }
                                            },
                                            {
                                                "name": "echo",
                                                "description": "Echo a message",
                                                "args": [
                                                    {
                                                        "name": "message",
                                                        "description": "Message to echo",
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
                                        "description": "Built-in string",
                                        "fields": null,
                                        "inputFields": null,
                                        "enumValues": null,
                                        "possibleTypes": null,
                                        "interfaces": null
                                    },
                                    {
                                        "kind": "INPUT_OBJECT",
                                        "name": "EchoInput",
                                        "description": "Echo input payload",
                                        "fields": null,
                                        "inputFields": [
                                            {
                                                "name": "message",
                                                "description": "Message to echo",
                                                "type": { "kind": "SCALAR", "name": "String", "ofType": null }
                                            }
                                        ],
                                        "enumValues": null,
                                        "possibleTypes": null,
                                        "interfaces": null
                                    },
                                    {
                                        "kind": "ENUM",
                                        "name": "Color",
                                        "description": "Example color enum",
                                        "fields": null,
                                        "inputFields": null,
                                        "enumValues": [
                                            { "name": "RED", "description": "Red" },
                                            { "name": "BLUE", "description": "Blue" }
                                        ],
                                        "possibleTypes": null,
                                        "interfaces": null
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
        .args(["discover", "graphql", &base, "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"))
        .stdout(predicate::str::contains("echo"));

    sxmc()
        .args([
            "discover",
            "graphql",
            &base,
            "echo",
            "message=hello",
            "--pretty",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"echo\": \"hello\""));

    let schema = command_json(&["discover", "graphql", &base, "--schema", "--format", "json"]);
    assert_eq!(schema["query_type"], "Query");
    assert!(schema["type_count"].as_u64().unwrap_or(0) >= 3);
    assert!(schema["types"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "Query" && entry["field_count"] == 2));

    let query_type = command_json(&[
        "discover", "graphql", &base, "--type", "Query", "--format", "json",
    ]);
    assert_eq!(query_type["name"], "Query");
    assert!(query_type["fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field["name"] == "echo" && field["arg_count"] == 1));

    let input_type = command_json(&[
        "discover",
        "graphql",
        &base,
        "--type",
        "EchoInput",
        "--format",
        "json",
    ]);
    assert_eq!(input_type["name"], "EchoInput");
    assert_eq!(input_type["input_field_count"], 1);

    let enum_type = command_json(&[
        "discover", "graphql", &base, "--type", "Color", "--format", "json",
    ]);
    assert_eq!(enum_type["enum_value_count"], 2);

    handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_discover_graphql_output_and_diff_report_changes() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let version = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let version_for_server = version.clone();

    let app = Router::new().route(
        "/graphql",
        post(move |Json(payload): Json<serde_json::Value>| {
            let version = version_for_server.clone();
            async move {
                let query = payload
                    .get("query")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let current = version.load(std::sync::atomic::Ordering::SeqCst);
                if query.contains("__schema") {
                    if current == 0 {
                        Json(serde_json::json!({
                            "data": {
                                "__schema": {
                                    "queryType": { "name": "Query" },
                                    "mutationType": null,
                                    "types": [
                                        {
                                            "kind": "OBJECT",
                                            "name": "Query",
                                            "description": "Root queries",
                                            "fields": [
                                                {
                                                    "name": "hello",
                                                    "description": "Say hello",
                                                    "args": [],
                                                    "type": { "kind": "SCALAR", "name": "String", "ofType": null }
                                                }
                                            ],
                                            "inputFields": null,
                                            "enumValues": null
                                        },
                                        {
                                            "kind": "SCALAR",
                                            "name": "String",
                                            "description": null,
                                            "fields": null,
                                            "inputFields": null,
                                            "enumValues": null
                                        }
                                    ],
                                    "directives": []
                                }
                            }
                        }))
                    } else {
                        Json(serde_json::json!({
                            "data": {
                                "__schema": {
                                    "queryType": { "name": "Query" },
                                    "mutationType": { "name": "Mutation" },
                                    "types": [
                                        {
                                            "kind": "OBJECT",
                                            "name": "Query",
                                            "description": "Root queries",
                                            "fields": [
                                                {
                                                    "name": "hello",
                                                    "description": "Say hello",
                                                    "args": [],
                                                    "type": { "kind": "SCALAR", "name": "String", "ofType": null }
                                                },
                                                {
                                                    "name": "status",
                                                    "description": "Get status",
                                                    "args": [],
                                                    "type": { "kind": "ENUM", "name": "Status", "ofType": null }
                                                }
                                            ],
                                            "inputFields": null,
                                            "enumValues": null
                                        },
                                        {
                                            "kind": "OBJECT",
                                            "name": "Mutation",
                                            "description": "Root mutations",
                                            "fields": [
                                                {
                                                    "name": "reset",
                                                    "description": "Reset state",
                                                    "args": [],
                                                    "type": { "kind": "SCALAR", "name": "Boolean", "ofType": null }
                                                }
                                            ],
                                            "inputFields": null,
                                            "enumValues": null
                                        },
                                        {
                                            "kind": "ENUM",
                                            "name": "Status",
                                            "description": null,
                                            "fields": null,
                                            "inputFields": null,
                                            "enumValues": [
                                                { "name": "OK", "description": null },
                                                { "name": "DEGRADED", "description": null }
                                            ]
                                        },
                                        {
                                            "kind": "SCALAR",
                                            "name": "String",
                                            "description": null,
                                            "fields": null,
                                            "inputFields": null,
                                            "enumValues": null
                                        },
                                        {
                                            "kind": "SCALAR",
                                            "name": "Boolean",
                                            "description": null,
                                            "fields": null,
                                            "inputFields": null,
                                            "enumValues": null
                                        }
                                    ],
                                    "directives": []
                                }
                            }
                        }))
                    }
                } else {
                    Json(serde_json::json!({ "data": { "hello": "world" } }))
                }
            }
        }),
    );
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let base = format!("http://{addr}/graphql");
    let temp = tempfile::tempdir().unwrap();
    let snapshot = temp.path().join("graphql-before.json");

    sxmc()
        .args([
            "discover",
            "graphql",
            &base,
            "--schema",
            "--output",
            snapshot.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success();
    assert!(snapshot.exists());

    version.store(1, std::sync::atomic::Ordering::SeqCst);

    let diff = command_json(&[
        "discover",
        "graphql-diff",
        "--before",
        snapshot.to_str().unwrap(),
        "--url",
        &base,
        "--format",
        "json",
    ]);
    assert_eq!(diff["source_type"], "graphql-diff");
    assert!(diff["mutation_type_changed"].as_bool().unwrap_or(false));
    assert!(diff["operation_count_changed"].as_bool().unwrap_or(false));
    assert!(diff["operations_added"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "query:status"));
    assert!(diff["operations_added"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "mutation:reset"));
    assert!(diff["types_added"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "ENUM:Status"));

    sxmc()
        .args([
            "discover",
            "graphql-diff",
            "--before",
            snapshot.to_str().unwrap(),
            "--url",
            &base,
            "--exit-code",
            "--format",
            "json",
        ])
        .assert()
        .failure();

    server.abort();
}

#[test]
fn test_discover_db_sqlite_lists_tables_and_columns() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("demo.sqlite");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
    conn.execute(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            email TEXT NOT NULL,
            active INTEGER DEFAULT 1
        )",
        [],
    )
    .unwrap();
    conn.execute(
        "CREATE TABLE posts (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            title TEXT NOT NULL,
            FOREIGN KEY(user_id) REFERENCES users(id)
        )",
        [],
    )
    .unwrap();
    conn.execute("CREATE INDEX idx_posts_user_id ON posts(user_id)", [])
        .unwrap();
    drop(conn);

    let value = command_json(&[
        "discover",
        "db",
        db_path.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(value["database_type"], "sqlite");
    assert_eq!(value["count"], 2);
    assert!(value["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "users" && entry["column_count"] == 3));
    assert!(value["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "posts"
            && entry["foreign_key_count"] == 1
            && entry["index_count"].as_u64().unwrap_or(0) >= 1));

    let users = command_json(&[
        "discover",
        "db",
        db_path.to_str().unwrap(),
        "users",
        "--format",
        "json",
    ]);
    assert_eq!(users["count"], 1);
    assert_eq!(users["entries"][0]["name"], "users");
    assert!(users["entries"][0]["columns"]
        .as_array()
        .unwrap()
        .iter()
        .any(|column| column["name"] == "email" && column["not_null"] == Value::Bool(true)));

    let compact = command_json(&[
        "discover",
        "db",
        db_path.to_str().unwrap(),
        "--compact",
        "--format",
        "json",
    ]);
    assert!(compact["entries"][0].get("columns").is_none());
    assert!(compact["entries"][0].get("foreign_keys").is_none());
    assert!(compact["entries"][0].get("indexes").is_none());
}

#[test]
fn test_discover_codebase_reports_manifests_tasks_and_entrypoints() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".github").join("workflows")).unwrap();
    fs::create_dir_all(temp.path().join(".cursor").join("rules")).unwrap();
    fs::create_dir_all(temp.path().join("requirements")).unwrap();
    fs::write(
        temp.path().join("Cargo.toml"),
        r#"[package]
name = "demo-app"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "demo-cli"
path = "src/main.rs"
"#,
    )
    .unwrap();
    fs::write(
        temp.path().join("package.json"),
        serde_json::to_string_pretty(&json!({
            "name": "demo-web",
            "scripts": {
                "dev": "vite",
                "test": "vitest run"
            }
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        temp.path().join("pyproject.toml"),
        r#"[project]
name = "demo-py"
version = "0.1.0"

[project.scripts]
demo-task = "demo.cli:main"
"#,
    )
    .unwrap();
    fs::write(
        temp.path().join("Makefile"),
        "build:\n\tcargo build\n\ndev:\n\tnpm run dev\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("requirements").join("dev.txt"),
        "pytest==8.0.0\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("docker-compose.yml"),
        "services:\n  web:\n    image: nginx:latest\n",
    )
    .unwrap();
    fs::write(temp.path().join("turbo.json"), "{ \"pipeline\": {} }\n").unwrap();
    fs::write(
        temp.path().join("tsconfig.json"),
        "{ \"compilerOptions\": {} }\n",
    )
    .unwrap();
    fs::write(temp.path().join("vite.config.ts"), "export default {};\n").unwrap();
    fs::write(
        temp.path().join(".github").join("workflows").join("ci.yml"),
        "name: CI\non: [push]\n",
    )
    .unwrap();
    fs::write(
        temp.path().join(".cursor").join("rules").join("team.md"),
        "# team rules\n",
    )
    .unwrap();

    let value = command_json(&[
        "discover",
        "codebase",
        temp.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(value["source_type"], "codebase");
    assert_eq!(value["manifest_count"], 4);
    assert!(value["task_runner_count"].as_u64().unwrap_or(0) >= 5);
    assert!(value["entrypoints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "demo-cli" && entry["kind"] == "cargo-bin"));
    assert!(value["entrypoints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "dev" && entry["kind"] == "npm-script"));
    assert!(value["entrypoints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "build" && entry["kind"] == "make-target"));
    assert!(value["entrypoints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "demo-task" && entry["kind"] == "python-script"));
    assert!(value["configs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["kind"] == "github-workflow"));
    assert!(value["project_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "rust"));
    assert!(value["project_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "node"));
    assert!(value["project_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "python"));
    assert!(value["project_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "containerized"));
    assert!(value["project_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "frontend"));
    assert!(value["recommended_commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["command"] == "cargo build"));
    assert!(value["recommended_commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["command"] == "npm run dev"));

    let compact = command_json(&[
        "discover",
        "codebase",
        temp.path().to_str().unwrap(),
        "--compact",
        "--format",
        "json",
    ]);
    assert_eq!(compact["manifest_count"], 4);
    assert!(compact.get("manifests").is_none());
    assert!(compact["entrypoint_names"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "demo-cli"));
    assert!(compact["project_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "python"));
    assert!(compact["recommended_command_names"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "build"));
}

#[test]
fn test_discover_codebase_output_and_diff_report_changes() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("Cargo.toml"),
        r#"[package]
name = "demo-app"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    fs::write(
        temp.path().join("package.json"),
        serde_json::to_string_pretty(&json!({
            "name": "demo-web",
            "scripts": {
                "dev": "vite"
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let snapshot = temp.path().join("codebase-before.json");
    sxmc()
        .args([
            "discover",
            "codebase",
            temp.path().to_str().unwrap(),
            "--output",
            snapshot.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success();
    assert!(snapshot.exists());

    fs::write(
        temp.path().join("pyproject.toml"),
        r#"[project]
name = "demo-py"
version = "0.1.0"

[project.scripts]
demo-task = "demo.cli:main"
"#,
    )
    .unwrap();
    fs::write(temp.path().join("Makefile"), "build:\n\tcargo build\n").unwrap();

    let diff = command_json(&[
        "discover",
        "codebase-diff",
        "--before",
        snapshot.to_str().unwrap(),
        "--root",
        temp.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(diff["source_type"], "codebase-diff");
    assert!(diff["manifest_count_changed"].as_bool().unwrap_or(false));
    assert!(diff["task_runner_count_changed"].as_bool().unwrap_or(false));
    assert!(diff["project_kinds_added"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "python"));
    assert!(diff["entrypoints_added"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "make-target:build"));
    assert!(diff["recommended_commands_added"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "make-help: make"));

    sxmc()
        .args([
            "discover",
            "codebase-diff",
            "--before",
            snapshot.to_str().unwrap(),
            "--root",
            temp.path().to_str().unwrap(),
            "--exit-code",
            "--format",
            "json",
        ])
        .assert()
        .failure();
}

#[test]
fn test_discover_traffic_har_groups_searches_and_compacts() {
    let temp = tempfile::tempdir().unwrap();
    let har_path = temp.path().join("capture.har");
    fs::write(
        &har_path,
        serde_json::to_string_pretty(&json!({
            "log": {
                "version": "1.2",
                "creator": { "name": "sxmc-test", "version": "1.0" },
                "entries": [
                    {
                        "request": {
                            "method": "GET",
                            "url": "https://api.example.com/users?page=1"
                        },
                        "response": {
                            "status": 200,
                            "content": { "mimeType": "application/json" }
                        }
                    },
                    {
                        "request": {
                            "method": "GET",
                            "url": "https://api.example.com/users?page=2"
                        },
                        "response": {
                            "status": 304,
                            "content": { "mimeType": "application/json" }
                        }
                    },
                    {
                        "request": {
                            "method": "POST",
                            "url": "https://api.example.com/users"
                        },
                        "response": {
                            "status": 201,
                            "content": { "mimeType": "application/json" }
                        }
                    },
                    {
                        "request": {
                            "method": "GET",
                            "url": "https://cdn.example.com/assets/app.js"
                        },
                        "response": {
                            "status": 200,
                            "content": { "mimeType": "application/javascript" }
                        }
                    }
                ]
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let value = command_json(&[
        "discover",
        "traffic",
        har_path.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(value["source_type"], "traffic");
    assert_eq!(value["request_count"], 4);
    assert_eq!(value["endpoint_count"], 3);
    assert!(value["endpoints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["key"] == "GET api.example.com /users" && entry["count"] == 2));
    assert!(value["endpoints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["key"] == "POST api.example.com /users" && entry["count"] == 1));

    let filtered = command_json(&[
        "discover",
        "traffic",
        har_path.to_str().unwrap(),
        "--search",
        "javascript",
        "--format",
        "json",
    ]);
    assert_eq!(filtered["endpoint_count"], 1);
    assert_eq!(filtered["endpoints"][0]["host"], "cdn.example.com");

    let endpoint = command_json(&[
        "discover",
        "traffic",
        har_path.to_str().unwrap(),
        "/users",
        "--format",
        "json",
    ]);
    assert_eq!(endpoint["endpoint_count"], 2);
    assert!(endpoint["endpoints"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["path"] == "/users"));

    let compact = command_json(&[
        "discover",
        "traffic",
        har_path.to_str().unwrap(),
        "--compact",
        "--format",
        "json",
    ]);
    assert_eq!(compact["endpoint_count"], 3);
    assert!(compact.get("endpoints").is_none());
    assert!(compact["endpoint_keys"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "GET api.example.com /users"));
}

#[test]
fn test_discover_traffic_output_and_diff_report_changes() {
    let temp = tempfile::tempdir().unwrap();
    let before_har = temp.path().join("before.har");
    let after_har = temp.path().join("after.har");
    let snapshot = temp.path().join("traffic-before.json");

    fs::write(
        &before_har,
        serde_json::to_string_pretty(&json!({
            "log": {
                "version": "1.2",
                "creator": { "name": "sxmc-test", "version": "1.0" },
                "entries": [
                    {
                        "request": { "method": "GET", "url": "https://api.example.com/users" },
                        "response": { "status": 200, "content": { "mimeType": "application/json" } }
                    }
                ]
            }
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &after_har,
        serde_json::to_string_pretty(&json!({
            "log": {
                "version": "1.2",
                "creator": { "name": "sxmc-test", "version": "1.0" },
                "entries": [
                    {
                        "request": { "method": "GET", "url": "https://api.example.com/users" },
                        "response": { "status": 200, "content": { "mimeType": "application/json" } }
                    },
                    {
                        "request": { "method": "POST", "url": "https://api.example.com/users" },
                        "response": { "status": 201, "content": { "mimeType": "application/json" } }
                    },
                    {
                        "request": { "method": "GET", "url": "https://cdn.example.com/app.js" },
                        "response": { "status": 200, "content": { "mimeType": "application/javascript" } }
                    }
                ]
            }
        }))
        .unwrap(),
    )
    .unwrap();

    sxmc()
        .args([
            "discover",
            "traffic",
            before_har.to_str().unwrap(),
            "--output",
            snapshot.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success();
    assert!(snapshot.exists());

    let diff = command_json(&[
        "discover",
        "traffic-diff",
        "--before",
        snapshot.to_str().unwrap(),
        "--source",
        after_har.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(diff["source_type"], "traffic-diff");
    assert!(diff["request_count_changed"].as_bool().unwrap_or(false));
    assert!(diff["endpoint_count_changed"].as_bool().unwrap_or(false));
    assert!(diff["endpoints_added"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "POST api.example.com /users"));
    assert!(diff["content_types_added"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry == "GET cdn.example.com /app.js: application/javascript"));

    sxmc()
        .args([
            "discover",
            "traffic-diff",
            "--before",
            snapshot.to_str().unwrap(),
            "--source",
            after_har.to_str().unwrap(),
            "--exit-code",
            "--format",
            "json",
        ])
        .assert()
        .failure();
}

#[test]
fn test_discover_traffic_accepts_curl_command_history() {
    let temp = tempfile::tempdir().unwrap();
    let curl_history = temp.path().join("curl-history.txt");
    fs::write(
        &curl_history,
        r#"curl https://api.example.com/users
curl -X POST -H 'Content-Type: application/json' https://api.example.com/users -d '{"name":"Ada"}'
curl https://cdn.example.com/assets/app.js
"#,
    )
    .unwrap();

    let value = command_json(&[
        "discover",
        "traffic",
        curl_history.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(value["source_type"], "traffic");
    assert_eq!(value["capture_kind"], "curl");
    assert_eq!(value["request_count"], 3);
    assert_eq!(value["endpoint_count"], 3);
    assert!(value["endpoints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["key"] == "POST api.example.com /users"));
    assert!(value["endpoints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["key"] == "POST api.example.com /users"
            && entry["content_types"]
                .as_array()
                .unwrap()
                .iter()
                .any(|content_type| content_type == "application/json")));

    let filtered = command_json(&[
        "discover",
        "traffic",
        curl_history.to_str().unwrap(),
        "--search",
        "cdn.example.com",
        "--format",
        "json",
    ]);
    assert_eq!(filtered["endpoint_count"], 1);
    assert_eq!(filtered["endpoints"][0]["host"], "cdn.example.com");
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
