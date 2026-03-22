use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result as AnyhowResult;

use sxmc::client::api;
use sxmc::error::Result;
use sxmc::output;
use sxmc::skills::{discovery, parser};

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

pub async fn cmd_skills_run(paths: &[PathBuf], name: &str, arguments: &[String]) -> Result<()> {
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

pub async fn cmd_api(
    client: &api::ApiClient,
    operation: Option<String>,
    arguments: &HashMap<String, String>,
    list: bool,
    search: Option<&str>,
    pretty: bool,
    format: Option<output::StructuredOutputFormat>,
) -> AnyhowResult<()> {
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
