# sxmc Test Script Guide

**Script:** `scripts/test-sxmc.sh`
**Lines:** ~1225
**Sections:** 17
**Tests:** 136

---

## Overview

`test-sxmc.sh` is a comprehensive, cross-platform bash test suite for the `sxmc` CLI. It validates every major feature surface — CLI inspection, MCP pipeline, API mode, security scanning, scaffolds, AI host initialization, caching, doctor diagnostics, and profile diffing — using only `bash` and `python3`.

The script was developed iteratively across sxmc versions v0.2.10 through v0.2.21, growing from ~50 tests to 136 tests as features were added.

---

## Prerequisites

| Requirement | Notes |
|---|---|
| `bash` | v3.2+ (ships with macOS); v4+ on Linux |
| `python3` | Required for all JSON assertions |
| `sxmc` | On PATH, or set `SXMC=path/to/binary` |
| Network | Optional; API mode tests need internet |
| `git` | Needed for most tests; nearly universal |

Optional tools tested: `brew`, `cargo`, `rustup`, `gh`, `curl`, `ssh`, `jq`, `node`, `npm`, and ~80 others. Missing tools are skipped gracefully.

---

## Running the Tests

### Basic run

```bash
bash scripts/test-sxmc.sh
```

### Save JSON results

```bash
bash scripts/test-sxmc.sh --json /tmp/sxmc-results.json
```

### Use a specific binary

```bash
SXMC=./target/release/sxmc bash scripts/test-sxmc.sh
```

### Binary resolution order

1. `$SXMC` environment variable (if set)
2. `sxmc` on `$PATH`
3. `target/release/sxmc` (relative to repo root)
4. `target/debug/sxmc` (relative to repo root)

---

## Script Architecture

### Setup (lines 1–103)

- Resolves repo root and fixtures directory
- Creates temp directory and isolated `$HOME` for bake/cache tests
- Parses `--json` flag for output file
- Defines color codes (TTY-aware, degrades to plain text)
- Defines helper functions:
  - `pass()`, `fail()`, `skip()` — test result reporters with counters
  - `section()` — section header printer
  - `has_cmd()` — checks if a command exists on PATH
  - `time_ms()` — cross-platform millisecond timing via python3
  - `json_field()` — extracts a value from JSON using a Python expression
  - `json_check()` — evaluates a boolean Python expression against JSON
  - `sxmc_isolated()` — runs sxmc with an isolated HOME directory
  - `cleanup()` — removes temp directory on exit (via trap)

### JSON Assertion Pattern

All JSON tests use two helper functions that pass expressions via environment variables to avoid bash quoting issues:

```bash
# Check a boolean condition against JSON
json_check "$json_output" "d.get('count', 0) >= 10"

# Extract a value from JSON
count=$(json_field "$json_output" "d['count']")
```

Key implementation detail: uses `except Exception:` (not bare `except:`) to avoid catching `SystemExit` from `sys.exit()`.

### Isolated Environment

Tests that create bakes or modify cache use `sxmc_isolated()` which overrides `HOME`, `USERPROFILE`, `XDG_CONFIG_HOME`, `APPDATA`, and `LOCALAPPDATA` to point at a temp directory. This prevents tests from polluting the real user config.

---

## Section-by-Section Guide

### Section 1: Environment (lines 119–146)
**What:** Prints version/OS info, confirms sxmc and python3 work
**How:** Runs `sxmc --version`, `uname`, `python3 --version`

### Section 2: Help & Completions (lines 148–184)
**What:** Validates `--help` output and shell completions
**How:**
- Checks `--help` for 14 subcommand keywords via grep
- Generates completions for bash, zsh, fish
- Sources bash completions and simulates tab-completion for subcommands and options

### Section 3: CLI Inspection Matrix (lines 186–257)
**What:** Bulk-tests CLI parsing against 95+ tools
**How:**
- Iterates over a large array of tool names
- Skips tools not on PATH
- Runs `sxmc inspect cli <tool>` and validates output is valid JSON
- Checks summary quality: rejects raw `usage:` lines, copyright notices, overstrike artifacts (`SSUUMM`), error messages, and bug report URLs
- Reports aggregate counts (parsed, failed, skipped, bad summaries)

### Section 4: Previously-Broken Tools (lines 259–319)
**What:** Regression tests for specific parser bugs from earlier versions
**How:** Uses `check_tool()` helper that runs inspect, evaluates a Python expression, and on failure prints diagnostic info (subcommand count, option count, summary preview)

### Section 5: Compact Mode (lines 321–372)
**What:** Validates `--compact` output format and size reduction
**How:**
- Compares full vs compact output byte counts
- Checks for compact-specific fields (`subcommand_count`, `option_count`)
- Confirms `provenance` is stripped
- Measures curl compact savings percentage

### Section 6: Profile Caching (lines 374–408)
**What:** Validates cache directory creation and warm-cache performance
**How:**
- Clears cache directories
- Times cold then warm runs using `time_ms()`
- Checks cache directory and file existence

### Section 7: Scaffold System (lines 410–468)
**What:** Tests profile-to-scaffold pipeline
**How:**
- Saves a git profile to a temp file
- Runs `scaffold skill`, `scaffold mcp-wrapper`, `scaffold llms-txt`
- Checks output mentions expected files
- Tests overflow hints using brew (115+ subcommands)

### Section 8: Init AI Pipeline (lines 470–498)
**What:** Tests AI host configuration generation for 10 hosts
**How:** Runs `init ai --from-cli git --client <host> --mode preview` for each host, checks output contains `Target:`. Tests `--coverage full` produces ≥10 sections.

### Section 9: Security Scanner (lines 500–557)
**What:** Tests security vulnerability detection
**How:**
- Scans bundled `malicious-skill` fixture for CRITICAL, SL-INJ-001, SL-EXEC-001, SL-SEC-001
- Creates a synthetic skill with API keys/tokens and verifies ≥3 secret patterns are detected

### Section 10: MCP Pipeline (lines 559–609)
**What:** Tests full MCP bake lifecycle
**How:** Uses `fixtures/stateful_mcp_server.py` to test bake create → list → tools → grep → remove. Runs in isolated environment.

### Section 11: Bake Validation (lines 611–637)
**What:** Tests bake source validation and `--skip-validate`
**How:** Attempts bake create with invalid source, checks for error and guidance, then verifies `--skip-validate` bypasses validation.

### Section 12: API Mode (lines 639–679)
**What:** Tests OpenAPI integration via Petstore spec
**How:** Checks network availability, then tests `--list`, `--search`, and direct API call. Requires internet.

### Section 13: Doctor Command (lines 681–785)
**What:** Tests diagnostic and repair commands
**How:**
- Validates JSON structure (root, startup_files, recommended_first_moves)
- Tests `--human` output format
- Creates temp directories with/without startup files for `--check` and `--check --only`
- Creates a mock CLI script for `--fix` tests
- Validates `--fix --dry-run` doesn't write files

### Section 14: Self-Dogfooding (lines 787–805)
**What:** Verifies sxmc repo ships its own AI config files
**How:** Checks existence and sxmc mention in CLAUDE.md, AGENTS.md, GEMINI.md, .cursor/rules/sxmc-cli-ai.md, .github/copilot-instructions.md

### Section 15: Depth Expansion & Batch Inspection (lines 807–1108)
**What:** Comprehensive tests for batch, cache management, and diffing
**How:**
- Tests `--depth 1` subcommand_profiles
- Batch with inline args, `--from-file` (plain text, comments, YAML, TOML)
- Batch `--output-dir`, `--format toon`, `--format ndjson`
- `--since` with RFC3339 timestamps
- `cache-stats`, `cache-invalidate` (exact, glob, `--dry-run`), `cache-clear`, `cache-warm`
- Profile diffing: saved, toon, saved-vs-saved, `--exit-code`, compact error, legacy tolerance, migration notes
- `--watch` ndjson flush test using a Python subprocess that reads the first frame within 2 seconds

### Section 16: Error Messages (lines 1110–1138)
**What:** Tests user-facing error quality
**How:** Triggers errors (nonexistent tool, no args, self-inspection) and checks for clear, actionable messages.

### Section 17: Serve & Skills (lines 1140–1181)
**What:** Tests MCP server and skills listing
**How:** Checks `serve --help` for transport, watch, and auth options. Lists skills from fixtures directory.

### Summary Output (lines 1183–1225)
**What:** Prints results and optionally writes JSON
**How:** Prints colored pass/fail/skip counts. Generates JSON summary via python3 with version, OS, timestamp, and all counters. Exits with code 1 if any test failed.

---

## Test Development History

The test script was developed during a structured testing session across sxmc releases:

1. **Initial creation** — Built for v0.2.10 with ~50 tests covering core functionality
2. **v0.2.13 expansion** — Added batch inspection, cache stats, doctor human output (grew to ~101 tests)
3. **v0.2.14–v0.2.16** — Added cache management, file-driven batch, doctor check tests
4. **v0.2.17** — Major expansion: diff engine, cache-warm, YAML/TOML from-file, completion integration, doctor fix (grew to ~130 tests)
5. **v0.2.18** — Fixed test data: changed `cargo` to `git` for compact diff test (cargo not on test machine's PATH)
6. **v0.2.19–v0.2.21** — Added schema tolerance, migration notes, saved-vs-saved diffs, exit codes, watch mode tests (final: 136 tests)

### Key debugging lessons

- **`json_check` failure (most time-consuming bug):** Bare `except:` in Python catches `SystemExit` from `sys.exit(0)`, so all checks silently returned failure. Fix: change to `except Exception:`.
- **Environment variable approach for Python expressions:** Passing check expressions as `$2` inline in Python strings caused quoting issues. Fix: pass via environment variable (`_JC_EXPR="$2" python3 -c "...eval(os.environ['_JC_EXPR'])..."`).
- **macOS bash 3.2 quirks:** Here-strings (`<<<`), `2>/dev/null`, and function contexts have subtle interaction issues on the ancient bash shipped with macOS.

---

## Adapting for Linux

The script should work on Linux with no modifications. Expected differences:

- macOS-specific tools will be skipped: `pbcopy`, `pbpaste`, `defaults`, `launchctl`, `diskutil`, `sips`, `mdls`, `mdfind`, `xcodebuild`, `swift`, `swiftc`, `open`
- `brew` tests will skip if Homebrew is not installed
- Cache path: `~/.cache/sxmc/` instead of `~/Library/Caches/sxmc/`
- bash 4+ on most Linux distros — fewer quirks than macOS bash 3.2
- Additional tools may be available (e.g., `apt`, `dpkg`, `systemctl`) but are not in the current test matrix

To add Linux-specific tools to the inspection matrix, append them to the `CLI_TOOLS` array in Section 3.
