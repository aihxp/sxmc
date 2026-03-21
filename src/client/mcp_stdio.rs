use std::path::Path;

use rmcp::model::*;
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use rmcp::{RoleClient, ServiceExt};
use tokio::process::Command;

use crate::cli_surfaces::parse_command_spec;
use crate::client::build_call_tool_params;
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
            .list_all_tools()
            .await
            .map_err(|e| SxmcError::McpError(format!("list_tools failed: {}", e)))?;
        Ok(result)
    }

    /// Call a tool by name with JSON arguments.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<CallToolResult> {
        let params = build_call_tool_params(name, arguments);

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
            .list_all_prompts()
            .await
            .map_err(|e| SxmcError::McpError(format!("list_prompts failed: {}", e)))?;
        Ok(result)
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
            .list_all_resources()
            .await
            .map_err(|e| SxmcError::McpError(format!("list_resources failed: {}", e)))?;
        Ok(result)
    }

    /// Return negotiated MCP server information from the initialization handshake.
    pub fn server_info(&self) -> Option<ServerInfo> {
        self.service.peer_info().cloned()
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
