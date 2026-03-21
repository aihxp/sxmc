# sxmc

One Rust binary for turning agent-facing interfaces into practical tools: serve skills over MCP, use MCP servers from the terminal, run APIs as CLIs, and generate startup-ready AI surfaces from existing CLIs with less adapter code, less prompt bloat, and fewer agent turns.

[Crates.io](https://crates.io/crates/sxmc) | [docs.rs](https://docs.rs/sxmc/latest/sxmc/)

## What is sxmc?

[MCP (Model Context Protocol)](https://modelcontextprotocol.io/) is an open standard for connecting AI assistants to external tools and data sources. Today, if you have skills (structured AI instructions), MCP servers, and APIs, each one requires its own adapter, its own client setup, and its own CLI wrapper. There is no single tool that bridges all three.

**sxmc** solves this. One Rust binary to bridge skills, MCP, APIs, and startup-facing AI scaffolds so you can reuse the same capabilities across agents, shells, and hosted MCP clients without building separate wrappers for each surface.
- Turns skill directories into MCP servers (stdio or remote HTTP)
- Makes MCP tools, prompts, and resources usable from the command line
- Auto-generates CLI commands from OpenAPI and GraphQL specs
- Inspects installed CLIs into host-aware profiles, doc blocks, and client config scaffolds
- Scans skills and MCP servers for security threats

Why that matters:
- fewer adapters and one installable tool instead of a pile of one-off bridges
- lower token overhead because MCP discovery and inspection can stay narrow and on demand
- easier reuse of the same workflows across local agents, hosted MCP clients, and terminal automation
- startup discoverability without unsafe overwrites because generated docs are preview-first and managed-block based

```
Skills  -->  MCP Server     (serve skills to any MCP client)
MCP Server  -->  CLI        (list MCP surfaces, invoke MCP tools)
Any API  -->  CLI           (OpenAPI & GraphQL auto-detection)
CLI  -->  AI Surfaces       (profiles, startup docs, and host config scaffolds)
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
  - [Any CLI as AI startup surfaces](#any-cli-as-ai-startup-surfaces)
  - [Security scanning](#security-scanning)
  - [Bake and reuse connections](#bake-and-reuse-connections)
  - [Generate skills from APIs](#generate-skills-from-apis)
- [Skills](#skills)
- [Security Scanning](#security-scanning-1)
- [Architecture](#architecture)
- [Client Compatibility](#client-compatibility)
- [CLI Reference](#cli-reference)
- [Development](#development)
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
- npm wrapper metadata aligned to `0.2.0`: [`packaging/npm`](packaging/npm)
  The wrapper downloads and verifies release binaries during `postinstall`.
- Homebrew formula pinned to the current release tag: [`packaging/homebrew/sxmc.rb`](packaging/homebrew/sxmc.rb)

Or build from source:

```bash
git clone https://github.com/aihxp/sxmc.git
cd sxmc
cargo build --release
# Binary at target/release/sxmc
```

Canonical docs:

- usage and client setup: [`docs/USAGE.md`](docs/USAGE.md)
- hosting, release, and distribution: [`docs/OPERATIONS.md`](docs/OPERATIONS.md)
- testing, smoke checks, and compatibility notes: [`docs/VALIDATION.md`](docs/VALIDATION.md)
- explicit support boundary: [`docs/PRODUCT_CONTRACT.md`](docs/PRODUCT_CONTRACT.md)
- CLI-to-AI model and write policy: [`docs/CLI_SURFACES.md`](docs/CLI_SURFACES.md)

## Quick Start

### Serve skills as an MCP server

```bash
# stdio (for MCP client configs)
sxmc serve

# Streamable HTTP MCP endpoint at http://127.0.0.1:8000/mcp
sxmc serve --transport http --host 127.0.0.1 --port 8000

# Development mode: reload skills when SKILL.md, scripts/, or references/ change
sxmc serve --watch

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
Validation coverage and compatibility notes live in
[`docs/VALIDATION.md`](docs/VALIDATION.md).

### Any MCP server as CLI

```bash
# stdio server
sxmc stdio "npx @mcp/github" --list
sxmc stdio "npx @mcp/github" search-repos query=rust
sxmc stdio "npx @mcp/github" --prompt triage-template
sxmc stdio "npx @mcp/github" --resource "repo://octocat/hello-world/README.md"

# HTTP server
sxmc http https://mcp.example.com/mcp --list
sxmc http https://mcp.example.com/mcp my-tool key=value
sxmc http https://mcp.example.com/mcp --prompt triage-template
sxmc http https://mcp.example.com/mcp --resource "repo://octocat/hello-world/README.md"
```

For day-to-day MCP use, prefer baked connections through `sxmc mcp`. Use
`sxmc stdio` and `sxmc http` as the raw transport layer when you need ad hoc
connections or transport-level debugging.

`sxmc stdio` and `sxmc http` are MCP bridges that can:
- list **tools**, **prompts**, and **resources**
- list one surface at a time with `--list-tools`, `--list-prompts`, or `--list-resources`
- keep discovery output bounded with `--limit N`
- invoke **tools**
- fetch **prompts** with `--prompt`
- read **resources** with `--resource`
- describe the negotiated server surface with `--describe`
- show one tool’s schema/help with `--describe-tool NAME`
- render structured MCP inspection more compactly with `--format toon`

This makes them especially useful for shell automation, CI, debugging, and
inspecting an MCP server outside an IDE or agent UI.
When a server is tool-only and does not implement prompts/resources,
generic `--list` now stays successful and skips unsupported surfaces instead of
failing the whole command.
General server discovery is intentionally summary-oriented now: `--describe`
keeps tool metadata lightweight, and `--describe-tool NAME` is the on-demand
path for full schema detail.

For an even more token-efficient, schema-on-demand workflow, `sxmc` also
supports baked MCP connections through `sxmc mcp ...`. That gives you a
stable `server/tool` interface similar to `mcp-cli`, while keeping full tool
schemas out of the default discovery path.

That means skills can flow through both stages in one go:

```bash
# Serve local skills over MCP, then bridge that MCP server back into CLI
sxmc stdio "sxmc serve --paths tests/fixtures" --list
sxmc stdio "sxmc serve --paths tests/fixtures" --list-tools
sxmc stdio "sxmc serve --paths tests/fixtures" --list-tools --limit 5
sxmc stdio "sxmc serve --paths tests/fixtures" get_available_skills --pretty
sxmc stdio "sxmc serve --paths tests/fixtures" --describe --format toon --limit 10
sxmc stdio "sxmc serve --paths tests/fixtures" --describe-tool get_skill_details
sxmc stdio "sxmc serve --paths tests/fixtures" get_skill_details name=simple-skill --pretty
sxmc stdio "sxmc serve --paths tests/fixtures" --prompt simple-skill arguments=friend
sxmc stdio "sxmc serve --paths tests/fixtures" --resource \
  "skill://skill-with-references/references/style-guide.md"
sxmc stdio "sxmc serve --paths tests/fixtures" get_skill_related_file \
  skill_name=skill-with-references \
  relative_path=references/style-guide.md
```

Hosted MCP servers work the same way over HTTP:

```bash
sxmc http http://127.0.0.1:8000/mcp \
  --auth-header "Authorization: Bearer $SXMC_MCP_TOKEN" \
  --list
sxmc http http://127.0.0.1:8000/mcp \
  --auth-header "Authorization: Bearer $SXMC_MCP_TOKEN" \
  --describe --format toon --limit 10
sxmc http http://127.0.0.1:8000/mcp \
  --auth-header "Authorization: Bearer $SXMC_MCP_TOKEN" \
  --prompt simple-skill arguments=friend
```

More end-to-end examples live in [`docs/USAGE.md`](docs/USAGE.md).

For hosted `/mcp` endpoints, prefer `--require-header` so remote access is not
left open by default. For single-token hosted deployments, `--bearer-token` is
usually the friendlier option.

You can also bake an MCP connection once, then use it through the lighter
`sxmc mcp` workflow:

```bash
sxmc bake create fixture-mcp \
  --type stdio \
  --source '["sxmc","serve","--paths","tests/fixtures"]'

sxmc mcp servers
sxmc mcp grep skill --limit 10
sxmc mcp tools fixture-mcp --limit 10
sxmc mcp info fixture-mcp/get_skill_details --format toon
sxmc mcp call fixture-mcp/get_skill_details \
  '{"name":"simple-skill","return_type":"content"}' --pretty
sxmc mcp prompt fixture-mcp/simple-skill arguments=friend
sxmc mcp read fixture-mcp/skill://skill-with-references/references/style-guide.md
```

For stateful MCP tools, keep one baked connection open with `sxmc mcp session`
instead of re-spawning a fresh one-shot process each time:

```bash
sxmc mcp session fixture-mcp <<'EOF'
tools --limit 5
info get_skill_details --format toon
call get_skill_details '{"name":"simple-skill","return_type":"content"}' --pretty
exit
EOF
```

Agent workflow guidance and hosted deployment notes are in
[`docs/USAGE.md`](docs/USAGE.md) and [`docs/OPERATIONS.md`](docs/OPERATIONS.md).

For `sxmc stdio`, you can now pass either shell-style quoting or a JSON-array
command spec such as `["sxmc","serve","--paths","tests/fixtures"]`. For nested
or project-local servers, `--cwd` gives you an explicit working directory when
you do not want to rely on the caller’s current directory.
For local skill development, `sxmc serve --watch` polls skill files once per
second and reloads the in-memory server when it detects a change.

### Any API as CLI

```bash
# Auto-detect (OpenAPI or GraphQL)
sxmc api https://petstore3.swagger.io/api/v3/openapi.json --list
sxmc api https://petstore3.swagger.io/api/v3/openapi.json findPetsByStatus status=available
sxmc api https://petstore3.swagger.io/api/v3/openapi.json findPetsByStatus status=available --format toon

# Explicit modes
sxmc spec ./openapi.yaml listPets limit=10
sxmc graphql https://api.example.com/graphql users limit=5
```

Protected endpoints can use `--auth-header`, and header values support
`env:VAR_NAME` and `file:/path/to/secret` forms for secret resolution.
For public OpenAPI smoke tests, `findPetsByStatus` on the Petstore v3 endpoint
is a more stable example than `getInventory`.
For structured API responses, `--format json|json-pretty|toon` lets you choose
between compact JSON, pretty JSON, or a Rust-native TOON-style rendering that
compresses repeated keys in tabular data. `--pretty` remains a shorthand for
pretty JSON.

### Any CLI as AI startup surfaces

```bash
# Inspect a real CLI into a normalized profile
sxmc inspect cli gh --format toon

# Generate startup-facing artifacts for one host profile
sxmc init ai --from-cli gh --client claude-code --mode preview
sxmc init ai --from-cli gh --client cursor --mode preview

# Apply a managed block to the real startup-read doc file
sxmc scaffold agent-doc \
  --from-profile examples/profiles/from_cli.json \
  --client claude-code \
  --mode apply

# Merge a known host config shape when sxmc supports it
sxmc scaffold client-config \
  --from-profile examples/profiles/from_cli.json \
  --client cursor \
  --mode apply
```

This shipped slice is intentionally safe:

- `inspect cli` builds the canonical JSON profile for a real command
- `init ai` generates a profile sidecar, an agent-doc block, and a host config scaffold
- `preview` and `write-sidecar` are the default review paths
- `apply` updates managed markdown blocks or mergeable config files only
- existing `AGENTS.md` / `CLAUDE.md` files are never overwritten wholesale

Current host-aware coverage includes:

- Claude Code
- Cursor
- Gemini CLI
- OpenAI/Codex-style setups
- generic stdio/http MCP startup scaffolds

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
- Local development convenience: `sxmc serve --watch`

See [`docs/USAGE.md`](docs/USAGE.md) for setup examples and
[`docs/VALIDATION.md`](docs/VALIDATION.md) for compatibility and smoke checks.

## CLI Reference

```
sxmc [subcommand] [options]

SERVER:
  serve [--paths ...] [--watch] [--transport stdio|http|sse] [--host 127.0.0.1] [--port 8000] [--require-header K:V] [--bearer-token TOKEN]

SKILLS:
  skills list [--paths ...] [--json]
  skills info <name> [--paths ...]
  skills run <name> [args...] [--paths ...]
  skills create <api-url> [--output-dir DIR] [--auth-header K:V]

CLIENT:
  stdio <command> [tool] [args...] [--prompt NAME] [--resource URI] [--list] [--list-tools] [--list-prompts] [--list-resources] [--describe] [--describe-tool NAME] [--search] [--pretty] [--env K=V] [--cwd DIR]
  http <url> [tool] [args...] [--prompt NAME] [--resource URI] [--list] [--list-tools] [--list-prompts] [--list-resources] [--describe] [--describe-tool NAME] [--search] [--pretty] [--auth-header K:V]
  mcp servers
  mcp grep <pattern> [--server NAME] [--limit N]
  mcp tools <server> [--search PATTERN] [--limit N]
  mcp prompts <server> [--limit N]
  mcp resources <server> [--limit N]
  mcp info <server/tool> [--pretty] [--format json|json-pretty|toon]
  mcp call <server/tool> [json-object|-] [--pretty]
  mcp prompt <server/prompt> [key=value...]
  mcp read <server/resource-uri> [--pretty]
  api <source> [operation] [args...] [--list] [--pretty] [--format json|json-pretty|toon] [--auth-header K:V]
  spec <source> [operation] [args...] [--list] [--pretty] [--format json|json-pretty|toon] [--auth-header K:V]
  graphql <url> [operation] [args...] [--list] [--pretty] [--format json|json-pretty|toon] [--auth-header K:V]

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

## License

MIT
