# Homebrew Packaging

The in-repo formula at [`sxmc.rb`](./sxmc.rb) is intended to be copied into a
tap repository, for example:

```text
aihxp/homebrew-tap/Formula/sxmc.rb
```

Typical release flow:

1. Cut and push the `vX.Y.Z` tag for `sxmc`
2. Wait for the GitHub Release assets to finish uploading
3. Compute or confirm the source archive `sha256`
4. Update the formula in the tap repo:
   - `url "https://github.com/aihxp/sxmc/archive/refs/tags/vX.Y.Z.tar.gz"`
   - matching `sha256`
5. Push the tap update

Install after the tap exists:

```bash
brew tap aihxp/tap
brew install sxmc
```

For local validation of the in-repo formula:

```bash
ruby -c packaging/homebrew/sxmc.rb
brew install --build-from-source ./packaging/homebrew/sxmc.rb
```
