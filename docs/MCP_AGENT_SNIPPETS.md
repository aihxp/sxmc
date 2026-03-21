# MCP agent-doc snippets

These snippets are meant to help agents use `sxmc`'s baked MCP workflow in a
token-efficient way.

Recommended default workflow:

1. `sxmc mcp servers`
2. `sxmc mcp grep <pattern>` or `sxmc mcp tools <server> --limit 10`
3. `sxmc mcp info <server/tool> --format toon`
4. `sxmc mcp call <server/tool> '<json-object>'`

Why this helps:

- discovery stays small
- full schemas are fetched only on demand
- prompts/resources stay lazily fetched
- large JSON output can be piped or redirected instead of copied into context

Available snippets:

- [AGENTS.md snippet](/Users/hprincivil/Projects/sxmc/examples/agent-docs/AGENTS.md.snippet)
- [CLAUDE.md snippet](/Users/hprincivil/Projects/sxmc/examples/agent-docs/CLAUDE.md.snippet)

Suggested usage:

- copy the relevant snippet into your repo guidance file
- adjust server names to match your baked MCP connections
- keep the `info -> call` workflow intact for the best token behavior
