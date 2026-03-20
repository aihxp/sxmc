# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.6] - 2026-03-20

### Added

- startup sanity script at `scripts/startup_smoke.sh`
- cross-platform startup benchmark helper at `scripts/benchmark_startup.py`

### Changed

- CI now runs `sxmc --version` and `sxmc --help` on every OS before the full test suite
- docs now separate benchmark timing claims from compatibility and smoke validation
- release metadata and package docs now align to `0.1.6`

## [0.1.7] - 2026-03-20

### Added

- explicit product contract doc for supported, graceful, and out-of-scope behavior
- release certification script for local startup, bridge, package, and wrapper checks
- real-world MCP smoke script for named official MCP servers

### Fixed

- zero-argument MCP tool calls now send `{}` by default for stricter servers

### Changed

- compatibility docs now record named external MCP servers, not just client categories
- release metadata and package docs now align to `0.1.7`

## [0.1.5] - 2026-03-20

### Added

- capability-aware MCP introspection with `sxmc stdio --describe` / `sxmc http --describe`
- per-tool schema/help output with `--describe-tool`
- explicit surface listing flags: `--list-tools`, `--list-prompts`, and `--list-resources`

### Fixed

- `--list` no longer fails on prompt-less MCP servers that return `-32601` for prompt listing
- HTTP MCP integration tests now wait for server readiness instead of relying on fixed sleeps

### Changed

- MCP bridge clients now use paginated `list_all_*` helpers for fuller server discovery
- release metadata and package docs now align to `0.1.5`

## [0.1.4] - 2026-03-20

### Added

- `sxmc serve --watch` for polling-based skill reloads during local development
- benchmark and launch docs for the current patch line, including reproducible CLI benchmark guidance

### Fixed

- Windows stdio command parsing fallback
- redundant skill discovery during CLI bridge flows

### Changed

- release metadata and package docs now align to `0.1.4`
- benchmark findings now explicitly call out that `--watch` is outside the default one-shot benchmark path

## [0.1.3] - 2026-03-20

### Added

- `--prompt` and `--resource` support for `sxmc stdio` and `sxmc http`
- safer stdio spawning with JSON-array command specs and `--cwd`
- inventory counts in the remote `/healthz` endpoint
- optional TOON-style structured output for `api`, `spec`, and `graphql`
- checksum verification in the npm wrapper installer
- Homebrew tap guidance under `packaging/homebrew/README.md`

### Changed

- MCP-to-CLI docs now describe the full bridge contract for tools, prompts, and resources
- distribution docs now treat crates.io and GitHub Releases as canonical, with npm/Homebrew as convenience channels

## [0.1.2] - 2026-03-20

### Added

- Regression coverage for project-local `.claude/skills` when bridged through
  `sxmc stdio "sxmc serve"`
- End-to-end validation notes for the `0.1.1` regressions and `0.1.2` fixes
- Patch release notes and release guidance for the `0.1.2` line

### Fixed

- Project-local skill script execution now resolves to absolute paths without
  requiring an explicit `--paths`
- OpenAPI 3 specs with relative `servers[0].url` values now resolve correctly
  against the spec source URL
- The new project-local skill regression test now uses OS-native temp scripts,
  so CI stays green on Windows as well as Unix

## [0.1.1] - 2026-03-20

### Added

- Crate-level documentation for docs.rs
- Links to crates.io and docs.rs in README

### Fixed

- Release workflow macOS Intel runner mapping

## [0.1.0] - 2026-03-20

### Added

- Skill discovery, parsing, and MCP serving (`sxmc serve`)
- Hybrid skill retrieval tools: `get_available_skills`, `get_skill_details`, `get_skill_related_file`
- Remote streamable HTTP MCP serving at `/mcp`
- Bearer token auth and header-based auth for remote MCP endpoints
- Health endpoint (`/healthz`) for hosted deployments
- MCP client for stdio and HTTP transports (`sxmc stdio`, `sxmc http`)
- OpenAPI and GraphQL auto-detection and CLI execution (`sxmc api`, `sxmc spec`, `sxmc graphql`)
- Security scanning for skills and MCP servers (`sxmc scan`)
- Connection baking for saved configs (`sxmc bake`)
- Secret resolution from environment variables (`env:`) and files (`file:`)
- Skill generation from API specs (`sxmc skills create`)
- File-based cache with TTL
- GitHub Actions CI and multi-platform release workflows
- npm wrapper and Homebrew formula scaffolds
- README, CONTRIBUTING, and LICENSE documentation

### Fixed

- Argument substitution order to prevent indexed replacement corruption
- Tool name extraction for multi-dot filenames
- API auth flow alignment with CLI docs

[0.1.1]: https://github.com/aihxp/sxmc/compare/v0.1.0...v0.1.1
[0.1.2]: https://github.com/aihxp/sxmc/compare/v0.1.1...v0.1.2
[0.1.3]: https://github.com/aihxp/sxmc/compare/v0.1.2...v0.1.3
[0.1.4]: https://github.com/aihxp/sxmc/compare/v0.1.3...v0.1.4
[0.1.5]: https://github.com/aihxp/sxmc/compare/v0.1.4...v0.1.5
[0.1.6]: https://github.com/aihxp/sxmc/compare/v0.1.5...v0.1.6
[0.1.7]: https://github.com/aihxp/sxmc/compare/v0.1.6...v0.1.7
[0.1.0]: https://github.com/aihxp/sxmc/releases/tag/v0.1.0
