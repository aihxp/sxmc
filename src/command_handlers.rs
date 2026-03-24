use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;

use sxmc::client::api;
use sxmc::error::{Result, SxmcError};
use sxmc::output;
use sxmc::skills::{discovery, models::Skill, parser};
use tokio::process::Command;

pub fn cmd_skills_list(paths: &[PathBuf], json_output: bool) -> Result<()> {
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

pub fn cmd_skills_info(paths: &[PathBuf], name: &str) -> Result<()> {
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

pub async fn cmd_skills_run(
    paths: &[PathBuf],
    name: &str,
    script: Option<&str>,
    env_vars: &[String],
    print_body: bool,
    arguments: &[String],
) -> Result<()> {
    let skill_dirs = discovery::discover_skills(paths)?;

    for dir in &skill_dirs {
        let source = dir.parent().and_then(|p| p.to_str()).unwrap_or("unknown");
        if let Ok(skill) = parser::parse_skill(dir, source) {
            if skill.name == name {
                let body = interpolate_skill_body(&skill.body, arguments);
                if print_body || skill.scripts.is_empty() {
                    println!("{}", body);
                    return Ok(());
                }

                let selected_script = select_skill_script(&skill, script)?;
                let env_pairs = parse_skill_env_vars(env_vars)?;
                execute_skill_script(&skill, selected_script.path.clone(), &env_pairs, arguments)
                    .await?;
                return Ok(());
            }
        }
    }
    Err(sxmc::error::SxmcError::SkillNotFound(name.to_string()))
}

fn interpolate_skill_body(body: &str, arguments: &[String]) -> String {
    let args_str = arguments.join(" ");
    let mut rendered = body.to_string();

    for (i, arg) in arguments.iter().enumerate().rev() {
        rendered = rendered.replace(&format!("$ARGUMENTS[{}]", i), arg);
        rendered = rendered.replace(&format!("${}", i), arg);
    }

    rendered.replace("$ARGUMENTS", &args_str)
}

fn select_skill_script<'a>(
    skill: &'a Skill,
    requested_script: Option<&str>,
) -> Result<&'a sxmc::skills::models::SkillScript> {
    if let Some(requested_script) = requested_script {
        let requested_lower = requested_script.to_ascii_lowercase();
        return skill
            .scripts
            .iter()
            .find(|candidate| {
                candidate.name.eq_ignore_ascii_case(requested_script)
                    || candidate
                        .path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .map(|stem| stem.eq_ignore_ascii_case(&requested_lower))
                        .unwrap_or(false)
            })
            .ok_or_else(|| {
                let available = skill
                    .scripts
                    .iter()
                    .map(|item| item.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                SxmcError::ExecutionError(format!(
                    "Skill `{}` does not have a script named `{}`. Available scripts: {}",
                    skill.name, requested_script, available
                ))
            });
    }

    match skill.scripts.as_slice() {
        [only] => Ok(only),
        [] => Err(SxmcError::ExecutionError(format!(
            "Skill `{}` does not define any runnable scripts",
            skill.name
        ))),
        _ => {
            let available = skill
                .scripts
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(SxmcError::ExecutionError(format!(
                "Skill `{}` has multiple scripts ({}). Re-run with --script <name> or --print-body.",
                skill.name, available
            )))
        }
    }
}

fn parse_skill_env_vars(env_vars: &[String]) -> Result<Vec<(String, String)>> {
    env_vars
        .iter()
        .map(|entry| {
            let Some((key, value)) = entry.split_once('=') else {
                return Err(SxmcError::ExecutionError(format!(
                    "Invalid --env value `{}`. Expected KEY=VALUE.",
                    entry
                )));
            };
            if key.trim().is_empty() {
                return Err(SxmcError::ExecutionError(format!(
                    "Invalid --env value `{}`. Environment variable name cannot be empty.",
                    entry
                )));
            }
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

async fn execute_skill_script(
    skill: &Skill,
    script_path: PathBuf,
    env_pairs: &[(String, String)],
    arguments: &[String],
) -> Result<()> {
    let mut command = Command::new(&script_path);
    command
        .args(arguments)
        .current_dir(&skill.base_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("SXMC_SKILL_NAME", &skill.name)
        .env("SXMC_SKILL_DIR", &skill.base_dir)
        .env("SXMC_SKILL_ARGUMENTS", arguments.join(" "));

    for (key, value) in env_pairs {
        command.env(key, value);
    }

    let output = command.output().await.map_err(|error| {
        SxmcError::ExecutionError(format!(
            "Failed to run {}: {}",
            script_path.display(),
            error
        ))
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        print!("{}", stdout);
    }
    if !stderr.is_empty() {
        eprint!("{}", stderr);
    }

    if output.status.success() {
        Ok(())
    } else {
        Err(SxmcError::ExecutionError(format!(
            "Skill script `{}` exited with status {}",
            script_path.display(),
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string())
        )))
    }
}

pub async fn cmd_api(
    client: &api::ApiClient,
    operation: Option<String>,
    arguments: &HashMap<String, String>,
    list: bool,
    search: Option<&str>,
    pretty: bool,
    format: Option<output::StructuredOutputFormat>,
) -> Result<()> {
    if list || search.is_some() {
        if let Some(format) = output::prefer_structured_output(format, pretty) {
            println!(
                "{}",
                output::format_structured_value(&client.list_value(search), format)
            );
        } else {
            println!("{}", client.format_list(search));
        }
    } else if let Some(op_name) = operation {
        let result = client.execute(&op_name, arguments).await?;
        let format = output::resolve_structured_format(format, pretty);
        println!("{}", output::format_structured_value(&result, format));
    } else {
        eprintln!("Specify an operation name or use --list");
        std::process::exit(1);
    }
    Ok(())
}
