use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use sxmc::cli_surfaces::{AiClientProfile, AiCoverage, ArtifactMode};
use sxmc::output;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum DiffOutputFormat {
    Json,
    JsonPretty,
    Toon,
    Ndjson,
    Markdown,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum DbDiscoveryType {
    Sqlite,
    Postgres,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum WatchNotificationTemplate {
    Standard,
    Compact,
    Slack,
}

impl DiffOutputFormat {
    pub fn as_structured(self) -> Option<output::StructuredOutputFormat> {
        match self {
            Self::Json => Some(output::StructuredOutputFormat::Json),
            Self::JsonPretty => Some(output::StructuredOutputFormat::JsonPretty),
            Self::Toon => Some(output::StructuredOutputFormat::Toon),
            Self::Ndjson => Some(output::StructuredOutputFormat::Ndjson),
            Self::Markdown => None,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "sxmc",
    version,
    about = "Sumac — bring out what your tools can do (Skills × MCP × CLI)"
)]
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

        /// Saved discovery snapshot file or directory to expose as MCP resources
        #[arg(long = "discovery-snapshot", value_delimiter = ',')]
        discovery_snapshots: Vec<PathBuf>,

        /// Discovery tool manifest file or directory to expose as MCP tools
        #[arg(long = "discovery-tool-manifest", value_delimiter = ',')]
        discovery_tool_manifests: Vec<PathBuf>,

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

        /// AI hosts whose MCP client config should be updated before serving
        #[arg(long = "register-host", value_enum, value_delimiter = ',')]
        register_hosts: Vec<AiClientProfile>,

        /// Project root used when writing MCP client config
        #[arg(long = "register-root")]
        register_root: Option<PathBuf>,

        /// Preview, patch, sidecar, or apply the MCP registration
        #[arg(long = "register-mode", value_enum, default_value = "apply")]
        register_mode: ArtifactMode,

        /// Optional MCP server name to register instead of the default
        #[arg(long = "register-name")]
        register_name: Option<String>,
    },

    /// Wrap a CLI as a focused MCP server
    Wrap {
        /// Command spec to wrap as MCP tools.
        /// Supports shell-style quoting or a JSON array like ["git"].
        command: String,

        /// Inspection depth used to derive tool schemas
        #[arg(long, default_value_t = 1)]
        depth: usize,

        /// Transport: stdio, http, or sse (alias for http)
        #[arg(long, default_value = "stdio")]
        transport: String,

        /// Port for HTTP transport
        #[arg(long, default_value = "8001")]
        port: u16,

        /// Host for HTTP transport
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Per-tool execution timeout in seconds
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,

        /// Emit periodic stderr progress notes for long-running wrapped tool calls
        #[arg(long, default_value_t = 0)]
        progress_seconds: u64,

        /// Working directory used when executing wrapped tools
        #[arg(long)]
        working_dir: Option<PathBuf>,

        /// Maximum stdout bytes to keep from a wrapped tool call
        #[arg(long, default_value_t = 256 * 1024)]
        max_stdout_bytes: usize,

        /// Maximum stderr bytes to keep from a wrapped tool call
        #[arg(long, default_value_t = 128 * 1024)]
        max_stderr_bytes: usize,

        /// Keep the most recent wrapped execution records as MCP-readable resources
        #[arg(long, default_value_t = 25)]
        execution_history_limit: usize,

        /// Only expose these generated MCP tool names
        #[arg(long = "allow-tool", value_delimiter = ',')]
        allow_tools: Vec<String>,

        /// Hide these generated MCP tool names
        #[arg(long = "deny-tool", value_delimiter = ',')]
        deny_tools: Vec<String>,

        /// Only expose these option/property names on generated wrapped tools
        #[arg(
            long = "allow-option",
            value_delimiter = ',',
            allow_hyphen_values = true
        )]
        allow_options: Vec<String>,

        /// Hide these option/property names on generated wrapped tools
        #[arg(
            long = "deny-option",
            value_delimiter = ',',
            allow_hyphen_values = true
        )]
        deny_options: Vec<String>,

        /// Only expose these positional/property names on generated wrapped tools
        #[arg(long = "allow-positional", value_delimiter = ',')]
        allow_positionals: Vec<String>,

        /// Hide these positional/property names on generated wrapped tools
        #[arg(long = "deny-positional", value_delimiter = ',')]
        deny_positionals: Vec<String>,

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

        /// Allow wrapping sxmc itself
        #[arg(long)]
        allow_self: bool,

        /// AI hosts whose MCP client config should be updated before serving
        #[arg(long = "register-host", value_enum, value_delimiter = ',')]
        register_hosts: Vec<AiClientProfile>,

        /// Project root used when writing MCP client config
        #[arg(long = "register-root")]
        register_root: Option<PathBuf>,

        /// Preview, patch, sidecar, or apply the MCP registration
        #[arg(long = "register-mode", value_enum, default_value = "apply")]
        register_mode: ArtifactMode,

        /// Optional MCP server name to register instead of the default
        #[arg(long = "register-name")]
        register_name: Option<String>,
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

        /// Return a compact operation listing shape
        #[arg(long)]
        compact: bool,

        /// Return only operation names
        #[arg(long)]
        names_only: bool,

        /// Return only required arguments/parameters for each operation
        #[arg(long)]
        required_only: bool,

        /// Return only counts and omit full operation arrays
        #[arg(long)]
        counts_only: bool,

        /// Omit descriptions/summaries from full operation listings
        #[arg(long)]
        no_descriptions: bool,

        /// Zero-based offset into the filtered operation list
        #[arg(long, value_name = "N")]
        offset: Option<usize>,

        /// Maximum operations to return
        #[arg(long, value_name = "N")]
        limit: Option<usize>,

        /// Keep only specific fields in each returned operation object
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,

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

    /// Discover structured interfaces from CLIs, APIs, and databases
    Discover {
        #[command(subcommand)]
        action: DiscoverAction,
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

        /// Return a compact operation listing shape
        #[arg(long)]
        compact: bool,

        /// Return only operation names
        #[arg(long)]
        names_only: bool,

        /// Return only required arguments/parameters for each operation
        #[arg(long)]
        required_only: bool,

        /// Return only counts and omit full operation arrays
        #[arg(long)]
        counts_only: bool,

        /// Omit descriptions/summaries from full operation listings
        #[arg(long)]
        no_descriptions: bool,

        /// Zero-based offset into the filtered operation list
        #[arg(long, value_name = "N")]
        offset: Option<usize>,

        /// Maximum operations to return
        #[arg(long, value_name = "N")]
        limit: Option<usize>,

        /// Keep only specific fields in each returned operation object
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,

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

        /// Return only operation names
        #[arg(long)]
        names_only: bool,

        /// Return only required arguments for each operation
        #[arg(long)]
        required_only: bool,

        /// Return only counts and omit full operation arrays
        #[arg(long)]
        counts_only: bool,

        /// Omit descriptions from full operation listings
        #[arg(long)]
        no_descriptions: bool,

        /// Zero-based offset into the filtered operation list
        #[arg(long, value_name = "N")]
        offset: Option<usize>,

        /// Maximum operations to return
        #[arg(long, value_name = "N")]
        limit: Option<usize>,

        /// Keep only specific fields in each returned operation object
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,

        /// Show a schema summary instead of listing/calling operations
        #[arg(long)]
        schema: bool,

        /// Inspect a specific GraphQL type by name
        #[arg(long = "type", value_name = "TYPE")]
        type_name: Option<String>,

        /// Write the discovered GraphQL schema summary to a JSON snapshot file
        #[arg(long)]
        output: Option<PathBuf>,

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

    /// Publish saved CLI profiles as a portable team bundle
    Publish {
        /// Destination path or HTTP(S) URL for the published bundle
        target: String,

        /// Optional explicit profile inputs (files or directories)
        inputs: Vec<PathBuf>,

        /// Project root to resolve relative profile paths from
        #[arg(long)]
        root: Option<PathBuf>,

        /// Recurse into profile directories
        #[arg(long)]
        recursive: bool,

        /// Optional bundle display name
        #[arg(long = "bundle-name")]
        bundle_name: Option<String>,

        /// Optional bundle description
        #[arg(long)]
        description: Option<String>,

        /// Optional role/scope label like backend/frontend/platform
        #[arg(long)]
        role: Option<String>,

        /// AI hosts this bundle is intended for
        #[arg(long = "hosts", value_enum, value_delimiter = ',')]
        hosts: Vec<AiClientProfile>,

        /// HTTP headers when publishing to a remote endpoint (Key:Value)
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,

        /// Network timeout in seconds for remote publish
        #[arg(long = "timeout-seconds", value_name = "SECONDS")]
        timeout_seconds: Option<u64>,

        /// Secret used to embed an HMAC signature into the exported bundle
        #[arg(
            long = "signature-secret",
            value_name = "SECRET",
            conflicts_with = "signing_key"
        )]
        signature_secret: Option<String>,

        /// Ed25519 signing key file used to sign the exported bundle
        #[arg(
            long = "signing-key",
            value_name = "PATH",
            conflicts_with = "signature_secret"
        )]
        signing_key: Option<PathBuf>,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },

    /// Pull a published profile bundle into the local saved-profile directory
    Pull {
        /// Source path or HTTP(S) URL for the bundle
        source: String,

        /// Project root to resolve relative output paths from
        #[arg(long)]
        root: Option<PathBuf>,

        /// Destination profile directory
        #[arg(long)]
        output_dir: Option<PathBuf>,

        /// Overwrite existing profile files
        #[arg(long, conflicts_with = "skip_existing")]
        overwrite: bool,

        /// Skip existing profile files
        #[arg(long, conflicts_with = "overwrite")]
        skip_existing: bool,

        /// HTTP headers when pulling from a remote endpoint (Key:Value)
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,

        /// Network timeout in seconds for remote pull
        #[arg(long = "timeout-seconds", value_name = "SECONDS")]
        timeout_seconds: Option<u64>,

        /// Require the pulled bundle to match this SHA-256 digest
        #[arg(long = "expected-sha256", value_name = "HEX")]
        expected_sha256: Option<String>,

        /// Secret used to verify an embedded HMAC signature on the pulled bundle
        #[arg(
            long = "signature-secret",
            value_name = "SECRET",
            conflicts_with = "public_key"
        )]
        signature_secret: Option<String>,

        /// Ed25519 public key file used to verify an embedded bundle signature
        #[arg(
            long = "public-key",
            value_name = "PATH",
            conflicts_with = "signature_secret"
        )]
        public_key: Option<PathBuf>,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },

    /// Inspect a CLI and add it to your AI host setup in one step
    Add {
        /// Command spec to inspect and onboard
        command: String,

        /// Inspection depth used to derive nested CLI context
        #[arg(long, default_value_t = 1)]
        depth: usize,

        /// Project root to write profiles and AI-host artifacts into
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,

        /// Write AI host artifacts into user-level host locations instead of the project
        #[arg(long, conflicts_with = "local", conflicts_with = "root")]
        global: bool,

        /// Explicitly keep AI host artifacts project-local
        #[arg(long, conflicts_with = "global")]
        local: bool,

        /// Skills path used when generating MCP client config artifacts
        #[arg(long, default_value = ".claude/skills")]
        skills_path: PathBuf,

        /// Explicit AI hosts to apply to instead of auto-detecting from the repo
        #[arg(
            long = "host",
            visible_alias = "client",
            value_enum,
            value_delimiter = ','
        )]
        hosts: Vec<AiClientProfile>,

        /// Preview changes instead of writing them
        #[arg(long)]
        preview: bool,

        /// Allow low-confidence CLI profiles to be written into AI host artifacts
        #[arg(long)]
        allow_low_confidence: bool,

        /// Allow inspecting sxmc itself
        #[arg(long)]
        allow_self: bool,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },

    /// Scan common tools or explicit selections and onboard them in one pass
    Setup {
        /// Explicit CLI tools to onboard; when omitted, Sumac scans a curated common-tool list
        #[arg(long = "tool", value_delimiter = ',')]
        tools: Vec<String>,

        /// Maximum number of auto-detected tools to onboard when --tool is omitted
        #[arg(long, default_value_t = 5)]
        limit: usize,

        /// Inspection depth used to derive nested CLI context
        #[arg(long, default_value_t = 1)]
        depth: usize,

        /// Project root to write profiles and AI-host artifacts into
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,

        /// Write AI host artifacts into user-level host locations instead of the project
        #[arg(long, conflicts_with = "local", conflicts_with = "root")]
        global: bool,

        /// Explicitly keep AI host artifacts project-local
        #[arg(long, conflicts_with = "global")]
        local: bool,

        /// Skills path used when generating MCP client config artifacts
        #[arg(long, default_value = ".claude/skills")]
        skills_path: PathBuf,

        /// Explicit AI hosts to apply to instead of auto-detecting from the repo
        #[arg(
            long = "host",
            visible_alias = "client",
            value_enum,
            value_delimiter = ','
        )]
        hosts: Vec<AiClientProfile>,

        /// Preview changes instead of writing them
        #[arg(long)]
        preview: bool,

        /// Allow low-confidence CLI profiles to be written into AI host artifacts
        #[arg(long)]
        allow_low_confidence: bool,

        /// Allow inspecting sxmc itself
        #[arg(long)]
        allow_self: bool,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
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
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,

        /// Inspect and repair user-level AI host locations instead of project-local files
        #[arg(long, conflicts_with = "local", conflicts_with = "root")]
        global: bool,

        /// Explicitly keep doctor checks project-local
        #[arg(long, conflicts_with = "global")]
        local: bool,

        /// Exit non-zero if startup-facing AI files are missing
        #[arg(long)]
        check: bool,

        /// Limit doctor startup-file checks to specific AI hosts
        #[arg(
            long = "only",
            visible_alias = "host",
            value_enum,
            value_delimiter = ','
        )]
        only_hosts: Vec<AiClientProfile>,

        /// Repair missing startup-facing files for the selected hosts
        #[arg(long, conflicts_with = "remove")]
        fix: bool,

        /// Remove startup-facing files/snippets for the selected hosts
        #[arg(long, conflicts_with = "fix")]
        remove: bool,

        /// Preview doctor repair writes without modifying files
        #[arg(long)]
        dry_run: bool,

        /// CLI to inspect when repairing startup-facing files
        #[arg(long = "from-cli")]
        from_cli: Option<String>,

        /// Inspection depth to use when repairing startup-facing files
        #[arg(long, default_value_t = 0)]
        depth: usize,

        /// Skill path to embed into generated client configs when repairing
        #[arg(long, default_value = ".claude/skills")]
        skills_path: PathBuf,

        /// Allow low-confidence startup-doc generation while repairing
        #[arg(long)]
        allow_low_confidence: bool,

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

    /// Unified status for startup files, cache, baked MCP servers, and saved CLI profiles
    Status {
        /// Project root to inspect for startup-facing AI files and saved profiles
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,

        /// Inspect user-level AI host locations instead of project-local files
        #[arg(long, conflicts_with = "local", conflicts_with = "root")]
        global: bool,

        /// Explicitly keep status checks project-local
        #[arg(long, conflicts_with = "global")]
        local: bool,

        /// Limit startup-file checks to specific AI hosts
        #[arg(
            long = "only",
            visible_alias = "host",
            value_enum,
            value_delimiter = ','
        )]
        only_hosts: Vec<AiClientProfile>,

        /// Compare capability readiness across specific AI hosts
        #[arg(long = "compare-hosts", value_enum, value_delimiter = ',')]
        compare_hosts: Vec<AiClientProfile>,

        /// Check health of baked MCP/API connections
        #[arg(long)]
        health: bool,

        /// Exit with code 1 when baked health checks report unhealthy entries
        #[arg(long, requires = "health")]
        exit_code: bool,

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

    /// Reconcile saved CLI profiles and AI-host artifacts against installed tools
    Sync {
        /// Project root to inspect for saved profiles and AI-host artifacts
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,

        /// Reconcile user-level AI host artifacts and state instead of project-local files
        #[arg(long, conflicts_with = "local", conflicts_with = "root")]
        global: bool,

        /// Explicitly keep sync project-local
        #[arg(long, conflicts_with = "global")]
        local: bool,

        /// Limit AI artifact refreshes to specific AI hosts
        #[arg(
            long = "only",
            visible_alias = "host",
            value_enum,
            value_delimiter = ','
        )]
        only_hosts: Vec<AiClientProfile>,

        /// Skills path used when regenerating MCP client config artifacts
        #[arg(long, default_value = ".claude/skills")]
        skills_path: PathBuf,

        /// Write updated profiles and AI-host artifacts instead of previewing the plan
        #[arg(long)]
        apply: bool,

        /// Exit non-zero when drift or sync errors are detected
        #[arg(long)]
        check: bool,

        /// Allow low-confidence CLI profiles to refresh AI-host artifacts
        #[arg(long)]
        allow_low_confidence: bool,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },

    /// Watch saved-profile drift and environment health over time
    Watch {
        /// Project root to inspect for startup-facing AI files and saved profiles
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,

        /// Watch user-level AI host locations instead of project-local files
        #[arg(long, conflicts_with = "local", conflicts_with = "root")]
        global: bool,

        /// Explicitly keep watch project-local
        #[arg(long, conflicts_with = "global")]
        local: bool,

        /// Limit startup-file checks to specific AI hosts
        #[arg(
            long = "only",
            visible_alias = "host",
            value_enum,
            value_delimiter = ','
        )]
        only_hosts: Vec<AiClientProfile>,

        /// Compare capability readiness across specific AI hosts
        #[arg(long = "compare-hosts", value_enum, value_delimiter = ',')]
        compare_hosts: Vec<AiClientProfile>,

        /// Check health of baked MCP/API connections
        #[arg(long)]
        health: bool,

        /// Poll interval in seconds
        #[arg(long, default_value_t = 5)]
        interval_seconds: u64,

        /// Exit with code 1 on the first observed change after the initial frame
        #[arg(long)]
        exit_on_change: bool,

        /// Exit with code 1 when any observed health frame contains unhealthy baked entries
        #[arg(long, requires = "health")]
        exit_on_unhealthy: bool,

        /// Append notification events as NDJSON to this file when watch frames change
        #[arg(long)]
        notify_file: Option<PathBuf>,

        /// Run this shell command when watch frames change
        #[arg(long)]
        notify_command: Option<String>,

        /// POST watch events as JSON to one or more webhook URLs
        #[arg(long = "notify-webhook", value_name = "URL", value_delimiter = ',')]
        notify_webhooks: Vec<String>,

        /// POST Slack-compatible watch notifications to one or more webhook URLs
        #[arg(
            long = "notify-slack-webhook",
            value_name = "URL",
            value_delimiter = ','
        )]
        notify_slack_webhooks: Vec<String>,

        /// Extra HTTP header to include with webhook notifications
        #[arg(long = "notify-header", value_name = "K:V")]
        notify_headers: Vec<String>,

        /// Payload template used for file and generic webhook notifications
        #[arg(long = "notify-template", value_enum, default_value = "standard")]
        notify_template: WatchNotificationTemplate,

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
        #[arg(long)]
        from_file: Option<PathBuf>,
        #[arg(long, value_name = "BATCH_RESULT")]
        retry_failed: Option<PathBuf>,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[arg(long, conflicts_with = "skip_existing")]
        overwrite: bool,
        #[arg(long, conflicts_with = "overwrite")]
        skip_existing: bool,
        #[arg(long, default_value_t = 0)]
        depth: usize,
        #[arg(long, value_name = "TIMESTAMP")]
        since: Option<String>,
        #[arg(long, default_value_t = 4)]
        parallel: usize,
        #[arg(long)]
        progress: bool,
        #[arg(long)]
        compact: bool,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
        #[arg(long)]
        allow_self: bool,
    },
    Diff {
        command: Option<String>,
        #[arg(long = "before")]
        before: PathBuf,
        #[arg(long = "after", conflicts_with = "command")]
        after: Option<PathBuf>,
        #[arg(long, default_value_t = 0)]
        depth: usize,
        #[arg(long)]
        exit_code: bool,
        #[arg(long, value_name = "SECONDS")]
        watch: Option<u64>,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<DiffOutputFormat>,
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
    MigrateProfile {
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    Drift {
        inputs: Vec<PathBuf>,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        recursive: bool,
        #[arg(long)]
        exit_code: bool,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
        #[arg(long)]
        allow_self: bool,
    },
    BundleExport {
        inputs: Vec<PathBuf>,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        recursive: bool,
        #[arg(long = "bundle-name")]
        bundle_name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        role: Option<String>,
        #[arg(long = "hosts", value_enum, value_delimiter = ',')]
        hosts: Vec<AiClientProfile>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long = "signature-secret", value_name = "SECRET")]
        #[arg(conflicts_with = "signing_key")]
        signature_secret: Option<String>,
        #[arg(
            long = "signing-key",
            value_name = "PATH",
            conflicts_with = "signature_secret"
        )]
        signing_key: Option<PathBuf>,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    BundleImport {
        input: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[arg(long, conflicts_with = "skip_existing")]
        overwrite: bool,
        #[arg(long, conflicts_with = "overwrite")]
        skip_existing: bool,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    BundleVerify {
        input: String,
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
        #[arg(long = "timeout-seconds", value_name = "SECONDS")]
        timeout_seconds: Option<u64>,
        #[arg(long = "expected-sha256", value_name = "HEX")]
        expected_sha256: Option<String>,
        #[arg(long = "signature-secret", value_name = "SECRET")]
        #[arg(conflicts_with = "public_key")]
        signature_secret: Option<String>,
        #[arg(
            long = "public-key",
            value_name = "PATH",
            conflicts_with = "signature_secret"
        )]
        public_key: Option<PathBuf>,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    BundleKeygen {
        #[arg(long, default_value = ".sxmc/keys")]
        output_dir: PathBuf,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    ExportCorpus {
        inputs: Vec<PathBuf>,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        recursive: bool,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    CorpusStats {
        input: PathBuf,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    CorpusQuery {
        input: PathBuf,
        #[arg(long)]
        command: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    KnownGood {
        input: PathBuf,
        #[arg(long)]
        command: String,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    TrustReport {
        input: String,
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
        #[arg(long = "timeout-seconds", value_name = "SECONDS")]
        timeout_seconds: Option<u64>,
        #[arg(long = "expected-sha256", value_name = "HEX")]
        expected_sha256: Option<String>,
        #[arg(
            long = "signature-secret",
            value_name = "SECRET",
            conflicts_with = "public_key"
        )]
        signature_secret: Option<String>,
        #[arg(
            long = "public-key",
            value_name = "PATH",
            conflicts_with = "signature_secret"
        )]
        public_key: Option<PathBuf>,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    TrustPolicy {
        input: String,
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
        #[arg(long = "timeout-seconds", value_name = "SECONDS")]
        timeout_seconds: Option<u64>,
        #[arg(long = "expected-sha256", value_name = "HEX")]
        expected_sha256: Option<String>,
        #[arg(
            long = "signature-secret",
            value_name = "SECRET",
            conflicts_with = "public_key"
        )]
        signature_secret: Option<String>,
        #[arg(
            long = "public-key",
            value_name = "PATH",
            conflicts_with = "signature_secret"
        )]
        public_key: Option<PathBuf>,
        #[arg(long)]
        require_signature: bool,
        #[arg(long)]
        require_verified_signature: bool,
        #[arg(long)]
        min_average_quality: Option<f64>,
        #[arg(long)]
        max_stale_count: Option<u64>,
        #[arg(long)]
        min_ready_count: Option<u64>,
        #[arg(long)]
        require_role: Option<String>,
        #[arg(long = "require-host", value_delimiter = ',')]
        require_hosts: Vec<String>,
        #[arg(long)]
        exit_code: bool,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    RegistryInit {
        dir: PathBuf,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    RegistryAdd {
        bundle: String,
        #[arg(long)]
        registry: PathBuf,
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
        #[arg(long = "timeout-seconds", value_name = "SECONDS")]
        timeout_seconds: Option<u64>,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    RegistryList {
        registry: PathBuf,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    RegistryServe {
        #[arg(long)]
        registry: PathBuf,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8002)]
        port: u16,
        #[arg(long, default_value_t = 64)]
        max_concurrency: usize,
        #[arg(long, default_value_t = 1024 * 1024)]
        max_request_bytes: usize,
    },
    RegistryPush {
        bundle: String,
        #[arg(long)]
        registry: String,
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
        #[arg(long = "timeout-seconds", value_name = "SECONDS")]
        timeout_seconds: Option<u64>,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    RegistrySync {
        source: String,
        #[arg(long)]
        registry: PathBuf,
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
        #[arg(long = "timeout-seconds", value_name = "SECONDS")]
        timeout_seconds: Option<u64>,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    RegistryPull {
        name: String,
        #[arg(long)]
        registry: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[arg(long, conflicts_with = "skip_existing")]
        overwrite: bool,
        #[arg(long, conflicts_with = "overwrite")]
        skip_existing: bool,
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
    CacheClear {
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    CacheInvalidate {
        command: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },
    CacheWarm {
        commands: Vec<String>,
        #[arg(long)]
        from_file: Option<PathBuf>,
        #[arg(long, default_value_t = 0)]
        depth: usize,
        #[arg(long, value_name = "TIMESTAMP")]
        since: Option<String>,
        #[arg(long, default_value_t = 4)]
        parallel: usize,
        #[arg(long)]
        progress: bool,
        #[arg(long)]
        pretty: bool,
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
        #[arg(long)]
        allow_self: bool,
    },
}

#[derive(Subcommand)]
pub enum DiscoverAction {
    /// Discover the real command surface of a CLI
    Cli {
        /// CLI tool or executable path
        command: String,

        /// Inspection depth used to derive nested command surfaces
        #[arg(long, default_value_t = 0)]
        depth: usize,

        /// Return a compact profile shape
        #[arg(long)]
        compact: bool,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,

        /// Allow inspecting sxmc itself
        #[arg(long)]
        allow_self: bool,
    },

    /// Discover operations from an API source (auto-detects OpenAPI or GraphQL)
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

        /// Return a compact operation listing shape
        #[arg(long)]
        compact: bool,

        /// Return only operation names
        #[arg(long)]
        names_only: bool,

        /// Return only required arguments/parameters for each operation
        #[arg(long)]
        required_only: bool,

        /// Return only counts and omit full operation arrays
        #[arg(long)]
        counts_only: bool,

        /// Omit descriptions/summaries from full operation listings
        #[arg(long)]
        no_descriptions: bool,

        /// Zero-based offset into the filtered operation list
        #[arg(long, value_name = "N")]
        offset: Option<usize>,

        /// Maximum operations to return
        #[arg(long, value_name = "N")]
        limit: Option<usize>,

        /// Keep only specific fields in each returned operation object
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,

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

    /// Discover operations from a GraphQL endpoint explicitly
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

        /// Return only operation names
        #[arg(long)]
        names_only: bool,

        /// Return only required arguments for each operation
        #[arg(long)]
        required_only: bool,

        /// Return only counts and omit full operation arrays
        #[arg(long)]
        counts_only: bool,

        /// Omit descriptions from full operation listings
        #[arg(long)]
        no_descriptions: bool,

        /// Zero-based offset into the filtered operation list
        #[arg(long, value_name = "N")]
        offset: Option<usize>,

        /// Maximum operations to return
        #[arg(long, value_name = "N")]
        limit: Option<usize>,

        /// Keep only specific fields in each returned operation object
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,

        /// Show a schema summary instead of listing/calling operations
        #[arg(long)]
        schema: bool,

        /// Inspect a specific GraphQL type by name
        #[arg(long = "type", value_name = "TYPE")]
        type_name: Option<String>,

        /// Write the discovered GraphQL schema summary to a JSON snapshot file
        #[arg(long)]
        output: Option<PathBuf>,

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

    /// Compare saved and live GraphQL schema snapshots
    #[command(name = "graphql-diff")]
    GraphqlDiff {
        /// Saved GraphQL schema snapshot
        #[arg(long)]
        before: PathBuf,

        /// Optional second snapshot to compare against instead of a live URL
        #[arg(long)]
        after: Option<PathBuf>,

        /// GraphQL endpoint to inspect when --after is omitted
        #[arg(long)]
        url: Option<String>,

        /// HTTP headers (Key:Value) for live introspection
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,

        /// Network timeout in seconds
        #[arg(long, value_name = "SECONDS")]
        timeout_seconds: Option<u64>,

        /// Exit non-zero when differences are found
        #[arg(long)]
        exit_code: bool,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },

    /// Discover tables, columns, relations, and indexes from SQLite or PostgreSQL
    Db {
        /// SQLite database file path or PostgreSQL connection string
        source: String,

        /// Show details for a single table or view
        table: Option<String>,

        /// List matching tables/views
        #[arg(long)]
        list: bool,

        /// Force the database type instead of auto-detecting from the source
        #[arg(long = "database-type", value_enum)]
        database_type: Option<DbDiscoveryType>,

        /// Search/filter by table name or SQL definition
        #[arg(long)]
        search: Option<String>,

        /// Write the discovered database surface to a JSON snapshot file
        #[arg(long)]
        output: Option<PathBuf>,

        /// Return a compact summary without full column/index/relation arrays
        #[arg(long)]
        compact: bool,

        /// Return only count metadata without full entry arrays
        #[arg(long)]
        counts_only: bool,

        /// Zero-based offset into each returned entry collection
        #[arg(long, value_name = "N")]
        offset: Option<usize>,

        /// Maximum entries to return from each collection
        #[arg(long, value_name = "N")]
        limit: Option<usize>,

        /// Keep only specific fields in each returned entry object
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },

    /// Discover manifests, task runners, workflows, and entrypoints in a codebase
    Codebase {
        /// Codebase root (defaults to current working directory)
        root: Option<PathBuf>,

        /// Write the discovered codebase surface to a JSON snapshot file
        #[arg(long)]
        output: Option<PathBuf>,

        /// Return a compact summary without full manifest/config arrays
        #[arg(long)]
        compact: bool,

        /// Return only count metadata without full collection arrays
        #[arg(long)]
        counts_only: bool,

        /// Zero-based offset into each returned collection
        #[arg(long, value_name = "N")]
        offset: Option<usize>,

        /// Maximum items to return from each collection
        #[arg(long, value_name = "N")]
        limit: Option<usize>,

        /// Keep only specific fields in each returned collection item
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },

    /// Discover request/response surfaces from HAR captures or saved curl history
    Traffic {
        /// HAR file path or curl/history text file
        source: PathBuf,

        /// Show details for a single endpoint key, host, or path
        endpoint: Option<String>,

        /// Write the discovered traffic surface to a JSON snapshot file
        #[arg(long)]
        output: Option<PathBuf>,

        /// List matching endpoints
        #[arg(long)]
        list: bool,

        /// Search/filter by method, host, path, URL, or content type
        #[arg(long)]
        search: Option<String>,

        /// Return a compact summary without full endpoint arrays
        #[arg(long)]
        compact: bool,

        /// Return only count metadata without full endpoint arrays
        #[arg(long)]
        counts_only: bool,

        /// Zero-based offset into the filtered endpoint list
        #[arg(long, value_name = "N")]
        offset: Option<usize>,

        /// Maximum endpoints to return
        #[arg(long, value_name = "N")]
        limit: Option<usize>,

        /// Keep only specific fields in each returned endpoint object
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },

    /// Compare saved and live traffic discovery snapshots
    #[command(name = "traffic-diff")]
    TrafficDiff {
        /// Saved traffic discovery snapshot
        #[arg(long)]
        before: PathBuf,

        /// Optional second snapshot to compare against instead of a live capture source
        #[arg(long)]
        after: Option<PathBuf>,

        /// HAR file or curl/history source to inspect when --after is omitted
        #[arg(long)]
        source: Option<PathBuf>,

        /// Exit non-zero when differences are found
        #[arg(long)]
        exit_code: bool,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
        #[arg(long, value_enum)]
        format: Option<output::StructuredOutputFormat>,
    },

    /// Compare saved and live codebase discovery snapshots
    #[command(name = "codebase-diff")]
    CodebaseDiff {
        /// Saved codebase discovery snapshot
        #[arg(long)]
        before: PathBuf,

        /// Optional second snapshot to compare against instead of a live root
        #[arg(long)]
        after: Option<PathBuf>,

        /// Live codebase root (defaults to current working directory when --after is omitted)
        #[arg(long)]
        root: Option<PathBuf>,

        /// Exit non-zero when differences are found
        #[arg(long)]
        exit_code: bool,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// Structured output format
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
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,
        #[arg(long, conflicts_with = "local", conflicts_with = "root")]
        global: bool,
        #[arg(long, conflicts_with = "global")]
        local: bool,
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
        #[arg(long)]
        remove: bool,
        #[arg(long)]
        allow_low_confidence: bool,
        #[arg(long)]
        allow_self: bool,
    },
    Discovery {
        /// Saved discovery snapshot from `sxmc discover ... --output`
        snapshot: PathBuf,

        /// Target a single AI host or all supported hosts
        #[arg(long, value_enum, default_value = "single")]
        coverage: AiCoverage,

        /// Single AI host to target
        #[arg(long, value_enum)]
        client: Option<AiClientProfile>,

        /// Hosts to apply when using full coverage in apply mode
        #[arg(long = "host", value_enum, value_delimiter = ',')]
        hosts: Vec<AiClientProfile>,

        /// Project root to write docs/config into
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,
        #[arg(long, conflicts_with = "local", conflicts_with = "root")]
        global: bool,
        #[arg(long, conflicts_with = "global")]
        local: bool,

        /// Preview, patch, sidecar, or apply the generated artifacts
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
    },
}

#[derive(Subcommand)]
pub enum ScaffoldAction {
    Ci {
        #[arg(long = "from-profile")]
        from_profile: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long, default_value = ".github/workflows")]
        output_dir: PathBuf,
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
    },
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
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,
        #[arg(long, conflicts_with = "local", conflicts_with = "root")]
        global: bool,
        #[arg(long, conflicts_with = "global")]
        local: bool,
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
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,
        #[arg(long, conflicts_with = "local", conflicts_with = "root")]
        global: bool,
        #[arg(long, conflicts_with = "global")]
        local: bool,
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
    DiscoveryPack {
        #[arg(long = "from-snapshot")]
        from_snapshot: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long, default_value = ".sxmc/discovery-pack")]
        output_dir: PathBuf,
        #[arg(long, value_enum, default_value = "preview")]
        mode: ArtifactMode,
    },
    DiscoveryTools {
        #[arg(long = "from-snapshot")]
        from_snapshot: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long, default_value = ".sxmc/discovery-tools")]
        output_dir: PathBuf,
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
        #[arg(long, conflicts_with = "paths")]
        installed: bool,
        #[arg(long, default_value = ".claude/skills", conflicts_with = "paths")]
        skills_path: PathBuf,
        #[arg(long, conflicts_with = "global")]
        local: bool,
        #[arg(long, conflicts_with_all = ["local", "root", "paths"])]
        global: bool,
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        names_only: bool,
        #[arg(long)]
        counts_only: bool,
        #[arg(long)]
        no_descriptions: bool,
        #[arg(long, value_delimiter = ',')]
        fields: Option<Vec<String>>,
        #[arg(long, value_name = "N")]
        offset: Option<usize>,
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    Info {
        name: String,
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,
        #[arg(long)]
        summary_only: bool,
    },
    Run {
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,
        #[arg(long)]
        script: Option<String>,
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env_vars: Vec<String>,
        #[arg(long)]
        print_body: bool,
        name: String,
        #[arg(trailing_var_arg = true)]
        arguments: Vec<String>,
    },
    Create {
        source: String,
        #[arg(long, default_value = ".claude/skills")]
        output_dir: PathBuf,
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
    },
    Install {
        source: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        r#ref: Option<String>,
        #[arg(long, default_value = ".claude/skills")]
        skills_path: PathBuf,
        #[arg(long, conflicts_with = "global")]
        local: bool,
        #[arg(long, conflicts_with_all = ["local", "root"])]
        global: bool,
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,
    },
    Update {
        name: Option<String>,
        #[arg(long, default_value = ".claude/skills")]
        skills_path: PathBuf,
        #[arg(long, conflicts_with = "global")]
        local: bool,
        #[arg(long, conflicts_with_all = ["local", "root"])]
        global: bool,
        #[arg(long, conflicts_with = "global")]
        root: Option<PathBuf>,
    },
}
