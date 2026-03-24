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

        /// Exit non-zero if startup-facing AI files are missing
        #[arg(long)]
        check: bool,

        /// Limit doctor startup-file checks to specific AI hosts
        #[arg(long = "only", value_enum, value_delimiter = ',')]
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
        #[arg(long)]
        root: Option<PathBuf>,

        /// Limit startup-file checks to specific AI hosts
        #[arg(long = "only", value_enum, value_delimiter = ',')]
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

    /// Watch saved-profile drift and environment health over time
    Watch {
        /// Project root to inspect for startup-facing AI files and saved profiles
        #[arg(long)]
        root: Option<PathBuf>,

        /// Limit startup-file checks to specific AI hosts
        #[arg(long = "only", value_enum, value_delimiter = ',')]
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
}
