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
sxmc init ai --from-cli gh --coverage full --mode preview
sxmc init ai --from-cli gh --coverage full --host claude-code,cursor --mode apply
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

sxmc scaffold skill \
  --from-profile examples/profiles/from_cli.json \
  --mode preview

sxmc scaffold mcp-wrapper \
  --from-profile examples/profiles/from_cli.json \
  --mode preview

sxmc scaffold llms-txt \
  --from-profile examples/profiles/from_cli.json \
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
- skill and MCP-wrapper scaffolds write new files rather than mutating existing docs
- `--coverage full` is the best way to generate broad startup coverage without committing to every host at once
- `--coverage full --mode apply` requires one or more `--host` values and sidecars the non-selected hosts

Current host profiles:

- `claude-code`
- `cursor`
- `gemini-cli`
- `github-copilot`
- `continue-dev`
- `junie`
- `windsurf`
- `openai-codex`
- `generic-stdio-mcp`
- `generic-http-mcp`

Full-coverage generation produces:

- a portable `AGENTS.md` block
- `CLAUDE.md` for Claude Code
- `.cursor/rules/sxmc-cli-ai.md` for Cursor
- `GEMINI.md` for Gemini CLI
- `.github/copilot-instructions.md` for GitHub Copilot
- `.continue/rules/sxmc-cli-ai.md` for Continue
- `.junie/guidelines.md` for Junie
- `.windsurf/rules/sxmc-cli-ai.md` for Windsurf
- host config scaffolds for Claude, Cursor, Gemini, OpenAI/Codex, and generic stdio/http MCP

Notes:

- GitHub Copilot gets a native instructions file, not an MCP config scaffold
- Continue, Junie, and Windsurf are native doc targets today, not MCP config targets
- `llms.txt` is optional and exported separately through `scaffold llms-txt`

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
