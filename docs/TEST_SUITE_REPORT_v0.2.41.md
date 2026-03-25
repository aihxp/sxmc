# Sumac (`sxmc`) v0.2.41 Test Suite Report

**Version:** 0.2.41  
**Platform:** macOS Darwin arm64  
**Date:** 2026-03-25  
**Test script:** `scripts/test-sxmc.sh`

---

## Results

| Metric | Value |
|---|---|
| Total tests | 275 |
| Passed | 275 |
| Failed | 0 |
| Skipped | 0 |
| CLI tools parsed | 94 |
| CLI tools failed to parse | 0 |
| Bad summaries | 0 |
| Benchmark iterations | 5 per measurement |

**ALL 275 TESTS PASSED — ZERO FAILURES, ZERO SKIPS**

---

## Scope

This pass covers the full shipped surface through `v0.2.41`, including:

- CLI inspection, compact mode, caching, diffing, drift, and watch
- scaffold generation and AI host initialization across 10 clients
- skill discovery, info, execution, and MCP serving
- MCP bake flows, stdio/http bridges, wrap, and wrapped execution telemetry
- OpenAPI API mode, GraphQL discovery, GraphQL schema snapshots, and GraphQL diffing
- codebase discovery, snapshotting, and diffing
- database discovery for SQLite/PostgreSQL, including snapshot output
- traffic discovery from HAR and saved `curl` history, plus traffic snapshots and diffing
- publish/pull, bundle export/import/verify/signing
- corpus export/query/stats, registry flows, trust policy, and known-good selection
- doctor, status, health gates, and host comparison
- side-by-side workflow comparisons and benchmark runs

---

## Highlights

- `94` installed CLI tools parsed successfully
- `0` parse failures
- `0` bad summaries
- `275` total tests passed
- `discover db` now supports `--output` snapshots in addition to SQLite/PostgreSQL inspection
- discovery help text now accurately reflects PostgreSQL and curl-history support
- the validation script numbering and guide are aligned with the current discovery section layout

---

## Benchmark Snapshot

Median snapshots from the run:

- warm CLI inspection: `4–5ms`
- `wrap git -> stdio --list`: `11ms`
- bundle export (5 profiles): `6ms`
- bundle export + HMAC sign: `6ms`
- full `inspect -> scaffold -> init-ai` pipeline for 5 CLIs: `70ms`

---

## Notes

- This report supersedes the previous `v0.2.40` “latest validation” references.
- The practical pre-`1.0.0` backlog remains limited to intentionally deferred larger bets and release/stability work, not missing core feature coverage.
