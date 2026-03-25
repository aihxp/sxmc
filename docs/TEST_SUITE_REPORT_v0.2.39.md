# Sumac (`sxmc`) v0.2.39 Test Suite Report

**Version:** 0.2.39  
**Platform:** macOS Darwin arm64  
**Date:** 2026-03-24  
**Test script:** `scripts/test-sxmc.sh`

---

## Results

| Metric | Value |
|---|---|
| Total tests | 250 |
| Passed | 250 |
| Failed | 0 |
| Skipped | 0 |
| CLI tools parsed | 94 |
| CLI tools failed to parse | 0 |
| Bad summaries | 0 |
| Benchmark iterations | 5 per measurement |

**ALL 250 TESTS PASSED — ZERO FAILURES, ZERO SKIPS**

---

## Scope

This pass covers the full shipped surface through `v0.2.39`, including:

- CLI inspection, compact mode, caching, diffing, drift, and watch
- scaffold generation and AI host initialization across 10 clients
- skill discovery, info, execution, and MCP serving
- MCP bake flows, stdio/http bridges, wrap, and wrapped execution telemetry
- OpenAPI API mode, publish/pull, bundle export/import/verify/signing
- corpus export/query/stats, registry flows, trust policy, and known-good selection
- doctor, status, health gates, and host comparison
- side-by-side workflow comparisons and benchmark runs

---

## Highlights

- `94` installed CLI tools parsed successfully
- `0` parse failures
- `0` bad summaries
- `250` total tests passed
- warm cache results stayed in the single-digit millisecond range
- all publish/pull, registry, trust, and known-good flows passed
- the repo-local validation script now prefers the local build before any installed `sxmc`, preventing stale-binary false failures

---

## Benchmark Snapshot

Median snapshots from the run:

- warm CLI inspection: `6–8ms`
- `wrap git -> stdio --list`: `19ms`
- bundle export (5 profiles): `15ms`
- bundle export + HMAC sign: `19ms`
- full `inspect -> scaffold -> init-ai` pipeline for 5 CLIs: `103ms`

---

## Notes

- This report supersedes the previous `v0.2.38` “latest validation” references.
- The product, repo, and metadata are now aligned under the Sumac brand while preserving the `sxmc` crate and CLI command identifiers.
