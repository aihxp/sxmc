# Product Contract

This document defines what `sxmc` claims to support today, what should fail
gracefully, and what is intentionally outside the contract.

Use this document together with:

- [COMPATIBILITY_MATRIX.md](COMPATIBILITY_MATRIX.md) for named client/server validation
- [SMOKE_TESTS.md](SMOKE_TESTS.md) for repeatable startup and transport checks
- [MCP_TO_CLI_VERIFICATION.md](MCP_TO_CLI_VERIFICATION.md) for bridge-specific evidence

## Supported And Expected To Work

These are the core product paths we should treat as stable:

### 1. Skills -> MCP

- `sxmc serve` loads skill directories and exposes them over MCP
- per-skill prompts are available when `SKILL.md` is present
- `scripts/` entries become MCP tools
- `references/` entries become MCP resources
- hybrid retrieval tools are always available:
  - `get_available_skills`
  - `get_skill_details`
  - `get_skill_related_file`

### 2. MCP -> CLI

- `sxmc stdio` can discover and invoke tools, prompts, and resources from a stdio MCP server
- `sxmc http` can discover and invoke tools, prompts, and resources from a streamable HTTP MCP server
- `sxmc mcp` can discover and invoke tools, prompts, and resources from baked stdio/http MCP connections
- `--list`, `--list-tools`, `--list-prompts`, `--list-resources`, `--describe`, and `--describe-tool` are supported CLI surfaces
- one-shot tool execution is supported
- one-shot prompt fetches with `--prompt` are supported
- one-shot resource reads with `--resource` are supported
- baked `server/tool` workflows are supported through `mcp servers|grep|tools|info|call|prompt|read`

### 3. API -> CLI

- `sxmc api` auto-detects OpenAPI vs GraphQL
- `sxmc spec` supports direct OpenAPI execution
- `sxmc graphql` supports GraphQL schema-driven invocation

### 4. Hosting And Auth

- local stdio MCP hosting is supported
- remote streamable HTTP MCP hosting at `/mcp` is supported
- `/healthz` is supported for hosted deployments
- bearer-token and required-header auth are supported for remote MCP hosting

## Should Fail Gracefully

These scenarios should not crash the product or produce misleading results:

- promptless/resource-less MCP servers should still allow tool discovery and one-shot tool calls
- zero-argument MCP tools should receive `{}` rather than an omitted argument object
- startup-only invocations like `sxmc --version` and `sxmc --help` should succeed on all supported platforms
- unsupported optional MCP surfaces should be skipped with a clear note rather than failing all discovery
- `scan` should continue to use non-zero exit status for findings by design, but not be treated as a crash

## Explicitly Outside The Contract

These are not promised as current product behavior:

- persistent multi-turn MCP sessions through repeated `sxmc stdio ...` invocations
- stateful "dialog" continuity across separate CLI invocations
- automated CI launch of proprietary clients like Cursor, Codex, or Claude Code
- universal compatibility with every third-party MCP server without caveats
- benchmark numbers as proof of broad client compatibility

## Release Bar

Before a release, we should be able to point to:

1. a passing local certification run
2. a current compatibility matrix
3. a current benchmark snapshot
4. a documented support boundary for anything still out of scope

If a behavior is not covered by one of those, it should not be described as a
guaranteed product path.
