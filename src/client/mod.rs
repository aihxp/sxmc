//! Client adapters for MCP servers and API surfaces.
//!
//! Use this module when you want to connect to:
//!
//! - stdio MCP servers via [`crate::client::mcp_stdio`]
//! - streamable HTTP MCP servers via [`crate::client::mcp_http`]
//! - OpenAPI sources via [`crate::client::openapi`]
//! - GraphQL endpoints via [`crate::client::graphql`]
//! - auto-detected API sources via [`crate::client::api`]
//! - local SQLite schema sources via [`crate::client::database`]

use rmcp::model::CallToolRequestParams;

pub mod api;
pub mod commands;
pub mod database;
pub mod graphql;
pub mod mcp_http;
pub mod mcp_stdio;
pub mod openapi;

pub(crate) fn build_call_tool_params(
    name: &str,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> CallToolRequestParams {
    let mut params = CallToolRequestParams::new(name.to_string());
    // Some MCP servers validate `arguments` as an object even for zero-arg tools.
    // Always sending `{}` keeps those tool calls interoperable.
    params.arguments = Some(arguments);
    params
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map, Value};

    use super::build_call_tool_params;

    #[test]
    fn call_tool_params_include_empty_object_for_zero_arg_tools() {
        let params = build_call_tool_params("read_graph", Map::new());
        assert_eq!(params.arguments, Some(Map::new()));
    }

    #[test]
    fn call_tool_params_preserve_explicit_arguments() {
        let mut arguments = Map::new();
        arguments.insert("a".into(), json!(2));
        arguments.insert("b".into(), Value::String("three".into()));

        let params = build_call_tool_params("get-sum", arguments.clone());
        assert_eq!(params.arguments, Some(arguments));
    }
}
