use clap::{Parser, Subcommand};
use std::path::PathBuf;

use std::collections::HashMap;

use sxmc::auth::secrets::{resolve_header, resolve_secret};
use sxmc::bake::config::SourceType;
use sxmc::bake::{BakeConfig, BakeStore};
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

        /// Search/filter tools by name or description
        #[arg(long)]
        search: Option<String>,

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

        /// Search/filter tools by name or description
        #[arg(long)]
        search: Option<String>,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,

        /// HTTP headers (Key:Value)
        #[arg(long = "auth-header", value_name = "K:V")]
        auth_headers: Vec<String>,
    },

    /// Connect to any API (auto-detects OpenAPI or GraphQL)
    Api {
        /// API URL or spec file path
        source: String,

        /// Operation to call (omit for --list)
        operation: Option<String>,

        /// Arguments as key=value pairs
        #[arg(trailing_var_arg = true)]
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
        #[arg(trailing_var_arg = true)]
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
        #[arg(trailing_var_arg = true)]
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
            search,
            pretty,
            env_vars,
            cwd,
        } => {
            let env = parse_env_vars(&env_vars);
            let client = mcp_stdio::StdioClient::connect(&command, &env, cwd.as_deref()).await?;
            let (tool_name, tool_args) = args
                .split_first()
                .map(|(name, rest)| (Some(name.as_str()), rest))
                .unwrap_or((None, &[]));

            if list || search.is_some() {
                let tools = client.list_tools().await?;
                println!("{}", output::format_tool_list(&tools, search.as_deref()));

                let prompts = client.list_prompts().await?;
                if !prompts.is_empty() {
                    println!();
                    println!("{}", output::format_prompt_list(&prompts));
                }

                let resources = client.list_resources().await?;
                if !resources.is_empty() {
                    println!();
                    println!("{}", output::format_resource_list(&resources));
                }
            } else if let Some(name) = prompt {
                let arguments = parse_kv_args(&args);
                let arguments = if arguments.is_empty() {
                    None
                } else {
                    Some(arguments)
                };
                let result = client.get_prompt(&name, arguments).await?;
                println!("{}", output::format_prompt_result(&result, pretty));
            } else if let Some(uri) = resource_uri {
                let result = client.read_resource(&uri).await?;
                println!("{}", output::format_resource_result(&result, pretty));
            } else if let Some(name) = tool_name {
                let arguments = parse_kv_args(tool_args);
                let result = client.call_tool(name, arguments).await?;
                println!("{}", output::format_tool_result(&result, pretty));
            } else {
                eprintln!("Specify a tool name, --prompt, --resource, or use --list");
                std::process::exit(1);
            }

            client.close().await?;
        }

        Commands::Http {
            url,
            prompt,
            resource_uri,
            args,
            list,
            search,
            pretty,
            auth_headers,
        } => {
            let headers = parse_headers(&auth_headers)?;
            let client = mcp_http::HttpClient::connect(&url, &headers).await?;
            let (tool_name, tool_args) = args
                .split_first()
                .map(|(name, rest)| (Some(name.as_str()), rest))
                .unwrap_or((None, &[]));

            if list || search.is_some() {
                let tools = client.list_tools().await?;
                println!("{}", output::format_tool_list(&tools, search.as_deref()));

                let prompts = client.list_prompts().await?;
                if !prompts.is_empty() {
                    println!();
                    println!("{}", output::format_prompt_list(&prompts));
                }

                let resources = client.list_resources().await?;
                if !resources.is_empty() {
                    println!();
                    println!("{}", output::format_resource_list(&resources));
                }
            } else if let Some(name) = prompt {
                let arguments = parse_kv_args(&args);
                let arguments = if arguments.is_empty() {
                    None
                } else {
                    Some(arguments)
                };
                let result = client.get_prompt(&name, arguments).await?;
                println!("{}", output::format_prompt_result(&result, pretty));
            } else if let Some(uri) = resource_uri {
                let result = client.read_resource(&uri).await?;
                println!("{}", output::format_resource_result(&result, pretty));
            } else if let Some(name) = tool_name {
                let arguments = parse_kv_args(tool_args);
                let result = client.call_tool(name, arguments).await?;
                println!("{}", output::format_tool_result(&result, pretty));
            } else {
                eprintln!("Specify a tool name, --prompt, --resource, or use --list");
                std::process::exit(1);
            }

            client.close().await?;
        }

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
            for report in &reports {
                let filtered_report = report.filtered(min_severity);
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&filtered_report.format_json())?
                    );
                } else if filtered_report.is_clean() {
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
        println!("{}", client.format_list(search));
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
