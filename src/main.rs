mod cli_args;
mod command_handlers;

use clap::{CommandFactory, Parser};
use clap_complete::generate;
use rmcp::model::{Prompt, Resource, ServerInfo, Tool};
use serde_json::{json, Value};
use std::io::BufRead;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;

use std::collections::HashMap;

use cli_args::{
    BakeAction, Cli, Commands, InitAction, InspectAction, McpAction, McpSessionAction,
    McpSessionCli, ScaffoldAction, SkillsAction,
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

fn print_doctor_report(value: &Value) {
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

    println!("Root: {}", value["root"].as_str().unwrap_or("<unknown>"));
    if !checked_hosts.is_empty() {
        println!("Checked hosts: {}", checked_hosts);
    }
    println!(
        "Baked MCP servers: {}",
        value["baked_mcp_servers"].as_u64().unwrap_or(0)
    );
    println!(
        "Profile cache dir: {} ({})",
        if portable_profiles_present {
            "present"
        } else {
            "missing"
        },
        portable_profiles_path
    );
    println!(
        "CLI profile cache: {} entries, {} bytes (TTL: {}h)",
        cache_entries, cache_total_bytes, cache_ttl_hours
    );
    println!("Cache path: {}", cache_path);
    println!("Startup files present: {startup_present}/{startup_total}");
    println!();
    println!("Startup files:");
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
            println!("  Present:");
            for (name, details) in present {
                let path = details["path"].as_str().unwrap_or_default();
                println!("  - {} ({})", name, path);
            }
        }

        if !missing.is_empty() {
            println!("  Missing:");
            for (name, details) in missing {
                let path = details["path"].as_str().unwrap_or_default();
                println!("  - {} ({})", name, path);
            }
        }
    }
    println!();
    println!("Recommended first moves:");
    if let Some(moves) = value["recommended_first_moves"].as_array() {
        for (index, item) in moves.iter().enumerate() {
            let surface = item["surface"].as_str().unwrap_or("surface");
            let command = item["command"].as_str().unwrap_or_default();
            let why = item["why"].as_str().unwrap_or_default();
            println!(
                "{}. {} -> `{}`",
                index + 1,
                surface.replace('_', " "),
                command
            );
            println!("   {}", why);
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
            }
        }
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

fn repair_doctor_startup_files(
    root: &std::path::Path,
    only_hosts: &[AiClientProfile],
    from_cli: &str,
    depth: usize,
    skills_path: &std::path::Path,
    allow_low_confidence: bool,
) -> Result<Vec<cli_surfaces::WriteOutcome>> {
    if only_hosts.is_empty() {
        return Err(sxmc::error::SxmcError::Other(
            "`sxmc doctor --fix` requires at least one `--only <host>` selection".into(),
        ));
    }

    let profile = cli_surfaces::inspect_cli_with_depth(from_cli, true, depth)?;
    ensure_profile_ready_for_agent_docs(&profile, allow_low_confidence)?;
    let (artifacts, selected_hosts) = resolve_cli_ai_init_artifacts(
        &profile,
        AiCoverage::Full,
        None,
        only_hosts,
        root,
        skills_path,
        ArtifactMode::Apply,
    )?;
    cli_surfaces::materialize_artifacts_with_apply_selection(
        &artifacts,
        ArtifactMode::Apply,
        root,
        &selected_hosts,
    )
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
                depth,
                since,
                parallel,
                progress,
                compact,
                pretty,
                format,
                allow_self,
            } => {
                let mut requests =
                    cli_surfaces::load_batch_requests(&commands, from_file.as_deref())?;
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
                let value = cli_surfaces::inspect_cli_batch(
                    &requests,
                    allow_self,
                    parallel,
                    progress,
                    since_filter.as_ref(),
                );
                if let Some(format) = output::prefer_structured_output(format, pretty) {
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
                depth,
                pretty,
                format,
                allow_self,
            } => {
                let before_profile = cli_surfaces::load_profile(&before)?;
                let after_profile =
                    cli_surfaces::inspect_cli_with_depth(&command, allow_self, depth)?;
                let value = cli_surfaces::diff_profile_value(&before_profile, &after_profile);
                if let Some(format) = output::prefer_structured_output(format, pretty) {
                    if matches!(format, output::StructuredOutputFormat::Toon) {
                        println!("{}", format_diff_toon(&value));
                        return Ok(());
                    }
                    println!("{}", output::format_structured_value(&value, format));
                } else {
                    let format = output::resolve_structured_format(format, pretty);
                    if matches!(format, output::StructuredOutputFormat::Toon) {
                        println!("{}", format_diff_toon(&value));
                        return Ok(());
                    }
                    println!("{}", output::format_structured_value(&value, format));
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
                    cli_surfaces::load_batch_requests(&commands, from_file.as_deref())?;
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
            from_cli,
            depth,
            skills_path,
            allow_low_confidence,
            human,
            pretty,
            format,
        } => {
            let root = resolve_generation_root(root)?;
            if fix {
                let from_cli = from_cli.as_deref().ok_or_else(|| {
                    sxmc::error::SxmcError::Other(
                        "`sxmc doctor --fix` requires `--from-cli <tool>`".into(),
                    )
                })?;
                let outcomes = repair_doctor_startup_files(
                    &root,
                    &only_hosts,
                    from_cli,
                    depth,
                    &skills_path,
                    allow_low_confidence,
                )?;
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
