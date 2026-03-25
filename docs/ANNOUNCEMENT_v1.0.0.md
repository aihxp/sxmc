# Sumac 1.0.0

Sumac (`sxmc`) is now in its first stable `1.x` line.

## What is stable

The core maintained workflow is:

```text
setup -> add -> status -> sync
```

That means:

- `sxmc setup` is the first-run onboarding path
- `sxmc add` is the one-tool onboarding path
- `sxmc status` is the unified health and knowledge view
- `sxmc sync` is the local reconciler for saved profiles and generated host artifacts

## What 1.0.0 promises

- stable onboarding and maintenance commands
- stable machine-readable output contracts for `add`, `setup`, `doctor`,
  `status`, and `sync`
- additive evolution for richer JSON and discovery metadata
- explicit best-effort boundaries for inferred summaries and ecosystem quirks

## Validation snapshot

- `296` tests passed
- `0` failed
- `0` skipped
- `94` installed CLI tools parsed successfully

See also:

- [STABILITY.md](STABILITY.md)
- [PRODUCT_CONTRACT.md](PRODUCT_CONTRACT.md)
- [TEST_SUITE_REPORT_v1.0.0.md](TEST_SUITE_REPORT_v1.0.0.md)
