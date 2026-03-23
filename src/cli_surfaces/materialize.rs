use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::cli_surfaces::model::{
    host_profile_spec, AiClientProfile, ApplyStrategy, ArtifactAudience, ArtifactMode,
    CliSurfaceProfile, ConfigShape, GeneratedArtifact, WriteOutcome, WriteStatus, AI_HOST_SPECS,
};
use crate::cli_surfaces::render::{
    render_agent_doc, render_ci_workflow, render_client_config, render_llms_txt,
    render_mcp_wrapper_readme, render_portable_agent_doc, render_skill_markdown, slugify,
};
use crate::error::{Result, SxmcError};

pub fn generate_profile_artifact(
    profile: &CliSurfaceProfile,
    root: &Path,
) -> Result<GeneratedArtifact> {
    let slug = slugify(&profile.command);
    let target_path = root
        .join(".sxmc")
        .join("ai")
        .join("profiles")
        .join(format!("{slug}.json"));
    let content = serde_json::to_string_pretty(profile)?;
    Ok(GeneratedArtifact {
        label: "CLI profile".into(),
        target_path,
        content,
        apply_strategy: ApplyStrategy::SidecarOnly,
        audience: ArtifactAudience::Shared,
        sidecar_scope: "profiles".into(),
    })
}

pub fn generate_agent_doc_artifact(
    profile: &CliSurfaceProfile,
    client: AiClientProfile,
    root: &Path,
) -> GeneratedArtifact {
    let spec = host_profile_spec(client);
    let target_path = root.join(spec.native_doc_target.unwrap_or("AGENTS.md"));
    let content = render_agent_doc(profile, client);
    GeneratedArtifact {
        label: format!("{} agent doc", spec.label),
        target_path,
        content,
        apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
        audience: ArtifactAudience::Client(client),
        sidecar_scope: spec.sidecar_scope.into(),
    }
}

pub fn generate_portable_agent_doc_artifact(
    profile: &CliSurfaceProfile,
    root: &Path,
) -> GeneratedArtifact {
    GeneratedArtifact {
        label: "Portable agent doc".into(),
        target_path: root.join("AGENTS.md"),
        content: render_portable_agent_doc(profile),
        apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
        audience: ArtifactAudience::Portable,
        sidecar_scope: "portable".into(),
    }
}

pub fn generate_host_native_agent_doc_artifacts(
    profile: &CliSurfaceProfile,
    root: &Path,
) -> Vec<GeneratedArtifact> {
    AI_HOST_SPECS
        .iter()
        .filter(|spec| {
            spec.native_doc_target.is_some()
                && !matches!(
                    spec.client,
                    AiClientProfile::GenericStdioMcp | AiClientProfile::GenericHttpMcp
                )
        })
        .map(|spec| GeneratedArtifact {
            label: format!("{} agent doc", spec.label),
            target_path: root.join(spec.native_doc_target.expect("checked above")),
            content: render_agent_doc(profile, spec.client),
            apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
            audience: ArtifactAudience::Client(spec.client),
            sidecar_scope: spec.sidecar_scope.into(),
        })
        .collect()
}

pub fn generate_full_coverage_init_artifacts(
    profile: &CliSurfaceProfile,
    root: &Path,
    skills_path: &Path,
) -> Result<Vec<GeneratedArtifact>> {
    let mut artifacts = vec![generate_profile_artifact(profile, root)?];
    artifacts.push(generate_portable_agent_doc_artifact(profile, root));
    artifacts.extend(generate_host_native_agent_doc_artifacts(profile, root));

    for spec in AI_HOST_SPECS {
        if let Some(artifact) =
            generate_client_config_artifact(profile, spec.client, root, skills_path)
        {
            artifacts.push(artifact);
        }
    }

    Ok(artifacts)
}

pub fn generate_client_config_artifact(
    profile: &CliSurfaceProfile,
    client: AiClientProfile,
    root: &Path,
    skills_path: &Path,
) -> Option<GeneratedArtifact> {
    let spec = host_profile_spec(client);
    let target_path = root.join(spec.native_config_target?);
    let absolute_skills_path = if skills_path.is_absolute() {
        skills_path.to_path_buf()
    } else {
        root.join(skills_path)
    };
    let server_name = format!("sxmc-cli-ai-{}", slugify(&profile.command));
    let content = render_client_config(client, &server_name, &absolute_skills_path);
    let apply_strategy = match spec.config_shape {
        Some(ConfigShape::JsonMcpServers) | Some(ConfigShape::JsonMcp) => {
            ApplyStrategy::JsonMcpConfig
        }
        Some(ConfigShape::TomlMcpServers) => ApplyStrategy::TomlManagedBlock,
        None => ApplyStrategy::SidecarOnly,
    };

    Some(GeneratedArtifact {
        label: format!("{} client config", spec.label),
        target_path,
        content,
        apply_strategy,
        audience: ArtifactAudience::Client(client),
        sidecar_scope: spec.sidecar_scope.into(),
    })
}

pub fn generate_skill_artifacts(
    profile: &CliSurfaceProfile,
    root: &Path,
    output_dir: &Path,
) -> Vec<GeneratedArtifact> {
    let slug = slugify(&profile.command);
    let skill_dir = if output_dir.is_absolute() {
        output_dir.join(format!("{slug}-cli"))
    } else {
        root.join(output_dir).join(format!("{slug}-cli"))
    };

    vec![GeneratedArtifact {
        label: "Skill scaffold".into(),
        target_path: skill_dir.join("SKILL.md"),
        content: render_skill_markdown(profile),
        apply_strategy: ApplyStrategy::DirectWrite,
        audience: ArtifactAudience::Shared,
        sidecar_scope: "skills".into(),
    }]
}

pub fn generate_ci_workflow_artifact(
    profile: &CliSurfaceProfile,
    root: &Path,
    output_dir: &Path,
) -> GeneratedArtifact {
    let slug = slugify(&profile.command);
    let workflow_dir = if output_dir.is_absolute() {
        output_dir.to_path_buf()
    } else {
        root.join(output_dir)
    };
    GeneratedArtifact {
        label: "CI drift workflow".into(),
        target_path: workflow_dir.join(format!("sxmc-drift-{slug}.yml")),
        content: render_ci_workflow(profile),
        apply_strategy: ApplyStrategy::DirectWrite,
        audience: ArtifactAudience::Shared,
        sidecar_scope: "ci".into(),
    }
}

pub fn generate_mcp_wrapper_artifacts(
    profile: &CliSurfaceProfile,
    root: &Path,
    output_dir: &Path,
) -> Result<Vec<GeneratedArtifact>> {
    let slug = slugify(&profile.command);
    let wrapper_dir = if output_dir.is_absolute() {
        output_dir.join(format!("{slug}-mcp-wrapper"))
    } else {
        root.join(output_dir).join(format!("{slug}-mcp-wrapper"))
    };
    let manifest = json!({
        "name": format!("{slug}-mcp-wrapper"),
        "source_command": profile.command,
        "summary": profile.summary,
        "notes": [
            "Wrap the CLI as a focused MCP server instead of mirroring every subcommand.",
            "Prefer a few narrow tools first and keep outputs machine-friendly.",
            "Use the profile and examples to decide what becomes a tool, prompt, or resource."
        ],
        "suggested_tools": profile.subcommands.iter().take(5).map(|subcommand| {
            json!({
                "name": subcommand.name,
                "summary": subcommand.summary,
                "confidence": subcommand.confidence
            })
        }).collect::<Vec<_>>(),
        "environment": profile.environment,
        "examples": profile.examples,
    });

    Ok(vec![
        GeneratedArtifact {
            label: "MCP wrapper README".into(),
            target_path: wrapper_dir.join("README.md"),
            content: render_mcp_wrapper_readme(profile),
            apply_strategy: ApplyStrategy::DirectWrite,
            audience: ArtifactAudience::Shared,
            sidecar_scope: "mcp-wrapper".into(),
        },
        GeneratedArtifact {
            label: "MCP wrapper manifest".into(),
            target_path: wrapper_dir.join("manifest.json"),
            content: serde_json::to_string_pretty(&manifest)?,
            apply_strategy: ApplyStrategy::DirectWrite,
            audience: ArtifactAudience::Shared,
            sidecar_scope: "mcp-wrapper".into(),
        },
    ])
}

pub fn generate_llms_txt_artifact(profile: &CliSurfaceProfile, root: &Path) -> GeneratedArtifact {
    GeneratedArtifact {
        label: "llms.txt export".into(),
        target_path: root.join("llms.txt"),
        content: render_llms_txt(profile),
        apply_strategy: ApplyStrategy::DirectWrite,
        audience: ArtifactAudience::Shared,
        sidecar_scope: "llms".into(),
    }
}

pub fn materialize_artifacts(
    artifacts: &[GeneratedArtifact],
    mode: ArtifactMode,
    root: &Path,
) -> Result<Vec<WriteOutcome>> {
    let mut outcomes = Vec::new();
    for artifact in artifacts {
        match mode {
            ArtifactMode::Preview => {
                println!(
                    "== {} ==\nTarget: {}\n\n{}\n",
                    artifact.label,
                    artifact.target_path.display(),
                    artifact.content.trim_end()
                );
                outcomes.push(WriteOutcome {
                    label: artifact.label.clone(),
                    path: artifact.target_path.clone(),
                    mode,
                    status: WriteStatus::Skipped,
                });
            }
            ArtifactMode::WriteSidecar => {
                let path = sidecar_path(&artifact.sidecar_scope, root, &artifact.target_path);
                let status = write_with_status(&path, &artifact.content)?;
                outcomes.push(WriteOutcome {
                    label: artifact.label.clone(),
                    path,
                    mode,
                    status,
                });
            }
            ArtifactMode::Patch => {
                println!("{}", render_patch_preview(artifact, root)?);
                outcomes.push(WriteOutcome {
                    label: artifact.label.clone(),
                    path: artifact.target_path.clone(),
                    mode,
                    status: WriteStatus::Skipped,
                });
            }
            ArtifactMode::Apply => {
                let (path, status) = apply_artifact(artifact, root)?;
                outcomes.push(WriteOutcome {
                    label: artifact.label.clone(),
                    path,
                    mode,
                    status,
                });
            }
        }
    }
    Ok(outcomes)
}

pub fn preview_artifacts(
    artifacts: &[GeneratedArtifact],
    mode: ArtifactMode,
    root: &Path,
) -> Result<Vec<WriteOutcome>> {
    let mut outcomes = Vec::new();
    for artifact in artifacts {
        let (path, status) = planned_artifact_write(artifact, mode, root)?;
        outcomes.push(WriteOutcome {
            label: artifact.label.clone(),
            path,
            mode,
            status,
        });
    }
    Ok(outcomes)
}

pub fn materialize_artifacts_with_apply_selection(
    artifacts: &[GeneratedArtifact],
    mode: ArtifactMode,
    root: &Path,
    selected_clients: &[AiClientProfile],
) -> Result<Vec<WriteOutcome>> {
    let mut outcomes = Vec::new();
    for artifact in artifacts {
        let effective_mode = if mode == ArtifactMode::Apply {
            match artifact.audience {
                ArtifactAudience::Shared => ArtifactMode::Apply,
                ArtifactAudience::Portable => {
                    if selected_clients.is_empty() {
                        ArtifactMode::WriteSidecar
                    } else {
                        ArtifactMode::Apply
                    }
                }
                ArtifactAudience::Client(client) => {
                    if selected_clients.contains(&client) {
                        ArtifactMode::Apply
                    } else {
                        ArtifactMode::WriteSidecar
                    }
                }
            }
        } else {
            mode
        };

        outcomes.extend(materialize_artifacts(
            std::slice::from_ref(artifact),
            effective_mode,
            root,
        )?);
    }
    Ok(outcomes)
}

pub fn preview_artifacts_with_apply_selection(
    artifacts: &[GeneratedArtifact],
    mode: ArtifactMode,
    root: &Path,
    selected_clients: &[AiClientProfile],
) -> Result<Vec<WriteOutcome>> {
    let mut outcomes = Vec::new();
    for artifact in artifacts {
        let effective_mode = if mode == ArtifactMode::Apply {
            match artifact.audience {
                ArtifactAudience::Shared => ArtifactMode::Apply,
                ArtifactAudience::Portable => {
                    if selected_clients.is_empty() {
                        ArtifactMode::WriteSidecar
                    } else {
                        ArtifactMode::Apply
                    }
                }
                ArtifactAudience::Client(client) => {
                    if selected_clients.contains(&client) {
                        ArtifactMode::Apply
                    } else {
                        ArtifactMode::WriteSidecar
                    }
                }
            }
        } else {
            mode
        };

        outcomes.extend(preview_artifacts(
            std::slice::from_ref(artifact),
            effective_mode,
            root,
        )?);
    }
    Ok(outcomes)
}

pub fn remove_artifacts(
    artifacts: &[GeneratedArtifact],
    mode: ArtifactMode,
    root: &Path,
) -> Result<Vec<WriteOutcome>> {
    let mut outcomes = Vec::new();
    for artifact in artifacts {
        match mode {
            ArtifactMode::Preview => {
                println!(
                    "Would remove {}: {}",
                    artifact.label,
                    artifact.target_path.display()
                );
                outcomes.push(WriteOutcome {
                    label: artifact.label.clone(),
                    path: artifact.target_path.clone(),
                    mode,
                    status: WriteStatus::Skipped,
                });
            }
            ArtifactMode::WriteSidecar => {
                let path = sidecar_path(&artifact.sidecar_scope, root, &artifact.target_path);
                remove_path_if_exists(&path)?;
                outcomes.push(WriteOutcome {
                    label: artifact.label.clone(),
                    path,
                    mode,
                    status: WriteStatus::Removed,
                });
            }
            ArtifactMode::Patch => {
                println!("{}", render_remove_patch_preview(artifact, root)?);
                outcomes.push(WriteOutcome {
                    label: artifact.label.clone(),
                    path: artifact.target_path.clone(),
                    mode,
                    status: WriteStatus::Skipped,
                });
            }
            ArtifactMode::Apply => {
                let path = remove_artifact(artifact, root)?;
                outcomes.push(WriteOutcome {
                    label: artifact.label.clone(),
                    path,
                    mode,
                    status: WriteStatus::Removed,
                });
            }
        }
    }
    Ok(outcomes)
}

pub fn remove_artifacts_with_apply_selection(
    artifacts: &[GeneratedArtifact],
    mode: ArtifactMode,
    root: &Path,
    selected_clients: &[AiClientProfile],
) -> Result<Vec<WriteOutcome>> {
    let mut outcomes = Vec::new();
    for artifact in artifacts {
        let effective_mode = if mode == ArtifactMode::Apply {
            match artifact.audience {
                ArtifactAudience::Shared => ArtifactMode::Apply,
                ArtifactAudience::Portable => {
                    if selected_clients.is_empty() {
                        ArtifactMode::WriteSidecar
                    } else {
                        ArtifactMode::Apply
                    }
                }
                ArtifactAudience::Client(client) => {
                    if selected_clients.contains(&client) {
                        ArtifactMode::Apply
                    } else {
                        ArtifactMode::WriteSidecar
                    }
                }
            }
        } else {
            mode
        };

        outcomes.extend(remove_artifacts(
            std::slice::from_ref(artifact),
            effective_mode,
            root,
        )?);
    }
    Ok(outcomes)
}

fn sidecar_path(scope: &str, root: &Path, original_target: &Path) -> PathBuf {
    let file_name = original_target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact.txt");
    let sidecar_name = format!("{}.sxmc.snippet", file_name);
    root.join(".sxmc")
        .join("ai")
        .join(slugify(scope))
        .join(sidecar_name)
}

fn render_patch_preview(artifact: &GeneratedArtifact, root: &Path) -> Result<String> {
    let existing = if artifact.target_path.exists() {
        fs::read_to_string(&artifact.target_path)?
    } else {
        String::new()
    };
    let proposed = proposed_applied_content(artifact, root)?;
    Ok(format!(
        "--- {}\n+++ {}\n{}\n",
        artifact.target_path.display(),
        artifact.target_path.display(),
        render_patch_body(&existing, &proposed)
    ))
}

fn render_remove_patch_preview(artifact: &GeneratedArtifact, root: &Path) -> Result<String> {
    let existing = if artifact.target_path.exists() {
        fs::read_to_string(&artifact.target_path)?
    } else {
        String::new()
    };
    let proposed = proposed_removed_content(artifact, root)?;
    Ok(format!(
        "--- {}\n+++ {}\n{}\n",
        artifact.target_path.display(),
        artifact.target_path.display(),
        render_patch_body(&existing, &proposed)
    ))
}

fn proposed_applied_content(artifact: &GeneratedArtifact, _root: &Path) -> Result<String> {
    match artifact.apply_strategy {
        ApplyStrategy::ManagedMarkdownBlock => {
            let existing = if artifact.target_path.exists() {
                fs::read_to_string(&artifact.target_path)?
            } else {
                String::new()
            };
            Ok(upsert_managed_block(
                &existing,
                &artifact.content,
                markdown_block_markers(artifact),
            ))
        }
        ApplyStrategy::JsonMcpConfig => {
            let existing = if artifact.target_path.exists() {
                fs::read_to_string(&artifact.target_path)?
            } else {
                String::new()
            };
            merge_json_mcp_config(&existing, &artifact.content)
        }
        ApplyStrategy::TomlManagedBlock => {
            let existing = if artifact.target_path.exists() {
                fs::read_to_string(&artifact.target_path)?
            } else {
                String::new()
            };
            Ok(upsert_managed_block(
                &existing,
                &artifact.content,
                toml_block_markers(artifact),
            ))
        }
        ApplyStrategy::DirectWrite => Ok(artifact.content.clone()),
        ApplyStrategy::SidecarOnly => Ok(artifact.content.clone()),
    }
}

fn proposed_removed_content(artifact: &GeneratedArtifact, root: &Path) -> Result<String> {
    match artifact.apply_strategy {
        ApplyStrategy::SidecarOnly => {
            let _path = sidecar_path(&artifact.sidecar_scope, root, &artifact.target_path);
            Ok(String::new())
        }
        ApplyStrategy::ManagedMarkdownBlock => {
            let existing = if artifact.target_path.exists() {
                fs::read_to_string(&artifact.target_path)?
            } else {
                String::new()
            };
            Ok(remove_managed_block(
                &existing,
                markdown_block_markers(artifact),
            ))
        }
        ApplyStrategy::JsonMcpConfig => {
            let existing = if artifact.target_path.exists() {
                fs::read_to_string(&artifact.target_path)?
            } else {
                String::new()
            };
            remove_json_mcp_config(&existing, &artifact.content)
        }
        ApplyStrategy::TomlManagedBlock => {
            let existing = if artifact.target_path.exists() {
                fs::read_to_string(&artifact.target_path)?
            } else {
                String::new()
            };
            Ok(remove_managed_block(
                &existing,
                toml_block_markers(artifact),
            ))
        }
        ApplyStrategy::DirectWrite => Ok(String::new()),
    }
}

fn render_patch_body(existing: &str, proposed: &str) -> String {
    let old_lines: Vec<&str> = existing.lines().collect();
    let new_lines: Vec<&str> = proposed.lines().collect();
    let mut body = String::new();
    for line in &old_lines {
        body.push('-');
        body.push_str(line);
        body.push('\n');
    }
    for line in &new_lines {
        body.push('+');
        body.push_str(line);
        body.push('\n');
    }
    body
}

fn apply_artifact(artifact: &GeneratedArtifact, root: &Path) -> Result<(PathBuf, WriteStatus)> {
    match artifact.apply_strategy {
        ApplyStrategy::SidecarOnly => {
            let path = sidecar_path(&artifact.sidecar_scope, root, &artifact.target_path);
            let status = write_with_status(&path, &artifact.content)?;
            Ok((path, status))
        }
        ApplyStrategy::ManagedMarkdownBlock => {
            let existing = if artifact.target_path.exists() {
                fs::read_to_string(&artifact.target_path)?
            } else {
                String::new()
            };
            let updated = upsert_managed_block(
                &existing,
                &artifact.content,
                markdown_block_markers(artifact),
            );
            let status = write_with_status(&artifact.target_path, &updated)?;
            Ok((artifact.target_path.clone(), status))
        }
        ApplyStrategy::JsonMcpConfig => {
            let existing = if artifact.target_path.exists() {
                fs::read_to_string(&artifact.target_path)?
            } else {
                String::new()
            };
            let updated = merge_json_mcp_config(&existing, &artifact.content)?;
            let status = write_with_status(&artifact.target_path, &updated)?;
            Ok((artifact.target_path.clone(), status))
        }
        ApplyStrategy::TomlManagedBlock => {
            let existing = if artifact.target_path.exists() {
                fs::read_to_string(&artifact.target_path)?
            } else {
                String::new()
            };
            let updated =
                upsert_managed_block(&existing, &artifact.content, toml_block_markers(artifact));
            let status = write_with_status(&artifact.target_path, &updated)?;
            Ok((artifact.target_path.clone(), status))
        }
        ApplyStrategy::DirectWrite => {
            let status = write_with_status(&artifact.target_path, &artifact.content)?;
            Ok((artifact.target_path.clone(), status))
        }
    }
}

fn planned_artifact_write(
    artifact: &GeneratedArtifact,
    mode: ArtifactMode,
    root: &Path,
) -> Result<(PathBuf, WriteStatus)> {
    match mode {
        ArtifactMode::Preview | ArtifactMode::Patch => {
            Ok((artifact.target_path.clone(), WriteStatus::Skipped))
        }
        ArtifactMode::WriteSidecar => {
            let path = sidecar_path(&artifact.sidecar_scope, root, &artifact.target_path);
            let status = planned_write_status(&path, &artifact.content)?;
            Ok((path, status))
        }
        ArtifactMode::Apply => match artifact.apply_strategy {
            ApplyStrategy::SidecarOnly => {
                let path = sidecar_path(&artifact.sidecar_scope, root, &artifact.target_path);
                let status = planned_write_status(&path, &artifact.content)?;
                Ok((path, status))
            }
            ApplyStrategy::ManagedMarkdownBlock
            | ApplyStrategy::JsonMcpConfig
            | ApplyStrategy::TomlManagedBlock => {
                let proposed = proposed_applied_content(artifact, root)?;
                let status = planned_write_status(&artifact.target_path, &proposed)?;
                Ok((artifact.target_path.clone(), status))
            }
            ApplyStrategy::DirectWrite => {
                let status = planned_write_status(&artifact.target_path, &artifact.content)?;
                Ok((artifact.target_path.clone(), status))
            }
        },
    }
}

fn remove_artifact(artifact: &GeneratedArtifact, root: &Path) -> Result<PathBuf> {
    match artifact.apply_strategy {
        ApplyStrategy::SidecarOnly => {
            let path = sidecar_path(&artifact.sidecar_scope, root, &artifact.target_path);
            remove_path_if_exists(&path)?;
            Ok(path)
        }
        ApplyStrategy::ManagedMarkdownBlock => {
            if !artifact.target_path.exists() {
                return Ok(artifact.target_path.clone());
            }
            let existing = fs::read_to_string(&artifact.target_path)?;
            let updated = remove_managed_block(&existing, markdown_block_markers(artifact));
            write_or_remove_target(&artifact.target_path, &updated)?;
            Ok(artifact.target_path.clone())
        }
        ApplyStrategy::JsonMcpConfig => {
            if !artifact.target_path.exists() {
                return Ok(artifact.target_path.clone());
            }
            let existing = fs::read_to_string(&artifact.target_path)?;
            let updated = remove_json_mcp_config(&existing, &artifact.content)?;
            write_or_remove_target(&artifact.target_path, &updated)?;
            Ok(artifact.target_path.clone())
        }
        ApplyStrategy::TomlManagedBlock => {
            if !artifact.target_path.exists() {
                return Ok(artifact.target_path.clone());
            }
            let existing = fs::read_to_string(&artifact.target_path)?;
            let updated = remove_managed_block(&existing, toml_block_markers(artifact));
            write_or_remove_target(&artifact.target_path, &updated)?;
            Ok(artifact.target_path.clone())
        }
        ApplyStrategy::DirectWrite => {
            remove_path_if_exists(&artifact.target_path)?;
            Ok(artifact.target_path.clone())
        }
    }
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

fn write_with_status(path: &Path, content: &str) -> Result<WriteStatus> {
    let existed = path.exists();
    if existed {
        let existing = fs::read_to_string(path).unwrap_or_default();
        if existing == content {
            return Ok(WriteStatus::Skipped);
        }
    }
    write_file(path, content)?;
    Ok(if existed {
        WriteStatus::Updated
    } else {
        WriteStatus::Created
    })
}

fn planned_write_status(path: &Path, content: &str) -> Result<WriteStatus> {
    if path.exists() {
        let existing = fs::read_to_string(path)?;
        if existing == content {
            Ok(WriteStatus::Skipped)
        } else {
            Ok(WriteStatus::Updated)
        }
    } else {
        Ok(WriteStatus::Created)
    }
}

fn write_or_remove_target(path: &Path, content: &str) -> Result<()> {
    if content.trim().is_empty() {
        remove_path_if_exists(path)?;
    } else {
        write_file(path, content)?;
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn markdown_block_markers(artifact: &GeneratedArtifact) -> (String, String) {
    (
        format!("<!-- sxmc:begin cli-ai:{} -->", artifact.sidecar_scope),
        format!("<!-- sxmc:end cli-ai:{} -->", artifact.sidecar_scope),
    )
}

fn toml_block_markers(artifact: &GeneratedArtifact) -> (String, String) {
    (
        format!("# sxmc:begin cli-ai:{}", artifact.sidecar_scope),
        format!("# sxmc:end cli-ai:{}", artifact.sidecar_scope),
    )
}

fn upsert_managed_block(existing: &str, new_content: &str, markers: (String, String)) -> String {
    let block = format!("{}\n{}\n{}\n", markers.0, new_content.trim_end(), markers.1);
    if let (Some(start), Some(end)) = (existing.find(&markers.0), existing.find(&markers.1)) {
        let mut updated = String::new();
        updated.push_str(&existing[..start]);
        if !updated.ends_with('\n') && !updated.is_empty() {
            updated.push('\n');
        }
        updated.push_str(&block);
        let after = &existing[end + markers.1.len()..];
        if !after.is_empty() {
            if !updated.ends_with('\n') {
                updated.push('\n');
            }
            updated.push_str(after.trim_start_matches('\n'));
        }
        return updated;
    }

    if existing.trim().is_empty() {
        return block;
    }

    let mut updated = existing.trim_end().to_string();
    updated.push_str("\n\n");
    updated.push_str(&block);
    updated
}

fn remove_managed_block(existing: &str, markers: (String, String)) -> String {
    if let (Some(start), Some(end)) = (existing.find(&markers.0), existing.find(&markers.1)) {
        let before = existing[..start].trim_end_matches('\n');
        let after = existing[end + markers.1.len()..].trim_start_matches('\n');
        let mut updated = String::new();
        if !before.is_empty() {
            updated.push_str(before);
        }
        if !before.is_empty() && !after.is_empty() {
            updated.push_str("\n\n");
        }
        if !after.is_empty() {
            updated.push_str(after);
        }
        return updated;
    }

    existing.to_string()
}

fn merge_json_mcp_config(existing: &str, generated: &str) -> Result<String> {
    let generated_value = serde_json::from_str::<Value>(generated)?;
    let root_key = if generated_value.get("mcpServers").is_some() {
        "mcpServers"
    } else if generated_value.get("mcp").is_some() {
        "mcp"
    } else {
        return Err(SxmcError::Other(
            "Generated config missing mcpServers or mcp object".into(),
        ));
    };

    let mut base = if existing.trim().is_empty() {
        json!({ root_key: {} })
    } else {
        serde_json::from_str::<Value>(existing)?
    };

    let generated_servers = generated_value
        .get(root_key)
        .and_then(Value::as_object)
        .ok_or_else(|| SxmcError::Other(format!("Generated config missing {} object", root_key)))?
        .clone();

    let root_obj = base
        .as_object_mut()
        .ok_or_else(|| SxmcError::Other("Existing config is not a JSON object".into()))?;
    if !root_obj.contains_key(root_key) {
        root_obj.insert(root_key.into(), Value::Object(Map::new()));
    }
    let servers = root_obj
        .get_mut(root_key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            SxmcError::Other(format!(
                "Existing config has a non-object {} value",
                root_key
            ))
        })?;

    for (name, config) in generated_servers {
        servers.insert(name, config);
    }

    serde_json::to_string_pretty(&base).map_err(Into::into)
}

fn remove_json_mcp_config(existing: &str, generated: &str) -> Result<String> {
    if existing.trim().is_empty() {
        return Ok(String::new());
    }

    let generated_value = serde_json::from_str::<Value>(generated)?;
    let root_key = if generated_value.get("mcpServers").is_some() {
        "mcpServers"
    } else if generated_value.get("mcp").is_some() {
        "mcp"
    } else {
        return Err(SxmcError::Other(
            "Generated config missing mcpServers or mcp object".into(),
        ));
    };

    let mut base = serde_json::from_str::<Value>(existing)?;
    let generated_servers = generated_value
        .get(root_key)
        .and_then(Value::as_object)
        .ok_or_else(|| SxmcError::Other(format!("Generated config missing {} object", root_key)))?;

    let root_obj = match base.as_object_mut() {
        Some(root) => root,
        None => return Ok(existing.to_string()),
    };

    let Some(servers) = root_obj.get_mut(root_key).and_then(Value::as_object_mut) else {
        return Ok(existing.to_string());
    };

    for name in generated_servers.keys() {
        servers.remove(name);
    }

    if servers.is_empty() {
        root_obj.remove(root_key);
    }

    if root_obj.is_empty() {
        Ok(String::new())
    } else {
        serde_json::to_string_pretty(&base).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::cli_surfaces::model::{AI_HOST_SPECS, CLI_AI_HOSTS_LAST_VERIFIED};

    #[test]
    fn merge_markdown_block_preserves_existing_content() {
        let existing = "# Repo\n\nExisting text.\n";
        let artifact = GeneratedArtifact {
            label: "Portable agent doc".into(),
            target_path: PathBuf::from("AGENTS.md"),
            content: String::new(),
            apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
            audience: ArtifactAudience::Portable,
            sidecar_scope: "portable".into(),
        };
        let updated = upsert_managed_block(
            existing,
            "## Generated\ncontent",
            markdown_block_markers(&artifact),
        );
        assert!(updated.contains("Existing text."));
        assert!(updated.contains("<!-- sxmc:begin cli-ai:portable -->"));
        assert!(updated.contains("## Generated"));
    }

    #[test]
    fn merge_markdown_blocks_with_different_scopes_coexist() {
        let portable = GeneratedArtifact {
            label: "Portable agent doc".into(),
            target_path: PathBuf::from("AGENTS.md"),
            content: String::new(),
            apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
            audience: ArtifactAudience::Portable,
            sidecar_scope: "portable".into(),
        };
        let codex = GeneratedArtifact {
            label: "OpenAI Codex agent doc".into(),
            target_path: PathBuf::from("AGENTS.md"),
            content: String::new(),
            apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
            audience: ArtifactAudience::Client(AiClientProfile::OpenaiCodex),
            sidecar_scope: "openai-codex".into(),
        };

        let first = upsert_managed_block("", "## Portable", markdown_block_markers(&portable));
        let second = upsert_managed_block(&first, "## Codex", markdown_block_markers(&codex));

        assert!(second.contains("<!-- sxmc:begin cli-ai:portable -->"));
        assert!(second.contains("<!-- sxmc:begin cli-ai:openai-codex -->"));
        assert!(second.contains("## Portable"));
        assert!(second.contains("## Codex"));
    }

    #[test]
    fn merge_json_config_preserves_existing_servers() {
        let existing = r#"{"mcpServers":{"existing":{"command":"foo","args":[]}}}"#;
        let generated = r#"{"mcpServers":{"sxmc-cli-ai-gh":{"command":"sxmc","args":["serve"]}}}"#;
        let merged = merge_json_mcp_config(existing, generated).unwrap();
        assert!(merged.contains("\"existing\""));
        assert!(merged.contains("\"sxmc-cli-ai-gh\""));
    }

    #[test]
    fn merge_json_config_supports_opencode_shape() {
        let existing = r#"{"mcp":{"existing":{"type":"local","command":["foo"]}}}"#;
        let generated = r#"{"mcp":{"sxmc-cli-ai-gh":{"type":"local","command":["sxmc","serve"]}}}"#;
        let merged = merge_json_mcp_config(existing, generated).unwrap();
        assert!(merged.contains("\"existing\""));
        assert!(merged.contains("\"sxmc-cli-ai-gh\""));
    }

    #[test]
    fn generate_client_config_for_all_profiles() {
        let profile: CliSurfaceProfile =
            serde_json::from_str(include_str!("../../examples/profiles/from_cli.json")).unwrap();
        let root = tempdir().unwrap();
        let skills_path = root.path().join(".claude/skills");

        for spec in AI_HOST_SPECS {
            if let Some(artifact) =
                generate_client_config_artifact(&profile, spec.client, root.path(), &skills_path)
            {
                assert!(
                    !artifact.content.is_empty(),
                    "{}",
                    CLI_AI_HOSTS_LAST_VERIFIED
                );
            }
        }
    }

    #[test]
    fn host_specs_have_labels_scopes_and_references() {
        for spec in AI_HOST_SPECS {
            assert!(!spec.label.is_empty());
            assert!(!spec.sidecar_scope.is_empty());
            assert!(spec.official_reference_url.starts_with("https://"));
        }
    }

    #[test]
    fn sidecar_write_keeps_real_doc_untouched() {
        let root = tempdir().unwrap();
        let target = root.path().join("AGENTS.md");
        fs::write(&target, "Existing").unwrap();
        let artifact = GeneratedArtifact {
            label: "Agent doc".into(),
            target_path: target.clone(),
            content: "## Generated".into(),
            apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
            audience: ArtifactAudience::Portable,
            sidecar_scope: "portable".into(),
        };

        let outcomes =
            materialize_artifacts(&[artifact], ArtifactMode::WriteSidecar, root.path()).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "Existing");
        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].path.to_string_lossy().contains(".sxmc"));
    }
}
