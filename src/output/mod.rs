use clap::ValueEnum;
use rmcp::model::{
    CallToolResult, GetPromptResult, Prompt, PromptMessageContent, PromptMessageRole,
    ReadResourceResult, Resource, ResourceContents, ServerInfo, Tool,
};
use serde_json::{json, Map, Value};

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum StructuredOutputFormat {
    Json,
    JsonPretty,
    Toon,
}

pub fn resolve_structured_format(
    format: Option<StructuredOutputFormat>,
    pretty: bool,
) -> StructuredOutputFormat {
    format.unwrap_or(if pretty {
        StructuredOutputFormat::JsonPretty
    } else {
        StructuredOutputFormat::Json
    })
}

pub fn format_structured_value(value: &Value, format: StructuredOutputFormat) -> String {
    match format {
        StructuredOutputFormat::Json => value.to_string(),
        StructuredOutputFormat::JsonPretty => {
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        }
        StructuredOutputFormat::Toon => encode_toon(value),
    }
}

/// Format a CallToolResult for display.
pub fn format_tool_result(result: &CallToolResult, pretty: bool) -> String {
    let texts: Vec<String> = result
        .content
        .iter()
        .filter_map(|c| c.raw.as_text().map(|t| t.text.clone()))
        .collect();

    let output = texts.join("\n");

    if pretty {
        // Try to parse as JSON and pretty-print
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&output) {
            if let Ok(pretty_str) = serde_json::to_string_pretty(&val) {
                return pretty_str;
            }
        }
    }

    output
}

/// Format a GetPromptResult for display.
pub fn format_prompt_result(result: &GetPromptResult, pretty: bool) -> String {
    if pretty {
        return serde_json::to_string_pretty(result)
            .unwrap_or_else(|_| serde_json::to_string(result).unwrap_or_default());
    }

    if result.messages.len() == 1 {
        if let Some(message) = result.messages.first() {
            if let PromptMessageContent::Text { text } = &message.content {
                return text.clone();
            }
        }
    }

    let messages: Vec<String> = result
        .messages
        .iter()
        .map(|message| {
            let role = match message.role {
                PromptMessageRole::User => "user",
                PromptMessageRole::Assistant => "assistant",
            };
            let content = match &message.content {
                PromptMessageContent::Text { text } => text.clone(),
                _ => serde_json::to_string_pretty(&message.content).unwrap_or_else(|_| {
                    serde_json::to_string(&message.content).unwrap_or_default()
                }),
            };
            format!("[{}]\n{}", role, content)
        })
        .collect();

    messages.join("\n\n")
}

/// Format a ReadResourceResult for display.
pub fn format_resource_result(result: &ReadResourceResult, pretty: bool) -> String {
    if pretty {
        return serde_json::to_string_pretty(result)
            .unwrap_or_else(|_| serde_json::to_string(result).unwrap_or_default());
    }

    let contents: Vec<String> = result
        .contents
        .iter()
        .map(|content| match content {
            ResourceContents::TextResourceContents { text, .. } => text.clone(),
            ResourceContents::BlobResourceContents { blob, .. } => blob.clone(),
        })
        .collect();

    contents.join("\n\n")
}

/// Format MCP tools as a list for display.
pub fn format_tool_list(tools: &[Tool], search: Option<&str>) -> String {
    let mut lines = Vec::new();

    for tool in tools {
        let name = tool.name.as_ref();
        let desc = tool.description.as_deref().unwrap_or("");

        if let Some(pattern) = search {
            let pattern_lower = pattern.to_lowercase();
            if !name.to_lowercase().contains(&pattern_lower)
                && !desc.to_lowercase().contains(&pattern_lower)
            {
                continue;
            }
        }

        lines.push(format!("  {}", name));
        if !desc.is_empty() {
            lines.push(format!("    {}", desc));
        }
    }

    if lines.is_empty() {
        if search.is_some() {
            return "No matching tools found.".to_string();
        }
        return "No tools available.".to_string();
    }

    format!("Tools ({}):\n{}", tools.len(), lines.join("\n"))
}

/// Format MCP prompts as a list for display.
pub fn format_prompt_list(prompts: &[Prompt]) -> String {
    let mut lines = Vec::new();

    for prompt in prompts {
        lines.push(format!("  {}", prompt.name));
        if let Some(ref desc) = prompt.description {
            lines.push(format!("    {}", desc));
        }
    }

    if lines.is_empty() {
        return "No prompts available.".to_string();
    }

    format!("Prompts ({}):\n{}", prompts.len(), lines.join("\n"))
}

/// Format MCP resources as a list for display.
pub fn format_resource_list(resources: &[Resource]) -> String {
    let mut lines = Vec::new();

    for resource in resources {
        lines.push(format!("  {} ({})", resource.name, resource.uri));
        if let Some(ref desc) = resource.description {
            lines.push(format!("    {}", desc));
        }
    }

    if lines.is_empty() {
        return "No resources available.".to_string();
    }

    format!("Resources ({}):\n{}", resources.len(), lines.join("\n"))
}

pub fn format_tool_detail(tool: &Tool, pretty: bool) -> String {
    let summary = summarize_tool(tool);
    if pretty {
        return serde_json::to_string_pretty(&summary).unwrap_or_else(|_| summary.to_string());
    }

    let mut lines = vec![format!("Tool: {}", tool.name)];

    if let Some(title) = &tool.title {
        lines.push(format!("Title: {}", title));
    }
    if let Some(description) = &tool.description {
        lines.push(format!("Description: {}", description));
    }

    let parameters = summary["parameters"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if parameters.is_empty() {
        lines.push("Parameters: none".to_string());
    } else {
        lines.push(format!("Parameters ({}):", parameters.len()));
        for parameter in parameters {
            let name = parameter["name"].as_str().unwrap_or("unknown");
            let ty = parameter["type"].as_str().unwrap_or("any");
            let required = parameter["required"].as_bool().unwrap_or(false);
            let mut line = format!(
                "  {}{} ({})",
                name,
                if required { " [required]" } else { "" },
                ty
            );
            if let Some(description) = parameter["description"].as_str() {
                line.push_str(&format!(" - {}", description));
            }
            if let Some(values) = parameter["enum"].as_array() {
                let values = values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ");
                if !values.is_empty() {
                    line.push_str(&format!(" [enum: {}]", values));
                }
            }
            lines.push(line);
        }
    }

    let capabilities = summary["execution"].clone();
    if capabilities != Value::Null {
        lines.push(format!("Execution: {}", capabilities));
    }

    let annotations = summary["annotations"].clone();
    if annotations != Value::Null {
        lines.push(format!("Hints: {}", annotations));
    }

    lines.join("\n")
}

pub fn summarize_server_info(server_info: Option<&ServerInfo>) -> Value {
    match server_info {
        Some(info) => json!({
            "protocol_version": info.protocol_version.to_string(),
            "server": {
                "name": info.server_info.name,
                "version": info.server_info.version,
                "title": info.server_info.title,
            },
            "instructions": info.instructions,
            "capabilities": {
                "tools": info.capabilities.tools.is_some(),
                "prompts": info.capabilities.prompts.is_some(),
                "resources": info.capabilities.resources.is_some(),
                "logging": info.capabilities.logging.is_some(),
                "completions": info.capabilities.completions.is_some(),
                "tasks": info.capabilities.tasks.is_some(),
                "extensions": info.capabilities.extensions.is_some(),
            }
        }),
        None => json!({
            "protocol_version": Value::Null,
            "server": Value::Null,
            "instructions": Value::Null,
            "capabilities": {
                "tools": Value::Null,
                "prompts": Value::Null,
                "resources": Value::Null,
                "logging": Value::Null,
                "completions": Value::Null,
                "tasks": Value::Null,
                "extensions": Value::Null,
            }
        }),
    }
}

pub fn summarize_tool(tool: &Tool) -> Value {
    let required = tool
        .input_schema
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(ToString::to_string))
        .collect::<Vec<_>>();

    let parameters = tool
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .iter()
                .map(|(name, schema)| summarize_schema_property(name, schema, &required))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    json!({
        "name": tool.name,
        "title": tool.title,
        "description": tool.description,
        "parameters": parameters,
        "annotations": tool.annotations.as_ref().map(|annotations| json!({
            "read_only": annotations.read_only_hint,
            "destructive": annotations.destructive_hint,
            "idempotent": annotations.idempotent_hint,
            "open_world": annotations.open_world_hint,
        })),
        "execution": tool.execution.as_ref().map(|execution| json!({
            "task_support": execution.task_support.map(|support| format!("{:?}", support).to_lowercase()),
        })),
        "input_schema": Value::Object((*tool.input_schema).clone()),
        "output_schema": tool
            .output_schema
            .as_ref()
            .map(|schema| Value::Object((**schema).clone())),
    })
}

pub fn summarize_prompt(prompt: &Prompt) -> Value {
    json!({
        "name": prompt.name,
        "title": prompt.title,
        "description": prompt.description,
        "arguments": prompt.arguments.as_ref().map(|arguments| {
            arguments.iter().map(|argument| {
                json!({
                    "name": argument.name,
                    "title": argument.title,
                    "description": argument.description,
                    "required": argument.required.unwrap_or(false),
                })
            }).collect::<Vec<_>>()
        }).unwrap_or_default(),
    })
}

pub fn summarize_resource(resource: &Resource) -> Value {
    json!({
        "name": resource.name,
        "title": resource.title,
        "description": resource.description,
        "uri": resource.uri,
        "mime_type": resource.mime_type,
        "size": resource.size,
    })
}

fn summarize_schema_property(name: &str, schema: &Value, required: &[String]) -> Value {
    let schema_type = schema
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("any")
        .to_string();

    let enum_values = schema
        .get("enum")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    json!({
        "name": name,
        "type": schema_type,
        "required": required.iter().any(|value| value == name),
        "description": schema.get("description").and_then(Value::as_str),
        "enum": enum_values,
    })
}

fn encode_toon(value: &Value) -> String {
    match value {
        Value::Object(map) => render_object(map, 0),
        Value::Array(items) => render_array(items, 0, None),
        _ => render_scalar(value),
    }
}

fn render_object(map: &Map<String, Value>, indent: usize) -> String {
    if map.is_empty() {
        return "{}".to_string();
    }

    let mut lines = Vec::new();
    for (key, value) in map {
        match value {
            Value::Object(child) => {
                lines.push(format!("{}{}:", indent_str(indent), key));
                lines.push(render_object(child, indent + 2));
            }
            Value::Array(items) => {
                if let Some(table) = render_tabular_array(items, indent, Some(key)) {
                    lines.push(table);
                } else if items.iter().all(is_primitive) {
                    let joined = items
                        .iter()
                        .map(render_scalar)
                        .collect::<Vec<_>>()
                        .join(", ");
                    lines.push(format!(
                        "{}{}[{}]: {}",
                        indent_str(indent),
                        key,
                        items.len(),
                        joined
                    ));
                } else {
                    lines.push(format!(
                        "{}{}: {}",
                        indent_str(indent),
                        key,
                        Value::Array(items.clone())
                    ));
                }
            }
            _ => lines.push(format!(
                "{}{}: {}",
                indent_str(indent),
                key,
                render_scalar(value)
            )),
        }
    }

    lines.join("\n")
}

fn render_array(items: &[Value], indent: usize, name: Option<&str>) -> String {
    if let Some(table) = render_tabular_array(items, indent, name) {
        return table;
    }

    if items.is_empty() {
        return "[]".to_string();
    }

    if items.iter().all(is_primitive) {
        let joined = items
            .iter()
            .map(render_scalar)
            .collect::<Vec<_>>()
            .join(", ");
        return match name {
            Some(name) => format!(
                "{}{}[{}]: {}",
                indent_str(indent),
                name,
                items.len(),
                joined
            ),
            None => format!("[{}]: {}", items.len(), joined),
        };
    }

    Value::Array(items.to_vec()).to_string()
}

fn render_tabular_array(items: &[Value], indent: usize, name: Option<&str>) -> Option<String> {
    let headers = tabular_headers(items)?;
    let header_prefix = match name {
        Some(name) => format!(
            "{}{}[{}]{{{}}}:",
            indent_str(indent),
            name,
            items.len(),
            headers.join(",")
        ),
        None => format!("[{}]{{{}}}:", items.len(), headers.join(",")),
    };

    let mut lines = vec![header_prefix];
    for item in items {
        let object = item.as_object()?;
        let row = headers
            .iter()
            .map(|key| {
                object
                    .get(key)
                    .map(render_scalar)
                    .unwrap_or_else(|| "null".to_string())
            })
            .collect::<Vec<_>>()
            .join(",");
        lines.push(format!("{}{}", indent_str(indent + 2), row));
    }

    Some(lines.join("\n"))
}

fn tabular_headers(items: &[Value]) -> Option<Vec<String>> {
    if items.is_empty() {
        return None;
    }

    let first = items.first()?.as_object()?;
    if first.is_empty() {
        return None;
    }

    let headers: Vec<String> = first.keys().cloned().collect();

    for item in items {
        let object = item.as_object()?;
        if object.len() != headers.len() {
            return None;
        }
        if !headers.iter().all(|key| object.contains_key(key)) {
            return None;
        }
        if !object.values().all(is_primitive) {
            return None;
        }
    }

    Some(headers)
}

fn render_scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(string) => serde_json::to_string(string).unwrap_or_else(|_| "\"\"".into()),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

fn is_primitive(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

fn indent_str(indent: usize) -> &'static str {
    const SPACES: &str = "                                ";
    &SPACES[..indent.min(SPACES.len())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_structured_value_json_compact() {
        let value = serde_json::json!({"name": "Ada", "count": 2});
        let rendered = format_structured_value(&value, StructuredOutputFormat::Json);
        let reparsed: Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(reparsed, value);
    }

    #[test]
    fn test_format_structured_value_toon_object() {
        let value = serde_json::json!({
            "name": "Ada",
            "active": true,
            "stats": {
                "count": 2
            }
        });

        let output = format_structured_value(&value, StructuredOutputFormat::Toon);
        assert!(output.contains(r#"name: "Ada""#));
        assert!(output.contains("active: true"));
        assert!(output.contains("stats:"));
        assert!(output.contains("  count: 2"));
    }

    #[test]
    fn test_format_structured_value_toon_tabular_array() {
        let value = serde_json::json!({
            "pets": [
                {"id": 1, "name": "Mochi"},
                {"id": 2, "name": "Pixel"}
            ]
        });

        let output = format_structured_value(&value, StructuredOutputFormat::Toon);
        assert!(output.contains("pets[2]{id,name}:"));
        assert!(output.contains(r#"  1,"Mochi""#));
        assert!(output.contains(r#"  2,"Pixel""#));
    }
}
