# `@aihxp/sxmc`

This package is a thin npm wrapper around the native `sxmc` Rust binary.

The wrapper metadata in this repo is aligned to **`0.1.4`** and expects the
matching GitHub Release assets for that version to exist before publish.

## Install

```bash
npm install -g @aihxp/sxmc
```

During `postinstall`, the package downloads the matching GitHub Release archive
for the current platform, verifies the matching `.sha256` file, and unpacks the
`sxmc` binary into `vendor/`.

## Usage

```bash
sxmc --version
sxmc serve
```

## Notes

- Keep the npm package version aligned with:
  - `Cargo.toml`
  - the Git tag / GitHub Release
  - the Homebrew formula version if you update distribution docs together
- `publishConfig.access` is already set to `public`, so the publish command is:
  `npm publish ./packaging/npm`
- This package expects GitHub Release assets named like
  `sxmc-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`.
- This package also expects matching checksum assets named like
  `sxmc-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz.sha256`.
- Supported targets match the release workflow:
  - `x86_64-unknown-linux-gnu`
  - `x86_64-apple-darwin`
  - `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc`
- Unsupported platforms should use `cargo install sxmc` or build from source.
- Set `SXMC_NPM_SKIP_DOWNLOAD=1` to skip the download during local development.
- Set `SXMC_NPM_DOWNLOAD_BASE=https://...` to point the installer at a custom
  release mirror.
- Current release tag alignment in-repo: **`v0.1.4`**
