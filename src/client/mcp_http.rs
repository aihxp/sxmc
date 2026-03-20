use rmcp::model::*;
use rmcp::service::RunningService;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::{RoleClient, ServiceExt};

use crate::error::{Result, SxmcError};

/// A client connected to an MCP server over HTTP (streamable HTTP transport).
pub struct HttpClient {
    service: RunningService<RoleClient, ()>,
}

impl HttpClient {
    /// Connect to an MCP server over HTTP.
    pub async fn connect(url: &str, headers: &[(String, String)]) -> Result<Self> {
        let config = StreamableHttpClientTransportConfig::with_uri(url);

        let mut header_map = reqwest::header::HeaderMap::new();
        for (key, value) in headers {
            let name: reqwest::header::HeaderName = key
                .parse()
                .map_err(|e| SxmcError::Other(format!("Invalid header name: {}", e)))?;
            let val: reqwest::header::HeaderValue = value
                .parse()
                .map_err(|e| SxmcError::Other(format!("Invalid header value: {}", e)))?;
            header_map.insert(name, val);
        }

        let client = reqwest::Client::builder()
            .default_headers(header_map)
            .build()?;

        let transport = StreamableHttpClientTransport::with_client(client, config);

        let service: RunningService<RoleClient, ()> = ()
            .serve(transport)
            .await
            .map_err(|e| SxmcError::McpError(format!("Failed to connect: {}", e)))?;

        Ok(Self { service })
    }

    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        let result = self
            .service
            .list_all_tools()
            .await
            .map_err(|e| SxmcError::McpError(format!("list_tools failed: {}", e)))?;
        Ok(result)
    }

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

    pub async fn list_prompts(&self) -> Result<Vec<Prompt>> {
        let result = self
            .service
            .list_all_prompts()
            .await
            .map_err(|e| SxmcError::McpError(format!("list_prompts failed: {}", e)))?;
        Ok(result)
    }

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

    pub async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult> {
        let params = ReadResourceRequestParams::new(uri);
        let result = self
            .service
            .read_resource(params)
            .await
            .map_err(|e| SxmcError::McpError(format!("read_resource failed: {}", e)))?;
        Ok(result)
    }

    pub async fn close(self) -> Result<()> {
        self.service
            .cancel()
            .await
            .map_err(|e| SxmcError::McpError(format!("Failed to close: {}", e)))?;
        Ok(())
    }
}
