//! Client adapters for MCP servers and API surfaces.
//!
//! Use this module when you want to connect to:
//!
//! - stdio MCP servers via [`crate::client::mcp_stdio`]
//! - streamable HTTP MCP servers via [`crate::client::mcp_http`]
//! - OpenAPI sources via [`crate::client::openapi`]
//! - GraphQL endpoints via [`crate::client::graphql`]
//! - auto-detected API sources via [`crate::client::api`]

pub mod api;
pub mod commands;
pub mod graphql;
pub mod mcp_http;
pub mod mcp_stdio;
pub mod openapi;
