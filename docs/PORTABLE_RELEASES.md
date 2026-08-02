# Portable release bundles

Portable bundles are the first release-engineering milestone for the Rust editor. They provide a
repeatable archive containing the graphical editor and command-line tool; they do not yet provide
an installer, code signing or notarization, or automatic updates. The graphical editor separately
maintains isolated local crash-recovery records for committed unsaved ROM changes.

## Bundle contents

`lm-package` creates `lunar-magic-rust-VERSION-TARGET.tar.gz` and a neighboring
`.tar.gz.sha256` checksum. The archive contains one top-level directory and these files in a stable
order:

- `lm-native` and `lm-cli` (with `.exe` on Windows)
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
cargo build --locked --release --target x86_64-unknown-linux-gnu -p lm-native -p lm-cli
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
Intel bundles. It retains the resulting CI artifacts for 14 days. The workflow deliberately does
not publish a GitHub release: release publication, platform signing, and long-term retention need a
separate reviewed policy.
