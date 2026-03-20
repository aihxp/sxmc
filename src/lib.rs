//! `sxmc` is a native Rust toolkit for working across three related surfaces:
//!
//! - Skills -> MCP server
//! - MCP server -> CLI
//! - OpenAPI or GraphQL API -> CLI
//!
//! The crate powers the `sxmc` binary, but it also exposes the building blocks
//! that the CLI uses internally. That makes it useful if you want to embed
//! skill discovery, MCP serving, API introspection, or security scanning in a
//! Rust application of your own.
//!
//! # What `sxmc` does
//!
//! `sxmc` treats a skills directory as structured input:
//!
//! - each `SKILL.md` body becomes an MCP prompt
//! - each file in `scripts/` becomes an MCP tool
//! - each file in `references/` becomes an MCP resource
//! - hybrid retrieval tools are added for broad client compatibility
//!
//! It also includes clients for:
//!
//! - local stdio MCP servers
//! - remote streamable HTTP MCP servers
//! - OpenAPI documents
//! - GraphQL endpoints
//!
//! # Module Guide
//!
//! - [`skills`] discovers and parses skills
//! - [`server`] turns parsed skills into an MCP server
//! - [`client`] connects to MCP, OpenAPI, and GraphQL sources
//! - [`security`] scans skills and MCP surfaces for common risks
//! - [`bake`] stores reusable connection definitions
//! - [`auth`] resolves secrets from environment variables and files
//!
//! # Typical CLI Flows
//!
//! The crate is primarily exercised through the `sxmc` binary:
//!
//! ```text
//! sxmc serve --paths ./skills
//! sxmc stdio "sxmc serve --paths ./skills" --list
//! sxmc http http://127.0.0.1:8000/mcp --list
//! sxmc api ./openapi.json --list
//! sxmc scan --paths ./skills
//! ```
//!
//! # Embedding in Rust
//!
//! A minimal server setup looks like:
//!
//! ```no_run
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let paths = vec![PathBuf::from("./skills")];
//!     sxmc::server::serve_stdio(&paths).await?;
//!     Ok(())
//! }
//! ```
//!
//! For remote serving, use [`server::serve_http`].
//!
//! # Security
//!
//! `sxmc` includes native scanners for:
//!
//! - prompt injection patterns
//! - hidden Unicode characters and homoglyphs
//! - embedded secrets
//! - dangerous script patterns
//! - suspicious MCP tool descriptions and responses
//!
//! The CLI exposes these through `sxmc scan`, and the underlying types are in
//! [`security`].

/// Secret resolution helpers used by CLI and client configuration flows.
pub mod auth;
/// Saved connection definitions for MCP servers and APIs.
pub mod bake;
/// Lightweight filesystem cache utilities.
pub mod cache;
/// MCP, OpenAPI, and GraphQL client adapters.
pub mod client;
/// Shared error types used across the crate.
pub mod error;
/// Process execution helpers for script-backed tools.
pub mod executor;
/// Output formatting helpers for CLI rendering.
pub mod output;
/// Security scanners and finding/report models.
pub mod security;
/// MCP server construction and transport serving.
pub mod server;
/// Skill discovery, parsing, and generation.
pub mod skills;
