# Validation Guide

This guide consolidates the release checklist, compatibility notes, smoke
tests, and benchmark summary.

For a concrete maintainer validation pass against **`2.0.0`**, see
[`VALIDATION_RUN_v2.0.0.md`](VALIDATION_RUN_v2.0.0.md). Older snapshots:
[`VALIDATION_RUN_v0.1.9.md`](VALIDATION_RUN_v0.1.9.md),
[`VALIDATION_RUN_v0.1.8.md`](VALIDATION_RUN_v0.1.8.md).

## What To Run Before A Release

From the repo root:

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo package --allow-dirty
bash scripts/certify_release.sh target/debug/sxmc tests/fixtures
```

Optional real-world MCP pass when Node and network are available:

```bash
bash scripts/smoke_real_world_mcps.sh target/debug/sxmc
```

## Coverage Summary

The maintained product coverage now centers on three layers:

- automated tests in `cargo test`
- release certification via `scripts/certify_release.sh`
- optional real-world MCP smoke via `scripts/smoke_real_world_mcps.sh`

High-value scenarios covered in this stack include:

- `skills -> MCP`
- `MCP -> CLI` over stdio and HTTP
- baked `sxmc mcp` workflows
- auth-required hosted MCP
- `/healthz`
- `serve --watch`
- local OpenAPI and GraphQL flows
- `skills create`
- promptless or resource-less third-party MCP servers
- zero-argument tool interoperability

## Compatibility Notes

`sxmc` has been exercised against:

- Codex-style local MCP configuration
- Cursor-style local and remote MCP configuration
- Gemini CLI-style local and remote MCP configuration
- Claude Code-style local and remote MCP configuration
- official external MCP servers such as:
  - `@modelcontextprotocol/server-everything`
  - `@modelcontextprotocol/server-memory`
  - `@modelcontextprotocol/server-filesystem`
  - `@modelcontextprotocol/server-sequential-thinking`
  - `@modelcontextprotocol/server-github`

The practical support boundary is defined in
[`PRODUCT_CONTRACT.md`](PRODUCT_CONTRACT.md).

## Benchmarks

Local one-shot paths are consistently fast enough that they are not the main
product concern. The more important product value is:

- fewer agent turns
- smaller prompt payloads
- on-demand MCP schema inspection instead of eager schema loading

Benchmarks are useful for regression sanity, not as proof of broad client
compatibility.

## Startup Sanity

Quick startup checks:

```bash
bash scripts/startup_smoke.sh target/debug/sxmc
python3 scripts/benchmark_startup.py /tmp/sxmc-startup-benchmark.md
```

## Current Read

The current validation posture is:

- release certification is scripted
- real-world MCP smoke is scripted
- broad end-to-end paths are covered in tests
- remaining work should come from real user findings, not speculative expansion

## Latest maintainer snapshot

**[VALIDATION_RUN_v2.0.0.md](VALIDATION_RUN_v2.0.0.md)** — **2.0.0** pass: tests (**123**), certify + smoke, benchmarks, five skills, five MCPs, **JSON / stderr notes**, promptless multi-invocation, **MCP → CLI**, **`sxmc mcp`**, **`sxmc mcp session`**, **Cursor-style stdio simulation (per USAGE)**, and **warnings inventory**.

Repeated standalone **`sxmc stdio …`** invocations do **not** share MCP session memory. For continuity, use **`sxmc mcp session <server>`** (see validation run §9).
