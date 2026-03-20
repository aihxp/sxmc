# sxmc

AI-agnostic Skills x MCP x CLI — a single Rust binary that turns skills into MCP servers, MCP servers into CLI commands, and any API into a CLI.

[Crates.io](https://crates.io/crates/sxmc) | [docs.rs](https://docs.rs/sxmc/latest/sxmc/)

## What is sxmc?

[MCP (Model Context Protocol)](https://modelcontextprotocol.io/) is an open standard for connecting AI assistants to external tools and data sources. Today, if you have skills (structured AI instructions), MCP servers, and APIs, each one requires its own adapter, its own client setup, and its own CLI wrapper. There is no single tool that bridges all three.

**sxmc** solves this. One Rust binary that:
- Turns skill directories into MCP servers (stdio or remote HTTP)
- Makes any MCP server usable from the command line
- Auto-generates CLI commands from OpenAPI and GraphQL specs
- Scans skills and MCP servers for security threats

```
Skills  -->  MCP Server    (serve skills to any MCP client)
MCP Server  -->  CLI       (turn any MCP server into CLI commands)
Any API  -->  CLI           (OpenAPI & GraphQL auto-detection)
```

## Table of Contents

- [What is sxmc?](#what-is-sxmc)
- [Prerequisites](#prerequisites)
- [Install](#install)
- [Quick Start](#quick-start)
  - [Serve skills as an MCP server](#serve-skills-as-an-mcp-server)
  - [Run a skill directly](#run-a-skill-directly)
  - [Any MCP server as CLI](#any-mcp-server-as-cli)
  - [Any API as CLI](#any-api-as-cli)
  - [Security scanning](#security-scanning)
  - [Bake and reuse connections](#bake-and-reuse-connections)
  - [Generate skills from APIs](#generate-skills-from-apis)
- [Skills](#skills)
- [Security Scanning](#security-scanning-1)
- [Architecture](#architecture)
- [Client Compatibility](#client-compatibility)
- [CLI Reference](#cli-reference)
- [Development](#development)
- [Acknowledgements](#acknowledgements)
- [License](#license)

## Prerequisites

- **Rust toolchain** (stable) — install via [rustup.rs](https://rustup.rs) (required for `cargo install`)
- **Node.js** (optional) — only needed if using the [npm wrapper](packaging/npm)
- No runtime dependencies — sxmc compiles to a single static binary

## Install

Install from crates.io:

```bash
cargo install sxmc
```

Other channels:

- GitHub Releases: prebuilt archives plus `.sha256` files
- npm wrapper scaffold: [`packaging/npm`](packaging/npm)
- Homebrew formula scaffold: [`packaging/homebrew/sxmc.rb`](packaging/homebrew/sxmc.rb)

Or build from source:

```bash
git clone https://github.com/aihxp/sxmc.git
cd sxmc
cargo build --release
# Binary at target/release/sxmc
```

Additional setup and client-specific configuration examples are in
[`docs/CLIENTS.md`](docs/CLIENTS.md). Release and publishing steps are in
[`docs/RELEASING.md`](docs/RELEASING.md). Distribution-channel notes are in
[`docs/DISTRIBUTION.md`](docs/DISTRIBUTION.md), smoke checks are in
[`docs/SMOKE_TESTS.md`](docs/SMOKE_TESTS.md), and launch copy is in
[`docs/LAUNCH.md`](docs/LAUNCH.md).

## Quick Start

### Serve skills as an MCP server

```bash
# stdio (for MCP client configs)
sxmc serve

# Streamable HTTP MCP endpoint at http://127.0.0.1:8000/mcp
sxmc serve --transport http --host 127.0.0.1 --port 8000

# Require auth headers for remote MCP access
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --require-header "Authorization: env:SXMC_MCP_TOKEN"

# Or use Bearer token auth plus a health endpoint
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --bearer-token env:SXMC_MCP_TOKEN
curl http://127.0.0.1:8000/healthz
sxmc http http://127.0.0.1:8000/mcp \
  --auth-header "Authorization: Bearer $SXMC_MCP_TOKEN" --list
```

Add to any MCP client config:

```json
{ "mcpServers": { "skills": { "command": "sxmc", "args": ["serve"] } } }
```

### Run a skill directly

```bash
sxmc skills list
sxmc skills run pr-review 42
```

Example output of `sxmc skills list`:

```
simple-skill
  A simple test skill

skill-with-references
  A skill with reference documents
  Resources: style-guide.md

skill-with-scripts
  A skill with executable scripts
  Tools: hello.sh
```

When served over MCP, each skill is exposed in a hybrid form:
- the skill body as an MCP prompt
- `scripts/` as MCP tools
- `references/` as MCP resources
- generic retrieval tools for listing skills, reading skill details, and reading files

This lets `sxmc` work well with local stdio-based MCP clients such as Codex,
Cursor, Gemini CLI, and similar coding agents.
It can also be hosted as a remote streamable HTTP MCP server for clients that
consume HTTP MCP endpoints.

### Any MCP server as CLI

```bash
# stdio server
sxmc stdio "npx @mcp/github" --list
sxmc stdio "npx @mcp/github" search-repos query=rust

# HTTP server
sxmc http https://mcp.example.com/mcp --list
sxmc http https://mcp.example.com/mcp my-tool key=value
```

That means skills can flow through both stages in one go:

```bash
# Serve local skills over MCP, then bridge that MCP server back into CLI
sxmc stdio "sxmc serve --paths tests/fixtures" --list
sxmc stdio "sxmc serve --paths tests/fixtures" get_available_skills --pretty
sxmc stdio "sxmc serve --paths tests/fixtures" get_skill_details name=simple-skill --pretty
sxmc stdio "sxmc serve --paths tests/fixtures" get_skill_related_file \
  skill_name=skill-with-references \
  relative_path=references/style-guide.md
```

For hosted `/mcp` endpoints, prefer `--require-header` so remote access is not
left open by default. For single-token hosted deployments, `--bearer-token` is
usually the friendlier option.

### Any API as CLI

```bash
# Auto-detect (OpenAPI or GraphQL)
sxmc api https://petstore.swagger.io/v3/openapi.json --list
sxmc api https://petstore.swagger.io/v3/openapi.json listPets limit=10

# Explicit modes
sxmc spec ./openapi.yaml listPets limit=10
sxmc graphql https://api.example.com/graphql users limit=5
```

Protected endpoints can use `--auth-header`, and header values support
`env:VAR_NAME` and `file:/path/to/secret` forms for secret resolution.

### Security scanning

```bash
sxmc scan                                     # scan all skills
sxmc scan --skill my-skill                    # scan one skill
sxmc scan --severity critical                 # filter by severity
sxmc scan --json                              # JSON output
```

Example output:

```
[SCAN] skill:malicious-skill — 7 issue(s) found
  [CRITICAL] SL-INJ-001 (Prompt injection detected): Line contains prompt injection pattern
  [CRITICAL] SL-SEC-001 (Potential secret exposed): Line may contain a hardcoded secret
  [ERROR]    SL-HIDE-001 (Hidden Unicode characters): Found 1 'zero-width space' character(s)
  [ERROR]    SL-EXEC-001 (Dangerous script operation): Line contains potentially dangerous operation
  [WARN]     SL-PERM-001 (Wildcard tool permission): Skill requests wildcard tool access '*'
[PASS] skill:simple-skill — no issues at severity >= info
[PASS] skill:other-skill — no issues at severity >= info
```

### Bake and reuse connections

```bash
sxmc bake create pets --type spec --source https://petstore.swagger.io/v3/openapi.json
sxmc bake list
sxmc bake show pets
```

### Generate skills from APIs

```bash
sxmc skills create https://api.example.com/openapi.json
# Creates a SKILL.md with all operations documented
```

## Skills

Skills are directories containing a `SKILL.md` file with YAML frontmatter and a markdown body. They can optionally include `scripts/` (executable tools) and `references/` (context resources).

```
my-skill/
  SKILL.md          # Required: frontmatter + instructions
  scripts/           # Optional: become MCP tools
    deploy.sh
  references/        # Optional: become MCP resources
    style-guide.md
```

### SKILL.md format

```markdown
---
name: my-skill
description: "What this skill does"
argument-hint: "<repo> [--dry-run]"
allowed-tools:
  - Bash
  - Read
---

Instructions for the AI when this skill is invoked.

Use $ARGUMENTS for user-provided arguments.
```

### Skill discovery

Skills are discovered from (in priority order):
1. `--paths` flag (explicit)
2. `.claude/skills/` (project-local)
3. `~/.claude/skills/` (user-global)

## Security Scanning

sxmc includes a native Rust security scanner that analyzes skills and MCP servers for threats. Scans are available through the `scan` command for skills and MCP servers.

### What it detects

**Skill scanning:**
- Prompt injection patterns (ignore instructions, role switching, jailbreak attempts)
- Hidden Unicode characters (zero-width spaces, RTL overrides, homoglyphs)
- Hardcoded secrets (AWS keys, GitHub tokens, API keys, passwords)
- Dangerous script operations (rm -rf, chmod 777, eval, curl|bash)
- Data exfiltration patterns (webhook posts, DNS exfil)
- Overly broad tool permissions (wildcard `*`, dangerous tool names)

**MCP server scanning:**
- Tool shadowing (servers overriding trusted tools)
- Prompt injection in tool descriptions
- Excessive permission requests
- Overly permissive input schemas

### Severity levels

| Level | Meaning |
|-------|---------|
| `info` | Informational, no action needed |
| `warning` | Potential issue, review recommended |
| `error` | Likely security problem |
| `critical` | Definite threat, blocks execution |

## Architecture

```
sxmc
├── Security Layer
│   ├── Skill Scanner — prompt injection, secrets, hidden chars
│   └── MCP Scanner  — tool shadowing, response injection
├── Scan Command
│   └── Explicit security analysis for skills and MCP servers
├── Server Side
│   └── Discovery → Parser → MCP Server (rmcp)
└── Client Side
    ├── MCP Client — stdio & HTTP transports
    ├── OpenAPI    — spec parsing + HTTP execution
    ├── GraphQL    — introspection + query building
    ├── Bake       — saved connection configs
    └── Cache      — file-based with TTL
```

Built on [rmcp](https://github.com/nicepkg/rmcp) (official Rust MCP SDK).

## Client Compatibility

`sxmc` currently targets local stdio MCP clients first, and also supports a
remote streamable HTTP MCP endpoint at `/mcp`.

- Supported now: Codex, Cursor, Gemini CLI, Claude Code-style local MCP clients
- Supported now for remote MCP consumers too: streamable HTTP MCP at `/mcp`
- Recommended for hosted remote MCP: `--bearer-token env:SXMC_MCP_TOKEN`
- Health endpoint for hosted deployments: `/healthz`

See [`docs/CLIENTS.md`](docs/CLIENTS.md) for setup examples.

## CLI Reference

```
sxmc [subcommand] [options]

SERVER:
  serve [--paths ...] [--transport stdio|http|sse] [--host 127.0.0.1] [--port 8000] [--require-header K:V] [--bearer-token TOKEN]

SKILLS:
  skills list [--paths ...] [--json]
  skills info <name> [--paths ...]
  skills run <name> [args...] [--paths ...]
  skills create <api-url> [--output-dir DIR] [--auth-header K:V]

CLIENT:
  stdio <command> [tool] [args...] [--list] [--search] [--pretty] [--env K=V]
  http <url> [tool] [args...] [--list] [--search] [--pretty] [--auth-header K:V]
  api <source> [operation] [args...] [--list] [--auth-header K:V]
  spec <source> [operation] [args...] [--list] [--auth-header K:V]
  graphql <url> [operation] [args...] [--list] [--auth-header K:V]

SECURITY:
  scan [--paths ...] [--skill <name>] [--severity warn|error|critical] [--json]

BAKE:
  bake create <name> --type <stdio|http|api|spec|graphql> --source <src> [--description ...]
  bake list
  bake show <name>
  bake update <name> [--type ...] [--source ...] [--description ...]
  bake remove <name>
```

Hybrid skill retrieval tools exposed by `serve`:
- `get_available_skills`
- `get_skill_details`
- `get_skill_related_file`

## Development

```bash
# Run tests
cargo test

# Build
cargo build --release

# Run directly
cargo run -- skills list --paths tests/fixtures
cargo run -- scan --paths tests/fixtures
bash scripts/smoke_test_clients.sh target/debug/sxmc tests/fixtures
```

## Acknowledgements

sxmc was inspired by and builds upon ideas from:

- [mcp2cli](https://github.com/knowsuchagency/mcp2cli) — the Python MCP-to-CLI bridge that sxmc reimplements in Rust with skills as a first-class concept
- [skill-to-mcp](https://github.com/biocontext-ai/skill-to-mcp) — an early skills-to-MCP adapter that helped validate the value of exposing skill collections through MCP
- [claude-skill-antivirus](https://github.com/claude-world/claude-skill-antivirus) — skill security scanning patterns
- [skillfile](https://github.com/eljulians/skillfile) — declarative skill manifest concepts
- [Mcpwn](https://github.com/Teycir/Mcpwn) — MCP server security analysis techniques
- [rmcp](https://github.com/nicepkg/rmcp) — the official Rust MCP SDK powering the protocol layer

## License

MIT
