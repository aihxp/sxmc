# sxmc

One Rust binary for turning agent-facing interfaces into practical tools: serve skills over MCP, use MCP servers from the terminal, run APIs as CLIs, and generate startup-ready AI surfaces from existing CLIs.

[Crates.io](https://crates.io/crates/sxmc) | [docs.rs](https://docs.rs/sxmc/latest/sxmc/)

## Why It Exists

Without `sxmc`, the same capability usually gets rebuilt several times:
- a skill adapter for one agent
- a JSON-RPC script for one MCP server
- a thin shell wrapper for one API
- per-host startup docs and config files for AI tools

`sxmc` collapses that into one installable binary with four core flows:

```text
Skills -> MCP server
MCP server -> CLI
API -> CLI
CLI -> AI startup surfaces
```

That means less glue code, narrower MCP discovery, fewer retry turns, and much less repeated setup across Claude Code, Cursor, Gemini CLI, Copilot, Codex-style tools, and generic MCP clients.

## Install

```bash
cargo install sxmc
```

Other channels:
- GitHub Releases: prebuilt archives with checksums
- npm wrapper: [`packaging/npm`](packaging/npm)
- Homebrew formula: [`packaging/homebrew/sxmc.rb`](packaging/homebrew/sxmc.rb)

## Quick Start

Serve local skills over MCP:

```bash
sxmc serve
sxmc serve --transport http --host 127.0.0.1 --port 8000
```

Inspect and call any MCP server from the terminal:

```bash
sxmc stdio "npx @modelcontextprotocol/server-memory" --list
sxmc stdio "npx @modelcontextprotocol/server-memory" create_entities 'entities=[{"name":"sxmc","entityType":"project","observations":["Rust MCP bridge"]}]'
```

Use a baked, token-efficient MCP workflow:

```bash
sxmc bake create memory --type stdio --source "npx @modelcontextprotocol/server-memory"
sxmc mcp servers
sxmc mcp info memory/create_entities --format toon
sxmc mcp call memory/create_entities '{"entities":[{"name":"sxmc","entityType":"project","observations":["Rust MCP bridge"]}]}'
```

Run an API as a CLI:

```bash
sxmc api https://petstore3.swagger.io/api/v3/openapi.json --list
sxmc api https://petstore3.swagger.io/api/v3/openapi.json findPetsByStatus status=available
```

Turn a CLI into startup-facing AI artifacts:

```bash
sxmc inspect cli gh --format toon
sxmc inspect cli cargo --depth 1 --format json-pretty
sxmc init ai --from-cli gh --coverage full --mode preview
sxmc init ai --from-cli gh --coverage full --host claude-code,cursor,github-copilot --mode apply
sxmc init ai --from-cli gh --coverage full --host claude-code --mode apply --remove
```

Generate shell completions:

```bash
sxmc completions zsh > "${fpath[1]}/_sxmc"
sxmc completions bash > ~/.local/share/bash-completion/completions/sxmc
```

## Practical Wins

- `sxmc stdio "<cmd>" --list` replaces ad hoc JSON-RPC client scripts for MCP discovery.
- `sxmc mcp grep "file"` searches across baked MCP servers, which is hard to reproduce cleanly with one-off tooling.
- `sxmc scan` catches hidden Unicode, dangerous permissions, and prompt-injection patterns that plain `grep` misses.
- `sxmc inspect cli ...` plus `sxmc init ai ...` turns per-host AI setup into generated, reviewable artifacts.

The current validation docs capture the real-world comparison set, token/turn estimates, and hidden retry-cost analysis.

## Command Overview

- `sxmc serve`: expose skills as stdio or HTTP MCP
- `sxmc skills`: list, inspect, run, and generate skills
- `sxmc stdio` / `sxmc http`: raw MCP bridge and debugging layer
- `sxmc mcp`: baked daily-use MCP workflow
- `sxmc api` / `sxmc spec` / `sxmc graphql`: API-to-CLI bridge
- `sxmc scan`: security scanning for skills and MCP surfaces
- `sxmc inspect` / `sxmc init` / `sxmc scaffold`: CLI-to-AI inspection and scaffolding
- `sxmc bake`: saved connections
- `sxmc completions`: shell completion generation

## Safety and Reliability

- preview-first AI artifact generation
- low-confidence CLI profiles are blocked from startup-doc generation unless explicitly overridden
- managed markdown/TOML blocks instead of wholesale overwrites
- recursive CLI inspection with `sxmc inspect cli --depth 1`
- cleanup support with `sxmc init ai --remove`
- atomic bake persistence
- baked stdio configs can pin a base directory for portable relative paths
- configurable timeouts for networked commands
- HTTP MCP guardrails for max concurrency and request body size
- stateful MCP workflows supported through `sxmc mcp session`

## Docs

- [Usage](docs/USAGE.md): install, daily workflows, MCP usage, CLI-to-AI, completions
- [Architecture](docs/ARCHITECTURE.md): module map, data flow, and design boundaries
- [Demo](docs/DEMO.md): short scripted demo path for terminal recordings
- [Operations](docs/OPERATIONS.md): hosting, release process, branch policy, distribution
- [Validation](docs/VALIDATION.md): tests, smoke checks, compatibility, token/turn findings
- [Product Contract](docs/PRODUCT_CONTRACT.md): explicit support boundary
- [CLI Surfaces](docs/CLI_SURFACES.md): CLI-to-AI model, profile contract, write policy
- [CLI to AI Compatibility](docs/CLI_TO_AI_COMPATIBILITY.md): host-by-host coverage matrix

## Development

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Use the validation flow before release:

```bash
bash scripts/certify_release.sh target/debug/sxmc tests/fixtures
bash scripts/smoke_real_world_mcps.sh target/debug/sxmc
```

## License

MIT
