# Portable release bundles

Portable bundles provide a repeatable archive containing the graphical editor and command-line
tools. A pushed `v*` tag uses the exact tag name as the embedded bundle version, verifies every
archive against its neighboring SHA-256 file, records GitHub artifact provenance, and publishes
all platform assets on the corresponding GitHub Release. Installer, platform code signing and
notarization, and automatic updates remain separate milestones. The graphical editor separately
maintains isolated local crash-recovery records for committed unsaved ROM changes.

## Bundle contents

`lm-package` creates `lunar-magic-rust-VERSION-TARGET.tar.gz` and a neighboring
`.tar.gz.sha256` checksum. The archive contains one top-level directory and these files in a stable
order:

- `lm-native`, `lm-cli`, and the isolated `lm-libretro` live-emulator backend (with `.exe` on Windows)
- `README.md`
- `LICENSE-MIT` and `LICENSE-APACHE`
- `RELEASE-MANIFEST.txt`

The release manifest records the version, target, byte length, and SHA-256 digest of every other
payload. Tar ownership, timestamps, modes, ordering, and gzip timestamps are normalized, so the
same inputs and arguments produce byte-identical archives. Inputs must be bounded regular files.
Publication uses create-new semantics and cleans up a newly created archive if its checksum cannot
also be created; an existing output is never replaced.

## Build and verify locally

For a native Linux x86-64 build, run:

```sh
cargo build --locked --release --target x86_64-unknown-linux-gnu -p lm-native -p lm-cli -p lm-libretro
cargo run --locked --release -p lm-package -- \
  --bin-dir target/x86_64-unknown-linux-gnu/release \
  --output-dir dist \
  --target x86_64-unknown-linux-gnu \
  --version 0.1.0
```

Verify and inspect the result before distributing it:

```sh
cd dist
shasum -a 256 -c lunar-magic-rust-0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -tzf lunar-magic-rust-0.1.0-x86_64-unknown-linux-gnu.tar.gz
tar -xOzf lunar-magic-rust-0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  lunar-magic-rust-0.1.0-x86_64-unknown-linux-gnu/RELEASE-MANIFEST.txt
```

On Windows, compare `Get-FileHash -Algorithm SHA256` with the first field of the checksum file.

## Continuous integration scope

The portable-release workflow builds Linux x86-64, Windows x86-64, macOS Apple Silicon, and macOS
Intel bundles. Manual runs produce `0.1.0-dev` CI artifacts retained for 14 days. A pushed `v*` tag
instead derives the version from that tag and, only after all four matrix builds succeed, downloads
the complete set, verifies every checksum with strict parsing, attests the artifacts, and creates
the matching GitHub Release with generated notes. The release job has scoped `contents: write`,
`id-token: write`, and `attestations: write` permissions; build jobs retain read-only access.
