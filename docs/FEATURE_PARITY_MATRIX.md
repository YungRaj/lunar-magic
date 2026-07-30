# Lunar Magic 3.63 feature-parity ledger

This ledger is the product-level completion boundary for the Rust reimplementation. It complements
`REIMPLEMENTATION_TEST_MATRIX.md`, whose rows primarily describe format and subsystem evidence.
Lunar Magic feature parity is not complete until every in-scope workflow below is `Pass`.

## Evidence and status rules

The original Lunar Magic 3.63 executable is the enumeration and behavior authority. The current
baseline contains 270 PE resources, including 107 dialog resources. Ghidra on port 8089 provides
the named implementation surface; Wine provides observable UI, file, and ROM-transition behavior.

Each workflow has five independent gates:

- **Model**: lossless decode/encode of every owned and opaque field.
- **Tx**: checked mutation, checksum repair, unchanged-range preservation, reopen, undo, and redo.
- **GUI**: the complete user workflow is reachable and usable in `lm-native`.
- **Oracle**: a retained Wine fixture or automated behavioral comparison covers success and
  rejection behavior.
- **Variants**: the supported ROM/format variants are explicitly enumerated and tested.

Gate values are `P` (proved), `~` (partial or indirect evidence), and `-` (missing). A row is
`Pass` only when all required gates are `P`. `N/A` is allowed only when the original workflow has
no corresponding gate, with the reason recorded in evidence.

Broad format support does not promote a row. For example, decoding MWL files does not prove the
interactive level import workflow, and a CLI transaction does not prove its native dialog.

## Workflow ledger

| Area | Original Lunar Magic workflow | Model | Tx | GUI | Oracle | Variants | Status | Primary evidence / next gap |
| --- | --- | :---: | :---: | :---: | :---: | :---: | --- | --- |
| Lifecycle | Open ROM, recent ROM, close, quit, dirty confirmation | P | P | P | ~ | ~ | Partial | `lm-app` lifecycle and native menus pass; automate Wine warning/cancel equivalence and modified-ROM cases |
| Lifecycle | Save and Save As with atomic replacement | P | P | P | ~ | ~ | Partial | Exact application snapshot acknowledgement passes; expand Wine failure/collision matrix |
| Lifecycle | Full-ROM restore points and associated-file restore | P | P | P | ~ | - | Partial | Exact `.lrp` header, linked `DIRL` records, record extents, stored/decoded checksums, raw-DEFLATE payloads, compact fill/raw commands, logical-ROM CRC-32, and bounded reconstruction pass; an LM 3.63 two-record archive restores the 2,097,664-byte target ROM byte-exactly from `smwOrig.smc`. All thirteen Ghidra-confirmed sidecar slots decode with incremental inheritance, per-record compression, and the pre-3.21 large-file offset rule. Native creation supports initial/appended full checkpoints, deltas, successful/failed reversion markers, consecutive-reversion replacement, reciprocal link/header updates, complete timestamp snapshots, and compressed/uncompressed ROM/sidecar data. Reconstruction follows the non-linear reversion graph. Native automatic-policy controls expose interval and daily full checkpoints while preserving continuity-break priority. The restore dialog atomically publishes the selected ROM, sidecars, and successful reversion marker, or records the native failed form. Open projects can create a new full archive or append manual full, delta, and automatic points. Native round-trip fixtures cover nonempty sidecars and timestamp-selected incremental sidecar storage. Add authentic Lunar Magic nonempty-sidecar fixtures and persist automatic settings |
| Lifecycle | Undo/redo with configured history limit | P | P | P | ~ | ~ | Partial | Shared project history passes; compare per-editor grouping and configured limit with Wine |
| ROM | Detect identity, mapper, copier header, checksum | P | P | P | P | ~ | Partial | Headered pristine fixture is now deterministic; broaden SA-1 and modified-ROM corpus |
| ROM | Expand ROM and mapper conversion rules | P | P | P | ~ | ~ | Partial | Native expansion dialog passes; recover all original size/ExLoROM/ZSNES compatibility choices |
| ROM | Add/remove copier header | P | P | P | ~ | P | Partial | Headered/headerless unit and process suite passes; add Wine dialog transition fixture |
| ROM | Create IPS patch | P | P | P | ~ | P | Partial | Native non-blocking original/modified/output workflow follows Ghidra `CreateIpsPatch` at `0041F0B0`, normalizes copier headers, rejects aliases, and publishes atomically; automate Wine dialogs |
| ROM | Apply IPS patch | P | P | P | ~ | P | Partial | Native preview/transaction passes; add Wine error and changed-range comparison |
| ROM | Scan/reclaim owned RATS blocks | P | P | P | N/A | ~ | Partial | Rust safety workflow intentionally requires ownership proof; broaden mapper variants |
| Level | Navigate all 512 slots and preserve viewport history | P | P | P | P | P | Pass | 512/512 rendering ledger plus application navigation tests |
| Level | Edit level header, mode, music, time, scroll, sprite header | P | P | P | ~ | ~ | Partial | Forms and ROM transactions pass; automate field-by-field Wine save comparison |
| Level | Edit primary/midway entrances | P | P | P | ~ | ~ | Partial | Native form and separate-midway runtime exist; incomplete Wine dialog/save matrix |
| Level | Edit screen exits | P | P | P | ~ | ~ | Partial | Typed fields and canvas route exist; compare all original exit dialog modes |
| Level | Edit secondary exits and clear operations | P | P | P | ~ | ~ | Partial | Complete 8192-entry backend and GUI exist; add Wine clear-one/clear-all fixtures |
| Level | Insert/edit/delete/move standard and extended objects | P | P | P | ~ | ~ | Partial | Authenticated object canvas works; complete interactive command-by-command Wine coverage |
| Level | Insert/edit/delete/move standard and custom sprites | P | P | P | ~ | ~ | Partial | Standard dispatch/rendering verified; broaden custom/expanded ROM fixtures and dialog parity |
| Level | Layer 2 object-backed editing | P | P | P | ~ | ~ | Partial | Shared history/canvas tests pass; add Wine object-mode interactions |
| Level | Layer 2 background tile editor, rectangle tools, undo | P | P | P | P | ~ | Partial | Ghidra-proved two-plane mapping and all focused tests pass; automate complete tool gestures |
| Level | Layer 3 settings, graphics, tilemap, remap commands | P | P | P | P | ~ | Partial | Wine import/re-export evidence exists; broaden installed runtime versions |
| Level | Per-level palette editing and import/export | P | P | P | ~ | ~ | Partial | Models and GUI exist; incomplete Wine ownership/animation-reservation matrix |
| Level | Per-level and global ExAnimation editing | P | P | P | P | ~ | Partial | Semantic Wine MWL fixture exists; broaden every trigger/type/legacy runtime |
| Level | Change bypassed GFX/ExGFX and animation options | ~ | ~ | ~ | - | - | Missing | Original graphics-set dialogs are richer than current level controls |
| Level | Custom object library (`.mw0`/`.mw0t`/`.osc`) | P | P | P | ~ | ~ | Partial | Paired document/editor passes; automate Lunar Magic reload and placement equivalence |
| Level | Custom sprite library (`.mw2`/`.mwt`/`.ssc`) | P | P | P | ~ | ~ | Partial | Size-table-bound editor passes; automate all SSC display/palette/source modes |
| Level | Import/export one binary MWL level | P | P | P | P | ~ | Partial | Complete controller, shell, native level-assets actions, and live Rust export → LM import/re-export oracle pass; broaden ROM/runtime variants |
| Level | Import/export legacy multi-file levels | P | P | P | ~ | ~ | Partial | Native two-stage bounded import and rollback-safe grouped export match the exact live level-105 `.mwl`/`.mw0`/`.mw1`/`.mw2` fixture and Ghidra `00477940`/`004796c0`; add custom-palette, historical-version, and Wine rejection fixtures |
| Level | Insert multiple levels from a directory | P | P | P | P | ~ | Partial | Shell and native progress dialog auto-target visible MWLs, skip hidden files, continue after failures, commit independently, and support Escape cancellation; broader variants remain |
| Level | Export all levels to a directory | P | P | P | P | ~ | Partial | Shell and non-blocking native actions implement all/modified modes, exact `%03X` naming, grouped publication, Ghidra predicate, and Wine-backed selection fixture; Escape cancellation and variants remain |
| Level | Export one/all level images as PNG/BMP | P | P | ~ | P | ~ | Partial | Renderer/CLI pass; reproduce original naming, bounds, prompts, and batch dialog |
| Level | Level usage analysis | P | N/A | P | P | ~ | Partial | Native background scan/progress/cancel and exact `LevelAnalysis.txt` framing pass; all four usage domains match Lunar Magic's authenticated 512-level working-ROM report, including zero Map16 count deltas; broaden modified-ROM variants (read-only workflow, so Tx is N/A) |
| Level | Restrict level access | P | N/A | P | P | ~ | Partial | Exact authenticated LM 3.63 ROM mutation, randomized protection material, native warning/title dialog, checksum preservation, and atomic rejection/undo pass; original restore-point/optional-IPS/direct-close workflow and non-LoROM variants remain |
| Level | LMSW/emulator level testing integration | ~ | - | - | - | - | Missing | Ghidra `LoadCurrentLevelIntoLmsw`, `LoadRomImageIntoLmsw`; release-safe replacement required |
| Map16 | Browse/edit subtiles, attributes, Acts Like, undo | P | P | P | ~ | ~ | Partial | Native editor now loads and atomically commits all 256 foreground/background pages; broaden Wine interaction and modified-set fixtures |
| Map16 | Import/export current page and complete sets | P | P | ~ | ~ | ~ | Partial | File models/CLI pass; native ROM editor now has bounded complete `.map16` import and create-new export across all 256 pages, preserves imported auxiliary/editor sections, and byte-exactly re-exports a genuine LM 3.63 fixture; original current-page multi-file GUI and prompt behavior remain incomplete |
| Map16 | Import bitmap with quantization, deduplication, previews | P | P | ~ | ~ | ~ | Partial | Native PNG and bounded background system-clipboard workflows load real level graphics/palette context, show synchronized pan/zoom previews, expose optimization controls and both recovered reduction choices, and model all 128 free/reusable/reserved palette states; the weighted set/subset allocator assigns one row per 8×8 tile and commits palette/GFX/Map16/runtime changes atomically. Median Cut matches the recovered moment/variance/average/SNES-rounding semantics, generated entries receive Lunar Magic's integer HSL row ordering before pixel remapping, and the low-color/Popularity paths match an exact Wine-observed four-color fixture; broader variants remain |
| Map16 | Clipboard bitmap and rectangle conversion | P | P | ~ | - | ~ | Partial | Clipboard models exist; disposable-process Wine automation now captures the original bitmap dialog plus pre/post palette and graphics buffers; compare broader option combinations and automate rectangle conversion |
| Graphics | 8×8 tile viewer/editor with selection, flips, copy/paste | P | P | P | ~ | ~ | Partial | Native graphics editor exists; complete original keyboard/status/zoom behavior unverified |
| Graphics | Extract/insert GFX and ExGFX | P | P | ~ | ~ | ~ | Partial | CLI/project operations pass; native original-equivalent file workflows incomplete |
| Graphics | Change compression and migrate dependent tables | P | P | P | P | ~ | Partial | Transactional dialog and recovered runtimes pass; broaden runtime/version variants |
| Graphics | External graphics editor launch and reload | P | ~ | ~ | - | ~ | Missing | Generic external tools exist; original GFX-slot workflow semantics are incomplete |
| Palette | Edit level/shared/overworld palettes | P | P | P | ~ | ~ | Partial | Multiple native editors pass; complete ownership and Wine interaction matrix remains |
| Palette | Import/export full palette, row, shared palette formats | P | P | ~ | ~ | ~ | Partial | Format/CLI coverage is broad; native dialogs and every original format need audit |
| Overworld | Edit Layer 1 tiles, levels, paths, events | P | P | P | P | ~ | Partial | Complete model and transfer oracle exist; interactive tool/variant matrix incomplete |
| Overworld | Edit Layer 2 tiles and event tiles | P | P | P | P | ~ | Partial | Transfer evidence exists; automate selection/move/restore gesture parity |
| Overworld | Edit paths and directional links | P | P | P | ~ | ~ | Partial | Standalone and ROM editors pass; compare one-way/reciprocal UI behavior |
| Overworld | Edit exit/warp links | P | P | P | ~ | ~ | Partial | Native workflow passes; broaden original dialog and modified-ROM fixtures |
| Overworld | Edit level settings, names, messages, player starts | P | P | P | P | ~ | Partial | Individual native workflows pass; broaden installed runtime versions |
| Overworld | Edit sprites and custom overworld sprite sidecars | P | P | P | ~ | ~ | Partial | Models/appearance editor exist; `.ovssc`/`.ovpath*` UI parity incomplete |
| Overworld | Edit event-number map, main/special reveals, tilemaps | P | P | P | P | ~ | Partial | Wine transfer fixture covers physical/semantic changes; broaden interactive cases |
| Overworld | Palette and ExAnimation editing | P | P | P | ~ | ~ | Partial | Models/native panels exist; original timers, triggers, and import dialogs incomplete |
| Overworld | Import/export complete overworld | P | P | ~ | P | ~ | Partial | Nine-domain transfer oracle passes; native transfer dialog parity incomplete |
| Title | Edit/import/export title-screen tilemap | P | P | P | P | ~ | Partial | Pristine/expanded transaction and Wine transfer evidence pass; broaden variants |
| Title | Record/import/export/play title movements | P | P | P | ~ | ~ | Partial | ZSNES/Snes9x/native formats pass; original emulator capture workflow incomplete |
| Credits | Edit/import/export credits tilemap | P | P | P | P | ~ | Partial | Legacy/expanded rows and Wine transfer pass; broaden runtime variants |
| Patches | Install expanded settings, Layer 3, palette runtimes | P | P | P | P | ~ | Partial | Three built-in families are recovered; original compatibility/migration families remain |
| Patches | Sprite 19 ASM fix and Lfix3 families | ~ | ~ | - | - | - | Missing | Named Ghidra installers exist; only partial Rust runtime coverage |
| Patches | Map16 staged hooks and compatibility upgrades | ~ | ~ | - | - | - | Missing | Named four-stage Ghidra family; full install/detect/migrate workflow absent |
| Configuration | Keyboard shortcuts | P | P | ~ | - | ~ | Partial | Portable configuration exists; no complete native customization dialog/oracle |
| Configuration | Main/user toolbar customization | P | P | ~ | - | ~ | Partial | Config model exists; original bitmap/icon/menu editor behavior incomplete |
| Configuration | Localization | P | P | ~ | - | ~ | Partial | Catalog model exists; original language discovery/switch behavior unverified |
| Configuration | External tools and event subscriptions | P | P | P | ~ | ~ | Partial | Safe argument-vector launcher passes; compare original menus, events, and registry migration |
| Help | Help topics, diagnostics, version/about | - | N/A | - | - | - | Missing | Original help launcher and diagnostic surfaces have no Rust parity ledger evidence |
| Release | Installer, portable packaging, updates, crash recovery | - | - | - | - | - | Missing | Required for mature drop-in replacement; not represented by format tests |

## Current critical path

The fastest route to a broadly usable editor is:

1. Finish the ordinary level-editing workflows and their native import/export dialogs.
2. Finish Map16 bitmap import and GFX/ExGFX native workflows.
3. Finish overworld transfer and custom-resource workflows.
4. Recover and implement the remaining runtime-patch families.
5. Broaden modified-ROM, SA-1, and ecosystem fixtures.
6. Complete configuration, help, packaging, and release gates.

Whenever a row changes, its evidence must name the relevant Rust tests and, where applicable, the
Wine fixture and Ghidra address. A passing aggregate test suite is necessary but cannot promote a
row whose workflow-specific evidence is absent.
