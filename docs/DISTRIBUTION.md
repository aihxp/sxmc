# Distribution

`sxmc` is primarily distributed as a native Rust CLI:

- crates.io: `cargo install sxmc`
- GitHub Releases: prebuilt archives per target
- npm wrapper: thin installer for prebuilt GitHub Release binaries
- Homebrew formula: source-build formula intended for a tap

The Rust crate and GitHub Release assets remain canonical. The npm wrapper and
Homebrew formula are convenience distribution channels layered on top.

Current repo alignment:

- crate version: `0.1.2`
- npm wrapper metadata: `0.1.2`
- Homebrew formula source tarball: `v0.1.2`
- GitHub Release binaries: `v0.1.2`

## npm Wrapper

The npm wrapper lives in [`packaging/npm`](../packaging/npm).

It is intentionally thin and now publish-ready:

- the package installs a small launcher script
- `postinstall` downloads the matching GitHub Release binary for the platform
- the installer verifies the matching `.sha256` asset before unpacking
- the launcher forwards all arguments to the native `sxmc` binary

Publish target:

```bash
npm publish ./packaging/npm --access public
```

Before publishing, verify that the matching GitHub Release assets already exist
for the wrapper version, including the checksum files. The current in-repo
wrapper is aligned to `v0.1.2`.

Before publishing, keep the npm package version aligned with:

- `Cargo.toml`
- the Git tag
- the GitHub Release asset names

Useful npm-specific knobs:

- `SXMC_NPM_SKIP_DOWNLOAD=1` skips the postinstall download for local development
- `SXMC_NPM_DOWNLOAD_BASE=https://...` points the wrapper at a different release mirror

## Homebrew Formula

A source-build Homebrew formula lives in [`packaging/homebrew/sxmc.rb`](../packaging/homebrew/sxmc.rb).

That formula is suitable for copying into a tap repository such as:

```text
aihxp/homebrew-tap/Formula/sxmc.rb
```

Example install target after setting up a tap:

```bash
brew install aihxp/tap/sxmc
```

If you promote the formula into a real tap, update the tarball URL and `sha256`
for each released version. The in-repo formula is currently pinned to the
`v0.1.2` source archive.

Tap-specific guidance lives in [`packaging/homebrew/README.md`](../packaging/homebrew/README.md).

## Release Asset Naming

Current GitHub Release assets use this pattern:

```text
sxmc-vX.Y.Z-<target>.tar.gz
sxmc-vX.Y.Z-<target>.zip
```

Examples:

- `sxmc-v0.1.2-x86_64-unknown-linux-gnu.tar.gz`
- `sxmc-v0.1.2-aarch64-apple-darwin.tar.gz`
- `sxmc-v0.1.2-x86_64-pc-windows-msvc.zip`

Those names are what the npm wrapper expects when downloading binaries.
The wrapper also expects matching checksum files with the same name plus
`.sha256`.
