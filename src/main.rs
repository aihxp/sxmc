use clap::{Parser, Subcommand};
use rmcp::model::{Prompt, Resource, ServerInfo, Tool};
use serde_json::{json, Value};
use std::io::BufRead;
use std::path::PathBuf;

use std::collections::HashMap;

use sxmc::auth::secrets::{resolve_header, resolve_secret};
use sxmc::bake::config::SourceType;
use sxmc::bake::{BakeConfig, BakeStore};
use sxmc::cli_surfaces::{self, AiClientProfile, AiCoverage, ArtifactMode};
use sxmc::client::{api, graphql, mcp_http, mcp_stdio, openapi};
use sxmc::error::Result;
use sxmc::output;
use sxmc::security;
use sxmc::server;
use sxmc::skills::{discovery, generator, parser};

#[derive(Parser)]
#[command(name = "sxmc", version, about = "AI-agnostic Skills × MCP × CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the MCP server (serves skills over MCP)
    Serve {
        /// Skill search paths (comma-separated)
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,

        /// Watch skill files and reload the in-memory server on change
        #[arg(long)]
        watch: bool,

        /// Transport: stdio, http, or sse (alias for http)
        #[arg(long, default_value = "stdio")]
        transport: String,

        /// Port for HTTP transport
        #[arg(long, default_value = "8000")]
        port: u16,

        /// Host for HTTP transport
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Require HTTP header(s) for remote MCP access (Key:Value)
        #[arg(long = "require-header", value_name = "K:V")]
        require_headers: Vec<String>,

        /// Require a Bearer token for remote MCP access
        #[arg(long, value_name = "TOKEN")]
        bearer_token: Option<String>,
    },

    /// Manage skills
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },

    /// Connect to an MCP server via stdio (MCP Server → CLI)
    Stdio {
        /// Command spec to spawn the MCP server.
        /// Supports shell-style quoting or a JSON array like ["sxmc","serve"].
        command: String,

        /// Prompt to fetch
        #[arg(long, conflicts_with = "resource_uri")]
        prompt: Option<String>,

        /// Resource URI to read
        #[arg(long = "resource", conflicts_with = "prompt")]
        resource_uri: Option<String>,

        /// Tool name followed by key=value pairs
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// List available tools, prompts, and resources
        #[arg(long)]
        list: bool,

        /// List only tools
        #[arg(long)]
        list_tools: bool,

        /// List only prompts
        #[arg(long)]
        list_prompts: bool,

        /// List only resources
        #[arg(long)]
        list_resources: bool,

        /// Search/filter tools by name or description
        #[arg(long)]
        search: Option<String>,

        /// Describe the negotiated MCP server surface as structured JSON
        #[arg(long, conflicts_with = "describe_tool")]
        describe: bool,

        /// Show detailed schema/help for a single tool
        #[arg(long, value_name = "TOOL", conflicts_with = "describe")]
        describe_tool: Option<String>,

        /// Structured output format for MCP describe output
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,

        /// Maximum items to show per listed MCP surface
        #[arg(long, value_name = "N")]
        limit: Option<usize>,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Environment variables for the server (KEY=VALUE)
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env_vars: Vec<String>,

        /// Working directory for the spawned MCP server
        #[arg(long)]
        cwd: Option<PathBuf>,
    },

    /// Connect to an MCP server via HTTP (MCP Server → CLI)
    Http {
        /// MCP server URL
        url: String,

        /// Prompt to fetch
        #[arg(long, conflicts_with = "resource_uri")]
        prompt: Option<String>,

        /// Resource URI to read
        #[arg(long = "resource", conflicts_with = "prompt")]
        resource_uri: Option<String>,

        /// Tool name followed by key=value pairs
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// List available tools, prompts, and resources
        #[arg(long)]
        list: bool,

        /// List only tools
        #[arg(long)]
        list_tools: bool,

        /// List only prompts
        #[arg(long)]
        list_prompts: bool,

        /// List only resources
        #[arg(long)]
        list_resources: bool,

        /// Search/filter tools by name or description
        #[arg(long)]
        search: Option<String>,

        /// Describe the negotiated MCP server surface as structured JSON
        #[arg(long, conflicts_with = "describe_tool")]
        describe: bool,

        /// Show detailed schema/help for a single tool
        #[arg(long, value_name = "TOOL", conflicts_with = "describe")]
        describe_tool: Option<String>,

        /// Structured output format for MCP describe output
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,

        /// Maximum items to show per listed MCP surface
        #[arg(long, value_name = "N")]
        limit: Option<usize>,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// HTTP headers (Key:Value)
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
    },

    /// Use baked MCP connections in a token-efficient, mcp-cli-style workflow
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },

    /// Connect to any API (auto-detects OpenAPI or GraphQL)
    Api {
        /// API URL or spec file path
        source: String,

        /// Operation to call (omit for --list)
        operation: Option<String>,

        /// Arguments as key=value pairs
        args: Vec<String>,

        /// List available operations
        #[arg(long)]
        list: bool,

        /// Search/filter operations
        #[arg(long)]
        search: Option<String>,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format for API responses
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,

        /// HTTP headers (Key:Value)
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
    },

    /// Connect to an OpenAPI spec (explicit)
    Spec {
        /// OpenAPI spec URL or file path
        source: String,

        /// Operation to call (omit for --list)
        operation: Option<String>,

        /// Arguments as key=value pairs
        args: Vec<String>,

        /// List available operations
        #[arg(long)]
        list: bool,

        /// Search/filter operations
        #[arg(long)]
        search: Option<String>,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format for API responses
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,

        /// HTTP headers (Key:Value)
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
    },

    /// Connect to a GraphQL endpoint (explicit)
    Graphql {
        /// GraphQL endpoint URL
        url: String,

        /// Operation to call (omit for --list)
        operation: Option<String>,

        /// Arguments as key=value pairs
        args: Vec<String>,

        /// List available operations
        #[arg(long)]
        list: bool,

        /// Search/filter operations
        #[arg(long)]
        search: Option<String>,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format for API responses
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,

        /// HTTP headers (Key:Value)
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
    },

    /// Scan skills and MCP servers for security issues
    Scan {
        /// Skill search paths (comma-separated)
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,

        /// Scan a specific skill by name
        #[arg(long)]
        skill: Option<String>,

        /// Scan an MCP server via stdio
        #[arg(long = "mcp-stdio")]
        mcp_stdio: Option<String>,

        /// Scan an MCP server via HTTP
        #[arg(long = "mcp")]
        mcp: Option<String>,

        /// Minimum severity to report: info, warn, error, critical
        #[arg(long, default_value = "info")]
        severity: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Environment variables for stdio server (KEY=VALUE)
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env_vars: Vec<String>,
    },

    /// Inspect structured sxmc artifacts
    Inspect {
        #[command(subcommand)]
        action: InspectAction,
    },

    /// Initialize startup-facing AI artifacts from an inspected CLI
    Init {
        #[command(subcommand)]
        action: InitAction,
    },

    /// Generate AI-facing scaffolds from an existing CLI surface profile
    Scaffold {
        #[command(subcommand)]
        action: ScaffoldAction,
    },

    /// Manage baked connection configs
    Bake {
        #[command(subcommand)]
        action: BakeAction,
    },
}

#[derive(Subcommand)]
enum BakeAction {
    /// Create a new baked config
    Create {
        /// Config name
        name: String,

        /// Source type: stdio, http, api, spec, graphql
        #[arg(long = "type", default_value = "stdio")]
        source_type: String,

        /// Source URL, command, or path
        #[arg(long)]
        source: String,

        /// Description
        #[arg(long)]
        description: Option<String>,

        /// Auth headers (Key:Value)
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,

        /// Env vars for stdio (KEY=VALUE)
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env_vars: Vec<String>,
    },
    /// List all baked configs
    List,
    /// Show details for a baked config
    Show { name: String },
    /// Update an existing baked config
    Update {
        /// Config name
        name: String,

        /// Source type: stdio, http, api, spec, graphql
        #[arg(long = "type")]
        source_type: Option<String>,

        /// Source URL, command, or path
        #[arg(long)]
        source: Option<String>,

        /// Description
        #[arg(long)]
        description: Option<String>,

        /// Auth headers (Key:Value)
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,

        /// Env vars for stdio (KEY=VALUE)
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env_vars: Vec<String>,
    },
    /// Remove a baked config
    Remove { name: String },
}

#[derive(Subcommand)]
enum InspectAction {
    /// Inspect a real CLI into a normalized profile
    Cli {
        /// Command spec to inspect.
        /// Supports shell-style quoting or a JSON array like ["gh"].
        command: String,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format for the profile
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,

        /// Allow inspecting sxmc itself
        #[arg(long)]
        allow_self: bool,
    },

    /// Render a CLI surface profile from JSON
    Profile {
        /// Path to a JSON profile file
        input: PathBuf,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format for the profile
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
}

#[derive(Subcommand)]
enum InitAction {
    /// Inspect a CLI and generate startup-facing AI artifacts for one host profile
    Ai {
        /// Command spec to inspect.
        /// Supports shell-style quoting or a JSON array like ["gh"].
        #[arg(long = "from-cli")]
        from_cli: String,

        /// Generation coverage
        #[arg(long, value_enum, default_value = "single")]
        coverage: AiCoverage,

        /// Target host/client profile for single-host generation
        #[arg(long, value_enum)]
        client: Option<AiClientProfile>,

        /// Host/client profiles to apply when using --coverage full with --mode apply
        #[arg(long = "host", value_enum, value_delimiter = ',')]
        hosts: Vec<AiClientProfile>,

        /// Skills path to embed in generated host configs
        #[arg(long, default_value = ".claude/skills")]
        skills_path: PathBuf,

        /// Root directory for generated or applied artifacts
        #[arg(long)]
        root: Option<PathBuf>,

        /// Output mode
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,

        /// Allow inspecting sxmc itself
        #[arg(long)]
        allow_self: bool,
    },
}

#[derive(Subcommand)]
enum ScaffoldAction {
    /// Generate a SKILL.md scaffold from a CLI surface profile
    Skill {
        /// Path to a JSON profile file
        #[arg(long = "from-profile")]
        from_profile: PathBuf,

        /// Root directory for generated or applied artifacts
        #[arg(long)]
        root: Option<PathBuf>,

        /// Output directory for generated skill files
        #[arg(long, default_value = ".claude/skills")]
        output_dir: PathBuf,

        /// Output mode
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
    },

    /// Generate an agent-doc snippet/block from a CLI surface profile
    AgentDoc {
        /// Path to a JSON profile file
        #[arg(long = "from-profile")]
        from_profile: PathBuf,

        /// Target host/client profile
        #[arg(long, value_enum)]
        client: Option<AiClientProfile>,

        /// Generation coverage
        #[arg(long, value_enum, default_value = "single")]
        coverage: AiCoverage,

        /// Host/client profiles to apply when using --coverage full with --mode apply
        #[arg(long = "host", value_enum, value_delimiter = ',')]
        hosts: Vec<AiClientProfile>,

        /// Root directory for generated or applied artifacts
        #[arg(long)]
        root: Option<PathBuf>,

        /// Output mode
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
    },

    /// Generate a host-specific client config scaffold from a CLI surface profile
    ClientConfig {
        /// Path to a JSON profile file
        #[arg(long = "from-profile")]
        from_profile: PathBuf,

        /// Target host/client profile
        #[arg(long, value_enum)]
        client: Option<AiClientProfile>,

        /// Generation coverage
        #[arg(long, value_enum, default_value = "single")]
        coverage: AiCoverage,

        /// Host/client profiles to apply when using --coverage full with --mode apply
        #[arg(long = "host", value_enum, value_delimiter = ',')]
        hosts: Vec<AiClientProfile>,

        /// Skills path to embed in generated host configs
        #[arg(long, default_value = ".claude/skills")]
        skills_path: PathBuf,

        /// Root directory for generated or applied artifacts
        #[arg(long)]
        root: Option<PathBuf>,

        /// Output mode
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
    },

    /// Generate an MCP wrapper scaffold from a CLI surface profile
    McpWrapper {
        /// Path to a JSON profile file
        #[arg(long = "from-profile")]
        from_profile: PathBuf,

        /// Root directory for generated or applied artifacts
        #[arg(long)]
        root: Option<PathBuf>,

        /// Output directory for generated wrapper files
        #[arg(long, default_value = ".sxmc/mcp-wrappers")]
        output_dir: PathBuf,

        /// Output mode
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
    },

    /// Generate an optional llms.txt export from a CLI surface profile
    #[command(name = "llms-txt")]
    LlmTxt {
        /// Path to a JSON profile file
        #[arg(long = "from-profile")]
        from_profile: PathBuf,

        /// Root directory for generated or applied artifacts
        #[arg(long)]
        root: Option<PathBuf>,

        /// Output mode
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
    },
}

#[derive(Subcommand)]
enum McpAction {
    /// List baked MCP servers (stdio/http bakes only)
    Servers {
        /// Pretty-print structured output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    /// List tools for a baked MCP server
    Tools {
        /// Baked MCP server name
        server: String,

        /// Search/filter tools by name or description
        #[arg(long)]
        search: Option<String>,

        /// Maximum items to show
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Search tools across baked MCP servers
    Grep {
        /// Search pattern
        pattern: String,

        /// Restrict search to one baked MCP server
        #[arg(long)]
        server: Option<String>,

        /// Maximum matches to show
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// List prompts for a baked MCP server
    Prompts {
        /// Baked MCP server name
        server: String,

        /// Maximum items to show
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// List resources for a baked MCP server
    Resources {
        /// Baked MCP server name
        server: String,

        /// Maximum items to show
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Show detailed schema/help for one tool as SERVER/TOOL
    Info {
        /// Target tool in SERVER/TOOL form
        target: String,

        /// Pretty-print structured output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    /// Call one tool as SERVER/TOOL with optional JSON object input
    Call {
        /// Target tool in SERVER/TOOL form
        target: String,

        /// JSON object payload, or '-' to read JSON from stdin
        payload: Option<String>,

        /// Pretty-print JSON-like tool output
        #[arg(long)]
        pretty: bool,
    },
    /// Read one resource as SERVER/RESOURCE_URI
    Read {
        /// Target resource in SERVER/RESOURCE_URI form
        target: String,

        /// Pretty-print structured output
        #[arg(long)]
        pretty: bool,
    },
    /// Fetch one prompt as SERVER/PROMPT with key=value arguments
    Prompt {
        /// Target prompt in SERVER/PROMPT form
        target: String,

        /// Prompt arguments as key=value pairs
        args: Vec<String>,

        /// Pretty-print structured output
        #[arg(long)]
        pretty: bool,
    },
    /// Keep a baked MCP connection open for multi-step stateful workflows
    Session {
        /// Baked MCP server name
        server: String,

        /// Read session commands from a file instead of stdin
        #[arg(long, value_name = "FILE")]
        script: Option<PathBuf>,

        /// Suppress session banner/help text
        #[arg(long)]
        quiet: bool,
    },
}

#[derive(Parser)]
struct McpSessionCli {
    #[command(subcommand)]
    action: McpSessionAction,
}

#[derive(Subcommand, Debug)]
enum McpSessionAction {
    /// List tools on the connected MCP server
    Tools {
        /// Search/filter tools by name or description
        #[arg(long)]
        search: Option<String>,

        /// Maximum items to show
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// List prompts on the connected MCP server
    Prompts {
        /// Maximum items to show
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// List resources on the connected MCP server
    Resources {
        /// Maximum items to show
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Describe the connected MCP server surface
    Describe {
        /// Pretty-print structured output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,

        /// Maximum items to show per surface
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Show detailed schema/help for one tool
    Info {
        /// Tool name
        tool: String,

        /// Pretty-print structured output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    /// Call one tool with optional JSON object input
    Call {
        /// Tool name
        tool: String,

        /// JSON object payload, or '-' to read JSON from stdin
        payload: Option<String>,

        /// Pretty-print tool output
        #[arg(long)]
        pretty: bool,
    },
    /// Read one resource
    Read {
        /// Resource URI
        resource: String,

        /// Pretty-print output
        #[arg(long)]
        pretty: bool,
    },
    /// Fetch one prompt with optional key=value arguments
    Prompt {
        /// Prompt name
        prompt: String,

        /// Prompt arguments as key=value pairs
        args: Vec<String>,

        /// Pretty-print output
        #[arg(long)]
        pretty: bool,
    },
}

#[derive(Subcommand)]
enum SkillsAction {
    /// List discovered skills
    List {
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,
        #[arg(long)]
        json: bool,
    },
    /// Show details for a specific skill
    Info {
        name: String,
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,
    },
    /// Run a skill directly
    Run {
        name: String,
        #[arg(trailing_var_arg = true)]
        arguments: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,
    },
    /// Generate a skill from an API spec
    Create {
        /// API spec URL or file path
        source: String,

        /// Output directory for the generated skill
        #[arg(long, default_value = ".claude/skills")]
        output_dir: PathBuf,

        /// HTTP headers for fetching the spec (Key:Value)
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
    },
}

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
                    mcp_stdio::StdioClient::connect(&config.source, &env, None).await?,
                ))
            }
            SourceType::Http => {
                let headers = parse_headers(&config.auth_headers)?;
                Ok(Self::Http(
                    mcp_http::HttpClient::connect(&config.source, &headers).await?,
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
        let result = client.call_tool(name, parse_kv_args(tool_args)).await?;
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
) -> Result<()> {
    let arguments = parse_json_object_arg(payload)?;
    let result = client.call_tool(tool_name, arguments).await?;
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
        } => call_mcp_tool(client, &tool, payload, pretty).await,
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

fn print_write_outcomes(outcomes: &[cli_surfaces::WriteOutcome]) {
    for outcome in outcomes {
        match outcome.mode {
            ArtifactMode::Preview => {}
            ArtifactMode::WriteSidecar => {
                println!(
                    "Wrote sidecar for {}: {}",
                    outcome.label,
                    outcome.path.display()
                );
            }
            ArtifactMode::Patch => {}
            ArtifactMode::Apply => {
                println!("Updated {}: {}", outcome.label, outcome.path.display());
            }
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
async fn main() -> anyhow::Result<()> {
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
        } => {
            let search_paths = resolve_paths(paths);
            let required_headers = parse_headers(&require_headers)?;
            let bearer_token = parse_optional_secret(bearer_token)?;
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
        } => {
            let headers = parse_headers(&auth_headers)?;
            let client =
                ConnectedMcpClient::Http(mcp_http::HttpClient::connect(&url, &headers).await?);
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

                if format.is_some() || pretty {
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
                    let format = output::resolve_structured_format(format, pretty);
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
                let result = call_mcp_tool(&client, tool_name, payload, pretty).await;
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
        } => {
            let headers = parse_headers(&auth_headers)?;
            let client = api::ApiClient::connect(&source, &headers).await?;
            eprintln!("[sxmc] Detected {} API", client.api_type());
            cmd_api(
                &client,
                operation,
                &args,
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
        } => {
            let headers = parse_headers(&auth_headers)?;
            let spec = openapi::OpenApiSpec::load(&source, &headers).await?;
            eprintln!("[sxmc] Loaded OpenAPI spec: {}", spec.title);
            let client = api::ApiClient::OpenApi(spec);
            cmd_api(
                &client,
                operation,
                &args,
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
        } => {
            let headers = parse_headers(&auth_headers)?;
            let gql = graphql::GraphQLClient::connect(&url, &headers).await?;
            let client = api::ApiClient::GraphQL(gql);
            cmd_api(
                &client,
                operation,
                &args,
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
                let client = mcp_http::HttpClient::connect(mcp_url, &[]).await?;
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
                pretty,
                format,
                allow_self,
            } => {
                let profile = cli_surfaces::inspect_cli(&command, allow_self)?;
                let value = cli_surfaces::profile_value(&profile);
                let format = output::resolve_structured_format(format, pretty);
                println!("{}", output::format_structured_value(&value, format));
            }
            InspectAction::Profile {
                input,
                pretty,
                format,
            } => {
                let raw = std::fs::read_to_string(&input)?;
                let value: Value = serde_json::from_str(&raw)?;
                let format = output::resolve_structured_format(format, pretty);
                println!("{}", output::format_structured_value(&value, format));
            }
        },

        Commands::Init { action } => match action {
            InitAction::Ai {
                from_cli,
                coverage,
                client,
                hosts,
                skills_path,
                root,
                mode,
                allow_self,
            } => {
                let root = resolve_generation_root(root)?;
                let profile = cli_surfaces::inspect_cli(&from_cli, allow_self)?;
                let (artifacts, selected_hosts) = resolve_cli_ai_init_artifacts(
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
            } => {
                let root = resolve_generation_root(root)?;
                let profile = cli_surfaces::load_profile(&from_profile)?;
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
            } => {
                let st = parse_source_type(&source_type);
                let mut store = BakeStore::load()?;
                store.create(BakeConfig {
                    name: name.clone(),
                    source_type: st,
                    source,
                    auth_headers,
                    env_vars,
                    description,
                })?;
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
                    if let Some(ref desc) = config.description {
                        println!("Description: {}", desc);
                    }
                    if !config.auth_headers.is_empty() {
                        println!("Auth headers: {}", config.auth_headers.len());
                    }
                    if !config.env_vars.is_empty() {
                        println!("Env vars: {}", config.env_vars.len());
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
            } => {
                let mut store = BakeStore::load()?;
                let existing = match store.show(&name) {
                    Some(config) => config.clone(),
                    None => {
                        eprintln!("Bake '{}' not found", name);
                        std::process::exit(1);
                    }
                };

                let updated = BakeConfig {
                    name: name.clone(),
                    source_type: source_type
                        .as_deref()
                        .map(parse_source_type)
                        .unwrap_or(existing.source_type),
                    source: source.unwrap_or(existing.source),
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
                    description: description.or(existing.description),
                };

                store.update(updated)?;
                println!("Updated bake: {}", name);
            }
            BakeAction::Remove { name } => {
                let mut store = BakeStore::load()?;
                store.remove(&name)?;
                println!("Removed bake: {}", name);
            }
        },
    }

    Ok(())
}

fn cmd_skills_list(paths: &[PathBuf], json_output: bool) -> Result<()> {
    let skill_dirs = discovery::discover_skills(paths)?;
    let mut skills = Vec::new();

    for dir in &skill_dirs {
        let source = dir.parent().and_then(|p| p.to_str()).unwrap_or("unknown");
        match parser::parse_skill(dir, source) {
            Ok(skill) => skills.push(skill),
            Err(e) => eprintln!("Warning: {}: {}", dir.display(), e),
        }
    }

    if json_output {
        let items: Vec<serde_json::Value> = skills
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "description": s.frontmatter.description,
                    "scripts": s.scripts.iter().map(|sc| &sc.name).collect::<Vec<_>>(),
                    "references": s.references.iter().map(|r| &r.name).collect::<Vec<_>>(),
                    "source": s.source,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else if skills.is_empty() {
        println!("No skills found.");
        for p in paths {
            println!("  {}", p.display());
        }
    } else {
        for skill in &skills {
            println!("{}", skill.name);
            if !skill.frontmatter.description.is_empty() {
                println!("  {}", skill.frontmatter.description);
            }
            if !skill.scripts.is_empty() {
                let names: Vec<_> = skill.scripts.iter().map(|s| s.name.as_str()).collect();
                println!("  Tools: {}", names.join(", "));
            }
            if !skill.references.is_empty() {
                let names: Vec<_> = skill.references.iter().map(|r| r.name.as_str()).collect();
                println!("  Resources: {}", names.join(", "));
            }
            println!();
        }
    }
    Ok(())
}

fn cmd_skills_info(paths: &[PathBuf], name: &str) -> Result<()> {
    let skill_dirs = discovery::discover_skills(paths)?;

    for dir in &skill_dirs {
        let source = dir.parent().and_then(|p| p.to_str()).unwrap_or("unknown");
        if let Ok(skill) = parser::parse_skill(dir, source) {
            if skill.name == name {
                println!("Name: {}", skill.name);
                println!("Description: {}", skill.frontmatter.description);
                println!("Source: {}", skill.source);
                println!("Directory: {}", skill.base_dir.display());
                if let Some(ref hint) = skill.frontmatter.argument_hint {
                    println!("Arguments: {}", hint);
                }
                if !skill.scripts.is_empty() {
                    println!("\nScripts:");
                    for s in &skill.scripts {
                        println!("  {} -> {}", s.name, s.path.display());
                    }
                }
                if !skill.references.is_empty() {
                    println!("\nReferences:");
                    for r in &skill.references {
                        println!("  {} ({})", r.name, r.uri);
                    }
                }
                println!("\n--- Body ---");
                println!("{}", skill.body);
                return Ok(());
            }
        }
    }
    Err(sxmc::error::SxmcError::SkillNotFound(name.to_string()))
}

async fn cmd_skills_run(paths: &[PathBuf], name: &str, arguments: &[String]) -> Result<()> {
    let skill_dirs = discovery::discover_skills(paths)?;

    for dir in &skill_dirs {
        let source = dir.parent().and_then(|p| p.to_str()).unwrap_or("unknown");
        if let Ok(skill) = parser::parse_skill(dir, source) {
            if skill.name == name {
                let args_str = arguments.join(" ");
                let mut body = skill.body.clone();

                for (i, arg) in arguments.iter().enumerate().rev() {
                    body = body.replace(&format!("$ARGUMENTS[{}]", i), arg);
                    body = body.replace(&format!("${}", i), arg);
                }

                body = body.replace("$ARGUMENTS", &args_str);

                println!("{}", body);
                return Ok(());
            }
        }
    }
    Err(sxmc::error::SxmcError::SkillNotFound(name.to_string()))
}

async fn cmd_api(
    client: &api::ApiClient,
    operation: Option<String>,
    args: &[String],
    list: bool,
    search: Option<&str>,
    pretty: bool,
    format: Option<output::StructuredOutputFormat>,
) -> anyhow::Result<()> {
    if list || search.is_some() {
        if format.is_some() || pretty {
            let format = output::resolve_structured_format(format, pretty);
            println!(
                "{}",
                output::format_structured_value(&client.list_value(search), format)
            );
        } else {
            println!("{}", client.format_list(search));
        }
    } else if let Some(op_name) = operation {
        let arguments = parse_string_kv_args(args);
        let result = client.execute(&op_name, &arguments).await?;
        let format = output::resolve_structured_format(format, pretty);
        println!("{}", output::format_structured_value(&result, format));
    } else {
        eprintln!("Specify an operation name or use --list");
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_capability_not_supported, list_optional_surface, McpSurface};
    use sxmc::error::SxmcError;

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
    fn does_not_hide_real_failures() {
        let error = SxmcError::McpError("list_prompts failed: connection reset".into());
        assert!(!is_capability_not_supported(&error));
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
