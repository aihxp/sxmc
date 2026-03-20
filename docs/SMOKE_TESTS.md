# Client Smoke Tests

These checks are meant to catch cross-client regressions before a release.
They do not launch Codex, Cursor, Gemini CLI, or Claude Code directly. Instead,
they validate the transport patterns those clients rely on:

- local stdio MCP
- remote streamable HTTP MCP
- bearer-protected remote MCP

For the release-by-release validation ledger, pair this file with
[`COMPATIBILITY_MATRIX.md`](COMPATIBILITY_MATRIX.md).

## Automated Smoke Script

Run from the repo root:

```bash
cargo build
bash scripts/smoke_test_clients.sh target/debug/sxmc tests/fixtures
```

That script verifies:

- `sxmc serve` over stdio can be bridged back through `sxmc stdio`
- `sxmc serve --transport http` is reachable at `/mcp`
- `sxmc serve --bearer-token ...` works with `sxmc http --auth-header ...`
- `/healthz` responds for remote deployments

## Manual Client Checks

### Codex

```bash
codex mcp add sxmc -- sxmc serve --paths /absolute/path/to/skills
codex mcp list
```

### Cursor

Point `mcp.json` at either:

- `command: "sxmc"` with `args: ["serve", "--paths", "..."]`
- `url: "http://HOST:PORT/mcp"` for remote MCP

### Gemini CLI

```bash
gemini mcp add sxmc sxmc serve --paths /absolute/path/to/skills
gemini mcp add sxmc-remote http://127.0.0.1:8000/mcp --transport http
gemini mcp list
```

### Claude Code

Use a local command definition:

```text
command: sxmc
args: ["serve", "--paths", "/absolute/path/to/skills"]
```

Or host a remote server:

```bash
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --bearer-token env:SXMC_MCP_TOKEN \
  --paths /absolute/path/to/skills
```

## Maintenance Pattern

After each release:

1. Run the automated smoke script.
2. Re-check the client config examples under [`../examples/clients`](../examples/clients).
3. Update [`COMPATIBILITY_MATRIX.md`](COMPATIBILITY_MATRIX.md) with the release
   version and validation date.
