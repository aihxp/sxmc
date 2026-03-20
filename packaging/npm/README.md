# `@aihxp/sxmc`

This package is a thin npm wrapper around the native `sxmc` Rust binary.

## Install

```bash
npm install -g @aihxp/sxmc
```

During `postinstall`, the package downloads the matching GitHub Release archive
for the current platform and unpacks the `sxmc` binary into `vendor/`.

## Usage

```bash
sxmc --version
sxmc serve
```

## Notes

- This package expects GitHub Release assets named like
  `sxmc-v0.1.1-x86_64-unknown-linux-gnu.tar.gz`.
- Supported targets match the release workflow:
  - `x86_64-unknown-linux-gnu`
  - `x86_64-apple-darwin`
  - `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc`
- Set `SXMC_NPM_SKIP_DOWNLOAD=1` to skip the download during local development.
