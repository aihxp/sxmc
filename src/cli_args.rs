use std::path::PathBuf;

use clap::{Parser, Subcommand};

use sxmc::cli_surfaces::{AiClientProfile, AiCoverage, ArtifactMode};
use sxmc::output;

#[derive(Parser)]
#[command(name = "sxmc", version, about = "AI-agnostic Skills × MCP × CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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

        /// Maximum concurrent HTTP requests to serve
        #[arg(long, default_value_t = 64)]
        max_concurrency: usize,

        /// Maximum HTTP request body size in bytes
        #[arg(long, default_value_t = 1024 * 1024)]
        max_request_bytes: usize,
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

        /// Network timeout in seconds
        #[arg(long, value_name = "SECONDS")]
        timeout_seconds: Option<u64>,
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

        /// Network timeout in seconds
        #[arg(long, value_name = "SECONDS")]
        timeout_seconds: Option<u64>,
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

        /// Network timeout in seconds
        #[arg(long, value_name = "SECONDS")]
        timeout_seconds: Option<u64>,
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

        /// Network timeout in seconds
        #[arg(long, value_name = "SECONDS")]
        timeout_seconds: Option<u64>,
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

    /// Generate shell completion scripts
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Show startup-discovery status and recommended first sxmc commands
    Doctor {
        /// Project root to inspect for startup-facing AI files
        #[arg(long)]
        root: Option<PathBuf>,

        /// Force the human-readable report even when stdout is not a TTY
        #[arg(long)]
        human: bool,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
}

#[derive(Subcommand)]
pub enum BakeAction {
    /// Create a new baked config
    Create {
        name: String,
        #[arg(long = "type", default_value = "stdio")]
        source_type: String,
        #[arg(long)]
        source: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env_vars: Vec<String>,
        #[arg(long = "timeout-seconds", value_name = "SECONDS")]
        timeout_seconds: Option<u64>,
        #[arg(long)]
        base_dir: Option<PathBuf>,
        #[arg(long)]
        skip_validate: bool,
    },
    List,
    Show {
        name: String,
    },
    Update {
        name: String,
        #[arg(long = "type")]
        source_type: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env_vars: Vec<String>,
        #[arg(long = "timeout-seconds", value_name = "SECONDS")]
        timeout_seconds: Option<u64>,
        #[arg(long)]
        base_dir: Option<PathBuf>,
        #[arg(long)]
        skip_validate: bool,
    },
    Remove {
        name: String,
    },
}

#[derive(Subcommand)]
pub enum InspectAction {
    Cli {
        command: String,
        #[arg(long, default_value_t = 0)]
        depth: usize,
        #[arg(long)]
        compact: bool,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
        #[arg(long)]
        allow_self: bool,
    },
    Batch {
        commands: Vec<String>,
        #[arg(long, default_value_t = 0)]
        depth: usize,
        #[arg(long)]
        compact: bool,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
        #[arg(long)]
        allow_self: bool,
    },
    Profile {
        input: PathBuf,
        #[arg(long)]
        compact: bool,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    CacheStats {
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
}

#[derive(Subcommand)]
pub enum InitAction {
    Ai {
        #[arg(long = "from-cli")]
        from_cli: String,
        #[arg(long, default_value_t = 0)]
        depth: usize,
        #[arg(long, value_enum, default_value = "single")]
        coverage: AiCoverage,
        #[arg(long, value_enum)]
        client: Option<AiClientProfile>,
        #[arg(long = "host", value_enum, value_delimiter = ',')]
        hosts: Vec<AiClientProfile>,
        #[arg(long, default_value = ".claude/skills")]
        skills_path: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
        #[arg(long)]
        remove: bool,
        #[arg(long)]
        allow_low_confidence: bool,
        #[arg(long)]
        allow_self: bool,
    },
}

#[derive(Subcommand)]
pub enum ScaffoldAction {
    Skill {
        #[arg(long = "from-profile")]
        from_profile: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long, default_value = ".claude/skills")]
        output_dir: PathBuf,
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
    },
    AgentDoc {
        #[arg(long = "from-profile")]
        from_profile: PathBuf,
        #[arg(long, value_enum)]
        client: Option<AiClientProfile>,
        #[arg(long, value_enum, default_value = "single")]
        coverage: AiCoverage,
        #[arg(long = "host", value_enum, value_delimiter = ',')]
        hosts: Vec<AiClientProfile>,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
        #[arg(long)]
        allow_low_confidence: bool,
    },
    ClientConfig {
        #[arg(long = "from-profile")]
        from_profile: PathBuf,
        #[arg(long, value_enum)]
        client: Option<AiClientProfile>,
        #[arg(long, value_enum, default_value = "single")]
        coverage: AiCoverage,
        #[arg(long = "host", value_enum, value_delimiter = ',')]
        hosts: Vec<AiClientProfile>,
        #[arg(long, default_value = ".claude/skills")]
        skills_path: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
    },
    McpWrapper {
        #[arg(long = "from-profile")]
        from_profile: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long, default_value = ".sxmc/mcp-wrappers")]
        output_dir: PathBuf,
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
    },
    #[command(name = "llms-txt")]
    LlmTxt {
        #[arg(long = "from-profile")]
        from_profile: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
    },
}

#[derive(Subcommand)]
pub enum McpAction {
    Servers {
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    Tools {
        server: String,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    Grep {
        pattern: String,
        #[arg(long)]
        server: Option<String>,
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    Prompts {
        server: String,
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    Resources {
        server: String,
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    Info {
        target: String,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    Call {
        target: String,
        payload: Option<String>,
        #[arg(long)]
        pretty: bool,
    },
    Read {
        target: String,
        #[arg(long)]
        pretty: bool,
    },
    Prompt {
        target: String,
        args: Vec<String>,
        #[arg(long)]
        pretty: bool,
    },
    Session {
        server: String,
        #[arg(long, value_name = "FILE")]
        script: Option<PathBuf>,
        #[arg(long)]
        quiet: bool,
    },
}

#[derive(Parser)]
pub struct McpSessionCli {
    #[command(subcommand)]
    pub action: McpSessionAction,
}

#[derive(Subcommand, Debug)]
pub enum McpSessionAction {
    Tools {
        #[arg(long)]
        search: Option<String>,
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    Prompts {
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    Resources {
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    Describe {
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    Info {
        tool: String,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    Call {
        tool: String,
        payload: Option<String>,
        #[arg(long)]
        pretty: bool,
    },
    Read {
        resource: String,
        #[arg(long)]
        pretty: bool,
    },
    Prompt {
        prompt: String,
        args: Vec<String>,
        #[arg(long)]
        pretty: bool,
    },
}

#[derive(Subcommand)]
pub enum SkillsAction {
    List {
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,
        #[arg(long)]
        json: bool,
    },
    Info {
        name: String,
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,
    },
    Run {
        name: String,
        #[arg(trailing_var_arg = true)]
        arguments: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,
    },
    Create {
        source: String,
        #[arg(long, default_value = ".claude/skills")]
        output_dir: PathBuf,
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
    },
}
