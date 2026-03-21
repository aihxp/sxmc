# Documentation

Additional documentation for sxmc developers and maintainers.

| Document | Description |
|----------|-------------|
| [CLIENTS.md](CLIENTS.md) | Client setup for Codex, Cursor, Gemini CLI, Claude Code, and compatibility matrix |
| [COMPATIBILITY_MATRIX.md](COMPATIBILITY_MATRIX.md) | Dated compatibility ledger for supported clients and remote MCP consumers |
| [PRODUCT_CONTRACT.md](PRODUCT_CONTRACT.md) | Explicit support boundary: what `sxmc` guarantees, degrades gracefully, or leaves out of scope |
| [CLI_SURFACES.md](CLI_SURFACES.md) | Concrete design for `CLI -> AI surfaces`, including provenance, depth limits, and review/apply rules |
| [CONNECTION_EXAMPLES.md](CONNECTION_EXAMPLES.md) | Copy-pasteable stdio, HTTP, and nested bridge examples |
| [DEPLOYMENT.md](DEPLOYMENT.md) | Hosted `/mcp` deployment, auth, health checks, and operations notes |
| [RELEASING.md](RELEASING.md) | Release process, version bumping, and crates.io publishing |
| [DISTRIBUTION.md](DISTRIBUTION.md) | Distribution channels: npm wrapper, Homebrew formula, asset naming |
| [SMOKE_TESTS.md](SMOKE_TESTS.md) | Startup sanity and client smoke procedures (automated and manual) |
| [E2E_VALIDATION_REPORT.md](E2E_VALIDATION_REPORT.md) | E2E validation: v0.1.1 regressions, 0.1.2 fixes, and the manual test matrix |
| [BENCHMARK_RUN_v0.1.6.md](BENCHMARK_RUN_v0.1.6.md) | v0.1.6 crates.io benchmark + `cargo test` counts + `scripts/benchmark_cli.sh` |
| [BENCHMARK_RUN_v0.1.5.md](BENCHMARK_RUN_v0.1.5.md) | Prior benchmark capture (v0.1.5) |
| [BENCHMARK_RUN_v0.1.3.md](BENCHMARK_RUN_v0.1.3.md) | Older benchmark capture (v0.1.3) |
| [REAL_WORLD_SKILLS_AND_MCP_REPORT.md](REAL_WORLD_SKILLS_AND_MCP_REPORT.md) | Five skills + five npm MCPs; promptless `--list`; multi-invocation / “dialog” notes (v0.1.6) |
| [VALUE_AND_BENCHMARK_FINDINGS.md](VALUE_AND_BENCHMARK_FINDINGS.md) | Value proposition, benchmark scope, startup benchmark helper, token estimation guidance |
| [MCP_TO_CLI_VERIFICATION.md](MCP_TO_CLI_VERIFICATION.md) | MCP → CLI (`stdio` / `http`) verification notes, bridge contract, and manual checks |
| [SKILLS_TO_MCP_TO_CLI_SAMPLES.md](SKILLS_TO_MCP_TO_CLI_SAMPLES.md) | Sample terminal output: skills→MCP→CLI vs `skills run` (nested `serve` + fixtures) |
| [LAUNCH.md](LAUNCH.md) | Release notes template, pitch copy, and announcement drafts |
