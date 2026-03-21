use std::collections::HashMap;

use serde_json::{json, Value};

use crate::client::commands::CommandDef;
use crate::client::graphql;
use crate::client::openapi;
use crate::error::{Result, SxmcError};

/// The detected API type.
#[derive(Debug, Clone, PartialEq)]
pub enum ApiType {
    OpenApi,
    GraphQL,
}

/// A unified API client that auto-detects the API type and delegates.
pub enum ApiClient {
    OpenApi(openapi::OpenApiSpec),
    GraphQL(graphql::GraphQLClient),
}

impl ApiClient {
    /// Auto-detect API type from source and connect.
    pub async fn connect(source: &str, auth_headers: &[(String, String)]) -> Result<Self> {
        let api_type = detect_api_type(source, auth_headers).await?;

        match api_type {
            ApiType::OpenApi => {
                let spec = openapi::OpenApiSpec::load(source, auth_headers).await?;
                Ok(ApiClient::OpenApi(spec))
            }
            ApiType::GraphQL => {
                let client = graphql::GraphQLClient::connect(source, auth_headers).await?;
                Ok(ApiClient::GraphQL(client))
            }
        }
    }

    /// Get commands for all operations.
    pub fn commands(&self) -> Vec<CommandDef> {
        match self {
            ApiClient::OpenApi(spec) => spec.commands(),
            ApiClient::GraphQL(client) => client.commands(),
        }
    }

    /// Execute an operation by name.
    pub async fn execute(&self, name: &str, args: &HashMap<String, String>) -> Result<Value> {
        match self {
            ApiClient::OpenApi(spec) => spec.execute(name, args).await,
            ApiClient::GraphQL(client) => client.execute(name, args).await,
        }
    }

    /// Format a listing of available operations.
    pub fn format_list(&self, search: Option<&str>) -> String {
        match self {
            ApiClient::OpenApi(spec) => {
                let ops = spec.list_operations(search);
                openapi::format_operation_list(&ops, None)
            }
            ApiClient::GraphQL(client) => {
                let ops = client.list_operations(search);
                graphql::format_graphql_list(&ops, None)
            }
        }
    }

    /// Return a structured listing of available operations.
    pub fn list_value(&self, search: Option<&str>) -> Value {
        let pattern = search.map(str::to_lowercase);
        let commands: Vec<CommandDef> = self
            .commands()
            .into_iter()
            .filter(|cmd| {
                if let Some(pattern) = &pattern {
                    cmd.name.to_lowercase().contains(pattern)
                        || cmd.description.to_lowercase().contains(pattern)
                } else {
                    true
                }
            })
            .collect();

        json!({
            "api_type": self.api_type(),
            "search": search,
            "count": commands.len(),
            "operations": commands,
        })
    }

    /// Get the API type label.
    pub fn api_type(&self) -> &str {
        match self {
            ApiClient::OpenApi(_) => "OpenAPI",
            ApiClient::GraphQL(_) => "GraphQL",
        }
    }
}

/// Detect the API type from a source URL or file path.
async fn detect_api_type(source: &str, auth_headers: &[(String, String)]) -> Result<ApiType> {
    let lower = source.to_lowercase();

    // File extension hints
    if lower.ends_with(".json") || lower.ends_with(".yaml") || lower.ends_with(".yml") {
        return Ok(ApiType::OpenApi);
    }

    // URL path hints
    if lower.contains("openapi") || lower.contains("swagger") {
        return Ok(ApiType::OpenApi);
    }

    if lower.contains("graphql") || lower.contains("/gql") {
        return Ok(ApiType::GraphQL);
    }

    // If it's a URL, try to fetch and detect from content
    if source.starts_with("http://") || source.starts_with("https://") {
        return detect_from_url(source, auth_headers).await;
    }

    // If it's a file, try to detect from content
    if let Ok(content) = std::fs::read_to_string(source) {
        return detect_from_content(&content);
    }

    Err(SxmcError::Other(format!(
        "Cannot determine API type for: {}. Use --spec or --graphql to specify explicitly.",
        source
    )))
}

/// Detect API type by fetching content from a URL.
async fn detect_from_url(url: &str, auth_headers: &[(String, String)]) -> Result<ApiType> {
    let client = build_client(auth_headers)?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| SxmcError::Other(format!("Failed to fetch: {}", e)))?;

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    // GraphQL endpoints typically don't return JSON specs on GET
    // OpenAPI specs are served as JSON/YAML
    let text = resp
        .text()
        .await
        .map_err(|e| SxmcError::Other(format!("Failed to read response: {}", e)))?;

    if content_type.contains("json") || content_type.contains("yaml") {
        return detect_from_content(&text);
    }

    // Last resort: try parsing as OpenAPI
    detect_from_content(&text)
}

/// Detect API type from content.
fn detect_from_content(content: &str) -> Result<ApiType> {
    // Try JSON parse
    if let Ok(val) = serde_json::from_str::<Value>(content) {
        if val.get("openapi").is_some() || val.get("swagger").is_some() {
            return Ok(ApiType::OpenApi);
        }
        if val.pointer("/data/__schema").is_some() {
            return Ok(ApiType::GraphQL);
        }
    }

    // YAML indicators
    if content.contains("openapi:") || content.contains("swagger:") {
        return Ok(ApiType::OpenApi);
    }

    Err(SxmcError::Other(
        "Cannot determine API type from content. Use --spec or --graphql to specify explicitly."
            .to_string(),
    ))
}

fn build_client(auth_headers: &[(String, String)]) -> Result<reqwest::Client> {
    let mut header_map = reqwest::header::HeaderMap::new();
    for (key, value) in auth_headers {
        if let (Ok(name), Ok(val)) = (
            key.parse::<reqwest::header::HeaderName>(),
            value.parse::<reqwest::header::HeaderValue>(),
        ) {
            header_map.insert(name, val);
        }
    }

    reqwest::Client::builder()
        .default_headers(header_map)
        .build()
        .map_err(|e| SxmcError::Other(format!("Failed to build HTTP client: {}", e)))
}
