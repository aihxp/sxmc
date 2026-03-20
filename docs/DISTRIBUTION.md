# Distribution

`sxmc` is primarily distributed as a native Rust CLI:

- crates.io: `cargo install sxmc`
- GitHub Releases: prebuilt archives per target

This repo also includes scaffolding for additional channels.

## npm Wrapper

The npm wrapper lives in [`packaging/npm`](../packaging/npm).

It is intentionally thin:

- the package installs a small launcher script
- `postinstall` downloads the matching GitHub Release binary for the platform
- the launcher forwards all arguments to the native `sxmc` binary

Planned publish target:

```bash
npm publish ./packaging/npm --access public
```

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

## Release Asset Naming

Current GitHub Release assets use this pattern:

```text
sxmc-vX.Y.Z-<target>.tar.gz
sxmc-vX.Y.Z-<target>.zip
```

Examples:

- `sxmc-v0.1.1-x86_64-unknown-linux-gnu.tar.gz`
- `sxmc-v0.1.1-aarch64-apple-darwin.tar.gz`
- `sxmc-v0.1.1-x86_64-pc-windows-msvc.zip`

Those names are what the npm wrapper expects when downloading binaries.
