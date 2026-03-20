# Client Smoke Tests

These checks are meant to catch cross-client regressions before a release.
They do not launch Codex, Cursor, Gemini CLI, or Claude Code directly. Instead,
they validate the transport patterns those clients rely on:

- local stdio MCP
- remote streamable HTTP MCP
- bearer-protected remote MCP

For the release-by-release validation ledger, pair this file with
[`COMPATIBILITY_MATRIX.md`](COMPATIBILITY_MATRIX.md).
For the explicit support boundary, pair it with
[`PRODUCT_CONTRACT.md`](PRODUCT_CONTRACT.md).

These checks are intentionally separate from the Linux timing harness in
[`VALUE_AND_BENCHMARK_FINDINGS.md`](VALUE_AND_BENCHMARK_FINDINGS.md). Smoke
tests answer "does it start and work," while benchmarks answer "how long did
this machine take."

## Startup Sanity

Before running the broader client smoke checks, confirm the binary starts
cleanly:

```bash
bash scripts/startup_smoke.sh target/debug/sxmc
```

For startup timing rather than pass/fail sanity, use:

```bash
python3 scripts/benchmark_startup.py /tmp/sxmc-startup-benchmark.md
```

CI runs `--version` and `--help` on every OS before the full test suite so
startup failures are surfaced earlier than transport-level failures.

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

## Optional Real-World MCP Smoke

When Node and network access are available, run:

```bash
bash scripts/smoke_real_world_mcps.sh target/debug/sxmc
```

That script exercises named official MCP servers that showed up in the
real-world validation notes:

- `@modelcontextprotocol/server-everything`
- `@modelcontextprotocol/server-memory`
- `@modelcontextprotocol/server-filesystem`
- `@modelcontextprotocol/server-sequential-thinking`
- `@modelcontextprotocol/server-github`

It also covers the zero-argument tool-call interoperability fix by invoking
strict servers without the old manual `_={}` workaround.

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

1. Run the startup sanity check.
2. Run the automated smoke script.
3. Optionally run the real-world MCP smoke script if Node/network are available.
4. Re-check the client config examples under [`../examples/clients`](../examples/clients).
5. Update [`COMPATIBILITY_MATRIX.md`](COMPATIBILITY_MATRIX.md) with the release
   version and validation date.
