use std::path::PathBuf;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

pub const PROFILE_SCHEMA: &str = "sxmc_cli_surface_profile_v1";
pub const CLI_AI_HOSTS_LAST_VERIFIED: &str = "2026-03-21";

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subcommand_profiles: Vec<CliSurfaceProfile>,
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

#[derive(Debug, Clone)]
pub struct ProfileQualityReport {
    pub ready_for_agent_docs: bool,
    pub reasons: Vec<String>,
}

impl CliSurfaceProfile {
    pub fn quality_report(&self) -> ProfileQualityReport {
        let mut reasons = Vec::new();
        let generic_summary = self.summary.trim()
            == format!("{} command-line interface", self.command)
            || self.summary.trim().eq_ignore_ascii_case(&self.command)
            || self.summary.trim().len() < 24;
        if generic_summary {
            reasons.push(
                "Summary stayed generic, so generated startup docs may be less useful than a hand-written snippet."
                    .into(),
            );
        }
        if self.examples.is_empty() {
            reasons.push(
                "No usage examples were detected, so generated guidance may not show the best first command."
                    .into(),
            );
        }
        if self.subcommands.is_empty() && self.options.len() < 2 {
            reasons.push(
                "The inspected CLI surface is sparse; sxmc could not confidently extract subcommands or enough options."
                    .into(),
            );
        }

        let rich_surface = !self.examples.is_empty()
            || !self.subcommand_profiles.is_empty()
            || self.subcommands.len() >= 3
            || self.options.len() >= 5;

        ProfileQualityReport {
            ready_for_agent_docs: rich_surface
                && !(generic_summary && self.subcommands.is_empty() && self.options.len() < 3),
            reasons,
        }
    }
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
    OpenCode,
    JetbrainsAiAssistant,
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConfigShape {
    JsonMcpServers,
    JsonMcp,
    TomlMcpServers,
}

#[derive(Debug, Clone, Copy)]
pub struct HostProfileSpec {
    pub client: AiClientProfile,
    pub label: &'static str,
    pub sidecar_scope: &'static str,
    pub native_doc_target: Option<&'static str>,
    pub native_config_target: Option<&'static str>,
    pub config_shape: Option<ConfigShape>,
    pub official_reference_url: &'static str,
}

pub const AI_HOST_SPECS: &[HostProfileSpec] = &[
    HostProfileSpec {
        client: AiClientProfile::ClaudeCode,
        label: "Claude Code",
        sidecar_scope: "claude-code",
        native_doc_target: Some("CLAUDE.md"),
        native_config_target: Some(".sxmc/ai/claude-code-mcp.json"),
        config_shape: Some(ConfigShape::JsonMcpServers),
        official_reference_url: "https://docs.anthropic.com/en/docs/claude-code/memory",
    },
    HostProfileSpec {
        client: AiClientProfile::Cursor,
        label: "Cursor",
        sidecar_scope: "cursor",
        native_doc_target: Some(".cursor/rules/sxmc-cli-ai.md"),
        native_config_target: Some(".cursor/mcp.json"),
        config_shape: Some(ConfigShape::JsonMcpServers),
        official_reference_url: "https://docs.cursor.com/context/rules-for-ai",
    },
    HostProfileSpec {
        client: AiClientProfile::GeminiCli,
        label: "Gemini CLI",
        sidecar_scope: "gemini-cli",
        native_doc_target: Some("GEMINI.md"),
        native_config_target: Some(".gemini/settings.json"),
        config_shape: Some(ConfigShape::JsonMcpServers),
        official_reference_url: "https://geminicli.com/docs/cli/gemini-md/",
    },
    HostProfileSpec {
        client: AiClientProfile::GithubCopilot,
        label: "GitHub Copilot",
        sidecar_scope: "github-copilot",
        native_doc_target: Some(".github/copilot-instructions.md"),
        native_config_target: None,
        config_shape: None,
        official_reference_url: "https://docs.github.com/en/copilot/tutorials/customization-library/custom-instructions/your-first-custom-instructions",
    },
    HostProfileSpec {
        client: AiClientProfile::ContinueDev,
        label: "Continue",
        sidecar_scope: "continue",
        native_doc_target: Some(".continue/rules/sxmc-cli-ai.md"),
        native_config_target: None,
        config_shape: None,
        official_reference_url: "https://docs.continue.dev/customize/rules",
    },
    HostProfileSpec {
        client: AiClientProfile::OpenCode,
        label: "OpenCode",
        sidecar_scope: "opencode",
        native_doc_target: Some("AGENTS.md"),
        native_config_target: Some("opencode.json"),
        config_shape: Some(ConfigShape::JsonMcp),
        official_reference_url: "https://opencode.ai/docs/rules",
    },
    HostProfileSpec {
        client: AiClientProfile::JetbrainsAiAssistant,
        label: "JetBrains AI Assistant",
        sidecar_scope: "jetbrains-ai-assistant",
        native_doc_target: Some(".aiassistant/rules/sxmc-cli-ai.md"),
        native_config_target: None,
        config_shape: None,
        official_reference_url: "https://www.jetbrains.com/help/ai-assistant/configure-project-rules.html",
    },
    HostProfileSpec {
        client: AiClientProfile::Junie,
        label: "Junie",
        sidecar_scope: "junie",
        native_doc_target: Some(".junie/guidelines.md"),
        native_config_target: None,
        config_shape: None,
        official_reference_url: "https://www.jetbrains.com/help/junie/customize-guidelines.html",
    },
    HostProfileSpec {
        client: AiClientProfile::Windsurf,
        label: "Windsurf",
        sidecar_scope: "windsurf",
        native_doc_target: Some(".windsurf/rules/sxmc-cli-ai.md"),
        native_config_target: None,
        config_shape: None,
        official_reference_url: "https://docs.windsurf.com/windsurf/cascade/memories",
    },
    HostProfileSpec {
        client: AiClientProfile::OpenaiCodex,
        label: "OpenAI/Codex",
        sidecar_scope: "openai-codex",
        native_doc_target: Some("AGENTS.md"),
        native_config_target: Some(".codex/mcp.toml"),
        config_shape: Some(ConfigShape::TomlMcpServers),
        official_reference_url: "https://developers.openai.com/codex/cli/",
    },
    HostProfileSpec {
        client: AiClientProfile::GenericStdioMcp,
        label: "Generic stdio MCP",
        sidecar_scope: "generic-stdio-mcp",
        native_doc_target: Some("AGENTS.md"),
        native_config_target: Some(".sxmc/ai/generic-stdio-mcp.json"),
        config_shape: Some(ConfigShape::JsonMcpServers),
        official_reference_url: "https://modelcontextprotocol.io/docs/learn/architecture",
    },
    HostProfileSpec {
        client: AiClientProfile::GenericHttpMcp,
        label: "Generic HTTP MCP",
        sidecar_scope: "generic-http-mcp",
        native_doc_target: Some("AGENTS.md"),
        native_config_target: Some(".sxmc/ai/generic-http-mcp.json"),
        config_shape: Some(ConfigShape::JsonMcpServers),
        official_reference_url: "https://modelcontextprotocol.io/docs/learn/architecture",
    },
];

#[derive(Debug, Clone)]
pub struct WriteOutcome {
    pub label: String,
    pub path: PathBuf,
    pub mode: ArtifactMode,
}

pub fn host_profile_spec(client: AiClientProfile) -> &'static HostProfileSpec {
    AI_HOST_SPECS
        .iter()
        .find(|spec| spec.client == client)
        .expect("missing host profile spec")
}
