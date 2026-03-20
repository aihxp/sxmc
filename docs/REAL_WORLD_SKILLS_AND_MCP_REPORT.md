# Real-world skills & MCP servers — integration report

**Date:** 2026-03-20  
**sxmc:** `0.1.6` (`cargo` / `sxmc --version`)  
**Host:** Linux x86_64  
**Node:** v22.x (`npx` available for official MCP npm packages)

This document records **manual integration tests**: five **real** skill directories taken from this machine (user skills, Cursor built-ins, OpenClaw npm bundle) and five **public** MCP servers invoked via **`npx`**. It is **not** a performance benchmark; it focuses on **whether sxmc behaves usefully** and **where friction appears**.

---

## 1. Five real-world skills

### 1.1 How they were selected

| # | Skill name (frontmatter) | Origin | Rationale |
|---|--------------------------|--------|-----------|
| 1 | `system-info` | `~/.claude/skills/system-info` | User-authored skill with **script** + **reference** |
| 2 | `create-skill` | `~/.cursor/skills-cursor/create-skill` | Large **Cursor** maintainer skill (markdown-heavy) |
| 3 | `shell` | `~/.cursor/skills-cursor/shell` | Cursor **/shell** workflow skill |
| 4 | `github` | `openclaw` npm package `skills/github` | Third-party **integration** skill (`gh` CLI) |
| 5 | `summarize` | `openclaw` npm package `skills/summarize` | Third-party **media/URL** workflow skill |

All five were symlinked under a single search root so one `sxmc` process could load them together:

```text
/tmp/sxmc-realworld-skills/
  system-info   -> ~/.claude/skills/system-info
  create-skill  -> ~/.cursor/skills-cursor/create-skill
  shell-skill   -> ~/.cursor/skills-cursor/shell
  github        -> ~/.npm-global/lib/node_modules/openclaw/skills/github
  summarize     -> ~/.npm-global/lib/node_modules/openclaw/skills/summarize
```

*(Folder name `shell-skill` vs frontmatter name `shell` is intentional: discovery uses directory names for on-disk layout; frontmatter `name` is what sxmc reports.)*

### 1.2 Discovery & metadata

```bash
sxmc skills list --paths /tmp/sxmc-realworld-skills
```

**Result:** **Success.** All five skills appeared with correct **descriptions** from YAML frontmatter.

### 1.3 `sxmc skills info <name>`

**Result:** **Success** for each skill. Full **body** text is returned (for `create-skill` this is **very long** — expected).

### 1.4 `sxmc skills run <name>`

**Behavior:** `skills run` prints the **skill body** with **`$ARGUMENTS`** substitution (empty if no args passed), not “executing” the skill in an agent sense.

| Skill | Notes |
|-------|--------|
| `system-info` | Body printed; **no script execution** here (by design of `skills run`). |
| `create-skill`, `shell`, `github`, `summarize` | Large or medium bodies printed to stdout — **works**, but noisy for terminal use. |

**Conclusion:** Works **as documented** for `skills run`; do not confuse with MCP tool execution.

### 1.5 `sxmc scan --paths … --skill <name>`

**Result:** **All five reported `[PASS]`** at default severity (no critical/error findings flagged on these particular files in this run).

### 1.6 Serving all five over MCP (`sxmc serve` + `sxmc stdio` bridge)

```bash
sxmc stdio "sxmc serve --paths /tmp/sxmc-realworld-skills" --list
```

**Result:** **Success (exit 0).**

- **Hybrid tools:** `get_available_skills`, `get_skill_details`, `get_skill_related_file`, plus **`system_info__sysinfo`** (only **system-info** had a `scripts/` entry).
- **Prompts:** one per skill (**5 prompts**).
- **Resources:** **usage-guide.md** from `system-info` only.

**Finding:** Skills without `scripts/` do **not** get a dedicated run-script MCP tool — they are still exposed as **prompts**, which matches sxmc’s hybrid model.

### 1.7 Skill-side errors

**None** for parsing, discovery, scan, or stdio listing in this configuration. Symlinked skill roots **were followed** correctly on this Linux host.

---

## 2. Five real-world MCP servers (npm + `npx`)

### 2.1 Test method

For each server:

```bash
sxmc stdio "npx -y <package> [args…]" --list
```

**Timeout:** 120s per server (first `npx` install can be slow).  
**Network:** required for npm.

### 2.2 Results summary (**sxmc 0.1.6**)

| # | Package / command | Tools listed? | Prompts / resources | Exit | Notes |
|---|-------------------|---------------|---------------------|------|--------|
| 1 | `@modelcontextprotocol/server-everything` | Yes (**13**) | Prompts **4**, resources listed | **0** | Reference implementation; **full success**. |
| 2 | `@modelcontextprotocol/server-memory` | Yes (**9**) | **Skipped** (not advertised) | **0** | stderr: *Skipping prompt/resource listing…*; **no** `-32601` failure. |
| 3 | `@modelcontextprotocol/server-filesystem /tmp` | Yes (**14**) | **Skipped** (not advertised) | **0** | Same pattern as memory. |
| 4 | `@modelcontextprotocol/server-sequential-thinking` | Yes (**1**) | **Skipped** (not advertised) | **0** | Same pattern. |
| 5 | `@modelcontextprotocol/server-github` | Yes (many) | **Skipped** (not advertised) | **0** | Same pattern; **no `GITHUB_TOKEN` required** for `--list` of tool metadata. |
| *(alt)* | `@modelcontextprotocol/server-fetch` | — | — | **1** | **npm 404** — package **does not exist** under that name (as of test date). |

### 2.3 Interpretation (important)

**As of v0.1.5**, `sxmc stdio` / `http` **`--list`** uses **capability-aware** listing:

- If the server **does not advertise** prompts or resources during initialization, sxmc **skips** those listings (with a short stderr notice) and still exits **0** after tools.
- If a listing is attempted and the server returns **`-32601` Method not found** (or similar), sxmc treats that as **“optional surface not available”**, prints a skip notice, and continues — **exit 0**.

**Compared to v0.1.3 and earlier:** those versions could **exit 1** after successful tool discovery when `prompts/list` failed. **That regression for common npm servers is resolved in 0.1.5.**

**Practical impact today:**

- **`sxmc stdio "…" --list`** is **reliable** for discovering **tools** on typical `@modelcontextprotocol/*` servers.
- **Tool invocation** still works as before; e.g. **`server-everything`**:

  ```bash
  sxmc stdio "npx -y @modelcontextprotocol/server-everything" get-sum a=2 b=3 --pretty
  ```

  returned: `The sum of 2 and 3 is 5.` (**success**).

### 2.4 Zero-argument tools (`read_graph`, `list_allowed_directories`, …)

Some npm MCP servers validate tool arguments as a **JSON object**. With **sxmc**’s `key=value` CLI parsing, **no arguments** after the tool name means the client may **omit** the arguments object entirely, which surfaces as **input validation** errors from the server (e.g. *expected object, received undefined*) even though **exit code stays 0**.

**Workaround used in the v0.1.6 manual run:** pass a disposable empty object so an object is always sent:

```bash
sxmc stdio "npx -y @modelcontextprotocol/server-memory" read_graph _={} --pretty
sxmc stdio "npx -y @modelcontextprotocol/server-filesystem /tmp" list_allowed_directories _={} --pretty
```

(`_` is ignored by the server schema; only `{}` matters.)

**Follow-up:** `master` now sends `{}` for zero-argument tool calls by default, so
strict object validators no longer need this workaround in the CLI.

### 2.5 “Dialog” / multi-step use on **promptless** MCP servers

**What `sxmc stdio` does:** each invocation **spawns one MCP server subprocess**, runs **one** user-facing action (e.g. `--list`, `call_tool`, `--describe`), then **tears the session down**. There is **no built-in REPL** for multiple tool calls inside a **single** long-lived MCP session.

**What we tested (v0.1.6):**

1. **Chained invocations (separate processes)** — useful to confirm promptless servers keep working after `--list` and for scripts/CI:
   - **`server-sequential-thinking`:** two `sequentialthinking` calls in a row (different `sxmc` processes) → both **exit 0**; JSON shows `thoughtHistoryLength: 1` each time because **state does not carry** between invocations.
   - **`server-memory`:** two `read_graph _={}` calls → both **exit 0**, empty graph each time (again, **new server** each run).
   - **`server-filesystem`:** `list_allowed_directories _={}` then `list_directory path=/tmp` → both **exit 0**.

2. **True multi-turn state** (thought history, in-memory graph, etc.) requires a **long-lived MCP host** (IDE agent, custom client, or a small script using an MCP SDK) — **not** multiple bare `sxmc stdio` calls.

**Conclusion:** **Promptless MCPs work fine for repeated one-shot tool calls via sxmc**; treat **multi-step “conversation”** as **orchestration outside sxmc** unless/until a persistent-session CLI mode exists.

---

## 3. Cross-cutting conclusions

| Topic | Verdict |
|-------|---------|
| **Load diverse real skills** | **Works** — symlinked multi-root bundle is OK. |
| **`skills list` / `info` / `run` / `scan`** | **Works** for all five; `run` is **prompt dump**, not automation. |
| **Serve 5 skills as MCP** | **Works** — prompts + hybrid tools + resources as designed. |
| **Bridge official MCP servers** | **Works** for **`--list`** on the five tested servers — **exit 0** with tools listed (**v0.1.5+**). |
| **Multi-step / “dialog” on promptless MCP** | **Repeated `sxmc stdio … tool` calls work** (each is a **new** session). **Stateful** chains need a long-lived host (§2.5). |
| **Wrong npm package name** | **User error surface** — `server-fetch` 404; verify package names on npm. |

---

## 4. Recommendations (product / docs)

1. ~~**MCP client `list`:** If `list_prompts` returns **`-32601`**, treat as **“no prompts”** and exit **0**~~ — **Done in v0.1.5** (optional-surface handling + advertised-capability skip).
2. **Docs:** Call out that **`@modelcontextprotocol/server-everything`** is the easiest **known-good** server for full **tools + prompts + resources** demos.
3. **Docs:** Link to npm scope **`@modelcontextprotocol/`** package list; **`server-fetch`** name may be wrong or unpublished.
4. ~~**Product:** For tools that declare **no parameters**, consider always sending **`{}`** as arguments when the CLI passes none, to match strict Zod/json-schema validators on popular npm servers.~~ — **Done on `master`**.

---

## 5. Reproduction checklist

```bash
# Skills bundle (adjust paths to your machine)
mkdir -p /tmp/sxmc-realworld-skills
# … add symlinks as in section 1.1 …

sxmc skills list --paths /tmp/sxmc-realworld-skills
sxmc scan --paths /tmp/sxmc-realworld-skills
sxmc stdio "sxmc serve --paths /tmp/sxmc-realworld-skills" --list

# MCP (requires Node + network; sxmc >= 0.1.5 for non-fatal optional surfaces)
sxmc stdio "npx -y @modelcontextprotocol/server-everything" --list
sxmc stdio "npx -y @modelcontextprotocol/server-everything" get-sum a=2 b=3 --pretty
sxmc stdio "npx -y @modelcontextprotocol/server-memory" --list   # expect exit 0 (tools + skip notices)

# Promptless “dialog” (separate sxmc processes; see §2.5)
sxmc stdio "npx -y @modelcontextprotocol/server-sequential-thinking" \
  sequentialthinking thought="Step A" thoughtNumber=1 totalThoughts=2 nextThoughtNeeded=true --pretty
sxmc stdio "npx -y @modelcontextprotocol/server-sequential-thinking" \
  sequentialthinking thought="Step B" thoughtNumber=2 totalThoughts=2 nextThoughtNeeded=false --pretty
sxmc stdio "npx -y @modelcontextprotocol/server-memory" read_graph _={} --pretty
```

---

## Related repository docs

- [MCP_TO_CLI_VERIFICATION.md](MCP_TO_CLI_VERIFICATION.md)
- [VALUE_AND_BENCHMARK_FINDINGS.md](VALUE_AND_BENCHMARK_FINDINGS.md)
- [BENCHMARK_RUN_v0.1.6.md](BENCHMARK_RUN_v0.1.6.md)
- [BENCHMARK_RUN_v0.1.5.md](BENCHMARK_RUN_v0.1.5.md)
- [CLIENTS.md](CLIENTS.md)
