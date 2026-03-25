# Side by side: **with `sxmc`** vs **without**

This note compares typical workflows: what you do **manually** (shell, curl, files) versus **`sxmc`** as the bridge (OpenAPI, skills, MCP). Examples use **public Petstore**, repo **`tests/fixtures`**, and **`server-everything`** when network is available.

---

## 1. Discover OpenAPI operations

| Without `sxmc` | With `sxmc` |
|----------------|-------------|
| Download the spec (`curl` / browser), then read JSON or write `jq` to list paths and methods. You map each operation to a URL and query style yourself. | `sxmc api <openapi-url> --list` (or `--list --format json`) gives **named operations** with parameters in one step. |

**Without:** `curl -s <openapi.json> | …` — you own parsing and naming.

**With (example):**

```bash
sxmc api https://petstore3.swagger.io/api/v3/openapi.json --list
# JSON: sxmc api … --list --format json
```

You get a **single structured view** (including `findPetsByStatus`, params, descriptions) instead of navigating raw `paths` in the spec.

---

## 2. Call an HTTP API

| Without `sxmc` | With `sxmc` |
|----------------|-------------|
| Know the **base URL**, path, query string, and HTTP method from the spec; build `curl` and parse JSON. | `sxmc api <spec> <operationName> key=value …` — same binary picks the right HTTP shape from the spec. |

**Without:**

```bash
curl -sS "https://petstore3.swagger.io/api/v3/pet/findByStatus?status=available"
```

Returns raw JSON; you must remember the path and query contract.

**With:**

```bash
sxmc api https://petstore3.swagger.io/api/v3/openapi.json findPetsByStatus status=available --format json
```

Same data, but **operation is named** and **arguments are typed** from the spec (optional `--format json` / `toon` for readability).

---

## 3. Read a skill (Agent Skills / SKILL.md)

| Without `sxmc` | With `sxmc` |
|----------------|-------------|
| `cat` / editor: find skill dir, open `SKILL.md`, parse frontmatter yourself. | `sxmc skills info <name> --paths …` shows **name, description, paths, body** in one consistent layout. |

**Without:** `head -n 20 path/to/skill/SKILL.md` — YAML + body mixed.

**With:**

```bash
sxmc skills info simple-skill --paths tests/fixtures
```

---

## 4. MCP tools (stdio): discovery

| Without `sxmc` | With `sxmc` |
|----------------|-------------|
| Run an MCP server (e.g. `npx …`), then use a **client** (IDE, or hand-written JSON-RPC over stdio) to send `tools/list` and interpret results. | `sxmc stdio "npx -y @modelcontextprotocol/server-everything" --list-tools` (or `--list`) prints **tools** (and optional prompts/resources) in the terminal. |

There is **no single “curl” for MCP** — you normally use a host. **`sxmc`** is the **CLI-shaped** host for listing and calling tools without writing JSON-RPC.

---

## 5. Skills → MCP → CLI (nested bridge)

| Without `sxmc` | With `sxmc` |
|----------------|-------------|
| Implement or configure an MCP server that loads skills and exposes tools; then use another client to call tools. | `sxmc serve --paths …` plus `sxmc stdio "sxmc serve --paths …" <tool>` — one binary acts as **server** and **client** for a quick scriptable check. |

**With:**

```bash
sxmc stdio "sxmc serve --paths tests/fixtures" skill_with_scripts__hello
```

---

## Summary

| Concern | Manual / fragmented | With `sxmc` |
|---------|---------------------|---------------|
| OpenAPI discovery | Spec + `jq` / editor | `sxmc api … --list` |
| OpenAPI call | `curl` + URL knowledge | `sxmc api … <op> …` |
| Skill content | `cat` SKILL.md | `sxmc skills info` / `list` |
| MCP discovery | Custom JSON-RPC client | `sxmc stdio … --list` |
| Automation | Glue scripts + multiple tools | One binary, consistent flags |

---

## Validation

For the maintained validation story, see:

- [`VALIDATION.md`](VALIDATION.md)
- [`TEST_SUITE_REPORT_v1.0.0.md`](TEST_SUITE_REPORT_v1.0.0.md)
- [`PRODUCT_CONTRACT.md`](PRODUCT_CONTRACT.md)
