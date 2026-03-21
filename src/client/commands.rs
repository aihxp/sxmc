use serde::Serialize;
use serde_json::Value;

/// A CLI command derived from an MCP tool, OpenAPI endpoint, or GraphQL operation.
#[derive(Debug, Clone, Serialize)]
pub struct CommandDef {
    pub name: String,
    pub description: String,
    pub params: Vec<ParamDef>,
}

/// A parameter for a command.
#[derive(Debug, Clone, Serialize)]
pub struct ParamDef {
    pub name: String,
    pub description: String,
    pub param_type: ParamType,
    pub required: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    String,
    Integer,
    Number,
    Boolean,
    Array,
    Object,
}

impl ParamType {
    pub fn from_json_schema(schema_type: &str) -> Self {
        match schema_type {
            "integer" => ParamType::Integer,
            "number" => ParamType::Number,
            "boolean" => ParamType::Boolean,
            "array" => ParamType::Array,
            "object" => ParamType::Object,
            _ => ParamType::String,
        }
    }
}

/// Extract CommandDefs from MCP tool definitions.
pub fn commands_from_mcp_tools(tools: &[rmcp::model::Tool]) -> Vec<CommandDef> {
    tools
        .iter()
        .map(|tool| {
            let params = extract_params_from_schema(tool.input_schema.as_ref());

            CommandDef {
                name: tool.name.to_string(),
                description: tool.description.as_deref().unwrap_or("").to_string(),
                params,
            }
        })
        .collect()
}

fn extract_params_from_schema(schema: &serde_json::Map<String, Value>) -> Vec<ParamDef> {
    let mut params = Vec::new();

    let required_set: Vec<String> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
        for (name, prop_schema) in props {
            let description = prop_schema
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let param_type = prop_schema
                .get("type")
                .and_then(|v| v.as_str())
                .map(ParamType::from_json_schema)
                .unwrap_or(ParamType::String);

            let default = prop_schema.get("default").map(|v| v.to_string());

            params.push(ParamDef {
                name: name.clone(),
                description,
                param_type,
                required: required_set.contains(name),
                default,
            });
        }
    }

    params.sort_by(|a, b| b.required.cmp(&a.required).then(a.name.cmp(&b.name)));
    params
}
