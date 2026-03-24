# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.33] - 2026-03-23

### Added

- `sxmc status --health --exit-code` and `sxmc watch --health --exit-on-unhealthy`
  for CI- and watch-friendly baked integration health gates

### Changed

- `sxmc status --health` now reports latency and slow-entry summaries by source
  type and panel

## [0.2.32] - 2026-03-23

### Added

- `sxmc scaffold ci --from-profile ...` for generating GitHub Actions drift
  workflows from saved CLI profiles

## [0.2.31] - 2026-03-23

### Added

- embedded HMAC-SHA256 bundle signing and verification across
  `inspect bundle-export`, `inspect bundle-verify`, `publish`, and `pull`

## [0.2.30] - 2026-03-23

### Added

- `sxmc inspect corpus-stats` and `sxmc inspect corpus-query` for local
  profile-corpus intelligence workflows

### Changed

- `sxmc wrap` now supports safer execution controls such as `--allow-tool`,
  `--deny-tool`, `--working-dir`, bounded stdout/stderr capture, and optional
  progress heartbeats for long-running wrapped commands
- wrapped CLI schemas now better reflect accepted scalar and repeated values
- `sxmc status --health` now groups checks into MCP/API/spec/graphql panels
- saved-profile inventory and corpus export now include scored quality metadata

## [0.2.29] - 2026-03-23

### Added

- top-level `sxmc watch` for polling status/drift/health over time with
  immediate frame flushing for piped consumers
- `sxmc inspect export-corpus` for exporting saved CLI profiles plus readiness
  and freshness metadata in JSON or NDJSON form

### Changed

- `sxmc status` now includes saved-profile inventory metadata such as stale
  counts, freshness visibility, and agent-doc readiness summaries
- `sxmc status --health` now reports baked health grouped by source type in
  addition to per-entry details

## [0.2.28] - 2026-03-23

### Added

- `sxmc inspect bundle-verify` plus optional SHA-256 enforcement on `pull`
  for safer team bundle distribution

## [0.2.27] - 2026-03-23

### Added

- top-level `sxmc publish` / `sxmc pull` commands for moving profile bundles
  over filesystem paths and HTTP(S) endpoints

## [0.2.26] - 2026-03-23

### Added

- `sxmc status --compare-hosts <hosts>` for explicit host-to-host capability
  comparison across selected AI environments
- team-friendly bundle metadata on `sxmc inspect bundle-export`, preserved by
  `sxmc inspect bundle-import`

## [0.2.25] - 2026-03-23

### Added

- `sxmc inspect bundle-export` / `sxmc inspect bundle-import` for portable
  local profile bundle distribution and recovery

### Changed

- `sxmc status --health` now reports baked-connection health and per-host
  readiness summaries in addition to saved-profile drift
- wrapped CLI tool execution now includes machine-friendly stdout detection in
  the returned execution envelope

## [0.2.24] - 2026-03-22

### Added

- `sxmc wrap <tool>` to inspect a CLI and expose its top-level subcommands as a
  runnable MCP server over stdio or streamable HTTP

### Changed

- validation coverage now includes end-to-end wrapped CLI serving and tool
  execution through the MCP stdio bridge

## [0.2.23] - 2026-03-22

### Added

- `sxmc status` for a unified view of startup files, baked MCP servers, cache
  health, and saved-profile drift under `.sxmc/ai/profiles`
- `sxmc inspect drift [paths...]` for checking saved CLI profiles against the
  currently installed tools

### Fixed

- `scripts/test-sxmc.sh`: CLI inspection “bad summary” heuristic no longer
  false-positives on GNU binutils (`nm`, `strings`, etc.) whose `man`/`--help`
  text includes `Report bugs to <url>`.

## [0.2.22] - 2026-03-22

### Added

- `sxmc inspect diff --format markdown` for PR-friendly human diffs
- `sxmc inspect batch --retry-failed <previous-batch.{json,ndjson}>` to rerun
  only failed command specs from an earlier batch result
- `sxmc inspect migrate-profile <input> [--output migrated.json]` to rewrite
  saved profiles into the current canonical schema
- `sxmc doctor --remove --only <hosts> --from-cli <tool>` to clean up
  generated startup files/snippets for selected hosts

### Changed

- `sxmc inspect batch --output-dir` now supports `--overwrite` and
  `--skip-existing` controls for managing existing saved profile files
- `sxmc inspect batch --output-dir` now writes a `batch-summary.json` manifest
  alongside the individual saved profiles
- validation coverage now includes markdown diffs, retrying failed batch
  entries, output-dir skip-existing behavior, profile migration, and doctor
  cleanup flows

## [0.2.21] - 2026-03-22

### Changed

- `sxmc inspect diff --watch` now flushes each rendered frame immediately so
  piped and other non-interactive consumers can observe updates without waiting
  for process exit
- validation coverage now explicitly includes `inspect diff --watch` with
  NDJSON output and piped stdout behavior

## [0.2.20] - 2026-03-22

### Changed

### Added

- `sxmc inspect diff --before <old.json> --after <new.json>` for comparing two saved CLI profiles without a live tool on PATH
- `sxmc inspect diff --exit-code` for CI-style success/changed signaling
- `sxmc doctor --fix --dry-run` to preview startup-file repairs without writing them
- `sxmc inspect batch --output-dir <dir>` to save each successful profile as a separate JSON file
- `sxmc inspect batch --format ndjson` to emit per-result events plus a final summary record
- `sxmc inspect diff --watch <seconds>` to re-run diffs on an interval until interrupted

### Changed

- `sxmc inspect diff` now emits migration notes when comparing saved profiles from older or provenance-sparse generators
- `sxmc inspect diff --format toon` now renders removed deltas as well as added ones
- the validation suite now covers saved-vs-saved diffing, diff exit codes, doctor dry-runs, batch output directories, and batch NDJSON streaming

## [0.2.19] - 2026-03-22

### Changed

- `sxmc inspect diff` now tolerates older or partially-missing saved profile fields instead of failing on strict schema decoding
- `sxmc doctor --fix` and related write flows now print a summary line with created, updated, skipped, and removed counts
- the validation suite now covers diffing a legacy-ish saved profile and the new doctor repair summary output

## [0.2.18] - 2026-03-22

### Changed

- `sxmc inspect diff` now gives an explicit compact-profile error telling you to save a full profile without `--compact`
- `sxmc inspect diff --format toon` now renders a human-oriented diff summary
- docs now clarify that YAML/TOML batch depth overrides populate `subcommand_profiles` in full output and only summary counts in compact output
- RFC3339 `--since` support is now covered in the validation suite
- `sxmc doctor --fix` now distinguishes created, updated, and skipped-unchanged outputs

## [0.2.17] - 2026-03-22

### Added

- `sxmc doctor --check --fix --only <hosts> --from-cli <tool>` to repair missing startup files for the selected hosts
- `sxmc inspect diff <tool> --before <profile.json>` to compare a live CLI against a saved profile
- `sxmc inspect cache-warm ...` to pre-populate cached CLI profiles without printing full profile payloads

### Changed

- `sxmc inspect batch --from-file` now supports YAML/TOML tool lists with per-command depth overrides in addition to plain-text lists
- `sxmc inspect batch --since <timestamp>` now skips tools whose executable has not changed since the given Unix-seconds or RFC3339 timestamp
- generated bash completions now have integration coverage for top-level subcommands and nested `inspect batch` options

## [0.2.16] - 2026-03-22

### Added

- `sxmc doctor --check --only <hosts>` to gate only the AI hosts a repo actually uses
- `sxmc inspect cache-invalidate <pattern> --dry-run` to preview exact or glob cache matches before removal

### Changed

- `sxmc inspect batch` now auto-enables stderr progress notes for larger batch runs on a real terminal
- `sxmc inspect batch --from-file` now explicitly documents comment lines, blank lines, trailing whitespace, and preserved inline arguments
- batch `--format toon` now has explicit validation coverage for failure details so TOON output stays self-contained

## [0.2.15] - 2026-03-22

### Added

- `sxmc inspect batch --from-file <path>` to load larger command lists from a file
- `sxmc doctor --check` for CI-style startup-file validation

### Changed

- `sxmc inspect cache-invalidate` now preserves non-targeted cache entries and reports before/after cache metrics accurately
- `sxmc inspect cache-invalidate` now supports glob-style patterns such as `git*` or `c*`
- `sxmc inspect batch --progress` now forces stderr progress notes for larger batch runs
- the comprehensive `scripts/test-sxmc.sh` gate now covers doctor check mode, file-driven batch inspection, exact vs pattern cache invalidation, and the fixed invalidate semantics

## [0.2.14] - 2026-03-22

### Added

- `sxmc inspect cache-clear` to wipe cached CLI profiles without manually deleting cache files
- `sxmc inspect cache-invalidate <tool>` to selectively invalidate cached CLI profiles for one command

### Changed

- `sxmc inspect batch` now supports `--parallel <N>` and runs bounded parallel worker threads instead of always inspecting sequentially
- batch inspection now emits summary-oriented `--format toon` output instead of dumping nested raw JSON inside a TOON envelope
- the comprehensive `scripts/test-sxmc.sh` gate now covers batch parallelism, cache invalidation/clear, and batch TOON rendering

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
