use clap::ValueEnum;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Result, SxmcError};

pub const PROFILE_SCHEMA: &str = "sxmc_cli_surface_profile_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliSurfaceProfile {
    pub profile_schema: String,
    pub command: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source: ProfileSource,
    #[serde(default)]
    pub subcommands: Vec<ProfileSubcommand>,
    #[serde(default)]
    pub options: Vec<ProfileOption>,
    #[serde(default)]
    pub positionals: Vec<ProfilePositional>,
    #[serde(default)]
    pub examples: Vec<ProfileExample>,
    #[serde(default)]
    pub auth: Vec<AuthRequirement>,
    #[serde(default)]
    pub environment: Vec<EnvironmentRequirement>,
    pub output_behavior: OutputBehavior,
    #[serde(default)]
    pub workflows: Vec<Workflow>,
    #[serde(default)]
    pub confidence_notes: Vec<ConfidenceNote>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSource {
    pub kind: String,
    pub identifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSubcommand {
    pub name: String,
    pub summary: String,
    pub confidence: ConfidenceLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileOption {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_name: Option<String>,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub confidence: ConfidenceLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilePositional {
    pub name: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub confidence: ConfidenceLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileExample {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub confidence: ConfidenceLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequirement {
    pub kind: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentRequirement {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputBehavior {
    pub stdout_style: String,
    pub stderr_usage: String,
    pub machine_friendly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    pub steps: Vec<String>,
    pub confidence: ConfidenceLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceNote {
    pub level: ConfidenceLevel,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub generated_by: String,
    pub generator_version: String,
    pub source_kind: String,
    pub source_identifier: String,
    pub profile_schema: String,
    pub generation_depth: u32,
    pub generated_at: String,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, ValueEnum)]
pub enum AiClientProfile {
    ClaudeCode,
    Cursor,
    GeminiCli,
    GithubCopilot,
    ContinueDev,
    Junie,
    Windsurf,
    OpenaiCodex,
    GenericStdioMcp,
    GenericHttpMcp,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, ValueEnum)]
pub enum ArtifactMode {
    Preview,
    WriteSidecar,
    Patch,
    Apply,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, ValueEnum)]
pub enum AiCoverage {
    Single,
    Full,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ArtifactAudience {
    Shared,
    Portable,
    Client(AiClientProfile),
}

#[derive(Debug, Clone)]
pub struct GeneratedArtifact {
    pub label: String,
    pub target_path: PathBuf,
    pub content: String,
    pub apply_strategy: ApplyStrategy,
    pub audience: ArtifactAudience,
    pub sidecar_scope: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ApplyStrategy {
    SidecarOnly,
    ManagedMarkdownBlock,
    JsonMcpConfig,
    TomlManagedBlock,
    DirectWrite,
}

#[derive(Debug, Clone)]
pub struct WriteOutcome {
    pub label: String,
    pub path: PathBuf,
    pub mode: ArtifactMode,
}

pub fn parse_command_spec(command: &str) -> Result<Vec<String>> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    if trimmed.starts_with('[') {
        return serde_json::from_str::<Vec<String>>(trimmed).map_err(|e| {
            SxmcError::Other(format!(
                "Invalid command JSON array. Expected [\"cmd\", \"arg1\", ...]: {}",
                e
            ))
        });
    }

    #[cfg(windows)]
    {
        if let Some(parts) = parse_windows_command_spec(trimmed) {
            return Ok(parts);
        }
        return Ok(trimmed.split_whitespace().map(str::to_string).collect());
    }

    #[cfg(not(windows))]
    shlex::split(trimmed).ok_or_else(|| {
        SxmcError::Other(
            "Invalid command string. Use shell-style quoting or a JSON array command spec.".into(),
        )
    })
}

#[cfg(windows)]
fn parse_windows_command_spec(command: &str) -> Option<Vec<String>> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Some(Vec::new());
    }

    if let Some(rest) = trimmed.strip_prefix('"') {
        let quote_end = rest.find('"')?;
        let executable = &rest[..quote_end];
        let args = rest[quote_end + 1..].trim();
        let mut parts = vec![executable.to_string()];
        parts.extend(args.split_whitespace().map(str::to_string));
        return Some(parts);
    }

    let executable_pattern = Regex::new(r"(?i)^(.+?\.(exe|cmd|bat|ps1))(?:\s+(.*))?$").ok()?;
    let captures = executable_pattern.captures(trimmed)?;
    let executable = captures.get(1)?.as_str();
    let mut parts = vec![executable.to_string()];
    if let Some(args) = captures.get(3) {
        parts.extend(args.as_str().split_whitespace().map(str::to_string));
    }
    Some(parts)
}

pub fn inspect_cli(command_spec: &str, allow_self: bool) -> Result<CliSurfaceProfile> {
    let parts = parse_command_spec(command_spec)?;
    if parts.is_empty() {
        return Err(SxmcError::Other("Empty command spec".into()));
    }

    let executable = &parts[0];
    let command_name = Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(executable)
        .to_string();

    if !allow_self && is_self_command(&command_name) {
        return Err(SxmcError::Other(
            "Refusing to inspect sxmc itself without --allow-self".into(),
        ));
    }

    let help_text = read_help_text(&parts)?;
    let parse = parse_help_text(&command_name, executable, &help_text);
    Ok(parse)
}

pub fn load_profile(path: &Path) -> Result<CliSurfaceProfile> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn profile_value(profile: &CliSurfaceProfile) -> Value {
    serde_json::to_value(profile).unwrap_or_else(|_| json!({}))
}

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
    let target_path = root.join(agent_doc_target(client));
    let content = render_agent_doc(profile, client);
    GeneratedArtifact {
        label: format!("{} agent doc", client_label(client)),
        target_path,
        content,
        apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
        audience: ArtifactAudience::Client(client),
        sidecar_scope: slugify(client_label(client)),
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
    vec![
        GeneratedArtifact {
            label: "Claude Code agent doc".into(),
            target_path: root.join("CLAUDE.md"),
            content: render_agent_doc(profile, AiClientProfile::ClaudeCode),
            apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
            audience: ArtifactAudience::Client(AiClientProfile::ClaudeCode),
            sidecar_scope: slugify(client_label(AiClientProfile::ClaudeCode)),
        },
        GeneratedArtifact {
            label: "Cursor rules doc".into(),
            target_path: root.join(".cursor/rules/sxmc-cli-ai.md"),
            content: render_agent_doc(profile, AiClientProfile::Cursor),
            apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
            audience: ArtifactAudience::Client(AiClientProfile::Cursor),
            sidecar_scope: slugify(client_label(AiClientProfile::Cursor)),
        },
        GeneratedArtifact {
            label: "Gemini CLI agent doc".into(),
            target_path: root.join("GEMINI.md"),
            content: render_agent_doc(profile, AiClientProfile::GeminiCli),
            apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
            audience: ArtifactAudience::Client(AiClientProfile::GeminiCli),
            sidecar_scope: slugify(client_label(AiClientProfile::GeminiCli)),
        },
        GeneratedArtifact {
            label: "GitHub Copilot instructions".into(),
            target_path: root.join(".github/copilot-instructions.md"),
            content: render_agent_doc(profile, AiClientProfile::GithubCopilot),
            apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
            audience: ArtifactAudience::Client(AiClientProfile::GithubCopilot),
            sidecar_scope: slugify(client_label(AiClientProfile::GithubCopilot)),
        },
        GeneratedArtifact {
            label: "Continue rules doc".into(),
            target_path: root.join(".continue/rules/sxmc-cli-ai.md"),
            content: render_agent_doc(profile, AiClientProfile::ContinueDev),
            apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
            audience: ArtifactAudience::Client(AiClientProfile::ContinueDev),
            sidecar_scope: slugify(client_label(AiClientProfile::ContinueDev)),
        },
        GeneratedArtifact {
            label: "Junie guidelines".into(),
            target_path: root.join(".junie/guidelines.md"),
            content: render_agent_doc(profile, AiClientProfile::Junie),
            apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
            audience: ArtifactAudience::Client(AiClientProfile::Junie),
            sidecar_scope: slugify(client_label(AiClientProfile::Junie)),
        },
        GeneratedArtifact {
            label: "Windsurf rules doc".into(),
            target_path: root.join(".windsurf/rules/sxmc-cli-ai.md"),
            content: render_agent_doc(profile, AiClientProfile::Windsurf),
            apply_strategy: ApplyStrategy::ManagedMarkdownBlock,
            audience: ArtifactAudience::Client(AiClientProfile::Windsurf),
            sidecar_scope: slugify(client_label(AiClientProfile::Windsurf)),
        },
    ]
}

pub fn generate_full_coverage_init_artifacts(
    profile: &CliSurfaceProfile,
    root: &Path,
    skills_path: &Path,
) -> Result<Vec<GeneratedArtifact>> {
    let mut artifacts = vec![generate_profile_artifact(profile, root)?];
    artifacts.push(generate_portable_agent_doc_artifact(profile, root));
    artifacts.extend(generate_host_native_agent_doc_artifacts(profile, root));

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
        if let Some(artifact) = generate_client_config_artifact(profile, client, root, skills_path)
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
    let target_path = root.join(client_config_target(client)?);
    let absolute_skills_path = if skills_path.is_absolute() {
        skills_path.to_path_buf()
    } else {
        root.join(skills_path)
    };
    let server_name = format!("sxmc-cli-ai-{}", slugify(&profile.command));
    let content = render_client_config(client, &server_name, &absolute_skills_path);
    let apply_strategy = match client {
        AiClientProfile::Cursor | AiClientProfile::GeminiCli => ApplyStrategy::JsonMcpConfig,
        AiClientProfile::OpenaiCodex => ApplyStrategy::TomlManagedBlock,
        _ => ApplyStrategy::SidecarOnly,
    };

    Some(GeneratedArtifact {
        label: format!("{} client config", client_label(client)),
        target_path,
        content,
        apply_strategy,
        audience: ArtifactAudience::Client(client),
        sidecar_scope: slugify(client_label(client)),
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
                });
            }
            ArtifactMode::WriteSidecar => {
                let path = sidecar_path(&artifact.sidecar_scope, root, &artifact.target_path);
                write_file(&path, &artifact.content)?;
                outcomes.push(WriteOutcome {
                    label: artifact.label.clone(),
                    path,
                    mode,
                });
            }
            ArtifactMode::Patch => {
                println!("{}", render_patch_preview(artifact, root)?);
                outcomes.push(WriteOutcome {
                    label: artifact.label.clone(),
                    path: artifact.target_path.clone(),
                    mode,
                });
            }
            ArtifactMode::Apply => {
                let path = apply_artifact(artifact, root)?;
                outcomes.push(WriteOutcome {
                    label: artifact.label.clone(),
                    path,
                    mode,
                });
            }
        }
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

fn is_self_command(command_name: &str) -> bool {
    let lowered = command_name.to_ascii_lowercase();
    lowered == "sxmc" || lowered == "sxmc.exe"
}

fn read_help_text(parts: &[String]) -> Result<String> {
    let mut command = Command::new(&parts[0]);
    if parts.len() > 1 {
        command.args(&parts[1..]);
    }
    command.arg("--help");
    let output = command
        .output()
        .map_err(|e| SxmcError::Other(format!("Failed to run '{} --help': {}", parts[0], e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let text = if !stdout.trim().is_empty() {
        stdout
    } else {
        stderr
    };

    if !output.status.success() && text.trim().is_empty() {
        return Err(SxmcError::Other(format!(
            "Command '{}' did not return readable help output",
            parts[0]
        )));
    }

    Ok(text)
}

fn parse_help_text(command_name: &str, source_identifier: &str, help: &str) -> CliSurfaceProfile {
    let lines: Vec<&str> = help.lines().collect();
    let summary = lines
        .iter()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .unwrap_or(command_name)
        .to_string();

    let description = parse_description(&lines);
    let subcommands = parse_subcommands(&lines);
    let options = parse_options(&lines);
    let positionals = parse_positionals(&lines, command_name);
    let examples = parse_examples(&lines, command_name);
    let (auth, environment) = infer_requirements(help);
    let workflows = infer_workflows(&subcommands);
    let output_behavior = infer_output_behavior(help);
    let mut confidence_notes = vec![ConfidenceNote {
        level: ConfidenceLevel::Medium,
        summary: "This profile was inferred from help output and may omit dynamic or plugin-provided behavior.".into(),
    }];
    if examples.is_empty() {
        confidence_notes.push(ConfidenceNote {
            level: ConfidenceLevel::Low,
            summary: "No examples were detected in help output; generated agent guidance may need manual examples.".into(),
        });
    }

    CliSurfaceProfile {
        profile_schema: PROFILE_SCHEMA.into(),
        command: command_name.into(),
        summary,
        description,
        source: ProfileSource {
            kind: "cli".into(),
            identifier: source_identifier.into(),
        },
        subcommands,
        options,
        positionals,
        examples,
        auth,
        environment,
        output_behavior,
        workflows,
        confidence_notes,
        provenance: Provenance {
            generated_by: "sxmc".into(),
            generator_version: env!("CARGO_PKG_VERSION").into(),
            source_kind: "cli".into(),
            source_identifier: source_identifier.into(),
            profile_schema: PROFILE_SCHEMA.into(),
            generation_depth: 0,
            generated_at: now_string(),
        },
    }
}

fn parse_description(lines: &[&str]) -> Option<String> {
    let mut description = Vec::new();
    let mut started = false;
    for line in lines.iter().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if started {
                break;
            }
            continue;
        }
        if is_section_heading(trimmed) || trimmed.starts_with("Usage:") {
            break;
        }
        started = true;
        description.push(trimmed.to_string());
    }
    if description.is_empty() {
        None
    } else {
        Some(description.join(" "))
    }
}

fn parse_subcommands(lines: &[&str]) -> Vec<ProfileSubcommand> {
    parse_table_section(lines, &["commands", "subcommands", "available commands"])
        .into_iter()
        .map(|(name, summary)| ProfileSubcommand {
            name,
            summary,
            confidence: ConfidenceLevel::High,
        })
        .collect()
}

fn parse_options(lines: &[&str]) -> Vec<ProfileOption> {
    let mut options = Vec::new();
    let mut in_options = false;
    let regex = Regex::new(
        r"^\s*(?:(-[A-Za-z0-9])(?:,\s*)?)?(--[A-Za-z0-9][A-Za-z0-9-]*)(?:[ =]([A-Z<>\[\]\-_|]+))?\s{2,}(.*)$",
    )
    .unwrap();
    let short_only_regex =
        Regex::new(r"^\s*(-[A-Za-z0-9])(?:[ =]([A-Z<>\[\]\-_|]+))?\s{2,}(.*)$").unwrap();

    for line in lines {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            if in_options {
                break;
            }
            continue;
        }
        if is_named_section(trimmed, &["options", "flags"]) {
            in_options = true;
            continue;
        }
        if !in_options {
            continue;
        }
        if is_section_heading(trimmed.trim()) {
            break;
        }
        if let Some(caps) = regex.captures(trimmed) {
            options.push(ProfileOption {
                name: caps
                    .get(2)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default(),
                short: caps.get(1).map(|m| m.as_str().to_string()),
                value_name: caps
                    .get(3)
                    .map(|m| m.as_str().trim_matches(&['<', '>'][..]).to_string()),
                required: false,
                summary: caps.get(4).map(|m| m.as_str().trim().to_string()),
                confidence: ConfidenceLevel::High,
            });
        } else if let Some(caps) = short_only_regex.captures(trimmed) {
            options.push(ProfileOption {
                name: caps
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default(),
                short: caps.get(1).map(|m| m.as_str().to_string()),
                value_name: caps
                    .get(2)
                    .map(|m| m.as_str().trim_matches(&['<', '>'][..]).to_string()),
                required: false,
                summary: caps.get(3).map(|m| m.as_str().trim().to_string()),
                confidence: ConfidenceLevel::Medium,
            });
        }
    }
    options
}

fn parse_positionals(lines: &[&str], command_name: &str) -> Vec<ProfilePositional> {
    let usage_line = lines
        .iter()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("Usage:") {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    if usage_line.is_empty() {
        return Vec::new();
    }

    usage_line
        .split_whitespace()
        .skip_while(|token| *token != command_name && !token.ends_with(command_name))
        .skip(1)
        .filter_map(|token| {
            if token.starts_with('-') || token.starts_with('[') || token == "[COMMAND]" {
                return None;
            }
            if !(token.starts_with('<') && token.ends_with('>')
                || token
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_' || c == '-'))
            {
                return None;
            }
            let name = token.trim_matches(&['<', '>'][..]).trim_matches('.');
            if name.is_empty() {
                return None;
            }
            Some(ProfilePositional {
                name: name.to_ascii_lowercase(),
                required: true,
                summary: None,
                confidence: ConfidenceLevel::Medium,
            })
        })
        .collect()
}

fn parse_examples(lines: &[&str], command_name: &str) -> Vec<ProfileExample> {
    let mut examples = Vec::new();
    let mut in_examples = false;
    for line in lines {
        let trimmed = line.trim_end();
        let stripped = trimmed.trim();
        if stripped.is_empty() {
            if in_examples && !examples.is_empty() {
                break;
            }
            continue;
        }
        if is_named_section(stripped, &["examples", "example"]) {
            in_examples = true;
            continue;
        }
        if !in_examples {
            continue;
        }
        if is_section_heading(stripped) {
            break;
        }
        if stripped.starts_with(command_name) || stripped.starts_with('$') {
            examples.push(ProfileExample {
                command: stripped.trim_start_matches("$ ").to_string(),
                summary: None,
                confidence: ConfidenceLevel::High,
            });
        }
    }
    examples
}

fn infer_requirements(help: &str) -> (Vec<AuthRequirement>, Vec<EnvironmentRequirement>) {
    let mut auth = Vec::new();
    let mut environment = Vec::new();
    let mut seen_env = std::collections::BTreeSet::new();

    if help.to_ascii_lowercase().contains("login")
        || help.to_ascii_lowercase().contains("authenticate")
        || help.to_ascii_lowercase().contains("auth")
    {
        auth.push(AuthRequirement {
            kind: "interactive".into(),
            summary:
                "Help output mentions login/authentication, so interactive setup may be required."
                    .into(),
        });
    }

    let env_regex = Regex::new(r"\b([A-Z][A-Z0-9_]{2,})\b").unwrap();
    for capture in env_regex.captures_iter(help) {
        let name = capture.get(1).map(|m| m.as_str()).unwrap_or_default();
        if (name.ends_with("_TOKEN")
            || name.ends_with("_KEY")
            || name.ends_with("_SECRET")
            || name == "TOKEN")
            && seen_env.insert(name.to_string())
        {
            environment.push(EnvironmentRequirement {
                name: name.into(),
                summary: Some(
                    "Detected in help output; likely needed for auth or configuration.".into(),
                ),
                required: true,
            });
            auth.push(AuthRequirement {
                kind: "env_var".into(),
                summary: format!("Help output mentions environment variable `{}`.", name),
            });
        }
    }

    (auth, environment)
}

fn infer_workflows(subcommands: &[ProfileSubcommand]) -> Vec<Workflow> {
    if subcommands.is_empty() {
        return Vec::new();
    }
    let steps = subcommands
        .iter()
        .take(3)
        .map(|subcommand| format!("Use `{}` for {}", subcommand.name, subcommand.summary))
        .collect();
    vec![Workflow {
        name: "Common command flow".into(),
        steps,
        confidence: ConfidenceLevel::Medium,
    }]
}

fn infer_output_behavior(help: &str) -> OutputBehavior {
    let lowered = help.to_ascii_lowercase();
    OutputBehavior {
        stdout_style: if lowered.contains("--json") || lowered.contains(" json ") {
            "mixed".into()
        } else {
            "plain_text".into()
        },
        stderr_usage: "Unknown; inspect live behavior before piping stderr into machine parsers."
            .into(),
        machine_friendly: lowered.contains("--json") || lowered.contains("json output"),
    }
}

fn parse_table_section(lines: &[&str], headings: &[&str]) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut in_section = false;

    for line in lines {
        let trimmed = line.trim_end();
        let stripped = trimmed.trim();

        if stripped.is_empty() {
            if in_section && !rows.is_empty() {
                break;
            }
            continue;
        }

        if is_named_section(stripped, headings) {
            in_section = true;
            continue;
        }

        if !in_section {
            continue;
        }

        if is_section_heading(stripped) {
            break;
        }

        let columns: Vec<&str> = stripped
            .split("  ")
            .filter(|chunk| !chunk.trim().is_empty())
            .collect();

        if columns.len() >= 2 {
            rows.push((
                columns[0].trim().to_string(),
                columns[1..].join(" ").trim().to_string(),
            ));
        }
    }

    rows
}

fn is_named_section(line: &str, headings: &[&str]) -> bool {
    let normalized = line.trim_end_matches(':').to_ascii_lowercase();
    headings.iter().any(|heading| normalized == *heading)
}

fn is_section_heading(line: &str) -> bool {
    line.ends_with(':')
}

fn now_string() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{}", seconds)
}

fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in input.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            previous_dash = false;
            ch.to_ascii_lowercase()
        } else {
            if previous_dash {
                continue;
            }
            previous_dash = true;
            '-'
        };
        slug.push(mapped);
    }
    slug.trim_matches('-').to_string()
}

fn client_label(client: AiClientProfile) -> &'static str {
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

fn agent_doc_target(client: AiClientProfile) -> &'static str {
    match client {
        AiClientProfile::ClaudeCode => "CLAUDE.md",
        AiClientProfile::GeminiCli => "GEMINI.md",
        AiClientProfile::GithubCopilot => ".github/copilot-instructions.md",
        AiClientProfile::ContinueDev => ".continue/rules/sxmc-cli-ai.md",
        AiClientProfile::Junie => ".junie/guidelines.md",
        AiClientProfile::Windsurf => ".windsurf/rules/sxmc-cli-ai.md",
        _ => "AGENTS.md",
    }
}

fn client_config_target(client: AiClientProfile) -> Option<&'static str> {
    match client {
        AiClientProfile::ClaudeCode => Some(".sxmc/ai/claude-code-mcp.json"),
        AiClientProfile::Cursor => Some(".cursor/mcp.json"),
        AiClientProfile::GeminiCli => Some(".gemini/settings.json"),
        AiClientProfile::GithubCopilot => None,
        AiClientProfile::ContinueDev => None,
        AiClientProfile::Junie => None,
        AiClientProfile::Windsurf => None,
        AiClientProfile::OpenaiCodex => Some(".codex/mcp.toml"),
        AiClientProfile::GenericStdioMcp => Some(".sxmc/ai/generic-stdio-mcp.json"),
        AiClientProfile::GenericHttpMcp => Some(".sxmc/ai/generic-http-mcp.json"),
    }
}

fn render_agent_doc(profile: &CliSurfaceProfile, client: AiClientProfile) -> String {
    let mut lines = vec![
        format!("## sxmc CLI Surface: `{}`", profile.command),
        String::new(),
        format!(
            "Use `{}` as a first-class terminal workflow in this repo for {}.",
            profile.command,
            client_label(client)
        ),
        String::new(),
        format!("Summary: {}", profile.summary),
    ];

    if let Some(description) = &profile.description {
        lines.push(String::new());
        lines.push(description.clone());
    }

    if !profile.examples.is_empty() {
        lines.push(String::new());
        lines.push("Preferred flow:".into());
        for (index, example) in profile.examples.iter().take(3).enumerate() {
            lines.push(format!("{}. `{}`", index + 1, example.command));
        }
    } else {
        lines.push(String::new());
        lines.push("Preferred flow:".into());
        lines.push(format!("1. `{} --help`", profile.command));
        if let Some(subcommand) = profile.subcommands.first() {
            lines.push(format!(
                "2. `{} {} --help`",
                profile.command, subcommand.name
            ));
        }
    }

    if !profile.subcommands.is_empty() {
        lines.push(String::new());
        lines.push("High-confidence subcommands:".into());
        for subcommand in profile.subcommands.iter().take(5) {
            lines.push(format!("- `{}`: {}", subcommand.name, subcommand.summary));
        }
    }

    if !profile.environment.is_empty() {
        lines.push(String::new());
        lines.push("Environment/auth notes:".into());
        for env in &profile.environment {
            lines.push(format!(
                "- `{}`{}",
                env.name,
                env.summary
                    .as_ref()
                    .map(|summary| format!(": {}", summary))
                    .unwrap_or_default()
            ));
        }
    }

    lines.push(String::new());
    lines.push("Guidance:".into());
    lines.push("- Keep bulky output in files or pipes when possible.".into());
    lines.push("- Prefer machine-friendly flags like `--json` when the CLI supports them.".into());
    lines.push("- Re-check `--help` before using low-confidence flows.".into());

    lines.join("\n")
}

fn render_portable_agent_doc(profile: &CliSurfaceProfile) -> String {
    let mut lines = vec![
        format!("## sxmc CLI Surface: `{}`", profile.command),
        String::new(),
        format!(
            "Use `{}` as a portable terminal workflow across AI tools in this repo.",
            profile.command
        ),
        String::new(),
        format!("Summary: {}", profile.summary),
    ];

    if let Some(description) = &profile.description {
        lines.push(String::new());
        lines.push(description.clone());
    }

    lines.push(String::new());
    lines.push("Recommended startup guidance:".into());
    lines.push(format!(
        "- Start with `{}` `--help` when the exact command shape is unclear.",
        profile.command
    ));
    lines.push("- Prefer machine-friendly flags like `--json` when available.".into());
    lines.push(
        "- Keep bulky output in files or pipes instead of pasting it into chat context.".into(),
    );
    lines.push("- Re-check auth or environment requirements before write actions.".into());

    if !profile.examples.is_empty() {
        lines.push(String::new());
        lines.push("Preferred commands:".into());
        for example in profile.examples.iter().take(4) {
            lines.push(format!("- `{}`", example.command));
        }
    }

    if !profile.subcommands.is_empty() {
        lines.push(String::new());
        lines.push("High-confidence subcommands:".into());
        for subcommand in profile.subcommands.iter().take(5) {
            lines.push(format!("- `{}`: {}", subcommand.name, subcommand.summary));
        }
    }

    lines.join("\n")
}

fn render_llms_txt(profile: &CliSurfaceProfile) -> String {
    let mut lines = vec![
        format!("# {}", profile.command),
        String::new(),
        profile.summary.clone(),
    ];

    if let Some(description) = &profile.description {
        lines.push(String::new());
        lines.push(description.clone());
    }

    if !profile.examples.is_empty() {
        lines.push(String::new());
        lines.push("## Recommended Commands".into());
        for example in profile.examples.iter().take(5) {
            lines.push(format!("- `{}`", example.command));
        }
    }

    if !profile.subcommands.is_empty() {
        lines.push(String::new());
        lines.push("## High-Confidence Subcommands".into());
        for subcommand in profile.subcommands.iter().take(6) {
            lines.push(format!("- `{}`: {}", subcommand.name, subcommand.summary));
        }
    }

    if !profile.environment.is_empty() {
        lines.push(String::new());
        lines.push("## Environment".into());
        for env in &profile.environment {
            lines.push(format!("- `{}`", env.name));
        }
    }

    lines.push(String::new());
    lines.push("## Notes".into());
    lines.push("- Generated by `sxmc scaffold llms-txt` from a CLI surface profile.".into());
    lines.push("- Review before publishing as project-facing LLM guidance.".into());

    lines.join("\n")
}

fn render_client_config(client: AiClientProfile, server_name: &str, skills_path: &Path) -> String {
    let skills_display = skills_path.display().to_string();
    match client {
        AiClientProfile::OpenaiCodex => format!(
            "# sxmc CLI->AI startup scaffold\n[mcp_servers.{server_name}]\ncommand = \"sxmc\"\nargs = [\"serve\", \"--paths\", \"{skills_display}\"]\n"
        ),
        AiClientProfile::GenericHttpMcp => serde_json::to_string_pretty(&json!({
            "mcpServers": {
                server_name: {
                    "url": "http://127.0.0.1:8000/mcp"
                }
            }
        }))
        .unwrap(),
        AiClientProfile::Cursor => serde_json::to_string_pretty(&json!({
            "mcpServers": {
                server_name: {
                    "type": "stdio",
                    "command": "sxmc",
                    "args": ["serve", "--paths", skills_display]
                }
            }
        }))
        .unwrap(),
        AiClientProfile::GeminiCli => serde_json::to_string_pretty(&json!({
            "mcpServers": {
                server_name: {
                    "command": "sxmc",
                    "args": ["serve", "--paths", skills_display]
                }
            }
        }))
        .unwrap(),
        _ => serde_json::to_string_pretty(&json!({
            "mcpServers": {
                server_name: {
                    "command": "sxmc",
                    "args": ["serve", "--paths", skills_display]
                }
            }
        }))
        .unwrap(),
    }
}

fn render_skill_markdown(profile: &CliSurfaceProfile) -> String {
    let name = format!("{}-cli", slugify(&profile.command));
    let description = profile
        .description
        .as_deref()
        .unwrap_or(&profile.summary)
        .replace('"', "'");
    let argument_hint = profile
        .positionals
        .iter()
        .map(|positional| format!("<{}>", positional.name))
        .chain(
            profile
                .options
                .iter()
                .take(3)
                .map(|option| option.name.clone()),
        )
        .collect::<Vec<_>>()
        .join(" ");

    let mut body = vec![
        "---".to_string(),
        format!("name: {}", name),
        format!("description: \"{}\"", description),
    ];
    if !argument_hint.trim().is_empty() {
        body.push(format!("argument-hint: \"{}\"", argument_hint));
    }
    body.push("---".to_string());
    body.push(String::new());
    body.push(format!("# {} CLI workflow", profile.command));
    body.push(String::new());
    body.push(profile.summary.clone());

    if let Some(description) = &profile.description {
        body.push(String::new());
        body.push(description.clone());
    }

    if !profile.examples.is_empty() {
        body.push(String::new());
        body.push("Recommended commands:".into());
        for example in profile.examples.iter().take(5) {
            body.push(format!("- `{}`", example.command));
        }
    }

    if !profile.subcommands.is_empty() {
        body.push(String::new());
        body.push("High-confidence subcommands:".into());
        for subcommand in profile.subcommands.iter().take(5) {
            body.push(format!("- `{}`: {}", subcommand.name, subcommand.summary));
        }
    }

    body.push(String::new());
    body.push("Execution guidance:".into());
    body.push(format!(
        "- Start with `{}` `--help` if the exact shape is unclear.",
        profile.command
    ));
    body.push("- Prefer machine-friendly flags like `--json` when available.".into());
    body.push("- Keep large output in files or pipes instead of pasting it into context.".into());
    body.push(
        "- Re-check auth or environment requirements before performing write actions.".into(),
    );
    body.push(String::new());
    body.push(
        "This file was generated by `sxmc scaffold skill` from a CLI profile and should be reviewed before wider use."
            .into(),
    );
    body.join("\n")
}

fn render_mcp_wrapper_readme(profile: &CliSurfaceProfile) -> String {
    let slug = slugify(&profile.command);
    let mut lines = vec![
        format!("# {} MCP wrapper scaffold", profile.command),
        String::new(),
        "This scaffold is a starting point for wrapping a CLI as a focused MCP server.".into(),
        String::new(),
        "Recommended approach:".into(),
        format!(
            "- Start from the `{}` CLI profile rather than mirroring the whole CLI.",
            slug
        ),
        "- Expose a few narrow tools first.".into(),
        "- Keep outputs machine-friendly and bounded.".into(),
        "- Treat prompts/resources as optional depending on the CLI.".into(),
    ];

    if !profile.subcommands.is_empty() {
        lines.push(String::new());
        lines.push("Candidate tool surfaces:".into());
        for subcommand in profile.subcommands.iter().take(5) {
            lines.push(format!("- `{}`: {}", subcommand.name, subcommand.summary));
        }
    }

    if !profile.examples.is_empty() {
        lines.push(String::new());
        lines.push("Examples to preserve in wrapper behavior:".into());
        for example in profile.examples.iter().take(5) {
            lines.push(format!("- `{}`", example.command));
        }
    }

    lines.push(String::new());
    lines.push("Files:".into());
    lines.push(
        "- `manifest.json` captures the inspected profile details and suggested wrapper shape."
            .into(),
    );
    lines.push(
        "- Add server code, tests, and launch scripts beside this scaffold as needed.".into(),
    );
    lines.join("\n")
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

fn proposed_applied_content(artifact: &GeneratedArtifact, root: &Path) -> Result<String> {
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
                markdown_block_markers(&artifact.target_path),
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
                toml_block_markers(&artifact.target_path),
            ))
        }
        ApplyStrategy::DirectWrite => Ok(artifact.content.clone()),
        ApplyStrategy::SidecarOnly => {
            let target = sidecar_path(&artifact.sidecar_scope, root, &artifact.target_path);
            let _ = target;
            Ok(artifact.content.clone())
        }
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

fn apply_artifact(artifact: &GeneratedArtifact, root: &Path) -> Result<PathBuf> {
    match artifact.apply_strategy {
        ApplyStrategy::SidecarOnly => {
            let path = sidecar_path(&artifact.sidecar_scope, root, &artifact.target_path);
            write_file(&path, &artifact.content)?;
            Ok(path)
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
                markdown_block_markers(&artifact.target_path),
            );
            write_file(&artifact.target_path, &updated)?;
            Ok(artifact.target_path.clone())
        }
        ApplyStrategy::JsonMcpConfig => {
            let existing = if artifact.target_path.exists() {
                fs::read_to_string(&artifact.target_path)?
            } else {
                String::new()
            };
            let updated = merge_json_mcp_config(&existing, &artifact.content)?;
            write_file(&artifact.target_path, &updated)?;
            Ok(artifact.target_path.clone())
        }
        ApplyStrategy::TomlManagedBlock => {
            let existing = if artifact.target_path.exists() {
                fs::read_to_string(&artifact.target_path)?
            } else {
                String::new()
            };
            let updated = upsert_managed_block(
                &existing,
                &artifact.content,
                toml_block_markers(&artifact.target_path),
            );
            write_file(&artifact.target_path, &updated)?;
            Ok(artifact.target_path.clone())
        }
        ApplyStrategy::DirectWrite => {
            write_file(&artifact.target_path, &artifact.content)?;
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

fn markdown_block_markers(target_path: &Path) -> (&'static str, &'static str) {
    let _ = target_path;
    ("<!-- sxmc:begin cli-ai -->", "<!-- sxmc:end cli-ai -->")
}

fn toml_block_markers(target_path: &Path) -> (&'static str, &'static str) {
    let _ = target_path;
    ("# sxmc:begin cli-ai", "# sxmc:end cli-ai")
}

fn upsert_managed_block(existing: &str, new_content: &str, markers: (&str, &str)) -> String {
    let block = format!("{}\n{}\n{}\n", markers.0, new_content.trim_end(), markers.1);
    if let (Some(start), Some(end)) = (existing.find(markers.0), existing.find(markers.1)) {
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

fn merge_json_mcp_config(existing: &str, generated: &str) -> Result<String> {
    let mut base = if existing.trim().is_empty() {
        json!({ "mcpServers": {} })
    } else {
        serde_json::from_str::<Value>(existing)?
    };

    let generated_value = serde_json::from_str::<Value>(generated)?;
    let generated_servers = generated_value
        .get("mcpServers")
        .and_then(Value::as_object)
        .ok_or_else(|| SxmcError::Other("Generated config missing mcpServers object".into()))?
        .clone();

    let root_obj = base
        .as_object_mut()
        .ok_or_else(|| SxmcError::Other("Existing config is not a JSON object".into()))?;
    if !root_obj.contains_key("mcpServers") {
        root_obj.insert("mcpServers".into(), Value::Object(Map::new()));
    }
    let servers = root_obj
        .get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            SxmcError::Other("Existing config has a non-object mcpServers value".into())
        })?;

    for (name, config) in generated_servers {
        servers.insert(name, config);
    }

    serde_json::to_string_pretty(&base).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parse_json_array_command_spec() {
        let parsed = parse_command_spec(r#"["sxmc","serve","--paths","tests/fixtures"]"#).unwrap();
        assert_eq!(parsed, vec!["sxmc", "serve", "--paths", "tests/fixtures"]);
    }

    #[test]
    fn merge_markdown_block_preserves_existing_content() {
        let existing = "# Repo\n\nExisting text.\n";
        let updated = upsert_managed_block(
            existing,
            "## Generated\ncontent",
            markdown_block_markers(Path::new("AGENTS.md")),
        );
        assert!(updated.contains("Existing text."));
        assert!(updated.contains("<!-- sxmc:begin cli-ai -->"));
        assert!(updated.contains("## Generated"));
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
    fn generate_client_config_for_all_profiles() {
        let profile: CliSurfaceProfile =
            serde_json::from_str(include_str!("../examples/profiles/from_cli.json")).unwrap();
        let root = tempdir().unwrap();
        let skills_path = root.path().join(".claude/skills");

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
            if let Some(artifact) =
                generate_client_config_artifact(&profile, client, root.path(), &skills_path)
            {
                assert!(!artifact.content.is_empty());
            }
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
