use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::client::commands::{CommandDef, ParamDef, ParamType};
use crate::error::{Result, SxmcError};

/// An OpenAPI operation extracted from the spec.
#[derive(Debug, Clone)]
pub struct OpenApiOperation {
    pub operation_id: String,
    pub summary: String,
    pub method: String,
    pub path: String,
    pub parameters: Vec<OpenApiParam>,
    pub request_body_schema: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct OpenApiParam {
    pub name: String,
    pub location: ParamLocation,
    pub description: String,
    pub required: bool,
    pub schema_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamLocation {
    Path,
    Query,
    Header,
    Cookie,
}

/// Parsed OpenAPI spec with operations ready to execute.
pub struct OpenApiSpec {
    pub title: String,
    pub base_url: String,
    pub operations: Vec<OpenApiOperation>,
    client: reqwest::Client,
}

impl OpenApiSpec {
    /// Load an OpenAPI spec from a URL or file path.
    pub async fn load(
        source: &str,
        auth_headers: &[(String, String)],
    ) -> Result<Self> {
        let raw = if source.starts_with("http://") || source.starts_with("https://") {
            fetch_spec(source, auth_headers).await?
        } else {
            let path = Path::new(source);
            let content = std::fs::read_to_string(path)
                .map_err(|e| SxmcError::Other(format!("Failed to read spec file: {}", e)))?;
            content
        };

        // Parse as JSON first, fall back to YAML
        let spec: Value = serde_json::from_str(&raw).or_else(|_| {
            serde_yaml::from_str(&raw)
                .map_err(|e| SxmcError::ParseError(format!("Failed to parse spec: {}", e)))
        })?;

        let title = spec
            .pointer("/info/title")
            .and_then(|v| v.as_str())
            .unwrap_or("API")
            .to_string();

        let base_url = extract_base_url(&spec);
        let operations = extract_operations(&spec);

        let mut header_map = reqwest::header::HeaderMap::new();
        for (key, value) in auth_headers {
            if let (Ok(name), Ok(val)) = (
                key.parse::<reqwest::header::HeaderName>(),
                value.parse::<reqwest::header::HeaderValue>(),
            ) {
                header_map.insert(name, val);
            }
        }

        let client = reqwest::Client::builder()
            .default_headers(header_map)
            .build()
            .map_err(|e| SxmcError::Other(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self {
            title,
            base_url,
            operations,
            client,
        })
    }

    /// Convert operations to CommandDef objects for CLI display.
    pub fn commands(&self) -> Vec<CommandDef> {
        self.operations
            .iter()
            .map(|op| {
                let mut params: Vec<ParamDef> = op
                    .parameters
                    .iter()
                    .map(|p| ParamDef {
                        name: p.name.clone(),
                        description: if p.description.is_empty() {
                            format!("{} parameter ({})", p.location_str(), p.schema_type)
                        } else {
                            p.description.clone()
                        },
                        param_type: ParamType::from_json_schema(&p.schema_type),
                        required: p.required,
                        default: None,
                    })
                    .collect();

                // If there's a request body, add a --body parameter
                if op.request_body_schema.is_some() {
                    params.push(ParamDef {
                        name: "body".to_string(),
                        description: "Request body (JSON)".to_string(),
                        param_type: ParamType::Object,
                        required: false,
                        default: None,
                    });
                }

                params.sort_by(|a, b| b.required.cmp(&a.required).then(a.name.cmp(&b.name)));

                CommandDef {
                    name: op.operation_id.clone(),
                    description: if op.summary.is_empty() {
                        format!("{} {}", op.method.to_uppercase(), op.path)
                    } else {
                        op.summary.clone()
                    },
                    params,
                }
            })
            .collect()
    }

    /// Execute an operation by name with the given arguments.
    pub async fn execute(
        &self,
        operation_id: &str,
        args: &HashMap<String, String>,
    ) -> Result<Value> {
        let op = self
            .operations
            .iter()
            .find(|o| o.operation_id == operation_id)
            .ok_or_else(|| {
                SxmcError::Other(format!("Operation not found: {}", operation_id))
            })?;

        // Build URL with path parameters substituted
        let mut url = format!("{}{}", self.base_url, op.path);
        for param in &op.parameters {
            if param.location == ParamLocation::Path {
                if let Some(value) = args.get(&param.name) {
                    url = url.replace(&format!("{{{}}}", param.name), value);
                }
            }
        }

        // Build query parameters
        let query_params: Vec<(&str, &str)> = op
            .parameters
            .iter()
            .filter(|p| p.location == ParamLocation::Query)
            .filter_map(|p| args.get(&p.name).map(|v| (p.name.as_str(), v.as_str())))
            .collect();

        // Build request
        let mut request = match op.method.as_str() {
            "get" => self.client.get(&url),
            "post" => self.client.post(&url),
            "put" => self.client.put(&url),
            "patch" => self.client.patch(&url),
            "delete" => self.client.delete(&url),
            "head" => self.client.head(&url),
            _ => self.client.get(&url),
        };

        if !query_params.is_empty() {
            request = request.query(&query_params);
        }

        // Add header parameters
        for param in &op.parameters {
            if param.location == ParamLocation::Header {
                if let Some(value) = args.get(&param.name) {
                    request = request.header(&param.name, value);
                }
            }
        }

        // Add request body
        if op.request_body_schema.is_some() {
            if let Some(body_str) = args.get("body") {
                let body: Value = serde_json::from_str(body_str)
                    .map_err(|e| SxmcError::Other(format!("Invalid JSON body: {}", e)))?;
                request = request.json(&body);
            }
        }

        let response = request
            .send()
            .await
            .map_err(|e| SxmcError::Other(format!("HTTP request failed: {}", e)))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| SxmcError::Other(format!("Failed to read response: {}", e)))?;

        // Try to parse as JSON, fall back to wrapping as string
        let value = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| {
            serde_json::json!({
                "status": status.as_u16(),
                "body": text,
            })
        });

        Ok(value)
    }

    /// List operations, optionally filtered by search term.
    pub fn list_operations(&self, search: Option<&str>) -> Vec<&OpenApiOperation> {
        self.operations
            .iter()
            .filter(|op| {
                if let Some(pattern) = search {
                    let p = pattern.to_lowercase();
                    op.operation_id.to_lowercase().contains(&p)
                        || op.summary.to_lowercase().contains(&p)
                        || op.path.to_lowercase().contains(&p)
                } else {
                    true
                }
            })
            .collect()
    }
}

impl OpenApiParam {
    fn location_str(&self) -> &str {
        match self.location {
            ParamLocation::Path => "path",
            ParamLocation::Query => "query",
            ParamLocation::Header => "header",
            ParamLocation::Cookie => "cookie",
        }
    }
}

/// Fetch a spec from a URL.
async fn fetch_spec(url: &str, auth_headers: &[(String, String)]) -> Result<String> {
    let mut header_map = reqwest::header::HeaderMap::new();
    for (key, value) in auth_headers {
        if let (Ok(name), Ok(val)) = (
            key.parse::<reqwest::header::HeaderName>(),
            value.parse::<reqwest::header::HeaderValue>(),
        ) {
            header_map.insert(name, val);
        }
    }

    let client = reqwest::Client::builder()
        .default_headers(header_map)
        .build()
        .map_err(|e| SxmcError::Other(format!("Failed to build HTTP client: {}", e)))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| SxmcError::Other(format!("Failed to fetch spec: {}", e)))?;
    resp.text()
        .await
        .map_err(|e| SxmcError::Other(format!("Failed to read spec response: {}", e)))
}

/// Extract the base URL from an OpenAPI spec.
fn extract_base_url(spec: &Value) -> String {
    // OpenAPI 3.x: servers[0].url
    if let Some(url) = spec.pointer("/servers/0/url").and_then(|v| v.as_str()) {
        return url.to_string();
    }

    // Swagger 2.x: host + basePath
    let host = spec.get("host").and_then(|v| v.as_str()).unwrap_or("localhost");
    let base_path = spec.get("basePath").and_then(|v| v.as_str()).unwrap_or("");
    let scheme = spec
        .get("schemes")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .unwrap_or("https");

    format!("{}://{}{}", scheme, host, base_path)
}

/// Extract operations from an OpenAPI spec.
fn extract_operations(spec: &Value) -> Vec<OpenApiOperation> {
    let mut operations = Vec::new();

    let paths = match spec.get("paths").and_then(|v| v.as_object()) {
        Some(p) => p,
        None => return operations,
    };

    let methods = ["get", "post", "put", "patch", "delete", "head", "options"];

    for (path, path_item) in paths {
        // Path-level parameters (shared across methods)
        let path_params = path_item
            .get("parameters")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for method in &methods {
            if let Some(operation) = path_item.get(*method).and_then(|v| v.as_object()) {
                let operation_id = operation
                    .get("operationId")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| generate_operation_id(method, path));

                let summary = operation
                    .get("summary")
                    .or_else(|| operation.get("description"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // Merge path-level and operation-level parameters
                let mut params = extract_params(&path_params, spec);
                let op_params = operation
                    .get("parameters")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let mut op_extracted = extract_params(&op_params, spec);
                params.append(&mut op_extracted);

                // Deduplicate by name+location
                let mut seen = std::collections::HashSet::new();
                params.retain(|p| seen.insert((p.name.clone(), p.location_str().to_string())));

                let request_body_schema = operation
                    .get("requestBody")
                    .and_then(|rb| {
                        rb.pointer("/content/application~1json/schema")
                            .or_else(|| rb.pointer("/content/application~1x-www-form-urlencoded/schema"))
                    })
                    .cloned();

                operations.push(OpenApiOperation {
                    operation_id,
                    summary,
                    method: method.to_string(),
                    path: path.clone(),
                    parameters: params,
                    request_body_schema,
                });
            }
        }
    }

    operations.sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
    operations
}

/// Extract parameters, resolving $ref if needed.
fn extract_params(params: &[Value], spec: &Value) -> Vec<OpenApiParam> {
    params
        .iter()
        .filter_map(|p| {
            let resolved = resolve_ref(p, spec);
            let obj = resolved.as_object()?;

            let name = obj.get("name")?.as_str()?.to_string();
            let location = match obj.get("in")?.as_str()? {
                "path" => ParamLocation::Path,
                "query" => ParamLocation::Query,
                "header" => ParamLocation::Header,
                "cookie" => ParamLocation::Cookie,
                _ => return None,
            };
            let description = obj
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let required = obj
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(location == ParamLocation::Path);
            let schema_type = obj
                .get("schema")
                .and_then(|s| s.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("string")
                .to_string();

            Some(OpenApiParam {
                name,
                location,
                description,
                required,
                schema_type,
            })
        })
        .collect()
}

/// Resolve a $ref pointer in the spec.
fn resolve_ref<'a>(value: &'a Value, spec: &'a Value) -> &'a Value {
    if let Some(ref_path) = value.get("$ref").and_then(|v| v.as_str()) {
        if let Some(path) = ref_path.strip_prefix("#/") {
            let pointer = format!("/{}", path);
            if let Some(resolved) = spec.pointer(&pointer) {
                return resolved;
            }
        }
    }
    value
}

/// Generate an operation ID from method + path.
fn generate_operation_id(method: &str, path: &str) -> String {
    let clean_path = path
        .trim_start_matches('/')
        .replace('/', "-")
        .replace(['{', '}'], "");

    if clean_path.is_empty() {
        method.to_string()
    } else {
        format!("{}-{}", method, clean_path)
    }
}

/// Format operations for display.
pub fn format_operation_list(ops: &[&OpenApiOperation], search: Option<&str>) -> String {
    let filtered: Vec<&&OpenApiOperation> = if let Some(pattern) = search {
        let p = pattern.to_lowercase();
        ops.iter()
            .filter(|op| {
                op.operation_id.to_lowercase().contains(&p)
                    || op.summary.to_lowercase().contains(&p)
            })
            .collect()
    } else {
        ops.iter().collect()
    };

    if filtered.is_empty() {
        if search.is_some() {
            return "No matching operations found.".to_string();
        }
        return "No operations available.".to_string();
    }

    let mut lines = Vec::new();
    for op in &filtered {
        lines.push(format!(
            "  {} ({} {})",
            op.operation_id,
            op.method.to_uppercase(),
            op.path
        ));
        if !op.summary.is_empty() {
            lines.push(format!("    {}", op.summary));
        }
    }

    format!("Operations ({}):\n{}", filtered.len(), lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_operation_id() {
        assert_eq!(generate_operation_id("get", "/pets"), "get-pets");
        assert_eq!(
            generate_operation_id("get", "/pets/{petId}"),
            "get-pets-petId"
        );
        assert_eq!(
            generate_operation_id("post", "/users/{id}/orders"),
            "post-users-id-orders"
        );
    }

    #[test]
    fn test_extract_base_url_openapi3() {
        let spec: Value = serde_json::json!({
            "openapi": "3.0.0",
            "servers": [{"url": "https://api.example.com/v1"}]
        });
        assert_eq!(extract_base_url(&spec), "https://api.example.com/v1");
    }

    #[test]
    fn test_extract_base_url_swagger2() {
        let spec: Value = serde_json::json!({
            "swagger": "2.0",
            "host": "petstore.swagger.io",
            "basePath": "/v2",
            "schemes": ["https"]
        });
        assert_eq!(
            extract_base_url(&spec),
            "https://petstore.swagger.io/v2"
        );
    }

    #[test]
    fn test_extract_operations() {
        let spec: Value = serde_json::json!({
            "openapi": "3.0.0",
            "paths": {
                "/pets": {
                    "get": {
                        "operationId": "listPets",
                        "summary": "List all pets",
                        "parameters": [
                            {
                                "name": "limit",
                                "in": "query",
                                "description": "Maximum number of items",
                                "required": false,
                                "schema": {"type": "integer"}
                            }
                        ]
                    },
                    "post": {
                        "operationId": "createPet",
                        "summary": "Create a pet",
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {"type": "object"}
                                }
                            }
                        }
                    }
                },
                "/pets/{petId}": {
                    "get": {
                        "operationId": "getPet",
                        "summary": "Get a pet by ID",
                        "parameters": [
                            {
                                "name": "petId",
                                "in": "path",
                                "required": true,
                                "schema": {"type": "string"}
                            }
                        ]
                    }
                }
            }
        });

        let ops = extract_operations(&spec);
        assert_eq!(ops.len(), 3);

        let list_pets = ops.iter().find(|o| o.operation_id == "listPets").unwrap();
        assert_eq!(list_pets.method, "get");
        assert_eq!(list_pets.parameters.len(), 1);
        assert_eq!(list_pets.parameters[0].name, "limit");

        let create_pet = ops.iter().find(|o| o.operation_id == "createPet").unwrap();
        assert!(create_pet.request_body_schema.is_some());

        let get_pet = ops.iter().find(|o| o.operation_id == "getPet").unwrap();
        assert_eq!(get_pet.parameters[0].location, ParamLocation::Path);
    }
}
