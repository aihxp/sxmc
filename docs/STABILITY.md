# Stability Guide

This document defines what the `1.x` line means for Sumac (`sxmc`) users.

Read it together with:

- [PRODUCT_CONTRACT.md](PRODUCT_CONTRACT.md) for the exact support boundary
- [USAGE.md](USAGE.md) for the canonical daily workflows
- [VALIDATION.md](VALIDATION.md) for the release bar and current validation pass

## What `1.0.0` Means

For Sumac, `1.0.0` is not a promise that every possible integration is perfect.
It is a promise that the core user workflow is stable enough to depend on:

1. discover or inspect a tool or surface
2. onboard it into AI host artifacts or MCP
3. observe current state with `status`
4. reconcile drift with `sync`

## Stable Commands

These command families are the stable product spine for the `1.x` line:

- `sxmc setup`
- `sxmc add`
- `sxmc doctor`
- `sxmc status`
- `sxmc sync`
- `sxmc wrap`
- `sxmc serve`
- `sxmc mcp`
- `sxmc api`
- `sxmc discover`

That means:

- command names should not move casually
- primary flags should not be renamed casually
- aliases such as `--client` and `--host` should keep their current meaning
- breaking changes should wait for a new major version

## Stable Machine-Readable Output

The following surfaces are treated as stable machine-readable contracts:

- `sxmc add --format ...`
- `sxmc setup --format ...`
- `sxmc doctor --format ...`
- `sxmc status --format ...`
- `sxmc sync --format ...`

For these outputs, Sumac should prefer:

- additive fields over field removal
- additive enum/state growth over silent semantic rewrites
- stable top-level shapes
- explicit recovery hints instead of implicit behavior changes

## Additive Evolution Rules

These changes are acceptable within the `1.x` line:

- adding new JSON fields
- adding new optional host support
- adding new discovery surfaces
- adding richer health or recovery metadata
- adding new warning or note fields where older consumers can safely ignore them

These changes should be treated as breaking and avoided until a major version:

- removing established top-level fields from stable machine-readable surfaces
- renaming stable commands without an alias path
- changing the meaning of existing host names or aliases
- changing stable exit-code semantics for `doctor`, `status --health`, or `sync --check`

## What Is Still Best-Effort

Some Sumac behavior is intentionally useful-but-not-absolute:

- inferred CLI summaries and descriptions
- profile quality scores
- discovery heuristics
- third-party MCP server quirks
- performance numbers

These should improve over time, but they are not stronger guarantees than the
stable workflow and output contract.

## Supported Daily Loop

The intended stable daily loop is:

```text
setup -> add -> status -> sync
                 \-> wrap / serve / discover / mcp / api as needed
```

If that loop regresses, it should be treated as a product-level bug, not just a
feature-specific issue.

## `1.x` Maintenance Discipline

For the stable `1.x` line, maintainers should default to:

- additive changes over destructive ones
- preserving the established machine-readable shapes for:
  - `sxmc add`
  - `sxmc setup`
  - `sxmc doctor`
  - `sxmc status`
  - `sxmc sync`
- treating regressions in the `setup -> add -> status -> sync` lifecycle as
  top-priority bugs
- documenting any user-visible behavior change in the README, product
  contract, or usage guide before release

In practice that means:

- prefer new fields over renaming or removing existing ones
- prefer aliases over command/flag churn
- prefer explicit migration notes over silent behavior changes

## Release Bar For `1.x`

Before shipping a `1.x` release, we should be able to say:

- the support boundary in [PRODUCT_CONTRACT.md](PRODUCT_CONTRACT.md) still matches the product
- the stable workflows in [USAGE.md](USAGE.md) still work as documented
- the current validation pass in [VALIDATION.md](VALIDATION.md) is green
- the machine-readable onboarding/maintenance surfaces remain additive and reviewable
