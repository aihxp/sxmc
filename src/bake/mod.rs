//! Persistent saved connection configs.
//!
//! A bake stores enough information to reconnect to an MCP server or API
//! source without repeating flags each time.

pub mod config;

pub use config::{BakeConfig, BakeStore};
