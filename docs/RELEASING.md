# Releasing

1. Set `version` in `Cargo.toml`, run `cargo build` to refresh `Cargo.lock`, and commit both.
2. Tag and push: `git tag vX.Y.Z && git push origin main vX.Y.Z`. The `Release` workflow tests, builds
   the macOS archives and creates the GitHub release.
3. Update the formula in [`g4rcez/homebrew-tap`](https://github.com/g4rcez/homebrew-tap): set `version`
   and the two `sha256` values. The checksums are in the `SHA256SUMS` file of the release:

   ```sh
   curl -sL https://github.com/g4rcez/rexflash/releases/download/vX.Y.Z/SHA256SUMS
   ```
