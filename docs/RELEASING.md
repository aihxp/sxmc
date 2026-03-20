# Releasing sxmc

## Release Goals

`sxmc` is distributed as:
- a Rust crate source package
- prebuilt GitHub Release binaries for macOS, Linux, and Windows

The repository is set up so that tag pushes produce release archives
automatically.

## Before Releasing

1. Update version in `Cargo.toml`
2. Make sure `README.md` and `docs/CLIENTS.md` reflect the current public MCP surface
3. Run:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo package
```

## Create a Release Tag

```bash
git tag v0.1.0
git push origin v0.1.0
```

The GitHub Actions release workflow will build archives for:
- `x86_64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

## Publish to crates.io

Once you are ready to make the crate publicly installable:

```bash
cargo login
cargo publish
```

After publishing, users can install with:

```bash
cargo install sxmc
```

Until then, users can install from Git:

```bash
cargo install --git https://github.com/aihxp/sxmc
```

## Public Surface to Keep Stable

If possible, avoid breaking these without a version bump and README update:
- `sxmc serve`
- hybrid tools:
  - `get_available_skills`
  - `get_skill_details`
  - `get_skill_related_file`
- script-backed tool naming convention
- `sxmc stdio` and `sxmc http` CLI flags
