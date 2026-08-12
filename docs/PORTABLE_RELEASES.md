# Portable release bundles

Portable bundles provide a repeatable archive containing the graphical editor and command-line
tools. A pushed `v*` tag uses the exact tag name as the embedded bundle version, verifies every
archive against its neighboring SHA-256 file, records GitHub artifact provenance, and publishes
all platform assets on the corresponding GitHub Release. Installer, platform code signing and
notarization, and automatic updates remain separate milestones. The graphical editor separately
maintains isolated local crash-recovery records for committed unsaved ROM changes.

## Bundle contents

`lm-package` creates `lunar-magic-rust-VERSION-TARGET.tar.gz` and a neighboring
`.tar.gz.sha256` checksum plus a canonical `.tar.gz.update` update manifest. The archive contains
one top-level directory and these files in a stable order:

- `lm-native`, `lm-cli`, and the isolated `lm-libretro` live-emulator backend (with `.exe` on Windows)
- `README.md`
- `LICENSE-MIT` and `LICENSE-APACHE`
- `RELEASE-MANIFEST.txt`

The release manifest records the version, target, byte length, and SHA-256 digest of every other
payload. Tar ownership, timestamps, modes, ordering, and gzip timestamps are normalized, so the
same inputs and arguments produce byte-identical archives. Inputs must be bounded regular files.
Publication uses create-new semantics and cleans up a newly created archive if its checksum cannot
or update manifest cannot also be created; an existing output is never replaced. The bounded
`LMUPDATE1` manifest binds the version, platform target, archive filename, exact byte length, and
SHA-256 digest. Consumers reject non-newer versions, wrong targets, malformed components, length
mismatches, and digest mismatches before offering an update.

After verification, the updater stages the archive with create-new semantics in an explicitly
selected directory, flushes it to stable storage, reopens it, and repeats the complete
version/target/length/digest verification. Verification failures create no file, collisions never
replace an existing archive, and write/reopen/final-verification failures remove only the newly
created staged file.

In the native editor, choose **Help → Stage verified update…**, select the `.update` file beside
its declared archive, review the verified version/platform/name/size, then explicitly choose a
staging folder. Merely selecting an offer creates no files. The editor does not replace its running
executable or relaunch automatically; the staged archive is ready for manual installation after
exit.

The extraction core installs a verified bundle into a brand-new
`lunar-magic-rust-VERSION-TARGET` directory. It never edits the running version in place. Only flat
regular files below that exact archive prefix are accepted; links, devices, traversal, nested paths,
duplicates, invalid tar checksums/sizes, decompression overflow, and missing required executables
fail with complete cleanup of the new directory. An existing version directory is preserved.

Version activation uses a small `LMCURRENT1` selector in the install root, never an in-place binary
replacement. The selector binds a direct child version directory and executable filename to the
executable's SHA-256 digest, is published via a synced same-directory temporary rename, and retains
the prior valid selector for rollback. Resolution canonicalizes containment and rehashes the target
before launch, so a moved, external, or modified executable fails closed.

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
