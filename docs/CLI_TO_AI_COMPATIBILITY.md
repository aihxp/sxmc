# CLI -> AI Compatibility

This matrix tracks the currently shipped `CLI -> AI` host coverage in `sxmc`.

| Host | Native startup doc target | Native config target | `init ai --coverage full` | `apply` behavior | Status |
|------|----------------------------|----------------------|---------------------------|------------------|--------|
| Claude Code | `CLAUDE.md` | `.sxmc/ai/claude-code-mcp.json` | Yes | applies selected host, otherwise sidecar | Supported |
| Cursor | `.cursor/rules/sxmc-cli-ai.md` | `.cursor/mcp.json` | Yes | merges JSON config and managed rule doc | Supported |
| Gemini CLI | `GEMINI.md` | `.gemini/settings.json` | Yes | merges JSON config and managed doc | Supported |
| GitHub Copilot | `.github/copilot-instructions.md` | none | Yes | native instructions file only | Supported |
| Continue | `.continue/rules/sxmc-cli-ai.md` | none | Yes | native rules doc only | Supported |
| Junie | `.junie/guidelines.md` | none | Yes | native guidelines doc only | Supported |
| Windsurf | `.windsurf/rules/sxmc-cli-ai.md` | none | Yes | native rules doc only | Supported |
| OpenAI/Codex | `AGENTS.md` portable fallback | `.codex/mcp.toml` | Yes | managed TOML block for config | Supported |
| Generic stdio MCP | `AGENTS.md` portable fallback | `.sxmc/ai/generic-stdio-mcp.json` | Yes | sidecar config only | Supported |
| Generic HTTP MCP | `AGENTS.md` portable fallback | `.sxmc/ai/generic-http-mcp.json` | Yes | sidecar config only | Supported |

## Notes

- `AGENTS.md` is the portable baseline, not the only target.
- Full coverage is safest in `preview` or `write-sidecar` mode.
- Full-coverage `apply` requires explicit `--host` selection.
- Non-selected hosts remain sidecars during `apply`.
- `llms.txt` is available as an optional export via:

```bash
sxmc scaffold llms-txt --from-profile examples/profiles/from_cli.json --mode preview
```

## Validation Scope

Current automated coverage includes:

- `inspect cli`
- full-coverage preview
- full-coverage apply with host selection
- native Claude, Cursor, Gemini, and GitHub Copilot doc generation
- Cursor config merge
- OpenAI/Codex TOML config insertion
- optional `llms.txt` export
