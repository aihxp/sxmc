use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    extract::DefaultBodyLimit,
    middleware::{self},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use rmcp::model::{
    Annotated, CallToolRequestParams, CallToolResult, Content, GetPromptRequestParams,
    GetPromptResult, JsonObject, ListPromptsResult, ListResourcesResult, ListToolsResult,
    PaginatedRequestParams, RawResource, ReadResourceRequestParams, ReadResourceResult,
    ResourceContents, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, tower::StreamableHttpService, StreamableHttpServerConfig,
};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Map, Value};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::limit::RequestBodyLimitLayer;

use crate::cli_surfaces::{
    parse_command_spec, CliSurfaceProfile, ConfidenceLevel, ProfileOption, ProfilePositional,
};
use crate::error::{Result, SxmcError};

use super::{require_auth, root_handler, HttpAuthConfig, HttpServeLimits};

#[derive(Copy, Clone)]
enum WrappedStreamKind {
    Stdout,
    Stderr,
}

impl WrappedStreamKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

enum WrappedExecutionEvent {
    Chunk(WrappedStreamKind, Vec<u8>),
    Progress(Value),
}

#[derive(Default)]
struct WrappedStreamCapture {
    text: String,
    bytes: usize,
    truncated: bool,
    event_count: u64,
    events: Vec<Value>,
}

impl WrappedStreamCapture {
    fn push_chunk(&mut self, kind: WrappedStreamKind, chunk: &[u8], max_bytes: usize) {
        self.bytes += chunk.len();
        self.event_count += 1;

        let chunk_text = String::from_utf8_lossy(chunk).to_string();
        let stored_text = if self.text.len() < max_bytes {
            let remaining = max_bytes - self.text.len();
            let (truncated_chunk, _) = truncate_output(&chunk_text, remaining);
            self.text.push_str(&truncated_chunk);
            truncated_chunk
        } else {
            String::new()
        };
        if self.text.len() >= max_bytes || stored_text.len() < chunk_text.len() {
            self.truncated = true;
        }

        self.events.push(json!({
            "index": self.event_count,
            "stream": kind.as_str(),
            "chunk_bytes": chunk.len(),
            "stored_bytes": stored_text.len(),
            "text": stored_text,
            "truncated": stored_text.len() < chunk_text.len(),
        }));
    }
}

#[derive(Clone)]
pub struct WrappedCliServer {
    wrapped_command: String,
    executable: String,
    fixed_args: Vec<String>,
    working_dir: Option<String>,
    timeout_secs: u64,
    progress_secs: u64,
    max_stdout_bytes: usize,
    max_stderr_bytes: usize,
    summary: String,
    option_policy: WrappedFilterPolicy,
    positional_policy: WrappedFilterPolicy,
    execution_history_limit: usize,
    execution_records: Arc<Mutex<VecDeque<Value>>>,
    next_execution_id: Arc<AtomicU64>,
    tools: Vec<WrappedCliTool>,
    tool_index: HashMap<String, usize>,
}

#[derive(Clone, Default)]
pub struct WrappedCliOptions {
    pub timeout_secs: u64,
    pub progress_secs: u64,
    pub working_dir: Option<String>,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub allow_tools: Vec<String>,
    pub deny_tools: Vec<String>,
    pub allow_options: Vec<String>,
    pub deny_options: Vec<String>,
    pub allow_positionals: Vec<String>,
    pub deny_positionals: Vec<String>,
    pub execution_history_limit: usize,
}

#[derive(Clone, Default)]
struct WrappedFilterPolicy {
    allow: HashSet<String>,
    deny: HashSet<String>,
}

#[derive(Clone, Default)]
struct WrappedArgumentPolicies {
    options: WrappedFilterPolicy,
    positionals: WrappedFilterPolicy,
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
    options: WrappedCliOptions,
) -> Result<WrappedCliServer> {
    let parts = parse_command_spec(command_spec)?;
    if parts.is_empty() {
        return Err(SxmcError::Other(
            "wrap requires a non-empty command spec".into(),
        ));
    }

    let argument_policies = WrappedArgumentPolicies {
        options: WrappedFilterPolicy::from_lists(&options.allow_options, &options.deny_options),
        positionals: WrappedFilterPolicy::from_lists(
            &options.allow_positionals,
            &options.deny_positionals,
        ),
    };
    let tools = build_wrapped_tools(
        profile,
        &parts,
        &options.allow_tools,
        &options.deny_tools,
        &argument_policies,
    );
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
        working_dir: options.working_dir,
        timeout_secs: options.timeout_secs,
        progress_secs: options.progress_secs,
        max_stdout_bytes: options.max_stdout_bytes,
        max_stderr_bytes: options.max_stderr_bytes,
        summary: profile.summary.clone(),
        option_policy: argument_policies.options,
        positional_policy: argument_policies.positionals,
        execution_history_limit: options.execution_history_limit.max(1),
        execution_records: Arc::new(Mutex::new(VecDeque::new())),
        next_execution_id: Arc::new(AtomicU64::new(1)),
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

    pub fn working_dir(&self) -> Option<&str> {
        self.working_dir.as_deref()
    }

    fn upsert_execution_record(&self, record: Value) {
        let execution_id = record["execution_id"].as_u64();
        if let Ok(mut records) = self.execution_records.lock() {
            if let Some(execution_id) = execution_id {
                if let Some(existing) = records
                    .iter_mut()
                    .find(|item| item["execution_id"].as_u64() == Some(execution_id))
                {
                    *existing = record;
                    return;
                }
            }
            records.push_back(record);
            while records.len() > self.execution_history_limit {
                records.pop_front();
            }
        }
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
        let execution_id = self.next_execution_id.fetch_add(1, Ordering::Relaxed);

        let mut args = self.fixed_args.clone();
        args.extend(tool.build_cli_args(request.arguments.as_ref())?);
        let execution_resource_uri = format!("sxmc-wrap://executions/{}", execution_id);
        let events_resource_uri = format!("sxmc-wrap://executions/{}/events", execution_id);
        let argv = std::iter::once(self.executable.clone())
            .chain(args.clone())
            .collect::<Vec<_>>();
        let mut stdout_capture = WrappedStreamCapture::default();
        let mut stderr_capture = WrappedStreamCapture::default();
        let mut progress_events = Vec::<Value>::new();
        let mut timeout = false;
        let mut exit_code = None::<i32>;

        let refresh_record = |server: &WrappedCliServer,
                              status: &str,
                              stdout_capture: &WrappedStreamCapture,
                              stderr_capture: &WrappedStreamCapture,
                              progress_events: &[Value],
                              elapsed_ms: u64,
                              exit_code: Option<i32>,
                              timeout: bool| {
            server.upsert_execution_record(json!({
                "execution_id": execution_id,
                "status": status,
                "execution_resource_uri": execution_resource_uri,
                "events_resource_uri": events_resource_uri,
                "wrapped_command": self.wrapped_command,
                "tool": tool.name,
                "summary": self.summary,
                "argv": argv,
                "working_dir": self.working_dir,
                "progress_seconds": self.progress_secs,
                "progress_event_count": progress_events.len(),
                "progress_events": progress_events,
                "stdout": stdout_capture.text,
                "stdout_bytes": stdout_capture.bytes,
                "stdout_truncated": stdout_capture.truncated,
                "stdout_event_count": stdout_capture.event_count,
                "stdout_events": stdout_capture.events,
                "stderr": stderr_capture.text,
                "stderr_bytes": stderr_capture.bytes,
                "stderr_truncated": stderr_capture.truncated,
                "stderr_nonempty": !stderr_capture.text.trim().is_empty(),
                "stderr_event_count": stderr_capture.event_count,
                "stderr_events": stderr_capture.events,
                "stream_event_count": stdout_capture.event_count + stderr_capture.event_count,
                "elapsed_ms": elapsed_ms,
                "long_running": !progress_events.is_empty()
                    || stdout_capture.event_count > 1
                    || stderr_capture.event_count > 0,
                "timeout_seconds": self.timeout_secs,
                "timeout": timeout,
                "exit_code": exit_code,
            }));
        };

        refresh_record(
            self,
            "running",
            &stdout_capture,
            &stderr_capture,
            &progress_events,
            0,
            exit_code,
            timeout,
        );

        let mut command = Command::new(&self.executable);
        command
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(working_dir) = self.working_dir.as_deref().map(Path::new) {
            command.current_dir(working_dir);
        }
        let mut child = command
            .spawn()
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            McpError::internal_error("Wrapped command stdout was not piped".to_string(), None)
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            McpError::internal_error("Wrapped command stderr was not piped".to_string(), None)
        })?;

        let (tx, mut rx) = mpsc::unbounded_channel::<WrappedExecutionEvent>();
        let stdout_tx = tx.clone();
        let stdout_handle = tokio::spawn(async move {
            let mut stdout = stdout;
            let mut buf = vec![0u8; 4096];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(read) => {
                        if stdout_tx
                            .send(WrappedExecutionEvent::Chunk(
                                WrappedStreamKind::Stdout,
                                buf[..read].to_vec(),
                            ))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        let stderr_tx = tx.clone();
        let stderr_handle = tokio::spawn(async move {
            let mut stderr = stderr;
            let mut buf = vec![0u8; 4096];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(read) => {
                        if stderr_tx
                            .send(WrappedExecutionEvent::Chunk(
                                WrappedStreamKind::Stderr,
                                buf[..read].to_vec(),
                            ))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let progress_done = Arc::new(AtomicBool::new(false));
        let progress_handle = if self.progress_secs > 0 {
            let command = self.wrapped_command.clone();
            let tool_name = tool.name.clone();
            let progress_secs = self.progress_secs;
            let done = progress_done.clone();
            let progress_tx = tx.clone();
            Some(tokio::spawn(async move {
                let mut elapsed = progress_secs;
                while !done.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_secs(progress_secs)).await;
                    if done.load(Ordering::Relaxed) {
                        break;
                    }
                    let message = format!(
                        "Wrapped tool '{}' for '{}' still running after {}s",
                        tool_name, command, elapsed
                    );
                    eprintln!("[sxmc] {}", message);
                    if progress_tx
                        .send(WrappedExecutionEvent::Progress(json!({
                            "elapsed_secs": elapsed,
                            "message": message,
                        })))
                        .is_err()
                    {
                        break;
                    }
                    elapsed += progress_secs;
                }
            }))
        } else {
            None
        };
        drop(tx);

        let started_at = std::time::Instant::now();
        let timeout_duration = Duration::from_secs(self.timeout_secs.max(1));
        let timeout_sleep = tokio::time::sleep(timeout_duration);
        tokio::pin!(timeout_sleep);

        let mut channel_closed = false;
        while !(exit_code.is_some() && channel_closed) {
            tokio::select! {
                status = child.wait(), if exit_code.is_none() => {
                    let status = status.map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    exit_code = Some(status.code().unwrap_or(-1));
                    progress_done.store(true, Ordering::Relaxed);
                }
                _ = &mut timeout_sleep, if exit_code.is_none() => {
                    timeout = true;
                    progress_done.store(true, Ordering::Relaxed);
                    let _ = child.kill().await;
                    let status = child.wait().await.map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    exit_code = Some(status.code().unwrap_or(-1));
                }
                maybe_event = rx.recv() => {
                    match maybe_event {
                        Some(WrappedExecutionEvent::Chunk(kind, chunk)) => {
                            match kind {
                                WrappedStreamKind::Stdout => stdout_capture.push_chunk(kind, &chunk, self.max_stdout_bytes),
                                WrappedStreamKind::Stderr => stderr_capture.push_chunk(kind, &chunk, self.max_stderr_bytes),
                            }
                            refresh_record(
                                self,
                                if exit_code.is_some() { "completed" } else { "running" },
                                &stdout_capture,
                                &stderr_capture,
                                &progress_events,
                                started_at.elapsed().as_millis() as u64,
                                exit_code,
                                timeout,
                            );
                        }
                        Some(WrappedExecutionEvent::Progress(event)) => {
                            progress_events.push(event);
                            refresh_record(
                                self,
                                if exit_code.is_some() { "completed" } else { "running" },
                                &stdout_capture,
                                &stderr_capture,
                                &progress_events,
                                started_at.elapsed().as_millis() as u64,
                                exit_code,
                                timeout,
                            );
                        }
                        None => channel_closed = true,
                    }
                }
            }
        }

        let _ = stdout_handle.await;
        let _ = stderr_handle.await;
        if let Some(progress_handle) = progress_handle {
            let _ = progress_handle.await;
        }
        while let Some(event) = rx.recv().await {
            match event {
                WrappedExecutionEvent::Chunk(kind, chunk) => match kind {
                    WrappedStreamKind::Stdout => {
                        stdout_capture.push_chunk(kind, &chunk, self.max_stdout_bytes)
                    }
                    WrappedStreamKind::Stderr => {
                        stderr_capture.push_chunk(kind, &chunk, self.max_stderr_bytes)
                    }
                },
                WrappedExecutionEvent::Progress(event) => progress_events.push(event),
            }
        }

        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        let stdout_json = serde_json::from_str::<Value>(&stdout_capture.text).ok();
        let machine_friendly_stdout = stdout_json.is_some();
        let payload = json!({
            "execution_id": execution_id,
            "status": if timeout { "timed_out" } else { "completed" },
            "execution_resource_uri": execution_resource_uri,
            "events_resource_uri": events_resource_uri,
            "wrapped_command": self.wrapped_command,
            "tool": tool.name,
            "summary": self.summary,
            "argv": argv,
            "working_dir": self.working_dir,
            "progress_seconds": self.progress_secs,
            "progress_event_count": progress_events.len(),
            "progress_events": progress_events,
            "stdout": stdout_capture.text,
            "stdout_bytes": stdout_capture.bytes,
            "stdout_truncated": stdout_capture.truncated,
            "stdout_json": stdout_json,
            "stdout_event_count": stdout_capture.event_count,
            "stdout_events": stdout_capture.events,
            "machine_friendly_stdout": machine_friendly_stdout,
            "stderr": stderr_capture.text,
            "stderr_bytes": stderr_capture.bytes,
            "stderr_truncated": stderr_capture.truncated,
            "stderr_nonempty": !stderr_capture.text.trim().is_empty(),
            "stderr_event_count": stderr_capture.event_count,
            "stderr_events": stderr_capture.events,
            "stream_event_count": stdout_capture.event_count + stderr_capture.event_count,
            "long_running": !progress_events.is_empty()
                || stdout_capture.event_count > 1
                || stderr_capture.event_count > 0,
            "exit_code": exit_code.unwrap_or(-1),
            "elapsed_ms": elapsed_ms,
            "timeout_seconds": self.timeout_secs,
            "timeout": timeout,
            "error": if timeout {
                Value::String(SxmcError::TimeoutError(self.timeout_secs).to_string())
            } else {
                Value::Null
            },
        });
        self.upsert_execution_record(payload.clone());
        let text = serde_json::to_string_pretty(&payload)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        if timeout || exit_code.unwrap_or(-1) != 0 {
            Ok(CallToolResult::error(vec![Content::text(text)]))
        } else {
            Ok(CallToolResult::success(vec![Content::text(text)]))
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
        let mut resources = Vec::new();
        resources.push(Annotated::new(
            RawResource::new(
                "sxmc-wrap://executions".to_string(),
                "Wrapped execution history".to_string(),
            )
            .with_description("Recent wrapped CLI execution summaries.")
            .with_mime_type("application/json"),
            None,
        ));
        if let Ok(records) = self.execution_records.lock() {
            for record in records.iter().rev() {
                let id = record["execution_id"].as_u64().unwrap_or(0);
                let tool = record["tool"].as_str().unwrap_or("wrapped-tool");
                resources.push(Annotated::new(
                    RawResource::new(
                        format!("sxmc-wrap://executions/{id}"),
                        format!("Wrapped execution {id} ({tool})"),
                    )
                    .with_description("Detailed wrapped CLI execution payload.")
                    .with_mime_type("application/json"),
                    None,
                ));
                resources.push(Annotated::new(
                    RawResource::new(
                        format!("sxmc-wrap://executions/{id}/events"),
                        format!("Wrapped execution {id} event stream ({tool})"),
                    )
                    .with_description(
                        "Captured stdout/stderr/progress events for a wrapped CLI execution.",
                    )
                    .with_mime_type("application/json"),
                    None,
                ));
            }
        }
        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ReadResourceResult, McpError> {
        let uri = request.uri.as_str();
        if uri == "sxmc-wrap://executions" {
            let entries = self
                .execution_records
                .lock()
                .map(|records| {
                    records
                        .iter()
                        .map(|record| {
                            json!({
                                "execution_id": record["execution_id"],
                                "tool": record["tool"],
                                "summary": record["summary"],
                                "elapsed_ms": record["elapsed_ms"],
                                "timeout": record["timeout"],
                                "exit_code": record["exit_code"],
                                "long_running": record["long_running"],
                                "resource_uri": record["execution_resource_uri"],
                                "events_resource_uri": record["events_resource_uri"],
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                serde_json::to_string_pretty(&json!({
                    "count": entries.len(),
                    "entries": entries,
                }))
                .map_err(|e| McpError::internal_error(e.to_string(), None))?,
                uri,
            )]));
        }
        if let Some(id_text) = uri
            .strip_prefix("sxmc-wrap://executions/")
            .and_then(|value| value.strip_suffix("/events"))
        {
            let id = id_text.parse::<u64>().map_err(|_| {
                McpError::invalid_params(
                    format!("Invalid wrapped execution event resource: {}", uri),
                    None,
                )
            })?;
            let payload = self
                .execution_records
                .lock()
                .ok()
                .and_then(|records| {
                    records
                        .iter()
                        .find(|record| record["execution_id"].as_u64() == Some(id))
                        .cloned()
                })
                .ok_or_else(|| {
                    McpError::invalid_params(
                        format!("Unknown wrapped execution event resource: {}", uri),
                        None,
                    )
                })?;
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                serde_json::to_string_pretty(&json!({
                    "execution_id": payload["execution_id"],
                    "status": payload["status"],
                    "count": payload["stream_event_count"].as_u64().unwrap_or(0)
                        + payload["progress_event_count"].as_u64().unwrap_or(0),
                    "stdout_event_count": payload["stdout_event_count"],
                    "stderr_event_count": payload["stderr_event_count"],
                    "progress_event_count": payload["progress_event_count"],
                    "stdout_events": payload["stdout_events"],
                    "stderr_events": payload["stderr_events"],
                    "progress_events": payload["progress_events"],
                }))
                .map_err(|e| McpError::internal_error(e.to_string(), None))?,
                uri,
            )]));
        }
        if let Some(id_text) = uri.strip_prefix("sxmc-wrap://executions/") {
            let id = id_text.parse::<u64>().map_err(|_| {
                McpError::invalid_params(
                    format!("Invalid wrapped execution resource: {}", uri),
                    None,
                )
            })?;
            let payload = self
                .execution_records
                .lock()
                .ok()
                .and_then(|records| {
                    records
                        .iter()
                        .find(|record| record["execution_id"].as_u64() == Some(id))
                        .cloned()
                })
                .ok_or_else(|| {
                    McpError::invalid_params(
                        format!("Unknown wrapped execution resource: {}", uri),
                        None,
                    )
                })?;
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                serde_json::to_string_pretty(&payload)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?,
                uri,
            )]));
        }
        Err(McpError::invalid_params(
            format!("Unknown resource: {}", uri),
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
        },
        "execution": {
            "working_dir": server.working_dir(),
            "timeout_seconds": server.timeout_secs,
            "progress_seconds": server.progress_secs,
            "streaming_events": true,
            "max_stdout_bytes": server.max_stdout_bytes,
            "max_stderr_bytes": server.max_stderr_bytes,
            "execution_history_limit": server.execution_history_limit,
            "option_policy": {
                "allow": server.option_policy.rendered_allow(),
                "deny": server.option_policy.rendered_deny(),
            },
            "positional_policy": {
                "allow": server.positional_policy.rendered_allow(),
                "deny": server.positional_policy.rendered_deny(),
            },
        },
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

fn build_wrapped_tools(
    profile: &CliSurfaceProfile,
    base_parts: &[String],
    allow_tools: &[String],
    deny_tools: &[String],
    argument_policies: &WrappedArgumentPolicies,
) -> Vec<WrappedCliTool> {
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
            argument_policies,
            &mut used_tool_names,
        ));
    }

    if tools.is_empty() {
        tools.push(build_wrapped_tool(
            base_parts,
            &[],
            Some(&profile.summary),
            profile,
            argument_policies,
            &mut used_tool_names,
        ));
    }

    let allow_tools = allow_tools
        .iter()
        .map(|item| sanitize_property_name(item))
        .collect::<HashSet<_>>();
    let deny_tools = deny_tools
        .iter()
        .map(|item| sanitize_property_name(item))
        .collect::<HashSet<_>>();

    tools
        .into_iter()
        .filter(|tool| {
            let normalized = sanitize_property_name(&tool.name);
            (allow_tools.is_empty() || allow_tools.contains(&normalized))
                && !deny_tools.contains(&normalized)
        })
        .collect()
}

fn build_wrapped_tool(
    base_parts: &[String],
    subcommand_path: &[String],
    fallback_summary: Option<&str>,
    profile: &CliSurfaceProfile,
    argument_policies: &WrappedArgumentPolicies,
    used_tool_names: &mut HashSet<String>,
) -> WrappedCliTool {
    let mut props = Map::new();
    let mut required = Vec::new();
    let mut allowed_properties = HashSet::new();
    let mut options = Vec::new();
    let mut positionals = Vec::new();

    for positional in profile
        .positionals
        .iter()
        .filter(|positional| argument_policies.positionals.allows_positional(positional))
    {
        let property = unique_property_name(
            &sanitize_property_name(&positional.name),
            &allowed_properties,
        );
        allowed_properties.insert(property.clone());
        props.insert(
            property.clone(),
            json!({
                "oneOf": [
                    {"type": "string"},
                    {"type": "number"},
                    {"type": "boolean"}
                ],
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

    for option in profile
        .options
        .iter()
        .filter(|option| argument_policies.options.allows_option(option))
    {
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
        let value_name = option.value_name.as_deref().unwrap_or_default();
        let repeated = value_name.contains("...")
            || value_name.contains(',')
            || value_name.chars().all(|ch| !ch.is_ascii_lowercase()) && value_name.ends_with('S');
        let scalar_schema = json!({
            "oneOf": [
                {"type": "string"},
                {"type": "number"},
                {"type": "boolean"}
            ]
        });
        let value_schema = if repeated {
            json!({
                "oneOf": [
                    scalar_schema.clone(),
                    {
                        "type": "array",
                        "minItems": 1,
                        "items": scalar_schema,
                    }
                ],
                "description": option.summary.clone().unwrap_or_else(|| format!("Value for `{}`.", option.name)),
            })
        } else {
            json!({
                "oneOf": [
                    {"type": "string"},
                    {"type": "number"},
                    {"type": "boolean"}
                ],
                "description": option.summary.clone().unwrap_or_else(|| format!("Value for `{}`.", option.name)),
            })
        };
        value_schema
    } else {
        json!({
            "type": "boolean",
            "description": option.summary.clone().unwrap_or_else(|| format!("Set `{}`.", option.name)),
            "default": false,
        })
    }
}

fn truncate_output(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_string(), false);
    }
    let mut end = max_bytes.min(value.len());
    while !value.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    (value[..end].to_string(), true)
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

impl WrappedFilterPolicy {
    fn from_lists(allow: &[String], deny: &[String]) -> Self {
        Self {
            allow: allow
                .iter()
                .map(|item| sanitize_property_name(item))
                .filter(|item| !item.is_empty())
                .collect(),
            deny: deny
                .iter()
                .map(|item| sanitize_property_name(item))
                .filter(|item| !item.is_empty())
                .collect(),
        }
    }

    fn allowed(&self, identifiers: impl IntoIterator<Item = String>) -> bool {
        let identifiers = identifiers
            .into_iter()
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        if identifiers.iter().any(|item| self.deny.contains(item)) {
            return false;
        }
        self.allow.is_empty() || identifiers.iter().any(|item| self.allow.contains(item))
    }

    fn allows_option(&self, option: &ProfileOption) -> bool {
        let mut identifiers = vec![sanitize_property_name(&option.name)];
        if let Some(short) = &option.short {
            identifiers.push(sanitize_property_name(short));
            identifiers.push(sanitize_property_name(&format!("-{}", short)));
        }
        if let Some(long) = option.name.strip_prefix("--") {
            identifiers.push(sanitize_property_name(long));
        }
        self.allowed(identifiers)
    }

    fn allows_positional(&self, positional: &ProfilePositional) -> bool {
        self.allowed(vec![sanitize_property_name(&positional.name)])
    }

    fn rendered_allow(&self) -> Vec<String> {
        let mut items = self.allow.iter().cloned().collect::<Vec<_>>();
        items.sort();
        items
    }

    fn rendered_deny(&self) -> Vec<String> {
        let mut items = self.deny.iter().cloned().collect::<Vec<_>>();
        items.sort();
        items
    }
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
