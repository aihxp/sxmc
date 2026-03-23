use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    middleware::{self},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, GetPromptRequestParams, GetPromptResult,
    JsonObject, ListPromptsResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResult, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, tower::StreamableHttpService, StreamableHttpServerConfig,
};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Map, Value};
use tokio_util::sync::CancellationToken;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::limit::RequestBodyLimitLayer;

use crate::cli_surfaces::{parse_command_spec, CliSurfaceProfile, ConfidenceLevel, ProfileOption};
use crate::error::{Result, SxmcError};
use crate::executor;

use super::{require_auth, root_handler, HttpAuthConfig, HttpServeLimits};

#[derive(Clone)]
pub struct WrappedCliServer {
    wrapped_command: String,
    executable: String,
    fixed_args: Vec<String>,
    timeout_secs: u64,
    summary: String,
    tools: Vec<WrappedCliTool>,
    tool_index: HashMap<String, usize>,
}

#[derive(Clone)]
struct WrappedCliTool {
    name: String,
    summary: String,
    subcommand_path: Vec<String>,
    input_schema: Arc<JsonObject>,
    allowed_properties: HashSet<String>,
    options: Vec<WrappedOptionBinding>,
    positionals: Vec<WrappedPositionalBinding>,
}

#[derive(Clone)]
struct WrappedOptionBinding {
    property: String,
    cli_flag: String,
    takes_value: bool,
    required: bool,
}

#[derive(Clone)]
struct WrappedPositionalBinding {
    property: String,
    required: bool,
}

pub fn build_wrapped_cli_server(
    command_spec: &str,
    profile: &CliSurfaceProfile,
    timeout_secs: u64,
) -> Result<WrappedCliServer> {
    let parts = parse_command_spec(command_spec)?;
    if parts.is_empty() {
        return Err(SxmcError::Other(
            "wrap requires a non-empty command spec".into(),
        ));
    }

    let tools = build_wrapped_tools(profile, &parts);
    if tools.is_empty() {
        return Err(SxmcError::Other(format!(
            "sxmc wrap could not derive any MCP tools from '{}'. Re-run with `sxmc inspect cli <tool> --depth 1` to confirm the CLI surface is discoverable.",
            profile.command
        )));
    }

    let tool_index = tools
        .iter()
        .enumerate()
        .map(|(index, tool)| (tool.name.clone(), index))
        .collect::<HashMap<_, _>>();

    Ok(WrappedCliServer {
        wrapped_command: profile.command.clone(),
        executable: parts[0].clone(),
        fixed_args: parts[1..].to_vec(),
        timeout_secs,
        summary: profile.summary.clone(),
        tools,
        tool_index,
    })
}

impl WrappedCliServer {
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    pub fn wrapped_command(&self) -> &str {
        &self.wrapped_command
    }
}

impl ServerHandler for WrappedCliServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, McpError> {
        let tools = self
            .tools
            .iter()
            .map(|tool| {
                Tool::new(
                    tool.name.clone(),
                    tool.summary.clone(),
                    tool.input_schema.clone(),
                )
            })
            .collect::<Vec<_>>();
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, McpError> {
        let tool_name: &str = request.name.as_ref();
        let tool = self
            .tool_index
            .get(tool_name)
            .and_then(|index| self.tools.get(*index))
            .ok_or_else(|| {
                McpError::invalid_params(format!("Unknown tool: {}", tool_name), None)
            })?;

        let mut args = self.fixed_args.clone();
        args.extend(tool.build_cli_args(request.arguments.as_ref())?);

        match executor::execute_command(&self.executable, &args, None, self.timeout_secs).await {
            Ok(result) => {
                let stdout_json = serde_json::from_str::<Value>(&result.stdout).ok();
                let machine_friendly_stdout = stdout_json.is_some();
                let stderr_nonempty = !result.stderr.trim().is_empty();
                let payload = json!({
                    "wrapped_command": self.wrapped_command,
                    "tool": tool.name,
                    "summary": self.summary,
                    "argv": std::iter::once(self.executable.clone())
                        .chain(args.clone())
                        .collect::<Vec<_>>(),
                    "stdout": result.stdout,
                    "stdout_json": stdout_json,
                    "machine_friendly_stdout": machine_friendly_stdout,
                    "stderr": result.stderr,
                    "stderr_nonempty": stderr_nonempty,
                    "exit_code": result.exit_code,
                });
                let text = serde_json::to_string_pretty(&payload)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                if result.exit_code == 0 {
                    Ok(CallToolResult::success(vec![Content::text(text)]))
                } else {
                    Ok(CallToolResult::error(vec![Content::text(text)]))
                }
            }
            Err(error) => Ok(CallToolResult::error(vec![Content::text(
                error.to_string(),
            )])),
        }
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult {
            prompts: Vec::new(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<GetPromptResult, McpError> {
        Err(McpError::invalid_params(
            format!(
                "Wrapped CLI servers do not expose prompts: {}",
                request.name
            ),
            None,
        ))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: Vec::new(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ReadResourceResult, McpError> {
        Err(McpError::invalid_params(
            format!(
                "Wrapped CLI servers do not expose resources: {}",
                request.uri
            ),
            None,
        ))
    }
}

impl WrappedCliTool {
    fn build_cli_args(
        &self,
        arguments: Option<&Map<String, Value>>,
    ) -> std::result::Result<Vec<String>, McpError> {
        let arguments = arguments.cloned().unwrap_or_default();
        for key in arguments.keys() {
            if !self.allowed_properties.contains(key) {
                return Err(McpError::invalid_params(
                    format!("Unknown argument '{}'", key),
                    None,
                ));
            }
        }

        let mut cli_args = self.subcommand_path.clone();

        for option in &self.options {
            let Some(value) = arguments.get(&option.property) else {
                if option.required {
                    return Err(McpError::invalid_params(
                        format!("Missing required option '{}'", option.property),
                        None,
                    ));
                }
                continue;
            };

            append_option_arg(&mut cli_args, option, value)?;
        }

        for positional in &self.positionals {
            let Some(value) = arguments.get(&positional.property) else {
                if positional.required {
                    return Err(McpError::invalid_params(
                        format!("Missing required positional '{}'", positional.property),
                        None,
                    ));
                }
                continue;
            };
            cli_args.push(stringify_cli_value(value, &positional.property)?);
        }

        Ok(cli_args)
    }
}

pub async fn serve_wrapped_cli_stdio(server: WrappedCliServer) -> Result<()> {
    let transport = rmcp::transport::stdio();
    let service = server
        .serve(transport)
        .await
        .map_err(|e| SxmcError::McpError(e.to_string()))?;
    service
        .waiting()
        .await
        .map_err(|e| SxmcError::McpError(e.to_string()))?;
    Ok(())
}

pub async fn serve_wrapped_cli_http(
    server: WrappedCliServer,
    host: &str,
    port: u16,
    required_headers: &[(String, String)],
    bearer_token: Option<&str>,
    limits: HttpServeLimits,
) -> Result<()> {
    let bind_addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| SxmcError::Other(format!("Failed to bind {bind_addr}: {e}")))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| SxmcError::Other(format!("Failed to read local addr: {e}")))?;
    let cancellation_token = CancellationToken::new();
    let auth = Arc::new(HttpAuthConfig::new(required_headers, bearer_token)?);
    let router =
        build_wrapped_http_router(server.clone(), cancellation_token.clone(), auth, limits);

    eprintln!(
        "[sxmc] Wrapped CLI MCP server for '{}' listening at http://{}/mcp",
        server.wrapped_command(),
        local_addr
    );
    if !required_headers.is_empty() {
        eprintln!(
            "[sxmc] Remote MCP auth enabled with {} required header(s)",
            required_headers.len()
        );
    }
    if bearer_token.is_some() {
        eprintln!("[sxmc] Bearer token auth enabled for remote MCP access");
    }

    let shutdown = cancellation_token.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        shutdown.cancel();
    });

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            cancellation_token.cancelled_owned().await;
        })
        .await
        .map_err(|e| SxmcError::Other(format!("HTTP server failed: {e}")))?;

    Ok(())
}

fn build_wrapped_http_router(
    server: WrappedCliServer,
    cancellation_token: CancellationToken,
    auth: Arc<HttpAuthConfig>,
    limits: HttpServeLimits,
) -> Router {
    let server_for_service = server.clone();
    let service: StreamableHttpService<WrappedCliServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(server_for_service.clone()),
            Default::default(),
            StreamableHttpServerConfig {
                stateful_mode: true,
                json_response: false,
                cancellation_token,
                ..Default::default()
            },
        );
    let health_payload = Arc::new(json!({
        "name": "sxmc-wrap",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "ok",
        "transport": "streamable-http",
        "wrapped_command": server.wrapped_command(),
        "inventory": {
            "tools": server.tool_count(),
        }
    }));
    let mcp_router = Router::new().nest_service("/mcp", service);
    let mcp_router = if auth.is_empty() {
        mcp_router
    } else {
        mcp_router.layer(middleware::from_fn({
            let auth = auth.clone();
            move |request, next| require_auth(auth.clone(), request, next)
        }))
    };

    Router::new()
        .route("/", get(root_handler))
        .route(
            "/healthz",
            get({
                let payload = health_payload.clone();
                move || async move { Json((*payload).clone()).into_response() }
            }),
        )
        .merge(mcp_router)
        .layer(DefaultBodyLimit::max(limits.max_request_body_bytes))
        .layer(RequestBodyLimitLayer::new(limits.max_request_body_bytes))
        .layer(ConcurrencyLimitLayer::new(limits.max_concurrency))
}

fn build_wrapped_tools(profile: &CliSurfaceProfile, base_parts: &[String]) -> Vec<WrappedCliTool> {
    let mut tools = Vec::new();
    let mut used_tool_names = HashSet::new();

    for subcommand in profile
        .subcommands
        .iter()
        .filter(|subcommand| subcommand.confidence != ConfidenceLevel::Low)
    {
        let child_profile = profile.subcommand_profiles.iter().find(|candidate| {
            relative_subcommand_path(&profile.command, &candidate.command)
                .first()
                .is_some_and(|segment| segment == &subcommand.name)
        });

        let subcommand_path = child_profile
            .map(|candidate| relative_subcommand_path(&profile.command, &candidate.command))
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| vec![subcommand.name.clone()]);

        let detail_profile = child_profile.unwrap_or(profile);
        tools.push(build_wrapped_tool(
            base_parts,
            &subcommand_path,
            Some(&subcommand.summary),
            detail_profile,
            &mut used_tool_names,
        ));
    }

    if tools.is_empty() {
        tools.push(build_wrapped_tool(
            base_parts,
            &[],
            Some(&profile.summary),
            profile,
            &mut used_tool_names,
        ));
    }

    tools
}

fn build_wrapped_tool(
    base_parts: &[String],
    subcommand_path: &[String],
    fallback_summary: Option<&str>,
    profile: &CliSurfaceProfile,
    used_tool_names: &mut HashSet<String>,
) -> WrappedCliTool {
    let mut props = Map::new();
    let mut required = Vec::new();
    let mut allowed_properties = HashSet::new();
    let mut options = Vec::new();
    let mut positionals = Vec::new();

    for positional in &profile.positionals {
        let property = unique_property_name(
            &sanitize_property_name(&positional.name),
            &allowed_properties,
        );
        allowed_properties.insert(property.clone());
        props.insert(
            property.clone(),
            json!({
                "type": "string",
                "description": positional.summary.clone().unwrap_or_else(|| format!("Value for positional `{}`.", positional.name)),
            }),
        );
        if positional.required {
            required.push(property.clone());
        }
        positionals.push(WrappedPositionalBinding {
            property,
            required: positional.required,
        });
    }

    for option in &profile.options {
        let property = option_property_name(option, &allowed_properties);
        allowed_properties.insert(property.clone());
        props.insert(property.clone(), option_schema(option));
        if option.required {
            required.push(property.clone());
        }
        options.push(WrappedOptionBinding {
            property,
            cli_flag: option
                .name
                .strip_prefix("--")
                .map(|_| option.name.clone())
                .unwrap_or_else(|| {
                    option
                        .short
                        .as_ref()
                        .map(|short| format!("-{}", short))
                        .unwrap_or_else(|| option.name.clone())
                }),
            takes_value: option.value_name.is_some(),
            required: option.required,
        });
    }

    let mut schema = Map::new();
    schema.insert("type".into(), Value::String("object".into()));
    schema.insert("properties".into(), Value::Object(props));
    schema.insert("additionalProperties".into(), Value::Bool(false));
    if !required.is_empty() {
        schema.insert(
            "required".into(),
            Value::Array(required.into_iter().map(Value::String).collect()),
        );
    }

    let tool_name_seed = if subcommand_path.is_empty() {
        executable_tool_name(base_parts)
    } else {
        subcommand_path
            .iter()
            .map(|segment| sanitize_property_name(segment))
            .collect::<Vec<_>>()
            .join("__")
    };
    let tool_name = unique_tool_name(tool_name_seed, used_tool_names);
    let summary = profile.summary.trim().to_string();
    let summary = if summary.is_empty() {
        fallback_summary
            .unwrap_or("Run the wrapped CLI command")
            .to_string()
    } else {
        summary
    };

    WrappedCliTool {
        name: tool_name,
        summary,
        subcommand_path: subcommand_path.to_vec(),
        input_schema: Arc::new(schema),
        allowed_properties,
        options,
        positionals,
    }
}

fn option_property_name(option: &ProfileOption, used: &HashSet<String>) -> String {
    let seed = option
        .name
        .strip_prefix("--")
        .map(sanitize_property_name)
        .filter(|value| !value.is_empty())
        .or_else(|| option.short.as_deref().map(sanitize_property_name))
        .unwrap_or_else(|| "option".into());
    unique_property_name(&seed, used)
}

fn option_schema(option: &ProfileOption) -> Value {
    if option.value_name.is_some() {
        json!({
            "type": "string",
            "description": option.summary.clone().unwrap_or_else(|| format!("Value for `{}`.", option.name)),
        })
    } else {
        json!({
            "type": "boolean",
            "description": option.summary.clone().unwrap_or_else(|| format!("Set `{}`.", option.name)),
            "default": false,
        })
    }
}

fn append_option_arg(
    cli_args: &mut Vec<String>,
    option: &WrappedOptionBinding,
    value: &Value,
) -> std::result::Result<(), McpError> {
    if option.takes_value {
        match value {
            Value::Array(values) => {
                for value in values {
                    cli_args.push(option.cli_flag.clone());
                    cli_args.push(stringify_cli_value(value, &option.property)?);
                }
            }
            _ => {
                cli_args.push(option.cli_flag.clone());
                cli_args.push(stringify_cli_value(value, &option.property)?);
            }
        }
        return Ok(());
    }

    match value {
        Value::Bool(true) => {
            cli_args.push(option.cli_flag.clone());
            Ok(())
        }
        Value::Bool(false) | Value::Null => Ok(()),
        _ => Err(McpError::invalid_params(
            format!(
                "Option '{}' expects a boolean because it maps to flag '{}'",
                option.property, option.cli_flag
            ),
            None,
        )),
    }
}

fn stringify_cli_value(value: &Value, field_name: &str) -> std::result::Result<String, McpError> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Number(number) => Ok(number.to_string()),
        Value::Bool(boolean) => Ok(boolean.to_string()),
        Value::Null => Err(McpError::invalid_params(
            format!("Argument '{}' cannot be null", field_name),
            None,
        )),
        Value::Array(_) | Value::Object(_) => Err(McpError::invalid_params(
            format!(
                "Argument '{}' must be a scalar value, not a nested object/array",
                field_name
            ),
            None,
        )),
    }
}

fn relative_subcommand_path(base_command: &str, child_command: &str) -> Vec<String> {
    let derived = child_command
        .strip_prefix(base_command)
        .map(str::trim)
        .filter(|rest| !rest.is_empty())
        .map(|rest| {
            rest.split_whitespace()
                .map(|segment| segment.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !derived.is_empty() {
        return derived;
    }

    child_command
        .split_whitespace()
        .last()
        .map(|segment| vec![segment.to_string()])
        .unwrap_or_default()
}

fn executable_tool_name(base_parts: &[String]) -> String {
    base_parts
        .first()
        .and_then(|part| Path::new(part).file_stem())
        .and_then(|stem| stem.to_str())
        .map(sanitize_property_name)
        .filter(|name: &String| !name.is_empty())
        .unwrap_or_else(|| "wrapped_cli".into())
}

fn sanitize_property_name(input: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn unique_property_name(seed: &str, used: &HashSet<String>) -> String {
    if !used.contains(seed) {
        return seed.to_string();
    }
    let mut index = 2;
    loop {
        let candidate = format!("{}_{}", seed, index);
        if !used.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn unique_tool_name(seed: String, used: &mut HashSet<String>) -> String {
    if used.insert(seed.clone()) {
        return seed;
    }

    let mut index = 2;
    loop {
        let candidate = format!("{}_{}", seed, index);
        if used.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}
