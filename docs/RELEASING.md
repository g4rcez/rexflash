# Releasing

1. Set `version` in `Cargo.toml`, run `cargo build` to refresh `Cargo.lock`, and commit both.
2. Tag and push: `git tag vX.Y.Z && git push origin main vX.Y.Z`. The `Release` workflow tests, builds
   the macOS archives and creates the GitHub release.
3. Update the formula in [`g4rcez/homebrew-tap`](https://github.com/g4rcez/homebrew-tap): set `url` to
   the new tag's source archive and `sha256` to its checksum:

   ```sh
   curl -sL https://github.com/g4rcez/rexflash/archive/refs/tags/vX.Y.Z.tar.gz | shasum -a 256
   ```
