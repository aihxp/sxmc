# Usage Guide

The shortest path through `sxmc` is:

- `serve` to publish skills as MCP
- `mcp` for daily MCP client work against baked connections
- `stdio` and `http` for raw or ad hoc MCP bridging
- `api`, `spec`, and `graphql` for API-to-CLI flows
- `inspect cli`, `init ai`, and `scaffold` for CLI-to-AI startup artifacts

## Install

Install from crates.io:

```bash
cargo install sxmc
```

Or build from source:

```bash
git clone https://github.com/aihxp/sxmc.git
cd sxmc
cargo build --release
```

Prebuilt release archives and checksums are published on GitHub Releases.

## Serve Skills As MCP

Local stdio MCP:

```bash
sxmc serve
sxmc serve --paths /absolute/path/to/skills
```

Local development with reloads:

```bash
sxmc serve --watch
```

Hosted streamable HTTP MCP:

```bash
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --bearer-token env:SXMC_MCP_TOKEN \
  --paths /absolute/path/to/skills
```

## Use MCP From The CLI

Ad hoc stdio bridge:

```bash
sxmc stdio '["sxmc","serve","--paths","tests/fixtures"]' --list
sxmc stdio '["sxmc","serve","--paths","tests/fixtures"]' --prompt simple-skill arguments=friend
sxmc stdio '["sxmc","serve","--paths","tests/fixtures"]' --resource \
  "skill://skill-with-references/references/style-guide.md"
```

Hosted bridge:

```bash
sxmc http http://127.0.0.1:8000/mcp \
  --auth-header "Authorization: Bearer $SXMC_MCP_TOKEN" \
  --describe --format toon --limit 10
```

Baked daily workflow:

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

Stateful MCP workflow:

```bash
sxmc mcp session fixture-mcp <<'EOF'
tools --limit 5
info get_skill_details --format toon
call get_skill_details '{"name":"simple-skill","return_type":"content"}' --pretty
exit
EOF
```

Recommended low-token MCP workflow:

1. `sxmc mcp servers`
2. `sxmc mcp grep <pattern>` or `sxmc mcp tools <server> --limit 10`
3. `sxmc mcp info <server/tool> --format toon`
4. `sxmc mcp call <server/tool> '<json-object>'`
5. use `sxmc mcp session <server>` when the MCP server expects stateful multi-step calls

## Use APIs As CLIs

Auto-detect:

```bash
sxmc api https://petstore3.swagger.io/api/v3/openapi.json --list
sxmc api https://petstore3.swagger.io/api/v3/openapi.json findPetsByStatus status=available
sxmc api https://petstore3.swagger.io/api/v3/openapi.json findPetsByStatus status=available --format toon
```

Explicit OpenAPI / GraphQL:

```bash
sxmc spec ./openapi.yaml listPets limit=10
sxmc graphql https://api.example.com/graphql users limit=5
```

## Turn CLIs Into AI Startup Surfaces

Inspect a real CLI:

```bash
sxmc inspect cli gh --format json-pretty
sxmc inspect cli gh --format toon
```

Generate startup-facing artifacts for a host profile:

```bash
sxmc init ai --from-cli gh --client claude-code --mode preview
sxmc init ai --from-cli gh --client cursor --mode preview
```

Generate from an existing saved profile:

```bash
sxmc scaffold agent-doc \
  --from-profile examples/profiles/from_cli.json \
  --client claude-code \
  --mode preview

sxmc scaffold client-config \
  --from-profile examples/profiles/from_cli.json \
  --client cursor \
  --mode preview
```

Write modes:

- `preview`
  - print generated artifacts to stdout
- `write-sidecar`
  - write sidecar files under `.sxmc/ai/...`
- `patch`
  - show a patch-style preview for apply-capable targets
- `apply`
  - update managed markdown blocks or mergeable config files

Safety rules:

- existing `AGENTS.md` / `CLAUDE.md` files are not overwritten wholesale
- `apply` uses managed `sxmc` blocks for markdown docs
- JSON MCP configs are merged where the host shape is known
- `sxmc` refuses to inspect itself unless you pass `--allow-self`

Current host profiles:

- `claude-code`
- `cursor`
- `gemini-cli`
- `openai-codex`
- `generic-stdio-mcp`
- `generic-http-mcp`

## Client Setup Notes

`sxmc` is designed to work well with:

- Codex
- Cursor
- Gemini CLI
- Claude Code
- generic local stdio MCP clients
- generic remote streamable HTTP MCP consumers

For local client configs, point the client at:

```text
command: sxmc
args: ["serve", "--paths", "/absolute/path/to/skills"]
```

For hosted clients, point them at:

```text
http://HOST:PORT/mcp
```

with bearer auth or required headers enabled on the server.

## Agent Guidance

If you maintain `AGENTS.md`, `CLAUDE.md`, or similar repo guidance, prefer
teaching agents this pattern:

1. search or list first
2. inspect one tool with `sxmc mcp info`
3. call one tool with `sxmc mcp call`
4. keep large output in files or pipes instead of pasting it into context
