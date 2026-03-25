# Product Contract

This document defines what `sxmc` claims to support today, what should fail
gracefully, and what is intentionally outside the contract.

Use this document together with:

- [USAGE.md](USAGE.md) for the canonical product workflows
- [OPERATIONS.md](OPERATIONS.md) for release and hosting guidance
- [VALIDATION.md](VALIDATION.md) for repeatable release checks and compatibility notes

## Supported And Expected To Work

These are the core product paths we should treat as stable:

### 1. Skills -> MCP

- `sxmc serve` loads skill directories and exposes them over MCP
- `sxmc serve --discovery-snapshot <file-or-dir>` exposes saved discovery
  snapshots as MCP-readable resources
- per-skill prompts are available when `SKILL.md` is present
- `scripts/` entries become MCP tools
- `references/` entries become MCP resources
- hybrid retrieval tools are always available:
  - `get_available_skills`
  - `get_skill_details`
  - `get_skill_related_file`

### 2. MCP -> CLI

- `sxmc stdio` can discover and invoke tools, prompts, and resources from a stdio MCP server
- `sxmc http` can discover and invoke tools, prompts, and resources from a streamable HTTP MCP server
- `sxmc mcp` can discover and invoke tools, prompts, and resources from baked stdio/http MCP connections
- `sxmc mcp session <server>` supports stateful multi-step workflows against a single baked MCP connection
- `--list`, `--list-tools`, `--list-prompts`, `--list-resources`, `--describe`, and `--describe-tool` are supported CLI surfaces
- one-shot tool execution is supported
- one-shot prompt fetches with `--prompt` are supported
- one-shot resource reads with `--resource` are supported
- baked `server/tool` workflows are supported through `mcp servers|grep|tools|info|call|prompt|read|session`

### 3. API -> CLI

- `sxmc api` auto-detects OpenAPI vs GraphQL
- `sxmc spec` supports direct OpenAPI execution
- `sxmc graphql` supports GraphQL schema-driven invocation

### 4. Hosting And Auth

- local stdio MCP hosting is supported
- remote streamable HTTP MCP hosting at `/mcp` is supported
- `/healthz` is supported for hosted deployments
- bearer-token and required-header auth are supported for remote MCP hosting

### 5. CLI -> AI Startup Surfaces

- `sxmc inspect cli <command>` is supported for deterministic help-based inspection
- `sxmc add <command>` is supported as the one-step inspect/save/onboard workflow
- `sxmc setup` is supported as the multi-tool onboarding workflow
- `sxmc init ai --from-cli <command> --client <profile>` is supported for generating startup-facing artifacts
- `sxmc init ai --from-cli <command> --coverage full` is supported for generating multi-host startup coverage
- `sxmc init discovery <snapshot-or-dir>` is supported for delivering saved
  discovery snapshots into startup-facing host docs
- `sxmc doctor` is supported for startup-file health and repair guidance
- `sxmc status` is supported as the unified machine-readable host/setup state surface
- `sxmc sync` is supported as the local reconciliation workflow for saved profiles and AI-host artifacts
- `sxmc scaffold agent-doc --from-profile ...` is supported
- `sxmc scaffold client-config --from-profile ...` is supported
- `sxmc scaffold skill --from-profile ...` is supported
- `sxmc scaffold mcp-wrapper --from-profile ...` is supported
- `sxmc scaffold llms-txt --from-profile ...` is supported as an optional export
- host profiles are supported for:
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
- preview, sidecar, patch, and apply modes are supported
- apply mode updates managed markdown blocks or mergeable config files only
- full-coverage apply updates only the explicitly selected `--host` targets and sidecars the rest
- `sxmc add`, `sxmc setup`, `sxmc doctor`, `sxmc status`, and `sxmc sync` all support explicit
  structured output via `--pretty` / `--format ...`
- `sxmc add --client ...` / `sxmc setup --client ...` and
  `sxmc doctor --host ...` / `sxmc status --host ...` are stable naming aliases
  for the primary host-selection flags

### 6. Stable Machine-Readable Contracts

These machine-readable surfaces are part of the promised product contract:

- `sxmc add --format ...`
- `sxmc setup --format ...`
- `sxmc doctor --format ...`
- `sxmc status --format ...`
- `sxmc sync --format ...`

Contract rules for these outputs:

- top-level shapes should remain stable within the `1.x` line
- new fields should be added additively rather than replacing old ones
- stable exit-code behavior should not drift silently
- recovery guidance should become more specific over time, not disappear

## Should Fail Gracefully

These scenarios should not crash the product or produce misleading results:

- promptless/resource-less MCP servers should still allow tool discovery and one-shot tool calls
- zero-argument MCP tools should receive `{}` rather than an omitted argument object
- startup-only invocations like `sxmc --version` and `sxmc --help` should succeed on all supported platforms
- unsupported optional MCP surfaces should be skipped with a clear note rather than failing all discovery
- `scan` should continue to use non-zero exit status for findings by design, but not be treated as a crash
- existing `AGENTS.md` / `CLAUDE.md` files should not be overwritten wholesale by CLI->AI generation
- scaffolded skill and MCP-wrapper files should be created as new files rather than overwriting arbitrary existing files

## Explicitly Outside The Contract

These are not promised as current product behavior:

- persistent multi-turn MCP sessions through repeated fresh `sxmc stdio ...` invocations
- stateful "dialog" continuity across separate CLI invocations without an explicit `sxmc mcp session`
- automated CI launch of proprietary clients like Cursor, Codex, or Claude Code
- universal compatibility with every third-party MCP server without caveats
- benchmark numbers as proof of broad client compatibility
- fully automatic client startup discovery without either a real config file or a real startup-read doc file

## Release Bar

Before a release, we should be able to point to:

1. a passing local certification run
2. a current compatibility matrix
3. a current benchmark snapshot
4. a documented support boundary for anything still out of scope
5. a current [STABILITY.md](STABILITY.md) statement that still matches the shipped UX

If a behavior is not covered by one of those, it should not be described as a
guaranteed product path.
