# Sumac (`sxmc`) v0.2.45 Test Suite Report

**Version:** 0.2.45  
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

This pass covers the full shipped surface through `v0.2.45`, including:

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
- the explicit `1.x` stability/support sweep across README, contract, validation, and operations docs
- side-by-side workflow comparisons and benchmark runs

---

## Highlights

- `94` installed CLI tools parsed successfully
- `0` parse failures
- `0` bad summaries
- `296` total tests passed
- the stable first-run lifecycle is now explicit in the docs:
  - `setup`
  - `add`
  - `status`
  - `sync`
- the support promise for the upcoming `1.x` line is now documented:
  - stable onboarding and maintenance commands
  - additive machine-readable output evolution
  - explicit best-effort boundaries for inferred metadata

---

## Benchmark Snapshot

Median snapshots from the run:

- warm CLI inspection: `6–7ms`
- `wrap git -> stdio --list`: `17ms`
- bundle export (5 profiles): `16ms`
- bundle export + HMAC sign: `20ms`
- full `inspect -> scaffold -> init-ai` pipeline for 5 CLIs: `98ms`

---

## Notes

- This report supersedes the previous `v0.2.44` “latest validation” references.
- The product contract, README, usage guide, operations checklist, and validation guide now all tell the same `1.x` story instead of leaving the stability promise implicit.
