//! Skill discovery, parsing, and generation.
//!
//! This module covers the full lifecycle of a skill directory:
//!
//! - [`crate::skills::discovery`] finds candidate skill folders
//! - [`crate::skills::parser`] reads `SKILL.md`, `scripts/`, and `references/`
//! - [`crate::skills::models`] defines the in-memory representation
//! - [`crate::skills::generator`] creates skills from OpenAPI sources

pub mod discovery;
pub mod generator;
pub mod models;
pub mod parser;
