# Sumac (`sxmc`) v0.2.44 Test Suite Report

**Version:** 0.2.44  
**Platform:** macOS Darwin arm64  
**Date:** 2026-03-25  
**Test script:** `scripts/test-sxmc.sh`

---

## Results

| Metric | Value |
|---|---|
| Total tests | 296 |
| Passed | 296 |
| Failed | 0 |
| Skipped | 0 |
| CLI tools parsed | 94 |
| CLI tools failed to parse | 0 |
| Bad summaries | 0 |
| Benchmark iterations | 5 per measurement |

**ALL 296 TESTS PASSED — ZERO FAILURES, ZERO SKIPS**

---

## Scope

This pass covers the full shipped surface through `v0.2.44`, including:

- CLI inspection, compact mode, caching, diffing, drift, sync, and watch
- scaffold generation and AI host initialization across 10 clients
- skill discovery, info, execution, and MCP serving
- MCP bake flows, stdio/http bridges, wrap, wrapped execution telemetry, and interactive/TUI filtering
- OpenAPI API mode, GraphQL discovery, GraphQL schema snapshots, and GraphQL diffing
- codebase discovery, snapshotting, and diffing
- database discovery for SQLite/PostgreSQL, including snapshot output
- traffic discovery from HAR and saved `curl` history, plus traffic snapshots and diffing
- publish/pull, bundle export/import/verify/signing
- corpus export/query/stats, registry flows, trust policy, and known-good selection
- doctor, status, health gates, host comparison, onboarding recovery, and one-step onboarding flows
- local sync reconciliation with `.sxmc/state.json` tracking and status integration
- side-by-side workflow comparisons and benchmark runs

---

## Highlights

- `94` installed CLI tools parsed successfully
- `0` parse failures
- `0` bad summaries
- `296` total tests passed
- `sxmc sync` now closes the local maintenance loop:
  - preview-first reconciliation
  - `--apply` for saved profiles and AI-host artifacts
  - `--check` for CI-style drift gating
- `sxmc status` now surfaces `sync_state`, so repos can see when local
  reconciliation last ran and which commands still need sync
- drift checks now infer the effective saved-profile depth from nested saved
  subcommand profiles, avoiding false drift after depth-expanded refreshes

---

## Benchmark Snapshot

Median snapshots from the run:

- warm CLI inspection: `7–8ms`
- `wrap git -> stdio --list`: `16ms`
- bundle export (5 profiles): `16ms`
- bundle export + HMAC sign: `20ms`
- full `inspect -> scaffold -> init-ai` pipeline for 5 CLIs: `105ms`

---

## Notes

- This report supersedes the previous `v0.2.43` “latest validation” references.
- The practical pre-`1.0.0` workflow gap around local reconciliation is now
  closed in-product instead of being left to manual `inspect`/`init ai` reruns.
