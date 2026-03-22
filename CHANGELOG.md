# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.13] - 2026-03-22

### Added

- `sxmc inspect batch` for multi-command CLI inspection in a single invocation
- `sxmc inspect cache-stats` for profile-cache inventory and size visibility
- `sxmc doctor --human` to force the readable startup report off-TTY

### Changed

- `scripts/test-sxmc.sh` is now part of release certification and Unix CI coverage
- CI now explicitly validates Ubuntu, macOS, and Windows product paths, including Windows smoke checks for `doctor`, compact inspection, and cache stats
- human `doctor` output now includes cache entry counts and size in addition to startup-file and next-step guidance

## [0.2.12] - 2026-03-22

### Changed

- docs and validation notes now clarify that `sxmc inspect cli` executes real subprocesses and therefore expects an actual executable on `PATH` (or an explicit path), not a shell-only alias or function

## [0.2.11] - 2026-03-22

### Changed

- invalid profile-file inputs for `inspect profile` and every `--from-profile` scaffold now explain that `sxmc` expected a real CLI surface profile from `sxmc inspect cli ...`

## [0.2.10] - 2026-03-22

### Changed

- recursive CLI inspection now surfaces deeper exploration more clearly, including `--depth 2` guidance for multi-layer CLIs and progress notes while nested subcommand help is collected
- bake validation errors now include source-type-specific guidance for stdio, HTTP MCP, OpenAPI, and GraphQL targets, plus explicit `--skip-validate` fallback guidance when you intentionally want to save an offline target

## [0.2.9] - 2026-03-22

### Changed

- Homebrew inspection now merges real `GLOBAL OPTIONS` like `--debug`, `--quiet`, `--verbose`, and `--help` back into the richer `brew commands` profile instead of dropping to zero top-level options
- human `sxmc doctor` output now groups present vs missing startup files, shows the profile cache directory, and recommends `sxmc serve --paths <dir>` when local skills or prompts should be exposed over MCP
- generated agent docs, skills, and `llms.txt` exports now show subcommand counts plus overflow hints instead of silently truncating larger CLIs after the first few entries
- interactive CLI inspection now emits lightweight stderr progress notes on cache misses and slower supplemental probes like `brew commands`
- CLI inspection profiles are cached against the executable fingerprint, bake create/update validate saved sources by default, and broader named-secret patterns are detected in scans
- the repo now ships generated `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, Cursor rules, and Copilot instructions as a self-dogfooding example

## [0.2.8] - 2026-03-22

### Added

- `sxmc doctor` now reports startup-discovery status plus recommended first commands for unfamiliar CLIs, MCP servers, APIs, startup-doc setup, and skill scans
- `sxmc inspect cli --compact` now returns a lower-context summary view for agent-friendly CLI inspection

### Changed

- CLI inspection now recognizes multi-section Cobra command groups like `GITHUB ACTIONS COMMANDS` and `ALIAS COMMANDS` again, which restores the full top-level `gh` command set
- version-banner handling is broader for mixed-case tools like `unzip`, so `man`-page `NAME` summaries win over release-banner text more reliably
- inspected CLI profiles are now cached against the executable fingerprint so repeated lookups reuse stable profiles until the binary changes
- bake create/update now validate saved sources by default and fail early on broken MCP/API targets unless `--skip-validate` is used
- secret detection now catches more named token/secret assignment patterns such as short OpenAI-style keys and generic `TOKEN=` / `SECRET=` forms
- generated CLI-to-AI agent docs now explicitly teach an `sxmc`-first workflow for unknown CLIs, MCP servers, and APIs
- the repo now checks in generated `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, Cursor rules, and Copilot instructions as a self-dogfooding example
- README, usage docs, demo flow, and integration coverage now reinforce the `sxmc`-first onboarding path for unknown surfaces

## [0.2.7] - 2026-03-22

### Changed

- CLI inspection now avoids treating man-page prose and interactive command tables as top-level CLI subcommands, which removes false positives for tools like `cat`, `lsof`, and `dc`
- summary selection now prefers real command descriptions over vendor banners, bug-report lines, option tables, and overstrike-formatted help text, which cleans up tools like `lsof`, `gzip`, `man`, `ping`, `dig`, `less`, and `more`
- `brew` inspection now supplements `--help` with `brew commands`, recovering a much broader real command set without carrying over the giant manual-page option wall
- help/man detection is stricter, so ordinary clap-style help for tools like `cargo` and `sxmc` is no longer mistaken for man-page output
- parser regression coverage now includes man-example false positives, title-case `Name` sections, overstrike stripping, and command-name description lines

## [0.2.6] - 2026-03-22

### Changed

- CLI inspection now filters bogus subcommands much more aggressively when parsing rich `--help` and `man` output, which fixes false positives in tools like `rg`, `grep`, and `python3`
- man-page parsing now prefers wrapped `NAME` descriptions over version banners and attribution text, which cleans up summaries for tools like `grep`, `cal`, `zip`, and `unzip`
- synopsis-derived flag extraction now recovers concise option sets for sparse man-page tools like `awk`
- Homebrew-style command sections now recover real top-level commands instead of collapsing into option-heavy profiles with no subcommands
- parser regression coverage now includes `rg`, wrapped man-page summaries, `brew`, and `awk`

## [0.2.5] - 2026-03-22

### Changed

- CLI inspection now merges richer help probing with targeted `man`-page supplementation instead of letting sparse manual output replace higher-signal `--help` results
- top-level option recovery improved for CLIs like `gh` and `rustup`, so lightweight global flags are preserved alongside subcommand extraction
- summary extraction is stricter about skipping generic section banners and option prose, which produces cleaner profiles and downstream AI artifacts for tools like `node`, `npm`, and `python3`
- Python-style environment variables are now filtered out of subcommand detection during CLI inspection
- integration coverage now includes real `gh`, `rustup`, `python3`, and `npm` parser regressions in addition to the earlier `git`, `cargo`, and `node` cases

## [0.2.4] - 2026-03-22

### Added

- `sxmc inspect cli --depth 1` for recursive top-level CLI inspection with nested `subcommand_profiles`
- `sxmc init ai --remove` to clean up previously applied CLI-to-AI startup artifacts
- `bake create/update --base-dir` so stdio bakes can preserve a working directory for relative sources
- integration coverage for recursive inspection, low-confidence gating, CLI-to-AI removal, base-dir bakes, and missing-command install hints

### Changed

- CLI inspection now falls back to `man` pages on Unix-like systems when `--help` is sparse or unsupported, which improves BSD/macOS tools like `ls`
- startup-doc generation now blocks low-confidence CLI profiles by default and requires `--allow-low-confidence` to force low-signal outputs
- `inspect`, `api --list`, and `mcp servers` now default to structured JSON when stdout is non-interactive, which makes them easier to pipe into agents and scripts
- stdio MCP spawn failures now include install-oriented hints instead of only low-level OS errors
- baked stdio MCP connections now honor their stored base directory when reconnecting

## [0.2.3] - 2026-03-21

### Changed

- CLI inspection now skips raw usage continuations when picking summaries, which cleans up generated profiles, agent docs, and skill scaffolds for tools like `git`
- grouped command detection is broader and more precise, covering `CORE COMMANDS`, `ADDITIONAL COMMANDS`, npm-style command lists, and Git-style command groupings without inventing bogus subcommands
- option parsing now handles wrapped and colon-delimited help layouts more cleanly, improving `node` and `python3` profiles
- CLI profile sanitization now strips local absolute paths from generated summaries and descriptions so host-specific install paths do not leak into AI-facing artifacts
- inspection safely probes richer help variants like `--help-all` when the CLI advertises them and chooses the higher-signal result
- regression coverage now includes real `git`, `cargo`, and `node` inspection behavior in addition to parser fixtures for `gh`, `git`, `npm`, `node`, `python3`, and `cargo`

## [0.2.2] - 2026-03-21

### Added

- `sxmc completions <shell>` for shell completion generation
- HTTP serving guardrails for max concurrency and request body size
- `docs/ARCHITECTURE.md` for contributors and maintainers
- `docs/DEMO.md` plus `scripts/demo.sh` for short repeatable demos

### Changed

- bake persistence is now atomic instead of rewriting `bakes.json` in place
- `http`, `api`, `spec`, and `graphql` now support `--timeout-seconds`
- baked HTTP/API/spec/graphql configs can persist timeout settings
- watch mode now prefers filesystem events and falls back to polling if needed
- lock poisoning now emits warnings instead of recovering silently
- `main.rs` was split by moving clap definitions into `src/cli_args.rs` and shared handlers into `src/command_handlers.rs`
- `cli_surfaces.rs` was split into model, inspect, render, and materialize modules
- `README.md` was trimmed to focus on install, quick start, and the core value proposition
- operations docs now include release cadence and hosted HTTP guardrail guidance

## [0.2.1] - 2026-03-21

### Added

- `CLI -> AI` startup scaffolds with:
  - `sxmc inspect cli <command>`
  - `sxmc init ai --from-cli <command> --client <profile>`
  - `sxmc scaffold skill --from-profile ...`
  - `sxmc scaffold agent-doc --from-profile ...`
  - `sxmc scaffold client-config --from-profile ...`
  - `sxmc scaffold mcp-wrapper --from-profile ...`
  - `sxmc scaffold llms-txt --from-profile ...`
- full-coverage CLI-to-AI generation with `--coverage full` plus explicit `--host` selection for safe apply behavior
- native host targets for:
  - Claude Code
  - Cursor
  - Gemini CLI
  - GitHub Copilot
  - Continue
  - OpenCode
  - JetBrains AI Assistant
  - Junie
  - Windsurf
- dedicated CLI-to-AI compatibility matrix in `docs/CLI_TO_AI_COMPATIBILITY.md`

### Changed

- CLI-to-AI apply mode now updates only explicitly selected host targets during full-coverage runs and sidecars the rest
- OpenCode config generation now uses its native JSON shape
- shared `AGENTS.md` targets now keep portable, OpenCode, and OpenAI/Codex managed blocks side by side during multi-host apply runs
- release and usage docs now describe the broader CLI-to-AI host coverage and optional `llms.txt` export

## [0.2.0] - 2026-03-21

### Added

- `sxmc mcp session <server>` for explicit stateful multi-step MCP workflows over one baked connection
- stateful MCP fixture coverage proving session memory survives repeated tool calls inside one session

### Changed

- `api/spec/graphql --list` now honors structured output flags like `--format json`, `--pretty`, and `--format toon`
- `scan --json` now emits a single machine-readable JSON document across multi-target scans
- product and validation docs now point stateful MCP users to `sxmc mcp session` instead of treating session continuity as an unsupported terminal path
- `sxmc` is now released as a stable `0.2.0` surface for skills, MCP, and API workflows
- release metadata and package docs now align to `0.2.0`

## [0.1.9] - 2026-03-21

### Added

- `sxmc mcp session <server>` for explicit stateful multi-step MCP workflows over one baked connection
- stateful MCP fixture coverage proving session memory survives repeated tool calls inside one session

### Changed

- product and validation docs now point stateful MCP users to `sxmc mcp session` instead of treating session continuity as an unsupported terminal path
- release metadata and package docs now align to `0.1.9`

## [0.1.8] - 2026-03-21

### Fixed

- Windows CI bake-based MCP integration tests now use isolated bake names so parallel CLI integration runs do not collide on shared baked config state
- release metadata and package docs now align to `0.1.8`

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
- Homebrew formula and distribution guidance under `docs/OPERATIONS.md`

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
[0.1.8]: https://github.com/aihxp/sxmc/compare/v0.1.7...v0.1.8
[0.1.9]: https://github.com/aihxp/sxmc/compare/v0.1.8...v0.1.9
[0.2.0]: https://github.com/aihxp/sxmc/compare/v0.1.9...v0.2.0
[0.2.1]: https://github.com/aihxp/sxmc/compare/v0.2.0...v0.2.1
[0.2.2]: https://github.com/aihxp/sxmc/compare/v0.2.1...v0.2.2
[0.2.3]: https://github.com/aihxp/sxmc/compare/v0.2.2...v0.2.3
[0.1.0]: https://github.com/aihxp/sxmc/releases/tag/v0.1.0
