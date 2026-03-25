# Sumac Test Script Guide

**Script:** `scripts/test-sxmc.sh`
**Lines:** ~1700
**Sections:** 41 (4 parts: old features, new features, 10x10x10 matrix, benchmarks)
**Tests:** 296 in the latest published report

Companion smoke script:

- `scripts/smoke_portable_core.sh` is the smaller cross-platform smoke path for
  Linux, macOS, and Windows CI. It checks the stable product lifecycle at a
  much lower cost than the full release-sized shell suite.
- `scripts/smoke_portable_fixtures.sh` is the portable fixture-based MCP smoke
  companion. It validates local skill serving and MCP client flows across
  stdio, baked MCP, hosted HTTP, and bearer-protected HTTP.

---

## Overview

`test-sxmc.sh` is a comprehensive, cross-platform bash test + benchmark suite for Sumac (`sxmc`). It validates every major feature surface — CLI inspection, MCP pipeline, API mode, security scanning, scaffolds, AI host initialization, caching, doctor diagnostics, profile diffing, wrap, status/watch, publish/pull, bundle signing, corpus, registry, trust, and side-by-side comparisons — using only `bash` and `python3`.

The script was developed iteratively across sxmc versions v0.2.10 through v1.0.0, growing from ~50 tests to the current release-sized validation suite as features were added.

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

### Portable cross-platform smoke

```bash
bash scripts/smoke_portable_core.sh target/debug/sxmc .
```

On Windows CI, the same script runs under Git Bash with
`target/debug/sxmc.exe`.

### Portable fixture MCP smoke

```bash
bash scripts/smoke_portable_fixtures.sh target/debug/sxmc tests/fixtures
```

This companion script stays smaller than `test-sxmc.sh` but exercises the
stable local MCP fixture workflow on every OS.

### Binary resolution order

1. `$SXMC` environment variable (if set)
2. `target/debug/sxmc` (relative to repo root)
3. `target/release/sxmc` (relative to repo root)
4. `sxmc` on `$PATH`

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

## Section Overview

The script is organized into 4 parts with 41 sections:

### Part A — Old Features (Sections 1–18)

Re-validates all features from v0.2.10–v0.2.21:

| Section | What it tests |
|---|---|
| 1. Environment | Binary runs, python3 available |
| 2. Help & Completions | --help output (20 subcommands), bash/zsh/fish completions |
| 3. CLI Inspection Matrix | 95+ tools bulk-parsed, summary quality checks |
| 4. Previously-Broken Tools | Regressions for brew, cat, python3, gh, etc. |
| 5. Compact Mode | Size reduction, field presence, provenance stripped |
| 6. Profile Caching | Cache creation, cold vs warm timing |
| 7. Scaffold System | skill, mcp-wrapper, llms-txt scaffolds |
| 8. Init AI Pipeline | 10 AI hosts, --coverage full |
| 9. Security Scanner | Prompt injection, secrets, dangerous ops |
| 10. MCP Pipeline | bake create/list/tools/grep/remove |
| 11. Bake Validation | Invalid source rejection, --skip-validate |
| 12. API Mode | Petstore OpenAPI: --list, --search, call |
| 13. Doctor Command | JSON/human, --check, --fix, --dry-run |
| 14. Self-Dogfooding | Repo ships its own AI config files |
| 15. Depth & Batch | subcommand_profiles, batch, cache-stats, cache-clear |
| 16. Error Messages | Clear errors for invalid inputs |
| 17. Serve | serve --help, skills list |
| 18. Wrap (basic) | wrap --help flags |

### Part B — New Features v0.2.22–v1.0.0 (Sections 19–33)

| Section | What it tests |
|---|---|
| 19. Wrap Execution & Filtering | --allow/deny-option/positional, progress, stdout limits, stdio bridge |
| 20. Status & Watch | Structured JSON, AI knowledge/recovery plan, --health, --exit-code, --compare-hosts, watch flags |
| 21. Publish / Pull | Help flags, signing, round-trip (publish → pull → verify profiles) |
| 22. Bundle Export/Import/Verify | Create bundle, verify integrity, import profiles |
| 23. Bundle Signing | Ed25519 keygen, HMAC + Ed25519 sign/verify/reject |
| 24. Corpus | export-corpus, corpus-stats, corpus-query |
| 25. Registry | registry-init, registry-add, registry-list |
| 26. Trust | trust-report, trust-policy |
| 27. Known-Good | Best profile selection |
| 28. New Inspect Features | diff --format markdown, migrate-profile, drift, batch --retry-failed |
| 29. Doctor Enhancements | --remove for cleanup and inferred `doctor --fix` recovery |
| 30. CI Scaffold | scaffold ci generates GitHub Actions workflows |
| 31. Health Gates | --health --exit-code returns 0/1 plus local `sync` preview/apply/state coverage |
| 32. Discovery Lifecycle | GraphQL/traffic lifecycle help, curl history detection, codebase/db/traffic/graphql snapshot and diff coverage |
| 33. Add Pipeline | one-step and multi-tool onboarding, discovery-to-doc bridging, and wrap/serve MCP auto-registration |

### Part C — 10x10x10 Matrix (Sections 34–37)

| Section | What it tests |
|---|---|
| 34. 10 Known CLIs | git, curl, ls, ssh, tar, grep, find, gh, python3, jq — each: inspect, compact, scaffold, init-ai |
| 35. 10 Known Skills | 4 fixtures + 6 synthetic — list, info, run, --script, --env, --print-body, serve (MCP tools/prompts/resources), MCP tool calls |
| 36. 10 Known MCPs | 1 fixture + 4 npm + 1 self-host + 4 synthetic — bake, list, tools, grep, remove |
| 37. Side-by-Side | With vs without Sumac: CLI understanding, AI host config, CLI→MCP, skill execution, serve, full pipeline — with timing |

### Part D — Benchmarks (Sections 38–41)

| Section | What it measures |
|---|---|
| 38. CLI Inspection | Cold/warm per-tool (5 runs median), batch --parallel 1 vs 4 |
| 39. Wrap & MCP | wrap git → stdio --list latency |
| 40. Bundle | Export, HMAC sign timing |
| 41. Pipeline | inspect → scaffold → init-ai for 5 CLIs end-to-end |

---

## Test Development History

The test script was developed during structured testing sessions across sxmc releases:

1. **v0.2.10** — Initial creation with ~50 tests covering core functionality
2. **v0.2.13** — Added batch inspection, cache stats, doctor human output (~101 tests)
3. **v0.2.14–v0.2.16** — Cache management, file-driven batch, doctor check
4. **v0.2.17–v0.2.21** — Diff engine, completion integration, doctor fix, schema tolerance (136 tests)
5. **v0.2.22–v0.2.37** — Major rewrite: added Part B (new features), Part C (10x10x10 matrix), Part D (benchmarks). Wrap, status, watch, publish/pull, bundles, signing, corpus, registry, trust, CI scaffold, health gates (247 tests)
6. **v0.2.38–v0.2.39** — Added skills execution depth (--script, --env, --print-body), side-by-side comparisons, MCP tool call tests, and final metadata-sync cleanup (250 tests)
7. **v0.2.40** — Added GraphQL/traffic discovery lifecycle coverage plus codebase and database discovery snapshot checks (published report: 257 tests)
8. **v0.2.41** — Added `discover db --output`, corrected discovery help text, and aligned script numbering/docs with the current 275-test suite
9. **v0.2.42–v0.2.43** — Added one-step onboarding (`add`, `setup`), discovery-to-doc bridging, MCP auto-registration, stronger `status`/`doctor` recovery flows, interactive/TUI wrap safety, and explicit onboarding/status contract coverage (293 tests)
10. **v0.2.44** — Added local sync reconciliation (`sxmc sync`), `.sxmc/state.json` state tracking, sync-aware `status`, and shell/Rust coverage for preview/apply/check behavior (296 tests)
11. **v0.2.45** — Added the explicit `1.x` stability/support sweep across README, product contract, validation docs, and release process while keeping the full 296-test validation bar green
12. **v1.0.0** — Promoted the same validated contract into the first stable major release without changing the tested feature surface or lowering the 296-test release bar

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
