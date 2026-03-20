use std::path::Path;

use rmcp::model::*;
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use rmcp::{RoleClient, ServiceExt};
use tokio::process::Command;

use crate::error::{Result, SxmcError};

/// A client connected to an MCP server over stdio.
pub struct StdioClient {
    service: RunningService<RoleClient, ()>,
}

impl StdioClient {
    /// Connect to an MCP server by spawning a subprocess.
    pub async fn connect(
        command: &str,
        env_vars: &[(String, String)],
        cwd: Option<&Path>,
    ) -> Result<Self> {
        let parts = parse_command_spec(command)?;
        if parts.is_empty() {
            return Err(SxmcError::Other("Empty command spec".into()));
        }

        let mut cmd = Command::new(&parts[0]);
        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }
        for (key, value) in env_vars {
            cmd.env(key, value);
        }
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }

        let transport = TokioChildProcess::new(cmd)
            .map_err(|e| SxmcError::McpError(format!("Failed to spawn: {}", e)))?;

        let service = ()
            .serve(transport)
            .await
            .map_err(|e| SxmcError::McpError(format!("Failed to initialize MCP session: {}", e)))?;

        Ok(Self { service })
    }

    /// List all available tools.
    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        let result = self
            .service
            .list_tools(None)
            .await
            .map_err(|e| SxmcError::McpError(format!("list_tools failed: {}", e)))?;
        Ok(result.tools)
    }

    /// Call a tool by name with JSON arguments.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<CallToolResult> {
        let mut params = CallToolRequestParams::new(name.to_string());
        if !arguments.is_empty() {
            params.arguments = Some(arguments);
        }

        let result = self
            .service
            .call_tool(params)
            .await
            .map_err(|e| SxmcError::McpError(format!("call_tool failed: {}", e)))?;
        Ok(result)
    }

    /// List all available prompts.
    pub async fn list_prompts(&self) -> Result<Vec<Prompt>> {
        let result = self
            .service
            .list_prompts(None)
            .await
            .map_err(|e| SxmcError::McpError(format!("list_prompts failed: {}", e)))?;
        Ok(result.prompts)
    }

    /// Get a prompt by name.
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<GetPromptResult> {
        let mut params = GetPromptRequestParams::new(name);
        params.arguments = arguments;

        let result = self
            .service
            .get_prompt(params)
            .await
            .map_err(|e| SxmcError::McpError(format!("get_prompt failed: {}", e)))?;
        Ok(result)
    }

    /// List all available resources.
    pub async fn list_resources(&self) -> Result<Vec<Resource>> {
        let result = self
            .service
            .list_resources(None)
            .await
            .map_err(|e| SxmcError::McpError(format!("list_resources failed: {}", e)))?;
        Ok(result.resources)
    }

    /// Read a resource by URI.
    pub async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult> {
        let params = ReadResourceRequestParams::new(uri);
        let result = self
            .service
            .read_resource(params)
            .await
            .map_err(|e| SxmcError::McpError(format!("read_resource failed: {}", e)))?;
        Ok(result)
    }

    /// Shut down the connection.
    pub async fn close(self) -> Result<()> {
        self.service
            .cancel()
            .await
            .map_err(|e| SxmcError::McpError(format!("Failed to close: {}", e)))?;
        Ok(())
    }
}

fn parse_command_spec(command: &str) -> Result<Vec<String>> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    if trimmed.starts_with('[') {
        return serde_json::from_str::<Vec<String>>(trimmed).map_err(|e| {
            SxmcError::Other(format!(
                "Invalid stdio command JSON array. Expected [\"cmd\", \"arg1\", ...]: {}",
                e
            ))
        });
    }

    #[cfg(windows)]
    {
        if let Some(parts) = parse_windows_command_spec(trimmed) {
            return Ok(parts);
        }
        // Fallback to simple whitespace splitting on Windows to preserve backslashes
        // (shlex uses POSIX rules which treat backslashes as escape characters)
        return Ok(trimmed.split_whitespace().map(str::to_string).collect());
    }

    #[cfg(not(windows))]
    shlex::split(trimmed).ok_or_else(|| {
        SxmcError::Other(
            "Invalid stdio command string. Use shell-style quoting or a JSON array command spec."
                .into(),
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

    let executable_pattern =
        regex::Regex::new(r"(?i)^(.+?\.(exe|cmd|bat|ps1))(?:\s+(.*))?$").ok()?;
    let captures = executable_pattern.captures(trimmed)?;
    let executable = captures.get(1)?.as_str();
    let mut parts = vec![executable.to_string()];

    if let Some(args) = captures.get(3) {
        parts.extend(args.as_str().split_whitespace().map(str::to_string));
    }

    Some(parts)
}

#[cfg(test)]
mod tests {
    use super::parse_command_spec;

    #[test]
    fn test_parse_command_spec_json_array() {
        let parsed = parse_command_spec(r#"["sxmc","serve","--paths","tests/fixtures"]"#).unwrap();
        assert_eq!(parsed, vec!["sxmc", "serve", "--paths", "tests/fixtures"]);
    }

    #[cfg(windows)]
    #[test]
    fn test_parse_command_spec_windows_executable_path() {
        let parsed = parse_command_spec(
            r#"D:\a\sxmc\sxmc\target\debug\sxmc.exe serve --paths tests/fixtures"#,
        )
        .unwrap();
        assert_eq!(
            parsed,
            vec![
                r#"D:\a\sxmc\sxmc\target\debug\sxmc.exe"#,
                "serve",
                "--paths",
                "tests/fixtures"
            ]
        );
    }
}
