# Changelog

All notable changes to Lunar Magic Rust are documented here.

## 1.0.0 — 2026-08-13

The first stable portable release.

### Highlights

- Native macOS, Linux, and Windows graphical SMW editor written in safe Rust.
- Composed in-game level and overworld rendering with direct canvas editing.
- Level objects, sprites, Layer 2, Map16, graphics, palettes, ExAnimation, entrances, exits,
  overworld routes, level nodes, and related native ROM workflows.
- Transactional ROM edits with validation, checksums, undo/redo, crash recovery, and Save As.
- Isolated live-emulator backend and permission-gated gameplay testing.
- Deterministic portable bundles for Linux x86-64, Windows x86-64, Apple Silicon macOS, and Intel
  macOS, including checksums, update manifests, provenance attestations, and rollback launcher.
- Extensive format, renderer, ROM-reopen, differential-oracle, and cross-platform test coverage.

### Compatibility

- A legally obtained Super Mario World ROM is required and is never distributed with the project.
- SMW US revision 0 is the primary built-in editing target.
- See `README.md` and `docs/FEATURE_PARITY_MATRIX.md` for the remaining compatibility limitations.
