use assert_cmd::Command;
use axum::{routing::get, Json, Router};
use predicates::prelude::*;
use std::fs;
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

    wait_for_http_server(port);

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

    wait_for_http_server(port);

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

    wait_for_http_server(port);

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

    wait_for_http_server(port);

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

    wait_for_http_server(port);

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

    wait_for_http_server(port);

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
