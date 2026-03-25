# Operations Guide

This guide consolidates deployment, release, and distribution notes.

## Hosted MCP

Serve a remote MCP endpoint:

```bash
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --max-concurrency 64 \
  --max-request-bytes 1048576 \
  --bearer-token env:SXMC_MCP_TOKEN \
  --paths /absolute/path/to/skills
```

Or require an exact header:

```bash
sxmc serve --transport http --host 0.0.0.0 --port 8000 \
  --max-concurrency 64 \
  --max-request-bytes 1048576 \
  --require-header "X-Internal-Token: secret-value" \
  --paths /absolute/path/to/skills
```

Key operational endpoints:

- MCP endpoint: `/mcp`
- health check: `/healthz`

Recommended default for hosted deployments:

- prefer `--bearer-token env:...` for a single shared token
- use `--require-header` for stricter internal or proxy-based setups
- keep the service behind a reverse proxy for TLS and access control
- keep request-body and concurrency limits in place for public or semi-public deployments

## Release Process

Before a release:

1. run the validation commands in [`VALIDATION.md`](VALIDATION.md)
2. update `CHANGELOG.md`
3. confirm `Cargo.toml` and packaging metadata are aligned
4. confirm `README.md` matches the current public surface
5. confirm the architecture and usage docs still match the shipped command set
6. confirm [`PRODUCT_CONTRACT.md`](PRODUCT_CONTRACT.md) and [`STABILITY.md`](STABILITY.md) still describe the shipped UX truthfully
7. confirm `scripts/test-sxmc.sh` is green against the release candidate binary

Release steps:

```bash
git tag vX.Y.Z
git push origin master --tags
cargo publish
```

GitHub Actions will build the release archives and checksums from the pushed
tag.

Release cadence policy:

- batch related changes into meaningful versions
- avoid rapid-fire public releases unless you are correcting a broken published release
- prefer validating the full docs, packaging, and smoke path once per release instead of shipping every intermediate checkpoint

Cross-platform release confidence:

- GitHub Actions runs Ubuntu, macOS, and Windows test lanes on `master`
- Unix CI lanes run `scripts/test-sxmc.sh` against the built debug binary
- Windows CI validates `doctor`, compact inspection, and cache stats through
  PowerShell JSON checks in addition to `cargo test`

## `1.x` Release Semantics

For the `1.x` line, release notes should distinguish clearly between:

- stable workflow commitments
  - onboarding with `setup` and `add`
  - maintenance with `status`, `doctor`, and `sync`
- additive capability growth
  - new discovery surfaces
  - richer JSON/status fields
  - broader host coverage
- best-effort heuristics
  - inferred summaries
  - quality scores
  - performance snapshots

That keeps Sumac honest: stable where it matters, still improving where
inference and ecosystem breadth naturally evolve.

## `1.x` Maintenance Discipline

For day-to-day `1.x` work:

- prefer additive changes over breaking rewrites
- protect the established JSON contracts for:
  - `sxmc add`
  - `sxmc setup`
  - `sxmc doctor`
  - `sxmc status`
  - `sxmc sync`
- treat regressions in the `setup -> add -> status -> sync` lifecycle as
  top-priority bugs
- update contract-facing docs when a user-visible workflow changes

## Distribution

Current distribution channels:

- crates.io
- GitHub Releases
- repo-local npm wrapper metadata in `packaging/npm`
- repo-local Homebrew formula in `packaging/homebrew/sxmc.rb`

The canonical install path remains:

```bash
cargo install sxmc
```

## Maintenance Notes

- keep `master` as the canonical branch
- keep branch protection enabled at least for force-push and deletion protection
- prefer `sxmc mcp` as the primary daily MCP client UX
- keep `sxmc stdio` and `sxmc http` as the lower-level raw bridge/debug layer
- keep docs focused on stable product paths rather than release-by-release notes
