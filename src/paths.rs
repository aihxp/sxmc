use std::path::{Path, PathBuf};

use crate::cli_surfaces::AiClientProfile;

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

fn home_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(dir) = env_path("USERPROFILE") {
            return dir;
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(dir) = env_path("HOME") {
            return dir;
        }
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"))
}

pub fn base_config_home() -> PathBuf {
    if let Some(dir) = env_path("XDG_CONFIG_HOME") {
        return dir;
    }
    #[cfg(windows)]
    {
        if let Some(dir) = env_path("APPDATA") {
            return dir;
        }
        return home_dir().join(".config");
    }
    #[cfg(not(windows))]
    {
        home_dir().join(".config")
    }
}

pub fn config_dir() -> PathBuf {
    if let Some(dir) = env_path("SXMC_CONFIG_HOME") {
        return dir;
    }
    base_config_home().join("sxmc")
}

pub fn cache_dir() -> PathBuf {
    if let Some(dir) = env_path("SXMC_CACHE_HOME") {
        return dir;
    }
    if let Some(dir) = env_path("XDG_CACHE_HOME") {
        return dir.join("sxmc");
    }
    #[cfg(windows)]
    {
        if let Some(dir) = env_path("LOCALAPPDATA") {
            return dir.join("sxmc");
        }
        if let Some(dir) = env_path("USERPROFILE") {
            return dir.join(".cache").join("sxmc");
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(dir) = env_path("HOME") {
            return dir.join(".cache").join("sxmc");
        }
    }
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("sxmc")
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InstallScope {
    Local,
    Global,
}

impl InstallScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Global => "global",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallPaths {
    project_root: PathBuf,
    scope: InstallScope,
}

impl InstallPaths {
    pub fn local(project_root: PathBuf) -> Self {
        Self {
            project_root,
            scope: InstallScope::Local,
        }
    }

    pub fn global(project_root: PathBuf) -> Self {
        Self {
            project_root,
            scope: InstallScope::Global,
        }
    }

    pub fn scope(&self) -> InstallScope {
        self.scope
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn state_root(&self) -> PathBuf {
        match self.scope {
            InstallScope::Local => self.project_root.join(".sxmc"),
            InstallScope::Global => config_dir(),
        }
    }

    pub fn profile_dir(&self) -> PathBuf {
        self.state_root().join("ai").join("profiles")
    }

    pub fn sync_state_path(&self) -> PathBuf {
        self.state_root().join("state.json")
    }

    pub fn sidecar_path(&self, scope: &str, original_target: &Path) -> PathBuf {
        let file_name = original_target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact.txt");
        let sidecar_name = format!("{}.sxmc.snippet", file_name);
        self.state_root()
            .join("ai")
            .join(slugify(scope))
            .join(sidecar_name)
    }

    pub fn portable_agent_doc_path(&self) -> PathBuf {
        match self.scope {
            InstallScope::Local => self.project_root.join("AGENTS.md"),
            InstallScope::Global => config_dir().join("AGENTS.md"),
        }
    }

    pub fn host_doc_path(&self, client: AiClientProfile) -> Option<PathBuf> {
        match self.scope {
            InstallScope::Local => local_host_doc_path(&self.project_root, client),
            InstallScope::Global => global_host_doc_path(client),
        }
    }

    pub fn host_config_path(&self, client: AiClientProfile) -> Option<PathBuf> {
        match self.scope {
            InstallScope::Local => local_host_config_path(&self.project_root, client),
            InstallScope::Global => global_host_config_path(client),
        }
    }

    pub fn resolve_skills_path(&self, skills_path: &Path) -> PathBuf {
        if skills_path.is_absolute() {
            return skills_path.to_path_buf();
        }
        match self.scope {
            InstallScope::Local => self.project_root.join(skills_path),
            InstallScope::Global => {
                if skills_path == Path::new(".claude/skills") {
                    home_dir().join(".claude").join("skills")
                } else {
                    config_dir().join(skills_path)
                }
            }
        }
    }
}

fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn local_host_doc_path(project_root: &Path, client: AiClientProfile) -> Option<PathBuf> {
    Some(project_root.join(local_host_doc_target(client)?))
}

fn local_host_config_path(project_root: &Path, client: AiClientProfile) -> Option<PathBuf> {
    Some(project_root.join(local_host_config_target(client)?))
}

fn local_host_doc_target(client: AiClientProfile) -> Option<&'static str> {
    Some(match client {
        AiClientProfile::ClaudeCode => "CLAUDE.md",
        AiClientProfile::Cursor => ".cursor/rules/sxmc-cli-ai.md",
        AiClientProfile::GeminiCli => "GEMINI.md",
        AiClientProfile::GithubCopilot => ".github/copilot-instructions.md",
        AiClientProfile::ContinueDev => ".continue/rules/sxmc-cli-ai.md",
        AiClientProfile::OpenCode => "AGENTS.md",
        AiClientProfile::JetbrainsAiAssistant => ".aiassistant/rules/sxmc-cli-ai.md",
        AiClientProfile::Junie => ".junie/guidelines.md",
        AiClientProfile::Windsurf => ".windsurf/rules/sxmc-cli-ai.md",
        AiClientProfile::OpenaiCodex => "AGENTS.md",
        AiClientProfile::GenericStdioMcp => "AGENTS.md",
        AiClientProfile::GenericHttpMcp => "AGENTS.md",
    })
}

fn local_host_config_target(client: AiClientProfile) -> Option<&'static str> {
    match client {
        AiClientProfile::ClaudeCode => Some(".sxmc/ai/claude-code-mcp.json"),
        AiClientProfile::Cursor => Some(".cursor/mcp.json"),
        AiClientProfile::GeminiCli => Some(".gemini/settings.json"),
        AiClientProfile::GithubCopilot => None,
        AiClientProfile::ContinueDev => None,
        AiClientProfile::OpenCode => Some("opencode.json"),
        AiClientProfile::JetbrainsAiAssistant => None,
        AiClientProfile::Junie => None,
        AiClientProfile::Windsurf => None,
        AiClientProfile::OpenaiCodex => Some(".codex/mcp.toml"),
        AiClientProfile::GenericStdioMcp => Some(".sxmc/ai/generic-stdio-mcp.json"),
        AiClientProfile::GenericHttpMcp => Some(".sxmc/ai/generic-http-mcp.json"),
    }
}

fn global_host_doc_path(client: AiClientProfile) -> Option<PathBuf> {
    let home = home_dir();
    let base = base_config_home();
    Some(match client {
        AiClientProfile::ClaudeCode => home.join(".claude").join("CLAUDE.md"),
        AiClientProfile::Cursor => home.join(".cursor").join("rules").join("sxmc-cli-ai.md"),
        AiClientProfile::GeminiCli => home.join(".gemini").join("GEMINI.md"),
        AiClientProfile::GithubCopilot => home.join(".github").join("copilot-instructions.md"),
        AiClientProfile::ContinueDev => home.join(".continue").join("rules").join("sxmc-cli-ai.md"),
        AiClientProfile::OpenCode => base.join("opencode").join("AGENTS.md"),
        AiClientProfile::JetbrainsAiAssistant => home
            .join(".aiassistant")
            .join("rules")
            .join("sxmc-cli-ai.md"),
        AiClientProfile::Junie => home.join(".junie").join("guidelines.md"),
        AiClientProfile::Windsurf => home.join(".windsurf").join("rules").join("sxmc-cli-ai.md"),
        AiClientProfile::OpenaiCodex => home.join(".codex").join("AGENTS.md"),
        AiClientProfile::GenericStdioMcp | AiClientProfile::GenericHttpMcp => {
            config_dir().join("AGENTS.md")
        }
    })
}

fn global_host_config_path(client: AiClientProfile) -> Option<PathBuf> {
    let home = home_dir();
    let base = base_config_home();
    match client {
        AiClientProfile::ClaudeCode => Some(config_dir().join("ai").join("claude-code-mcp.json")),
        AiClientProfile::Cursor => Some(home.join(".cursor").join("mcp.json")),
        AiClientProfile::GeminiCli => Some(home.join(".gemini").join("settings.json")),
        AiClientProfile::GithubCopilot => None,
        AiClientProfile::ContinueDev => None,
        AiClientProfile::OpenCode => Some(base.join("opencode").join("opencode.json")),
        AiClientProfile::JetbrainsAiAssistant => None,
        AiClientProfile::Junie => None,
        AiClientProfile::Windsurf => None,
        AiClientProfile::OpenaiCodex => Some(home.join(".codex").join("mcp.toml")),
        AiClientProfile::GenericStdioMcp => {
            Some(config_dir().join("ai").join("generic-stdio-mcp.json"))
        }
        AiClientProfile::GenericHttpMcp => {
            Some(config_dir().join("ai").join("generic-http-mcp.json"))
        }
    }
}
