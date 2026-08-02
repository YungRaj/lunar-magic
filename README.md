# Lunar Magic clean-room reimplementation

A cross-platform, clean-room reimplementation of Lunar Magic for Super Mario World, written in
safe Rust.

The project is building a native level editor and a reusable ROM-editing engine for macOS, Linux,
and Windows. It already opens supported SMW ROMs, renders and edits several level and overworld
domains, performs transactional ROM changes with undo/redo, and includes command-line tools for
format conversion and compatibility testing. It is active work and does **not** yet provide complete
Lunar Magic 3.63 feature parity.

![Native Rust level editor showing level 001, Map16 graphics, object tools, and the rendered canvas](docs/images/native-level-editor.png)

The native workspace keeps ROM-backed tools on the left and the scrollable level canvas on the
right. The screenshot shows the built-in SMW-US profile rendering level `$001`; see the
[illustrated usage guide](docs/USAGE.md) for the controls and safe editing workflow.

> This repository contains no Nintendo ROM data and no Lunar Magic source code. You must supply a
> legally obtained ROM. Keep an untouched backup and save edits to a separate file while the project
> remains under development.

## Quick start

### Requirements

- Rust 1.85 or newer
- Cargo
- A legally obtained Super Mario World ROM
- Platform build dependencies required by
  [`eframe`](https://github.com/emilk/egui/tree/master/crates/eframe)

The workspace uses Rust edition 2024 and forbids unsafe Rust.

### Build and test

From the repository root:

```sh
cargo build --workspace
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

### Run the graphical editor

Open the editor without a ROM:

```sh
cargo run -p lm-native
```

Open a ROM at startup:

```sh
cargo run -p lm-native -- "/path/to/Super Mario World (USA).sfc"
```

The built-in level workspace uses the complete remaining window: a bounded, independently
scrollable tool panel stays on the left while the level canvas retains the majority of the width
and all available editor height. The canvas has its own two-axis scrolling, so long horizontal or
vertical levels no longer push the editing surface below header, Map16, object, or sprite controls.

You can also use **File → Open ROM** after the application starts. Use **Save As** and choose a new
output path. Do not use your only copy of a ROM as a working file.

Supported startup options:

```text
lm-native [ROM] [--rom ROM] [--profile FILE]
          [--ui-config FILE] [--tools-config FILE] [--recent-state FILE]
```

### Run the command-line tools

Print the complete command usage:

```sh
cargo run -p lm-cli -- help
```

A few useful examples:

```sh
# Identify and inspect a ROM.
cargo run -p lm-cli -- inspect game.sfc

# Inspect RATS allocations.
cargo run -p lm-cli -- rats game.sfc

# Inspect an exported Lunar Magic level.
cargo run -p lm-cli -- mwl level.mwl

# Render a portable level bundle to PNG.
cargo run -p lm-cli -- render-level \
  level.lmlevel all.lm16set graphics.lmgfx palette.lmpal \
  16 27 0 0 level.png
```

The CLI contains many lower-level and oracle-oriented commands. Consult its generated help and
search `crates/lm-cli/src` for the implementation of a particular command.

## What works today

The project currently has tested support for substantial parts of the SMW editing pipeline:

- LoROM, FastROM, ExLoROM, and SA-1-aware ROM addressing, copier headers, checksums, expansion,
  changed-range tracking, IPS patches, and safe RATS allocation.
- LZ2, recovered LZ3, terminated RLE, and native sized-RLE codecs.
- Native Lunar Magic object, sprite, Layer 2, Layer 3, Map16, palette, graphics, ExAnimation, MWL,
  entrance, screen-exit, and secondary-exit data models.
- Native rendering for Map16, graphics, palettes, Layer 1, object-backed Layer 2, compressed Layer 2,
  selected Layer 3 content, entities with known appearance definitions, and editor overlays.
- A graphical level workspace with level navigation, camera controls, selection, object/sprite
  editing, compressed Layer 2 tile selection/painting, and object-backed Layer 2 list/canvas
  placement and dragging in the automatically detected vanilla workspace, native asset panels,
  shared undo/redo, and revision-checked atomic saves.
- A profile-free graphical SMW-US Map16 workspace for visually selecting tiles across all eight
  native definition pages, editing their four subtiles and Acts-Like values, previewing selectable
  level/tileset/palette contexts, and protected transactional installation. Pristine 512 KiB ROMs
  receive safe default allocation bounds and expand to 1 MiB in the same undoable commit.
- Overworld models and editors for layers, events, paths, warps, messages, level names, player
  starts, sprites, palettes, ExAnimation, title-screen assets, credits, and related installed
  runtimes.
- Transactional project changes: a failed validation, allocation, pointer update, or checksum repair
  does not publish a partial edit.
- A differential oracle framework with retained Lunar Magic 3.63 fixtures and semantic comparison
  that ignores irrelevant allocation placement.
- A permission-gated external-emulator workflow that runs the current in-memory ROM snapshot,
  supports configured level-context arguments, and owns stop/reap/temporary-file cleanup.
- Checksummed native crash recovery for committed unsaved ROM revisions, restored as an unnamed
  dirty project so the recovered result must be published explicitly with Save As.

The detailed inventory, usage guides, and recovery evidence live in `docs/`:

- [Documentation index](docs/README.md)
- [Illustrated usage guide](docs/USAGE.md)
- [Detailed implementation notes](docs/IMPLEMENTATION_NOTES.md)
- [Reverse-engineering notes](docs/REVERSE_ENGINEERING.md)
- [Architecture](docs/REIMPLEMENTATION_ARCHITECTURE.md)
- [Compatibility and test matrix](docs/REIMPLEMENTATION_TEST_MATRIX.md)
- [Product feature-parity ledger](docs/FEATURE_PARITY_MATRIX.md)
- [Portable release bundles](docs/PORTABLE_RELEASES.md)

## What is not finished

This is not yet a drop-in replacement for Lunar Magic. Important remaining areas include:

- Complete rendering and editing behavior for every standard and custom object and sprite.
- Full fidelity for all level modes, special layer behavior, animation rules, and custom resources.
- Complete graphical workflows for every modeled ROM feature.
- Broader ROM-revision and ecosystem compatibility, including more SA-1 and modified-ROM fixtures.
- Exhaustive behavioral comparison against Lunar Magic 3.63 for all save and transfer operations.
- Usability work expected from a mature editor: polished tools, keyboard workflows, diagnostics,
  installers, signed releases, updates, and recovery of editor-local staged forms in addition to
  the implemented committed-ROM crash snapshot.

Do not infer feature parity from the number of implemented formats. A format is considered complete
only when its decode, edit, save, reopen, undo/redo, GUI, and differential compatibility evidence
all pass.

### Future automation and AI tooling

After the core editor and ROM formats are sufficiently complete, expose the reusable editing
operations through an optional MCP server. The server should let local automation clients such as
Codex, Claude, or Gemini inspect a project, construct and modify levels, build a ROM, launch an
emulator test, and consume structured validation results.

This interface must use the same validated project transactions as the GUI and CLI rather than
editing ROM bytes directly. Planned validation includes parse/reopen checks, preserved-range and
checksum checks, sprite/object boundary checks, emulator smoke tests, and deterministic snapshots.
Complex and Kaizo-style level support must come from complete format and gameplay-semantics
coverage; an AI-facing API is not a substitute for that compatibility work.

## Typical editing workflow

1. Make an immutable backup of the source ROM.
2. Start `lm-native` and open a supported ROM.
3. Inspect the detected game revision and mapper before editing.
4. Navigate to a level or open the relevant focused workspace.
5. Make a small edit and verify it in the native preview.
6. Save to a new ROM path.
7. Test the output in an emulator.
8. Report reproducible differences with the source/output hashes and exact operation.

The application uses revision-checked commands and undoable project transactions. Frontend widgets
do not write ROM bytes directly.

## Codebase tour

The dependency direction is intentional:

```text
formats/codecs
      ↓
ROM project transactions
      ↓
toolkit-independent application commands
      ↓
native GUI and CLI
```

| Crate | Responsibility |
| --- | --- |
| `lm-rom` | ROM images, mapper addressing, headers, checksums, expansion, IPS |
| `lm-codec` | LZ2, LZ3, and RLE codecs |
| `lm-rats` | Validated RATS scanning, allocation, replacement, and reclamation |
| `lm-level` | Levels, objects, sprites, exits, Map16, Layer 2/3, native level files |
| `lm-graphics` | SNES tiles, palettes, ExAnimation, graphics import and conversion |
| `lm-overworld` | Overworld layers, events, paths, messages, sprites, and metadata |
| `lm-title` | Title-screen movement recordings and emulator-state extraction |
| `lm-snes` | Typed 65C816 code construction for independently authored runtimes |
| `lm-profile` | Revision-specific SMW layouts and recovered runtime descriptions |
| `lm-project` | Atomic ROM operations, pointer updates, history, undo, and redo |
| `lm-render` | Deterministic software rendering and PNG output |
| `lm-oracle` | Lunar Magic fixture capture, normalization, replay, and comparison |
| `lm-app` | GUI-neutral application state, commands, effects, and controllers |
| `lm-native` | Cross-platform `egui`/`eframe` graphical frontend |
| `lm-cli` | Headless format, rendering, ROM-editing, and oracle workflows |
| `lm-package` | Deterministic create-new portable release archives and SHA-256 manifests |

### Where to start reading

For a new contributor:

1. Read [the architecture guide](docs/REIMPLEMENTATION_ARCHITECTURE.md).
2. Read `crates/lm-app/src/lib.rs` to understand the command boundary.
3. Follow one feature vertically:
   - its data model in `lm-level`, `lm-graphics`, or `lm-overworld`;
   - its transaction in `lm-project`;
   - its controller/command in `lm-app`;
   - its GUI in `lm-native` or command in `lm-cli`;
   - its fixture and tests.
4. Use [the reverse-engineering ledger](docs/REVERSE_ENGINEERING.md) for recovered addresses, layouts, and
   confidence notes.
5. Use [the compatibility test matrix](docs/REIMPLEMENTATION_TEST_MATRIX.md) to find gaps that still
   need compatibility evidence.

## Design rules

### Preserve data the operation does not own

Unknown bits, extension bytes, unrecognized records, and unrelated ROM ranges must survive a
round trip. Models expose known semantics without normalizing opaque data away.

### Validate before mutation

Parsing, shape checks, mapping, allocation, and pointer validation happen before publication. A
failed operation must leave both ROM bytes and history unchanged.

### Keep the GUI thin

The native frontend collects input and displays state. Editing semantics belong in reusable models,
project transactions, and toolkit-independent controllers. GUI actions are bound to a document
revision so stale views cannot overwrite newer edits.

### Prefer semantic compatibility

Lunar Magic may choose different free-space locations between runs. Compatibility tests compare the
meaning of owned data, changed ranges, preserved ranges, checksums, and reopen behavior—not merely
whether two complete ROM images are byte-for-byte identical.

### No guessed compatibility

Revision-specific addresses and behaviors belong in explicit profiles backed by decompilation,
dynamic analysis, or reproducible fixtures. Unsupported layouts should fail clearly rather than be
silently treated as a known revision.

## Testing

Run a focused crate while developing:

```sh
cargo test -p lm-level
cargo test -p lm-project
cargo test -p lm-native
```

Before committing:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

To capture the actual native framebuffer after startup without granting an external screen-recording
tool access, enable the opt-in visual-smoke feature:

```sh
LM_NATIVE_SCREENSHOT_TO=/tmp/lm-native.png \
  cargo run -p lm-native --features visual-smoke -- \
  --level 105 "Super Mario World (USA).sfc"
```

To capture a later part of a level, offset the preview camera in level-major and perpendicular
tiles. For a horizontal level, `MAJOR` moves right and `MINOR` moves down; vertical levels swap
those screen axes:

```sh
LM_NATIVE_SCREENSHOT_TO=/tmp/lm-cookie-mountain.png \
LM_NATIVE_PREVIEW_CAMERA_MAJOR=15 \
  cargo run -p lm-native --features visual-smoke -- \
  --level 001 "Super Mario World (USA).sfc"
```

The process waits for the populated workspace to settle, captures its own main viewport, writes a
PNG, and exits. This exercises real window creation, GPU painting, startup ROM loading, and native
editor composition; it does not simulate pointer or keyboard interaction. Level numbers use Lunar
Magic's hexadecimal notation, so `--level 102` opens level `$102`. Treat the output as a local
diagnostic and never commit screenshots containing ROM-derived graphics.

For a resumable visual audit, render a level/style/screen-offset matrix into a hashed HTML contact
sheet. Independent framebuffer captures can run concurrently, and macOS hosts losslessly
recompress the otherwise intentionally uncompressed diagnostic PNGs:

```sh
LM_RENDER_AUDIT_JOBS=8 \
  tools/render-audit.sh /tmp/lm-render-audit "Super Mario World (USA).sfc" all 0 game

open /tmp/lm-render-audit/index.html
```

The optional `LEVELS`, `SCREENS`, and `STYLES` arguments are comma-separated. `SCREENS` are
hexadecimal major-axis screen offsets from each level's authenticated entrance, so `0,1,2` captures
the entrance view and the next two screens. Use `game,editor` to review both pixel and editor
presentation modes. Existing PNGs are retained on rerun while the ordered manifest and contact
sheet are regenerated.

Wine's window-capture APIs can return an all-black image on macOS. For Lunar Magic 3.63 raster
comparisons, build the address-authenticated helper and read its live primary level-editor DIB
instead:

```sh
i686-w64-mingw32-gcc -std=c11 -O2 -Wall -Wextra -Werror \
  tools/wine-level-dib-capture.c -o /tmp/wine-level-dib-capture.exe

wine /tmp/wine-level-dib-capture.exe \
  "Lunar Magic.exe" 'Z:\tmp\lunar-magic-level.bmp'
```

The helper writes the exact top-down 32-bit BGRA surface created by
`CreatePrimaryEditorRenderSurface` at Lunar Magic address `$0044DA20`; it validates dimensions and
the live pointer before reading any pixels. The globals are specific to the authenticated 3.63
binary and must be re-established in Ghidra before using the helper with another version.

To switch through a list of levels automatically and publish the live DIB captures as a hashed
gallery, keep Lunar Magic 3.63 open under Wine and run:

```sh
tools/lunar-magic-reference-audit.sh \
  /tmp/lm-live-reference \
  001,002,003,100,101,102 \
  /tmp/lm-render-audit

open /tmp/lm-live-reference/index.html
```

The optional third argument is an existing Rust render-audit directory. When supplied, each gallery
card places Lunar Magic's live editor DIB beside the matching Rust editor framebuffer. The script
builds the 32-bit Wine helpers from the authenticated sources, drives Lunar Magic's Open Level
command, retains resumable PNG captures, and writes an ordered manifest. Use
`LM_WINE_EXECUTABLE` to select a differently named target process and `LM_MINGW_CC` to select the
32-bit MinGW compiler.

Some integration tests use the legally supplied `Super Mario World (USA).sfc` fixture at the
repository root. The expected pristine SHA-1 is:

```text
6b47bb75d16514b6a476aa0c73a683a2a4c18765
```

Never commit ROMs, emulator states containing copyrighted data, or proprietary Lunar Magic files.
Fixtures committed to the repository must contain only redistributable metadata, hashes, patches,
observations, or independently authored data.

## Reverse-engineering workflow

Lunar Magic 3.63 is treated as a behavioral oracle, not as a source dependency:

1. Identify a behavior or format gap.
2. Record static evidence in Ghidra and, where useful, observe the executable under Wine.
3. Capture a minimal legal before/after fixture with exact hashes and operation arguments.
4. Model the semantics in a format crate.
5. Implement a failure-atomic project operation.
6. expose it through `lm-app`, then the native GUI and/or CLI.
7. reopen the result and compare semantic observations and preserved regions.
8. document confidence and remaining unknowns.

Keep recovered facts and addresses in [the reverse-engineering ledger](docs/REVERSE_ENGINEERING.md). Keep
implementation prose that would overwhelm this overview in
[the detailed implementation notes](docs/IMPLEMENTATION_NOTES.md).

## Contributing

- Keep changes focused and add tests at the lowest useful layer.
- Use typed errors; do not panic on malformed external input.
- Preserve opaque bytes unless the edit explicitly owns them.
- Avoid embedding revision-specific addresses outside `lm-profile`.
- Add controller tests for stale revisions, failure atomicity, undo/redo, and reopen behavior.
- Add GUI tests for input-to-command routing where practical.
- Run formatting, all workspace tests, and strict Clippy before pushing.
- Update the compatibility matrix when evidence changes a feature's status.

When reporting a bug, include:

- platform and Rust version;
- source ROM revision, mapper, header state, and cryptographic hash;
- the exact command or GUI operation;
- expected and actual behavior;
- output/error text;
- whether Lunar Magic 3.63 behaves differently on the same input.

## License

The workspace is dual-licensed under MIT or Apache-2.0. Nintendo assets, commercial ROMs, and Lunar
Magic itself are not included or licensed by this repository.
