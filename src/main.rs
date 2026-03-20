use clap::{Parser, Subcommand};
use std::path::PathBuf;

use skillsxmcpxcli::error::Result;
use skillsxmcpxcli::server;
use skillsxmcpxcli::skills::{discovery, parser};

#[derive(Parser)]
#[command(name = "sxmc", version, about = "AI-agnostic Skills × MCP × CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the MCP server
    Serve {
        /// Skill search paths (comma-separated)
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,

        /// Transport: stdio or sse
        #[arg(long, default_value = "stdio")]
        transport: String,

        /// Port for SSE transport
        #[arg(long, default_value = "8000")]
        port: u16,
    },

    /// Manage skills
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
}

#[derive(Subcommand)]
enum SkillsAction {
    /// List discovered skills
    List {
        /// Skill search paths
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show details for a specific skill
    Info {
        /// Skill name
        name: String,

        /// Skill search paths
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,
    },

    /// Run a skill directly
    Run {
        /// Skill name
        name: String,

        /// Arguments to pass
        #[arg(trailing_var_arg = true)]
        arguments: Vec<String>,

        /// Skill search paths
        #[arg(long, value_delimiter = ',')]
        paths: Option<Vec<PathBuf>>,
    },
}

fn resolve_paths(paths: Option<Vec<PathBuf>>) -> Vec<PathBuf> {
    paths.unwrap_or_else(discovery::default_paths)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            paths,
            transport,
            port: _port,
        } => {
            let search_paths = resolve_paths(paths);
            match transport.as_str() {
                "stdio" => {
                    server::serve_stdio(&search_paths).await?;
                }
                "sse" => {
                    eprintln!("[sxmc] SSE transport not yet implemented");
                    std::process::exit(1);
                }
                other => {
                    eprintln!("[sxmc] Unknown transport: {}", other);
                    std::process::exit(1);
                }
            }
        }

        Commands::Skills { action } => match action {
            SkillsAction::List { paths, json } => {
                let search_paths = resolve_paths(paths);
                cmd_skills_list(&search_paths, json)?;
            }
            SkillsAction::Info { name, paths } => {
                let search_paths = resolve_paths(paths);
                cmd_skills_info(&search_paths, &name)?;
            }
            SkillsAction::Run {
                name,
                arguments,
                paths,
            } => {
                let search_paths = resolve_paths(paths);
                cmd_skills_run(&search_paths, &name, &arguments).await?;
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
    } else {
        if skills.is_empty() {
            println!("No skills found.");
            println!("Search paths:");
            for p in paths {
                println!("  {}", p.display());
            }
            return Ok(());
        }

        for skill in &skills {
            println!("{}", skill.name);
            if !skill.frontmatter.description.is_empty() {
                println!("  {}", skill.frontmatter.description);
            }
            if !skill.scripts.is_empty() {
                println!(
                    "  Tools: {}",
                    skill
                        .scripts
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            if !skill.references.is_empty() {
                println!(
                    "  Resources: {}",
                    skill
                        .references
                        .iter()
                        .map(|r| r.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
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

    Err(skillsxmcpxcli::error::SxmcError::SkillNotFound(
        name.to_string(),
    ))
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

    Err(skillsxmcpxcli::error::SxmcError::SkillNotFound(
        name.to_string(),
    ))
}
