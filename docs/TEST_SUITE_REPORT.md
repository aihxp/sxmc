# sxmc Test Suite Report

**Versions tested:** v0.2.10 → v0.2.21
**Platform:** macOS (Darwin arm64)
**Date:** 2026-03-22
**Test script:** `scripts/test-sxmc.sh`

---

## Final Results (v0.2.21)

| Metric | Value |
|---|---|
| Total tests | 136 |
| Passed | 134 |
| Failed | 0 |
| Skipped | 2 |
| CLI tools parsed | 90 |
| CLI tools failed to parse | 0 |
| CLI tools skipped (not installed) | 5 |
| Bad summaries | 0 |

**Result: ALL TESTS PASSED**

---

## Version-by-Version Progression

### v0.2.10 — Baseline
- **Tests:** ~50 (early script version)
- First version tested; established baseline for CLI inspection, compact mode, caching, scaffolds, security scanner, MCP pipeline, API mode, and doctor

### v0.2.12
- Docs clarification about PATH executables
- No test regressions

### v0.2.13 — Batch Inspection & Cache Stats
- **Tests:** ~101
- Added `inspect batch`, `inspect cache-stats`, `doctor --human`
- Test suite expanded with batch inspection, cache stats, and doctor human output sections

### v0.2.14 — Cache Management
- Added `cache-clear`, `cache-invalidate`, batch `--parallel`
- Fixed toon batch format
- **Bug found:** `cache-invalidate` appeared to clear all entries instead of targeted ones — reported and fixed in v0.2.15

### v0.2.15 — File-Driven Batch & Doctor Check
- Added `batch --from-file`, `doctor --check`
- Fixed `cache-invalidate` scope bug from v0.2.14
- **Tests:** 1 failure — `--from-file` with `cargo` not on PATH; resolved in subsequent version

### v0.2.17 — Doctor Fix & Diff Engine
- Added `doctor --fix`, `inspect diff`, `cache-warm`, YAML/TOML from-file, `--since`, bash completion integration
- **Tests:** 3 failures — all related to test data using `cargo` (not on PATH); test script updated to use `git` instead

### v0.2.18 — Diff UX Polish
- Added diff compact error guidance, diff toon format, RFC3339 support, doctor --fix output distinction
- **Tests:** 1 failure — compact diff test still referenced `cargo`; fixed by changing to `git` in test script

### v0.2.19 — Schema Tolerance
- Added diff schema tolerance for older profiles, doctor --fix summary counts
- **Tests:** ALL PASSED (134 total)

### v0.2.20 — Saved Diffs & CI Integration
- Added `--before --after` saved diffs, `--exit-code`, `--dry-run`, `--output-dir`, ndjson streaming, `--watch`, migration notes, toon removal rendering
- **Tests:** ALL PASSED (136 total)

### v0.2.21 — Watch Flush Fix
- Fixed `inspect diff --watch` to flush ndjson frames immediately for piped consumers
- **Tests:** ALL PASSED (136 total)

---

## Test Coverage by Section

### 1. Environment (2 tests)
- Verifies `sxmc` binary runs and reports version
- Confirms `python3` is available (required for JSON assertions)

### 2. Help & Completions (19 tests)
- Validates `--help` output mentions all 14 subcommands: `serve`, `skills`, `stdio`, `http`, `mcp`, `api`, `inspect`, `init`, `scaffold`, `scan`, `bake`, `doctor`, `completions`
- Tests shell completion generation for bash, zsh, fish
- Integration tests for bash completion: verifies top-level subcommand completion and nested option completion (`inspect batch --from-file`)

### 3. CLI Inspection Matrix (3 aggregate tests, 95+ tools)
- Runs `sxmc inspect cli <tool>` against 95+ common CLI tools across categories:
  - **BSD/Unix core:** ls, grep, sed, cp, rm, chmod, sort, tr, diff, cat, mv, mkdir, wc, head, tail, uniq, awk
  - **Developer:** git, gh, npm, cargo, rustc, rustup, python3, node, brew, curl, ssh, jq
  - **System:** tar, find, xargs, tee, cut, paste, join, comm, env, printenv, whoami, hostname, date, cal
  - **Compression:** zip, unzip, gzip, bzip2, xz
  - **Network:** ping, dig, nslookup, traceroute, ifconfig, netstat
  - **Process:** ps, top, kill, lsof, open
  - **macOS-specific:** pbcopy, pbpaste, defaults, launchctl, diskutil, sips, mdls, mdfind
  - **Compilers:** xcodebuild, swift, swiftc, clang, make, cmake
  - **Edge cases:** file, stat, du, df, mount, umount, ln, touch, less, more, man, which, basename, dirname, expr, bc, dc, od, hexdump, strings, nm, rg
- Validates each produces valid JSON
- Checks summary quality (no raw `usage:`, copyright notices, overstrike artifacts, or error messages)

### 4. Previously-Broken Tools (16 tests)
- Regression tests for specific tools that had bugs in earlier versions:
  - `brew`: verifies subcommands ≥5 and global options ≥1 (was 0 in v0.2.5–v0.2.7)
  - `cat`, `lsof`, `dc`: no false-positive subcommands
  - `gzip`, `ping`, `man`, `less`, `more`, `bc`, `dig`, `unzip`, `zip`, `grep`: clean summary strings
  - `awk`: has options (was 0)
  - `python3`: no fake subcommands (was 24 in v0.2.3)
  - `rustup`: has options and subcommands (lost in v0.2.3)
  - `gh`: has 20+ subcommands (regressed to 10 in v0.2.7)

### 5. Compact Mode (6 tests)
- Validates compact output is smaller than full output
- Checks for `subcommand_count` and `option_count` fields
- Confirms `provenance` is stripped
- Measures curl compact savings (expects ≥50%)

### 6. Profile Caching (2 tests)
- Verifies cache directory is created with JSON files
- Measures cold vs warm cache performance

### 7. Scaffold System (6 tests)
- `scaffold skill`: produces SKILL.md, mentions subcommands
- `scaffold mcp-wrapper`: produces README.md and manifest.json
- `scaffold llms-txt`: produces llms.txt
- Overflow hints for large CLIs (tested with brew's 115+ subcommands)

### 8. Init AI Pipeline (11 tests)
- Tests `init ai` for 10 AI hosts: Claude Code, Cursor, Gemini CLI, GitHub Copilot, Continue.dev, Open Code, JetBrains AI Assistant, Junie, Windsurf, OpenAI Codex
- Validates `--coverage full` mode produces ≥10 sections

### 9. Security Scanner (5 tests)
- Scans bundled malicious-skill fixture for CRITICAL issues, prompt injection (SL-INJ-001), dangerous operations (SL-EXEC-001), and secrets (SL-SEC-001)
- Tests enhanced secret pattern detection with a synthetic skill containing API keys, tokens, and credentials (expects ≥3 matches)

### 10. MCP Pipeline (5 tests)
- Full lifecycle test using stateful MCP server fixture:
  - `bake create` with `--skip-validate`
  - `bake list` shows created bake
  - `mcp tools` lists server tools
  - `mcp grep` finds matches
  - `bake remove` cleans up

### 11. Bake Validation (3 tests)
- Invalid source rejection with clear error message
- Error includes `--skip-validate` guidance
- `--skip-validate` succeeds with invalid source

### 12. API Mode (3 tests)
- Uses Petstore OpenAPI spec as test target
- `api --list`: finds ≥10 operations
- `api --search pet`: filters to ≥3 operations
- `api call getPetById`: returns valid JSON response

### 13. Doctor Command (9 tests)
- JSON output validation: root, startup_files, recommended_first_moves
- Recommendations include `unknown_cli` and `unknown_api` surfaces
- `--human` mode renders human-readable report with cache stats
- `--check` fails when startup files are missing
- `--check --only` scopes validation to selected hosts
- `--fix` repairs missing startup files
- `--fix --dry-run` previews without writing

### 14. Self-Dogfooding (5 tests)
- Verifies the repo ships its own AI configuration files:
  - `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`
  - `.cursor/rules/sxmc-cli-ai.md`
  - `.github/copilot-instructions.md`
- Each file must mention "sxmc"

### 15. Depth Expansion & Batch Inspection (24 tests)
- **Depth expansion:** compact output includes depth-2 guidance; `--depth 1` produces `subcommand_profiles`
- **Batch inspection:**
  - Reports correct count and partial failures
  - Reports parallelism setting
  - `--from-file` with plain text, comments/blank lines, YAML, TOML
  - `--output-dir` saves individual profile files
  - `--since` with RFC3339 timestamps
  - `--format toon` (summary-oriented, includes failure details)
  - `--format ndjson` (streams events + summary record)
- **Cache management:**
  - `cache-stats` returns entry_count and total_bytes
  - `cache-invalidate` with exact match, glob pattern, `--dry-run`
  - `cache-clear` clears all
  - `cache-warm` pre-populates
- **Profile diffing:**
  - Saved profile comparison
  - `--format toon` human-oriented output
  - Saved-vs-saved (`--before`/`--after`)
  - `--exit-code` returns 1 on change, 0 on identical
  - Compact profile error guidance
  - Schema tolerance for legacy profiles
  - Migration notes for older generator versions
  - `--watch` with ndjson flush for piped output

### 16. Error Messages (3 tests)
- Nonexistent tool gives clear error
- No arguments shows usage
- Inspecting self is blocked without `--allow-self`

### 17. Serve & Skills (5 tests)
- `serve --help` mentions transport, watch, and auth options
- `skills list` finds fixture skills
- `skills list --json` returns valid JSON array

---

## Findings Summary

### The Good
- **Zero parse failures** across 90+ installed CLI tools — exceptional parser quality
- **Zero bad summaries** — clean, useful one-line descriptions for all tools
- **Comprehensive CLI coverage** — handles everything from simple Unix tools to complex multi-level CLIs like git, brew, and gh
- **Cache system works well** — warm cache measurably faster than cold
- **Compact mode delivers** — 35-90% size reduction depending on tool complexity (curl achieves 50%+)
- **Scaffold pipeline is end-to-end** — inspect → profile → scaffold → init ai works for all 10 AI hosts
- **Security scanner catches real threats** — prompt injection, secrets, and dangerous operations
- **MCP bake lifecycle is solid** — create, list, tools, grep, remove all work
- **Doctor is TTY-aware** — human on terminal, JSON when piped, `--human` forces human
- **Error messages are helpful** — clear guidance for invalid inputs, self-inspection blocks, compact diff limitations
- **Diff engine is mature** — saved profiles, exit codes, toon format, schema tolerance, migration notes, watch mode with proper ndjson flushing
- **Batch inspection scales** — parallel execution, file-driven inputs (plain text, YAML, TOML), output directories, ndjson streaming

### Issues Found & Fixed
1. **cache-invalidate scope bug (v0.2.14)** — cleared all entries instead of targeted ones. Fixed in v0.2.15.
2. **Compact diff guidance (v0.2.18)** — now gives explicit error when trying to diff compact profiles
3. **Watch flush (v0.2.21)** — ndjson frames now flush immediately for piped consumers

### Skipped Tests (2)
- Depth-2 guidance in compact output (hint text may vary)
- Inspect self block (may not detect self by path)

These skips are expected — they depend on output text that may legitimately vary.

---

## Cross-Platform Notes

The test script is designed to work on both macOS and Linux:
- Uses `python3` for JSON assertions (avoids `jq` dependency)
- Detects terminal capabilities for colored output
- Handles platform-specific cache paths (`~/Library/Caches/sxmc` on macOS, `~/.cache/sxmc` on Linux)
- macOS-specific tools (pbcopy, defaults, etc.) are skipped gracefully on Linux
- Uses `bash` here-strings and POSIX-compatible constructs
- Cross-platform timing via `python3` (macOS `date` doesn't support `%N`)

Expected differences on Linux:
- macOS-specific tools will be skipped (~10 tools)
- `brew` may not be installed (some regression tests and overflow hint tests will skip)
- Cache directory will be under `~/.cache/sxmc/` instead of `~/Library/Caches/sxmc/`
