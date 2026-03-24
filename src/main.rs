mod cli_args;
mod command_handlers;

use chrono::Utc;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use hmac::{Hmac, Mac};
use rmcp::model::{Prompt, Resource, ServerInfo, Tool};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::BufRead;
use std::io::IsTerminal;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use std::collections::HashMap;

use cli_args::{
    BakeAction, Cli, Commands, DiffOutputFormat, InitAction, InspectAction, McpAction,
    McpSessionAction, McpSessionCli, ScaffoldAction, SkillsAction,
};
use command_handlers::{cmd_api, cmd_skills_info, cmd_skills_list, cmd_skills_run};
use sxmc::auth::secrets::{resolve_header, resolve_secret};
use sxmc::bake::config::SourceType;
use sxmc::bake::{BakeConfig, BakeStore};
use sxmc::cli_surfaces::{self, AiClientProfile, AiCoverage, ArtifactMode};
use sxmc::client::{api, graphql, mcp_http, mcp_stdio, openapi};
use sxmc::error::Result;
use sxmc::output;
use sxmc::security;
use sxmc::server::{self, HttpServeLimits};
use sxmc::skills::{discovery, generator, parser};

const PROFILE_BUNDLE_SCHEMA: &str = "sxmc_profile_bundle_v1";
const PROFILE_CORPUS_SCHEMA: &str = "sxmc_profile_corpus_v1";
const PROFILE_STALE_DAYS: i64 = 30;
const PROFILE_BUNDLE_SIGNATURE_ALGORITHM: &str = "hmac-sha256";
const BAKED_HEALTH_SLOW_MS: u64 = 1_000;

fn resolve_paths(paths: Option<Vec<PathBuf>>) -> Vec<PathBuf> {
    paths.unwrap_or_else(discovery::default_paths)
}

fn parse_kv_args(args: &[String]) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for arg in args {
        if let Some((key, value)) = arg.split_once('=') {
            // Try to parse as JSON value, fall back to string
            let val = serde_json::from_str(value)
                .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
            map.insert(key.to_string(), val);
        }
    }
    map
}

fn parse_env_vars(vars: &[String]) -> Vec<(String, String)> {
    vars.iter()
        .filter_map(|v| {
            v.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect()
}

fn parse_string_kv_args(args: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for arg in args {
        if let Some((key, value)) = arg.split_once('=') {
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

fn parse_headers(headers: &[String]) -> Result<Vec<(String, String)>> {
    headers.iter().map(|h| resolve_header(h)).collect()
}

fn parse_optional_secret(secret: Option<String>) -> Result<Option<String>> {
    secret.map(|value| resolve_secret(&value)).transpose()
}

fn parse_timeout(timeout_seconds: Option<u64>) -> Option<Duration> {
    timeout_seconds.map(Duration::from_secs)
}

enum ConnectedMcpClient {
    Stdio(mcp_stdio::StdioClient),
    Http(mcp_http::HttpClient),
}

impl ConnectedMcpClient {
    async fn connect(config: &BakeConfig) -> Result<Self> {
        match config.source_type {
            SourceType::Stdio => {
                let env = parse_env_vars(&config.env_vars);
                Ok(Self::Stdio(
                    mcp_stdio::StdioClient::connect(
                        &config.source,
                        &env,
                        config.base_dir.as_deref(),
                    )
                    .await?,
                ))
            }
            SourceType::Http => {
                let headers = parse_headers(&config.auth_headers)?;
                Ok(Self::Http(
                    mcp_http::HttpClient::connect(
                        &config.source,
                        &headers,
                        parse_timeout(config.timeout_seconds),
                    )
                    .await?,
                ))
            }
            _ => Err(sxmc::error::SxmcError::Other(format!(
                "Bake '{}' is not an MCP connection. Only stdio/http bakes are supported.",
                config.name
            ))),
        }
    }

    async fn list_tools(&self) -> Result<Vec<Tool>> {
        match self {
            Self::Stdio(client) => client.list_tools().await,
            Self::Http(client) => client.list_tools().await,
        }
    }

    fn server_info(&self) -> Option<ServerInfo> {
        match self {
            Self::Stdio(client) => client.server_info(),
            Self::Http(client) => client.server_info(),
        }
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<rmcp::model::CallToolResult> {
        match self {
            Self::Stdio(client) => client.call_tool(name, arguments).await,
            Self::Http(client) => client.call_tool(name, arguments).await,
        }
    }

    async fn list_prompts(&self) -> Result<Vec<Prompt>> {
        match self {
            Self::Stdio(client) => client.list_prompts().await,
            Self::Http(client) => client.list_prompts().await,
        }
    }

    async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<rmcp::model::GetPromptResult> {
        match self {
            Self::Stdio(client) => client.get_prompt(name, arguments).await,
            Self::Http(client) => client.get_prompt(name, arguments).await,
        }
    }

    async fn list_resources(&self) -> Result<Vec<Resource>> {
        match self {
            Self::Stdio(client) => client.list_resources().await,
            Self::Http(client) => client.list_resources().await,
        }
    }

    async fn read_resource(&self, uri: &str) -> Result<rmcp::model::ReadResourceResult> {
        match self {
            Self::Stdio(client) => client.read_resource(uri).await,
            Self::Http(client) => client.read_resource(uri).await,
        }
    }

    async fn close(self) -> Result<()> {
        match self {
            Self::Stdio(client) => client.close().await,
            Self::Http(client) => client.close().await,
        }
    }
}

fn baked_mcp_servers(store: &BakeStore) -> Vec<&BakeConfig> {
    store
        .list()
        .into_iter()
        .filter(|config| matches!(config.source_type, SourceType::Stdio | SourceType::Http))
        .collect()
}

fn get_baked_mcp_server(store: &BakeStore, name: &str) -> Result<BakeConfig> {
    let config = store.get(name).cloned().ok_or_else(|| {
        sxmc::error::SxmcError::Other(format!(
            "Bake '{}' not found. Use `sxmc mcp servers` to see available MCP connections.",
            name
        ))
    })?;

    if !matches!(config.source_type, SourceType::Stdio | SourceType::Http) {
        return Err(sxmc::error::SxmcError::Other(format!(
            "Bake '{}' uses {:?}, not stdio/http MCP.",
            name, config.source_type
        )));
    }

    Ok(config)
}

async fn finish_connected_mcp_client<T>(
    client: ConnectedMcpClient,
    result: Result<T>,
) -> Result<T> {
    let close_result = client.close().await;

    match (result, close_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(_)) => Err(error),
    }
}

async fn connect_named_baked_mcp_client(name: &str) -> Result<ConnectedMcpClient> {
    let store = BakeStore::load()?;
    let config = get_baked_mcp_server(&store, name)?;
    ConnectedMcpClient::connect(&config).await
}

fn split_server_target(target: &str) -> Result<(&str, &str)> {
    target.split_once('/').ok_or_else(|| {
        sxmc::error::SxmcError::Other(format!(
            "Invalid target '{}'. Expected SERVER/NAME.",
            target
        ))
    })
}

fn parse_json_object_arg(
    payload: Option<String>,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    let Some(payload) = payload else {
        return Ok(serde_json::Map::new());
    };

    let raw = if payload == "-" {
        use std::io::Read;
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|e| sxmc::error::SxmcError::Other(format!("Failed to read stdin: {}", e)))?;
        buffer
    } else {
        payload
    };

    if raw.trim().is_empty() {
        return Ok(serde_json::Map::new());
    }

    let value: Value = serde_json::from_str(&raw).map_err(|e| {
        sxmc::error::SxmcError::Other(format!("MCP tool payload must be a JSON object: {}", e))
    })?;

    value.as_object().cloned().ok_or_else(|| {
        sxmc::error::SxmcError::Other("MCP tool payload must be a JSON object.".into())
    })
}

fn looks_like_argument_shape_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("invalid params")
        || lower.contains("validation")
        || lower.contains("expected object")
        || lower.contains("missing required")
        || lower.contains("required property")
        || lower.contains("schema")
}

fn annotate_mcp_tool_call_error(
    error: sxmc::error::SxmcError,
    inspect_hint: &str,
    session_hint: Option<&str>,
) -> sxmc::error::SxmcError {
    let message = match error {
        sxmc::error::SxmcError::McpError(message) => message,
        sxmc::error::SxmcError::Other(message) => message,
        other => return other,
    };

    let mut notes = Vec::new();
    if looks_like_argument_shape_error(&message) {
        notes.push(format!(
            "Inspect the tool schema first with `{}`.",
            inspect_hint
        ));
    }
    if let Some(session_hint) = session_hint {
        notes.push(format!(
            "If the tool expects multi-step state, use `{}` instead of repeated one-shot calls.",
            session_hint
        ));
    }
    notes.push(
        "When machine-parsing structured output, consume stdout only; informational `[sxmc]` lines are written to stderr."
            .into(),
    );

    sxmc::error::SxmcError::Other(format!(
        "{}\n\nRecovery hints:\n- {}",
        message,
        notes.join("\n- ")
    ))
}

fn parse_optional_kv_args(args: &[String]) -> Option<serde_json::Map<String, serde_json::Value>> {
    let arguments = parse_kv_args(args);
    if arguments.is_empty() {
        None
    } else {
        Some(arguments)
    }
}

fn format_mcp_grep_results(
    results: &[(String, Tool)],
    pattern: &str,
    limit: Option<usize>,
) -> String {
    let total = results.len();
    if total == 0 {
        return format!("No MCP tools matched '{}'.", pattern);
    }

    let shown = limit.unwrap_or(total).min(total);
    let mut lines = Vec::new();
    for (server, tool) in results.iter().take(shown) {
        lines.push(format!("  {}/{}", server, tool.name.as_ref()));
        if let Some(description) = &tool.description {
            lines.push(format!("    {}", description));
        }
    }

    let header = if shown < total {
        format!("Matches for '{}' ({} shown of {}):", pattern, shown, total)
    } else {
        format!("Matches for '{}' ({}):", pattern, total)
    };

    format!("{}\n{}", header, lines.join("\n"))
}

#[derive(Clone, Copy)]
enum McpSurface {
    Tools,
    Prompts,
    Resources,
}

impl McpSurface {
    fn label(self) -> &'static str {
        match self {
            Self::Tools => "tool",
            Self::Prompts => "prompt",
            Self::Resources => "resource",
        }
    }

    fn plural_label(self) -> &'static str {
        match self {
            Self::Tools => "tools",
            Self::Prompts => "prompts",
            Self::Resources => "resources",
        }
    }
}

#[derive(Clone, Debug, Default)]
struct McpCapabilities {
    tools: Option<bool>,
    prompts: Option<bool>,
    resources: Option<bool>,
}

impl McpCapabilities {
    fn from_server_info(server_info: Option<&ServerInfo>) -> Self {
        match server_info {
            Some(info) => Self {
                tools: Some(info.capabilities.tools.is_some()),
                prompts: Some(info.capabilities.prompts.is_some()),
                resources: Some(info.capabilities.resources.is_some()),
            },
            None => Self::default(),
        }
    }

    fn supports(&self, surface: McpSurface) -> Option<bool> {
        match surface {
            McpSurface::Tools => self.tools,
            McpSurface::Prompts => self.prompts,
            McpSurface::Resources => self.resources,
        }
    }
}

fn is_capability_not_supported(error: &sxmc::error::SxmcError) -> bool {
    match error {
        sxmc::error::SxmcError::McpError(message) => {
            let lower = message.to_ascii_lowercase();
            lower.contains("-32601")
                || lower.contains("method not found")
                || lower.contains("not supported")
        }
        _ => false,
    }
}

async fn list_optional_surface<T, F>(
    surface: McpSurface,
    advertised: Option<bool>,
    list_future: F,
) -> Result<Vec<T>>
where
    F: std::future::Future<Output = Result<Vec<T>>>,
{
    if advertised == Some(false) {
        eprintln!(
            "[sxmc] Skipping {} listing because the MCP server did not advertise that capability during initialization.",
            surface.label()
        );
        return Ok(Vec::new());
    }

    match list_future.await {
        Ok(items) => Ok(items),
        Err(error) if is_capability_not_supported(&error) => {
            eprintln!(
                "[sxmc] Skipping {} listing because the MCP server does not advertise that capability.",
                surface.label()
            );
            Ok(Vec::new())
        }
        Err(error) => Err(error),
    }
}

fn print_empty_surface_notice(surface: McpSurface, advertised: Option<bool>) {
    if advertised == Some(false) {
        println!(
            "No {} available. The MCP server did not advertise {} support.",
            surface.plural_label(),
            surface.label()
        );
    } else {
        match surface {
            McpSurface::Tools => println!("No tools available."),
            McpSurface::Prompts => println!("No prompts available."),
            McpSurface::Resources => println!("No resources available."),
        }
    }
}

fn build_mcp_description(
    server_info: Option<&ServerInfo>,
    tools: &[Tool],
    prompts: &[Prompt],
    resources: &[Resource],
    limit: Option<usize>,
) -> Value {
    let tool_limit = limit.unwrap_or(tools.len()).min(tools.len());
    let prompt_limit = limit.unwrap_or(prompts.len()).min(prompts.len());
    let resource_limit = limit.unwrap_or(resources.len()).min(resources.len());
    let mut description = output::summarize_server_info(server_info);
    description["detail_mode"] = json!("summary");
    description["counts"] = json!({
        "tools": tools.len(),
        "prompts": prompts.len(),
        "resources": resources.len(),
    });
    description["shown"] = json!({
        "tools": tool_limit,
        "prompts": prompt_limit,
        "resources": resource_limit,
    });
    description["truncated"] = json!({
        "tools": tool_limit < tools.len(),
        "prompts": prompt_limit < prompts.len(),
        "resources": resource_limit < resources.len(),
    });
    if let Some(limit) = limit {
        description["limit"] = json!(limit);
    }
    description["tools"] = Value::Array(
        tools
            .iter()
            .take(tool_limit)
            .map(output::summarize_tool_brief)
            .collect(),
    );
    description["prompts"] = Value::Array(
        prompts
            .iter()
            .take(prompt_limit)
            .map(output::summarize_prompt)
            .collect(),
    );
    description["resources"] = Value::Array(
        resources
            .iter()
            .take(resource_limit)
            .map(output::summarize_resource)
            .collect(),
    );
    description
}

#[derive(Clone, Copy)]
struct McpBridgeRequest<'a> {
    prompt: Option<&'a str>,
    resource_uri: Option<&'a str>,
    args: &'a [String],
    list: bool,
    list_tools: bool,
    list_prompts: bool,
    list_resources: bool,
    search: Option<&'a str>,
    describe: bool,
    describe_tool: Option<&'a str>,
    format: Option<output::StructuredOutputFormat>,
    limit: Option<usize>,
    pretty: bool,
}

impl McpBridgeRequest<'_> {
    fn introspection_requested(self) -> bool {
        self.list
            || self.list_tools
            || self.list_prompts
            || self.list_resources
            || self.search.is_some()
            || self.describe
            || self.describe_tool.is_some()
    }
}

async fn run_mcp_bridge_command(
    client: &ConnectedMcpClient,
    request: McpBridgeRequest<'_>,
) -> Result<()> {
    let server_info = client.server_info();
    let capabilities = McpCapabilities::from_server_info(server_info.as_ref());
    let (tool_name, tool_args) = request
        .args
        .split_first()
        .map(|(name, rest)| (Some(name.as_str()), rest))
        .unwrap_or((None, &[]));

    if request.introspection_requested() {
        let needs_tools = request.list
            || request.list_tools
            || request.search.is_some()
            || request.describe
            || request.describe_tool.is_some();
        let needs_prompts = request.list || request.list_prompts || request.describe;
        let needs_resources = request.list || request.list_resources || request.describe;

        let tools = if needs_tools {
            list_optional_surface(
                McpSurface::Tools,
                capabilities.supports(McpSurface::Tools),
                client.list_tools(),
            )
            .await?
        } else {
            Vec::new()
        };

        if let Some(name) = request.describe_tool {
            let tool = tools
                .iter()
                .find(|tool| tool.name.as_ref() == name)
                .ok_or_else(|| {
                    sxmc::error::SxmcError::Other(format!("Tool not found: {}", name))
                })?;
            println!(
                "{}",
                output::format_tool_detail(tool, request.pretty, request.format)
            );
            return Ok(());
        }

        if request.describe {
            let prompts = if needs_prompts {
                list_optional_surface(
                    McpSurface::Prompts,
                    capabilities.supports(McpSurface::Prompts),
                    client.list_prompts(),
                )
                .await?
            } else {
                Vec::new()
            };
            let resources = if needs_resources {
                list_optional_surface(
                    McpSurface::Resources,
                    capabilities.supports(McpSurface::Resources),
                    client.list_resources(),
                )
                .await?
            } else {
                Vec::new()
            };
            let description = build_mcp_description(
                server_info.as_ref(),
                &tools,
                &prompts,
                &resources,
                request.limit,
            );
            let format = output::resolve_structured_format(request.format, request.pretty);
            println!("{}", output::format_structured_value(&description, format));
            return Ok(());
        }

        let mut printed_any = false;

        if request.list || request.list_tools || request.search.is_some() {
            println!(
                "{}",
                output::format_tool_list(&tools, request.search, request.limit)
            );
            printed_any = true;
        }

        if request.list || request.list_prompts {
            let prompts = list_optional_surface(
                McpSurface::Prompts,
                capabilities.supports(McpSurface::Prompts),
                client.list_prompts(),
            )
            .await?;
            if printed_any {
                println!();
            }
            if prompts.is_empty() {
                print_empty_surface_notice(
                    McpSurface::Prompts,
                    capabilities.supports(McpSurface::Prompts),
                );
            } else {
                println!("{}", output::format_prompt_list(&prompts, request.limit));
            }
            printed_any = true;
        }

        if request.list || request.list_resources {
            let resources = list_optional_surface(
                McpSurface::Resources,
                capabilities.supports(McpSurface::Resources),
                client.list_resources(),
            )
            .await?;
            if printed_any {
                println!();
            }
            if resources.is_empty() {
                print_empty_surface_notice(
                    McpSurface::Resources,
                    capabilities.supports(McpSurface::Resources),
                );
            } else {
                println!(
                    "{}",
                    output::format_resource_list(&resources, request.limit)
                );
            }
        }
    } else if let Some(name) = request.prompt {
        let result = client
            .get_prompt(name, parse_optional_kv_args(request.args))
            .await?;
        println!("{}", output::format_prompt_result(&result, request.pretty));
    } else if let Some(uri) = request.resource_uri {
        let result = client.read_resource(uri).await?;
        println!(
            "{}",
            output::format_resource_result(&result, request.pretty)
        );
    } else if let Some(name) = tool_name {
        let result = client
            .call_tool(name, parse_kv_args(tool_args))
            .await
            .map_err(|error| {
                annotate_mcp_tool_call_error(
                    error,
                    &format!("sxmc ... --describe-tool {}", name),
                    None,
                )
            })?;
        println!("{}", output::format_tool_result(&result, request.pretty));
    } else {
        eprintln!("Specify a tool name, --prompt, --resource, or use --list");
        std::process::exit(1);
    }

    Ok(())
}

fn mcp_session_help() -> &'static str {
    r#"Stateful MCP session commands:
  tools [--search PATTERN] [--limit N]
  prompts [--limit N]
  resources [--limit N]
  describe [--pretty] [--format json|json-pretty|toon] [--limit N]
  info TOOL [--pretty] [--format json|json-pretty|toon]
  call TOOL [JSON_OBJECT|-] [--pretty]
  prompt NAME [key=value ...] [--pretty]
  read RESOURCE_URI [--pretty]
  help
  exit

Examples:
  info sequentialthinking --format toon
  call sequentialthinking '{"thought":"Step A","thoughtNumber":1,"totalThoughts":2,"nextThoughtNeeded":true}' --pretty
  call sequentialthinking '{"thought":"Step B","thoughtNumber":2,"totalThoughts":2,"nextThoughtNeeded":false}' --pretty
"#
}

enum ParsedMcpSessionInput {
    Action(McpSessionAction),
    Help,
    Exit,
}

fn parse_mcp_session_input(line: &str) -> Result<Option<ParsedMcpSessionInput>> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }

    match trimmed {
        "help" => return Ok(Some(ParsedMcpSessionInput::Help)),
        "exit" | "quit" => return Ok(Some(ParsedMcpSessionInput::Exit)),
        _ => {}
    }

    let args = shlex::split(trimmed).ok_or_else(|| {
        sxmc::error::SxmcError::Other("Failed to parse session command line.".into())
    })?;

    let mut argv = vec!["sxmc-session".to_string()];
    argv.extend(args);
    let parsed = McpSessionCli::try_parse_from(argv)
        .map_err(|e| sxmc::error::SxmcError::Other(format!("Invalid session command:\n{}", e)))?;

    Ok(Some(ParsedMcpSessionInput::Action(parsed.action)))
}

async fn print_mcp_tools(
    client: &ConnectedMcpClient,
    search: Option<&str>,
    limit: Option<usize>,
) -> Result<()> {
    let tools = client.list_tools().await?;
    println!("{}", output::format_tool_list(&tools, search, limit));
    Ok(())
}

async fn print_mcp_prompts(client: &ConnectedMcpClient, limit: Option<usize>) -> Result<()> {
    let prompts = list_optional_surface(McpSurface::Prompts, None, client.list_prompts()).await?;
    if prompts.is_empty() {
        print_empty_surface_notice(McpSurface::Prompts, None);
    } else {
        println!("{}", output::format_prompt_list(&prompts, limit));
    }
    Ok(())
}

async fn print_mcp_resources(client: &ConnectedMcpClient, limit: Option<usize>) -> Result<()> {
    let resources =
        list_optional_surface(McpSurface::Resources, None, client.list_resources()).await?;
    if resources.is_empty() {
        print_empty_surface_notice(McpSurface::Resources, None);
    } else {
        println!("{}", output::format_resource_list(&resources, limit));
    }
    Ok(())
}

async fn print_mcp_tool_info(
    client: &ConnectedMcpClient,
    tool_name: &str,
    pretty: bool,
    format: Option<output::StructuredOutputFormat>,
) -> Result<()> {
    let tools = client.list_tools().await?;
    let tool = tools
        .iter()
        .find(|tool| tool.name.as_ref() == tool_name)
        .ok_or_else(|| sxmc::error::SxmcError::Other(format!("Tool not found: {}", tool_name)))?;
    println!("{}", output::format_tool_detail(tool, pretty, format));
    Ok(())
}

async fn call_mcp_tool(
    client: &ConnectedMcpClient,
    tool_name: &str,
    payload: Option<String>,
    pretty: bool,
    inspect_hint: &str,
    session_hint: Option<&str>,
) -> Result<()> {
    let arguments = parse_json_object_arg(payload)?;
    let result = client
        .call_tool(tool_name, arguments)
        .await
        .map_err(|error| annotate_mcp_tool_call_error(error, inspect_hint, session_hint))?;
    println!("{}", output::format_tool_result(&result, pretty));
    Ok(())
}

async fn read_mcp_resource(
    client: &ConnectedMcpClient,
    resource_uri: &str,
    pretty: bool,
) -> Result<()> {
    let result = client.read_resource(resource_uri).await?;
    println!("{}", output::format_resource_result(&result, pretty));
    Ok(())
}

async fn fetch_mcp_prompt(
    client: &ConnectedMcpClient,
    prompt_name: &str,
    args: &[String],
    pretty: bool,
) -> Result<()> {
    let result = client
        .get_prompt(prompt_name, parse_optional_kv_args(args))
        .await?;
    println!("{}", output::format_prompt_result(&result, pretty));
    Ok(())
}

async fn describe_mcp_server(
    client: &ConnectedMcpClient,
    pretty: bool,
    format: Option<output::StructuredOutputFormat>,
    limit: Option<usize>,
) -> Result<()> {
    let server_info = client.server_info();
    let capabilities = McpCapabilities::from_server_info(server_info.as_ref());
    let tools = list_optional_surface(
        McpSurface::Tools,
        capabilities.supports(McpSurface::Tools),
        client.list_tools(),
    )
    .await?;
    let prompts = list_optional_surface(
        McpSurface::Prompts,
        capabilities.supports(McpSurface::Prompts),
        client.list_prompts(),
    )
    .await?;
    let resources = list_optional_surface(
        McpSurface::Resources,
        capabilities.supports(McpSurface::Resources),
        client.list_resources(),
    )
    .await?;
    let description =
        build_mcp_description(server_info.as_ref(), &tools, &prompts, &resources, limit);
    let format = output::resolve_structured_format(format, pretty);
    println!("{}", output::format_structured_value(&description, format));
    Ok(())
}

async fn execute_mcp_session_action(
    client: &ConnectedMcpClient,
    action: McpSessionAction,
) -> Result<()> {
    match action {
        McpSessionAction::Tools { search, limit } => {
            print_mcp_tools(client, search.as_deref(), limit).await
        }
        McpSessionAction::Prompts { limit } => print_mcp_prompts(client, limit).await,
        McpSessionAction::Resources { limit } => print_mcp_resources(client, limit).await,
        McpSessionAction::Describe {
            pretty,
            format,
            limit,
        } => describe_mcp_server(client, pretty, format, limit).await,
        McpSessionAction::Info {
            tool,
            pretty,
            format,
        } => print_mcp_tool_info(client, &tool, pretty, format).await,
        McpSessionAction::Call {
            tool,
            payload,
            pretty,
        } => {
            call_mcp_tool(
                client,
                &tool,
                payload,
                pretty,
                &format!("info {} --format toon", tool),
                Some("sxmc mcp session <server>"),
            )
            .await
        }
        McpSessionAction::Read { resource, pretty } => {
            read_mcp_resource(client, &resource, pretty).await
        }
        McpSessionAction::Prompt {
            prompt,
            args,
            pretty,
        } => fetch_mcp_prompt(client, &prompt, &args, pretty).await,
    }
}

async fn run_mcp_session<R: BufRead>(
    client: &ConnectedMcpClient,
    reader: R,
    quiet: bool,
) -> Result<()> {
    if !quiet {
        eprintln!("{}", mcp_session_help().trim_end());
    }

    for line_result in reader.lines() {
        let line = line_result.map_err(|e| {
            sxmc::error::SxmcError::Other(format!("Failed to read session input: {}", e))
        })?;

        match parse_mcp_session_input(&line)? {
            None => {}
            Some(ParsedMcpSessionInput::Help) => {
                println!("{}", mcp_session_help().trim_end());
            }
            Some(ParsedMcpSessionInput::Exit) => break,
            Some(ParsedMcpSessionInput::Action(action)) => {
                execute_mcp_session_action(client, action).await?;
            }
        }
    }

    Ok(())
}

fn parse_source_type(source_type: &str) -> SourceType {
    match source_type {
        "stdio" => SourceType::Stdio,
        "http" => SourceType::Http,
        "api" => SourceType::Api,
        "spec" => SourceType::Spec,
        "graphql" => SourceType::Graphql,
        other => {
            eprintln!(
                "Unknown source type: {}. Use: stdio, http, api, spec, graphql",
                other
            );
            std::process::exit(1);
        }
    }
}

fn resolve_generation_root(root: Option<PathBuf>) -> Result<PathBuf> {
    match root {
        Some(path) => Ok(path),
        None => std::env::current_dir().map_err(Into::into),
    }
}

fn doctor_target_key_for_host(client: AiClientProfile, config: bool) -> &'static str {
    match (client, config) {
        (AiClientProfile::ClaudeCode, false) => "claude_code",
        (AiClientProfile::ClaudeCode, true) => "claude_code_mcp",
        (AiClientProfile::Cursor, false) => "cursor_rules",
        (AiClientProfile::Cursor, true) => "cursor_mcp",
        (AiClientProfile::GeminiCli, false) => "gemini_cli",
        (AiClientProfile::GeminiCli, true) => "gemini_mcp",
        (AiClientProfile::GithubCopilot, false) => "github_copilot",
        (AiClientProfile::GithubCopilot, true) => "github_copilot_config",
        (AiClientProfile::ContinueDev, false) => "continue_dev",
        (AiClientProfile::ContinueDev, true) => "continue_dev_config",
        (AiClientProfile::OpenCode, false) => "open_code_agent_doc",
        (AiClientProfile::OpenCode, true) => "open_code",
        (AiClientProfile::JetbrainsAiAssistant, false) => "jetbrains_ai_assistant",
        (AiClientProfile::JetbrainsAiAssistant, true) => "jetbrains_ai_assistant_config",
        (AiClientProfile::Junie, false) => "junie",
        (AiClientProfile::Junie, true) => "junie_config",
        (AiClientProfile::Windsurf, false) => "windsurf",
        (AiClientProfile::Windsurf, true) => "windsurf_config",
        (AiClientProfile::OpenaiCodex, false) => "openai_codex_agent_doc",
        (AiClientProfile::OpenaiCodex, true) => "openai_codex_mcp",
        (AiClientProfile::GenericStdioMcp, false) => "generic_stdio_agent_doc",
        (AiClientProfile::GenericStdioMcp, true) => "generic_stdio_mcp",
        (AiClientProfile::GenericHttpMcp, false) => "generic_http_agent_doc",
        (AiClientProfile::GenericHttpMcp, true) => "generic_http_mcp",
    }
}

fn doctor_startup_targets(
    root: &std::path::Path,
    only_hosts: &[AiClientProfile],
) -> Vec<(String, PathBuf)> {
    if only_hosts.is_empty() {
        return vec![
            ("portable_agent_doc".into(), root.join("AGENTS.md")),
            ("claude_code".into(), root.join("CLAUDE.md")),
            ("gemini_cli".into(), root.join("GEMINI.md")),
            (
                "cursor_rules".into(),
                root.join(".cursor").join("rules").join("sxmc-cli-ai.md"),
            ),
            (
                "github_copilot".into(),
                root.join(".github").join("copilot-instructions.md"),
            ),
            (
                "continue_dev".into(),
                root.join(".continue").join("rules").join("sxmc-cli-ai.md"),
            ),
            ("open_code".into(), root.join("opencode.json")),
            (
                "jetbrains_ai_assistant".into(),
                root.join(".aiassistant")
                    .join("rules")
                    .join("sxmc-cli-ai.md"),
            ),
            ("junie".into(), root.join(".junie").join("guidelines.md")),
            (
                "windsurf".into(),
                root.join(".windsurf").join("rules").join("sxmc-cli-ai.md"),
            ),
            ("openai_codex_agent_doc".into(), root.join("AGENTS.md")),
            (
                "openai_codex_mcp".into(),
                root.join(".codex").join("mcp.toml"),
            ),
            ("cursor_mcp".into(), root.join(".cursor").join("mcp.json")),
            (
                "gemini_mcp".into(),
                root.join(".gemini").join("settings.json"),
            ),
        ];
    }

    let mut targets = Vec::new();
    for host in only_hosts {
        let spec = cli_surfaces::host_profile_spec(*host);
        if let Some(path) = spec.native_doc_target {
            targets.push((
                doctor_target_key_for_host(*host, false).into(),
                root.join(path),
            ));
        }
        if let Some(path) = spec.native_config_target {
            targets.push((
                doctor_target_key_for_host(*host, true).into(),
                root.join(path),
            ));
        }
    }
    targets
}

fn doctor_value(root: &std::path::Path, only_hosts: &[AiClientProfile]) -> Result<Value> {
    let bake_store = BakeStore::load()?;
    let cache_stats = sxmc::cache::Cache::new(60 * 60 * 24 * 14)?.stats()?;
    let startup_targets = doctor_startup_targets(root, only_hosts);

    let startup_files = startup_targets
        .into_iter()
        .map(|(name, path)| {
            (
                name.to_string(),
                json!({
                    "path": path.display().to_string(),
                    "present": path.exists(),
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();

    Ok(json!({
        "root": root.display().to_string(),
        "checked_hosts": only_hosts
            .iter()
            .map(|host| cli_surfaces::host_profile_spec(*host).sidecar_scope)
            .collect::<Vec<_>>(),
        "baked_mcp_servers": bake_store.list().len(),
        "portable_profile_dir": {
            "path": root.join(".sxmc").join("ai").join("profiles").display().to_string(),
            "present": root.join(".sxmc").join("ai").join("profiles").exists(),
        },
        "cache": {
            "path": cache_stats.path.display().to_string(),
            "entry_count": cache_stats.entry_count,
            "total_bytes": cache_stats.total_bytes,
            "default_ttl_secs": cache_stats.default_ttl_secs,
        },
        "startup_files": startup_files,
        "recommended_first_moves": [
            {
                "surface": "unknown_cli",
                "command": "sxmc inspect cli <tool> --depth 1 --format json-pretty",
                "why": "Get a structured profile instead of pasting raw help text into context."
            },
            {
                "surface": "unknown_mcp_server",
                "command": "sxmc stdio \"<cmd>\" --list",
                "why": "Discover tools, prompts, and resources before guessing JSON-RPC calls."
            },
            {
                "surface": "known_baked_mcp",
                "command": "sxmc mcp grep <pattern>",
                "why": "Search across baked MCP servers before opening every schema."
            },
            {
                "surface": "unknown_api",
                "command": "sxmc api <url-or-spec> --list",
                "why": "List real operations from the live spec instead of hand-constructing URLs."
            },
            {
                "surface": "startup_install",
                "command": "sxmc init ai --from-cli <tool> --coverage full --mode preview",
                "why": "Generate reviewable startup docs and host configs before applying them."
            },
            {
                "surface": "local_skills_or_prompts",
                "command": "sxmc serve --paths <dir>",
                "why": "Expose a local skills directory as an MCP server when you want prompts and tools to show up in AI hosts."
            },
            {
                "surface": "suspicious_skill_or_repo",
                "command": "sxmc scan --paths <dir>",
                "why": "Check for prompt injection, secrets, Unicode tricks, and dangerous script patterns."
            }
        ]
    }))
}

fn default_saved_profiles_dir(root: &std::path::Path) -> PathBuf {
    root.join(".sxmc").join("ai").join("profiles")
}

fn bundle_slug(input: &str) -> String {
    let mut out = String::new();
    let mut last_sep = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_sep = false;
        } else if !last_sep {
            out.push('-');
            last_sep = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "profile".into()
    } else {
        out
    }
}

fn is_http_target(target: &str) -> bool {
    target.starts_with("http://") || target.starts_with("https://")
}

fn file_uri_to_path(uri: &str) -> PathBuf {
    PathBuf::from(uri.trim_start_matches("file://"))
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{:02x}", byte);
    }
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    bytes_to_hex(&Sha256::digest(bytes))
}

fn resolved_hosts(only_hosts: &[AiClientProfile]) -> Vec<AiClientProfile> {
    if only_hosts.is_empty() {
        vec![
            AiClientProfile::ClaudeCode,
            AiClientProfile::Cursor,
            AiClientProfile::GeminiCli,
            AiClientProfile::GithubCopilot,
            AiClientProfile::ContinueDev,
            AiClientProfile::OpenCode,
            AiClientProfile::JetbrainsAiAssistant,
            AiClientProfile::Junie,
            AiClientProfile::Windsurf,
            AiClientProfile::OpenaiCodex,
        ]
    } else {
        only_hosts.to_vec()
    }
}

fn collect_profile_paths(paths: &[PathBuf], recursive: bool) -> Result<Vec<PathBuf>> {
    fn visit_dir(dir: &Path, recursive: bool, results: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if recursive {
                    visit_dir(&path, recursive, results)?;
                }
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("json"))
                .unwrap_or(false)
            {
                results.push(path);
            }
        }
        Ok(())
    }

    let mut results = Vec::new();
    for path in paths {
        if path.is_dir() {
            visit_dir(path, recursive, &mut results)?;
        } else if path.is_file() {
            results.push(path.clone());
        }
    }
    results.sort();
    results.dedup();
    Ok(results)
}

fn load_bundle_value(path: &Path) -> Result<Value> {
    let value: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let schema = value
        .get("bundle_schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if schema != PROFILE_BUNDLE_SCHEMA {
        return Err(sxmc::error::SxmcError::Other(format!(
            "Bundle file '{}' is not a valid sxmc profile bundle. Expected `bundle_schema: {}`.",
            path.display(),
            PROFILE_BUNDLE_SCHEMA
        )));
    }
    Ok(value)
}

fn validate_bundle_value(value: Value, source_label: &str) -> Result<Value> {
    let schema = value
        .get("bundle_schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if schema != PROFILE_BUNDLE_SCHEMA {
        return Err(sxmc::error::SxmcError::Other(format!(
            "Bundle source '{}' is not a valid sxmc profile bundle. Expected `bundle_schema: {}`.",
            source_label, PROFILE_BUNDLE_SCHEMA
        )));
    }
    Ok(value)
}

fn bundle_sha256_from_value(value: &Value) -> Result<String> {
    Ok(sha256_hex(&serde_json::to_vec(value)?))
}

fn unsigned_bundle_value(value: &Value) -> Value {
    let mut unsigned = value.clone();
    if let Some(object) = unsigned.as_object_mut() {
        object.remove("signature");
    }
    unsigned
}

fn bundle_signature_from_value(value: &Value, secret: &str) -> Result<String> {
    let payload = serde_json::to_vec(&unsigned_bundle_value(value))?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|error| {
        sxmc::error::SxmcError::Other(format!(
            "Failed to initialize bundle signature generator: {}",
            error
        ))
    })?;
    mac.update(&payload);
    let bytes = mac.finalize().into_bytes();
    Ok(bytes_to_hex(bytes.as_slice()))
}

fn sign_bundle_value(mut value: Value, signature_secret: Option<&str>) -> Result<Value> {
    if let Some(secret) = signature_secret {
        let signature = bundle_signature_from_value(&value, secret)?;
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "signature".into(),
                json!({
                    "algorithm": PROFILE_BUNDLE_SIGNATURE_ALGORITHM,
                    "value": signature,
                }),
            );
        }
    }
    Ok(value)
}

fn bundle_signature_report(value: &Value) -> Value {
    match value.get("signature") {
        Some(Value::Object(signature)) => json!({
            "present": true,
            "algorithm": signature.get("algorithm").and_then(Value::as_str),
            "value": signature.get("value").and_then(Value::as_str),
        }),
        _ => json!({
            "present": false,
            "algorithm": Value::Null,
            "value": Value::Null,
        }),
    }
}

fn verify_bundle_signature(
    value: &Value,
    signature_secret: Option<&str>,
    source_label: &str,
) -> Result<Value> {
    let base = bundle_signature_report(value);
    let Some(secret) = signature_secret else {
        return Ok(base);
    };
    let signature = value
        .get("signature")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            sxmc::error::SxmcError::Other(format!(
                "Bundle source '{}' is missing embedded signature metadata. Re-export it with --signature-secret before verifying.",
                source_label
            ))
        })?;
    let algorithm = signature
        .get("algorithm")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if algorithm != PROFILE_BUNDLE_SIGNATURE_ALGORITHM {
        return Err(sxmc::error::SxmcError::Other(format!(
            "Bundle source '{}' uses unsupported signature algorithm '{}'. Expected '{}'.",
            source_label, algorithm, PROFILE_BUNDLE_SIGNATURE_ALGORITHM
        )));
    }
    let expected = signature
        .get("value")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            sxmc::error::SxmcError::Other(format!(
                "Bundle source '{}' is missing an embedded signature value.",
                source_label
            ))
        })?;
    let actual = bundle_signature_from_value(value, secret)?;
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(sxmc::error::SxmcError::Other(format!(
            "Bundle source '{}' did not match the expected embedded signature.\nExpected: {}\nActual:   {}",
            source_label, expected, actual
        )));
    }
    let mut verified = base;
    if let Some(object) = verified.as_object_mut() {
        object.insert("verified".into(), Value::Bool(true));
    }
    Ok(verified)
}

fn verify_bundle_digest(
    value: &Value,
    expected_sha256: Option<&str>,
    source_label: &str,
) -> Result<String> {
    let actual = bundle_sha256_from_value(value)?;
    if let Some(expected) = expected_sha256 {
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(sxmc::error::SxmcError::Other(format!(
                "Bundle source '{}' did not match the expected SHA-256.\nExpected: {}\nActual:   {}",
                source_label, expected, actual
            )));
        }
    }
    Ok(actual)
}

fn bundle_metadata_value(
    bundle_name: Option<&str>,
    description: Option<&str>,
    role: Option<&str>,
    hosts: &[AiClientProfile],
) -> Value {
    json!({
        "name": bundle_name,
        "description": description,
        "role": role,
        "hosts": hosts
            .iter()
            .map(|host| cli_surfaces::host_profile_spec(*host).sidecar_scope)
            .collect::<Vec<_>>(),
    })
}

fn export_profile_bundle_value(
    profile_paths: &[PathBuf],
    bundle_name: Option<&str>,
    description: Option<&str>,
    role: Option<&str>,
    hosts: &[AiClientProfile],
) -> Result<Value> {
    let mut profiles = Vec::new();
    let mut entries = Vec::new();
    for path in profile_paths {
        let profile = cli_surfaces::load_profile(path)?;
        entries.push(json!({
            "command": profile.command,
            "path": path.display().to_string(),
        }));
        profiles.push(serde_json::to_value(profile)?);
    }
    Ok(json!({
        "bundle_schema": PROFILE_BUNDLE_SCHEMA,
        "generated_by": "sxmc",
        "generator_version": env!("CARGO_PKG_VERSION"),
        "generated_at": Utc::now().to_rfc3339(),
        "profile_count": profiles.len(),
        "metadata": bundle_metadata_value(bundle_name, description, role, hosts),
        "entries": entries,
        "profiles": profiles,
    }))
}

#[derive(Copy, Clone)]
enum BundleImportMode {
    Unique,
    Overwrite,
    SkipExisting,
}

fn import_profile_bundle_from_value(
    source_label: &str,
    bundle_value: Value,
    output_dir: &Path,
    mode: BundleImportMode,
) -> Result<Value> {
    let profiles: Vec<cli_surfaces::CliSurfaceProfile> = bundle_value
        .get("profiles")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            sxmc::error::SxmcError::Other(format!(
                "Bundle file '{}' is missing a `profiles` array.",
                source_label
            ))
        })?
        .iter()
        .cloned()
        .map(serde_json::from_value)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(sxmc::error::SxmcError::from)?;
    fs::create_dir_all(output_dir)?;
    let mut written = Vec::new();
    let mut skipped = Vec::new();
    let mut slug_counts: HashMap<String, usize> = HashMap::new();

    for profile in profiles {
        let base_slug = bundle_slug(&profile.command);
        let target = match mode {
            BundleImportMode::Overwrite => output_dir.join(format!("{base_slug}.json")),
            BundleImportMode::SkipExisting => {
                let path = output_dir.join(format!("{base_slug}.json"));
                if path.exists() {
                    skipped.push(json!({
                        "command": profile.command,
                        "path": path.display().to_string(),
                        "reason": "existing file preserved",
                    }));
                    continue;
                }
                path
            }
            BundleImportMode::Unique => {
                let count = slug_counts.entry(base_slug.clone()).or_insert(0);
                let mut path = output_dir.join(format!("{base_slug}.json"));
                while path.exists() {
                    *count += 1;
                    path = output_dir.join(format!("{base_slug}-{}.json", *count + 1));
                }
                path
            }
        };
        fs::write(
            &target,
            serde_json::to_string_pretty(&cli_surfaces::profile_value(&profile))?,
        )?;
        written.push(json!({
            "command": profile.command,
            "path": target.display().to_string(),
        }));
    }

    Ok(json!({
        "bundle_schema": PROFILE_BUNDLE_SCHEMA,
        "input": source_label,
        "output_dir": output_dir.display().to_string(),
        "metadata": bundle_value.get("metadata").cloned().unwrap_or(Value::Null),
        "imported_count": written.len(),
        "skipped_count": skipped.len(),
        "written": written,
        "skipped": skipped,
    }))
}

fn import_profile_bundle_value(
    input: &Path,
    output_dir: &Path,
    mode: BundleImportMode,
) -> Result<Value> {
    let bundle_value = load_bundle_value(input)?;
    import_profile_bundle_from_value(&input.display().to_string(), bundle_value, output_dir, mode)
}

fn request_header_map(headers: &[(String, String)]) -> Result<reqwest::header::HeaderMap> {
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

    let mut map = HeaderMap::new();
    for (key, value) in headers {
        let name = HeaderName::from_bytes(key.as_bytes()).map_err(|error| {
            sxmc::error::SxmcError::Other(format!("Invalid HTTP header name '{}': {}", key, error))
        })?;
        let value = HeaderValue::from_str(value).map_err(|error| {
            sxmc::error::SxmcError::Other(format!(
                "Invalid HTTP header value for '{}': {}",
                key, error
            ))
        })?;
        map.insert(name, value);
    }
    Ok(map)
}

async fn publish_bundle_target(
    target: &str,
    bundle_value: &Value,
    headers: &[(String, String)],
    timeout: Option<Duration>,
) -> Result<Value> {
    if is_http_target(target) {
        let client = reqwest::Client::builder()
            .timeout(timeout.unwrap_or(Duration::from_secs(30)))
            .build()
            .map_err(|error| {
                sxmc::error::SxmcError::Other(format!(
                    "Failed to create HTTP client for bundle publish: {}",
                    error
                ))
            })?;
        let response = client
            .put(target)
            .headers(request_header_map(headers)?)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec_pretty(bundle_value)?)
            .send()
            .await
            .map_err(|error| {
                sxmc::error::SxmcError::Other(format!(
                    "Failed to publish profile bundle to '{}': {}",
                    target, error
                ))
            })?;
        let status = response.status();
        response.error_for_status().map_err(|error| {
            sxmc::error::SxmcError::Other(format!(
                "Failed to publish profile bundle to '{}': {}",
                target, error
            ))
        })?;
        Ok(json!({
            "target": target,
            "transport": "http",
            "http_status": status.as_u16(),
        }))
    } else {
        let target_path = if target.starts_with("file://") {
            file_uri_to_path(target)
        } else {
            PathBuf::from(target)
        };
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target_path, serde_json::to_string_pretty(bundle_value)?)?;
        Ok(json!({
            "target": target_path.display().to_string(),
            "transport": "file",
        }))
    }
}

async fn read_bundle_source(
    source: &str,
    headers: &[(String, String)],
    timeout: Option<Duration>,
) -> Result<Value> {
    if is_http_target(source) {
        let client = reqwest::Client::builder()
            .timeout(timeout.unwrap_or(Duration::from_secs(30)))
            .build()
            .map_err(|error| {
                sxmc::error::SxmcError::Other(format!(
                    "Failed to create HTTP client for bundle pull: {}",
                    error
                ))
            })?;
        let response = client
            .get(source)
            .headers(request_header_map(headers)?)
            .send()
            .await
            .map_err(|error| {
                sxmc::error::SxmcError::Other(format!(
                    "Failed to pull profile bundle from '{}': {}",
                    source, error
                ))
            })?
            .error_for_status()
            .map_err(|error| {
                sxmc::error::SxmcError::Other(format!(
                    "Failed to pull profile bundle from '{}': {}",
                    source, error
                ))
            })?;
        let value: Value = response.json().await.map_err(|error| {
            sxmc::error::SxmcError::Other(format!(
                "Profile bundle response from '{}' was not valid JSON: {}",
                source, error
            ))
        })?;
        validate_bundle_value(value, source)
    } else {
        let path = if source.starts_with("file://") {
            file_uri_to_path(source)
        } else {
            PathBuf::from(source)
        };
        validate_bundle_value(load_bundle_value(&path)?, source)
    }
}

fn drift_entry_for_profile(path: &Path, allow_self: bool) -> Value {
    match cli_surfaces::load_profile(path) {
        Ok(saved) => match cli_surfaces::inspect_cli_with_depth(
            &saved.command,
            allow_self,
            saved.provenance.generation_depth as usize,
        ) {
            Ok(live) => {
                let diff = cli_surfaces::diff_profile_value(&saved, &live);
                json!({
                    "path": path.display().to_string(),
                    "command": saved.command,
                    "changed": diff_value_has_changes(&diff),
                    "error": Value::Null,
                    "diff": diff,
                })
            }
            Err(error) => json!({
                "path": path.display().to_string(),
                "command": saved.command,
                "changed": false,
                "error": error.to_string(),
            }),
        },
        Err(error) => json!({
            "path": path.display().to_string(),
            "command": Value::Null,
            "changed": false,
            "error": error.to_string(),
        }),
    }
}

fn profile_freshness_value(profile: &cli_surfaces::CliSurfaceProfile) -> Value {
    let generated_at = profile.provenance.generated_at.trim();
    if generated_at.is_empty() {
        return json!({
            "known": false,
            "generated_at": Value::Null,
            "age_days": Value::Null,
            "stale": Value::Null,
        });
    }

    match chrono::DateTime::parse_from_rfc3339(generated_at) {
        Ok(parsed) => {
            let parsed = parsed.with_timezone(&Utc);
            let age_days = Utc::now().signed_duration_since(parsed).num_days().max(0);
            json!({
                "known": true,
                "generated_at": generated_at,
                "age_days": age_days,
                "stale": age_days > PROFILE_STALE_DAYS,
            })
        }
        Err(_) => json!({
            "known": false,
            "generated_at": generated_at,
            "age_days": Value::Null,
            "stale": Value::Null,
            "parse_error": true,
        }),
    }
}

fn saved_profile_inventory_value(profile_paths: &[PathBuf]) -> Value {
    let mut entries = Vec::new();
    let mut ready_count = 0usize;
    let mut stale_count = 0usize;
    let mut freshness_known_count = 0usize;
    let mut error_count = 0usize;

    for path in profile_paths {
        match cli_surfaces::load_profile(path) {
            Ok(profile) => {
                let quality = profile.quality_report();
                let freshness = profile_freshness_value(&profile);
                if quality.ready_for_agent_docs {
                    ready_count += 1;
                }
                if freshness["known"].as_bool().unwrap_or(false) {
                    freshness_known_count += 1;
                }
                if freshness["stale"].as_bool().unwrap_or(false) {
                    stale_count += 1;
                }
                entries.push(json!({
                    "path": path.display().to_string(),
                    "command": profile.command,
                    "summary": profile.summary,
                    "subcommand_count": profile.subcommands.len(),
                    "option_count": profile.options.len(),
                    "quality": {
                        "ready_for_agent_docs": quality.ready_for_agent_docs,
                        "score": quality.score,
                        "level": quality.level,
                        "reasons": quality.reasons,
                    },
                    "freshness": freshness,
                    "provenance": {
                        "generated_at": profile.provenance.generated_at,
                        "generator_version": profile.provenance.generator_version,
                        "source_kind": profile.provenance.source_kind,
                    }
                }));
            }
            Err(error) => {
                error_count += 1;
                entries.push(json!({
                    "path": path.display().to_string(),
                    "error": error.to_string(),
                }));
            }
        }
    }

    let total = entries.len();
    json!({
        "count": total,
        "ready_count": ready_count,
        "not_ready_count": total.saturating_sub(ready_count + error_count),
        "freshness_known_count": freshness_known_count,
        "stale_count": stale_count,
        "fresh_count": freshness_known_count.saturating_sub(stale_count),
        "unknown_freshness_count": total.saturating_sub(freshness_known_count + error_count),
        "error_count": error_count,
        "stale_after_days": PROFILE_STALE_DAYS,
        "entries": entries,
    })
}

fn drift_value(profile_paths: &[PathBuf], allow_self: bool) -> Value {
    let entries: Vec<Value> = profile_paths
        .iter()
        .map(|path| drift_entry_for_profile(path, allow_self))
        .collect();
    let changed_count = entries
        .iter()
        .filter(|entry| entry["changed"].as_bool().unwrap_or(false))
        .count();
    let error_count = entries
        .iter()
        .filter(|entry| !entry["error"].is_null())
        .count();
    json!({
        "count": entries.len(),
        "changed_count": changed_count,
        "unchanged_count": entries.len().saturating_sub(changed_count + error_count),
        "error_count": error_count,
        "entries": entries,
    })
}

fn status_value(root: &std::path::Path, only_hosts: &[AiClientProfile]) -> Result<Value> {
    let mut value = doctor_value(root, only_hosts)?;
    let profile_dir = default_saved_profiles_dir(root);
    let (drift, inventory) = if profile_dir.exists() {
        let paths = collect_profile_paths(std::slice::from_ref(&profile_dir), true)?;
        let inventory = saved_profile_inventory_value(&paths);
        let drift = drift_value(&paths, true);
        (drift, inventory)
    } else {
        (
            json!({
                "count": 0,
                "changed_count": 0,
                "unchanged_count": 0,
                "error_count": 0,
                "entries": [],
            }),
            json!({
                "count": 0,
                "ready_count": 0,
                "not_ready_count": 0,
                "freshness_known_count": 0,
                "stale_count": 0,
                "fresh_count": 0,
                "unknown_freshness_count": 0,
                "error_count": 0,
                "stale_after_days": PROFILE_STALE_DAYS,
                "entries": [],
            }),
        )
    };

    if let Some(object) = value.as_object_mut() {
        object.insert(
            "saved_profiles".into(),
            json!({
                "path": profile_dir.display().to_string(),
                "present": profile_dir.exists(),
                "drift": drift,
                "inventory": inventory,
            }),
        );
    }
    Ok(value)
}

fn export_profile_corpus_value(profile_paths: &[PathBuf]) -> Value {
    let mut entries = Vec::new();
    let mut error_count = 0usize;
    let mut ready_count = 0usize;
    let mut stale_count = 0usize;
    let mut freshness_known_count = 0usize;

    for path in profile_paths {
        match cli_surfaces::load_profile(path) {
            Ok(profile) => {
                let quality = profile.quality_report();
                let freshness = profile_freshness_value(&profile);
                if quality.ready_for_agent_docs {
                    ready_count += 1;
                }
                if freshness["known"].as_bool().unwrap_or(false) {
                    freshness_known_count += 1;
                }
                if freshness["stale"].as_bool().unwrap_or(false) {
                    stale_count += 1;
                }
                entries.push(json!({
                    "type": "profile",
                    "path": path.display().to_string(),
                    "command": profile.command,
                    "summary": profile.summary,
                    "quality": {
                        "ready_for_agent_docs": quality.ready_for_agent_docs,
                        "score": quality.score,
                        "level": quality.level,
                        "reasons": quality.reasons,
                    },
                    "freshness": freshness,
                    "profile": cli_surfaces::profile_value(&profile),
                }));
            }
            Err(error) => {
                error_count += 1;
                entries.push(json!({
                    "type": "error",
                    "path": path.display().to_string(),
                    "error": error.to_string(),
                }));
            }
        }
    }

    let total = entries.len();
    json!({
        "corpus_schema": PROFILE_CORPUS_SCHEMA,
        "generated_at": Utc::now().to_rfc3339(),
        "count": total,
        "ready_count": ready_count,
        "not_ready_count": total.saturating_sub(ready_count + error_count),
        "freshness_known_count": freshness_known_count,
        "stale_count": stale_count,
        "fresh_count": freshness_known_count.saturating_sub(stale_count),
        "unknown_freshness_count": total.saturating_sub(freshness_known_count + error_count),
        "error_count": error_count,
        "stale_after_days": PROFILE_STALE_DAYS,
        "entries": entries,
    })
}

fn load_corpus_value(path: &Path) -> Result<Value> {
    let value: Value = serde_json::from_slice(&fs::read(path)?)?;
    let schema = value
        .get("corpus_schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if schema != PROFILE_CORPUS_SCHEMA {
        return Err(sxmc::error::SxmcError::Other(format!(
            "Corpus file '{}' is not a valid sxmc profile corpus. Expected `corpus_schema: {}`.",
            path.display(),
            PROFILE_CORPUS_SCHEMA
        )));
    }
    Ok(value)
}

fn corpus_stats_value(value: &Value) -> Value {
    let entries = value["entries"].as_array().cloned().unwrap_or_default();
    let profile_entries = entries
        .iter()
        .filter(|entry| entry["type"] == "profile")
        .cloned()
        .collect::<Vec<_>>();
    let command_count = profile_entries
        .iter()
        .filter_map(|entry| entry["command"].as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    let ready_count = profile_entries
        .iter()
        .filter(|entry| {
            entry["quality"]["ready_for_agent_docs"]
                .as_bool()
                .unwrap_or(false)
        })
        .count();
    let stale_count = profile_entries
        .iter()
        .filter(|entry| entry["freshness"]["stale"].as_bool().unwrap_or(false))
        .count();
    let average_quality_score = if profile_entries.is_empty() {
        0.0
    } else {
        profile_entries
            .iter()
            .map(|entry| entry["quality"]["score"].as_u64().unwrap_or(0) as f64)
            .sum::<f64>()
            / profile_entries.len() as f64
    };
    json!({
        "corpus_schema": value["corpus_schema"],
        "generated_at": value["generated_at"],
        "count": value["count"],
        "profile_count": profile_entries.len(),
        "error_count": value["error_count"],
        "command_count": command_count,
        "ready_count": ready_count,
        "stale_count": stale_count,
        "average_quality_score": average_quality_score,
    })
}

fn corpus_query_value(
    value: &Value,
    command: Option<&str>,
    search: Option<&str>,
    limit: usize,
) -> Value {
    let search = search.map(|item| item.to_lowercase());
    let mut entries = value["entries"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry["type"] == "profile")
        .filter(|entry| {
            command.is_none_or(|needle| entry["command"].as_str() == Some(needle))
                && search.as_ref().is_none_or(|needle| {
                    let command = entry["command"].as_str().unwrap_or_default().to_lowercase();
                    let summary = entry["summary"].as_str().unwrap_or_default().to_lowercase();
                    command.contains(needle) || summary.contains(needle)
                })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        b["quality"]["score"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["quality"]["score"].as_u64().unwrap_or(0))
    });
    let total_matches = entries.len();
    entries.truncate(limit);
    json!({
        "corpus_schema": value["corpus_schema"],
        "query": {
            "command": command,
            "search": search,
            "limit": limit,
        },
        "match_count": total_matches,
        "entries": entries,
    })
}

fn host_capability_map(
    root: &Path,
    only_hosts: &[AiClientProfile],
) -> serde_json::Map<String, Value> {
    let hosts = resolved_hosts(only_hosts);
    let mut summary = serde_json::Map::new();
    for host in hosts {
        let spec = cli_surfaces::host_profile_spec(host);
        let doc_present = spec
            .native_doc_target
            .map(|path| root.join(path).exists())
            .unwrap_or(false);
        let config_present = spec
            .native_config_target
            .map(|path| root.join(path).exists())
            .unwrap_or(false);
        summary.insert(
            spec.sidecar_scope.into(),
            json!({
                "label": spec.label,
                "doc_present": doc_present,
                "config_present": config_present,
                "ready": doc_present || config_present,
            }),
        );
    }
    summary
}

fn host_capability_value(root: &Path, only_hosts: &[AiClientProfile]) -> Value {
    Value::Object(host_capability_map(root, only_hosts))
}

fn compare_host_capabilities(root: &Path, compare_hosts: &[AiClientProfile]) -> Value {
    let hosts = resolved_hosts(compare_hosts);
    let capability_map = host_capability_map(root, &hosts);
    let mut differences = Vec::new();
    for field in ["ready", "doc_present", "config_present"] {
        let mut truthy = Vec::new();
        let mut falsy = Vec::new();
        for host in &hosts {
            let spec = cli_surfaces::host_profile_spec(*host);
            let key = spec.sidecar_scope;
            let value = capability_map
                .get(key)
                .and_then(|entry| entry.get(field))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if value {
                truthy.push(key);
            } else {
                falsy.push(key);
            }
        }
        if !truthy.is_empty() && !falsy.is_empty() {
            differences.push(json!({
                "field": field,
                "hosts_true": truthy,
                "hosts_false": falsy,
            }));
        }
    }
    json!({
        "hosts": hosts
            .iter()
            .map(|host| cli_surfaces::host_profile_spec(*host).sidecar_scope)
            .collect::<Vec<_>>(),
        "difference_count": differences.len(),
        "differences": differences,
    })
}

async fn baked_health_value() -> Result<Value> {
    let store = BakeStore::load()?;
    let mut entries = Vec::new();
    let mut by_source_type = serde_json::Map::new();
    let mut panels = serde_json::Map::new();
    let configs = store.list();
    let mut latency_sum_ms = 0u64;
    let mut max_latency_ms = 0u64;
    let mut slow_count = 0usize;
    for config in configs {
        let started = Instant::now();
        let check = validate_bake_config(config).await;
        let latency_ms = started.elapsed().as_millis() as u64;
        let source_type = format!("{:?}", config.source_type).to_lowercase();
        let healthy = check.is_ok();
        let slow = latency_ms >= BAKED_HEALTH_SLOW_MS;
        let panel_name = match config.source_type {
            SourceType::Stdio | SourceType::Http => "mcp",
            SourceType::Api => "api",
            SourceType::Spec => "spec",
            SourceType::Graphql => "graphql",
        };
        latency_sum_ms += latency_ms;
        max_latency_ms = max_latency_ms.max(latency_ms);
        if slow {
            slow_count += 1;
        }
        let entry = by_source_type
            .entry(source_type.clone())
            .or_insert_with(|| {
                json!({
                    "count": 0,
                    "healthy_count": 0,
                    "unhealthy_count": 0,
                    "slow_count": 0,
                    "latency_sum_ms": 0,
                    "avg_latency_ms": 0,
                    "max_latency_ms": 0,
                })
            });
        if let Some(object) = entry.as_object_mut() {
            let count = object.get("count").and_then(Value::as_u64).unwrap_or(0) + 1;
            object.insert("count".into(), Value::from(count));
            let key = if healthy {
                "healthy_count"
            } else {
                "unhealthy_count"
            };
            let current = object.get(key).and_then(Value::as_u64).unwrap_or(0) + 1;
            object.insert(key.into(), Value::from(current));
            let slow_total = object
                .get("slow_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                + u64::from(slow);
            object.insert("slow_count".into(), Value::from(slow_total));
            let latency_total = object
                .get("latency_sum_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                + latency_ms;
            object.insert("latency_sum_ms".into(), Value::from(latency_total));
            object.insert("avg_latency_ms".into(), Value::from(latency_total / count));
            let previous_max = object
                .get("max_latency_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            object.insert(
                "max_latency_ms".into(),
                Value::from(previous_max.max(latency_ms)),
            );
        }
        let panel_entry = json!({
            "name": config.name,
            "source_type": source_type,
            "source": config.source,
            "panel": panel_name,
            "healthy": healthy,
            "latency_ms": latency_ms,
            "slow": slow,
            "error": check.err().map(|error| error.to_string()),
        });
        if let Some(object) = panels
            .entry(panel_name)
            .or_insert_with(|| {
                json!({
                    "count": 0,
                    "healthy_count": 0,
                    "unhealthy_count": 0,
                    "slow_count": 0,
                    "latency_sum_ms": 0,
                    "avg_latency_ms": 0,
                    "max_latency_ms": 0,
                    "entries": [],
                })
            })
            .as_object_mut()
        {
            let count = object.get("count").and_then(Value::as_u64).unwrap_or(0) + 1;
            object.insert("count".into(), Value::from(count));
            let key = if healthy {
                "healthy_count"
            } else {
                "unhealthy_count"
            };
            let current = object.get(key).and_then(Value::as_u64).unwrap_or(0) + 1;
            object.insert(key.into(), Value::from(current));
            let slow_total = object
                .get("slow_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                + u64::from(slow);
            object.insert("slow_count".into(), Value::from(slow_total));
            let latency_total = object
                .get("latency_sum_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                + latency_ms;
            object.insert("latency_sum_ms".into(), Value::from(latency_total));
            object.insert("avg_latency_ms".into(), Value::from(latency_total / count));
            let previous_max = object
                .get("max_latency_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            object.insert(
                "max_latency_ms".into(),
                Value::from(previous_max.max(latency_ms)),
            );
            object
                .entry("entries")
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Some(items) = object.get_mut("entries").and_then(Value::as_array_mut) {
                items.push(panel_entry.clone());
            }
        }
        entries.push(panel_entry);
    }
    let healthy_count = entries
        .iter()
        .filter(|entry| entry["healthy"].as_bool().unwrap_or(false))
        .count();
    let total_count = entries.len();
    Ok(json!({
        "checked_at": Utc::now().to_rfc3339(),
        "count": total_count,
        "healthy_count": healthy_count,
        "unhealthy_count": total_count.saturating_sub(healthy_count),
        "slow_count": slow_count,
        "slow_threshold_ms": BAKED_HEALTH_SLOW_MS,
        "latency_sum_ms": latency_sum_ms,
        "avg_latency_ms": if total_count == 0 { 0 } else { latency_sum_ms / total_count as u64 },
        "max_latency_ms": max_latency_ms,
        "by_source_type": by_source_type,
        "panels": panels,
        "entries": entries,
    }))
}

fn status_has_unhealthy_baked_health(value: &Value) -> bool {
    value["baked_health"]["unhealthy_count"]
        .as_u64()
        .unwrap_or(0)
        > 0
}

async fn status_value_with_health(
    root: &std::path::Path,
    only_hosts: &[AiClientProfile],
    compare_hosts: &[AiClientProfile],
    include_health: bool,
) -> Result<Value> {
    let mut value = status_value(root, only_hosts)?;
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "host_capabilities".into(),
            host_capability_value(root, only_hosts),
        );
        if compare_hosts.len() >= 2 {
            object.insert(
                "host_capability_diff".into(),
                compare_host_capabilities(root, compare_hosts),
            );
        }
        if include_health {
            object.insert("baked_health".into(), baked_health_value().await?);
        }
    }
    Ok(value)
}

fn should_render_doctor_human(
    human: bool,
    format: Option<output::StructuredOutputFormat>,
    pretty: bool,
    stdout_is_tty: bool,
) -> bool {
    if human {
        return true;
    }

    format.is_none() && !pretty && stdout_is_tty
}

fn format_doctor_report(value: &Value) -> String {
    let mut lines = Vec::new();
    let startup_files = value["startup_files"].as_object();
    let startup_total = startup_files.map(|files| files.len()).unwrap_or(0);
    let startup_present = startup_files
        .map(|files| {
            files
                .values()
                .filter(|details| details["present"].as_bool().unwrap_or(false))
                .count()
        })
        .unwrap_or(0);
    let portable_profiles_present = value["portable_profile_dir"]["present"]
        .as_bool()
        .unwrap_or(false);
    let portable_profiles_path = value["portable_profile_dir"]["path"]
        .as_str()
        .unwrap_or_default();
    let cache_path = value["cache"]["path"].as_str().unwrap_or_default();
    let cache_entries = value["cache"]["entry_count"].as_u64().unwrap_or(0);
    let cache_total_bytes = value["cache"]["total_bytes"].as_u64().unwrap_or(0);
    let cache_ttl_hours = value["cache"]["default_ttl_secs"].as_u64().unwrap_or(0) / 3600;
    let checked_hosts = value["checked_hosts"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();

    lines.push(format!(
        "Root: {}",
        value["root"].as_str().unwrap_or("<unknown>")
    ));
    if !checked_hosts.is_empty() {
        lines.push(format!("Checked hosts: {}", checked_hosts));
    }
    lines.push(format!(
        "Baked MCP servers: {}",
        value["baked_mcp_servers"].as_u64().unwrap_or(0)
    ));
    lines.push(format!(
        "Profile cache dir: {} ({})",
        if portable_profiles_present {
            "present"
        } else {
            "missing"
        },
        portable_profiles_path
    ));
    lines.push(format!(
        "CLI profile cache: {} entries, {} bytes (TTL: {}h)",
        cache_entries, cache_total_bytes, cache_ttl_hours
    ));
    lines.push(format!("Cache path: {}", cache_path));
    lines.push(format!(
        "Startup files present: {startup_present}/{startup_total}"
    ));
    lines.push(String::new());
    lines.push("Startup files:".into());
    if let Some(files) = startup_files {
        let mut entries: Vec<_> = files.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let present: Vec<_> = entries
            .iter()
            .filter(|(_, details)| details["present"].as_bool().unwrap_or(false))
            .collect();
        let missing: Vec<_> = entries
            .iter()
            .filter(|(_, details)| !details["present"].as_bool().unwrap_or(false))
            .collect();

        if !present.is_empty() {
            lines.push("  Present:".into());
            for (name, details) in present {
                let path = details["path"].as_str().unwrap_or_default();
                lines.push(format!("  - {} ({})", name, path));
            }
        }

        if !missing.is_empty() {
            lines.push("  Missing:".into());
            for (name, details) in missing {
                let path = details["path"].as_str().unwrap_or_default();
                lines.push(format!("  - {} ({})", name, path));
            }
        }
    }
    lines.push(String::new());
    lines.push("Recommended first moves:".into());
    if let Some(moves) = value["recommended_first_moves"].as_array() {
        for (index, item) in moves.iter().enumerate() {
            let surface = item["surface"].as_str().unwrap_or("surface");
            let command = item["command"].as_str().unwrap_or_default();
            let why = item["why"].as_str().unwrap_or_default();
            lines.push(format!(
                "{}. {} -> `{}`",
                index + 1,
                surface.replace('_', " "),
                command
            ));
            lines.push(format!("   {}", why));
        }
    }
    lines.join("\n")
}

fn print_doctor_report(value: &Value) {
    println!("{}", format_doctor_report(value));
}

fn format_status_report(value: &Value) -> String {
    let mut lines = vec![format_doctor_report(value)];
    let saved_profiles = &value["saved_profiles"];
    lines.push(String::new());
    lines.push("Saved CLI profiles".into());
    lines.push(format!(
        "Path: {}",
        saved_profiles["path"].as_str().unwrap_or("<unknown>")
    ));
    let drift = &saved_profiles["drift"];
    lines.push(format!(
        "Profiles: {} total, {} changed, {} unchanged, {} errors",
        drift["count"].as_u64().unwrap_or(0),
        drift["changed_count"].as_u64().unwrap_or(0),
        drift["unchanged_count"].as_u64().unwrap_or(0),
        drift["error_count"].as_u64().unwrap_or(0)
    ));
    let inventory = &saved_profiles["inventory"];
    lines.push(format!(
        "Quality/Freshness: {} ready, {} not ready, {} stale, {} unknown freshness, {} inventory errors",
        inventory["ready_count"].as_u64().unwrap_or(0),
        inventory["not_ready_count"].as_u64().unwrap_or(0),
        inventory["stale_count"].as_u64().unwrap_or(0),
        inventory["unknown_freshness_count"].as_u64().unwrap_or(0),
        inventory["error_count"].as_u64().unwrap_or(0)
    ));
    if let Some(entries) = drift["entries"].as_array() {
        let changed = entries
            .iter()
            .filter(|entry| entry["changed"].as_bool().unwrap_or(false))
            .take(5)
            .collect::<Vec<_>>();
        if !changed.is_empty() {
            lines.push("Changed profiles:".into());
            for entry in changed {
                lines.push(format!(
                    "- {} ({})",
                    entry["command"].as_str().unwrap_or("<unknown>"),
                    entry["path"].as_str().unwrap_or("<unknown>")
                ));
            }
        }
    }
    if let Some(entries) = inventory["entries"].as_array() {
        let stale = entries
            .iter()
            .filter(|entry| entry["freshness"]["stale"].as_bool().unwrap_or(false))
            .take(5)
            .collect::<Vec<_>>();
        if !stale.is_empty() {
            lines.push("Stale profiles:".into());
            for entry in stale {
                lines.push(format!(
                    "- {} ({})",
                    entry["command"].as_str().unwrap_or("<unknown>"),
                    entry["path"].as_str().unwrap_or("<unknown>")
                ));
            }
        }
    }
    if let Some(hosts) = value["host_capabilities"].as_object() {
        lines.push(String::new());
        lines.push("Host capabilities".into());
        let mut entries = hosts.iter().collect::<Vec<_>>();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        for (key, details) in entries {
            let label = details["label"].as_str().unwrap_or(key);
            let doc_present = details["doc_present"].as_bool().unwrap_or(false);
            let config_present = details["config_present"].as_bool().unwrap_or(false);
            let ready = details["ready"].as_bool().unwrap_or(false);
            lines.push(format!(
                "- {}: ready={} doc_present={} config_present={}",
                label, ready, doc_present, config_present
            ));
        }
    }
    if let Some(diff) = value.get("host_capability_diff") {
        lines.push(String::new());
        lines.push(format!(
            "Host capability comparison: {} differing field(s)",
            diff["difference_count"].as_u64().unwrap_or(0)
        ));
        if let Some(entries) = diff["differences"].as_array() {
            for entry in entries {
                let field = entry["field"].as_str().unwrap_or("field");
                let hosts_true = entry["hosts_true"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                let hosts_false = entry["hosts_false"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                lines.push(format!(
                    "- {}: true on [{}], false on [{}]",
                    field, hosts_true, hosts_false
                ));
            }
        }
    }
    if let Some(health) = value.get("baked_health") {
        lines.push(String::new());
        lines.push(format!(
            "Baked connection health: {} healthy, {} unhealthy, {} slow ({} total)",
            health["healthy_count"].as_u64().unwrap_or(0),
            health["unhealthy_count"].as_u64().unwrap_or(0),
            health["slow_count"].as_u64().unwrap_or(0),
            health["count"].as_u64().unwrap_or(0)
        ));
        if let Some(checked_at) = health["checked_at"].as_str() {
            lines.push(format!("Checked at: {}", checked_at));
        }
        lines.push(format!(
            "Latency: avg {}ms, max {}ms, slow threshold {}ms",
            health["avg_latency_ms"].as_u64().unwrap_or(0),
            health["max_latency_ms"].as_u64().unwrap_or(0),
            health["slow_threshold_ms"].as_u64().unwrap_or(0)
        ));
        if let Some(by_type) = health["by_source_type"].as_object() {
            let mut entries = by_type.iter().collect::<Vec<_>>();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            for (source_type, details) in entries {
                lines.push(format!(
                    "- {}: {} healthy, {} unhealthy, {} slow ({} total, avg {}ms, max {}ms)",
                    source_type,
                    details["healthy_count"].as_u64().unwrap_or(0),
                    details["unhealthy_count"].as_u64().unwrap_or(0),
                    details["slow_count"].as_u64().unwrap_or(0),
                    details["count"].as_u64().unwrap_or(0),
                    details["avg_latency_ms"].as_u64().unwrap_or(0),
                    details["max_latency_ms"].as_u64().unwrap_or(0)
                ));
            }
        }
        if let Some(panels) = health["panels"].as_object() {
            let mut entries = panels.iter().collect::<Vec<_>>();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            for (panel, details) in entries {
                lines.push(format!(
                    "- panel {}: {} healthy, {} unhealthy, {} slow ({} total, avg {}ms, max {}ms)",
                    panel,
                    details["healthy_count"].as_u64().unwrap_or(0),
                    details["unhealthy_count"].as_u64().unwrap_or(0),
                    details["slow_count"].as_u64().unwrap_or(0),
                    details["count"].as_u64().unwrap_or(0),
                    details["avg_latency_ms"].as_u64().unwrap_or(0),
                    details["max_latency_ms"].as_u64().unwrap_or(0)
                ));
            }
        }
        if let Some(entries) = health["entries"].as_array() {
            for entry in entries
                .iter()
                .filter(|entry| !entry["healthy"].as_bool().unwrap_or(false))
                .take(5)
            {
                lines.push(format!(
                    "- {} [{}] {}ms: {}",
                    entry["name"].as_str().unwrap_or("<unknown>"),
                    entry["source_type"].as_str().unwrap_or("unknown"),
                    entry["latency_ms"].as_u64().unwrap_or(0),
                    entry["error"].as_str().unwrap_or("unhealthy")
                ));
            }
        }
    }
    lines.join("\n")
}

fn print_status_report(value: &Value) {
    println!("{}", format_status_report(value));
}

fn render_status_output(
    value: &Value,
    format: Option<output::StructuredOutputFormat>,
    pretty: bool,
    stdout_is_tty: bool,
) -> String {
    if should_render_doctor_human(false, format, pretty, stdout_is_tty) {
        format_status_report(value)
    } else {
        let format = output::resolve_structured_format(format, pretty);
        output::format_structured_value(value, format)
    }
}

fn print_drift_report(value: &Value) {
    println!(
        "Saved CLI profile drift: {} changed, {} unchanged, {} errors ({} total)",
        value["changed_count"].as_u64().unwrap_or(0),
        value["unchanged_count"].as_u64().unwrap_or(0),
        value["error_count"].as_u64().unwrap_or(0),
        value["count"].as_u64().unwrap_or(0)
    );
    if let Some(entries) = value["entries"].as_array() {
        for entry in entries {
            if let Some(error) = entry["error"].as_str() {
                println!(
                    "- {}: error: {}",
                    entry["path"].as_str().unwrap_or("<unknown>"),
                    error
                );
            } else {
                println!(
                    "- {}: {}",
                    entry["command"].as_str().unwrap_or("<unknown>"),
                    if entry["changed"].as_bool().unwrap_or(false) {
                        "changed"
                    } else {
                        "unchanged"
                    }
                );
            }
        }
    }
}

fn print_batch_inspect_report(value: &Value, compact: bool) {
    let count = value["count"].as_u64().unwrap_or(0);
    let inspected_count = value["inspected_count"].as_u64().unwrap_or(count);
    let success_count = value["success_count"].as_u64().unwrap_or(0);
    let failed_count = value["failed_count"].as_u64().unwrap_or(0);
    let skipped_count = value["skipped_count"].as_u64().unwrap_or(0);
    println!(
        "Inspected {} of {} command(s): {} succeeded, {} failed, {} skipped",
        inspected_count, count, success_count, failed_count, skipped_count
    );

    if let Some(profiles) = value["profiles"].as_array() {
        for profile in profiles {
            let command = profile["command"].as_str().unwrap_or("<unknown>");
            let summary = profile["summary"].as_str().unwrap_or_default();
            if compact {
                let subcommand_count = profile["subcommand_count"].as_u64().unwrap_or(0);
                let option_count = profile["option_count"].as_u64().unwrap_or(0);
                println!(
                    "- {}: {} ({} subcommands, {} options)",
                    command, summary, subcommand_count, option_count
                );
            } else {
                let subcommand_count = profile["subcommands"]
                    .as_array()
                    .map(|items| items.len())
                    .unwrap_or(0);
                let option_count = profile["options"]
                    .as_array()
                    .map(|items| items.len())
                    .unwrap_or(0);
                println!(
                    "- {}: {} ({} subcommands, {} options)",
                    command, summary, subcommand_count, option_count
                );
            }
        }
    }

    if let Some(failures) = value["failures"].as_array() {
        if !failures.is_empty() {
            println!();
            println!("Failures:");
            for failure in failures {
                println!(
                    "- {}: {}",
                    failure["command"].as_str().unwrap_or("<unknown>"),
                    failure["error"].as_str().unwrap_or("unknown error")
                );
            }
        }
    }

    if let Some(skipped) = value["skipped"].as_array() {
        if !skipped.is_empty() {
            println!();
            println!("Skipped:");
            for entry in skipped {
                println!(
                    "- {}: {}",
                    entry["command"].as_str().unwrap_or("<unknown>"),
                    entry["reason"].as_str().unwrap_or("skipped")
                );
            }
        }
    }

    if let Some(output_dir) = value["output_dir"].as_str() {
        println!();
        println!(
            "Saved {} profile file(s) to {}",
            value["written_profile_count"].as_u64().unwrap_or(0),
            output_dir
        );
    }
}

fn format_batch_toon(value: &Value, compact: bool) -> String {
    let mut lines = Vec::new();
    lines.push(format!("count: {}", value["count"].as_u64().unwrap_or(0)));
    lines.push(format!(
        "inspected_count: {}",
        value["inspected_count"]
            .as_u64()
            .unwrap_or_else(|| value["count"].as_u64().unwrap_or(0))
    ));
    lines.push(format!(
        "parallelism: {}",
        value["parallelism"].as_u64().unwrap_or(0)
    ));
    lines.push(format!(
        "success_count: {}",
        value["success_count"].as_u64().unwrap_or(0)
    ));
    lines.push(format!(
        "failed_count: {}",
        value["failed_count"].as_u64().unwrap_or(0)
    ));
    lines.push(format!(
        "skipped_count: {}",
        value["skipped_count"].as_u64().unwrap_or(0)
    ));
    lines.push(String::new());
    lines.push("profiles:".into());

    if let Some(profiles) = value["profiles"].as_array() {
        for profile in profiles {
            let command = profile["command"].as_str().unwrap_or("<unknown>");
            let summary = profile["summary"].as_str().unwrap_or_default();
            if compact {
                lines.push(format!(
                    "- {}: {} ({} subcommands, {} options)",
                    command,
                    summary,
                    profile["subcommand_count"].as_u64().unwrap_or(0),
                    profile["option_count"].as_u64().unwrap_or(0)
                ));
            } else {
                let subcommand_count = profile["subcommands"]
                    .as_array()
                    .map(|items| items.len())
                    .unwrap_or(0);
                let option_count = profile["options"]
                    .as_array()
                    .map(|items| items.len())
                    .unwrap_or(0);
                lines.push(format!(
                    "- {}: {} ({} subcommands, {} options)",
                    command, summary, subcommand_count, option_count
                ));
            }
        }
    }

    if let Some(failures) = value["failures"].as_array() {
        if !failures.is_empty() {
            lines.push(String::new());
            lines.push("failures:".into());
            for failure in failures {
                lines.push(format!(
                    "- {}: {}",
                    failure["command"].as_str().unwrap_or("<unknown>"),
                    failure["error"].as_str().unwrap_or("unknown error")
                ));
            }
        }
    }

    if let Some(skipped) = value["skipped"].as_array() {
        if !skipped.is_empty() {
            lines.push(String::new());
            lines.push("skipped:".into());
            for entry in skipped {
                lines.push(format!(
                    "- {}: {}",
                    entry["command"].as_str().unwrap_or("<unknown>"),
                    entry["reason"].as_str().unwrap_or("skipped")
                ));
            }
        }
    }

    if let Some(output_dir) = value["output_dir"].as_str() {
        lines.push(String::new());
        lines.push(format!(
            "saved_profiles: {} -> {}",
            value["written_profile_count"].as_u64().unwrap_or(0),
            output_dir
        ));
    }

    lines.join("\n")
}

fn format_diff_toon(value: &Value) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "command: {}",
        value["command"].as_str().unwrap_or("<unknown>")
    ));
    lines.push(format!(
        "summary_changed: {}",
        value["summary_changed"].as_bool().unwrap_or(false)
    ));
    if let Some(before) = value["before_summary"].as_str() {
        lines.push(format!("before_summary: {}", before));
    }
    if let Some(after) = value["after_summary"].as_str() {
        lines.push(format!("after_summary: {}", after));
    }
    if let Some(note) = value["migration_note"].as_str() {
        lines.push(format!("migration_note: {}", note));
    }

    let add_list = |lines: &mut Vec<String>, label: &str, field: &Value| {
        if let Some(items) = field.as_array() {
            if !items.is_empty() {
                lines.push(String::new());
                lines.push(format!("{}:", label));
                for item in items {
                    lines.push(format!("- {}", item.as_str().unwrap_or("<unknown>")));
                }
            }
        }
    };

    add_list(&mut lines, "subcommands_added", &value["subcommands_added"]);
    add_list(
        &mut lines,
        "subcommands_removed",
        &value["subcommands_removed"],
    );
    add_list(&mut lines, "options_added", &value["options_added"]);
    add_list(&mut lines, "options_removed", &value["options_removed"]);
    add_list(&mut lines, "environment_added", &value["environment_added"]);
    add_list(
        &mut lines,
        "environment_removed",
        &value["environment_removed"],
    );
    lines.join("\n")
}

fn slugify_loose(input: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn compact_value_from_full_profile_value(profile: &Value) -> Value {
    serde_json::from_value::<cli_surfaces::CliSurfaceProfile>(profile.clone())
        .map(|profile| cli_surfaces::compact_profile_value(&profile))
        .unwrap_or_else(|_| profile.clone())
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum BatchOutputWriteMode {
    Unique,
    Overwrite,
    SkipExisting,
}

fn diff_value_has_changes(value: &Value) -> bool {
    value["summary_changed"].as_bool().unwrap_or(false)
        || value["description_changed"].as_bool().unwrap_or(false)
        || !value["subcommands_added"]
            .as_array()
            .map(|items| items.is_empty())
            .unwrap_or(true)
        || !value["subcommands_removed"]
            .as_array()
            .map(|items| items.is_empty())
            .unwrap_or(true)
        || !value["options_added"]
            .as_array()
            .map(|items| items.is_empty())
            .unwrap_or(true)
        || !value["options_removed"]
            .as_array()
            .map(|items| items.is_empty())
            .unwrap_or(true)
        || !value["environment_added"]
            .as_array()
            .map(|items| items.is_empty())
            .unwrap_or(true)
        || !value["environment_removed"]
            .as_array()
            .map(|items| items.is_empty())
            .unwrap_or(true)
        || value["before_generation_depth"] != value["after_generation_depth"]
        || value["before_nested_profile_count"] != value["after_nested_profile_count"]
}

fn resolve_batch_profile_output_path(
    output_dir: &Path,
    command: &str,
    slug_counts: &mut HashMap<String, usize>,
) -> (String, PathBuf) {
    let mut slug = slugify_loose(command);
    if slug.is_empty() {
        slug = "profile".into();
    }
    let count = slug_counts.entry(slug.clone()).or_insert(0);
    *count += 1;
    loop {
        let file_name = if *count == 1 {
            format!("{slug}.json")
        } else {
            format!("{slug}-{}.json", *count)
        };
        let path = output_dir.join(&file_name);
        if !path.exists() {
            return (slug, path);
        }
        *count += 1;
    }
}

fn write_batch_profile_file(
    output_dir: &Path,
    command: &str,
    profile: &Value,
    compact: bool,
    slug_counts: &mut HashMap<String, usize>,
    write_mode: BatchOutputWriteMode,
) -> Result<Value> {
    fs::create_dir_all(output_dir)?;
    let rendered_value = if compact {
        compact_value_from_full_profile_value(profile)
    } else {
        profile.clone()
    };
    let path = match write_mode {
        BatchOutputWriteMode::Unique => {
            resolve_batch_profile_output_path(output_dir, command, slug_counts).1
        }
        BatchOutputWriteMode::Overwrite | BatchOutputWriteMode::SkipExisting => {
            let mut slug = slugify_loose(command);
            if slug.is_empty() {
                slug = "profile".into();
            }
            output_dir.join(format!("{slug}.json"))
        }
    };
    let existed = path.exists();
    if matches!(write_mode, BatchOutputWriteMode::SkipExisting) && path.exists() {
        return Ok(json!({
            "command": command,
            "path": path.display().to_string(),
            "compact": compact,
            "action": "skipped_existing",
        }));
    }
    fs::write(&path, serde_json::to_string_pretty(&rendered_value)?)?;
    Ok(json!({
        "command": command,
        "path": path.display().to_string(),
        "compact": compact,
        "action": if matches!(write_mode, BatchOutputWriteMode::Overwrite) && existed {
            "overwritten"
        } else {
            "written"
        },
    }))
}

fn attach_batch_output_dir_metadata(
    value: &mut Value,
    output_dir: &Path,
    written_profiles: &[Value],
) {
    let written_count = written_profiles
        .iter()
        .filter(|entry| entry["action"].as_str().unwrap_or("written") != "skipped_existing")
        .count();
    let skipped_existing_count = written_profiles
        .iter()
        .filter(|entry| entry["action"].as_str() == Some("skipped_existing"))
        .count();
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "output_dir".into(),
            Value::String(output_dir.display().to_string()),
        );
        object.insert(
            "written_profile_count".into(),
            Value::from(written_count as u64),
        );
        object.insert(
            "skipped_existing_count".into(),
            Value::from(skipped_existing_count as u64),
        );
        object.insert(
            "written_profiles".into(),
            Value::Array(written_profiles.to_vec()),
        );
    }
}

fn write_batch_manifest_file(output_dir: &Path, value: &Value) -> Result<PathBuf> {
    fs::create_dir_all(output_dir)?;
    let path = output_dir.join("batch-summary.json");
    fs::write(&path, serde_json::to_string_pretty(value)?)?;
    Ok(path)
}

fn batch_event_for_output(event: &Value, compact: bool) -> Value {
    match event["type"].as_str().unwrap_or_default() {
        "profile" => {
            let profile = if compact {
                compact_value_from_full_profile_value(&event["profile"])
            } else {
                event["profile"].clone()
            };
            json!({
                "type": "profile",
                "command": event["command"],
                "profile": profile,
            })
        }
        _ => event.clone(),
    }
}

fn format_diff_markdown(value: &Value) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "# CLI Diff: `{}`",
        value["command"].as_str().unwrap_or("<unknown>")
    ));
    lines.push(String::new());
    lines.push(format!(
        "- Summary changed: `{}`",
        value["summary_changed"].as_bool().unwrap_or(false)
    ));
    if let Some(before) = value["before_summary"].as_str() {
        lines.push(format!("- Before summary: {}", before));
    }
    if let Some(after) = value["after_summary"].as_str() {
        lines.push(format!("- After summary: {}", after));
    }
    if let Some(note) = value["migration_note"].as_str() {
        lines.push(format!("- Migration note: {}", note));
    }

    let mut push_section = |title: &str, field: &Value| {
        if let Some(items) = field.as_array() {
            if !items.is_empty() {
                lines.push(String::new());
                lines.push(format!("## {}", title));
                lines.push(String::new());
                for item in items {
                    lines.push(format!("- `{}`", item.as_str().unwrap_or("<unknown>")));
                }
            }
        }
    };

    push_section("Added subcommands", &value["subcommands_added"]);
    push_section("Removed subcommands", &value["subcommands_removed"]);
    push_section("Added options", &value["options_added"]);
    push_section("Removed options", &value["options_removed"]);
    push_section("Added environment", &value["environment_added"]);
    push_section("Removed environment", &value["environment_removed"]);
    lines.join("\n")
}

fn diff_display_value(value: &Value, format: DiffOutputFormat) -> String {
    if matches!(format, DiffOutputFormat::Toon) {
        format_diff_toon(value)
    } else if matches!(format, DiffOutputFormat::Markdown) {
        format_diff_markdown(value)
    } else {
        output::format_structured_value(value, format.as_structured().unwrap())
    }
}

fn resolve_diff_output_format(format: Option<DiffOutputFormat>, pretty: bool) -> DiffOutputFormat {
    format.unwrap_or(if pretty {
        DiffOutputFormat::JsonPretty
    } else {
        DiffOutputFormat::Json
    })
}

fn print_cache_stats_report(value: &Value) {
    println!("CLI profile cache");
    println!("Path: {}", value["path"].as_str().unwrap_or("<unknown>"));
    println!("Entries: {}", value["entry_count"].as_u64().unwrap_or(0));
    println!("Size: {} bytes", value["total_bytes"].as_u64().unwrap_or(0));
    println!(
        "Default TTL: {} seconds",
        value["default_ttl_secs"].as_u64().unwrap_or(0)
    );
}

fn print_cache_warm_report(value: &Value) {
    println!(
        "Warmed {} CLI profile(s) with parallelism {} ({} failures, {} skipped)",
        value["warmed_count"].as_u64().unwrap_or(0),
        value["parallelism"].as_u64().unwrap_or(0),
        value["failed_count"].as_u64().unwrap_or(0),
        value["skipped_count"].as_u64().unwrap_or(0)
    );
}

fn print_migrated_profile_report(value: &Value) {
    println!(
        "Migrated CLI profile for `{}`",
        value["command"].as_str().unwrap_or("<unknown>")
    );
    if let Some(input) = value["input"].as_str() {
        println!("Input: {}", input);
    }
    if let Some(output) = value["output"].as_str() {
        println!("Output: {}", output);
    }
    println!(
        "Schema: {}",
        value["profile_schema"].as_str().unwrap_or("<unknown>")
    );
}

fn print_corpus_stats_report(value: &Value) {
    println!("Profile corpus");
    println!(
        "Entries: {} (profiles: {}, errors: {})",
        value["count"].as_u64().unwrap_or(0),
        value["profile_count"].as_u64().unwrap_or(0),
        value["error_count"].as_u64().unwrap_or(0)
    );
    println!(
        "Commands: {} | Ready: {} | Stale: {} | Avg quality: {:.1}",
        value["command_count"].as_u64().unwrap_or(0),
        value["ready_count"].as_u64().unwrap_or(0),
        value["stale_count"].as_u64().unwrap_or(0),
        value["average_quality_score"].as_f64().unwrap_or(0.0)
    );
}

fn print_corpus_query_report(value: &Value) {
    println!(
        "Corpus query: {} match(es)",
        value["match_count"].as_u64().unwrap_or(0)
    );
    if let Some(entries) = value["entries"].as_array() {
        for entry in entries {
            println!(
                "- {}: {} [quality={} stale={}]",
                entry["command"].as_str().unwrap_or("<unknown>"),
                entry["summary"].as_str().unwrap_or_default(),
                entry["quality"]["score"].as_u64().unwrap_or(0),
                entry["freshness"]["stale"].as_bool().unwrap_or(false)
            );
        }
    }
}

async fn validate_bake_config(config: &BakeConfig) -> Result<()> {
    match config.source_type {
        SourceType::Stdio | SourceType::Http => {
            let client = ConnectedMcpClient::connect(config).await.map_err(|error| {
                let base = format!(
                    "Bake '{}' could not connect during validation: {}",
                    config.name, error
                );
                sxmc::error::SxmcError::Other(augment_bake_validation_message(
                    config,
                    &base,
                    &error.to_string(),
                ))
            })?;
            let result = client.list_tools().await.map_err(|error| {
                let base = format!(
                    "Bake '{}' connected but list_tools failed during validation: {}",
                    config.name, error
                );
                sxmc::error::SxmcError::Other(augment_bake_validation_message(
                    config,
                    &base,
                    &error.to_string(),
                ))
            });
            client.close().await?;
            result.map(|_| ())
        }
        SourceType::Api => {
            let headers = parse_headers(&config.auth_headers)?;
            api::ApiClient::connect(
                &config.source,
                &headers,
                parse_timeout(config.timeout_seconds.or(Some(10))),
            )
            .await
            .map(|_| ())
            .map_err(|error| {
                let base = format!(
                    "Bake '{}' could not validate API source '{}': {}",
                    config.name, config.source, error
                );
                sxmc::error::SxmcError::Other(augment_bake_validation_message(
                    config,
                    &base,
                    &error.to_string(),
                ))
            })
        }
        SourceType::Spec => {
            let headers = parse_headers(&config.auth_headers)?;
            openapi::OpenApiSpec::load(
                &config.source,
                &headers,
                parse_timeout(config.timeout_seconds.or(Some(10))),
            )
            .await
            .map(|_| ())
            .map_err(|error| {
                let base = format!(
                    "Bake '{}' could not validate OpenAPI source '{}': {}",
                    config.name, config.source, error
                );
                sxmc::error::SxmcError::Other(augment_bake_validation_message(
                    config,
                    &base,
                    &error.to_string(),
                ))
            })
        }
        SourceType::Graphql => {
            let headers = parse_headers(&config.auth_headers)?;
            graphql::GraphQLClient::connect(
                &config.source,
                &headers,
                parse_timeout(config.timeout_seconds.or(Some(10))),
            )
            .await
            .map(|_| ())
            .map_err(|error| {
                let base = format!(
                    "Bake '{}' could not validate GraphQL source '{}': {}",
                    config.name, config.source, error
                );
                sxmc::error::SxmcError::Other(augment_bake_validation_message(
                    config,
                    &base,
                    &error.to_string(),
                ))
            })
        }
    }
}

fn augment_bake_validation_message(config: &BakeConfig, base: &str, detail: &str) -> String {
    let mut hints = Vec::new();
    let lowered = detail.to_ascii_lowercase();

    match config.source_type {
        SourceType::Stdio => {
            hints.push("Run the stdio command directly once to confirm it starts and speaks MCP over stdout.".to_string());
            if lowered.contains("command not found")
                || lowered.contains("no such file or directory")
            {
                hints.push("The configured executable was not found on PATH. Use a full path, install the tool first, or wrap npm-based servers with `npx`.".to_string());
            }
            if config.source.contains("npx") {
                hints.push("If this is an npm MCP server, verify the package name manually with `npx ... --help` or install it globally before baking it.".to_string());
            }
            if config.source.contains("python")
                || config.source.contains(".py")
                || config.source.contains("uv ")
                || config.source.contains("uvx ")
            {
                hints.push("For Python-backed servers, confirm the virtualenv or tool runner is available in the same environment where sxmc will execute the bake.".to_string());
            }
            if config.source.contains("docker") || config.source.contains("podman") {
                hints.push("For container-backed servers, confirm the image exists locally and that the command keeps stdin/stdout attached for MCP traffic.".to_string());
            }
        }
        SourceType::Http => {
            hints.push("Check that the HTTP MCP server is already running and that the URL points at its streamable MCP endpoint (often `/mcp`).".to_string());
            if lowered.contains("401")
                || lowered.contains("403")
                || lowered.contains("unauthorized")
            {
                hints.push("Validation reached the server but auth failed. Re-check `--auth-header` values or bearer-token setup.".to_string());
            }
            if lowered.contains("connection refused")
                || lowered.contains("timed out")
                || lowered.contains("dns")
                || lowered.contains("connect")
            {
                hints.push("If the server is intentionally offline right now, re-run with `--skip-validate` and bring it up before calling it later.".to_string());
            }
        }
        SourceType::Api => {
            hints.push("Verify the API spec URL is reachable and that any auth headers or timeout settings are correct.".to_string());
            if lowered.contains("401")
                || lowered.contains("403")
                || lowered.contains("unauthorized")
            {
                hints.push("The API rejected auth during validation. Re-check tokens, headers, and whether the endpoint expects a different auth scheme.".to_string());
            }
            if !config.source.ends_with(".json")
                && !config.source.ends_with(".yaml")
                && !config.source.ends_with(".yml")
            {
                hints.push("If this is an API docs page rather than a machine-readable spec, bake the raw OpenAPI URL instead of the human HTML page.".to_string());
            }
        }
        SourceType::Spec => {
            hints.push("Confirm the OpenAPI document URL/file is valid JSON or YAML and reachable from this machine.".to_string());
            if lowered.contains("401")
                || lowered.contains("403")
                || lowered.contains("unauthorized")
            {
                hints.push("The spec endpoint likely needs auth. Re-check `--auth-header` values or fetch the spec once manually first.".to_string());
            }
        }
        SourceType::Graphql => {
            hints.push("Verify the GraphQL endpoint is reachable and supports the schema/introspection flow expected by `sxmc graphql`.".to_string());
            if lowered.contains("401")
                || lowered.contains("403")
                || lowered.contains("unauthorized")
            {
                hints.push("The GraphQL endpoint rejected auth during validation. Re-check tokens and headers.".to_string());
            }
            if lowered.contains("introspection")
                || lowered.contains("schema")
                || lowered.contains("field")
            {
                hints.push("If introspection is disabled in production, validate against a staging/schema endpoint or save the bake with `--skip-validate` until a schema source is available.".to_string());
            }
        }
    }

    hints.push("If you intentionally want to save an offline or placeholder target, re-run with `--skip-validate`.".to_string());

    let mut message = base.to_string();
    if !hints.is_empty() {
        message.push_str("\nHints:");
        for hint in hints {
            message.push_str("\n- ");
            message.push_str(&hint);
        }
    }
    message
}

fn print_write_outcomes(outcomes: &[cli_surfaces::WriteOutcome]) {
    let mut created = 0usize;
    let mut updated = 0usize;
    let mut skipped = 0usize;
    let mut removed = 0usize;

    for outcome in outcomes {
        match outcome.mode {
            ArtifactMode::Preview => {}
            ArtifactMode::WriteSidecar => {
                let verb = match outcome.status {
                    cli_surfaces::WriteStatus::Created => "Created sidecar for",
                    cli_surfaces::WriteStatus::Updated => "Updated sidecar for",
                    cli_surfaces::WriteStatus::Skipped => "Skipped unchanged sidecar for",
                    cli_surfaces::WriteStatus::Removed => "Removed sidecar for",
                };
                println!("{} {}: {}", verb, outcome.label, outcome.path.display());
                match outcome.status {
                    cli_surfaces::WriteStatus::Created => created += 1,
                    cli_surfaces::WriteStatus::Updated => updated += 1,
                    cli_surfaces::WriteStatus::Skipped => skipped += 1,
                    cli_surfaces::WriteStatus::Removed => removed += 1,
                }
            }
            ArtifactMode::Patch => {}
            ArtifactMode::Apply => {
                let verb = match outcome.status {
                    cli_surfaces::WriteStatus::Created => "Created",
                    cli_surfaces::WriteStatus::Updated => "Updated",
                    cli_surfaces::WriteStatus::Skipped => "Skipped unchanged",
                    cli_surfaces::WriteStatus::Removed => "Removed",
                };
                println!("{} {}: {}", verb, outcome.label, outcome.path.display());
                match outcome.status {
                    cli_surfaces::WriteStatus::Created => created += 1,
                    cli_surfaces::WriteStatus::Updated => updated += 1,
                    cli_surfaces::WriteStatus::Skipped => skipped += 1,
                    cli_surfaces::WriteStatus::Removed => removed += 1,
                }
            }
        }
    }

    let total = created + updated + skipped + removed;
    if total > 0 {
        println!(
            "Summary: Created {}, Updated {}, Skipped unchanged {}, Removed {}",
            created, updated, skipped, removed
        );
    }
}

fn print_remove_outcomes(outcomes: &[cli_surfaces::WriteOutcome]) {
    for outcome in outcomes {
        match outcome.mode {
            ArtifactMode::Preview => {}
            ArtifactMode::WriteSidecar | ArtifactMode::Apply => {
                println!("Removed {}: {}", outcome.label, outcome.path.display());
            }
            ArtifactMode::Patch => {}
        }
    }
}

fn ensure_profile_ready_for_agent_docs(
    profile: &cli_surfaces::CliSurfaceProfile,
    allow_low_confidence: bool,
) -> Result<()> {
    let report = profile.quality_report();
    if report.ready_for_agent_docs || allow_low_confidence {
        return Ok(());
    }

    let reasons = if report.reasons.is_empty() {
        "CLI profile confidence is too low for startup-doc generation.".to_string()
    } else {
        report
            .reasons
            .into_iter()
            .map(|reason| format!("- {}", reason))
            .collect::<Vec<_>>()
            .join("\n")
    };

    Err(sxmc::error::SxmcError::Other(format!(
        "Refusing to generate startup-facing agent docs from a low-confidence CLI profile.\n{}\nUse --allow-low-confidence to force generation or inspect with --depth 1 for a richer profile.",
        reasons
    )))
}

struct DoctorRepairOptions<'a> {
    root: &'a std::path::Path,
    only_hosts: &'a [AiClientProfile],
    from_cli: &'a str,
    depth: usize,
    skills_path: &'a std::path::Path,
    allow_low_confidence: bool,
    dry_run: bool,
    remove: bool,
}

fn repair_doctor_startup_files(
    options: DoctorRepairOptions<'_>,
) -> Result<Vec<cli_surfaces::WriteOutcome>> {
    if options.only_hosts.is_empty() {
        return Err(sxmc::error::SxmcError::Other(
            "`sxmc doctor --fix` requires at least one `--only <host>` selection".into(),
        ));
    }

    let profile = cli_surfaces::inspect_cli_with_depth(options.from_cli, true, options.depth)?;
    let (artifacts, selected_hosts) = resolve_cli_ai_init_artifacts(
        &profile,
        AiCoverage::Full,
        None,
        options.only_hosts,
        options.root,
        options.skills_path,
        ArtifactMode::Apply,
    )?;
    if options.remove {
        if options.dry_run {
            cli_surfaces::remove_artifacts_with_apply_selection(
                &artifacts,
                ArtifactMode::Preview,
                options.root,
                &selected_hosts,
            )
        } else {
            cli_surfaces::remove_artifacts_with_apply_selection(
                &artifacts,
                ArtifactMode::Apply,
                options.root,
                &selected_hosts,
            )
        }
    } else {
        ensure_profile_ready_for_agent_docs(&profile, options.allow_low_confidence)?;
        if options.dry_run {
            cli_surfaces::preview_artifacts_with_apply_selection(
                &artifacts,
                ArtifactMode::Apply,
                options.root,
                &selected_hosts,
            )
        } else {
            cli_surfaces::materialize_artifacts_with_apply_selection(
                &artifacts,
                ArtifactMode::Apply,
                options.root,
                &selected_hosts,
            )
        }
    }
}

fn require_cli_ai_client(
    coverage: AiCoverage,
    client: Option<AiClientProfile>,
) -> Result<AiClientProfile> {
    match (coverage, client) {
        (AiCoverage::Single, Some(client)) => Ok(client),
        (AiCoverage::Single, None) => Err(sxmc::error::SxmcError::Other(
            "Single-host CLI->AI generation requires --client".into(),
        )),
        (AiCoverage::Full, Some(client)) => Ok(client),
        (AiCoverage::Full, None) => Ok(AiClientProfile::ClaudeCode),
    }
}

fn validate_full_apply_hosts(
    mode: ArtifactMode,
    coverage: AiCoverage,
    hosts: &[AiClientProfile],
) -> Result<()> {
    if coverage == AiCoverage::Full && mode == ArtifactMode::Apply && hosts.is_empty() {
        return Err(sxmc::error::SxmcError::Other(
            "Full-coverage apply requires at least one --host so sxmc knows which native files to update".into(),
        ));
    }
    Ok(())
}

fn ai_client_display_name(client: AiClientProfile) -> &'static str {
    match client {
        AiClientProfile::ClaudeCode => "Claude Code",
        AiClientProfile::Cursor => "Cursor",
        AiClientProfile::GeminiCli => "Gemini CLI",
        AiClientProfile::GithubCopilot => "GitHub Copilot",
        AiClientProfile::ContinueDev => "Continue",
        AiClientProfile::OpenCode => "OpenCode",
        AiClientProfile::JetbrainsAiAssistant => "JetBrains AI Assistant",
        AiClientProfile::Junie => "Junie",
        AiClientProfile::Windsurf => "Windsurf",
        AiClientProfile::OpenaiCodex => "OpenAI/Codex",
        AiClientProfile::GenericStdioMcp => "Generic stdio MCP",
        AiClientProfile::GenericHttpMcp => "Generic HTTP MCP",
    }
}

fn resolve_cli_ai_init_artifacts(
    profile: &cli_surfaces::CliSurfaceProfile,
    coverage: AiCoverage,
    client: Option<AiClientProfile>,
    hosts: &[AiClientProfile],
    root: &std::path::Path,
    skills_path: &std::path::Path,
    mode: ArtifactMode,
) -> Result<(Vec<cli_surfaces::GeneratedArtifact>, Vec<AiClientProfile>)> {
    validate_full_apply_hosts(mode, coverage, hosts)?;
    match coverage {
        AiCoverage::Single => {
            let client = require_cli_ai_client(coverage, client)?;
            let profile_artifact = cli_surfaces::generate_profile_artifact(profile, root)?;
            let agent_doc = cli_surfaces::generate_agent_doc_artifact(profile, client, root);
            let mut artifacts = vec![profile_artifact, agent_doc];
            if let Some(client_config) =
                cli_surfaces::generate_client_config_artifact(profile, client, root, skills_path)
            {
                artifacts.push(client_config);
            }
            Ok((artifacts, vec![client]))
        }
        AiCoverage::Full => Ok((
            cli_surfaces::generate_full_coverage_init_artifacts(profile, root, skills_path)?,
            hosts.to_vec(),
        )),
    }
}

fn resolve_cli_ai_agent_doc_artifacts(
    profile: &cli_surfaces::CliSurfaceProfile,
    coverage: AiCoverage,
    client: Option<AiClientProfile>,
    hosts: &[AiClientProfile],
    root: &std::path::Path,
    mode: ArtifactMode,
) -> Result<(Vec<cli_surfaces::GeneratedArtifact>, Vec<AiClientProfile>)> {
    validate_full_apply_hosts(mode, coverage, hosts)?;
    match coverage {
        AiCoverage::Single => {
            let client = require_cli_ai_client(coverage, client)?;
            Ok((
                vec![cli_surfaces::generate_agent_doc_artifact(
                    profile, client, root,
                )],
                vec![client],
            ))
        }
        AiCoverage::Full => {
            let mut artifacts = vec![cli_surfaces::generate_portable_agent_doc_artifact(
                profile, root,
            )];
            artifacts.extend(cli_surfaces::generate_host_native_agent_doc_artifacts(
                profile, root,
            ));
            Ok((artifacts, hosts.to_vec()))
        }
    }
}

fn resolve_cli_ai_client_config_artifacts(
    profile: &cli_surfaces::CliSurfaceProfile,
    coverage: AiCoverage,
    client: Option<AiClientProfile>,
    hosts: &[AiClientProfile],
    root: &std::path::Path,
    skills_path: &std::path::Path,
    mode: ArtifactMode,
) -> Result<(Vec<cli_surfaces::GeneratedArtifact>, Vec<AiClientProfile>)> {
    validate_full_apply_hosts(mode, coverage, hosts)?;
    match coverage {
        AiCoverage::Single => {
            let client = require_cli_ai_client(coverage, client)?;
            let artifact =
                cli_surfaces::generate_client_config_artifact(profile, client, root, skills_path)
                    .ok_or_else(|| {
                    sxmc::error::SxmcError::Other(format!(
                        "{} does not have a native MCP config target in sxmc",
                        ai_client_display_name(client)
                    ))
                })?;
            Ok((vec![artifact], vec![client]))
        }
        AiCoverage::Full => {
            let mut artifacts = Vec::new();
            for client in [
                AiClientProfile::ClaudeCode,
                AiClientProfile::Cursor,
                AiClientProfile::GeminiCli,
                AiClientProfile::GithubCopilot,
                AiClientProfile::ContinueDev,
                AiClientProfile::OpenCode,
                AiClientProfile::JetbrainsAiAssistant,
                AiClientProfile::Junie,
                AiClientProfile::Windsurf,
                AiClientProfile::OpenaiCodex,
                AiClientProfile::GenericStdioMcp,
                AiClientProfile::GenericHttpMcp,
            ] {
                if let Some(artifact) = cli_surfaces::generate_client_config_artifact(
                    profile,
                    client,
                    root,
                    skills_path,
                ) {
                    artifacts.push(artifact);
                }
            }
            Ok((artifacts, hosts.to_vec()))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            paths,
            watch,
            transport,
            port,
            host,
            require_headers,
            bearer_token,
            max_concurrency,
            max_request_bytes,
        } => {
            let search_paths = resolve_paths(paths);
            let required_headers = parse_headers(&require_headers)?;
            let bearer_token = parse_optional_secret(bearer_token)?;
            let limits = HttpServeLimits {
                max_concurrency,
                max_request_body_bytes: max_request_bytes,
            };
            match transport.as_str() {
                "stdio" => {
                    if !required_headers.is_empty() || bearer_token.is_some() {
                        eprintln!(
                            "[sxmc] Warning: remote auth flags are ignored for stdio transport"
                        );
                    }
                    server::serve_stdio(&search_paths, watch).await?
                }
                "http" | "sse" => {
                    server::serve_http(
                        &search_paths,
                        &host,
                        port,
                        &required_headers,
                        bearer_token.as_deref(),
                        watch,
                        limits,
                    )
                    .await?
                }
                other => {
                    eprintln!("[sxmc] Unknown transport: {}", other);
                    std::process::exit(1);
                }
            }
        }

        Commands::Wrap {
            command,
            depth,
            transport,
            port,
            host,
            timeout_seconds,
            progress_seconds,
            working_dir,
            max_stdout_bytes,
            max_stderr_bytes,
            allow_tools,
            deny_tools,
            allow_options,
            deny_options,
            allow_positionals,
            deny_positionals,
            require_headers,
            bearer_token,
            max_concurrency,
            max_request_bytes,
            allow_self,
        } => {
            let profile = cli_surfaces::inspect_cli_with_depth(&command, allow_self, depth)?;
            let working_dir = working_dir.map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    std::env::current_dir()
                        .unwrap_or_else(|_| PathBuf::from("."))
                        .join(path)
                }
            });
            let server = server::build_wrapped_cli_server(
                &command,
                &profile,
                server::WrappedCliOptions {
                    timeout_secs: timeout_seconds,
                    progress_secs: progress_seconds,
                    working_dir: working_dir.map(|path| path.display().to_string()),
                    max_stdout_bytes,
                    max_stderr_bytes,
                    allow_tools,
                    deny_tools,
                    allow_options,
                    deny_options,
                    allow_positionals,
                    deny_positionals,
                },
            )?;
            let required_headers = parse_headers(&require_headers)?;
            let bearer_token = parse_optional_secret(bearer_token)?;
            let limits = HttpServeLimits {
                max_concurrency,
                max_request_body_bytes: max_request_bytes,
            };
            match transport.as_str() {
                "stdio" => {
                    if !required_headers.is_empty() || bearer_token.is_some() {
                        eprintln!(
                            "[sxmc] Warning: remote auth flags are ignored for stdio transport"
                        );
                    }
                    server::serve_wrapped_cli_stdio(server).await?
                }
                "http" | "sse" => {
                    server::serve_wrapped_cli_http(
                        server,
                        &host,
                        port,
                        &required_headers,
                        bearer_token.as_deref(),
                        limits,
                    )
                    .await?
                }
                other => {
                    eprintln!("[sxmc] Unknown transport: {}", other);
                    std::process::exit(1);
                }
            }
        }

        Commands::Skills { action } => match action {
            SkillsAction::List { paths, json } => {
                cmd_skills_list(&resolve_paths(paths), json)?;
            }
            SkillsAction::Info { name, paths } => {
                cmd_skills_info(&resolve_paths(paths), &name)?;
            }
            SkillsAction::Run {
                name,
                arguments,
                paths,
            } => {
                cmd_skills_run(&resolve_paths(paths), &name, &arguments).await?;
            }
            SkillsAction::Create {
                source,
                output_dir,
                auth_headers,
            } => {
                let headers = parse_headers(&auth_headers)?;
                let skill_dir =
                    generator::generate_from_openapi(&source, &output_dir, &headers).await?;
                println!("Generated skill at: {}", skill_dir.display());
            }
        },

        Commands::Stdio {
            command,
            prompt,
            resource_uri,
            args,
            list,
            list_tools,
            list_prompts,
            list_resources,
            search,
            describe,
            describe_tool,
            format,
            limit,
            pretty,
            env_vars,
            cwd,
        } => {
            let env = parse_env_vars(&env_vars);
            let client = ConnectedMcpClient::Stdio(
                mcp_stdio::StdioClient::connect(&command, &env, cwd.as_deref()).await?,
            );
            let request = McpBridgeRequest {
                prompt: prompt.as_deref(),
                resource_uri: resource_uri.as_deref(),
                args: &args,
                list,
                list_tools,
                list_prompts,
                list_resources,
                search: search.as_deref(),
                describe,
                describe_tool: describe_tool.as_deref(),
                format,
                limit,
                pretty,
            };
            let result = run_mcp_bridge_command(&client, request).await;
            finish_connected_mcp_client(client, result).await?;
        }

        Commands::Http {
            url,
            prompt,
            resource_uri,
            args,
            list,
            list_tools,
            list_prompts,
            list_resources,
            search,
            describe,
            describe_tool,
            format,
            limit,
            pretty,
            auth_headers,
            timeout_seconds,
        } => {
            let headers = parse_headers(&auth_headers)?;
            let client = ConnectedMcpClient::Http(
                mcp_http::HttpClient::connect(&url, &headers, parse_timeout(timeout_seconds))
                    .await?,
            );
            let request = McpBridgeRequest {
                prompt: prompt.as_deref(),
                resource_uri: resource_uri.as_deref(),
                args: &args,
                list,
                list_tools,
                list_prompts,
                list_resources,
                search: search.as_deref(),
                describe,
                describe_tool: describe_tool.as_deref(),
                format,
                limit,
                pretty,
            };
            let result = run_mcp_bridge_command(&client, request).await;
            finish_connected_mcp_client(client, result).await?;
        }

        Commands::Mcp { action } => match action {
            McpAction::Servers { pretty, format } => {
                let store = BakeStore::load()?;
                let servers = baked_mcp_servers(&store);

                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    let value = Value::Array(
                        servers
                            .iter()
                            .map(|config| {
                                json!({
                                    "name": config.name,
                                    "transport": match config.source_type {
                                        SourceType::Stdio => "stdio",
                                        SourceType::Http => "http",
                                        _ => "unsupported",
                                    },
                                    "source": config.source,
                                    "description": config.description,
                                })
                            })
                            .collect(),
                    );
                    println!("{}", output::format_structured_value(&value, format));
                } else if servers.is_empty() {
                    println!("No baked MCP servers found.");
                    println!("Create one with: sxmc bake create NAME --type stdio --source '...'");
                } else {
                    println!("MCP servers ({}):", servers.len());
                    for config in servers {
                        let transport = match config.source_type {
                            SourceType::Stdio => "stdio",
                            SourceType::Http => "http",
                            _ => "unsupported",
                        };
                        println!("  {} [{}]", config.name, transport);
                        if let Some(description) = &config.description {
                            println!("    {}", description);
                        }
                    }
                }
            }
            McpAction::Tools {
                server,
                search,
                limit,
            } => {
                let client = connect_named_baked_mcp_client(&server).await?;
                let result = print_mcp_tools(&client, search.as_deref(), limit).await;
                finish_connected_mcp_client(client, result).await?;
            }
            McpAction::Grep {
                pattern,
                server,
                limit,
            } => {
                let store = BakeStore::load()?;
                let mut results: Vec<(String, Tool)> = Vec::new();

                let configs: Vec<BakeConfig> = if let Some(server) = server {
                    vec![get_baked_mcp_server(&store, &server)?]
                } else {
                    baked_mcp_servers(&store).into_iter().cloned().collect()
                };

                for config in configs {
                    let server_name = config.name.clone();
                    let client = ConnectedMcpClient::connect(&config).await?;
                    let tools = client.list_tools().await?;
                    client.close().await?;

                    let pattern_lower = pattern.to_lowercase();
                    for tool in tools {
                        let name = tool.name.as_ref().to_lowercase();
                        let desc = tool.description.as_deref().unwrap_or("").to_lowercase();
                        if name.contains(&pattern_lower) || desc.contains(&pattern_lower) {
                            results.push((server_name.clone(), tool));
                        }
                    }
                }

                results.sort_by(|a, b| {
                    a.0.cmp(&b.0)
                        .then_with(|| a.1.name.as_ref().cmp(b.1.name.as_ref()))
                });

                println!("{}", format_mcp_grep_results(&results, &pattern, limit));
            }
            McpAction::Prompts { server, limit } => {
                let client = connect_named_baked_mcp_client(&server).await?;
                let result = print_mcp_prompts(&client, limit).await;
                finish_connected_mcp_client(client, result).await?;
            }
            McpAction::Resources { server, limit } => {
                let client = connect_named_baked_mcp_client(&server).await?;
                let result = print_mcp_resources(&client, limit).await;
                finish_connected_mcp_client(client, result).await?;
            }
            McpAction::Info {
                target,
                pretty,
                format,
            } => {
                let (server, tool_name) = split_server_target(&target)?;
                let client = connect_named_baked_mcp_client(server).await?;
                let result = print_mcp_tool_info(&client, tool_name, pretty, format).await;
                finish_connected_mcp_client(client, result).await?;
            }
            McpAction::Call {
                target,
                payload,
                pretty,
            } => {
                let (server, tool_name) = split_server_target(&target)?;
                let client = connect_named_baked_mcp_client(server).await?;
                let result = call_mcp_tool(
                    &client,
                    tool_name,
                    payload,
                    pretty,
                    &format!("sxmc mcp info {}/{} --format toon", server, tool_name),
                    Some(&format!("sxmc mcp session {}", server)),
                )
                .await;
                finish_connected_mcp_client(client, result).await?;
            }
            McpAction::Read { target, pretty } => {
                let (server, resource_uri) = split_server_target(&target)?;
                let client = connect_named_baked_mcp_client(server).await?;
                let result = read_mcp_resource(&client, resource_uri, pretty).await;
                finish_connected_mcp_client(client, result).await?;
            }
            McpAction::Prompt {
                target,
                args,
                pretty,
            } => {
                let (server, prompt_name) = split_server_target(&target)?;
                let client = connect_named_baked_mcp_client(server).await?;
                let result = fetch_mcp_prompt(&client, prompt_name, &args, pretty).await;
                finish_connected_mcp_client(client, result).await?;
            }
            McpAction::Session {
                server,
                script,
                quiet,
            } => {
                let client = connect_named_baked_mcp_client(&server).await?;
                let result = if let Some(script) = script {
                    let file = std::fs::File::open(&script).map_err(|e| {
                        sxmc::error::SxmcError::Other(format!(
                            "Failed to open session script '{}': {}",
                            script.display(),
                            e
                        ))
                    })?;
                    let reader = std::io::BufReader::new(file);
                    run_mcp_session(&client, reader, quiet).await
                } else {
                    let stdin = std::io::stdin();
                    let reader = stdin.lock();
                    run_mcp_session(&client, reader, quiet).await
                };
                finish_connected_mcp_client(client, result).await?;
            }
        },

        Commands::Api {
            source,
            operation,
            args,
            list,
            search,
            pretty,
            format,
            auth_headers,
            timeout_seconds,
        } => {
            let headers = parse_headers(&auth_headers)?;
            let client =
                api::ApiClient::connect(&source, &headers, parse_timeout(timeout_seconds)).await?;
            eprintln!("[sxmc] Detected {} API", client.api_type());
            let arguments = parse_string_kv_args(&args);
            cmd_api(
                &client,
                operation,
                &arguments,
                list,
                search.as_deref(),
                pretty,
                format,
            )
            .await?;
        }

        Commands::Spec {
            source,
            operation,
            args,
            list,
            search,
            pretty,
            format,
            auth_headers,
            timeout_seconds,
        } => {
            let headers = parse_headers(&auth_headers)?;
            let spec =
                openapi::OpenApiSpec::load(&source, &headers, parse_timeout(timeout_seconds))
                    .await?;
            eprintln!("[sxmc] Loaded OpenAPI spec: {}", spec.title);
            let client = api::ApiClient::OpenApi(spec);
            let arguments = parse_string_kv_args(&args);
            cmd_api(
                &client,
                operation,
                &arguments,
                list,
                search.as_deref(),
                pretty,
                format,
            )
            .await?;
        }

        Commands::Graphql {
            url,
            operation,
            args,
            list,
            search,
            pretty,
            format,
            auth_headers,
            timeout_seconds,
        } => {
            let headers = parse_headers(&auth_headers)?;
            let gql =
                graphql::GraphQLClient::connect(&url, &headers, parse_timeout(timeout_seconds))
                    .await?;
            let client = api::ApiClient::GraphQL(gql);
            let arguments = parse_string_kv_args(&args);
            cmd_api(
                &client,
                operation,
                &arguments,
                list,
                search.as_deref(),
                pretty,
                format,
            )
            .await?;
        }

        Commands::Scan {
            paths,
            skill,
            mcp_stdio: mcp_stdio_cmd,
            mcp,
            severity,
            json,
            env_vars,
        } => {
            let min_severity = match severity.to_lowercase().as_str() {
                "critical" => security::Severity::Critical,
                "error" => security::Severity::Error,
                "warn" | "warning" => security::Severity::Warning,
                _ => security::Severity::Info,
            };

            let mut reports = Vec::new();

            if let Some(ref mcp_cmd) = mcp_stdio_cmd {
                // Scan MCP server via stdio
                let env = parse_env_vars(&env_vars);
                let client = mcp_stdio::StdioClient::connect(mcp_cmd, &env, None).await?;
                let tools = client.list_tools().await?;
                let report = security::mcp_scanner::scan_tools(&tools, mcp_cmd);
                reports.push(report);
                client.close().await?;
            } else if let Some(ref mcp_url) = mcp {
                // Scan MCP server via HTTP
                let client = mcp_http::HttpClient::connect(mcp_url, &[], None).await?;
                let tools = client.list_tools().await?;
                let report = security::mcp_scanner::scan_tools(&tools, mcp_url);
                reports.push(report);
                client.close().await?;
            } else {
                // Scan skills
                let search_paths = resolve_paths(paths);
                let skill_dirs = discovery::discover_skills(&search_paths)?;

                for dir in &skill_dirs {
                    let source = dir.parent().and_then(|p| p.to_str()).unwrap_or("unknown");
                    if let Ok(parsed_skill) = parser::parse_skill(dir, source) {
                        if let Some(ref target_name) = skill {
                            if parsed_skill.name != *target_name {
                                continue;
                            }
                        }
                        let report = security::skill_scanner::scan_skill(&parsed_skill);
                        reports.push(report);
                    }
                }
            }

            // Output results
            let mut exit_code = 0;
            if json {
                let rendered_reports: Vec<Value> = reports
                    .iter()
                    .map(|report| report.filtered(min_severity).format_json())
                    .collect();

                let json_value = if rendered_reports.len() == 1 {
                    rendered_reports
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| json!({}))
                } else {
                    json!({
                        "severity": severity,
                        "count": rendered_reports.len(),
                        "reports": rendered_reports,
                    })
                };

                println!("{}", serde_json::to_string_pretty(&json_value)?);
            } else {
                for report in &reports {
                    let filtered_report = report.filtered(min_severity);
                    if filtered_report.is_clean() {
                        println!(
                            "[PASS] {} — no issues at severity >= {}",
                            report.target, severity
                        );
                    } else {
                        println!("{}", filtered_report.format_text());
                        if filtered_report.has_errors() {
                            exit_code = 1;
                        }
                    }
                }
            }

            if reports.is_empty() {
                if skill.is_some() {
                    eprintln!("Skill not found");
                    std::process::exit(1);
                }
                println!("No skills found to scan.");
            }

            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }

        Commands::Inspect { action } => match action {
            InspectAction::Cli {
                command,
                depth,
                compact,
                pretty,
                format,
                allow_self,
            } => {
                let profile = cli_surfaces::inspect_cli_with_depth(&command, allow_self, depth)?;
                let value = if compact {
                    cli_surfaces::compact_profile_value(&profile)
                } else {
                    cli_surfaces::profile_value(&profile)
                };
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&value, format));
                } else {
                    let format = output::resolve_structured_format(format, pretty);
                    println!("{}", output::format_structured_value(&value, format));
                }
            }
            InspectAction::Batch {
                commands,
                from_file,
                retry_failed,
                output_dir,
                overwrite,
                skip_existing,
                depth,
                since,
                parallel,
                progress,
                compact,
                pretty,
                format,
                allow_self,
            } => {
                let mut requests = cli_surfaces::load_batch_requests(
                    &commands,
                    from_file.as_deref(),
                    retry_failed.as_deref(),
                )?;
                for request in &mut requests {
                    if request.depth == 0 {
                        request.depth = depth;
                    }
                }
                if requests.is_empty() {
                    return Err(sxmc::error::SxmcError::Other(
                        "inspect batch requires at least one command spec or --from-file input"
                            .into(),
                    ));
                }
                let since_filter = since
                    .as_deref()
                    .map(cli_surfaces::parse_batch_since_filter)
                    .transpose()?;
                let mut written_profiles = Vec::new();
                let mut slug_counts = HashMap::new();
                let write_mode = if overwrite {
                    BatchOutputWriteMode::Overwrite
                } else if skip_existing {
                    BatchOutputWriteMode::SkipExisting
                } else {
                    BatchOutputWriteMode::Unique
                };
                let output_dir = output_dir.map(|path| {
                    if path.is_absolute() {
                        path
                    } else {
                        std::env::current_dir()
                            .unwrap_or_else(|_| PathBuf::from("."))
                            .join(path)
                    }
                });
                let preferred_format = output::prefer_structured_output(format, pretty);
                let mut value = if matches!(
                    preferred_format,
                    Some(output::StructuredOutputFormat::Ndjson)
                ) {
                    let output_dir_ref = output_dir.clone();
                    let mut stream_error: Option<sxmc::error::SxmcError> = None;
                    let value = cli_surfaces::inspect_cli_batch_with_callback(
                        &requests,
                        allow_self,
                        parallel,
                        progress,
                        since_filter.as_ref(),
                        |event| {
                            if stream_error.is_some() {
                                return;
                            }
                            if let Some(dir) = output_dir_ref.as_ref() {
                                if event["type"] == "profile" {
                                    match write_batch_profile_file(
                                        dir,
                                        event["command"].as_str().unwrap_or("<unknown>"),
                                        &event["profile"],
                                        compact,
                                        &mut slug_counts,
                                        write_mode,
                                    ) {
                                        Ok(metadata) => written_profiles.push(metadata),
                                        Err(error) => stream_error = Some(error),
                                    }
                                }
                            }
                            println!(
                                "{}",
                                output::format_structured_value(
                                    &batch_event_for_output(event, compact),
                                    output::StructuredOutputFormat::Ndjson,
                                )
                            );
                        },
                    );
                    if let Some(error) = stream_error {
                        return Err(error);
                    }
                    value
                } else {
                    cli_surfaces::inspect_cli_batch(
                        &requests,
                        allow_self,
                        parallel,
                        progress,
                        since_filter.as_ref(),
                    )
                };
                if let Some(dir) = output_dir.as_ref() {
                    if !matches!(
                        preferred_format,
                        Some(output::StructuredOutputFormat::Ndjson)
                    ) {
                        if let Some(profiles) = value["profiles"].as_array() {
                            for profile in profiles {
                                written_profiles.push(write_batch_profile_file(
                                    dir,
                                    profile["command"].as_str().unwrap_or("<unknown>"),
                                    profile,
                                    compact,
                                    &mut slug_counts,
                                    write_mode,
                                )?);
                            }
                        }
                    }
                    attach_batch_output_dir_metadata(&mut value, dir, &written_profiles);
                    let manifest_path = write_batch_manifest_file(dir, &value)?;
                    if let Some(object) = value.as_object_mut() {
                        object.insert(
                            "written_manifest_path".into(),
                            Value::String(manifest_path.display().to_string()),
                        );
                    }
                }
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    if matches!(format, output::StructuredOutputFormat::Ndjson) {
                        println!(
                            "{}",
                            output::format_structured_value(
                                &json!({
                                    "type": "summary",
                                    "count": value["count"],
                                    "inspected_count": value["inspected_count"],
                                    "parallelism": value["parallelism"],
                                    "success_count": value["success_count"],
                                    "failed_count": value["failed_count"],
                                    "skipped_count": value["skipped_count"],
                                    "output_dir": value.get("output_dir").cloned().unwrap_or(Value::Null),
                                    "written_profile_count": value.get("written_profile_count").cloned().unwrap_or(Value::from(0)),
                                    "skipped_existing_count": value.get("skipped_existing_count").cloned().unwrap_or(Value::from(0)),
                                    "written_manifest_path": value.get("written_manifest_path").cloned().unwrap_or(Value::Null),
                                }),
                                output::StructuredOutputFormat::Ndjson,
                            )
                        );
                        return Ok(());
                    }
                    if matches!(format, output::StructuredOutputFormat::Toon) {
                        println!("{}", format_batch_toon(&value, compact));
                        return Ok(());
                    }
                    let rendered = if compact {
                        let compact_profiles = value["profiles"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|profile| {
                                serde_json::from_value::<cli_surfaces::CliSurfaceProfile>(
                                    profile.clone(),
                                )
                                .ok()
                            })
                            .map(|profile| cli_surfaces::compact_profile_value(&profile))
                            .collect::<Vec<_>>();
                        let compact_value = json!({
                            "count": value["count"],
                            "parallelism": value["parallelism"],
                            "success_count": value["success_count"],
                            "failed_count": value["failed_count"],
                            "skipped_count": value["skipped_count"],
                            "profiles": compact_profiles,
                            "failures": value["failures"],
                            "skipped": value["skipped"],
                        });
                        output::format_structured_value(&compact_value, format)
                    } else {
                        output::format_structured_value(&value, format)
                    };
                    println!("{rendered}");
                } else {
                    print_batch_inspect_report(&value, compact);
                }
            }
            InspectAction::Diff {
                command,
                before,
                after,
                depth,
                exit_code,
                watch,
                pretty,
                format,
                allow_self,
            } => {
                let render_format = resolve_diff_output_format(format, pretty);
                let render_once = || -> Result<Value> {
                    let before_profile = cli_surfaces::load_profile(&before)?;
                    let after_profile = if let Some(after_path) = after.as_ref() {
                        cli_surfaces::load_profile(after_path)?
                    } else {
                        let command = command.as_deref().ok_or_else(|| {
                            sxmc::error::SxmcError::Other(
                                "inspect diff requires either a live <command> or `--after <profile.json>`".into(),
                            )
                        })?;
                        cli_surfaces::inspect_cli_with_depth(command, allow_self, depth)?
                    };
                    Ok(cli_surfaces::diff_profile_value(
                        &before_profile,
                        &after_profile,
                    ))
                };
                if let Some(interval) = watch {
                    let interval = Duration::from_secs(interval.max(1));
                    let mut last_rendered = None::<String>;
                    loop {
                        let value = render_once()?;
                        let rendered = diff_display_value(&value, render_format);
                        if last_rendered.as_ref() != Some(&rendered) {
                            println!("{rendered}");
                            println!();
                            std::io::stdout().flush()?;
                            last_rendered = Some(rendered);
                        }
                        std::thread::sleep(interval);
                    }
                } else {
                    let value = render_once()?;
                    println!("{}", diff_display_value(&value, render_format));
                    if exit_code && diff_value_has_changes(&value) {
                        std::process::exit(1);
                    }
                }
            }
            InspectAction::Profile {
                input,
                compact,
                pretty,
                format,
            } => {
                let profile = cli_surfaces::load_profile(&input)?;
                let value = if compact {
                    cli_surfaces::compact_profile_value(&profile)
                } else {
                    cli_surfaces::profile_value(&profile)
                };
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&value, format));
                } else {
                    let format = output::resolve_structured_format(format, pretty);
                    println!("{}", output::format_structured_value(&value, format));
                }
            }
            InspectAction::MigrateProfile {
                input,
                output: migrate_output,
                pretty,
                format,
            } => {
                let profile = cli_surfaces::load_profile(&input)?;
                let value = cli_surfaces::profile_value(&profile);
                if let Some(path) = migrate_output {
                    let path = if path.is_absolute() {
                        path
                    } else {
                        std::env::current_dir()
                            .unwrap_or_else(|_| PathBuf::from("."))
                            .join(path)
                    };
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&path, serde_json::to_string_pretty(&value)?)?;
                    let report = json!({
                        "command": profile.command,
                        "input": input.display().to_string(),
                        "output": path.display().to_string(),
                        "profile_schema": profile.profile_schema,
                    });
                    if let Some(format) = output::prefer_structured_output(format, pretty) {
                        println!("{}", output::format_structured_value(&report, format));
                    } else {
                        print_migrated_profile_report(&report);
                    }
                } else if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&value, format));
                } else {
                    let format = output::resolve_structured_format(format, pretty);
                    println!("{}", output::format_structured_value(&value, format));
                }
            }
            InspectAction::Drift {
                inputs,
                root,
                recursive,
                exit_code,
                pretty,
                format,
                allow_self,
            } => {
                let root = resolve_generation_root(root)?;
                let use_default_recursive = inputs.is_empty();
                let profile_inputs = if use_default_recursive {
                    vec![default_saved_profiles_dir(&root)]
                } else {
                    inputs
                        .into_iter()
                        .map(|path| {
                            if path.is_absolute() {
                                path
                            } else {
                                root.join(path)
                            }
                        })
                        .collect::<Vec<_>>()
                };
                let profile_paths =
                    collect_profile_paths(&profile_inputs, recursive || use_default_recursive)?;
                let value = drift_value(&profile_paths, allow_self);
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&value, format));
                } else {
                    print_drift_report(&value);
                }
                if exit_code && value["changed_count"].as_u64().unwrap_or(0) > 0 {
                    std::process::exit(1);
                }
            }
            InspectAction::BundleExport {
                inputs,
                root,
                recursive,
                bundle_name,
                description,
                role,
                hosts,
                output,
                signature_secret,
                pretty,
                format,
            } => {
                let root = resolve_generation_root(root)?;
                let signature_secret = parse_optional_secret(signature_secret)?;
                let use_default_recursive = inputs.is_empty();
                let profile_inputs = if use_default_recursive {
                    vec![default_saved_profiles_dir(&root)]
                } else {
                    inputs
                        .into_iter()
                        .map(|path| {
                            if path.is_absolute() {
                                path
                            } else {
                                root.join(path)
                            }
                        })
                        .collect::<Vec<_>>()
                };
                let profile_paths =
                    collect_profile_paths(&profile_inputs, recursive || use_default_recursive)?;
                let value = sign_bundle_value(
                    export_profile_bundle_value(
                        &profile_paths,
                        bundle_name.as_deref(),
                        description.as_deref(),
                        role.as_deref(),
                        &hosts,
                    )?,
                    signature_secret.as_deref(),
                )?;
                let output_path = if output.is_absolute() {
                    output
                } else {
                    root.join(output)
                };
                if let Some(parent) = output_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&output_path, serde_json::to_string_pretty(&value)?)?;
                let sha256 = bundle_sha256_from_value(&value)?;
                let report = json!({
                    "bundle_schema": PROFILE_BUNDLE_SCHEMA,
                    "output": output_path.display().to_string(),
                    "profile_count": value["profile_count"],
                    "sha256": sha256,
                    "signature": bundle_signature_report(&value),
                    "metadata": value["metadata"],
                    "entries": value["entries"],
                });
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&report, format));
                } else {
                    println!(
                        "Exported {} profiles to {}",
                        report["profile_count"].as_u64().unwrap_or(0),
                        report["output"].as_str().unwrap_or("<unknown>")
                    );
                }
            }
            InspectAction::BundleImport {
                input,
                root,
                output_dir,
                overwrite,
                skip_existing,
                pretty,
                format,
            } => {
                let root = resolve_generation_root(root)?;
                let output_dir = output_dir.unwrap_or_else(|| default_saved_profiles_dir(&root));
                let output_dir = if output_dir.is_absolute() {
                    output_dir
                } else {
                    root.join(output_dir)
                };
                let mode = if overwrite {
                    BundleImportMode::Overwrite
                } else if skip_existing {
                    BundleImportMode::SkipExisting
                } else {
                    BundleImportMode::Unique
                };
                let value = import_profile_bundle_value(&input, &output_dir, mode)?;
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&value, format));
                } else {
                    println!(
                        "Imported {} profiles into {} ({} skipped)",
                        value["imported_count"].as_u64().unwrap_or(0),
                        value["output_dir"].as_str().unwrap_or("<unknown>"),
                        value["skipped_count"].as_u64().unwrap_or(0)
                    );
                }
            }
            InspectAction::BundleVerify {
                input,
                auth_headers,
                timeout_seconds,
                expected_sha256,
                signature_secret,
                pretty,
                format,
            } => {
                let headers = parse_headers(&auth_headers)?;
                let signature_secret = parse_optional_secret(signature_secret)?;
                let bundle_value =
                    read_bundle_source(&input, &headers, parse_timeout(timeout_seconds)).await?;
                let sha256 =
                    verify_bundle_digest(&bundle_value, expected_sha256.as_deref(), &input)?;
                let signature =
                    verify_bundle_signature(&bundle_value, signature_secret.as_deref(), &input)?;
                let report = json!({
                    "bundle_schema": PROFILE_BUNDLE_SCHEMA,
                    "input": input,
                    "sha256": sha256,
                    "signature": signature,
                    "verified": true,
                    "expected_sha256": expected_sha256,
                    "profile_count": bundle_value["profile_count"],
                    "metadata": bundle_value.get("metadata").cloned().unwrap_or(Value::Null),
                });
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&report, format));
                } else {
                    println!(
                        "Verified bundle {} ({} profiles)",
                        report["input"].as_str().unwrap_or("<unknown>"),
                        report["profile_count"].as_u64().unwrap_or(0)
                    );
                }
            }
            InspectAction::ExportCorpus {
                inputs,
                root,
                recursive,
                output,
                pretty,
                format,
            } => {
                let root = resolve_generation_root(root)?;
                let use_default_recursive = inputs.is_empty();
                let profile_inputs = if use_default_recursive {
                    vec![default_saved_profiles_dir(&root)]
                } else {
                    inputs
                        .into_iter()
                        .map(|path| {
                            if path.is_absolute() {
                                path
                            } else {
                                root.join(path)
                            }
                        })
                        .collect::<Vec<_>>()
                };
                let profile_paths =
                    collect_profile_paths(&profile_inputs, recursive || use_default_recursive)?;
                let value = export_profile_corpus_value(&profile_paths);
                let render_format = output::resolve_structured_format(format, pretty);
                let rendered = if matches!(render_format, output::StructuredOutputFormat::Ndjson) {
                    let entries =
                        Value::Array(value["entries"].as_array().cloned().unwrap_or_default());
                    output::format_structured_value(
                        &entries,
                        output::StructuredOutputFormat::Ndjson,
                    )
                } else {
                    output::format_structured_value(&value, render_format)
                };
                if let Some(output_path) = output {
                    let output_path = if output_path.is_absolute() {
                        output_path
                    } else {
                        root.join(output_path)
                    };
                    if let Some(parent) = output_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&output_path, rendered)?;
                    let report = json!({
                        "corpus_schema": PROFILE_CORPUS_SCHEMA,
                        "output": output_path.display().to_string(),
                        "count": value["count"],
                        "error_count": value["error_count"],
                    });
                    if let Some(format) = output::prefer_structured_output(format, pretty) {
                        println!("{}", output::format_structured_value(&report, format));
                    } else {
                        println!(
                            "Exported {} corpus entries to {}",
                            report["count"].as_u64().unwrap_or(0),
                            report["output"].as_str().unwrap_or("<unknown>")
                        );
                    }
                } else {
                    println!("{rendered}");
                }
            }
            InspectAction::CorpusStats {
                input,
                pretty,
                format,
            } => {
                let value = load_corpus_value(&input)?;
                let stats = corpus_stats_value(&value);
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&stats, format));
                } else {
                    print_corpus_stats_report(&stats);
                }
            }
            InspectAction::CorpusQuery {
                input,
                command,
                search,
                limit,
                pretty,
                format,
            } => {
                let value = load_corpus_value(&input)?;
                let query =
                    corpus_query_value(&value, command.as_deref(), search.as_deref(), limit);
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&query, format));
                } else {
                    print_corpus_query_report(&query);
                }
            }
            InspectAction::CacheStats { pretty, format } => {
                let value = cli_surfaces::cache_stats_value()?;
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&value, format));
                } else {
                    print_cache_stats_report(&value);
                }
            }
            InspectAction::CacheClear { pretty, format } => {
                let value = cli_surfaces::clear_profile_cache_value()?;
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&value, format));
                } else {
                    println!(
                        "Cleared CLI profile cache at {} ({} entries remain, {} bytes)",
                        value["path"].as_str().unwrap_or("<unknown>"),
                        value["entry_count"].as_u64().unwrap_or(0),
                        value["total_bytes"].as_u64().unwrap_or(0)
                    );
                }
            }
            InspectAction::CacheInvalidate {
                command,
                dry_run,
                pretty,
                format,
            } => {
                let value = cli_surfaces::invalidate_profile_cache_value(&command, dry_run)?;
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&value, format));
                } else {
                    if value["dry_run"].as_bool().unwrap_or(false) {
                        println!(
                            "Would invalidate {} cached profile entries for `{}` ({} entries would remain)",
                            value["matched_entries"].as_u64().unwrap_or(0),
                            value["command"].as_str().unwrap_or("<unknown>"),
                            value["remaining_entries"].as_u64().unwrap_or(0)
                        );
                    } else {
                        println!(
                            "Invalidated {} cached profile entries for `{}` ({} entries remain)",
                            value["removed_entries"].as_u64().unwrap_or(0),
                            value["command"].as_str().unwrap_or("<unknown>"),
                            value["remaining_entries"].as_u64().unwrap_or(0)
                        );
                    }
                }
            }
            InspectAction::CacheWarm {
                commands,
                from_file,
                depth,
                since,
                parallel,
                progress,
                pretty,
                format,
                allow_self,
            } => {
                let mut requests =
                    cli_surfaces::load_batch_requests(&commands, from_file.as_deref(), None)?;
                for request in &mut requests {
                    if request.depth == 0 {
                        request.depth = depth;
                    }
                }
                if requests.is_empty() {
                    return Err(sxmc::error::SxmcError::Other(
                        "inspect cache-warm requires at least one command spec or --from-file input"
                            .into(),
                    ));
                }
                let since_filter = since
                    .as_deref()
                    .map(cli_surfaces::parse_batch_since_filter)
                    .transpose()?;
                let value = cli_surfaces::warm_profile_cache(
                    &requests,
                    allow_self,
                    parallel,
                    progress,
                    since_filter.as_ref(),
                );
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    println!("{}", output::format_structured_value(&value, format));
                } else {
                    print_cache_warm_report(&value);
                }
            }
        },

        Commands::Publish {
            target,
            inputs,
            root,
            recursive,
            bundle_name,
            description,
            role,
            hosts,
            auth_headers,
            timeout_seconds,
            signature_secret,
            pretty,
            format,
        } => {
            let root = resolve_generation_root(root)?;
            let signature_secret = parse_optional_secret(signature_secret)?;
            let use_default_recursive = inputs.is_empty();
            let profile_inputs = if use_default_recursive {
                vec![default_saved_profiles_dir(&root)]
            } else {
                inputs
                    .into_iter()
                    .map(|path| {
                        if path.is_absolute() {
                            path
                        } else {
                            root.join(path)
                        }
                    })
                    .collect::<Vec<_>>()
            };
            let profile_paths =
                collect_profile_paths(&profile_inputs, recursive || use_default_recursive)?;
            let bundle_value = sign_bundle_value(
                export_profile_bundle_value(
                    &profile_paths,
                    bundle_name.as_deref(),
                    description.as_deref(),
                    role.as_deref(),
                    &hosts,
                )?,
                signature_secret.as_deref(),
            )?;
            let resolved_target = if is_http_target(&target) || target.starts_with("file://") {
                target.clone()
            } else {
                root.join(&target).display().to_string()
            };
            let headers = parse_headers(&auth_headers)?;
            let destination = publish_bundle_target(
                &resolved_target,
                &bundle_value,
                &headers,
                parse_timeout(timeout_seconds),
            )
            .await?;
            let sha256 = bundle_sha256_from_value(&bundle_value)?;
            let report = json!({
                "bundle_schema": PROFILE_BUNDLE_SCHEMA,
                "target": destination["target"],
                "transport": destination["transport"],
                "http_status": destination.get("http_status").cloned().unwrap_or(Value::Null),
                "profile_count": bundle_value["profile_count"],
                "sha256": sha256,
                "signature": bundle_signature_report(&bundle_value),
                "metadata": bundle_value["metadata"],
                "entries": bundle_value["entries"],
            });
            if let Some(format) = output::prefer_structured_output(format, pretty) {
                println!("{}", output::format_structured_value(&report, format));
            } else {
                println!(
                    "Published {} profiles to {}",
                    report["profile_count"].as_u64().unwrap_or(0),
                    report["target"].as_str().unwrap_or("<unknown>")
                );
            }
        }

        Commands::Pull {
            source,
            root,
            output_dir,
            overwrite,
            skip_existing,
            auth_headers,
            timeout_seconds,
            expected_sha256,
            signature_secret,
            pretty,
            format,
        } => {
            let root = resolve_generation_root(root)?;
            let signature_secret = parse_optional_secret(signature_secret)?;
            let output_dir = output_dir.unwrap_or_else(|| default_saved_profiles_dir(&root));
            let output_dir = if output_dir.is_absolute() {
                output_dir
            } else {
                root.join(output_dir)
            };
            let mode = if overwrite {
                BundleImportMode::Overwrite
            } else if skip_existing {
                BundleImportMode::SkipExisting
            } else {
                BundleImportMode::Unique
            };
            let resolved_source = if is_http_target(&source) || source.starts_with("file://") {
                source.clone()
            } else {
                root.join(&source).display().to_string()
            };
            let headers = parse_headers(&auth_headers)?;
            let bundle_value =
                read_bundle_source(&resolved_source, &headers, parse_timeout(timeout_seconds))
                    .await?;
            let sha256 =
                verify_bundle_digest(&bundle_value, expected_sha256.as_deref(), &resolved_source)?;
            let signature = verify_bundle_signature(
                &bundle_value,
                signature_secret.as_deref(),
                &resolved_source,
            )?;
            let value = import_profile_bundle_from_value(
                &resolved_source,
                bundle_value,
                &output_dir,
                mode,
            )?;
            let mut report = value;
            if let Some(object) = report.as_object_mut() {
                object.insert("sha256".into(), Value::from(sha256));
                object.insert("signature".into(), signature);
                object.insert(
                    "expected_sha256".into(),
                    expected_sha256.map(Value::from).unwrap_or(Value::Null),
                );
            }
            if let Some(format) = output::prefer_structured_output(format, pretty) {
                println!("{}", output::format_structured_value(&report, format));
            } else {
                println!(
                    "Pulled {} profiles from {} into {}",
                    report["imported_count"].as_u64().unwrap_or(0),
                    report["input"].as_str().unwrap_or("<unknown>"),
                    report["output_dir"].as_str().unwrap_or("<unknown>")
                );
            }
        }

        Commands::Init { action } => match action {
            InitAction::Ai {
                from_cli,
                depth,
                coverage,
                client,
                hosts,
                skills_path,
                root,
                mode,
                remove,
                allow_low_confidence,
                allow_self,
            } => {
                let root = resolve_generation_root(root)?;
                let profile = cli_surfaces::inspect_cli_with_depth(&from_cli, allow_self, depth)?;
                if !remove {
                    ensure_profile_ready_for_agent_docs(&profile, allow_low_confidence)?;
                }
                let (artifacts, selected_hosts) = resolve_cli_ai_init_artifacts(
                    &profile,
                    coverage,
                    client,
                    &hosts,
                    &root,
                    &skills_path,
                    mode,
                )?;
                if remove {
                    let outcomes = cli_surfaces::remove_artifacts_with_apply_selection(
                        &artifacts,
                        mode,
                        &root,
                        &selected_hosts,
                    )?;
                    print_remove_outcomes(&outcomes);
                } else {
                    let outcomes = cli_surfaces::materialize_artifacts_with_apply_selection(
                        &artifacts,
                        mode,
                        &root,
                        &selected_hosts,
                    )?;
                    print_write_outcomes(&outcomes);
                }
            }
        },

        Commands::Scaffold { action } => match action {
            ScaffoldAction::Ci {
                from_profile,
                root,
                output_dir,
                mode,
            } => {
                let root = resolve_generation_root(root)?;
                let profile = cli_surfaces::load_profile(&from_profile)?;
                let artifact =
                    cli_surfaces::generate_ci_workflow_artifact(&profile, &root, &output_dir);
                let outcomes = cli_surfaces::materialize_artifacts(&[artifact], mode, &root)?;
                print_write_outcomes(&outcomes);
            }
            ScaffoldAction::Skill {
                from_profile,
                root,
                output_dir,
                mode,
            } => {
                let root = resolve_generation_root(root)?;
                let profile = cli_surfaces::load_profile(&from_profile)?;
                let artifacts =
                    cli_surfaces::generate_skill_artifacts(&profile, &root, &output_dir);
                let outcomes = cli_surfaces::materialize_artifacts(&artifacts, mode, &root)?;
                print_write_outcomes(&outcomes);
            }
            ScaffoldAction::AgentDoc {
                from_profile,
                coverage,
                client,
                hosts,
                root,
                mode,
                allow_low_confidence,
            } => {
                let root = resolve_generation_root(root)?;
                let profile = cli_surfaces::load_profile(&from_profile)?;
                ensure_profile_ready_for_agent_docs(&profile, allow_low_confidence)?;
                let (artifacts, selected_hosts) = resolve_cli_ai_agent_doc_artifacts(
                    &profile, coverage, client, &hosts, &root, mode,
                )?;
                let outcomes = cli_surfaces::materialize_artifacts_with_apply_selection(
                    &artifacts,
                    mode,
                    &root,
                    &selected_hosts,
                )?;
                print_write_outcomes(&outcomes);
            }
            ScaffoldAction::ClientConfig {
                from_profile,
                coverage,
                client,
                hosts,
                skills_path,
                root,
                mode,
            } => {
                let root = resolve_generation_root(root)?;
                let profile = cli_surfaces::load_profile(&from_profile)?;
                let (artifacts, selected_hosts) = resolve_cli_ai_client_config_artifacts(
                    &profile,
                    coverage,
                    client,
                    &hosts,
                    &root,
                    &skills_path,
                    mode,
                )?;
                let outcomes = cli_surfaces::materialize_artifacts_with_apply_selection(
                    &artifacts,
                    mode,
                    &root,
                    &selected_hosts,
                )?;
                print_write_outcomes(&outcomes);
            }
            ScaffoldAction::McpWrapper {
                from_profile,
                root,
                output_dir,
                mode,
            } => {
                let root = resolve_generation_root(root)?;
                let profile = cli_surfaces::load_profile(&from_profile)?;
                let artifacts =
                    cli_surfaces::generate_mcp_wrapper_artifacts(&profile, &root, &output_dir)?;
                let outcomes = cli_surfaces::materialize_artifacts(&artifacts, mode, &root)?;
                print_write_outcomes(&outcomes);
            }
            ScaffoldAction::LlmTxt {
                from_profile,
                root,
                mode,
            } => {
                let root = resolve_generation_root(root)?;
                let profile = cli_surfaces::load_profile(&from_profile)?;
                let artifact = cli_surfaces::generate_llms_txt_artifact(&profile, &root);
                let outcomes = cli_surfaces::materialize_artifacts(&[artifact], mode, &root)?;
                print_write_outcomes(&outcomes);
            }
        },

        Commands::Bake { action } => match action {
            BakeAction::Create {
                name,
                source_type,
                source,
                description,
                auth_headers,
                env_vars,
                timeout_seconds,
                base_dir,
                skip_validate,
            } => {
                let st = parse_source_type(&source_type);
                let config = BakeConfig {
                    name: name.clone(),
                    source_type: st,
                    source,
                    base_dir: base_dir.or_else(|| std::env::current_dir().ok()),
                    auth_headers,
                    env_vars,
                    timeout_seconds,
                    description,
                };
                if !skip_validate {
                    validate_bake_config(&config).await?;
                }
                let mut store = BakeStore::load()?;
                store.create(config)?;
                println!("Created bake: {}", name);
            }
            BakeAction::List => {
                let store = BakeStore::load()?;
                let configs = store.list();
                if configs.is_empty() {
                    println!("No baked configs.");
                } else {
                    for config in configs {
                        println!("{}", config);
                    }
                }
            }
            BakeAction::Show { name } => {
                let store = BakeStore::load()?;
                if let Some(config) = store.show(&name) {
                    println!("Name: {}", config.name);
                    println!("Type: {:?}", config.source_type);
                    println!("Source: {}", config.source);
                    if let Some(ref base_dir) = config.base_dir {
                        println!("Base dir: {}", base_dir.display());
                    }
                    if let Some(ref desc) = config.description {
                        println!("Description: {}", desc);
                    }
                    if !config.auth_headers.is_empty() {
                        println!("Auth headers: {}", config.auth_headers.len());
                    }
                    if !config.env_vars.is_empty() {
                        println!("Env vars: {}", config.env_vars.len());
                    }
                    if let Some(timeout) = config.timeout_seconds {
                        println!("Timeout: {}s", timeout);
                    }
                } else {
                    eprintln!("Bake '{}' not found", name);
                    std::process::exit(1);
                }
            }
            BakeAction::Update {
                name,
                source_type,
                source,
                description,
                auth_headers,
                env_vars,
                timeout_seconds,
                base_dir,
                skip_validate,
            } => {
                let mut store = BakeStore::load()?;
                let existing = match store.show(&name) {
                    Some(config) => config.clone(),
                    None => {
                        eprintln!("Bake '{}' not found", name);
                        std::process::exit(1);
                    }
                };
                let source_changed = source_type.is_some() || source.is_some();

                let updated = BakeConfig {
                    name: name.clone(),
                    source_type: source_type
                        .as_deref()
                        .map(parse_source_type)
                        .unwrap_or(existing.source_type),
                    source: source.unwrap_or(existing.source),
                    base_dir: base_dir
                        .or_else(|| {
                            if source_changed {
                                std::env::current_dir().ok()
                            } else {
                                None
                            }
                        })
                        .or(existing.base_dir),
                    auth_headers: if auth_headers.is_empty() {
                        existing.auth_headers
                    } else {
                        auth_headers
                    },
                    env_vars: if env_vars.is_empty() {
                        existing.env_vars
                    } else {
                        env_vars
                    },
                    timeout_seconds: timeout_seconds.or(existing.timeout_seconds),
                    description: description.or(existing.description),
                };

                if !skip_validate {
                    validate_bake_config(&updated).await?;
                }
                store.update(updated)?;
                println!("Updated bake: {}", name);
            }
            BakeAction::Remove { name } => {
                let mut store = BakeStore::load()?;
                store.remove(&name)?;
                println!("Removed bake: {}", name);
            }
        },
        Commands::Completions { shell } => {
            let mut command = Cli::command();
            generate(shell, &mut command, "sxmc", &mut std::io::stdout());
        }
        Commands::Doctor {
            root,
            check,
            only_hosts,
            fix,
            remove,
            dry_run,
            from_cli,
            depth,
            skills_path,
            allow_low_confidence,
            human,
            pretty,
            format,
        } => {
            let root = resolve_generation_root(root)?;
            if fix || remove {
                let from_cli = from_cli.as_deref().ok_or_else(|| {
                    sxmc::error::SxmcError::Other(if remove {
                        "`sxmc doctor --remove` requires `--from-cli <tool>`".into()
                    } else {
                        "`sxmc doctor --fix` requires `--from-cli <tool>`".into()
                    })
                })?;
                let outcomes = repair_doctor_startup_files(DoctorRepairOptions {
                    root: &root,
                    only_hosts: &only_hosts,
                    from_cli,
                    depth,
                    skills_path: &skills_path,
                    allow_low_confidence,
                    dry_run,
                    remove,
                })?;
                print_write_outcomes(&outcomes);
            }
            let value = doctor_value(&root, &only_hosts)?;
            if should_render_doctor_human(human, format, pretty, std::io::stdout().is_terminal()) {
                print_doctor_report(&value);
            } else if let Some(format) = output::prefer_structured_output(format, pretty) {
                println!("{}", output::format_structured_value(&value, format));
            } else {
                let format = output::resolve_structured_format(format, pretty);
                println!("{}", output::format_structured_value(&value, format));
            }
            if check {
                let startup_files = value["startup_files"].as_object();
                let has_missing = startup_files
                    .map(|files| {
                        files
                            .values()
                            .any(|details| !details["present"].as_bool().unwrap_or(false))
                    })
                    .unwrap_or(false);
                if has_missing {
                    std::process::exit(1);
                }
            }
        }
        Commands::Status {
            root,
            only_hosts,
            compare_hosts,
            health,
            exit_code,
            human,
            pretty,
            format,
        } => {
            let root = resolve_generation_root(root)?;
            let value =
                status_value_with_health(&root, &only_hosts, &compare_hosts, health).await?;
            if should_render_doctor_human(human, format, pretty, std::io::stdout().is_terminal()) {
                print_status_report(&value);
            } else if let Some(format) = output::prefer_structured_output(format, pretty) {
                println!("{}", output::format_structured_value(&value, format));
            } else {
                let format = output::resolve_structured_format(format, pretty);
                println!("{}", output::format_structured_value(&value, format));
            }
            if exit_code && status_has_unhealthy_baked_health(&value) {
                std::process::exit(1);
            }
        }
        Commands::Watch {
            root,
            only_hosts,
            compare_hosts,
            health,
            interval_seconds,
            exit_on_change,
            exit_on_unhealthy,
            pretty,
            format,
        } => {
            let root = resolve_generation_root(root)?;
            let stdout_is_tty = std::io::stdout().is_terminal();
            let interval = Duration::from_secs(interval_seconds.max(1));
            let mut last_rendered = None::<String>;
            let mut first_frame = true;
            loop {
                let value =
                    status_value_with_health(&root, &only_hosts, &compare_hosts, health).await?;
                let rendered = render_status_output(&value, format, pretty, stdout_is_tty);
                if last_rendered.as_ref() != Some(&rendered) {
                    println!("{rendered}");
                    println!();
                    std::io::stdout().flush()?;
                    if exit_on_unhealthy && status_has_unhealthy_baked_health(&value) {
                        std::process::exit(1);
                    }
                    if exit_on_change && !first_frame {
                        std::process::exit(1);
                    }
                    last_rendered = Some(rendered);
                }
                first_frame = false;
                std::thread::sleep(interval);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        annotate_mcp_tool_call_error, is_capability_not_supported, list_optional_surface,
        looks_like_argument_shape_error, should_render_doctor_human, McpSurface,
    };
    use sxmc::error::SxmcError;
    use sxmc::output::StructuredOutputFormat;

    #[test]
    fn detects_json_rpc_method_not_found_as_optional_capability_gap() {
        let error = SxmcError::McpError(
            "list_prompts failed: JSON-RPC error -32601: Method not found".into(),
        );
        assert!(is_capability_not_supported(&error));
    }

    #[test]
    fn detects_textual_not_supported_errors() {
        let error = SxmcError::McpError("list_resources failed: capability not supported".into());
        assert!(is_capability_not_supported(&error));
    }

    #[test]
    fn doctor_prefers_human_when_tty_and_no_structured_flags() {
        assert!(should_render_doctor_human(false, None, false, true));
    }

    #[test]
    fn doctor_prefers_json_when_not_tty() {
        assert!(!should_render_doctor_human(false, None, false, false));
    }

    #[test]
    fn doctor_human_flag_overrides_non_tty() {
        assert!(!should_render_doctor_human(
            false,
            Some(StructuredOutputFormat::Json),
            false,
            true
        ));
        assert!(should_render_doctor_human(true, None, false, false));
    }

    #[test]
    fn does_not_hide_real_failures() {
        let error = SxmcError::McpError("list_prompts failed: connection reset".into());
        assert!(!is_capability_not_supported(&error));
    }

    #[test]
    fn detects_argument_shape_errors() {
        assert!(looks_like_argument_shape_error(
            "call_tool failed: invalid params: expected object"
        ));
        assert!(looks_like_argument_shape_error(
            "call_tool failed: validation error: missing required property"
        ));
        assert!(!looks_like_argument_shape_error(
            "call_tool failed: connection reset"
        ));
    }

    #[test]
    fn tool_call_errors_include_recovery_hints() {
        let error = annotate_mcp_tool_call_error(
            SxmcError::McpError("call_tool failed: invalid params: expected object".into()),
            "sxmc mcp info demo/tool --format toon",
            Some("sxmc mcp session demo"),
        );
        let rendered = error.to_string();
        assert!(rendered.contains("Recovery hints:"));
        assert!(rendered.contains("sxmc mcp info demo/tool --format toon"));
        assert!(rendered.contains("sxmc mcp session demo"));
        assert!(rendered.contains("stdout only"));
    }

    #[tokio::test]
    async fn optional_surface_returns_empty_when_capability_is_missing() {
        let items = list_optional_surface::<String, _>(McpSurface::Prompts, None, async {
            Err(SxmcError::McpError(
                "list_prompts failed: JSON-RPC error -32601: Method not found".into(),
            ))
        })
        .await
        .unwrap();

        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn optional_surface_skips_when_server_does_not_advertise_capability() {
        let items = list_optional_surface::<String, _>(McpSurface::Resources, Some(false), async {
            panic!("list future should not be polled when capability is absent");
            #[allow(unreachable_code)]
            Ok(Vec::new())
        })
        .await
        .unwrap();

        assert!(items.is_empty());
    }
}
