# Compatibility Matrix

This file is the maintained compatibility ledger for `sxmc`.

Use it to record what was verified, against which release, and on what date.
It complements the transport-level smoke script in [`SMOKE_TESTS.md`](SMOKE_TESTS.md).

## Current Validation Snapshot

Validated against: `sxmc 0.1.4`  
Validation date: `2026-03-20`

| Client / consumer | Local stdio | Remote HTTP MCP | Validation method | Status | Notes |
|-------------------|-------------|-----------------|-------------------|--------|-------|
| Codex | Yes | Yes | config example + transport smoke checks | Validated | Best fit for local dev and shell-heavy workflows |
| Cursor | Yes | Yes | config example + transport smoke checks | Validated | Prompt/resource and tool discovery supported |
| Gemini CLI | Yes | Yes | config example + transport smoke checks | Validated | `gemini mcp add` examples are maintained |
| Claude Code | Yes | Yes | config example + transport smoke checks | Validated | Use stdio locally; use `/mcp` for remote hosting |
| Generic hosted HTTP MCP consumer | No | Yes | `/mcp` + `/healthz` + bearer/header smoke checks | Validated | Stand-in for remote MCP UIs and connector-style consumers |

## What "Validated" Means Here

For a client row to remain marked as validated:

1. The release must pass the automated smoke script:

```bash
cargo build
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

## Known Scope Limits

- This matrix records compatibility for the transport and config patterns that
  real clients rely on. It does not launch every proprietary client in CI.
- Remote HTTP validation is generic by design. It verifies the hosted `/mcp`
  shape that remote-capable clients consume.
- When a client changes its MCP config format, update the example file and this
  matrix together.
