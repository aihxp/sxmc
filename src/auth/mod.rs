//! Authentication and secret resolution utilities.
//!
//! The most commonly used helpers live in [`crate::auth::secrets`] and support values such
//! as `env:NAME` and `file:/path/to/secret`.

pub mod secrets;
