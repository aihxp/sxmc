# Real-world skills & MCP servers — integration report

**Date:** 2026-03-20  
**sxmc:** `0.1.3` (`cargo` / `sxmc --version`)  
**Host:** Linux x86_64  
**Node:** v22.x (`npx` available for official MCP npm packages)

This document records **manual integration tests**: five **real** skill directories taken from this machine (user skills, Cursor built-ins, OpenClaw npm bundle) and five **public** MCP servers invoked via **`npx`**. It is **not** a performance benchmark; it focuses on **whether sxmc behaves usefully** and **where friction appears**.

Maintainer note: the original finding about prompt-less servers making
`sxmc stdio ... --list` exit non-zero has since been addressed on `master`.
Current behavior is capability-aware: unsupported prompt/resource surfaces are
skipped during `--list` instead of failing the whole command.

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
  github        -> ~/.npm-global/.../openclaw/skills/github
  summarize     -> ~/.npm-global/.../openclaw/skills/summarize
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

### 2.2 Results summary

| # | Package / command | Tools listed? | Prompts listed? | Exit | Notes |
|---|-------------------|---------------|-----------------|------|--------|
| 1 | `@modelcontextprotocol/server-everything` | Yes (**13**) | Yes (**4**) | **0** | Reference implementation; **full success**. |
| 2 | `@modelcontextprotocol/server-memory` | Yes (**9**) | **Failed** | **1** | After tools: `list_prompts` → **JSON-RPC `-32601` Method not found**. |
| 3 | `@modelcontextprotocol/server-filesystem /tmp` | Yes (**14**) | **Failed** | **1** | Same **`list_prompts`** / **`-32601`** pattern. |
| 4 | `@modelcontextprotocol/server-sequential-thinking` | Yes (**1**) | **Failed** | **1** | Same pattern. |
| 5 | `@modelcontextprotocol/server-github` | Yes (many) | **Failed** | **1** | Same pattern; **no `GITHUB_TOKEN` required** for `--list` of tool metadata. |
| *(alt)* | `@modelcontextprotocol/server-fetch` | — | — | **1** | **npm 404** — package **does not exist** under that name (as of test date). |

### 2.3 Interpretation (important)

Many official MCP servers implement **tools** (and sometimes **resources**) but **do not** implement the **`prompts/list`** capability. At test time, **sxmc**’s `stdio`/`http` **`--list` path called `list_prompts` unconditionally** after listing tools. When the server returned **`-32601`**, the **overall CLI exited non-zero** even though **tool discovery already succeeded**.

**Practical impact:**

- At test time, **`sxmc stdio "…" --list`** looked **failed** for many real servers.
- **Tool invocation** may still work: e.g. **`server-everything`**:

  ```bash
  sxmc stdio "npx -y @modelcontextprotocol/server-everything" get-sum a=2 b=3 --pretty
  ```

  returned: `The sum of 2 and 3 is 5.` (**success**).

So: **MCP → CLI works**, and the recorded brittleness around prompt-less servers
was a good trigger for making `--list` treat unsupported prompts/resources as
**non-fatal**.

### 2.4 Follow-up test (`server-memory`)

Calling `read_graph` with guessed CLI args produced **`-32602` input validation** (arguments not passed as the server expects). This is **normal** when hand-testing; it shows the **session stays alive** for `call_tool` after connect.

---

## 3. Cross-cutting conclusions

| Topic | Verdict |
|-------|---------|
| **Load diverse real skills** | **Works** — symlinked multi-root bundle is OK. |
| **`skills list` / `info` / `run` / `scan`** | **Works** for all five; `run` is **prompt dump**, not automation. |
| **Serve 5 skills as MCP** | **Works** — prompts + hybrid tools + resources as designed. |
| **Bridge official MCP servers** | **Useful** — **tool listing** works, and current `master` treats unsupported prompts/resources as non-fatal during `--list`. |
| **Wrong npm package name** | **User error surface** — `server-fetch` 404; verify package names on npm. |

---

## 4. Recommendations (product / docs)

1. **MCP client `list`:** Landed on `master` — if prompts/resources are not supported, `--list` now skips those surfaces instead of exiting non-zero.
2. **Docs:** Call out that **`@modelcontextprotocol/server-everything`** is the easiest **known-good** server for **`sxmc stdio … --list`** demos.
3. **Docs:** Link to npm scope **`@modelcontextprotocol/`** package list; **`server-fetch`** name may be wrong or unpublished.

---

## 5. Reproduction checklist

```bash
# Skills bundle (adjust paths to your machine)
mkdir -p /tmp/sxmc-realworld-skills
# … add symlinks as in section 1.1 …

sxmc skills list --paths /tmp/sxmc-realworld-skills
sxmc scan --paths /tmp/sxmc-realworld-skills
sxmc stdio "sxmc serve --paths /tmp/sxmc-realworld-skills" --list

# MCP (requires Node + network)
sxmc stdio "npx -y @modelcontextprotocol/server-everything" --list
sxmc stdio "npx -y @modelcontextprotocol/server-everything" get-sum a=2 b=3 --pretty
sxmc stdio "npx -y @modelcontextprotocol/server-memory" --list   # current master should succeed and skip prompts
```

---

## Related repository docs

- [MCP_TO_CLI_VERIFICATION.md](MCP_TO_CLI_VERIFICATION.md)
- [VALUE_AND_BENCHMARK_FINDINGS.md](VALUE_AND_BENCHMARK_FINDINGS.md)
- [CLIENTS.md](CLIENTS.md)
