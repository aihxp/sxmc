use assert_cmd::Command;
use predicates::prelude::*;
use std::net::TcpListener;
use std::process::Command as ProcessCommand;
use std::time::Duration;

fn sxmc() -> Command {
    Command::cargo_bin("sxmc").unwrap()
}

fn sxmc_bin_string() -> String {
    assert_cmd::cargo::cargo_bin("sxmc")
        .to_string_lossy()
        .into_owned()
}

fn pick_unused_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
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
        .stdout(predicate::str::contains("api"))
        .stdout(predicate::str::contains("scan"))
        .stdout(predicate::str::contains("bake"));
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
fn test_http_lists_hybrid_skill_tools() {
    let port = pick_unused_port();
    let mut child = ProcessCommand::new(sxmc_bin_string())
        .args([
            "serve",
            "--transport",
            "http",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--paths",
            "tests/fixtures",
        ])
        .spawn()
        .unwrap();

    std::thread::sleep(Duration::from_millis(750));

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
fn test_http_lists_hybrid_skill_tools_with_required_header() {
    let port = pick_unused_port();
    let mut child = ProcessCommand::new(sxmc_bin_string())
        .args([
            "serve",
            "--transport",
            "http",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--require-header",
            "Authorization: Bearer integration-token",
            "--paths",
            "tests/fixtures",
        ])
        .spawn()
        .unwrap();

    std::thread::sleep(Duration::from_millis(750));

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
    let port = pick_unused_port();
    let mut child = ProcessCommand::new(sxmc_bin_string())
        .env("SXMC_TEST_BEARER_TOKEN", "integration-bearer-token")
        .args([
            "serve",
            "--transport",
            "http",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--bearer-token",
            "env:SXMC_TEST_BEARER_TOKEN",
            "--paths",
            "tests/fixtures",
        ])
        .spawn()
        .unwrap();

    std::thread::sleep(Duration::from_millis(750));

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
    let port = pick_unused_port();
    let mut child = ProcessCommand::new(sxmc_bin_string())
        .args([
            "serve",
            "--transport",
            "http",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--bearer-token",
            "health-token",
            "--paths",
            "tests/fixtures",
        ])
        .spawn()
        .unwrap();

    std::thread::sleep(Duration::from_millis(750));

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

    let _ = child.kill();
    let _ = child.wait();
}
