# Contributing to sxmc

## Getting Started

1. Fork the repo and clone it
2. Install Rust (stable): https://rustup.rs
3. Build: `cargo build`
4. Run tests: `cargo test`

## Development

### Project Structure

```
src/
├── main.rs              # CLI entrypoint (clap)
├── lib.rs               # Public re-exports
├── skills/              # Skill discovery, parsing, generation
├── security/            # Security scanning (skills + MCP servers)
├── server/              # MCP server (rmcp)
├── client/              # MCP + API clients (stdio, HTTP, OpenAPI, GraphQL)
├── auth/                # Secret resolution
├── bake/                # Saved connection configs
├── output/              # Output formatting
├── cache.rs             # File-based caching
├── executor.rs          # Subprocess execution
└── error.rs             # Error types
```

### Running Tests

```bash
# All tests
cargo test

# Unit tests only
cargo test --lib

# Integration tests only
cargo test --test cli_integration

# Specific test
cargo test test_scan_malicious_skill
```

### Adding Security Patterns

Security patterns live in `src/security/patterns.rs`. Each pattern is a `LazyLock<Regex>` compiled once at startup.

To add a new detection:
1. Add the regex pattern in `patterns.rs`
2. Call it from the appropriate scanner (`skill_scanner.rs` or `mcp_scanner.rs`)
3. Add a test in the scanner module
4. Add a test fixture in `tests/fixtures/` if needed

### Adding API Support

The client module uses a `CommandDef` abstraction shared across all API types. To add a new API type:
1. Create `src/client/your_api.rs`
2. Implement parsing to produce `Vec<CommandDef>`
3. Implement execution that takes a `CommandDef` + args and makes HTTP calls
4. Wire it into `src/client/api.rs` for auto-detection

### Hybrid Skill Surface

`sxmc serve` exposes skills in two ways at once:
1. Native MCP primitives:
   prompts for skill bodies, tools for `scripts/`, resources for `references/`
2. Generic MCP tools:
   `get_available_skills`, `get_skill_details`, and `get_skill_related_file`

That hybrid model is what allows `skills -> MCP -> CLI` to work through the existing
`sxmc stdio` and `sxmc http` bridges without requiring prompt/resource-specific CLI flags.

## Pull Requests

- Keep PRs focused on a single change
- Include tests for new functionality
- Run `cargo test` and `cargo clippy` before submitting
- Update the README and CLI examples if changing user-facing behavior or flags
- Update `docs/CLIENTS.md` if client integration steps or compatibility status change
- Update `docs/RELEASING.md` if release or packaging steps change
