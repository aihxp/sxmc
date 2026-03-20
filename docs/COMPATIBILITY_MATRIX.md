# Compatibility Matrix

This file is the maintained compatibility ledger for `sxmc`.

Use it to record what was verified, against which release, and on what date.
It complements the transport-level smoke script in [`SMOKE_TESTS.md`](SMOKE_TESTS.md).
It is not a performance benchmark; keep timing interpretation in
[`VALUE_AND_BENCHMARK_FINDINGS.md`](VALUE_AND_BENCHMARK_FINDINGS.md).

## Current Validation Snapshot

Validated against: `sxmc 0.1.7`  
Validation date: `2026-03-20`

| Client / consumer | Local stdio | Remote HTTP MCP | Validation method | Status | Notes |
|-------------------|-------------|-----------------|-------------------|--------|-------|
| Codex | Yes | Yes | config example + transport smoke checks | Validated | Best fit for local dev and shell-heavy workflows |
| Cursor | Yes | Yes | config example + transport smoke checks | Validated | Prompt/resource and tool discovery supported |
| Gemini CLI | Yes | Yes | config example + transport smoke checks | Validated | `gemini mcp add` examples are maintained |
| Claude Code | Yes | Yes | config example + transport smoke checks | Validated | Use stdio locally; use `/mcp` for remote hosting |
| Generic hosted HTTP MCP consumer | No | Yes | `/mcp` + `/healthz` + bearer/header smoke checks | Validated | Stand-in for remote MCP UIs and connector-style consumers |

## Named MCP Server Snapshot

These rows capture real external MCP server validation separate from proprietary
client setup examples.

| Server | Tool list | Prompts/resources | Zero-arg tool call | Stateful multi-step via repeated CLI calls | Status | Notes |
|--------|-----------|-------------------|--------------------|--------------------------------------------|--------|-------|
| `@modelcontextprotocol/server-everything` | Yes | Yes | N/A | N/A | Validated | Best known-good demo server for full surfaces |
| `@modelcontextprotocol/server-memory` | Yes | Skipped when not advertised | Yes | No | Validated | Promptless server; repeated CLI calls start fresh state |
| `@modelcontextprotocol/server-filesystem /tmp` | Yes | Skipped when not advertised | Yes | No | Validated | `list_allowed_directories` is the key zero-arg check |
| `@modelcontextprotocol/server-sequential-thinking` | Yes | Skipped when not advertised | N/A | No | Validated | One-shot tool calls work; thought history does not persist across processes |
| `@modelcontextprotocol/server-github` | Yes | Skipped when not advertised | N/A | N/A | Validated | `--list` works without `GITHUB_TOKEN` for metadata discovery |

## What "Validated" Means Here

For a client row to remain marked as validated:

1. The release must pass the automated smoke script:

```bash
cargo build
bash scripts/startup_smoke.sh target/debug/sxmc
bash scripts/smoke_test_clients.sh target/debug/sxmc tests/fixtures
```

2. The client setup example in [`CLIENTS.md`](CLIENTS.md) must still match the
   current public CLI and MCP surface.

3. At least one manual happy-path check should be performed for the client:
   - register `sxmc serve`
   - confirm tools/prompts/resources are visible
   - confirm one tool call or prompt/resource fetch succeeds

## Release Checklist For This Matrix

For each release:

1. Install the released crate:

```bash
cargo install sxmc --force
```

2. Re-run the smoke script.

3. Re-check the client snippets in:
   - [`CLIENTS.md`](CLIENTS.md)
   - [`../examples/clients/codex-mcp.toml`](../examples/clients/codex-mcp.toml)
   - [`../examples/clients/cursor-mcp.json`](../examples/clients/cursor-mcp.json)
   - [`../examples/clients/gemini-settings.json`](../examples/clients/gemini-settings.json)
   - [`../examples/clients/claude-code-mcp.json`](../examples/clients/claude-code-mcp.json)

4. Update:
   - validated release version
   - validation date
   - status / notes for any regressions or caveats

5. If Node and network access are available, rerun:

```bash
SXMC_CERTIFY_EXTERNAL=1 bash scripts/certify_release.sh target/debug/sxmc tests/fixtures
```

## Known Scope Limits

- This matrix records compatibility for the transport and config patterns that
  real clients rely on. It does not launch every proprietary client in CI.
- Remote HTTP validation is generic by design. It verifies the hosted `/mcp`
  shape that remote-capable clients consume.
- When a client changes its MCP config format, update the example file and this
  matrix together.
- Startup timings and performance claims belong in the benchmark docs, not in
  this compatibility ledger.
