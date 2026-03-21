# Validation run — **sxmc 2.0.0** (2026-03-21)

Maintainer pass after the **2.0.0** release: **tests**, **`certify_release.sh`**, **`smoke_real_world_mcps.sh`**, **benchmarks**, **five real skills**, **five npm MCPs**, **promptless multi-invocation**, **JSON on stdout**, **MCP → CLI**, **`bake` + `sxmc mcp`**, **`sxmc mcp session`**, **Cursor-oriented workflow (simulated)**, and a **warnings / stderr** inventory.

## Environment

- **Host:** Linux x86_64  
- **sxmc:** **2.0.0** (`target/release/sxmc`; `cargo search sxmc` → **2.0.0**)  
- **Node:** `npx` for smoke script  

---

## 1. Automated tests (`cargo test`)

| Suite | Count | Result |
|-------|------:|:------:|
| Library unit tests | **70** | pass |
| `src/main.rs` unit tests | **5** | pass |
| `tests/cli_integration.rs` | **47** | pass |
| Doc tests | **1** | pass |
| **Total** | **123** | **pass** |

**Compiler / test warnings:** **`cargo test` produced no `warning:` lines** in the captured log (`/tmp/sxmc-200-test.log` on the maintainer host).

---

## 2. `scripts/certify_release.sh`

```bash
bash scripts/certify_release.sh target/release/sxmc tests/fixtures
```

**Result:** **Passed** (`Release certification checks passed.`).  
**Errors:** none in `/tmp/sxmc-200-certify.log`.

---

## 3. `scripts/smoke_real_world_mcps.sh`

**Result:** **Passed** (`Real-world MCP smoke checks passed.`).

**Stderr (informational, not failures):** for prompt-less third-party servers, lines such as:

```text
[sxmc] Skipping prompt listing because the MCP server did not advertise that capability during initialization.
[sxmc] Skipping resource listing because the MCP server did not advertise that capability during initialization.
```

These match the **graceful degradation** described in [`PRODUCT_CONTRACT.md`](PRODUCT_CONTRACT.md). **Exit codes remained 0.**

---

## 4. Benchmarks (`scripts/benchmark_cli.sh`, 5 runs, median ms)

| Scenario | Step | Median (ms) |
|----------|------|------------:|
| A | stdio → `skill_with_scripts__hello` | **12** |
| B | `api` Petstore `--list` | **611** |
| B | `api` `findPetsByStatus` | **1022** |
| B | `curl` only | **408** |
| C | Nested `serve` `--list` | **10** |
| D | `scan` `malicious-skill` | **12** |
| Micro | Local OpenAPI + HTTP `listPets` | **14** |

Petstore steps are **network-dominated**; use as **regression sanity** only.

---

## 5. Five real-world skills

Path: `/tmp/sxmc-realworld-skills` (same symlink bundle as prior runs).

**Result:** `sxmc skills list` and **`scan --skill` ×5** → all **[PASS]** at default severity.

---

## 6. Promptless “dialog” (two `stdio` invocations)

**`@modelcontextprotocol/server-sequential-thinking`:** two `sequentialthinking` calls → both **exit 0**; **`thoughtHistoryLength": 1`** each time → **no** shared session across processes (expected for one-shot `stdio`).

---

## 7. JSON outputs (stdout vs stderr)

**Important:** For machine parsing, consume **stdout only**; **`[sxmc]` lines are on stderr** and must not be fed to `json.load`.

| Check | Result (stdout only, stderr discarded) |
|-------|----------------------------------------|
| `skills list --json` | Valid JSON — **OK** |
| `scan --paths … --json` | Single JSON document — **OK** (**2.0.0** changelog: multi-target scan emits one document) |
| `api … --list --format json` | Valid JSON (e.g. `api_type`, `count`, `operations[]`) — **OK** (**2.0.0**: `--list` honors structured flags) |
| `stdio … --describe --format json` | Valid JSON — **OK** |
| `api … <operation> --format json` | Valid JSON — **OK** (spot-check: `findPetsByStatus`) |

**Finding:** Behavior matches **2.0.0** release notes for **structured `--list`** and **scan JSON**.

---

## 8. MCP → CLI & CLI → agent (`bake` + `sxmc mcp`)

- **Ad hoc `stdio` / nested `serve`:** exercised via certify + benchmarks.  
- **`bake` + `mcp call …/get_skill_details`** with JSON payload → returns **`simple-skill`** body as expected.

---

## 9. Stateful MCP: `sxmc mcp session`

**Manual check** (`stateful_mcp_server.py`, isolated `XDG_CONFIG_HOME`): **`remember_state`** then **`read_state`** returned stored **`v2`**.

**Finding:** Session memory **persists within `mcp session`**, consistent with [`CHANGELOG.md`](../CHANGELOG.md) / [`USAGE.md`](USAGE.md).

---

## 10. Cursor workflow (simulated; no IDE launch)

[`USAGE.md`](USAGE.md) recommends local clients use:

```text
command: sxmc
args: ["serve", "--paths", "/absolute/path/to/skills"]
```

**Simulated check (terminal):** `sxmc stdio` with a **JSON array command spec** pointing at **`target/release/sxmc`**, **`serve`**, and **`--paths`** to **`tests/fixtures`**, then **`--list`**:

- **Result:** **exit 0**; tools / prompts / resources listed — same bridge Cursor would drive over stdio MCP.

**Limitation:** This run **did not** open the **Cursor** IDE or edit `.cursor/mcp.json`; it validates the **documented wire shape** (stdio MCP + `serve --paths`). Full IDE integration remains a **client-side** configuration test.

---

## 11. Errors and warnings summary

| Source | Findings |
|--------|----------|
| `cargo test` | No **`warning:`** lines observed |
| `certify_release` | No errors; **exit 0** |
| `smoke_real_world_mcps` | **No failures**; stderr **skip** lines only |
| JSON parsing | **Fails if stderr is concatenated with stdout** — operator error, not a product bug; document **stdout-only** parsing |

---

## 12. Does it match the description?

| Area | Verdict |
|------|---------|
| **Tests + certify + smoke** | **Yes** |
| **2.0.0 structured list + scan JSON** | **Yes** (§7) |
| **Stateful `mcp session`** | **Yes** (§9) |
| **Cursor-style stdio command** | **Yes** for documented JSON-array / `serve` pattern (§10) |
| **Performance** | Benchmarks for **sanity** only (§4) |

---

## Related

- [`VALIDATION.md`](VALIDATION.md)  
- [`USAGE.md`](USAGE.md)  
- [`VALIDATION_RUN_v0.1.9.md`](VALIDATION_RUN_v0.1.9.md) — prior numbered snapshot  
