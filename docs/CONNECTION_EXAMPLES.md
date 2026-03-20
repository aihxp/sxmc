# Connection Examples

Copy-pasteable examples for common `sxmc` connection patterns.

## Local stdio MCP server

```bash
sxmc serve --paths /absolute/path/to/skills
```

## Local stdio bridge with JSON-array command spec

```bash
sxmc stdio '["sxmc","serve","--paths","/absolute/path/to/skills"]' --list
```

This avoids shell quoting issues and is the safest pattern for nested commands.

## Local stdio bridge with explicit working directory

```bash
sxmc stdio '["sxmc","serve"]' --cwd /absolute/path/to/project --list
```

This is useful when the server should discover project-local `.claude/skills`.

## Hosted MCP server

```bash
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --bearer-token env:SXMC_MCP_TOKEN \
  --paths /absolute/path/to/skills
```

## Hosted MCP bridge

```bash
sxmc http http://127.0.0.1:8000/mcp \
  --auth-header "Authorization: Bearer $SXMC_MCP_TOKEN" \
  --list
```

## Skills to MCP to CLI

```bash
sxmc stdio '["sxmc","serve","--paths","tests/fixtures"]' get_available_skills --pretty
sxmc stdio '["sxmc","serve","--paths","tests/fixtures"]' --prompt simple-skill arguments=friend
sxmc stdio '["sxmc","serve","--paths","tests/fixtures"]' --resource \
  "skill://skill-with-references/references/style-guide.md"
```
