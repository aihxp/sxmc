# Usage Guide

The shortest path through `sxmc` is:

- `doctor` to see startup-discovery status and the next best `sxmc` command
- `serve` to publish skills as MCP
- `mcp` for daily MCP client work against baked connections
- `stdio` and `http` for raw or ad hoc MCP bridging
- `wrap` to turn an existing CLI into a focused MCP server immediately
- `api`, `spec`, and `graphql` for API-to-CLI flows
- `inspect cli`, `init ai`, and `scaffold` for CLI-to-AI startup artifacts

## Install

Install from crates.io:

```bash
cargo install sxmc
```

Or build from source:

```bash
git clone https://github.com/aihxp/sxmc.git
cd sxmc
cargo build --release
```

Prebuilt release archives and checksums are published on GitHub Releases.

## Serve Skills As MCP

Local stdio MCP:

```bash
sxmc serve
sxmc serve --paths /absolute/path/to/skills
```

Local development with reloads:

```bash
sxmc serve --watch
```

Hosted streamable HTTP MCP:

```bash
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --max-concurrency 64 \
  --max-request-bytes 1048576 \
  --bearer-token env:SXMC_MCP_TOKEN \
  --paths /absolute/path/to/skills
```

## Use MCP From The CLI

Ad hoc stdio bridge:

```bash
sxmc stdio '["sxmc","serve","--paths","tests/fixtures"]' --list
sxmc stdio '["sxmc","serve","--paths","tests/fixtures"]' --prompt simple-skill arguments=friend
sxmc stdio '["sxmc","serve","--paths","tests/fixtures"]' --resource \
  "skill://skill-with-references/references/style-guide.md"
```

Hosted bridge:

```bash
sxmc http http://127.0.0.1:8000/mcp \
  --auth-header "Authorization: Bearer $SXMC_MCP_TOKEN" \
  --timeout-seconds 15 \
  --describe --format toon --limit 10
```

Baked daily workflow:

```bash
sxmc bake create fixture-mcp \
  --type stdio \
  --source '["sxmc","serve","--paths","tests/fixtures"]'

sxmc mcp servers
sxmc mcp grep skill --limit 10
sxmc mcp tools fixture-mcp --limit 10
sxmc mcp info fixture-mcp/get_skill_details --format toon
sxmc mcp call fixture-mcp/get_skill_details \
  '{"name":"simple-skill","return_type":"content"}' --pretty
sxmc mcp prompt fixture-mcp/simple-skill arguments=friend
sxmc mcp read fixture-mcp/skill://skill-with-references/references/style-guide.md
```

Stateful MCP workflow:

```bash
sxmc mcp session fixture-mcp <<'EOF'
tools --limit 5
info get_skill_details --format toon
call get_skill_details '{"name":"simple-skill","return_type":"content"}' --pretty
exit
EOF
```

Recommended low-token MCP workflow:

1. `sxmc mcp servers`
2. `sxmc mcp grep <pattern>` or `sxmc mcp tools <server> --limit 10`
3. `sxmc mcp info <server/tool> --format toon`
4. `sxmc mcp call <server/tool> '<json-object>'`
5. use `sxmc mcp session <server>` when the MCP server expects stateful multi-step calls

## Wrap A CLI As MCP

Local stdio MCP wrapper:

```bash
sxmc wrap git
sxmc stdio '["sxmc","wrap","git"]' --list-tools
sxmc stdio '["sxmc","wrap","git"]' --describe-tool add
```

Hosted streamable HTTP wrapper:

```bash
sxmc wrap gh --transport http --host 127.0.0.1 --port 8001 \
  --bearer-token env:SXMC_WRAP_TOKEN

sxmc http http://127.0.0.1:8001/mcp \
  --auth-header "Authorization: Bearer $SXMC_WRAP_TOKEN" \
  --list-tools
```

Notes:

- `sxmc wrap <tool>` inspects the CLI first, then exposes focused MCP tools for
  the discovered top-level command surface.
- default inspection depth is `1`, so wrapped tools can pick up subcommand
  options and positionals without requiring a separate saved profile step.
- wrapped tool calls execute the real CLI directly, with argument validation
  driven by the generated MCP schema and a default per-call timeout of 30
  seconds.
- `sxmc wrap` also supports `--allow-tool`, `--deny-tool`, `--allow-option`,
  `--deny-option`, `--allow-positional`, `--deny-positional`, `--working-dir`,
  bounded stdout/stderr capture, and optional stderr heartbeat progress notes
  for long-running wrapped commands.
- long-running wrapped calls now also return structured `progress_events`,
  `progress_event_count`, `long_running`, and timeout metadata in the final JSON
  payload, so MCP clients can reason about slow or timed-out executions without
  scraping stderr.

## Use APIs As CLIs

Auto-detect:

```bash
sxmc api https://petstore3.swagger.io/api/v3/openapi.json --list
sxmc api https://petstore3.swagger.io/api/v3/openapi.json findPetsByStatus status=available
sxmc api https://petstore3.swagger.io/api/v3/openapi.json --timeout-seconds 15 --list
sxmc api https://petstore3.swagger.io/api/v3/openapi.json findPetsByStatus status=available --format toon
```

Explicit OpenAPI / GraphQL:

```bash
sxmc spec ./openapi.yaml listPets limit=10
sxmc graphql https://api.example.com/graphql users limit=5
sxmc graphql https://api.example.com/graphql --timeout-seconds 15 users limit=5
```

Network timeout notes:

- `sxmc http`, `sxmc api`, `sxmc spec`, and `sxmc graphql` accept `--timeout-seconds`
- baked HTTP/API/spec/graphql connections can persist a timeout with `sxmc bake create --timeout-seconds ...`
- if omitted, the underlying client default applies

## Turn CLIs Into AI Startup Surfaces

If the surface is unknown, start here first:

```bash
sxmc doctor
sxmc doctor --human
sxmc doctor --check --only claude-code,cursor
sxmc doctor --check --fix --only claude-code,cursor --from-cli gh
sxmc doctor --remove --only claude-code --from-cli gh
sxmc status --human
sxmc status --health --format json-pretty
sxmc status --health --exit-code
sxmc status --compare-hosts claude-code,cursor --format json-pretty
sxmc watch --health --exit-on-unhealthy --format ndjson
sxmc inspect cli <tool> --depth 1 --format json-pretty
sxmc inspect cli <tool> --depth 2 --compact --format json-pretty
sxmc inspect batch git cargo brew --parallel 4 --compact --format json-pretty
sxmc inspect batch --from-file tools.txt --compact --format json-pretty
sxmc inspect batch --from-file tools.yaml --since 2026-03-22T00:00:00Z --format json-pretty
sxmc inspect batch --retry-failed previous-batch.json --parallel 4
sxmc inspect drift .sxmc/ai/profiles --recursive --format json-pretty
sxmc inspect diff git --before before.json --format json-pretty
sxmc inspect diff --before before.json --after after.json --format markdown
sxmc inspect migrate-profile legacy-profile.json --output migrated-profile.json
sxmc inspect export-corpus --root . --format json-pretty
sxmc inspect corpus-stats corpus.json --format json-pretty
sxmc inspect corpus-query corpus.json --command git --format json-pretty
sxmc inspect bundle-export --output team-profiles.bundle.json
sxmc inspect bundle-import team-profiles.bundle.json --output-dir .sxmc/ai/profiles
sxmc inspect cache-stats --format json-pretty
sxmc inspect cache-invalidate cargo --format json-pretty
sxmc inspect cache-invalidate 'g*' --dry-run --format json-pretty
sxmc inspect cache-clear --format json-pretty
sxmc inspect cache-warm --from-file tools.toml --parallel 4 --format json-pretty
sxmc stdio "<cmd>" --list
sxmc mcp grep <pattern>
sxmc api <url-or-spec> --list
sxmc serve --paths <dir>
sxmc scan --paths <dir>
```

Inspect a real CLI:

```bash
sxmc inspect cli gh --format json-pretty
sxmc inspect cli gh --format toon
sxmc inspect cli curl --compact --format json-pretty
sxmc inspect cli cargo --depth 1 --format json-pretty
sxmc inspect cli gh --depth 2 --compact --format json-pretty
sxmc inspect batch git cargo brew --parallel 4 --compact --format json-pretty
sxmc inspect batch --from-file tools.txt --parallel 4 --compact --format json-pretty
sxmc inspect batch --from-file tools.yaml --parallel 4 --since 2026-03-22T00:00:00Z
sxmc inspect batch --retry-failed previous-batch.ndjson --parallel 4
sxmc inspect diff git --before before.json --format json-pretty
sxmc inspect diff git --before before.json --format toon
sxmc inspect diff --before before.json --after after.json --format markdown
sxmc scaffold ci --from-profile .sxmc/ai/profiles/git.json --mode apply
sxmc inspect export-corpus --root . --output corpus.ndjson --format ndjson
sxmc inspect corpus-stats corpus.json --format json-pretty
sxmc inspect corpus-query corpus.json --search content --limit 10 --format json-pretty
sxmc inspect bundle-export --bundle-name "Platform Bundle" --role platform --hosts claude-code,cursor --output team-profiles.bundle.json
sxmc inspect bundle-export --bundle-name "Platform Bundle" --role platform --signature-secret env:SXMC_BUNDLE_SECRET --output team-profiles.bundle.json
sxmc inspect bundle-verify team-profiles.bundle.json --signature-secret env:SXMC_BUNDLE_SECRET --format json-pretty
sxmc publish team-profiles.bundle.json --bundle-name "Platform Bundle" --role platform --signature-secret env:SXMC_BUNDLE_SECRET
sxmc pull team-profiles.bundle.json --output-dir .sxmc/ai/profiles --expected-sha256 <digest> --signature-secret env:SXMC_BUNDLE_SECRET
sxmc inspect cache-stats --format json-pretty
sxmc inspect cache-invalidate cargo --format json-pretty
sxmc inspect cache-invalidate 'g*' --dry-run --format json-pretty
sxmc inspect cache-clear --format json-pretty
sxmc inspect cache-warm --from-file tools.toml --parallel 4 --format json-pretty
```

Important:

- `sxmc inspect cli ...` runs a real subprocess, so the target must be an
  actual executable on `PATH` or an explicit path to a binary/script.
- shell aliases and shell functions from an interactive session are not visible
  to `sxmc` subprocess execution.

Notes:

- `sxmc doctor` defaults to a human-readable report on a real terminal and
  structured JSON when stdout is piped or redirected.
- `sxmc doctor --human` forces the readable report even when you are capturing
  output off-TTY.
- `sxmc doctor --check --only claude-code,cursor` turns doctor into a scoped CI
  gate for the specific AI hosts a repo actually uses.
- `sxmc doctor --check --fix --only claude-code,cursor --from-cli gh` repairs
  missing startup files for the selected hosts by running the same generation
  path as `init ai`.
- `sxmc doctor --remove --only claude-code --from-cli gh` removes generated
  startup files or managed snippets for the selected hosts.
- `sxmc status` extends doctor with saved-profile drift so you can see whether
  `.sxmc/ai/profiles` still matches the currently installed tools.
- `sxmc status` also includes saved-profile inventory metadata so you can spot
  stale profiles, freshness gaps, and profiles that are not yet ready for
  startup-doc generation.
- saved-profile inventory and exported corpus entries now include a quality
  score/level in addition to the boolean ready/not-ready signal.
- `sxmc status --health` also validates baked MCP/API connections and adds a
  `baked_health` summary plus per-host readiness under `host_capabilities`.
- `sxmc status --health` also groups checks into MCP/API/spec/graphql panels so
  you can read operational health by surface instead of only by raw bake type.
- `sxmc status --health` now includes latency and slow-entry summaries so
  degraded integrations are easier to spot without digging through raw entries.
- `sxmc status --health --exit-code` turns baked health into a CI-friendly gate
  that fails whenever unhealthy integrations are present.
- `sxmc status --compare-hosts claude-code,cursor` highlights readiness,
  doc-presence, and config-presence differences across selected AI hosts.
- `sxmc watch` polls the same status surface over time, flushes the first frame
  immediately for piped consumers, and can exit non-zero on the first observed
  change after the initial frame with `--exit-on-change`.
- `sxmc watch --health --exit-on-unhealthy` exits on the first observed frame
  with unhealthy baked MCP/API integrations.
- `sxmc inspect batch ...` keeps partial failures in a `failures` array instead
  of failing the whole run on the first missing command.
- `sxmc inspect batch ... --parallel N` bounds concurrency for larger batch jobs.
- `sxmc inspect batch ...` automatically emits stderr progress notes for larger
  batch runs on a real terminal; `--progress` forces them for smaller runs too.
- `sxmc inspect batch --retry-failed previous-batch.json` reloads only the
  failed command specs from a prior batch JSON or NDJSON result.
- `sxmc inspect batch --from-file tools.txt` reads one command spec per line.
  Blank lines and lines starting with `#` are ignored, trailing whitespace is
  trimmed, and inline arguments like `git status` are preserved.
- `.yaml` / `.yml` / `.toml` batch files can use structured tool entries with
  per-command depth overrides.
- depth overrides are fully reflected in full JSON output via
  `subcommand_profiles`; compact output keeps only summary fields like
  `nested_profile_count`.
- `sxmc inspect batch ... --since <timestamp>` skips commands whose executable
  has not changed since the given Unix-seconds or RFC3339 timestamp.
- `sxmc inspect diff <tool> --before before.json` compares a live CLI against a
  previously saved profile and reports added/removed options and subcommands.
- `sxmc inspect diff --before old.json --after new.json` compares two saved
  profiles without needing the live tool on `PATH`.
- `sxmc inspect drift .sxmc/ai/profiles --recursive` checks saved profile files
  against the currently installed commands and reports which ones changed.
- `sxmc scaffold ci --from-profile <profile> --mode apply` generates a
  GitHub Actions drift workflow that turns `inspect diff --exit-code` into a
  ready-to-commit CI gate.
- `sxmc inspect migrate-profile legacy-profile.json --output migrated.json`
  rewrites a saved profile through the current schema-tolerant loader and emits
  canonical current-schema JSON.
- `sxmc inspect export-corpus` packages saved profiles plus readiness and
  freshness metadata into a corpus-friendly JSON envelope. Use
  `--format ndjson` for one-record-per-line export.
- `sxmc inspect corpus-stats corpus.json` summarizes command coverage, stale
  profiles, and average quality across an exported corpus.
- `sxmc inspect corpus-query corpus.json --search <term>` lets you query an
  exported corpus by command or summary without loading the whole file by hand.
- `sxmc inspect bundle-export --output profiles.bundle.json` packages saved
  profiles from `.sxmc/ai/profiles` into one portable bundle file.
- bundle export also accepts optional team metadata via `--bundle-name`,
  `--description`, `--role`, and `--hosts`, which is preserved on import.
- `sxmc inspect bundle-import profiles.bundle.json --output-dir ./profiles`
  restores bundle contents into a target profile directory, with
  `--overwrite` or `--skip-existing` controls when files already exist.
- `sxmc inspect bundle-verify <bundle>` validates a bundle schema and reports
  its canonical SHA-256 digest, with optional `--expected-sha256` enforcement
  and optional embedded-signature verification via `--signature-secret`.
- `sxmc inspect bundle-export --signature-secret env:SXMC_BUNDLE_SECRET ...`
  embeds an HMAC-SHA256 signature into the bundle so downstream pulls can verify
  authenticity without relying on transport trust alone.
- `sxmc publish <target>` wraps bundle export plus transport, so you can write
  a team bundle directly to a file path, `file://` URI, or HTTP(S) endpoint,
  and its report includes the canonical bundle SHA-256 plus any embedded
  signature metadata.
- `sxmc pull <source>` fetches a published bundle from a file path, `file://`
  URI, or HTTP(S) endpoint and restores it into a local profile directory, with
  optional `--expected-sha256` and `--signature-secret` verification before
  import.
- `sxmc inspect diff --format markdown` renders a PR-friendly Markdown summary
  of summary, subcommand, option, and environment deltas.
- `sxmc inspect diff --watch 3` re-runs the diff every three seconds, and each
  frame is flushed immediately so piped/non-interactive consumers can observe
  updates without waiting for process exit.
- `sxmc inspect diff --watch 3 --format ndjson` emits one JSON object per
  change frame, which is useful for scripted monitoring.
- `sxmc inspect diff` expects a full saved profile, not a compact one. If you
  want to diff later, save with `sxmc inspect cli <tool> --format json-pretty`
  and omit `--compact`.
- `sxmc inspect cache-stats` shows cache path, entry count, size, and default
  TTL so repeated inspection behavior is visible.
- `sxmc inspect cache-invalidate <tool>` removes cached profiles for one command
  without flushing the entire cache.
- `sxmc inspect cache-invalidate 'g*' --dry-run` previews exact or glob
  invalidation matches before removing anything.
- `sxmc inspect cache-clear` wipes all cached CLI profiles.
- `sxmc inspect cache-warm ...` pre-populates the profile cache without dumping
  full profile payloads into stdout.
- `sxmc inspect batch --output-dir ./profiles --skip-existing` preserves
  existing profile files, while `--overwrite` replaces them in place.
- `sxmc inspect batch --output-dir ./profiles` also writes
  `batch-summary.json` so exported profile directories keep a machine-readable
  manifest alongside the individual profile files.

Generate startup-facing artifacts for a host profile:

```bash
sxmc init ai --from-cli gh --client claude-code --mode preview
sxmc init ai --from-cli gh --client cursor --mode preview
sxmc init ai --from-cli gh --coverage full --mode preview
sxmc init ai --from-cli gh --coverage full --host claude-code,cursor --mode apply
sxmc init ai --from-cli gh --coverage full --host claude-code --mode apply --remove
```

Pipeline summary:

```text
CLI binary -> sxmc inspect cli -> JSON profile -> sxmc init ai / scaffold -> AI-ready files
```

Generate from an existing saved profile:

```bash
sxmc scaffold agent-doc \
  --from-profile examples/profiles/from_cli.json \
  --client claude-code \
  --mode preview

sxmc scaffold client-config \
  --from-profile examples/profiles/from_cli.json \
  --client cursor \
  --mode preview

sxmc scaffold skill \
  --from-profile examples/profiles/from_cli.json \
  --mode preview

sxmc scaffold mcp-wrapper \
  --from-profile examples/profiles/from_cli.json \
  --mode preview

sxmc scaffold llms-txt \
  --from-profile examples/profiles/from_cli.json \
  --mode preview
```

Write modes:

- `preview`
  - print generated artifacts to stdout
- `write-sidecar`
  - write sidecar files under `.sxmc/ai/...`
- `patch`
  - show a patch-style preview for apply-capable targets
- `apply`
  - update managed markdown blocks or mergeable config files

Safety rules:

- existing `AGENTS.md` / `CLAUDE.md` files are not overwritten wholesale
- `apply` uses managed `sxmc` blocks for markdown docs
- JSON MCP configs are merged where the host shape is known
- `sxmc` refuses to inspect itself unless you pass `--allow-self`
- `sxmc init ai` blocks low-confidence startup-doc generation unless you pass `--allow-low-confidence`
- skill and MCP-wrapper scaffolds write new files rather than mutating existing docs
- `--coverage full` is the best way to generate broad startup coverage without committing to every host at once
- `--coverage full --mode apply` requires one or more `--host` values and sidecars the non-selected hosts
- `sxmc init ai --remove` removes previously applied managed blocks and generated config entries for the selected hosts
- `sxmc bake create` and `sxmc bake update` validate sources by default; use `--skip-validate` when you intentionally want to persist an offline or placeholder target
- bake validation errors now include source-type-specific hints for stdio, HTTP MCP, OpenAPI, and GraphQL targets so you can tell whether the problem is install, auth, endpoint shape, or just an intentionally offline target
- `inspect profile` and every `--from-profile` scaffold now fail with a profile-specific error if the input is empty, not valid JSON, or not an `sxmc` CLI surface profile

Deeper inspection:

- `sxmc inspect cli --depth 1` recursively inspects top-level high-confidence subcommands
- larger values like `--depth 2` keep recursing into nested command groups for multi-layer CLIs such as `gh`
- `sxmc inspect cli --compact` returns a lower-context summary with counts plus the top subcommands/options instead of the full profile
- nested subcommand profiles are stored under `subcommand_profiles`
- macOS and BSD-style tools can fall back to `man` output when `--help` is sparse or unsupported
- higher-signal `--help` results stay primary, while `man` output supplements weak summaries and missing options
- Homebrew inspection now keeps real global options like `--debug`, `--quiet`, `--verbose`, and `--help` while still using `brew commands` for broad subcommand discovery
- parser hardening now recovers top-level flags for CLIs like `gh` and `rustup`
- Python-style environment variables are filtered out of subcommand detection
- inspected CLI profiles are cached automatically, keyed by command plus executable fingerprint, so repeated agent lookups reuse stable profiles until the binary changes
- interactive recursive inspections emit lightweight stderr progress notes on cache hits, nested subcommand probes, and slower supplemental lookups such as `brew commands`
- generated agent docs, skills, and `llms.txt` exports show subcommand counts and overflow hints instead of truncating large CLIs with no indication of what was omitted
- if a command only exists as a shell alias/function wrapper, `sxmc inspect cli` will correctly report that no real executable was found; that is an environment issue, not a parser failure

Current host profiles:

- `claude-code`
- `cursor`
- `gemini-cli`
- `github-copilot`
- `continue-dev`
- `open-code`
- `jetbrains-ai-assistant`
- `junie`
- `windsurf`
- `openai-codex`
- `generic-stdio-mcp`
- `generic-http-mcp`

Full-coverage generation produces:

- a portable `AGENTS.md` block
- `CLAUDE.md` for Claude Code
- `.cursor/rules/sxmc-cli-ai.md` for Cursor
- `GEMINI.md` for Gemini CLI
- `.github/copilot-instructions.md` for GitHub Copilot
- `.continue/rules/sxmc-cli-ai.md` for Continue
- `opencode.json` for OpenCode
- `.aiassistant/rules/sxmc-cli-ai.md` for JetBrains AI Assistant
- `.junie/guidelines.md` for Junie
- `.windsurf/rules/sxmc-cli-ai.md` for Windsurf
- host config scaffolds for Claude, Cursor, Gemini, OpenAI/Codex, and generic stdio/http MCP
- this repo itself now checks in generated startup docs for the main host surfaces as a self-dogfooding example

At a high level:

| Stage | Command | Result |
|---|---|---|
| Inspect | `sxmc inspect cli gh --format json-pretty` | canonical JSON profile |
| Initialize | `sxmc init ai --from-cli gh --client claude-code` | startup-facing host artifacts |
| Scaffold | `sxmc scaffold ... --from-profile ...` | deeper outputs like `SKILL.md`, `llms.txt`, or an MCP wrapper |

Notes:

- GitHub Copilot gets a native instructions file, not an MCP config scaffold
- OpenCode gets a native `opencode.json` scaffold
- Continue, Junie, and Windsurf are native doc targets today, not MCP config targets
- JetBrains AI Assistant is a native rules-doc target today, not an MCP config target
- `llms.txt` is optional and exported separately through `scaffold llms-txt`

## Shell Completions

Generate completions from clap:

```bash
sxmc completions bash
sxmc completions zsh
sxmc completions fish
```

Example installation:

```bash
sxmc completions zsh > "${fpath[1]}/_sxmc"
sxmc completions bash > ~/.local/share/bash-completion/completions/sxmc
```

## Client Setup Notes

`sxmc` is designed to work well with:

- Codex
- Cursor
- Gemini CLI
- Claude Code
- generic local stdio MCP clients
- generic remote streamable HTTP MCP consumers

For local client configs, point the client at:

```text
command: sxmc
args: ["serve", "--paths", "/absolute/path/to/skills"]
```

For hosted clients, point them at:

```text
http://HOST:PORT/mcp
```

with bearer auth or required headers enabled on the server.

## Agent Guidance

If you maintain `AGENTS.md`, `CLAUDE.md`, or similar repo guidance, prefer
teaching agents this pattern:

1. when the surface is unknown, run `sxmc doctor` and then use the matching `sxmc` bridge first
2. use `sxmc inspect cli <tool> --depth 1` for unfamiliar CLIs
3. use `sxmc api <url-or-spec> --list` before hand-constructing requests
4. search or list first for MCP
5. inspect one tool with `sxmc mcp info`
6. call one tool with `sxmc mcp call`
7. use `sxmc mcp session <server>` when a tool expects multi-step state
8. keep large output in files or pipes instead of pasting it into context
9. parse stdout only for machine-readable output; informational `[sxmc]` lines go to stderr
