# Validation run — **sxmc v0.1.9** (2026-03-21)

**Newer run:** [`VALIDATION_RUN_v2.0.0.md`](VALIDATION_RUN_v2.0.0.md) (**2.0.0**).

Maintainer pass: **automated tests**, **`certify_release.sh`**, **`smoke_real_world_mcps.sh`**, **benchmarks**, **five real skills**, **five npm MCPs**, **promptless multi-invocation**, **JSON outputs**, **MCP → CLI**, **`bake` + `sxmc mcp` (CLI → agent)**, and **`sxmc mcp session`** (stateful MCP).

## Environment

- **Host:** Linux x86_64  
- **sxmc:** **0.1.9** (`target/release/sxmc` from this repo; `cargo search sxmc` → **0.1.9** on crates.io)  
- **Node:** `npx` for `@modelcontextprotocol/*` smoke  

---

## 1. Automated tests (`cargo test`)

| Suite | Count | Result |
|-------|------:|:------:|
| Library unit tests | **70** | pass |
| `src/main.rs` unit tests | **5** | pass |
| `tests/cli_integration.rs` | **47** | pass |
| Doc tests | **1** | pass |
| **Total** | **123** | **pass** |

Includes **`test_mcp_session_preserves_stateful_tool_memory`** (Python `stateful_mcp_server.py` fixture).

---

## 2. `scripts/certify_release.sh`

```bash
bash scripts/certify_release.sh target/release/sxmc tests/fixtures
```

**Result:** **Passed** (`Release certification checks passed.`).

---

## 3. `scripts/smoke_real_world_mcps.sh`

```bash
bash scripts/smoke_real_world_mcps.sh target/release/sxmc
```

**Result:** **Passed** (`Real-world MCP smoke checks passed.`)

Servers: **everything**, **memory**, **filesystem `/tmp`**, **sequential-thinking**, **github** (same five as [`VALIDATION.md`](VALIDATION.md)).

---

## 4. Benchmarks (`scripts/benchmark_cli.sh`, 5 runs, median ms)

Regression sanity only; Petstore is **network-dominated**.

| Scenario | Step | Median (ms) |
|----------|------|------------:|
| A | stdio → `skill_with_scripts__hello` | **12** |
| B | `api` Petstore `--list` | **715** |
| B | `api` `findPetsByStatus` | **1321** |
| B | `curl` only | **510** |
| C | Nested `serve` `--list` | **11** |
| D | `scan` `malicious-skill` | **12** |
| Micro | Local OpenAPI + HTTP `listPets` | **14** |

---

## 5. Five real-world skills

Path: `/tmp/sxmc-realworld-skills` (symlinks: `system-info`, Cursor `create-skill` / `shell`, OpenClaw `github` / `summarize`).

| Check | Result |
|-------|--------|
| `skills list` | OK |
| `scan --skill` ×5 | **[PASS]** ×5 |

---

## 6. Promptless “dialog” (repeated `stdio` invocations)

Two **`sequentialthinking`** calls on **`@modelcontextprotocol/server-sequential-thinking`**: both **exit 0**; JSON shows **`thoughtHistoryLength": 1`** each time — **no shared memory** between processes (expected for one-shot `sxmc stdio`).

---

## 7. JSON outputs (machine-readable checks)

Parsed with **`python3`** (`json.load` or first `JSONDecoder().raw_decode` where needed):

| Command | Result |
|---------|--------|
| `sxmc skills list --paths tests/fixtures --json` | Single JSON document — **OK** |
| `sxmc scan --paths tests/fixtures --json` | Single JSON document — **OK** (`reports` array when scanning multiple targets) |
| `sxmc stdio "…serve…" --describe --format json` | Single JSON — **OK** |
| `sxmc mcp info <bake>/get_skill_details --format json` (temp bake) | Single JSON — **OK** |
| `sxmc api <petstore> --list --format json` | Single JSON document — **OK** |
| `sxmc api <petstore> findPetsByStatus status=available --format json` | Response body JSON — **OK** |

---

## 8. MCP → CLI (features)

- **`stdio` / `http`:** discovery, optional surfaces, zero-arg tools (smoke script) — **OK**  
- **Nested `serve`:** fixtures bridge — **OK**  

---

## 9. CLI → agent (`bake` + `sxmc mcp`)

Validated: **`bake create`** → **`mcp servers` / `mcp grep` / `mcp call`** with JSON payload — matches [`USAGE.md`](USAGE.md) and agent snippets under `examples/agent-docs/`.

---

## 10. Stateful MCP: `sxmc mcp session` (**v0.1.9**)

**Product claim:** [`CHANGELOG.md`](../CHANGELOG.md) — explicit multi-step workflows over **one baked connection**.

**Manual check** (isolated `XDG_CONFIG_HOME`, bake → `tests/fixtures/stateful_mcp_server.py`):

```text
stdin:
  call remember_state '{"key":"k","value":"session-ok"}' --pretty
  call read_state '{"key":"k"}' --pretty
  exit
```

**Result:** `read_state` returned **`"value": "session-ok"`** — **session memory preserved** inside **`mcp session`**, unlike two separate `sxmc stdio` processes.

---

## 11. Does it match the description?

| Area | Verdict |
|------|---------|
| **Tests + certify + smoke** | **Yes** — aligned with [`PRODUCT_CONTRACT.md`](PRODUCT_CONTRACT.md) |
| **JSON where advertised** | **Yes** for `skills list --json`, `scan --json`, `describe --format json`, `mcp info --format json`, `api --list --format json`, and `api … call --format json` |
| **Stateful MCP** | **Yes** — `mcp session` demonstrated with fixture server |
| **Performance** | Benchmarks **only** for regression sanity |

---

## Related

- [`VALIDATION.md`](VALIDATION.md)  
- [`USAGE.md`](USAGE.md)  
- [`VALIDATION_RUN_v0.1.8.md`](VALIDATION_RUN_v0.1.8.md) — prior snapshot  
