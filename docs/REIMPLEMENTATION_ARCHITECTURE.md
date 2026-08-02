# Lunar Magic clean-room Rust architecture

This is a behavioral reimplementation plan, not a translation of the Windows executable. The recovered Ghidra database and `REVERSE_ENGINEERING.md` establish observable behavior; `REIMPLEMENTATION_TEST_MATRIX.md` defines the compatibility gate.

## Workspace boundaries

| Crate | Responsibility | Must not depend on |
|---|---|---|
| `lm-rom` | Copier-header-transparent internal-header detection, explicit map-mode qualification, whole-image mapper conversion/addressability, checksums, bounded reads/writes, immutable input plus transactional edits | GUI, GPU |
| `lm-codec` | LZ2 and distinct length-bounded/terminated RLE forms, including terminator-safe packet generation, plus graphics planar/chunky conversion | ROM allocation, GUI |
| `lm-rats` | RATS validation, bank-aware allocation, protection-aware deduplication/replacement/erase, and relocation ledger | GUI |
| `lm-level` | Level headers, lossless object/sprite streams with checked legacy/expanded control framing, synchronized `.mw0`/`.mw0t` custom-object and grouped `.mw2`/`.mwt` custom-sprite libraries, exact `.m16` and block-canonical `.s16` raw Map16 sidecars, entrances, exits, Map16 workspace with exact public-page shape checks, atomic semantic editing, complete-set graph validation/interchange, native partial transfer, and complete semantic serialization | Platform APIs |
| `lm-graphics` | GFX/ExGFX containers, recovered generic SNES 1–8-bpp planar tiles (including odd final planes), ownership-aware atomic graphics/palette editing, binary-alpha transparency-safe palette-row bitmap import, flip-aware occupied-slot materialization, deterministic bounded Wu RGB-to-BGR555 quantization, animation records, and materialized animation frames | Window system |
| `lm-overworld` | Layer data, checked event-reveal persistence, validated/serialized navigation paths, warp endpoints, sprites, messages, and atomic stable-key editing/serialization for level names, player starts, and lossless submap settings | Window system |
| `lm-title` | Lossless title-screen movement recordings, bounded native interchange, exact minimal ZSNES V143 states, and plain/gzip tagged Snes9x RAM extraction | ROM mutation, window system |
| `lm-project` | Atomic multi-subsystem load/save transaction, including pre-allocation Map16 shape/graph and overworld event-source validation, dirty tracking, undo commands, sidecar preservation | Concrete GUI toolkit |
| `lm-profile` | Strict identity-bound external revision layouts and recovered lookup tables shared by CLI and application frontends | UI state, ROM mutation |
| `lm-render` | Pure level/Map16/overworld scene construction, event previews, bounded software-reference rendering with invariant-sealed fallible canvases, and deterministic RGBA PNG artifacts; optional GPU backend | ROM mutation |
| `lm-oracle` | Fixture manifests, original-program observations, decoded comparisons, changed-range reports | Production UI |
| `lm-app` | macOS/cross-platform UI, commands, dialogs, clipboard adapters, bounded portable recent-document persistence, snapshot-safe paired custom-object sidecar documents, versioned external-tool configuration and shell-free launch effects | Raw ROM offsets |
| `lm-native` | Cross-platform `eframe` presentation, native file dialogs, correlated frontend-effect execution, recoverable file persistence, and editor-surface composition over `lm-app`, including revision-bound fixed Lunar Magic metadata, global secondary-exit, title-recording, title/credits tilemap, player-start, overworld-settings, event-number, main/special-event reveals, event tilemaps, level-name, boss-sequence, variable overworld-message, path-link, and warp-link workspaces | ROM layouts, codecs, serializer internals |

## Core invariants

- Parsing is total over arbitrary bytes: malformed input returns structured errors and never panics or reads out of bounds.
- LZ2/LZ3 oracle observations compare decoded hashes and deterministic canonical re-encodings rather
  than physical command bytes, allowing semantically identical compressor choices to match while
  retaining strict termination, output bounds, and canonical-reopen evidence.
- ROM edits are transactions. Validation and allocation complete before bytes are committed; failure leaves the original image unchanged.
- Complete logical-ROM replacement retains the longest common prefix and records the remaining
  before/after tails as one reversible project edit. Targets must retain the qualified mapper and
  stable cartridge identity, end on a complete mapper-addressable bank, and leave an optional
  copier header byte-exact. Transactional IPS application uses this boundary, invalidates the
  now-stale revision audit profile after success, and treats an exact patch as a history-free no-op.
- Copier-header conversion is the complementary physical-file transaction. History stores an
  optional exact 512-byte before/after prefix separately from logical edits, compare-guards both
  directions, and counts header-only changes as project dirtiness. Logical bytes, mapper
  qualification, checksum evidence, and an installed revision profile remain unchanged.
- Identity-qualified projects synchronize cached stored/computed checksum evidence after ordinary
  mutations, grouped payload commits, and first-class project undo/redo. Stable cartridge identity
  fields remain fixed unless a ROM is explicitly reopened and redetected, while application
  snapshots never expose a checksum view older than their project revision.
- Mapper validity is a whole-image property: the logical ROM must be nonempty, end on a complete
  32-KiB bank, and have a representable final byte. Identity detection, expansion, prepared
  mutations, and native tagged saves share that predicate.
- Native level persistence can group the modeled object, sprite, palette, and ExAnimation payloads,
  an optional installed expanded-settings record, all pointer rewrites, ROM growth, and checksum
  repair into one history operation. This is the transaction boundary used by level import for its
  object/sprite subset.
- Revision profiles construct the corresponding aggregate load layout and a copy-on-write save
  plan whose four allocators share one complete metadata-protection policy. Frontends therefore do
  not independently assemble layouts or drift on which native tables must remain protected.
- Standalone native graphics, palette, ExAnimation, Map16 page/set, and complete-overworld saves
  expose the same checksum-inclusive transaction boundary. Headless imports and application
  controllers cannot publish a relocated payload without its matching internal-header checksum.
- Proven-ownership RATS relocation can stage allocation, every pointer rewrite, reuse-aware old
  block erasure, and checksum repair in one commit and one undo batch. Its `LMRATS01` reclaimable
  set must exactly equal the unique caller-supplied previous descriptors. The project API rejects
  pointer, direct-write, checksum, and complete internal-header/vector overlap before mutation; all
  application and CLI callers therefore inherit the same header-protection policy.
- Revision runtime installation uses a separate two-pass relocatable-patch transaction. It first
  validates exact hook preconditions and the complete allocation/protection policy on a staging
  image, allocates every tagged payload, then resolves cross-payload and hook operands as 24-bit
  SNES addresses. Payload bodies, identity-checked direct writes, optional mapper-valid growth, and
  checksum repair commit as one undoable history batch. A late fixup, mapping, allocation, hook, or
  checksum failure leaves the ROM, history, and project revision unchanged. This keeps generic
  relocation mechanics independent of revision-owned machine-code templates.
- `LMPAT001` is the bounded canonical adapter above that transaction. It binds stable game, region,
  revision, and mapper identity while retaining address-independent payload bodies, hook
  preconditions/replacements, and fixup records. `lm-profile` combines it with the audited
  profile-wide allocation policy (including mapper-valid staged growth); the CLI owns bounded
  reads, create-new output, and reopened-tag verification. Templates cannot weaken metadata
  protection or silently select another revision.
- The application owns a typed `InstallRevisionPatch` command rather than invoking the CLI. A
  bounded `LMRPINS1` shell specification resolves the template relative to itself, then submits it
  with the current project revision. The state machine rebinds the plan to its active profile and
  ROM identity, blocks overlapping saves, advances revision exactly once, and retains installation
  as one normal undo/redo/save history entry.
- The native revision-patch installer uses the shared nonblocking bounded document loader for one
  canonical template, captures the audited profile and project revision before the read, and
  rejects a foreign identity before creating editable state. Its focused workspace owns only
  search-range/fill text and the immutable template; plan construction, profile-wide protection,
  allocation, fixups, and checksum repair remain behind `InstallRevisionPatch`. Rejection retains
  the workspace for correction, while accepted dispatch is the sole close acknowledgement.
- The application keeps ordinary editor dispatch separate from the focused ownership-backed shell
  boundary. That module alone decodes `LMRATS01` evidence and routes level, aggregate level-assets,
  Map16, graphics, palette, ExAnimation, and complete-overworld scripts to snapshot-bound
  reclamation commits; individual frontends cannot recreate or weaken the ownership protocol.
- The graphical native frontend mirrors that boundary with one shared, bounded `LMRATS01` loader
  and explicit reclaiming actions for aggregate level assets, Map16, palettes, graphics,
  ExAnimation, and overworld data. Each action calls the corresponding application controller's
  ownership-aware preparation API; UI modules do not erase blocks, rewrite pointers, or interpret
  ownership evidence themselves.
- A standalone graphical reclamation operation uses that same loader and adds no ownership
  inference. Its revision-bound workspace previews exact reclaim/retain counts and reclaimed bytes,
  accepts only a typed fill byte, and submits `ReclaimOwnedRats`. The application re-plans against
  current bytes, treats an empty plan as a revision-preserving no-op, and commits erasure plus
  checksum repair as one history entry.
- Native ROM editors retain staged controller state until the application confirms command
  acceptance. Preparation alone never closes a window: stale-revision or transaction rejection is
  surfaced by the shell while the exact staged model remains retryable, and successful dispatch is
  the sole acknowledgement that clears the editor workspace. Revision-sensitive ROM expansion and
  whole-table graphics migration use the same acknowledgement boundary and retain their dialog
  inputs when dispatch fails.
- The native IPS dialog performs only bounded asynchronous patch loading and immutable preview
  (logical sizes plus changed/added/removed bytes). The application state owns patch decoding,
  identity requalification, revision and save-conflict checks, history publication, and exact
  undo/redo; malformed, stale, identity-changing, or partially banked results never mutate state.
- The native copier-header dialog snapshots the current physical state, defaults to its inverse,
  and owns only target/fill presentation. It also exposes the identity-bound Lunar Magic 3.63
  SMW-US canonical header as a typed action rather than approximating its structured bytes with a
  fill value. Application revision, identity, and pending-save checks precede the project-layer
  exact-prefix edit; cancellation, no-op targets, stale workspaces, and rejected dispatch retain
  document bytes and history.
- Unknown tagged blocks and unknown bits survive load/save unless an operation explicitly owns them.
- Models store semantic values and retain the original encoded representation when round-trip preservation requires it.
- Mapper conversion is centralized in `lm-rom`; feature crates never duplicate LoROM, ExLoROM, SA-1, or copier-header arithmetic.
- Rendering consumes immutable model snapshots. UI selection, hover, and drag state do not mutate serialized data until a command commits.
- Existing world-space canvases pass through a focused bounded viewport rasterizer with exact
  rational nearest-neighbor sampling and transparent signed-edge clipping. `lm-app` adapts its
  recovered 100–5000% `LevelViewport` state to this renderer without duplicating camera math.
- Screen-space editor decoration is a separate painter-ordered renderer module. It validates every
  overlay before mutation, clips signed grid and half-open selection geometry, animates deterministic
  dash phases, and alpha-composites after viewport sampling through `render_editor_preview`.
  Its canonical `LMOVLY01` interchange preserves ordered records and is bounded to 256 overlays;
  all application preview specifications resolve the optional artifact relative to themselves and
  share one bounded decoder/compositor rather than owning domain-specific decoration formats.
  A focused CLI module normalizes and observes this format as one create-new batch; command parsing
  is also isolated so the general portable-file parser remains below its complexity threshold.
- Complete portable Map16-page, level, and overworld model-to-raster workflows are public focused
  `lm-render` modules. CLI commands own only bounded decoding, alias policy, create-new publication,
  and reporting; native frontends cannot accidentally fork reveal, animation, or asset-validation
  semantics.
- Portable graphics tile sheets and palette swatch grids use the same shared renderer boundary and
  have standalone `render-graphics` and `render-palette` CLI workflows. Their argument parsing,
  render dispatch, format decoding, and create-new publication remain separate focused modules.
- Native complete `.smwpal` interchange is isolated from the portable `LMPAL1` model. A focused
  `lm-graphics` module accepts only the two recovered exact layouts: the legacy backend's `0x7e2`
  SNES-color bytes, or the expanded backend's `0x800`-byte main palette followed by its separately
  owned `0x10`-byte auxiliary region. The CLI losslessly normalizes either form and publishes a
  backend-, color-, and auxiliary-addressable observation as one create-new batch.
- ROM-backed palette mutation additionally requires a bounded `LMPALOWN` ownership artifact. Its
  fixed-width records distinguish editable, fixed, and ExAnimation-owned colors (including the
  owning record number), reject unknown/noncanonical encodings, and must exactly match the decoded
  palette shape before a controller is created. Presentation code may copy protected colors but
  disables their mutation controls; allocation authority remains a separate explicit range.
- ROM-backed graphics mutation follows the same boundary through `LMGFXOWN`: one exact record per
  decoded tile identifies editable, fixed, generic ExAnimation, original-animation, level
  ExAnimation, or global ExAnimation ownership. Canonical version 2 retains the three native slot
  classes and bounded slot number; version 1 remains decode-compatible. The ownership-validating
  controller decoder is mandatory for commit-capable frontends; the all-editable convenience
  decoder remains display-only. Protected tiles can be inspected and copied but not pasted or
  painted, and graphics allocation authority is still supplied separately.
  Focused CLI argument and execution modules normalize either ownership format and publish
  per-entry semantic observations through one create-new batch; input aliases, output collisions,
  and partial publication are rejected by the same atomic-output boundary as other artifacts.
- Version-2 TPL is a second focused native boundary: exact `TPL 02` framing followed by precisely
  256 little-endian BGR555 words. Its decoder does not conflate version 0's RGB triplets with native
  words, and its own CLI normalization/observation path uses the same grouped publication rules.
- Raw native palette interchange is modeled as exactly 257 BGR555 words, paired when requested
  with a lossless 257-byte `.palmask` mask where zero retains the destination and any nonzero byte
  selects the source. Masked application validates all three shapes before cloning, applies selected
  colors, then clears selected indices `0, 16, …, 240` exactly like the recovered loader. Palette
  argument parsing is isolated in a focused CLI module rather than growing the generic asset parser.
- RGB `.pal` is retained as exactly 256 ordered RGB24 triplets. Its model reproduces the recovered
  evidence vote between channels stored as `xxxxx000` and channels whose low three bits replicate
  the high five-bit value, preserves arbitrary source bytes on normalization, and reports both RGB
  and converted BGR555 values. Palette dispatch is also separated from the generic asset dispatcher.
- The runnable application reaches portable previews through focused Map16, overworld, graphics,
  palette, and ExAnimation specification modules. They share only a bounded key/value grammar;
  each domain owns its wire magic, typed fields, and tests. Specification paths are relative to
  their files, preserve Unicode/spaces, and publish only new PNGs; no render routing or file
  grammar is embedded in the main application dispatcher.
- Complete-level, overworld, Map16 set/page, graphics, and palette preview specifications share one
  focused optional-camera parser.
  It consumes an all-or-none signed-origin, nonzero-dimension, exact-zoom field group and feeds the
  generic editor viewport adapter; standalone and current-document rendering therefore cannot
  drift in pan, clipping, or recovered 100–5000% zoom validation.
- The executable entry point owns only startup and top-level typed routing. ROM open/close/profile
  lifecycle and bounded reads, frontend action/save policy, and external-tool execution live in
  separate application modules, keeping platform adapters and persistence policy independently
  testable instead of accumulating them in one source file.
- The graphical executable is a separate leaf crate. Its application view, native dialogs,
  frontend-effect interpreter, and editor content are separate modules. Open/save/close/quit and
  history/navigation always pass through `lm-app::AppState`; the toolkit cannot mutate a ROM or
  acknowledge an uncorrelated persistence request directly. Profile installation also crosses the
  audited application command boundary. Native Graphics, Palette, and Map16 views decode immutable
  profile-bound controller snapshots and adapt public `lm-render` canvases to toolkit textures;
  they do not read profile offsets or implement domain-specific rasterization in the GUI crate.
  Independent `LMPAL1` files have a focused native palette-editor module that routes edits,
  undo/redo, and persistence through `PaletteDocumentController`, and participates in application
  quit protection. This portable document is fully editable because it owns all contained colors;
  a ROM-backed view remains read-only unless exact fixed/animation ownership and an explicit
  allocation search extent are supplied rather than inferred by presentation code.
  The runnable shell's separate `graphics-edit-owned`, `palette-edit-owned`, and
  `exanimation-edit-owned` paths additionally bind an `LMRATS01` allocation claim to the exact
  tagged descriptor captured in the corresponding controller snapshot, then use the project-layer
  one-batch relocation transaction. Tile/color/record semantics and allocation ownership stay
  distinct types, and neither is inferred from payload contents or pointer reachability.
  Portable `LMGFX4BP` files likewise have a native graphics-editor module backed by
  `GraphicsDocumentController`; an independently decoded `LMPAL1` supplies display colors without
  becoming part of graphics persistence. Tile selection, 4bpp pixel replacement, controller
  history, and immutable saves are separate from the painter/hit-testing module, and both graphics
  and palette documents are checked in sequence before application shutdown may proceed.
  Standalone `LM16PAGE` editing follows the same boundary: `Map16PageDocumentController` owns
  changes and persistence, while independently decoded graphics and palette files are read-only
  rendering dependencies. Window lifecycle, shared-renderer texture adaptation and tile hit
  testing, and exact packed-subtile form conversion are distinct modules. Map16 joins palette and
  graphics in the sequential dirty-document shutdown gate.
  Compact `LMEXAN1` documents likewise have separate native lifecycle and semantic-form modules.
  The frontend requires the exact revision size-mode bytes and maximum-record bound before decode,
  then routes global fields, triggers, records, and ordinary frame words through
  `ExAnimationDocumentController`. Special transfer kinds without ordinary frames remain visible
  and read-only at that form boundary. ExAnimation participates in the same sequential shutdown
  gate and immutable recoverable persistence protocol.
  `LMLEVEL2` has a native editor split across lifecycle, renderer/hit-testing, semantic forms, and
  property-panel modules. It consumes explicit Map16, graphics, palette, and exact layer-dimension
  inputs, and routes tilemap, header, object, sprite, and entrance mutations through the complete
  level document owner. Separate auxiliary and advanced panels cover screen and secondary exits,
  keyed local Map16 overrides, the optional expanded header, and every losslessly modeled Layer 3
  setting/buffer while retaining opaque values. That owner exposes one mixed-domain edit enum spanning properties,
  both object streams, sprites, Layer 3, and auxiliary collections; the whole staged level must
  validate, encode, and reopen canonically before one history revision is committed. The level
  document joins all other standalone editors in sequential dirty-shutdown protection.
  `LMOWFULL` has a similarly modular native editor: lifecycle/tile orchestration, renderer and hit
  testing, fixed-record forms, record panels, palette editing, and compact animation editing are
  separate modules. Exact ExAnimation size modes and maximum records remain explicit; Map16 and
  graphics are read-only render dependencies, while the embedded palette is explicitly owned by
  the portable aggregate. Layer tiles, reveal pairs, endpoints, message cells, lossless sprites,
  palette colors, and animation state all cross the existing nine-domain atomic overworld
  controller. The shared 256-byte mode-table reader prevents standalone and aggregate animation
  editors from drifting on revision interpretation.
- Provider-resolved `LMENTAPP` level entities and `LMOWAPP1` overworld sprite definitions have
  separate focused normalize-and-observe commands. Their observations retain painter/part order,
  stable source or sprite identities, signed placement, palettes, tiles, and flips; renderer input
  evidence therefore remains auditable without embedding provider logic in the renderer.
  `LMENTAPP` additionally has a painter-order application controller and bounded `LMENTED1`
  lifecycle for atomic sequence edits. `LMOWAPP1` has a separate controller and bounded
  `LMOWAED1` lifecycle whose definition operations use stable sprite IDs and whose nested part
  operations preserve painter order. Both provide revision checks, canonical reopen, immutable
  saves, and dirty shutdown without conflating the two identity models.
  Both controllers additionally own bounded whole-file undo/redo so navigation restores stable
  identity, painter order, and every placement field together.
- Portable-document lifecycle commands are typed independently of the terminal grammar. The
  shell parser translates text into those commands, while native graphical frontends can route
  open/edit/render/status/save/close/discard operations without depending on CLI parsing details.
- Level selection owns a focused, bounded back/forward history independent of ROM undo history.
  Each entry retains the signed world origin and exact rational zoom, constrained to the recovered
  inclusive 100–5000% editor range, while transient window dimensions remain frontend-owned.
  Complete-level preview specifications can carry that camera state as an all-or-none field group,
  connecting unsaved document revisions to bounded viewport PNGs through the same public adapter.
  Direct selection clears the forward branch, project
  replacement resets the trail, and history navigation emits typed view, viewport, and tool effects.
- Lossless MWL containers have their own revisioned application controller and focused shell
  adapter. Atomic edit batches own only flags, attribution, the recovered level number, or an
  explicitly named opaque section; canonical reopen, immutable save snapshots, stale-token
  rejection, dirty shutdown participation, and bounded whole-container undo/redo apply without
  assigning meanings to unknown bytes. A separate typed optional-assets operation decodes palette
  and compact ExAnimation data from a source container under an exact size-mode interpretation,
  then commits both sections as one canonical target revision while preserving all unrelated
  sections. Its bounded specification parser and file I/O remain in the shell adapter. The native
  MWL window reaches that same controller operation through a separate background-load module that
  binds the source and exact mode table; toolkit code owns only chooser state, limits, and error
  presentation. Its sibling semantic-panel module edits a decoded aggregate clone and submits it
  as shared `MwlOptionalAssetsEdit` commands. The bounded headless edit specification emits the
  same commands. The controller applies complete ordered batches to a staged aggregate, revalidates
  the declared animation-record limit, encodes both sections, and canonically reopens the complete
  MWL before advancing history.
- Typed MWL optional-asset commands and the bounded `LMMWLOE1` grammar live in `lm-project`, not in
  either frontend. The create-new CLI, revisioned application shell, and native semantic panel
  therefore share one parser and mutation engine. The CLI owns only bounded file reads,
  interpretation inputs, canonical reopen comparison, and atomic publication. Relocation-neutral
  oracle observations compose the same typed aggregate with the exact revision size-mode table,
  exposing each ordinary ExAnimation frame and source word while keeping allocator/source-pointer
  provenance out of semantic equality. Record-targeted frame commands resolve that same table in
  the domain engine and atomically insert, replace, remove, or reorder frames; neither the terminal
  grammar nor the native panel decides single-versus-double width independently.
- The exact 32-byte expanded-level settings record has a typed Layer 3 projection over word 0's
  verified `$2000` enable bit and word 1's packed file/length/destination descriptor. It deliberately
  retains selector aliases, unrelated flag bits, and all fourteen opaque words. Standalone and MWL
  CLI adapters, oracle observations, the native semantic form, and a revisioned MWL application
  transaction share that projection. The `LMMWLL31` shell adapter owns only bounded specification
  parsing; canonical reopen, undo/redo, save snapshots, and stale-revision rejection remain in the
  document controller. Installing the revision-specific `$4c0` main patch, `$6e00`
  expanded-settings runtime/table, and separately allocated tilemap payload remains outside this
  portable record-editing boundary.
- The native built-in-runtime installer is a focused revision-bound workspace over the two
  application commands for recovered SMW-US-v1 installation. It selects either expanded settings
  alone or the complete Layer 3 group, performs no allocation or serialization in the frontend,
  refuses stale revisions, and remains open after command rejection. Application acknowledgement
  is the only state that closes it, matching the transactional project-operation dialogs.
- Complete `LM16SET1` files also have a standalone application document controller, separate from
  the profile-bound native-ROM Map16 controller. It owns revision checks, atomic graph-validated
  edit batches, canonical reopen, immutable save snapshots, dirty shutdown participation, and
  current-revision `LMM16DR1` page previews without coupling those policies to a UI toolkit. The
  shared bounded canonical-value history supplies monotonic undo/redo across complete graph-valid
  workspaces.
- Individual `LM16PAGE` artifacts have a focused headless normalize-and-observe workflow. Its
  observation includes the source-page identity as well as all 256 graphics and Acts Like entries,
  and grouped create-new publication cannot leave a normalization without its requested evidence.
  A page-scoped application controller and `LMPGEDT1` shell workflow provide atomic tile, subtile,
  and Acts Like editing with canonical reopen, immutable saves, and bounded revision-safe
  undo/redo. It deliberately preserves arbitrary 16-bit external Acts Like values without graph
  claims; only a complete `LM16SET1` owns cross-page graph
  validation. `LMPGDR1` previews the controller's current revision through the shared renderer with
  bounded spec-relative assets and create-new publication.
- Native `LMLVL1` level-transfer files have a separate interpretation-bound workflow requiring
  either the exact four-table sprite-length artifact or an explicit `standard` choice. Canonical
  observations retain source slot, raw object records, recovered object command IDs and parameter
  bytes, orientation-neutral coordinate nibbles, screen-advance flags, expanded screen/control
  tokens, and exact sprite record bytes instead of conflating native transfers with complete
  portable bundles. Typed object mutation changes only proven command, parameter, or placement
  bits and rejects record-shape or terminator collisions; absolute placement remains stream- and
  orientation-contextual. Command-zero parameter-1/3 records are typed as the two editor-maintained
  screen-jump encodings and retain an exact packed target.
  A revisioned application document binds the same interpretation through `LMNLDOC1`, shares one
  staged edit engine with the ROM-backed controller, and owns canonical reopen, immutable saves,
  stale-token rejection, dirty shutdown, and the full typed shell lifecycle. Its bounded canonical
  undo/redo retains the exact sprite-length interpretation across every restored revision.
- Aggregate `LMNATAS1` files compose canonical `LMLVL1`, `LMPAL1`, and `LMEXAN1` sections with the
  optional exact expanded-settings record. Profile-driven CLI export/import uses the aggregate
  loader and single grouped save transaction, validates nested source identities and profile
  shapes, and requires a complete semantic reopen before publishing a new ROM.
- `LMNATAS1` normalization and oracle observation are interpretation-bound to the same profile.
  The observer composes the existing field-complete domain observers under stable aggregate paths,
  so differential fixtures report the exact nested field that changed rather than only a payload
  hash; normalization and requested evidence publish atomically.
- A revisioned toolkit-neutral `LMNATAS1` document controller binds sprite-length and animation
  interpretation for its lifetime. It shares the aggregate staged edit engine with the ROM-backed
  controller, requires canonical semantic reopen before advancing a revision, and correlates
  immutable asynchronous save snapshots without clearing newer dirty state.
  The runnable frontend opens it through a separate bounded `LMNADOC1` profile-binding
  specification, consumes the shared `LMNATED1` composition loader, and routes status,
  save, close, discard, EOF, and quit through the common portable-document session owner.
  Its palette preview reuses `LMPALDR1`, the shared palette renderer, viewport/overlay adapter, and
  deterministic create-new PNG publication against the controller's current unsaved revision.
  A reusable bounded portable-value history stores canonical aggregate states. Undo and redo are
  stale-revision protected and monotonic, preserve asynchronous save baselines, and discard the
  redo branch after divergent edits without allowing unbounded editor-session memory growth.
- Runnable aggregate editing uses a separate bounded `LMNATED1` composition specification. It
  resolves the existing domain scripts relative to the specification, parses them all before
  mutation, and drives the profile-qualified aggregate controller so mixed edits remain one
  checksum-inclusive application revision and undo operation.
- Complete `LMOWFULL` documents similarly retain their exact size-mode table and animation bound
  for their entire lifetime. Native-ROM and portable-document controllers call one staged
  nine-domain edit engine, while `LMOWDOC1` and `LMOWDRN1` keep open interpretation and preview
  assets explicit. Source-slot, palette-ownership, canonical-reopen, immutable-save, and dirty
  shutdown checks therefore cannot diverge between frontend routes. Portable overworld history
  reuses the bounded canonical-value stack for monotonic undo/redo across the complete aggregate.
- A focused `overworld-file` CLI workflow decodes that same interpretation-bound artifact and
  atomically publishes its canonical nine-domain encoding with a complete semantic observation.
  The observer records source slot, declared shape, both tile planes, reveal pairs, endpoints,
  messages, sprites, exact palette words, and compact animation fields.
- Undo records are semantic commands or bounded snapshots with deterministic serialization, never pointers into UI objects.
- Explicit `LMRATS01` allocation-authority artifacts can be canonically normalized and observed as
  one atomic group. Evidence paths are keyed by exact header offsets and retain payload bounds,
  lengths, and retain/reclaim disposition without granting the observer mutation authority.
- Undo/redo stack transitions occur only after the complete ROM edit batch succeeds. Configurable
  operation-count limits trim oldest undo entries and a zero limit disables retention.
- Clipboard transfers use the versioned `application/x-lm-editor-clipboard` payload. The shell
  validates editor-domain compatibility; paste and cut effects are committed by editor controllers
  through the same transactional project APIs as direct edits.
- ExAnimation frame clipboard operations retain explicit one/two-word widths. Controller-owned
  copy/cut/paste planning canonicalizes selections, preserves transfer order, and stages the whole
  frame batch before replacing a lossless record. Compact persistence rejects disabled-trigger
  values and fixed-workspace bytes with no compact representation; shrinking a frame list clears
  only its stale former payload while retaining unrelated fixed-record bytes.
- Portable `LMEXAN1` documents retain the exact 256-entry size-mode table and record bound supplied
  by `LMEXDOC1`. The same staged compact edit engine serves native and document controllers;
  revision checks, canonical reopen, immutable saves, and dirty shutdown remain toolkit-neutral.
  The shared bounded canonical history adds monotonic revision-safe undo/redo without losing the
  retained interpretation tables.
  Rendering stays behind provider-resolved `LMANFRM` rather than guessing transfer execution;
  canonical normalization and target-addressable tick/tile/palette observations make that provider
  boundary independently auditable.
- The headless `exanimation-file` workflow likewise requires the exact 256-byte mode table and
  maximum record count before decoding. It atomically publishes canonical compact encoding and a
  semantic observation, while one shared bounded mode-table reader serves file inspection, frame
  editing, and native transfer commands.
- External-tool templates expand into an executable, an argument vector, and an optional working
  directory without shell parsing. The core never launches processes; frontends own permissions,
  lifecycle, and cleanup.
- All revision-specific pointer tables, the installed direct expanded-settings table, stream limits, graphics compression policy, sprite-length data, and ExAnimation mode
  data enter the application through the strict `LMREVPRO1` revision-profile format. The app has
  no guessed address defaults; audited profiles are external compatibility inputs. Controller
  factories require the profile's game, region, revision, and mapper to match the detected ROM
  identity before any profile-provided address is read. Installing or clearing a profile advances
  the shared application revision and atomically invalidates background controller work without
  changing ROM bytes, dirty state, or undo history.
- An optional profile-declared expanded-settings table has its own native application controller.
  It binds the selected level and exact 32-byte record to an immutable revision snapshot, changes
  only indexed lossless words, and prepares checksum-inclusive undoable mutations without raw
  offsets entering the frontend command layer.
- Exported exact expanded-settings records have a separate portable document controller. Atomic
  duplicate-free word batches enter a 100-state revision-safe history, while immutable saves and
  the dirty baseline remain independent of undo/redo navigation.
- Complete graphics compression migration is a focused project transaction rather than a frontend
  loop. It decodes the entire source table, stages every tagged allocation and pointer, repairs the
  checksum, semantically reopens all target-codec slots, and only then applies one undoable ROM
  mutation. The CLI exposes that boundary through copy-on-write `graphics-recompress`; the
  application shell exposes the same transaction as a typed revision-checked command and derives
  its source layout and allocation protection from the installed qualified profile. Application
  history records the source and target codecs on the migration entry and synchronizes the
  profile's effective in-memory codec only for that typed entry, including across interleaved
  ordinary edits, profile lifecycle changes, and undo/redo, while leaving the external profile
  artifact immutable.
- External profiles have one bounded reader shared by frontends. Total bytes, lines, line bytes,
  and profile-name bytes are capped before field storage; invalid UTF-8 and over-limit inputs never
  reach ROM identity or pointer-table processing.
- Profile qualification is non-mutating and exhaustive over the 16 declared pointer tables plus
  the direct expanded-settings table when declared. Every
  entry must be readable, mapper-valid, and point inside the logical ROM; reports summarize unique
  targets and ranges without dereferencing or claiming ownership of the target payload. Application
  profile installation must pass this qualification atomically before editor capabilities expose
  the profile.
  The typed report distinguishes pointer-target summaries from direct-table span evidence and
  retains an explicit absent capability for legacy profiles.
- Headless native exports consume the same complete profile for levels, Map16, graphics, palettes,
  ExAnimation, expanded settings, and overworld data. Raw-offset CLI forms remain diagnostic compatibility paths, not
  a second source of revision metadata.
- Profile-driven imports cover the same seven domains, derive checksum location from detected ROM
  identity, protect the full declared span of every pointer table including non-default strides,
  and require semantic reopen equality before create-new publication.
- Pointer-table spans must be pairwise disjoint and cannot overlap the direct expanded-settings
  span. Allocation for any profile-driven import protects every metadata table across every domain
  plus the complete 64-byte internal header/vector block, rather
  than only the tables being edited. Profile qualification also rejects tables overlapping that
  block and payload pointers targeting any declared table or the internal header.
- The same protection is exposed by `RevisionProfile::allocation_policy` for interactive
  controllers. Callers must supply an explicit nonempty mapper-addressable range inside the active
  image; no application frontend treats zero or `0xff` bytes as ownership evidence by themselves.
- Prepared ROM mutations validate the declared mapper against the complete resulting logical image
  even when they do not grow it and the raw project has no detected identity. Mapper metadata can
  therefore never authorize a write into an image extent it cannot map.
- Explicit ROM expansion stages bank-aligned growth and checksum repair on a private image and
  applies one reversible logical-tail mutation. The copy-on-write CLI additionally enforces detected
  mapper agreement, bounded input/target sizes, exact copier-header preservation, semantic reopen,
  and create-new publication. A typed application command derives mapper/checksum policy from its
  immutable controller snapshot and routes the same operation through revision, history, dirty, and
  save-lifecycle state; neither frontend invents unrecovered Lunar Magic metadata or runtime hooks.
- Filesystem frontends stage and sync document snapshots beside their destination. New documents
  use no-replace publication; existing-document saves preserve permissions and recover the prior
  file when platform rename semantics require a backup. I/O failure releases pending shell state.
- The graphical native frontend moves only immutable ROM bytes, the exact request ID, and the
  destination into a focused persistence worker. Recoverable replacement or create-new I/O occurs
  off the render thread; completion returns to `AppState` on the UI thread before the saved
  baseline can advance. Collisions, I/O errors, worker disconnects, and mismatched acknowledgements
  therefore cannot silently mark a newer revision clean.
- The same single-file worker primitive is embedded independently by portable palette, graphics,
  and Map16-page editors. Each controller keeps ownership of its saved baseline and request token;
  the worker owns no semantic model, and close/save controls prevent a document from disappearing
  while its immutable snapshot is being published.
- Paired `.mw0`/`.mw0t` saves stage and sync both snapshots before moving either destination, keep
  both originals as backups through publication, reject canonical-path and platform-file-identity
  aliases (including hard links), symlinks, and non-files, and restore the pair on failure. The
  controller acknowledges only the exact immutable snapshot that was written.
- Native frontends consume complete typed `LMLOC001` Unicode catalogs and structurally validated
  `LMTBAR01` toolbar layouts. These remain separate focused modules and are installed atomically in
  application state without changing the project revision or ROM history.
- Portable `LMSHORT1` bindings reuse toolbar action identifiers while representing logical Unicode,
  function, editing, and navigation keys with primary/secondary/shift/alt modifiers. Native
  frontends normalize platform events once, then resolve them through immutable application state.
- Standalone `LMLAY3V1` files have a canonical semantic oracle observation covering recovered
  selectors, graphics IDs, retained settings bytes, tilemap digest, and literal remap bytes. The
  CLI can publish normalization and observation together through one all-or-nothing output batch.
- Provider-resolved `LML3FRM1` planes have an equally canonical observation of their source digest,
  painter placement, ordered tile indexes, palettes, signed coordinates, and flips. The focused
  CLI workflow normalizes and publishes this evidence atomically without interpreting remap logic.
- `LMUICFG1` aggregates the canonical localization, toolbar, and shortcut encodings with independent
  section bounds. Installation validates the entire aggregate before one state swap, preventing a
  corrupt nested section from leaving a frontend with a mixed configuration generation.
- The runnable shell installs `LMUICFG1` through the same bounded aggregate decoder, parses
  platform-neutral logical gestures in a focused module, and routes named or shortcut actions
  through `AppState`'s shared availability function. Chooser and clipboard work remain typed
  frontend requests rather than shell-specific lifecycle shortcuts.
- Menu, toolbar, and shortcut invocation use one application-owned availability function. Enabled
  actions become either exact parameterless commands or typed copy/cut/paste requests, so document,
  history, selection, and pending-I/O rules do not diverge between native frontends.
- Frontend configuration and capability calculation are isolated from generic command dispatch.
- Native `ExAnimation` slot options use a focused application controller instead of enlarging the
  compact-record controller. The controller stages the seven shared option bytes, preserves their
  opaque low nibbles, prepares a checksum-inclusive RATS relocation from an immutable snapshot,
  and crosses the same revision-checked mutation boundary as other ROM editors.
  Recent documents, localization, toolbar layout, shortcuts, action resolution, and enabled-state
  queries therefore form one toolkit-independent state boundary, while ROM lifecycle and mutation
  remain in the project command state.
- Transactional application editing is a separate project-state boundary. It owns immutable
  controller snapshots, revision-token checks and overflow, semantic no-op detection, atomic ROM
  write/growth commits, and undo/redo transitions so individual editor controllers cannot diverge
  on concurrency or history behavior.
- Revision-profile application state is isolated from command routing. Validation, ROM identity
  matching, exhaustive table audit, no-op replacement, installation/clearing, selection
  invalidation, and revision advancement succeed as one operation or preserve the active profile.
- Navigation and selection compatibility are independent application policies. Navigation owns
  editor status, selection reset, view effects, and real level-change tool events; selection policy
  owns the exact editor/clipboard-kind matrix and selected-record cardinality checks.
- Runnable-shell startup parsing consumes native OS strings rather than forcing paths through UTF-8.
  It accepts one legacy positional ROM or explicit `--rom`, an optional audited revision profile,
  aggregate UI and external-tool configurations, an explicit recent-state store, help, and `--`
  path disambiguation, while rejecting duplicate, unknown, missing, or extra arguments before
  application state changes. Profile preload occurs only after ROM identity is established and uses
  the same bounded parse, qualification audit, and revisioned command as interactive installation.
- `--recent-state` connects the portable `LMRECNT1` model to a real session lifecycle. Startup
  accepts a missing destination or strictly decodes one bounded regular file; successful named
  opens and Save As acknowledgements trigger no-op-aware, same-directory staged create/replace
  publication, while malformed, oversized, symlink, and non-file stores are rejected.
- `--script` supplies a bounded regular UTF-8 command file to the same line parser and dispatcher as
  interactive input. File bytes, line count, and per-line bytes are capped before dispatch;
  confirmations consume ordinary subsequent lines, command errors produce process failure, and EOF
  cannot silently discard a dirty project. A process-level test exercises the built executable.
- The runnable terminal drives the native level controller through a focused bounded `LMLEDIT1`
  parser rather than duplicating edit semantics. Before producing a prepared mutation, the
  controller encodes and reparses both object and sprite streams under exact revision metadata and
  requires structural equality; raw records that satisfy generic bounds but disagree with native
  lengths never enter application state. Object commands include exact raw replacement and
  bit-preserving typed command-ID/parameter/coordinate/screen-advance changes, with
  shape-changing or terminator-colliding typed edits rejected. Existing screen-jump controls also
  accept exact packed-target edits without changing their recovered encoding variant.
- A separate bounded `LMM16ED1` parser drives the complete Map16 controller without coupling its
  grammar to level records. Tile, quadrant, and Acts Like changes all pass through whole-workspace
  graph validation and the same profile-wide allocation protection before one prepared commit.
- Palette scripts remain a distinct `LMPALED1` grammar because exact per-color ownership is part of
  their input contract. The terminal parses level, Map16, and palette artifacts behind one typed
  editor-script shell command, keeping the main dispatcher stable while domain validation remains
  in focused modules and controllers.
- Palette files use the same native/portable split as graphics: one staged ownership-aware edit
  engine, a revisioned canonical `LMPAL1` document, and a bounded `LMPALDR1` swatch-grid renderer.
  The document participates in immutable-save and dirty-shutdown policy without exposing ROM
  allocation details to portable frontends, and shares the bounded canonical history.
- Graphics extends that typed route with its own `LMGFXED1` parser and exact tile-ownership shape.
  Row-major 4bpp pixels remain a graphics-domain concern; compression, allocation, checksum repair,
  and revision checking stay behind the existing controller/project boundaries.
- Native and portable graphics controllers call one staged ownership-aware edit engine. The
  standalone `LMGFX4BP` document adds exact revision/save tokens, canonical reopen, dirty shutdown
  participation, and `LMGFXDR1` tile-sheet previews through the public bounded renderer, leaving
  the shell adapter responsible only for bounded files and create-new publication. Graphics undo
  and redo use the same canonical history primitive as other primary portable editors.
- Focused `graphics-file` and `palette-file` CLI modules decode their versioned portable artifacts,
  report source slots and shapes, and atomically publish canonical encodings with semantic oracle
  observations. They reuse the same observers as native exports rather than defining a second
  comparison vocabulary.
- Paired custom-object sidecars remain an independent application document, reached through a
  focused bounded `LMCUSED1` parser. The runnable shell derives `.mw0t` from `.mw0`, applies edits
  against the controller revision, refuses dirty close, and acknowledges a save only after the
  recoverable two-file persistence boundary publishes both snapshots. Its bounded canonical
  history restores records, descriptions, and text framing together without moving the saved
  baseline.
- The headless paired-sidecar workflow can publish normalized `.mw0`, normalized `.mw0t`, and a
  canonical record-addressable observation in one create-new batch. Observations include retained
  BOM/newline framing, exact object bytes, and Unicode descriptions.
- The separate `.mw2`/`.mwt` boundary retains a binary stream header, one-or-more-record placement
  groups, native bit-zero group markers, revision-table-selected record widths, optional text BOM,
  line-ending style, and trailing-line framing. Its headless workflow requires the exact four-table
  length artifact and publishes both normalized sidecars and semantic observation atomically.
- Its application controller keeps the length table immutable for the document lifetime and
  revalidates every programmatic record width at both batch commit and snapshot creation.
  `LMSPDOC1` binds data and interpretation paths; bounded `LMSPRED1` edits, exact save tokens,
  recoverable paired replacement, dirty-close/EOF handling, and the portable session registry give
  custom sprites the same toolkit-neutral lifecycle as custom objects. Bounded canonical undo/redo
  restores both sidecars while retaining that immutable record-width interpretation.
- Native `.m16`/`.s16` data has its own typed application document rather than being folded into
  portable `LM16SET1`. `LMN16DC1` fixes the interpretation before reading; `LMN16ED1` batches raw
  dword changes against a revision; immutable save tokens emit exact `.m16` or canonical `.s16`;
  and the shared portable-session registry enforces dirty close, quit, and EOF behavior.
- Portable overworld metadata uses a separate revisioned application document controller. A
  bounded `LMOMEDT1` parser edits names, starts, and submap settings by stable key while preserving
  raw flags and unknown bytes; dirty-close and exact snapshot acknowledgement follow the same
  frontend-owned persistence discipline without claiming an unverified native ROM layout.
- Portable overworld paths now have a failure-atomic stable-key edit layer and revisioned document
  controller. Bounded `LMOPEDT1` scripts upsert/remove nodes and directional edges, cascade incident
  edge removal, and require a valid reciprocal or explicitly one-way final graph before persistence.
  Path and metadata controllers both own bounded canonical histories whose navigation restores the
  entire stable-key domain without moving the saved baseline.
- Standalone overworld path and metadata workflows can atomically publish canonical normalization
  and semantic observation artifacts. This makes their stable keys, raw flags, retained unknown
  fields, and graph topology directly usable by differential suites without parsing CLI prose.
- Portable Layer 3 artifacts use a dedicated revisioned document controller and bounded
  `LML3EDT1` grammar. All proven selectors, graphics IDs, reserved bytes, tilemap bytes, and opaque
  remap commands are editable without routing them through an unverified native ROM patch layout.
  Successful edits canonically encode and reopen before entering a bounded revision-safe undo/redo
  history whose saved baseline remains independent.
- Complete `LMLEVEL2` documents also have a toolkit-neutral revisioned controller. Cross-domain
  auxiliary batches are staged, canonically encoded and reopened before commit; frontend saves use
  immutable request-correlated snapshots so edits made while persistence is in flight remain dirty.
  The shared bounded canonical-value history adds monotonic, revision-checked undo/redo while
  retaining the independent saved baseline and invalidating divergent redo branches. A focused
  runnable-shell module owns open/edit/render/status/undo/redo/save/close/discard lifecycle, while
  the CLI and application share the bounded `LMAUXED1` parser from the semantic level crate.
- The graphical aggregate level-assets editor is a thin coordinator over separate level,
  ExAnimation, palette, and expanded-settings panel modules. Those panels produce typed
  `NativeLevelAssetsControllerEdit` values only; transaction planning, ROM layout, serialization,
  ownership validation, and reclamation remain below the presentation crate.
- The runnable frontend owns all independent portable controllers through one focused session
  registry. Shutdown enumerates dirty sessions, refuses implicit end-of-input data loss, and only
  drops portable state after explicit confirmation, independently of the main ROM document guard.
- Release-oracle rendering evidence is artifact-bound rather than declarative. A focused validator
  checks the external `render.png` digest, bounded PNG chunk graph and CRCs, RGBA IHDR, dimensions,
  image data, terminal IEND, and exact consumption before a `render-level` case can qualify.
- A separate release-policy module validates provenance per case. Every qualifying workflow result
  independently carries nonempty mapper, header, region, revision, ROM-size, and fixture-family
  metadata; corpus-wide existential coverage cannot mask an under-specified individual result.
- `lm-render` exposes the full portable-level pipeline as a public typed API rather than hiding it
  in the CLI. Map16 references, appearances, source-bound Layer 3, graphics/palette references,
  rectangular shapes, and canvas allocation share one implementation across CLI and native GUIs.
- The runnable application reaches that API through a focused bounded `LMBNDR1` specification.
  Paths are spec-relative and lossless, rendering reads the open controller's current revision, and
  deterministic PNG publication is create-new so preview export cannot overwrite existing work.
- Revision-profile text handling is split into a shared field schema, canonical encoder, and strict
  decoder/validator. Frontends share one key inventory without coupling parsing control flow to
  serialization or accumulating both directions in one production source module.
- Before external release qualification, the runnable shell enforces create-new ROM publication.
  Replacing the opened ROM through `save` or a UI action requires an explicit startup capability;
  the default `save-as` route retains the atomic non-overwrite boundary.
- Emulator release evidence is artifact-bound: the qualifying observation identifies the exact
  output ROM, emulator, positive frame count, and screenshot digest/dimensions, while the shared
  bounded PNG validator checks the external `emulator.png` structure and CRCs.

## First implementation slice

1. Implement `RomImage`, copier-header detection, mapper address conversion, checksum calculation, and changed-range reporting.
2. Implement LZ2 decode/encode and the two recovered RLE forms with golden vectors, malformed streams, and property-based round trips.
3. Implement RATS parsing/allocation/erase over transactional ROM images.
4. Parse and serialize level headers, object streams, sprite streams, Map16 definitions, entrances, and exits without a GUI.
5. Add a software reference renderer for Map16 and level object placement, including the recovered standard-object dispatch slots.
6. Capture oracle fixtures from Lunar Magic and require decoded-model equality plus unchanged-region checks.
7. Build the native UI only after the headless compatibility suite passes for the corresponding subsystem.

## Compatibility tiers

- **Tier 0 — preservation:** open and save without edits; every unowned byte and unknown structure is preserved.
- **Tier 1 — core levels:** mapper support, codecs, RATS, level objects, sprites, entrances/exits, Map16, palettes, and graphics.
- **Tier 2 — advanced levels:** ExAnimation, Layer 3, custom metadata, imports/exports, clipboard formats, undo/redo.
- **Tier 3 — overworld:** layers, events, paths, warps, sprites, messages, palettes, animations, and save transactions.
- **Tier 4 — ecosystem/UI:** external tools, toolbar definitions, localization, image export, keyboard workflows, and platform-native presentation.

Core external-tool integration stops at a typed invocation boundary. The application validates and
expands executable, working-directory, and argument fields independently. The runnable shell can
load, inspect, and preview event subscriptions, and its explicit `tool-exec` commands run those
requests synchronously with a direct argument vector rather than a command interpreter. Automatic
event effects remain previews there. The graphical native frontend owns a bounded permission queue
that presents the executable, working directory, and independently expanded argument vector. It
launches approved requests directly on a worker thread, serializes confirmation and execution,
surfaces start/exit/worker failures, and never invokes a command shell; denied requests are dropped
without execution.
Project-open, confirmed-save, and real level-transition events are generated by application state,
not duplicated by individual frontends. Expansion failures become typed diagnostic effects after
the primary transition commits, preventing optional tools from blocking editor operations.

No tier is complete until its rows in `REIMPLEMENTATION_TEST_MATRIX.md` pass on clean, headered, expanded, and ecosystem-modified fixtures.

## Planned MCP agent automation

After the core compatibility tiers are qualified, the editor should optionally host an MCP server
so Codex, Claude, Gemini, and other compatible clients can construct and test ROM hacks through
the same semantic controllers as the native UI. This is a roadmap requirement, not permission for
agents to write arbitrary ROM offsets. The server boundary should expose:

- bounded project, level, overworld, Map16, graphics, palette, sprite, object, entrance, exit,
  Layer 2, Layer 3, and ExAnimation inspection;
- revision-bound transactional edits, preview, diff, validation, undo, redo, save-as, and rollback;
- catalog and constraint queries sufficient to design ordinary and complex/Kaizo levels without
  guessing command shapes, sprite extensions, screen transitions, or resource ownership;
- deterministic rendering, emulator launch/boot gates, screenshot and diagnostic collection, and
  structured play-test results;
- capability discovery and explicit policy gates for filesystem publication, external processes,
  destructive replacement, and any operation that can invalidate an existing hack.

Every MCP mutation must carry the open project revision, pass the same serializer and overlap
checks as an interactive edit, and return a semantic observation that can be replayed in the
compatibility corpus. Long-running agent sessions should work on recoverable branches or Save As
artifacts, never silently overwrite the only ROM copy. Protocol conformance, stale requests,
malformed/excessive batches, disconnected clients, concurrent edit conflicts, emulator failure,
and exact rollback belong in the compatibility matrix before this interface is considered stable.

Layer 3 currently has a deliberately lossless semantic boundary: `LMLAY3V1` preserves raw selector
and feature bytes, four validated 12-bit graphics identifiers, the recovered 0x2000-byte tilemap
workspace, and the remap command stream. Optional state is embedded in `LMLEVEL2` version 2,
failure-atomic editing is separate from serialization, and oracle observations cover every field.
Native ROM pointer/patch installation remains revision-profile work and is not guessed.
Rendering consumes a separate provider-resolved `LML3FRM1` plane. It binds to canonical Layer 3
source bytes by SHA-256 and declares one of four painter boundaries, keeping unverified native remap
opcodes out of the software renderer while still supporting deterministic oracle-produced previews.
The application shell exposes Layer 3 as a revision-scoped editor mode and keeps raw tilemap and
remap selections in separate typed clipboard namespaces. Navigation and clipboard routing are
toolkit-independent; native ROM commit support remains gated on a verified revision layout.

## Oracle record format

Each oracle case should store a manifest containing the Lunar Magic version, input SHA-256, operation identifier and arguments, output SHA-256, changed byte ranges, warnings/errors, and normalized decoded models before and after. Original ROMs and copyrighted assets remain outside the repository; manifests can reference locally supplied fixture hashes.

For saves that legally relocate RATS payloads, equality is evaluated in this order: decoded semantic model, owned payload bytes, pointer-table consistency, checksum, unchanged-region invariant, then allocation placement. Exact whole-file equality is an additional assertion only where Lunar Magic's placement is deterministic.

Release qualification is executable rather than inferred from isolated green commands.
`oracle-release-gate` requires every discovered case to replay with paired semantic observations,
then applies one explicit corpus policy containing Lunar Magic version; open/save, render, edit,
Lunar Magic reopen, and emulator-boot operations; and mapper, header, region, revision, ROM-size,
and fixture-family requirements. Cases carrying tool errors cannot qualify. This keeps behavioral
correctness and representative breadth in a single failing gate while copyrighted ROM fixtures
remain external.
Discovery continues below case directories so nested manifests cannot be concealed by a parent
manifest. The suite root and every required manifest, ROM, and observation artifact are checked
with non-following metadata and must be regular local entries rather than symlinks.
Each required operation has a typed semantic-evidence contract inside its hash-bound after
observation. The release tool validates outcomes such as checksum correctness, unchanged-region
preservation, render digest/dimensions, semantic reopen equality, and emulator boot success rather
than trusting the manifest operation label by itself.
