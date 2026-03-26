# Compatibility Matrix

This matrix summarizes the maintained `1.x` validation lanes for Sumac
(`sxmc`).

## Core Product Paths

| Path | Linux | macOS | Windows | Notes |
|---|---|---|---|---|
| `cargo test` | yes | yes | yes | Core Rust regression suite |
| `scripts/smoke_portable_core.sh` | yes | yes | yes | Stable onboarding/discovery-delivery smoke |
| `scripts/smoke_portable_fixtures.sh` | yes | yes | yes | Local fixture MCP smoke for stdio, baked MCP, HTTP, bearer HTTP |
| `scripts/test-sxmc.sh` | yes | yes | best-effort local | Comprehensive release-sized shell suite; full CI lanes on Unix |
| Windows PowerShell JSON smoke | n/a | n/a | yes | Explicit `doctor` / inspection / cache JSON checks |

## Optional External Coverage

| Path | Requires network | Requires Node | Purpose |
|---|---:|---:|---|
| `scripts/smoke_real_world_mcps.sh` | yes | yes | Real-world MCP interoperability sanity |
| `sxmc api <url>` live calls | yes | no | OpenAPI/GraphQL live surface checks |

## Local Fixture Contract

Portable fixture coverage is centered on [tests/fixtures/README.md](/Users/hprincivil/Projects/sxmc/tests/fixtures/README.md):

- `simple-skill`
- `malicious-skill`
- `skill-with-references`
- `skill-with-scripts`
- `stateful_mcp_server.py`

Use [scripts/fixture_inventory.py](/Users/hprincivil/Projects/sxmc/scripts/fixture_inventory.py) when you want a generated inventory of the shipped validation fixtures.

## CLI -> AI Host Coverage

This matrix tracks the maintained `CLI -> AI` host coverage in `sxmc`. Host
conventions in this table were last verified on **2026-03-21** against the
official references listed below.

| Host | Native startup doc target | Native config target | `init ai --coverage full` | `apply` behavior | Reference |
|------|----------------------------|----------------------|---------------------------|------------------|-----------|
| Claude Code | `CLAUDE.md` | `.sxmc/ai/claude-code-mcp.json` | Yes | applies selected host, otherwise sidecar | [Anthropic docs](https://docs.anthropic.com/en/docs/claude-code/memory) |
| Cursor | `.cursor/rules/sxmc-cli-ai.md` | `.cursor/mcp.json` | Yes | merges JSON config and managed rule doc | [Cursor docs](https://docs.cursor.com/context/rules-for-ai) |
| Gemini CLI | `GEMINI.md` | `.gemini/settings.json` | Yes | merges JSON config and managed doc | [Gemini CLI docs](https://geminicli.com/docs/cli/gemini-md/) |
| GitHub Copilot | `.github/copilot-instructions.md` | none | Yes | native instructions file only | [GitHub docs](https://docs.github.com/en/copilot/tutorials/customization-library/custom-instructions/your-first-custom-instructions) |
| Continue | `.continue/rules/sxmc-cli-ai.md` | none | Yes | native rules doc only | [Continue docs](https://docs.continue.dev/customize/rules) |
| OpenCode | `AGENTS.md` portable fallback | `opencode.json` | Yes | merges JSON config in native OpenCode shape | [OpenCode docs](https://opencode.ai/docs/rules) |
| JetBrains AI Assistant | `.aiassistant/rules/sxmc-cli-ai.md` | none | Yes | native rules doc only | [JetBrains AI Assistant docs](https://www.jetbrains.com/help/ai-assistant/configure-project-rules.html) |
| Junie | `.junie/guidelines.md` | none | Yes | native guidelines doc only | [Junie docs](https://www.jetbrains.com/help/junie/customize-guidelines.html) |
| Windsurf | `.windsurf/rules/sxmc-cli-ai.md` | none | Yes | native rules doc only | [Windsurf docs](https://docs.windsurf.com/windsurf/cascade/memories) |
| OpenAI/Codex | `AGENTS.md` portable fallback | `.codex/mcp.toml` | Yes | managed TOML block for config | [Codex docs](https://developers.openai.com/codex/cli/) |
| Generic stdio MCP | `AGENTS.md` portable fallback | `.sxmc/ai/generic-stdio-mcp.json` | Yes | sidecar config only | [MCP architecture](https://modelcontextprotocol.io/docs/learn/architecture) |
| Generic HTTP MCP | `AGENTS.md` portable fallback | `.sxmc/ai/generic-http-mcp.json` | Yes | sidecar config only | [MCP architecture](https://modelcontextprotocol.io/docs/learn/architecture) |

### Host Coverage Notes

- `AGENTS.md` is the portable baseline, not the only target.
- `setup`, `add`, `init ai`, `doctor`, `status`, `sync`, and `watch` support both project-local writes (default / `--local`) and user-level host installs (`--global`).
- Full coverage is safest in `preview` or `write-sidecar` mode.
- Full-coverage `apply` requires explicit `--host` selection.
- Non-selected hosts remain sidecars during `apply`.
- `llms.txt` is available as an optional export via:

```bash
sxmc scaffold llms-txt --from-profile examples/profiles/from_cli.json --mode preview
```

Current automated host coverage includes:

- `inspect cli`
- full-coverage preview
- full-coverage apply with host selection
- native Claude, Cursor, Gemini, and GitHub Copilot doc generation
- Cursor config merge
- OpenAI/Codex TOML config insertion
- optional `llms.txt` export
