# MCP → CLI verification notes

This document records **manual verification** (2026-03) that **sxmc** can act
as an MCP **client** and expose remote or stdio MCP servers through the CLI
bridge: **`sxmc stdio`**, **`sxmc http`**, and the baked, schema-on-demand
workflow under **`sxmc mcp`**.

## What “MCP → CLI” means in sxmc today

`sxmc stdio` and `sxmc http` currently provide:

- listing of **tools**, **prompts**, and **resources**
- per-surface discovery with `--list-tools`, `--list-prompts`, and `--list-resources`
- bounded discovery with `--limit N`
- invocation of **tools**
- fetching of **prompts** with `--prompt`
- reading of **resources** with `--resource`
- structured server introspection with `--describe`
- single-tool schema/help inspection with `--describe-tool`
- pretty-printing and shell-friendly inspection of MCP results
- compact structured inspection with `--format toon`
- baked MCP server workflows with `sxmc mcp servers|tools|info|call|prompt|read`
- cross-server search with `sxmc mcp grep`

So the precise contract is:

- **MCP discovery surface:** tools, prompts, resources
- **MCP invocation surface:** tools, prompts, resources
- **Low-token workflow:** baked server names plus on-demand `info` before `call`

For the explicit product boundary, including what is not yet supported by the
CLI bridge, see [PRODUCT_CONTRACT.md](PRODUCT_CONTRACT.md).

## Summary

| Transport | Discovery | Invocation | Status | Evidence |
|-----------|-----------|------------|--------|----------|
| **stdio** | tools, prompts, resources | tools, prompts, resources | **Working** | Live runs + `tests/cli_integration.rs` (`test_stdio_*`) |
| **HTTP** (streamable MCP) | tools, prompts, resources | tools, prompts, resources | **Working** | Live run against local `sxmc serve --transport http` + `test_http_*` in same file |
| **baked MCP** (`sxmc mcp`) | named servers, tools, prompts, resources | tools, prompts, resources | **Working** | `test_mcp_*` in `tests/cli_integration.rs` |

## Best for

- shell automation against MCP tools
- CI checks and scripted workflows
- debugging MCP servers outside a full agent/IDE
- inspecting the available tool/prompt/resource surface quickly with `--list`
- checking negotiated capabilities and schemas with `--describe`
- keeping discovery small for model-visible contexts with `--limit` and `--format toon`
- pulling one prompt or resource on demand without loading the whole server into an IDE

## Implementation (source)

- **stdio client:** `src/client/mcp_stdio.rs` — `StdioClient`, rmcp child-process transport.
- **HTTP client:** `src/client/mcp_http.rs` — streamable HTTP MCP.
- **CLI wiring:** `src/main.rs` — subcommands `stdio` and `http`.

## Manual checks performed

For **captured stdout/stderr samples** of the nested **skills → MCP → CLI** path (fixture `tests/fixtures`), see **[SKILLS_TO_MCP_TO_CLI_SAMPLES.md](SKILLS_TO_MCP_TO_CLI_SAMPLES.md)**.

### 1. `sxmc stdio` (nested skills server)

```bash
sxmc stdio "sxmc serve" --list
sxmc stdio "sxmc serve" --list-tools
sxmc stdio "sxmc serve" --list-tools --limit 5
sxmc stdio "sxmc serve" --describe --format toon --limit 10
sxmc stdio "sxmc serve" --describe-tool get_skill_details
```

Expected: hybrid tools (`get_available_skills`, `get_skill_details`, `get_skill_related_file`) plus per-skill script tools when skills are discovered.

```bash
sxmc stdio "sxmc serve --paths /path/to/sxmc/tests/fixtures" get_available_skills --pretty
```

Expected: JSON listing fixture skills.

```bash
sxmc stdio "sxmc serve --paths /path/to/sxmc/tests/fixtures" skill_with_scripts__hello args=test
```

Expected: tool execution succeeds and returns script output.

```bash
sxmc stdio "sxmc serve --paths /path/to/sxmc/tests/fixtures" --prompt simple-skill arguments=friend
sxmc stdio "sxmc serve --paths /path/to/sxmc/tests/fixtures" --resource \
  "skill://skill-with-references/references/style-guide.md"
```

Expected: prompt text and reference content are returned directly to stdout.

### 2. `sxmc http` (local HTTP MCP server)

In one shell:

```bash
sxmc serve --transport http --host 127.0.0.1 --port 8765 --paths /path/to/sxmc/tests/fixtures
```

In another:

```bash
sxmc http http://127.0.0.1:8765/mcp --list
sxmc http http://127.0.0.1:8765/mcp --describe --format toon --limit 10
```

Expected: same tool list as stdio serve for the same `--paths` (prompts/resources included).

`--describe` is intentionally summary-oriented so general server inspection stays
cheap. Use `--describe-tool NAME` only when you need a full schema for a single
tool.

```bash
sxmc http http://127.0.0.1:8765/mcp \
  --auth-header "Authorization: Bearer $SXMC_MCP_TOKEN" \
  --list
```

Expected: hosted/secured streamable HTTP MCP endpoints can be inspected the same
way once auth headers are supplied.

```bash
sxmc http http://127.0.0.1:8765/mcp --prompt simple-skill arguments=friend
sxmc http http://127.0.0.1:8765/mcp --resource \
  "skill://skill-with-references/references/style-guide.md"
```

Expected: prompt/resource retrieval works the same way over remote streamable HTTP MCP.

### 3. `sxmc mcp` (baked, schema-on-demand workflow)

```bash
sxmc bake create fixture-mcp \
  --type stdio \
  --source '["sxmc","serve","--paths","/path/to/sxmc/tests/fixtures"]'

sxmc mcp servers
sxmc mcp grep skill --limit 10
sxmc mcp tools fixture-mcp --limit 10
sxmc mcp info fixture-mcp/get_skill_details --format toon
sxmc mcp call fixture-mcp/get_skill_details \
  '{"name":"simple-skill","return_type":"content"}' --pretty
```

Expected: small discovery output by default, then full schema only when `info`
is explicitly requested for a single tool.

## Caveats

- The bridge only works as well as the **upstream MCP server** (auth, crashes, non-compliant responses).
- `--list` is capability-aware and now skips unsupported prompt/resource surfaces on tool-only servers instead of failing the whole command.
- **Tool names and arguments** must match the server’s schema (same as any MCP client).
- `sxmc stdio` accepts either shell-style quoting or a JSON-array command spec.
  For complex quoting, prefer a JSON array such as `["sxmc","serve","--paths","tests/fixtures"]`
  or a wrapper script.
- `sxmc stdio --cwd /path/to/project ...` is often the safest option for
  project-local servers whose discovery depends on the current directory.

## Common failure modes

- wrong or missing auth headers when using `sxmc http`
- upstream MCP server exits early or returns a non-compliant response
- tool name typo or argument mismatch
- prompt/resource name or URI typo
- over-complicated shell quoting inside the `sxmc stdio "<command>"` string

## Automated regression tests

From the repo root:

```bash
cargo test --test cli_integration
```

Covers stdio bridging, HTTP MCP with auth headers/bearer tokens, hybrid skill tools, and related flows.

## Related docs

- [SMOKE_TESTS.md](SMOKE_TESTS.md) — scripted smoke including stdio ↔ serve loop
- [CLIENTS.md](CLIENTS.md) — example `sxmc http` with `--auth-header`
- [VALUE_AND_BENCHMARK_FINDINGS.md](VALUE_AND_BENCHMARK_FINDINGS.md) — why MCP → CLI is useful
