# Client Setup

`sxmc` is designed first for stdio-based MCP clients, but it can also run as a
remote streamable HTTP MCP server at `/mcp`.

For the dated validation ledger, see
[`COMPATIBILITY_MATRIX.md`](COMPATIBILITY_MATRIX.md).

For remote deployments, prefer:

```bash
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --bearer-token env:SXMC_MCP_TOKEN \
  --paths /absolute/path/to/skills
```

## Support Matrix

| Client | Local stdio MCP | Remote HTTP MCP | Status with `sxmc` |
|--------|------------------|-----------------|--------------------|
| Codex CLI / Codex IDE | Yes | Yes | Supported |
| Cursor | Yes | Yes | Supported |
| Gemini CLI | Yes | Yes | Supported |
| Claude Code and other local coding agents | Yes | Yes | Supported |
| ChatGPT Apps / Claude.ai connectors | No local stdio | Yes | Use the remote `/mcp` endpoint when those products accept remote MCP URLs |

Copy-pasteable config files also live in [`../examples/clients`](../examples/clients).

## Codex

Codex can register local MCP servers directly from the CLI.

```bash
codex mcp add sxmc -- sxmc serve --paths /absolute/path/to/skills
```

Codex can also connect to a remote HTTP MCP server:

```bash
codex mcp add sxmc-remote --url http://127.0.0.1:8000/mcp
```

Codex quick smoke check:

```bash
codex mcp list
```

The equivalent persistent config shape is:

```toml
[mcp_servers.sxmc]
url = "http://127.0.0.1:8000/mcp"
```

If you need environment variables for skill execution, add them when registering:

```bash
codex mcp add sxmc --env FOO=bar -- sxmc serve --paths /absolute/path/to/skills
```

## Cursor

Cursor supports stdio servers through `mcp.json`. You can configure either:
- project-local: `.cursor/mcp.json`
- user/global: the Cursor MCP config location for your installation

Example:

```json
{
  "mcpServers": {
    "sxmc": {
      "type": "stdio",
      "command": "sxmc",
      "args": ["serve", "--paths", "/absolute/path/to/skills"]
    }
  }
}
```

After reloading Cursor, the `sxmc` prompts, tools, resources, and hybrid skill
retrieval tools should appear in the MCP tools UI.

If you host `sxmc` remotely, use Cursor's HTTP MCP configuration and point it at
`http://HOST:PORT/mcp`.

Remote example:

```json
{
  "mcpServers": {
    "sxmc": {
      "url": "http://127.0.0.1:8000/mcp"
    }
  }
}
```

If you protect the endpoint with `--bearer-token` or `--require-header`, add the
matching auth configuration in Cursor's MCP server definition.

## Gemini CLI

Gemini CLI supports MCP servers from `.gemini/settings.json` or
`~/.gemini/settings.json`.

Example:

```json
{
  "mcpServers": {
    "sxmc": {
      "command": "sxmc",
      "args": ["serve", "--paths", "/absolute/path/to/skills"]
    }
  }
}
```

Then launch Gemini CLI and run:

```text
/mcp list
```

Gemini CLI can also package `sxmc` as part of a local extension if you want to
bundle a skills directory and a `GEMINI.md` context file together.

Gemini CLI can also register servers directly from the command line:

```bash
gemini mcp add sxmc sxmc serve --paths /absolute/path/to/skills
gemini mcp add sxmc-remote http://127.0.0.1:8000/mcp --transport http
```

For a remote server, configure the MCP server URL as `http://HOST:PORT/mcp`.
If you use `--bearer-token` or `--require-header`, include the same auth
configuration in the remote MCP config.

## Claude Code and Similar Local MCP Clients

For local coding agents that accept a stdio MCP server definition, point them at:

```text
command: sxmc
args: ["serve", "--paths", "/absolute/path/to/skills"]
```

For remote-capable clients, host:

```bash
sxmc serve --transport http --host 0.0.0.0 --port 8000 --paths /absolute/path/to/skills
```

and use:

```text
http://YOUR_HOST:8000/mcp
```

For anything beyond localhost, prefer:

```bash
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --bearer-token env:SXMC_MCP_TOKEN \
  --paths /absolute/path/to/skills
```

Health and smoke checks:

```bash
curl http://127.0.0.1:8000/healthz
sxmc http http://127.0.0.1:8000/mcp --auth-header "Authorization: Bearer $SXMC_MCP_TOKEN" --list
sxmc http http://127.0.0.1:8000/mcp --auth-header "Authorization: Bearer $SXMC_MCP_TOKEN" --prompt simple-skill arguments=friend
```

Because `sxmc` exposes a hybrid surface, these clients can use:
- native prompts for skill bodies
- native resources for `references/`
- native tools for `scripts/`
- generic retrieval tools for `skills -> MCP -> CLI` compatibility

## Recommended Pattern

For broadest compatibility, prefer the hybrid pattern already implemented by
`sxmc serve`:

- `get_available_skills`
- `get_skill_details`
- `get_skill_related_file`

Those generic tools are the most portable across clients that are better at
tool calling than prompt/resource handling.

For shell-side inspection outside those clients, `sxmc stdio` / `sxmc http`
can now fetch native prompts/resources directly with `--prompt` and
`--resource` in addition to calling tools.

## Release and Distribution

Additional release-channel notes are in:

- [`docs/SMOKE_TESTS.md`](SMOKE_TESTS.md)
- [`docs/DISTRIBUTION.md`](DISTRIBUTION.md)

If you want a single repeatable pre-release check, run the smoke script from
[`docs/SMOKE_TESTS.md`](SMOKE_TESTS.md) before tagging.
