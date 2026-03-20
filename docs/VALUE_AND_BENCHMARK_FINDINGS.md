# sxmc — value proposition & benchmark findings

This note collects **why sxmc is useful**, **how to measure it**, and **representative timing** from automated CLI benchmarks. Token figures are **estimates** unless you instrument your own LLM client.

## Added value (summary)

| Capability | What it avoids / replaces |
|------------|---------------------------|
| **Skills → MCP** (`serve`) | A custom MCP adapter per skill repo; repeating SKILL.md bodies in chat |
| **MCP → CLI** (`stdio`, `http`) | Ad-hoc JSON-RPC clients; debugging only inside an IDE |
| **OpenAPI / GraphQL → CLI** (`api`, `spec`, `graphql`) | Pasting large specs; hand-written `curl` for every operation |
| **Security** (`scan`) | LLM-only “please audit this skill” passes (slow, variable) |
| **Distribution** | One Rust binary (plus optional wrappers) instead of several stacks |

**Even a single, narrow MCP server** often benefits from **`sxmc stdio …` / `sxmc http …`**: scriptability, `--list` / `--pretty` inspection, CI, and debugging outside a full agent.

## Representative wall-clock results (CLI)

Environment: **Linux x86_64**, **sxmc 0.1.2**, **5 runs**, **median milliseconds**. Commands hit public Petstore where noted (network variance is expected).

| Scenario | Command / step | Median (ms) | Notes |
|----------|------------------|------------|--------|
| A | `sxmc stdio "sxmc serve" …` → skill script tool | **~18** | Local subprocess + script; negligible vs human/LLM |
| B | `sxmc api <petstore openapi> --list` | **~700–850** | Dominated by fetch + parse + network |
| B | `sxmc api … findPetsByStatus` | **~1200–1300** | Same; not CPU-bound on sxmc |
| B | `curl` to known Petstore URL only | **~600** | Lower bound: no spec in process |
| C | `sxmc stdio "sxmc serve --paths …/tests/fixtures" --list` | **~10** | Nested MCP bridge |
| D | `sxmc scan --paths … --skill malicious-skill` | **~11** | Exits non-zero when findings exist (by design) |
| Micro | Local OpenAPI file + tiny `http.server` + `sxmc api … listPets` | **~14** | Reduces internet jitter for method comparison |

**Takeaway:** sxmc adds **small** local overhead on top of I/O. The **big win** is usually **fewer agent turns and smaller prompts**, not microseconds saved on disk.

## Token impact (order-of-magnitude estimates)

Not measured in-repo; use your provider dashboard.

| Workflow | Typical directional effect |
|----------|-----------------------------|
| MCP exposes skills vs **pasting SKILL.md + references each turn** | **Material drop** in repeated **input** tokens on later turns |
| `sxmc api` vs **attaching full OpenAPI JSON** | Often **thousands–tens of thousands** fewer input tokens when the spec would otherwise enter context (large specs can be **100k+ characters**) |
| `sxmc scan` vs **LLM security review** | **Near-zero** LLM tokens for the scan itself vs **hundreds–thousands+** for a serious review |

Rule of thumb for pasted English-ish text: **characters ÷ 4 ≈ rough token equivalents** (vary by tokenizer).

## Reproducing timings

See [E2E_VALIDATION_REPORT.md](E2E_VALIDATION_REPORT.md) for release validation. For a **repeatable local harness**, keep a script that:

1. Runs each command **N** times (e.g. 5).
2. Records **median** wall time (mean is ok; median resists Petstore flakes).
3. Uses a **local OpenAPI + stdlib `http.server`** slice to separate client logic from WAN variance.

Contributions welcome if we add `scripts/benchmark_cli.sh` to the repo later.

## Related docs

- [E2E_VALIDATION_REPORT.md](E2E_VALIDATION_REPORT.md) — v0.1.1 vs v0.1.2 validation
- [SMOKE_TESTS.md](SMOKE_TESTS.md) — MCP transport smoke tests
- [CLIENTS.md](CLIENTS.md) — Cursor / Codex / etc.
