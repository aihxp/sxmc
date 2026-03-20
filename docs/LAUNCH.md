# Launch Notes

This doc is the reusable source for release notes, short announcements, and
quick demos.

## One-Line Pitch

`sxmc` is a Rust CLI that turns skills into MCP servers, MCP servers into CLI
commands, and OpenAPI or GraphQL APIs into CLI commands.

## What Makes It Different

- one native binary instead of a pile of adapters
- hybrid `skills -> MCP -> CLI` flow
- local stdio MCP plus remote streamable HTTP MCP
- bearer auth and health checks for hosted `/mcp`
- built-in security scanning for skills and MCP surfaces

## 60-Second Demo

```bash
cargo install sxmc

# Serve local skills over MCP
sxmc serve --paths ./skills

# Or host them remotely
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --bearer-token env:SXMC_MCP_TOKEN \
  --paths ./skills

# Bridge the same MCP server back into CLI
sxmc stdio "sxmc serve --paths ./skills" --list

# Inspect a remote MCP server
sxmc http http://127.0.0.1:8000/mcp \
  --auth-header "Authorization: Bearer $SXMC_MCP_TOKEN" \
  --list
```

## Release Notes Template

### Highlights

- local `stdio` MCP serving for skill directories
- remote `/mcp` serving with bearer auth and `/healthz`
- hybrid skill retrieval tools for broad client compatibility
- `sxmc stdio` and `sxmc http` bridges for MCP-to-CLI use cases
- OpenAPI and GraphQL API-to-CLI support
- built-in security scanning

### Patch Release Notes (`0.1.4`)

- adds `sxmc serve --watch` so local skill edits reload automatically
- fixes Windows stdio command parsing and removes redundant skill discovery
- adds reproducible benchmark notes and carries forward benchmark interpretation guidance
- keeps the broader MCP bridge, TOON output, and packaging channels aligned in the patch line

### Install

```bash
cargo install sxmc
```

Docs:

- `README.md`
- `docs/CLIENTS.md`
- `docs/SMOKE_TESTS.md`
- `docs/DISTRIBUTION.md`

## Short Announcement Copy

### X / short post

Built `sxmc`: a Rust CLI that turns skills into MCP servers, MCP servers into
CLI commands, and OpenAPI/GraphQL APIs into CLI commands.

It supports local stdio MCP, remote `/mcp`, bearer auth, health checks, and
security scanning.

Install:

```bash
cargo install sxmc
```

### Longer post

`sxmc` is now live.

It started from a simple idea: skills, MCP, and CLI adapters should not need to
be separate tools. `sxmc` combines those flows into one Rust binary:

- skills -> MCP
- MCP -> CLI
- API -> CLI

It supports local stdio MCP clients like Codex, Cursor, Gemini CLI, and Claude
Code-style setups, and it can also host a remote streamable HTTP MCP endpoint
at `/mcp` with bearer-token protection and a `/healthz` endpoint.
