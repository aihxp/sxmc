# Roadmap

This document captures the post-`1.0.0` product priorities for Sumac (`sxmc`).
It intentionally avoids re-listing completed pre-`1.0.0` work.

## Principles

- Preserve the single-binary, low-dependency model.
- Prefer features that compound the value of existing profiles, discovery
  snapshots, and maintained host artifacts.
- Keep `sxmc` host-neutral so it remains useful across Claude Code, Cursor,
  Copilot, Gemini CLI, Codex, and future AI hosts.
- Treat generated tool knowledge as something that must stay current, not as a
  one-time scaffold.
- Prefer additive changes to stable `1.x` commands and JSON output.

## Priority 1: Continuous Maintenance

Goal: keep generated knowledge fresh as tools evolve.

Focus areas:

- make `sync`, `watch`, and `status` feel like one coherent maintenance loop
- reduce manual refresh steps after tool upgrades
- make stale or broken host state easier to recover automatically

Candidate work:

- completed foundation:
  - tighter `status -> sync -> doctor` recovery loops
  - explicit local sync state tracking for saved profiles and derived host artifacts
  - watch notification hooks for changed/unhealthy frames
  - broader host-aware remediation suggestions
  - webhook delivery for watch events
  - richer notification payload templates and destination presets

## Priority 2: Discovery -> Delivery

Goal: make discovered interfaces immediately useful to AI hosts and MCP clients.

Focus areas:

- discovered knowledge should not stop at stdout or saved snapshots
- codebase, database, GraphQL, and traffic discovery should have clearer delivery
  paths into host docs, MCP resources, or generated tools

Candidate work:

- completed foundation:
  - richer `init discovery` that accepts one snapshot or a snapshot directory
  - discover-to-MCP resource generation for saved discovery snapshots served
    through `sxmc serve`
  - discover-driven scaffolds for common team workflows
  - discovery-tool scaffolds for GraphQL/database/traffic snapshots
  - executable higher-level wrappers served directly from those snapshot manifests

## Priority 3: Ecosystem Hardening

Goal: improve interoperability without compromising the stable core.

Focus areas:

- more host/client compatibility depth
- safer wrapping for interactive, TUI, or partial-automation tools
- better import/export ergonomics around bundles, registries, and trust policies

Candidate work:

- completed foundation:
  - safer execution defaults and clearer interactive-surface policy
  - strong trust and registry workflows for local/team distribution
  - broader compatibility smoke coverage for discovery-delivery workflows
  - expanded compatibility fixtures and smoke coverage

## Priority 4: Operational Hardening

Goal: keep the stable product path boring, reliable, and easy to support.

Focus areas:

- cross-platform validation and packaging hygiene
- simpler contributor docs and fewer stale documentation surfaces
- regression discipline around `setup -> add -> status -> sync`

Candidate work:

- completed foundation:
  - reduced documentation sprawl as features stabilized
  - kept stable contracts and release docs tightly aligned
  - portable Linux/macOS/Windows smoke coverage for the stable discovery and
    delivery path
  - portable Linux/macOS/Windows fixture coverage for local MCP serving, baked
    MCP, hosted HTTP, and bearer-protected HTTP flows

## What Is Not On This Roadmap

These remain intentionally out of the near-term `1.x` plan unless the product
direction changes:

- terminal emulation as a substitute for understanding interactive tools
- heavyweight always-on infrastructure for local workflows
- GUI/app discovery unless it can be done in a Sumac-native, inspectable way

## Success Criteria

`sxmc` should eventually make these statements true:

- “Our AI knowledge stays fresh as tools and projects change.”
- “Interfaces we discover can be delivered directly to the AI environments that need them.”
- “The stable onboarding and maintenance workflow remains predictable across `1.x`.”
- “We can tell what our AI environment knows, what is stale, and how to fix it.”
