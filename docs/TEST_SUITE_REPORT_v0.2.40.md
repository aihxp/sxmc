# Sumac (`sxmc`) v0.2.40 Test Suite Report

**Version:** 0.2.40  
**Platform:** macOS Darwin arm64  
**Date:** 2026-03-24  
**Test script:** `scripts/test-sxmc.sh`

---

## Results

| Metric | Value |
|---|---|
| Total tests | 257 |
| Passed | 257 |
| Failed | 0 |
| Skipped | 0 |
| CLI tools parsed | 94 |
| CLI tools failed to parse | 0 |
| Bad summaries | 0 |
| Benchmark iterations | 5 per measurement |

**ALL 257 TESTS PASSED — ZERO FAILURES, ZERO SKIPS**

---

## Scope

This pass covers the full shipped surface through `v0.2.40`, including:

- CLI inspection, compact mode, caching, diffing, drift, and watch
- scaffold generation and AI host initialization across 10 clients
- skill discovery, info, execution, and MCP serving
- MCP bake flows, stdio/http bridges, wrap, and wrapped execution telemetry
- OpenAPI API mode, GraphQL discovery, GraphQL schema snapshots, and GraphQL diffing
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
- `257` total tests passed
- GraphQL discovery now has saved-schema snapshots and drift detection
- traffic discovery now accepts both HAR captures and replayable `curl`/shell request history
- warm cache results stayed in the single-digit millisecond range

---

## Benchmark Snapshot

Median snapshots from the run:

- warm CLI inspection: `6–8ms`
- `wrap git -> stdio --list`: `20ms`
- bundle export (5 profiles): `15ms`
- bundle export + HMAC sign: `20ms`
- full `inspect -> scaffold -> init-ai` pipeline for 5 CLIs: `101ms`

---

## Notes

- This report supersedes the previous `v0.2.39` “latest validation” references.
- The practical pre-`1.0.0` discovery backlog is now closed except for the intentionally deferred larger bets such as live traffic capture and GUI/app discovery.
