# CLI -> AI Surfaces

This document turns the `CLI -> AI surfaces` idea into a concrete product model
for `sxmc`.

## Goal

Take a real CLI or a generated CLI wrapper, inspect it into a normalized JSON
profile, and generate agent-ready scaffolds from that profile.

## What Counts As An AI Surface

`AI surfaces` in `sxmc` should stay intentionally small and explicit.

Supported target surface types:

- `skill_markdown`
  - a `SKILL.md`-style artifact
- `agent_doc_snippet`
  - a suggested block for files like `AGENTS.md`, `CLAUDE.md`, or similar
- `mcp_wrapper_scaffold`
  - a scaffold for turning the CLI into an MCP-facing wrapper
- `client_config_snippet`
  - a small example block for a client or agent setup file

Out of scope by default:

- rewriting an entire existing `AGENTS.md` / `CLAUDE.md`
- autonomous multi-file project refactors
- automatic commits of generated docs without review

## Pipeline Model

The intended pipeline is:

```text
real CLI -> normalized JSON profile -> chosen AI surface(s)
```

Optional broader product graph:

```text
Skills -> MCP -> Generated CLI -> AI surfaces
MCP -> Generated CLI -> AI surfaces
real CLI -> AI surfaces
```

Each hop is explicit. `sxmc` should not silently chain multiple hops.

## Generated CLI Boundary

`sxmc stdio` / `sxmc http` are runtime bridge commands, not generated CLI
artifacts.

If `sxmc` later supports `MCP -> Generated CLI`, that generated CLI should be
treated as a separate artifact class with provenance metadata.

Two distinct categories:

- `runtime_bridge`
  - ephemeral CLI behavior like `sxmc stdio "..." get-sum`
- `generated_cli`
  - a standalone wrapper script, binary scaffold, or command project emitted by `sxmc`

Only `generated_cli` should be considered a new inspectable artifact, and even
then only with explicit user opt-in.

## Provenance And Loop Prevention

Every generated artifact should carry provenance metadata.

Minimum fields:

- `generated_by`
- `generator_version`
- `source_kind`
- `source_identifier`
- `profile_schema`
- `generation_depth`
- `generated_at`

Loop-safety rules:

1. Real sources are inspectable by default.
2. Generated sources are not inspectable by default.
3. `generation_depth` defaults to `0` for real sources and increments for each generated artifact.
4. Default maximum generation depth is `1`.
5. Going beyond `1` should require explicit opt-in.
6. Self-targeting should be blocked by default for `sxmc` itself unless explicitly allowed.

## Review / Apply Model

Default outputs should be reviewable, not silently applied.

Preferred order of operations:

1. `stdout` preview
2. sidecar output file
3. patch preview
4. explicit write/apply

Recommended default behavior:

- `--print`
  - print the generated artifact or profile to stdout
- `--output`
  - write a sidecar file
- `--patch`
  - emit a patch or diff preview
- `--write`
  - write new files only
- `--apply`
  - reserved for explicit future mutation of existing docs

For agent-doc outputs, `--apply` should never be the default.

## Intermediate Representation

The CLI profile JSON is the product contract.

It should include:

- command identity
- summary/description
- subcommands
- options/flags
- positionals
- examples
- auth/environment requirements
- output behavior
- inferred workflows
- provenance

This schema should be versioned. The initial schema name should be:

- `sxmc_cli_surface_profile_v1`

## Confidence Model

CLI inspection is inherently imperfect. Generated output should reflect that.

Suggested levels:

- `high`
  - directly observed in help or examples
- `medium`
  - inferred from structure or naming
- `low`
  - heuristic guess that should be reviewed carefully

Generated artifacts should prefer high-confidence inputs and clearly separate
inference from observation.

## Practical Product Shape

If this lands in `sxmc`, the clean command family is:

- `sxmc inspect cli <command>`
- `sxmc scaffold skill --from-profile <file>`
- `sxmc scaffold mcp --from-profile <file>`
- `sxmc scaffold agent-doc --from-profile <file>`

This keeps inspection deterministic and generation reviewable.

## Recommended 1.0 Scope

For a first stable version of this feature, keep the surface narrow:

- one versioned JSON profile schema
- one deterministic inspection path
- one `SKILL.md` scaffold target
- one agent-doc snippet target
- provenance on every generated artifact
- no default mutation of existing repo docs
