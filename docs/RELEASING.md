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
bash scripts/smoke_test_clients.sh target/debug/sxmc tests/fixtures
```

4. Smoke-test both MCP entrypoints:

```bash
sxmc serve --paths tests/fixtures
sxmc serve --transport http --host 127.0.0.1 --port 8000 --paths tests/fixtures
sxmc serve --transport http --host 127.0.0.1 --port 8000 \
  --require-header "Authorization: test-token" --paths tests/fixtures
sxmc serve --transport http --host 127.0.0.1 --port 8000 \
  --bearer-token test-token --paths tests/fixtures
```

## Create a Release Tag

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

The GitHub Actions release workflow will build archives for:
- `x86_64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

The workflow also publishes matching `.sha256` files for each archive.

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

docs.rs should rebuild automatically after the new crate version becomes
available.

## Optional Distribution Channels

Additional packaging scaffolds live in:

- [`packaging/npm`](../packaging/npm)
- [`packaging/homebrew/sxmc.rb`](../packaging/homebrew/sxmc.rb)
- [`docs/DISTRIBUTION.md`](DISTRIBUTION.md)

Until then, users can install from Git:

```bash
cargo install --git https://github.com/aihxp/sxmc
```

## Public Surface to Keep Stable

If possible, avoid breaking these without a version bump and README update:
- `sxmc serve`
- remote MCP endpoint shape: `sxmc serve --transport http ...` at `/mcp`
- remote MCP auth flag: `--require-header K:V`
- remote MCP bearer auth flag: `--bearer-token TOKEN`
- remote MCP health endpoint: `/healthz`
- hybrid tools:
  - `get_available_skills`
  - `get_skill_details`
  - `get_skill_related_file`
- script-backed tool naming convention
- `sxmc stdio` and `sxmc http` CLI flags
