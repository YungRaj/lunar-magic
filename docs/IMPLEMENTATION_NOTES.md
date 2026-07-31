# Detailed implementation notes

This workspace is a platform-neutral Rust reimplementation of the observable ROM-editing behavior
of Lunar Magic. It is organized as reusable format libraries, a headless command-line tool, and a
toolkit-independent application state machine so native macOS, Linux, and Windows frontends can
share the same editing core.

It does not contain Nintendo ROM data or Lunar Magic source code. Supply only ROM images you are
legally permitted to use, and keep an immutable backup. Writable CLI operations require a distinct
output path. Every CLI artifact is staged, synchronized, and atomically published with create-new
semantics; an existing file, symlink, or hard-link alias is never replaced.
Commands producing a normalized artifact plus an oracle observation stage both first and roll back
all newly published names if any destination collides or publication fails. Multi-output paths are
resolved through their canonical parent directories before staging, rejecting lexical and
symlinked-parent aliases. Rollback verifies that a published name still identifies the staged inode
instead of deleting from stale pathname state.

## Workspace

- `lm-rom`: copier headers, supported-game identity, explicit LoROM/FastROM/ExLoROM/SA-1 map-mode
  qualification, exact SA-1 ROM-window rejection for non-ROM `$40`–`$7D` banks, whole-image mapper
  addressability, exhaustive canonical PC/bus round-trip coverage across every mapped byte, explicit
  ExLoROM WRAM-tail exclusion, bounded images, mapper-validated bank expansion, recursive mirrored
  checksums with explicit wrapping repeat arithmetic and exhaustive irregular-size reference
  coverage, changed-range tracking, and bounded IPS creation/application. IPS support consumes raw
  and RLE records in order, preserves legal overlaps, handles zero-filled growth and optional
  truncation, works around the reserved `EOF` record offset deterministically, and rejects malformed
  framing or results outside the 24-bit address space. Copier-header conversion adds a caller-filled
  512-byte prefix or removes only that prefix while preserving every logical ROM byte; original
  header state participates in restore/accept snapshots and logical changed-range comparison.
- `lm-codec`: total LZ2, recovered LZ3, RLE, and native even/odd-plane sized-RLE
  decoders plus deterministic encoders; LZ2 emits literal,
  byte/word/incrementing fill, forward-copy, bit-reversed-copy, and reverse-copy commands with
  bounded match search and overlapping-copy support. Prefix decoding reports the exact compressed
  extent of an untagged bank-bounded stream and rejects the reserved command-7 encodings; ordinary
  standalone decoding requires the terminator to consume the complete input. LZ3 is kept as a
  distinct module: its zero-fill command, short relative dictionary operands, absolute operands,
  bit-reversed copies, and both reverse-copy command numbers are decoded with explicit bounds. Its
  deterministic encoder emits all native fills and bounded forward, bit-reversed, and reverse
  dictionary transforms, selecting compact relative operands whenever their 128-byte window fits,
  including after the absolute operand's `0x7fff` source-address ceiling.
  Terminated RLE reserves `FF FF` exclusively for end-of-stream and splits an otherwise ambiguous
  128-byte `FF` run; sized RLE retains the compact packet because its container supplies the
  decoded length. Both RLE forms expose exact consumed-length prefix decoding; their ordinary
  container APIs require complete encoded consumption, so a terminator or declared output length
  cannot hide trailing packets or bytes. Tagged native graphics apply this exact boundary to the
  complete owned RATS payload, while clean untagged bank data intentionally uses prefix semantics.
  Standalone codec CLI transforms bound both input and output to 16 MiB, preflight sized-RLE output,
  reject existing or aliased destinations before input processing, and publish create-new files
  atomically. Semantic codec observations apply the same compressed/decoded ceilings and reject
  output collisions before reading or canonical re-encoding the source.
  Exhaustive three-symbol inputs through length nine round-trip through optimized and literal LZ2
  plus terminated and sized RLE at the exact output bound; every one-byte-short bound rejects, and
  every possible two-byte LZ2 input is exercised as a total non-panicking decode operation.
  Real Wine transitions may refine lossless materialization rules without requiring allocator-level
  byte identity. The Lunar Magic 3.63 `-TransferTitleScreen` fixture binds exact before/after hashes
  and changed ranges while comparing allocation-independent primary/secondary observations. It
  proves title blank-word normalization and semantically matches the Rust implementation despite
  independent allocation choices.
- `lm-rats`: validated STAR/RATS scanning, bank-aware allocation, duplicate reuse, replacement,
  protected ranges, and safe erase. Replacement and erasure require an exact currently valid block
  descriptor; forged, stale, header-corrupted, or protected descriptors cannot erase arbitrary ROM
  bytes. Protection applies equally to placement, content-based reuse, in-place replacement, and
  erasure, including the transactional project deduplication path. Search, bank, fill, and
  protected-range policy validation always precedes reuse, so an identical payload cannot conceal
  malformed allocation authority. Free-range endpoints use checked subtraction, so a zero-based
  search shorter than the requested block cannot allocate beyond its declared end; exhaustive
  small-image tests compare placement against an independent brute-force allocator across search,
  bank, occupied-byte, and protected-range combinations.
- `lm-level`: legacy/expanded headers, native object and sprite streams, current binary MWL files,
  native Layer 2 object streams and normalized 0x800-byte tilemaps with exact recovered legacy
  0x360-byte expansion/compaction and split-plane transforms,
  versioned standalone native-level and Map16 page files, Map16 page/set planes and Acts Like graph
  validation; public Map16 pages must contain exactly 256 tiles before set or file encoding and
  graph traversal. Whole-set validation caches exact distance-to-terminal rather than a boolean,
  so resolution limits include previously visited suffixes and cannot depend on numeric tile
  iteration order; exhaustive small functional graphs agree with individual chain resolution.
  Individual page serialization enforces the canonical 256-tile shape at its public boundary.
  Native whole-set saving derives tile, resolution-limit, and two-plane request counts with checked
  aggregate arithmetic before allocating the transaction batch; no count is silently saturated.
  Entrances, screen exits, and all six secondary-exit planes have exact 8,192-entry
  shape and non-masking native/MWL persistence, including checked MWL import destinations;
  packed MWL exit sections preflight exact aggregate byte size before allocating;
  revision sprite-length edits reject selectors outside the four recovered tables instead of
  aliasing them through bit masking, and native serialization validates every record against its
  exact selector/ID entry with token-specific mismatch errors; native sprite streams preflight
  escaped records, control tokens, and terminators before allocation; object streams expose the
  recovered distributed six-bit command ID, command-specific parameter byte, orientation-neutral
  coordinate nibbles, and screen-advance bit while retaining every unowned bit and extension byte,
  and preflight their exact aggregate length and native bank
  bound before allocation; failure-atomic ordered object and
  sprite editing with explicit native/portable storage limits and checked generic stream
  aggregation; cross-domain atomic
  entrance, screen-exit, secondary-exit, and stable-key Map16-override editing; bit-preserving
  header and explicit-shape raw tilemap editing; lossless bounded `LMLAY3V1` Layer 3 settings,
  four 12-bit graphics IDs, 0x2000-byte tilemap workspace, preserved remap commands, and
  failure-atomic edits; bounded `LMLEVEL2` and `LM16SET1` bundles round-trip every modeled level
  domain and complete Map16 workspace. `LMLEVEL2` version 2 embeds optional Layer 3 state while
  retaining read compatibility with version 1 bundles. Complete-level encoding validates every
  collection and preflights the exact aggregate file length before allocating, so the 16-MiB
  artifact limit cannot be crossed while composing many individually valid records.
- `lm-graphics`: recovered generic SNES 1-through-8-bpp planar tiles plus 4bpp files, with
  odd-depth final-plane layout, fallible non-truncating persistence, and exact
  aggregate-size preflight before allocation, checked fallible joining of unbounded file groups,
  versioned decoded graphics, fallible exact-word
  BGR555 palette persistence with checked two-byte aggregate sizing, lossless exact-length legacy
  and expanded `.smwpal` working-palette files including the expanded auxiliary region, exact
  version-2 TPL files with 256 native color words, exact 257-word raw SNES palette files and
  lossless 257-byte `.palm` selection masks with failure-atomic masked-import semantics, and exact
  256-color RGB24 `.pal` files with recovered high-bit versus bit-replicated expansion detection,
  fixed-record ExAnimation sets
  with visible-slot and aggregate-size validation, compact ExAnimation
  interchange, bounded pixel and
  palette-row editing, joined graphics, quantization, and validated lossless ExAnimation editing.
- `lm-overworld`: shape-validated layers with checked aggregate persistence, endpoints and
  fixed-shape messages with checked aggregate persistence,
  lossless extensible sprite records,
  events whose sole public serializer enforces reveal-source round-trip safety, plus lossless
  legacy-pair conversion across all
  256 event IDs, palettes, validated navigation graphs, exact native three-plane path-link tables,
  and aggregate editor models; `LMOWPATH` preserves
  stable path nodes, one-way/reciprocal edges, exit indices, and unowned flags, while `LMOWMETA`
  preserves level names, player starts, submap settings, and revision-specific bytes. Focused
  Sprite-appearance file sizing uses checked multiplication and addition at every aggregate
  boundary. Focused editing APIs provide bounds-checked layer/message tile changes and atomic insert/remove/reorder
  operations for events, endpoints, messages, and revision-shaped sprite records.
- `lm-title`: lossless title-screen movement recordings, bounded native interchange, exact minimal
  ZSNES V143 states, and plain/gzip Snes9x tagged-RAM extraction.
- `lm-project`: atomic multi-range edits, tagged-payload/pointer saves, bounded clean-ROM payload
  loading, native level/Map16/graphics/palette/ExAnimation/overworld-layer/event/endpoint loading
  and saving through versioned pointer layouts, including a nine-payload atomic overworld save for
  both layers, event planes, endpoints, messages, sprites, palette, and ExAnimation, a versioned
  self-describing complete-overworld interchange with checked aggregate sizing and an explicit
  whole-file bound, plus dirty tracking and batched undo/redo. Fixed-size history edits are
  compare-and-replace guarded in both directions: stale external byte changes make undo or redo
  fail without moving the history cursor, and a partially reverted batch is restored before the
  error is returned.
  Native level saves stage Layer 1 objects and sprites together with their pointer updates and SNES
  checksum as one reversible commit. A broader native-level-assets API also groups the currently
  modeled palette and ExAnimation payloads into that same allocation/checksum transaction; late
  validation, allocation, mapping, or checksum failure publishes neither ROM bytes nor history.
  Native Layer 2 loading selects object or tilemap storage from the level mode, decodes Lunar
  Magic's terminated byte-run RLE, and transactionally saves through an explicit revision layout;
  legacy high-byte and split-plane encodings are never guessed. A focused aggregate API commits
  Layer 1, Layer 2, sprites, palette, ExAnimation, optional expanded settings, and checksum repair
  as one history entry and has synthetic reopen/undo coverage.
  Optional installed subsystems use marker-gated layout alternatives with deterministic
  primary/fallback precedence. Per-level palette access distinguishes an absent installation from
  a decoded palette and refuses to create a table implicitly. Per-level ExAnimation additionally
  distinguishes subsystem absence, an empty slot, and a present payload while applying the
  recovered newer `$FFFF00` versus legacy `$FF0000` pointer-presence masks.
  Checksum-aware save variants provide the same boundary for standalone graphics, palette,
  ExAnimation, Map16 page/set, and complete-overworld workflows. CLI imports, profile imports, and
  application controllers use these variants, preventing a payload edit and its checksum repair
  from becoming independently visible operations.
  Event-reveal saves reject source indexes that Lunar Magic's loader would otherwise normalize to
  zero, before allocation or history mutation, so successful saves reopen semantically unchanged.
  Tagged saves can expand a full ROM to the mapper-valid end of their allocation policy; growth,
  allocations, pointer updates, and ordinary writes form one reversible history batch, including
  copier-header-transparent shrink-on-undo and exact restore-on-redo. Every three-byte pointer in
  a batch is intrinsically protected from every allocation even when caller policy omits it, and
  three-byte pointer ranges are derived once with checked arithmetic and shared by overlap checks,
  allocator protection, and final writes. Overflowing or overlapping pointers are rejected before
  staging rather than producing orphaned blocks.
  Fixed-shape resources enforce their declared length for both untagged data and valid RATS
  payloads, so tagging cannot bypass Map16, palette, or overworld revision shapes.
  Terminated and bounded read policies validate nonzero bank sizes, and tagged-or-untagged policies
  validate their fallback configuration before examining a tag; malformed latent fallback policy
  therefore returns a typed error instead of being hidden by a tagged payload or reaching modulo
  arithmetic.
  Every prepared mutation, including write-only mutations on projects without detected identity,
  must have a mapper capable of addressing the complete resulting logical image, and that image
  must end on a full 32-KiB mapper-bank boundary, before any write or history entry is accepted.
  Supported-ROM identity and native tagged-payload saves use the same shared image-shape predicate,
  so an addressable but truncated final bank cannot enter through a lower-level workflow.
  High-level payload replacement is copy-on-write: byte-identical blocks may be shared by several
  pointer slots, so an old allocation is retained until a revision-specific reference index proves
  exclusive ownership. Duplicate reuse must fall wholly inside the authorized search range and
  applies the same mapper-bank and protected-range constraints as fresh allocation; a structurally
  valid cross-bank tag is retained for diagnostics but cannot be reused, replaced, or erased through
  a bank-constrained allocator. An explicit RATS ownership manifest can dry-run and
  transactionally reclaim only the exact owned blocks absent from its retained subset; stale,
  duplicate, overlapping, or
  foreign retained descriptors fail before mutation. Successful collection is one undoable batch.
  The bounded canonical `LMRATS01` artifact preserves exact 64-bit block descriptors and sorts both
  sets deterministically. Stale or forged previous-block descriptors also fail before allocation.
- `lm-snes`: deterministic 65C816 code construction for independently authored ROM runtimes.
  Typed labels resolve checked signed-eight-bit branches, width transitions and common
  load/store/logic/control-flow instructions have explicit encoders, and JSL/JML operands produce
  address-independent cross-payload fixups instead of embedding allocator-selected addresses.
  Duplicate or unbound labels and out-of-range branches fail before code bytes are published.
- `lm-render`: deterministic software rendering for indexed tiles, Map16, two-layer level scenes,
  definition-driven object/sprite previews, event-materialized overworld layers and sprite
  appearances, source-bound provider-resolved Layer 3 planes with four explicit painter positions,
  priority-aware painter ordering, overflow-safe rational zoom/pan viewports, invariant-sealed
  bounded RGBA canvases with fallible construction, overflow-clipped signed and unsigned draw
  origins including Map16 quadrant offsets and extreme world/instance subtraction, and byte-stable
  dependency-free PNG export for golden fixtures. Visible world rectangles use outward-rounded,
  wide-intermediate rational spans, so even a sub-world-pixel viewport remains nonempty when it
  samples a pixel and representable extreme ratios do not fail from intermediate overflow;
  `rasterize_canvas_viewport` also turns an existing world-space canvas into a bounded camera
  raster with exact nearest-neighbor zoom and transparent clipping outside signed source bounds.
  A separate painter-ordered editor-overlay module draws signed/clipped cell grids and phased
  marching selection outlines with deterministic straight-alpha source-over blending. The
  application-level `render_editor_preview` composes these screen-space overlays after camera
  sampling, keeping selection animation independent from immutable world snapshots.
  Canonical bounded `LMOVLY01` artifacts serialize up to 256 ordered grid/selection records with
  exact signed geometry, RGBA colors, dash length, and animation phase. Every application render
  specification accepts an optional spec-relative `overlays` path; decoding and semantic
  validation complete before PNG publication.
  The headless `editor-overlay-file INPUT [NORMALIZED [OBSERVATION]]` workflow reports
  grid/selection counts and atomically publishes a canonical artifact with its painter-indexed
  `LMOBS1` semantic snapshot. Inputs and both outputs must be distinct.
  signed tile clipping is cross-checked against a wide-integer reference at every nearby and
  extreme origin. PNG chunk and stored-DEFLATE capacities use
  checked arithmetic throughout, including exact 65,535-byte block boundaries, so overflow is a
  typed render error rather than a wrapped or underestimated allocation.
- `lm-oracle`: SHA-256 fixture identity, deterministic manifest capture and decoded-model
  observations, semantic before/after comparison, changed-range comparison, explicit validated
  owned-allocation policies, content-based RATS relocation matching, and unchanged-region
  verification. Recorded Lunar Magic errors remain diagnostic data but invalidate ordinary and
  release-gate case reports, so a rejected external operation cannot count as parity evidence.
  Parsed and newly captured manifests require nonempty case/version/operation identity, canonical
  lowercase 64-digit SHA-256 fields, and unique nonempty operation-argument names before they can
  become
  corpus coverage evidence. The public corpus auditor independently revalidates programmatic
  manifests, reports invalid cases by stable input index, and excludes them from every version,
  operation, and argument coverage requirement. Changed and owned allocation claims must be
  nonempty, sorted, and pairwise disjoint; direct replay exposes manifest validity and cannot pass
  when programmatic range claims violate those invariants.
- `lm-app`: native-frontend-neutral commands, path-aware document lifecycle and file-dialog effects,
  snapshot-safe asynchronous save acknowledgement with overlapping-save rejection, typed/versioned
  cross-platform clipboard payloads, canonical editor selections, copy/cut/paste routing, menu
  capabilities, bounded back/forward level-and-viewport navigation with branch invalidation and
  exact rational zoom restoration constrained to the recovered 100–5000% editor range,
  toolkit-independent `render_editor_viewport` and compatibility `render_level_viewport` adapters
  connecting that camera state to the shared software renderer,
  confirmation-safe open/close/quit behavior, and versioned
  external-tool configuration with shell-free Unicode-safe template expansion and event
  subscriptions. Frontend protocol/effect and capability schemas live in a focused module separate
  from mutable application state and document persistence. Save-dialog cancellation is an explicit
  transition that releases the pending snapshot while preserving the document path, dirty baseline,
  revision, and undo history. Runnable-shell orchestration is separated from its large test-only
  end-to-end workflow corpus, while bounded editor-script loading and revision-bound level,
  Map16, palette, graphics, ExAnimation, and overworld commit preparation live in a focused editor
  shell module instead of the executable entry point. Custom-object sidecars, Layer 3 documents,
  overworld paths, and overworld metadata each have a separate shell module for bounded loading,
  revision-safe editing, atomic save acknowledgement, status, and guarded close/discard behavior.
  External-tool configuration replacement and subscribed-event expansion also live in a focused
  application-state module, separate from generic command dispatch and the platform process runner.
  Recent documents, aggregate frontend configuration, localization, toolbar/shortcut resolution,
  action enablement, and capability reporting share a separate frontend-state module. Undo/redo,
  revision tokens, immutable controller snapshots, and no-op-aware atomic ROM writes and growth
  mutations are grouped in a transactional project-state module used by every editor controller.
  Revision-profile validation, identity/audit qualification, installation, clearing, and controller
  invalidation form their own application-state boundary. Navigation transitions and the typed
  editor-selection/clipboard compatibility matrix are separate policy modules rather than branches
  embedded in generic lifecycle dispatch. A strict OS-string startup parser supports a legacy
  positional ROM or `--rom`, plus optional `--profile`, `--ui-config`, `--tools-config`, and
  `--recent-state` preload paths without losing non-UTF-8-capable platform path semantics. Startup
  profiles use the same bounded identity-bound audit and revisioned installation as interactive
  profiles. The explicit recent-state store is bounded, rejects symlinks and non-files, and uses
  atomic create/replace publication only when the MRU value changes. `--script FILE` runs a bounded
  UTF-8 command stream through the exact interactive parser and dispatcher; confirmation answers
  come from subsequent script lines, and dirty-project EOF remains an error.
External-tool encoding preflights the exact aggregate `LMTOOLS1` length before allocation, so its
64-MiB format limit cannot be enforced only after composing a much larger temporary buffer.
Clipboard envelopes enforce per-record/count limits and a checked 64-MiB aggregate encoded limit
before allocation or record copying, preventing individually valid records from composing an
unbounded platform clipboard payload.
Frontend presentation is also data-driven: strict `LMLOC001` catalogs provide a complete typed
Unicode string set, while `LMTBAR01` layouts bind stable identifiers and localized labels to
toolkit-neutral actions. `LMSHORT1` maps Unicode, function, editing, and navigation keys plus
platform-neutral primary/secondary/shift/alt modifiers onto those same typed actions. All three
formats are bounded, versioned, deterministic, and installed atomically so malformed configuration
cannot replace a working catalog, toolbar, or shortcut set. `LMUICFG1` additionally packages the
three canonical formats into one independently length-bounded frontend bundle; application state
validates every nested section before replacing any part of the active configuration.
Toolbar and shortcut activation share application-owned enablement. Parameterless actions resolve
to exact shell commands; copy, cut, and paste resolve to typed requests for editor serialization or
platform clipboard access, keeping native frontends free of duplicated lifecycle rules.
The native effect bridge also forwards application-validated `WriteClipboard` bytes through the
same bounded `LMCLIP1` text envelope. It validates the canonical payload again at the platform
boundary; cut and paste still require an active editor-owned selection so neither can degrade into
a misleading copy-only or untyped operation.
The runnable frontend exposes this path through `ui-config FILE`, `ui-status`,
`ui-action ACTION`, and `ui-shortcut GESTURE`. Gestures use tokens such as
`primary+shift+f12`, `secondary+left`, or a single Unicode character. Both named and shortcut
actions pass through the same live capability gate; path chooser and clipboard actions are
reported as explicit frontend requests instead of bypassing their ownership boundaries.
Open, Close, and Quit are rejected while persistence is in flight so a frontend cannot discard the
document context required to acknowledge that exact snapshot.
Save effects carry monotonically assigned request identifiers. Successful, failed, and cancelled
callbacks must return that identifier; a delayed callback from an older save cannot consume,
cancel, or mark clean a newer pending snapshot, even when both saves target the same path.
ROM chooser effects carry monotonically assigned request identifiers. Completion and cancellation
must return the matching identifier; duplicate, delayed, or superseded results cannot replace a
newer document, and edits made while a chooser is open invalidate its completion.
The direct ROM loader is startup-only and rejects an existing document or pending chooser, so
interactive frontends cannot bypass confirmation and request correlation through a second API.
- `lm-native`: an initial cross-platform graphical frontend built on `eframe`. It drives the same
  correlated `lm-app` open/save protocol as the terminal adapter, uses native file dialogs, bounds
  selected ROMs to 32 MiB plus a copier header before reading, performs recoverable persistence on
  a focused worker thread,
  and exposes dirty-close confirmation, undo/redo, level navigation, and editor-surface switching.
  Only immutable bytes and the destination cross the worker boundary. Completion returns the exact
  request ID to `AppState` on the UI thread; success advances that snapshot's saved baseline, while
  create-new collision, replacement failure, worker failure, or stale acknowledgement releases or
  preserves application state according to the same request-correlated protocol without freezing
  rendering.
  Portable palette, graphics, and Map16-page windows reuse that worker boundary with their own
  controller request IDs. Their save buttons and close actions are gated while I/O is active;
  completion acknowledges only the matching document snapshot, and failure cancels that pending
  snapshot while retaining edits for retry.
  The ROM menu also exposes a global secondary-exit editor over the profile-derived 8,192-entry
  native table. It opens either pristine four-plane SMW data or either recovered installed
  six-plane layout, stages all six logical fields by hexadecimal entry number, and commits through
  the same revision-checked application command as the headless workflow. A pristine commit
  installs the recovered Lfix3 and expanded-secondary-exit runtime transactionally; an installed
  commit updates its owned planes. Stale dispatches leave the staged workspace open, and dirty
  close or quit requires confirmation.
  A separate title-screen recording window exposes the recovered `$4..=$8000`-byte movement
  payload without interpreting unknown input commands. It detects pristine ROMs or the exact
  two-RATS-block Lunar Magic playback installation, displays installed bytes in canonical
  sixteen-byte hexadecimal rows, validates two-digit tokens and the final `$FF`, and dispatches
  the same transactional install/update command used by the CLI. Invalid text, stale revisions,
  and rejected commits retain the editable payload; dirty close and quit require confirmation.
  Shared native tilemap workspaces cover the title screen and credits without duplicating their
  ROM formats in the frontend. The title editor addresses both 32×29 materialized word planes;
  the credits editor addresses the complete 32×256 word grid, including rows unavailable to the
  pristine 202-row representation. Both use hexadecimal row, column, and word forms with an
  explicit load-before-apply guard, then dispatch the recovered pristine-install or installed-update
  application command. Revision changes, invalid coordinates, failed dispatch, dirty close, and
  application shutdown retain or confirm staged state through the same lifecycle boundary.
  Overworld global setup has two additional ROM workspaces. Player starts expose Mario and Luigi's
  tile-centered X/Y coordinates and seven native submaps while displaying and preserving the four
  adjacent unowned option bytes. Global settings expose every word of all seven lossless
  `$200..$206` special records. A profile-level detector distinguishes verified installed storage
  from pristine defaults, so the frontend never probes `STAR` offsets. The latter commit installs
  expanded settings when absent; both editors use load-before-apply, stale-revision, rejected-
  dispatch, and dirty-shutdown safeguards.
  Event support adds two more exact-table windows. The event-number editor addresses all 256 byte
  mappings and exposes the currently stored native prefix; editing a high entry intentionally
  selects the recovered extended-table installation. The special-event editor addresses all 24
  source/destination/direction records and validates the `$07FF` source limit before staging.
  Both commit through their existing application commands, retaining edits on stale revisions,
  invalid forms, rejected dispatch, close, and application shutdown.
  The level-name window losslessly edits every byte of each 19-tile native name by canonical SMW
  level number. It reconstructs pristine dictionary names on open, grows direct-record storage
  through the `$024`/`$101` numbering gap with explicit blank records, validates the complete
  positional prefix, and uses the same transactional install/update and semantic-reopen command.
  Selection changes require an explicit reload, and stale commits or dirty shutdown retain staged
  data rather than guessing a font-to-Unicode mapping.
  Boss-sequence text has its own modular workspace for all seven 24×8 tile grids. Its form,
  revision-bound storage, and lifecycle are separate modules; exact tile-index editing validates
  the recovered 56-row native framing before dispatch and transactionally converts pristine
  scattered rows into the combined owned allocation used by installed ROMs.
  Typed external-tool effects enter a bounded native permission queue. The confirmation displays
  the exact executable, working directory, and every independently expanded argument before a
  launch is allowed. Approved processes run directly without a command shell on a worker thread;
  start failures, signals, nonzero exits, and disconnected workers remain visible without blocking
  rendering, while denial never starts a process. Manual and event-triggered tools share this path.
  Platform presentation, dialogs, effect execution, and editor content are split into focused
  modules; the crate contains no ROM offsets or serialization logic. Its Profile menu installs an
  identity-audited `LMREVPRO1` file through `AppState`, after which Graphics, Palette, and Map16
  modes decode immutable revision-bound controller snapshots and display nearest-neighbor textures
  produced by the shared `lm-render` APIs. Preview failures remain visible diagnostics rather than
  guessed layout behavior. The Documents menu also opens a complete interactive `LMPAL1` palette
  editor in a separate native window. It provides exact swatch selection, platform color picking,
  canonical BGR555 conversion, 100-state controller undo/redo, immutable recoverable saves, and
  dirty-document close/quit confirmation. ROM-backed palette editing requires exact contextual
  `LMPALOWN` ownership evidence plus an explicit allocation range. Fixed and ExAnimation-owned
  colors remain visible and copyable but their mutation controls are disabled; the native UI never
  silently marks them editable.
  `lm-cli graphics-ownership-file` and `palette-ownership-file` validate these artifacts, optionally
  publish canonical copies, and emit entry-addressable semantic observations as one atomic
  create-new batch.
  ROM-backed graphics editing likewise requires exact `LMGFXOWN` tile ownership evidence and a
  separate allocation range. Fixed and ExAnimation-owned tiles remain previewable and copyable,
  while paste and pixel-paint controls are disabled. The Documents menu opens canonical `LMGFX4BP`
  graphics with a selected `LMPAL1` color
  source. Its separate editor provides palette-row and index selection, a scrollable tile sheet,
  enlarged 8×8 click/drag pixel painting, exact 4bpp validation, canonical controller reopen,
  bounded undo/redo, recoverable save, and sequential dirty-document quit protection. Tile drawing
  and float-safe hit testing live in a focused painter module rather than the document controller.
  Every relocatable ROM-backed editor—level assets, Map16, palette, graphics, ExAnimation, and
  overworld—also exposes a separate reclaiming commit. It loads a bounded canonical `LMRATS01`
  manifest and delegates the complete allocation, repointing, proven-old-block erasure, checksum
  repair, semantic reopen, and application-history mutation to its domain controller. The ordinary
  commit remains copy-on-write; the UI never infers reclamation authority from a `STAR` tag.
  Prepared editor commits use an explicit application-dispatch acknowledgement. A rejected stale
  or otherwise invalid mutation leaves the editor and all staged controller changes open for
  inspection and retry; only an accepted revision-checked command closes the workspace. The ROM
  expansion and complete graphics-codec migration dialogs follow the same rule, retaining their
  exact target, fill, codec, and allocation-range inputs after rejection. Profile installation and
  removal likewise invalidate cached native previews only after the application accepts the
  profile transition. Profile-backed graphics, palette, and Map16 decoding plus rasterization runs
  on a revision/mode-keyed renderer worker; stale completions are discarded before texture upload,
  and only the final `egui` texture creation remains on the event thread.
  Portable `LM16PAGE` documents are editable as a third standalone window after selecting their
  `LMGFX4BP` and `LMPAL1` display dependencies. The page uses the shared Map16 renderer, supports
  exact 16×16 tile selection, all four packed 8×8 subtile fields and flips, Acts Like values,
  controller undo/redo, recoverable saving, and dirty-close protection. Its document lifecycle,
  texture/hit-testing adapter, and packed-subtile form are separate source modules.
  Portable `LMEXAN1` documents have an interactive ExAnimation editor as well. Opening remains
  revision-explicit: the UI requires the recovered 256-byte size-mode table and asks for the
  maximum record count instead of guessing either value. It edits slot setting/header fields,
  trigger enable/value pairs, record transfer metadata, and ordinary one- or two-word frames;
  records can be appended or removed, while special no-frame transfer kinds are displayed but not
  misleadingly exposed through the ordinary frame form. All mutations use the compact document
  controller's canonical reopen, bounded history, recoverable save, and dirty-shutdown lifecycle.
  `LMLEVEL2` complete-level documents now have a native authoring window backed by a unified
  cross-domain transaction boundary. It requires exact row-major dimensions and explicit
  `LM16SET1`, `LMGFX4BP`, and `LMPAL1` rendering dependencies, then uses the shared level renderer
  for selectable Layer 1/2 tilemaps. The UI edits Map16 cells, level number, legacy header fields,
  sprite header, lossless Layer 1/2 object records, revision-sized sprite records, and entrances.
  Focused auxiliary and advanced panels additionally edit raw screen exits, every secondary-exit
  field, keyed local Map16 definitions, the optional 16-word expanded header, and complete Layer 3
  state: selectors, four 12-bit graphics files, exact reserved bytes, raw tilemap bytes, and
  literal remap commands. Optional Layer 3 and expanded records can be enabled or disabled without
  inventing revision-specific meanings for their opaque values.
  Controller batches can also atomically span properties, either object layer, sprites, Layer 3,
  and every auxiliary domain; one invalid late command rolls the entire batch back. Rendering,
  record/form parsing, panel composition, and lifecycle/persistence are separate native modules.
  The automatically detected pristine SMW-US ROM editor also keeps a bounded staged history across
  header, Layer 1 object, and sprite mutations. Undo can return byte-for-byte to the opened-ROM
  baseline, redo survives ordinary navigation, and a divergent semantic edit invalidates only the
  abandoned branch; the visible forms and canvas selection are refreshed after either operation.
  Native screen-jump controls receive a separate packed-target editor that preserves their recovered
  low-first or high-first encoding and rejects targets that the original record form cannot hold.
  Native screen-exit objects likewise have a dedicated source-screen and destination/flags form.
  Editing follows Lunar Magic's recovered command-zero parameter-0/parameter-2 compact and extended
  representations, can change record shape without losing the unrelated new-screen bit, and has a
  reciprocal Lunar Magic 3.63 MWL import/re-export oracle.
  The same workspace edits the four pristine main-entrance planes and detects Lunar Magic's
  separately owned midway runtime. Installed midway records retain all four packed bytes and update
  only after hook, helper, table-pointer, and RATS-ownership validation. On a pristine ROM the GUI
  can install the complete Lfix3 core, `$D0` helper, `$800` midway table, enable flag, relocations,
  and checksum repair as one undoable mutation. Wine oracles prove Lunar Magic 3.63 re-exports both
  direct installed-table updates and first-time Rust installations with exact entrance semantics.
  The sprite panel reports canonical original and staged byte lengths under the active SSC record
  table. Non-growing streams use the exclusive in-place path; growth uses the Wine-verified
  copy-on-write path confined to the pristine shared bank, updates only the selected level's low
  pointer, retains old unowned bytes, and repairs the checksum.
  ROM-backed aggregate level-assets editing requires `LMPALOWN` evidence for its palette domain;
  protected colors remain read-only even while level, ExAnimation, and settings edits are staged in
  the same transaction. Its native adapter separates bounded evidence/profile lifecycle, focused
  aggregate panels, and four-domain allocation/reclamation commits. Both commit variants consume
  the same profile-derived save plan, so their table protection and placement shapes cannot drift.
  Complete `LMOWFULL` overworld documents are also interactive. Opening requires the exact
  256-byte ExAnimation size-mode table, explicit maximum animation-record count, `LM16SET1`, and
  `LMGFX4BP`; the palette is owned by the aggregate itself. The shared renderer provides Layer 1/2
  tile selection and an event-reveal preview slider. Focused panels edit fixed-shape event reveal
  pairs, endpoints, individual message tiles, lossless sprites including revision extension bytes,
  every embedded palette color, and compact ExAnimation globals, triggers, records, and frame
  words. All edits pass through `OverworldDocumentController` with canonical reopen, undo/redo,
  recoverable save, and dirty-shutdown protection. Animation-mode reading is a shared exact-table
  module used by both standalone and overworld editors.
  The ROM-backed overworld editor likewise requires `LMPALOWN` evidence before decoding its
  nine-domain workspace, so a combined save cannot acquire palette authority by implication.
  Portable `LMOWPATH` navigation graphs have a focused native editor too. Opening makes the
  optional reciprocal-edge validation policy explicit; node forms edit stable IDs, coordinates,
  submaps, optional level links, and raw flags, while edge forms edit stable source/direction
  keys, destinations, optional exit links, deliberate one-way state, and all unowned flag bits.
  Node and edge mutation, forms, and shell lifecycle are separate modules. Changes use
  `OverworldPathController` canonical validation and history, recoverable replacement, and the
  same dirty-close and sequential quit protection as the other portable document windows.
  `LMOWMETA` documents are likewise editable without assuming a native ROM table. Its focused
  forms cover stable-key level-name records with all 19 exact tile bytes, player coordinates and
  submaps, and music/palette/Layer 1/Layer 2 scroll settings. Raw flags and the five
  revision-specific submap bytes remain visible and lossless. The separate metadata window uses
  `OverworldMetadataController` validation, canonical undo/redo, recoverable saves, and
  dirty-document close and quit confirmation.
  Provider-resolved `LMENTAPP` level entity appearances have their own painter-order editor.
  Layer 1 objects, Layer 2 objects, and sprites retain full 32-bit semantic IDs; each record edits
  its exact tile, palette row, signed origin, and flip state. Records can be inserted, replaced,
  removed, and reordered without conflating their sequence key with their semantic source. The
  focused form and lifecycle modules delegate canonical validation and bounded undo/redo to
  `EntityAppearanceDocumentController`, with recoverable saving and dirty-shutdown protection.
  The overworld renderer's `LMOWAPP1` provider boundary is authorable in a parallel native window.
  Stable 16-bit sprite definitions can be inserted, removed, and reordered, and each definition's
  painter-ordered parts expose the exact tile, palette, signed offsets, and flip state. Nested
  definition and part forms are separate from lifecycle orchestration; all mutations pass through
  `OverworldAppearanceDocumentController` canonical validation and history with recoverable saves
  and dirty-close/quit protection.
  Standalone `LMLAY3V1` artifacts are editable in a focused Layer 3 window. It exposes the four
  selector/flag bytes, four recovered 12-bit graphics-file IDs, all 16 reserved bytes, the bounded
  raw tilemap workspace, and literal remap-command stream without assigning guessed opcode
  meanings. The independent form produces one failure-atomic `Layer3Edit` batch; lifecycle code
  delegates canonical reopen, bounded history, recoverable persistence, and dirty shutdown to
  `Layer3DocumentController`.
  Binary MWL containers also have a lossless native editor. It preserves the source version and
  exposes only independently recovered flags, the exact 48-byte attribution field, and the level
  number from a valid 64-byte level-header section. All eight sections remain selectable opaque
  byte streams with their current lengths visible. A typed Layer 1 panel preserves the exact
  five-byte legacy header and edits ordered 3–8 byte standard, extended, and custom object records.
  For a selected record it also exposes the recovered distributed command ID, command-specific
  parameter, orientation-neutral coordinate nibbles, screen-advance bit, and either packed
  screen-jump target without assigning an unsupported X/Y interpretation.
  A separate typed sprite panel exposes the stream header, ordinary/custom records, expanded
  Y-position/control tokens, ordering, and the four revision record-length tables without hiding
  extension bytes. Section replacement, semantic object/sprite commits, and container-header edits
  remain distinct so one operation cannot silently overwrite another. Focused MWL forms feed
  `MwlDocumentController` canonical transactions, undo/redo, recoverable saves, and dirty-document
  shutdown handling.
  The built-in pristine-SMW ROM editor uses the same recovered sprite field model: a selected
  record exposes its sprite number, five-bit screen, X, low five Y bits, and extra bits alongside
  the lossless raw bytes. Applying those fields repacks only the native three-byte record and
  immediately updates the level canvas placement. The ROM canvas uses a fixed tile scale with
  two-axis scrolling across all 32 screens and grows its perpendicular axis to keep the second
  native sprite row visible; artwork, hit testing, and drag coordinates share that exact space.
  Object-backed Layer 2 streams are decoded through the adjacent `$02E600` pointer table and
  painted behind Layer 1 with the same tileset handler map; both layers contribute to the bounded
  canvas extent. Compressed Layer 2 tilemaps are also projected behind Layer 1 as the recovered
  32×32 Map16 plane. Their storage uses column-major 16-row halves rather than ordinary row-major
  order; the native canvas applies Lunar Magic's exact index formula and expands to show the plane.
  Explicit one-shot canvas tools also create an ordinary object or sprite at the clicked tile from
  the matching semantic form. Object creation inserts at an absolute screen and regenerates only
  owned transitions; sprite creation validates the selected native width and restores stable
  cross-screen order. Failed placements leave the staged stream unchanged.
  The sprite form includes a searchable visual catalog for every authenticated standard handler
  from `$00` through `$ED`. Catalog cells render through the same recovered mode-, orientation-,
  position-, and graphics-aware dispatcher as the canvas. In addition to the packed within-screen
  byte, catalog dispatch now receives the form's complete native major axis (`screen * 16 + X`) and
  five-bit minor axis, so coordinate-sensitive handlers do not render as though every selection
  were at origin. The packed handler byte is built from the minor and major coordinate nibbles; it
  is no longer confused with the serialized `yyyyEESY` record byte containing extra and screen
  flags. Choosing one constructs a valid native record and arms one-shot placement instead of
  requiring users to type packed bytes.
  Attached `.ssc` metadata contributes a separate description/hex-searchable custom catalog.
  Default selectors are deduplicated by sprite number and extra-bit table, render through SSC plus
  optional external-Map16 definitions, and materialize the exact declared native record width with
  zero-filled extension bytes. The direct-ROM decoder derives its complete four-table length model
  from those selectors, reloads when that authority changes, preserves extensions during semantic
  edits, and rejects conflicting declarations before parsing or mutating a stream.
  A sprite can also be dragged directly on either horizontal or vertical level canvases; the drop
  is converted back to bounded native
  screen/X/Y fields, stably restores Lunar Magic's legacy screen ordering while retaining
  within-screen priority, and commits both changes through the same controller transaction. A
  direct-ROM Wine oracle confirms that Lunar Magic 3.63 exports the exact Rust-relocated,
  field-edited, screen-sorted stream. Standard sprite previews receive the
  level's actual mode and horizontal/vertical orientation, so the recovered position- and
  mode-dependent generator labels no longer render with a fabricated horizontal mode-zero context.
  Animated standard previews advance through their recovered four phases at 8 Hz, with repaint
  scheduling enabled only while an animated sprite is present.
  Both the standard and SSC atlas catalogs now pass the same current animated GFX33 texture used
  by placed sprites. Each catalog subtile therefore routes bit `$0200` to the animated page while
  page-zero subtiles remain backed by the ordinary SP atlas.
  Standard catalog parts also apply the placed canvas's authenticated translucency for `$E1`'s
  `$1B8` ghost and `$90`'s `$1C0-$1F3` range. SSC catalog parts remain opaque because those
  standard-handler-specific rules do not own custom definitions. The placed canvas now makes the
  same source distinction, so a custom SSC `$E1` or `$90` cannot inherit translucency merely from
  its sprite number.
  The sprite atlas now materializes all eight native sprite palette rows instead of precoloring
  every definition with row 8; each 16-bit subtile selects its encoded palette while retaining
  flips. Resolved SSC previews also consume the global `$10000` graphics-base and `$20000`
  palette-source tables. Graphics bases representable by the loaded 1,024-tile atlas are applied
  to every subtile. External graphics pages and custom palette blocks remain explicit in the
  toolkit-neutral preview model and produce an honest unresolved GUI marker until their
  `ExternalGraphics` assets are loaded, rather than displaying incorrect vanilla art.
  Texture availability no longer controls placement geometry: when those remapped SSC parts are
  unresolved, their complete signed display offsets still determine selection outlines and pointer
  hit testing instead of collapsing the custom sprite to its encoded one-cell marker.
  The toolkit-neutral asset path already decodes all eight bounded `ExSpriteGFX00–07.bin` slots
  plus either SNES-word `.mw3` or RGB-triplet `.pal` custom palettes and rasterizes complete
  16×16 SSC definitions with per-subtile palette selection, transparency, and flips. Opening an
  SSC in the native frontend now discovers the nearest project `ExternalGraphics` directory,
  applies Lunar Magic's `.mw3`-before-`.pal` preference, reads only present assets through bounded
  background I/O, and caches successfully rasterized definitions in both the custom catalog and
  level canvas. Mixed definitions are supported as well: ordinary SP1–SP4 indexed graphics can
  use an SSC external palette, and `ExSpriteGFX` tiles can use the current level's ordinary sprite
  palette. The proven global source map is not conflated with the sprite atlas: mode 1 (`+$0000`)
  resolves the retained foreground tiles, mode 2 (`+$0400`) resolves SP1–SP4 after subtracting
  `$400`, mode 3 (`+$0900`) resolves the pristine eight-slot Layer 3 cache (GFX28–2B followed by
  four blank slots), and mode 0 (`+$2000`) resolves external slots. When the validated expanded
  settings allocation is installed, words 15→12 select the active level's four Layer 3 files;
  otherwise the same loader materializes Lunar Magic's pristine defaults. Reopening or closing the SSC,
  changing the ROM revision, or changing the active graphics tilesets invalidates those textures.
  Ordinary objects can likewise be dragged across all 32 native screens on horizontal or vertical
  canvases. The atomic relocation updates the two proven coordinate nibbles, stably orders ordinary
  objects by absolute screen, and regenerates minimal advance bits or canonical screen-jump
  controls while preserving extension bytes and trailing opaque controls. Interleaved unknown
  command-zero controls are rejected rather than guessed. Lunar Magic 3.63 Wine import/re-export
  confirms the cross-screen coordinate and transition rewrite exactly.
  A companion visual Add Object catalog enumerates all noncontrol commands `$01`–`$3F`, resolves
  each through the active normal/castle/rope/underground/ghost-house handler family, and previews
  authenticated Map16 footprints from the loaded tileset. Valid handlers without visible recovered
  cells retain an explicit hexadecimal fallback. Choosing an object initializes its smallest
  canonical parameter form and arms absolute canvas placement.
  Attached `.osc` metadata adds an active-tileset custom catalog searchable by hexadecimal
  object/parameter pair or description. Its composites resolve ordinary and external Map16
  definitions, while placement constructs the native command-derived 3–8-byte stream shape and
  retains required extension bytes. OSC compact/linear metadata lengths are not misused as native
  level-stream framing.
  The interpretation-bound native-level document editor now exposes the same five semantic sprite
  fields for installed/custom records. It validates edits against the exact 1,024-byte length table
  loaded with the document, preserves all extension bytes, and rejects sprite-number/extra-bit
  changes that would silently select a different record width.
  Its object panel likewise exposes command, parameter, coordinate nibbles, and absolute screen for
  ordinary standard/custom records. One atomic batch validates command/parameter shape and then
  uses the same transition-preserving relocation engine, retaining custom extension bytes.
  The window now includes an orientation-aware placement canvas derived solely from the document's
  streams and header. Its fixed editing scale grows both native axes for decoded placements,
  including expanded sprite upper-coordinate tokens, and scrolls across all screens. It draws
  screen boundaries, object footprints, authenticated standard-sprite composite geometry, and
  labeled unresolved/custom sprite markers; clicking a placement loads the corresponding semantic
  form. Explicit canvas tools move the selected ordinary object through transition-preserving
  stream relocation or rewrite a selected sprite's coordinate fields as one undoable edit while
  preserving its identity, extra bits, and custom extension bytes. Expanded sprite moves can cross
  upper-coordinate bands: shared screen controls are rebuilt into minimal canonical transitions,
  record order is retained, and the selected token index is tracked. Still-uninterpreted opaque
  control tokens fail closed rather than being silently reordered. The canvas deliberately does not
  fabricate ROM graphics or SSC/OSC artwork absent from this portable document boundary.
  Exact 32-byte expanded-settings records have a standalone native editor as well. All sixteen
  little-endian words are visible as four-digit hexadecimal values and applied as one duplicate-
  free atomic batch; the UI deliberately does not label meanings that have not been proven.
  Focused word parsing is separate from `ExpandedSettingsDocumentController` lifecycle ownership,
  which provides exact history, immutable recoverable saves, and dirty-close/quit protection.
  Paired `.mw0`/`.mw0t` custom-object libraries are editable as one synchronized native document.
  The window edits complete variable-width object records and one-line Unicode descriptions,
  supports insertion/replacement/removal/reordering and Unicode search, and exposes retained UTF-8
  BOM, LF/CRLF, and trailing-line framing. Entry forms and lifecycle code are separate; mutations
  use `CustomObjectLibraryController` canonical history, and saves publish both existing sidecars
  through one recoverable pair replacement before acknowledging the shared snapshot.
  Paired `.mw2`/`.mwt` custom-sprite placement libraries have the corresponding native editor.
  Opening requires the exact 1,024-byte revision sprite-length table; no three-byte default is
  guessed. The editor retains the binary header, multi-record placement boundaries, every
  variable-width sprite byte, Unicode descriptions, and BOM/newline/trailing framing. Placements
  support insertion/replacement/removal/reordering and Unicode search. Focused multi-line forms
  feed `CustomSpriteLibraryController` revision validation, while save publishes the synchronized
  pair recoverably before acknowledging its immutable snapshot.
  Native `.m16` and `.s16` Map16 sidecars have an explicit-kind raw-entry editor. Opening asks
  whether the file is the exact 2,048-entry custom-object table or the 28,672-entry sparse sprite
  workspace instead of inferring semantics from bytes. Every entry remains one lossless 32-bit
  value, and the window shows the controller's current canonical save length. Focused form parsing
  feeds `NativeMap16SidecarController`; exact `.m16` sizing, `.s16` last-nonzero/block rounding,
  undo/redo, recoverable saving, and dirty shutdown stay controller-owned.
  Lossless `.dsc` custom-display sidecars have a complete-source native editor. The source is
  represented as exact hexadecimal bytes rather than forced through UTF-8, preserving malformed
  lines, non-UTF-8 payloads, optional BOM, line endings, unknown flags, and unrecognized records.
  A separate read-only diagnostics panel shows recovered description/style, display-mapping, and
  alternate-mapping interpretations for valid records without synthesizing a writer. Source-form
  parsing and diagnostics are separate from `DscSidecarController` history, recoverable save, and
  dirty-shutdown lifecycle.
  Complete `LM16SET1` workspaces are editable independently of the single-page interchange.
  Opening requires explicit `LMGFX4BP` and `LMPAL1` display dependencies, then the shared Map16
  renderer provides page-by-page tile selection. The window edits every existing page's four
  packed subtiles and Acts Like target, validating the complete graph with a workspace-sized
  traversal bound. Standalone documents can append a canonical blank page or remove the last page;
  both operations are graph-validated, revisioned, canonically reopened, and undoable. ROM-backed
  page-table growth remains profile-gated because its pointer-table semantics are revision-specific.
  Rendering/selection, packed-subtile parsing, controller lifecycle, recoverable saving, and dirty
  shutdown remain separated across focused native modules.
  Run it with `cargo run -p lm-native`.
- `lm-cli`: inspection, addressing, codecs, RATS listing, diffs, verified copy-on-write native-level,
  Map16, and graphics export/import, bounded patches, and explicit or automatically located
  checksum repair. Its command/operation schema, reusable scalar-value parsers, and native
  asset-inspection subgrammar, six native import/export transfer grammars, and four image-to-Map16
  import grammars live in focused modules separate from top-level command orchestration and
  execution. The shared command data model is itself split into bitmap-import records,
  oracle-capture records, and native transfer/migration records behind stable re-exports. The
  exhaustive positional-parser corpus is also isolated in test-only modules, so production
  argument orchestration remains compact.
  `ips-create BEFORE AFTER OUTPUT.ips` emits a deterministic patch and `ips-apply SOURCE PATCH.ips
  OUTPUT` applies one to a create-new destination. Both workflows bound every input, reject output
  aliases before reading, support growth and shrink trailers, and never replace an existing file.
  The application shell exposes the same library through `ips-create SPEC` (`LMIPSC01` with
  `before`, `after`, and `output`) and `ips-apply SPEC` (`LMIPSA01` with `source`, `patch`, and
  `output`). Specification-relative paths preserve spaces and Unicode across platforms; all assets
  remain bounded and publication uses the shared atomic create-new persistence layer.
  The graphical frontend's **Apply IPS Patch…** action instead applies a bounded patch directly to
  the open project's logical ROM. It previews source/target sizes and changed bytes, preserves an
  optional copier header exactly, requires a complete mapper-addressable bank and unchanged stable
  cartridge identity, and commits growth or shrinkage as one undoable revision. Stale, malformed,
  identity-changing, and exact no-op patches leave history and bytes untouched; success clears the
  installed revision profile because arbitrary patching invalidates its audit evidence.
  `copier-header-add INPUT OUTPUT FILL` and `copier-header-remove INPUT OUTPUT` provide the same
  bounded create-new conversion from the CLI. The application equivalents consume `LMHDRAD1`
  (`input`, `output`, decimal `fill`) and `LMHDRRM1` (`input`, `output`) specifications, retaining
  Unicode/space-containing relative paths and refusing no-op or colliding conversions.
  The graphical **Convert Copier Header…** workflow operates on the open project instead. It
  displays the current physical state and unchanged logical size, adds a caller-selected exact
  fill or removes and retains all 512 existing bytes, and enters that physical-prefix conversion
  into ordinary revisioned history. Undo restores nonuniform removed headers byte-for-byte, redo is
  compare-guarded, dirty/save state includes header-only changes, and pending saves or stale dialogs
  cannot mutate the document.

Complex crates are deliberately divided into focused modules; raw ROM offsets do not cross into the
application shell. The aggregate native level-assets surface follows the same rule: its coordinator
owns only tab state and revision loading, while level/object/sprite editing, ExAnimation editing,
palette ownership UI, and expanded-settings forms live in four sibling modules. These panels emit
typed controller edits and contain no allocation, pointer, checksum, or serialization logic.

Current binary `.mwl` containers use the recovered eight-section directory. Parsing bounds the
whole file and every section, rejects header overlap, noncanonical empty entries, and pairwise
section overlap, while retaining every section payload and attribution byte. `mwl-normalize`
decodes and atomically emits a canonical contiguous create-new file for fixture comparison; it does
not claim semantic knowledge for still-opaque fields inside native sections. Common two-word
payload sections preflight metadata-plus-payload length before allocation or installation.
The palette section now has an exact typed model: two provenance words, one backdrop color, and
256 stored BGR555 colors. Its recovered one-entry circular rotation relative to TPL order is
explicit and reversible; `mwl-palette-tpl INPUT.mwl OUTPUT.tpl` exports the natural 256-color TPL
order without accidentally treating the backdrop as a palette entry. Empty and populated
ExAnimation sections likewise bridge to the canonical compact ROM representation with full
consumption checks. `MwlOptionalLevelAssets` transfers the typed palette and ExAnimation sections
as one failure-atomic unit while preserving every unrelated target section and container field.
`mwl-transfer-optional-assets SOURCE TARGET SIZE_MODES MAX_RECORDS OUTPUT` exposes that operation
as a create-new CLI workflow. Lunar Magic 3.63 accepted a Rust-generated combined file, installed
both optional subsystems, and re-exported the same semantic palette and ExAnimation data.
Allocator-dependent runtime tables can be resolved through
`ChainedSnesPointerLocator`, which follows the installed hook's 24-bit target and a checked signed
displacement to the final table operand rather than hard-coding one captured allocation address.
Revision profiles express these locators with paired
`exanimation.*_locator_operand_offset` and `exanimation.*_locator_displacement` keys; the recovered
primary Lunar Magic 3.63 layout uses displacement `-0x86`.
`mwl-observe` emits a canonical `LMOBS1` snapshot containing the proven container fields, exact
attribution bytes, section lengths and SHA-256 identities, plus the fixed level number and common
two-word payload metadata only where those recovered shapes validate. This permits differential
fixtures to detect semantic or opaque-byte changes without embedding copyrighted level payloads.
`mwl-observe-optional-assets INPUT SIZE_MODES MAX_RECORDS OUTPUT` instead emits a relocation-neutral,
field-addressable snapshot of all 257 palette colors and the decoded compact ExAnimation globals,
enabled triggers, record fields, lossless encoded-record identities, and every frame source word
under the supplied revision size-mode interpretation.

The application also owns `.mwl` files as revisioned portable documents. `mwl-open FILE`,
`mwl-edit-file SCRIPT`, `mwl-import-optional-assets-file SPEC`, `mwl-undo`, `mwl-redo`,
`mwl-status`, `mwl-save`, `mwl-close`, and
`mwl-discard` provide their full lifecycle. Its 100-state history restores complete canonical
containers, including every opaque section and attribution byte, while retaining an independent
saved baseline and invalidating divergent redo. Bounded `LMWLEDT1` scripts can change flags, the exact attribution bytes, the recovered
level number, or an explicitly named opaque section. Duplicate targets and malformed late commands
roll back the entire batch; successful changes must canonically encode and reopen before commit.
Bounded `LMMWLOPT1` import specifications bind a source MWL, exact 256-byte size-mode table, and
maximum animation-record count. The application decodes both typed optional sections before
creating one document revision, preserves every unrelated target section, and supports ordinary
undo, redo, save, dirty-close protection, and retry after failed imports.
`mwl-edit-optional-assets-file SPEC` provides the corresponding headless semantic editor.
`LMMWLOES1` binds an exact mode table, maximum-record limit, and bounded `LMMWLOE1` edit script.
Ordered commands cover both provenance pairs, palette colors, animation creation and globals,
triggers, exact lossless record insertion/replacement/removal, and size-mode-aware frame
insert/replace/remove/move operations. Frame commands carry one or two hexadecimal source words;
the shared engine resolves the selected record's revision mode and rejects a width mismatch. A
late invalid target or record-limit failure rolls back the whole batch without advancing history.
The standalone create-new equivalent is
`mwl-edit-optional-assets INPUT SIZE_MODES MAX_RECORDS EDITS OUTPUT`. It uses the same parser and
domain edit engine, preserves unrelated MWL sections, verifies semantic reopen, refuses existing or
aliased destinations, and publishes only after the entire ordered batch succeeds.
`mwl-edit-layer3-settings-file SPEC` is the corresponding revisioned application-shell workflow
for the expanded record. Its bounded `LMMWLL31` specification contains `enabled`, hexadecimal
`file`, `length-selector`, and `offset-selector` fields. The document controller applies them as
one typed change, preserving unrelated bits, words, and sections through canonical reopen,
undo/redo, recoverable save, and dirty-close protection.
A Wine oracle edited palette color `$100` to BGR555 `$1234` and trigger 3 to `$07` through this
Rust workflow, imported the result into Lunar Magic 3.63, and re-exported it. All 270 decoded
optional-asset fields initially matched exactly. The strengthened size-mode-aware observation now
compares all 273 fields, including the record's single-word frame source `$0600`; the retained
manifest also verifies the complete ROM
transition, 19 newly owned RATS allocations, and no unexpected byte ranges.
A separate frame-mutation oracle used `frame-replace 0 0 1234`, then imported and re-exported the
result with Lunar Magic under Wine. The exported frame source remained exactly `$1234`; its
complete ROM transition likewise verifies 19 owned allocations, no unexpected ranges, and zero
field-level semantic differences.
The native cross-platform MWL window exposes the same operation through a focused semantic import
panel. It reads the source and mode table together on the bounded document worker, retains its
maximum-record input, and delegates the atomic revision to `MwlDocumentController`; the GUI does
not decode palette, ExAnimation, or MWL wire formats itself. Users can also bind an exact mode
table to the current sections without importing another file. A separate typed panel then edits
both provenance words, all 257 native palette colors, ExAnimation provenance and globals, all
sixteen triggers, and the complete compact record list. Append, replace, remove, metadata, trigger,
and color changes each pass through the controller's record-limit-aware canonical replacement
boundary as one undoable revision. Both the native panel and scripted shell emit the shared
toolkit-neutral `MwlOptionalAssetsEdit` commands, preventing their mutation semantics from drifting.
The native panel also exposes targeted frame insertion, replacement, removal, and ordering through
those same commands instead of rebuilding a whole record in toolkit code.
The same window now owns a typed sprite panel. It can select legacy or expanded framing, edit the
stream header, insert/replace/delete/reorder raw records and expanded Y-position/control tokens, and
override any of the four 256-entry revision length-table entries for custom records. The panel
stages changes independently, then delegates one canonical, revision-checked replacement to
`MwlDocumentController`; malformed lengths, terminator collisions, stale revisions, and invalid
tokens do not alter document history. The reciprocal Wine oracle proves Lunar Magic 3.63 imports
the Rust-authored stream and re-exports the same semantics.
Immutable save snapshots, failed-save retry, newer-edit retention, and dirty quit/EOF protection
match the other portable editors.

## Build and verify

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

The workspace declares Rust 1.85 as its minimum supported version and forbids unsafe code.

## CLI examples

```sh
cargo run -p lm-cli -- inspect game.smc
cargo run -p lm-cli -- rats game.smc
cargo run -p lm-cli -- rats-manifest owned.lmrats normalized.lmrats ownership.obs
cargo run -p lm-cli -- rats-plan game.smc owned.lmrats 0xff
cargo run -p lm-cli -- rats-reclaim game.smc reclaimed.smc owned.lmrats 0xff
cargo run -p lm-cli -- mwl level.mwl
cargo run -p lm-cli -- mwl-normalize level.mwl normalized.mwl
cargo run -p lm-cli -- mwl-observe level.mwl level.obs
cargo run -p lm-cli -- mwl-palette-tpl level.mwl level.tpl
cargo run -p lm-cli -- level game.smc lorom 105 0x1000 0x2000 legacy
cargo run -p lm-cli -- level-export game.smc lorom 105 0x1000 0x2000 legacy standard level.lmlvl
cargo run -p lm-cli -- level-import game.smc edited.smc lorom 105 0x1000 0x2000 legacy standard level.lmlvl 0x7fdc 0x300000 0x400000
cargo run -p lm-cli -- level-import-owned game.smc edited.smc lorom 105 0x1000 0x2000 legacy standard level.lmlvl 0x7fdc 0x300000 0x400000 level.lmrats
cargo run -p lm-cli -- map16 game.smc lorom 10 0x3000 0x3300
cargo run -p lm-cli -- map16 game.smc lorom 10 0x3000 0x3300 page.obs
cargo run -p lm-cli -- map16-export game.smc lorom 10 0x3000 0x3300 page.map16
cargo run -p lm-cli -- map16-import game.smc edited.smc lorom 10 0x3000 0x3300 page.map16 0x7fdc 0x300000 0x400000
cargo run -p lm-cli -- map16-import-owned game.smc edited.smc lorom 10 0x3000 0x3300 page.map16 0x7fdc 0x300000 0x400000 page.lmrats
cargo run -p lm-cli -- graphics game.smc lorom 32 0x3600 0x8000 0x10000
cargo run -p lm-cli -- graphics-export game.smc lorom 32 0x3600 0x8000 0x10000 gfx.lmgfx
cargo run -p lm-cli -- graphics-import game.smc edited.smc lorom 32 0x3600 0x8000 0x10000 gfx.lmgfx 0x7fdc 0x300000 0x400000
cargo run -p lm-cli -- graphics-export game.smc lorom 32 0x3600 0x8000 0x10000 lz3 gfx.lmgfx
cargo run -p lm-cli -- graphics-import game.smc edited.smc lorom 32 0x3600 0x8000 0x10000 lz3 gfx.lmgfx 0x7fdc 0x300000 0x400000
cargo run -p lm-cli -- graphics-import-owned game.smc edited.smc lorom 32 0x3600 0x8000 0x10000 lz3 gfx.lmgfx 0x7fdc 0x300000 0x400000 graphics.lmrats
cargo run -p lm-cli -- graphics-recompress game.smc lz3.smc lorom 0x3600 0x100 0x8000 0x10000 lz2 lz3 0x7fdc 0x300000 0x400000
cargo run -p lm-cli -- palette game.smc lorom 5 0x3900 0x100
cargo run -p lm-cli -- palette game.smc lorom 5 0x3900 0x100 palette.obs
cargo run -p lm-cli -- palette-export game.smc lorom 5 0x3900 0x100 palette.lmpal
cargo run -p lm-cli -- palette-import game.smc edited.smc lorom 5 0x3900 0x100 palette.lmpal 0x7fdc 0x300000 0x400000
cargo run -p lm-cli -- palette-import-owned game.smc edited.smc lorom 5 0x3900 0x100 palette.lmpal 0x7fdc 0x300000 0x400000 palette.lmrats
cargo run -p lm-cli -- exanimation game.smc lorom 105 0x3c00 0x20 0x8000 modes.bin
cargo run -p lm-cli -- exanimation-export game.smc lorom 105 0x3c00 0x20 0x8000 modes.bin animation.lmexan
cargo run -p lm-cli -- exanimation-import game.smc edited.smc lorom 105 0x3c00 0x20 0x8000 modes.bin animation.lmexan 0x7fdc 0x300000 0x400000
cargo run -p lm-cli -- exanimation-import-owned game.smc edited.smc lorom 105 0x3c00 0x20 0x8000 modes.bin animation.lmexan 0x7fdc 0x300000 0x400000 animation.lmrats
cargo run -p lm-cli -- exanimation-frames animation.lmexan modes.bin 20 0 frame-edits.txt edited-animation.lmexan
cargo run -p lm-cli -- overworld-messages game.smc lorom 0 0x3f00 0x10 messages.obs
cargo run -p lm-cli -- overworld-sprites game.smc lorom 0 0x4200 0x10 9 sprites.obs
cargo run -p lm-cli -- overworld-export game.smc lorom 0 overworld.layout modes.bin world.lmow
cargo run -p lm-cli -- overworld-import game.smc edited.smc lorom 0 overworld.layout modes.bin world.lmow 0x7fdc 0x300000 0x400000
cargo run -p lm-cli -- overworld-import-owned game.smc edited.smc lorom 0 overworld.layout modes.bin world.lmow 0x7fdc 0x300000 0x400000 world.lmrats
cargo run -p lm-cli -- overworld-path world.lmowpath normalized.lmowpath paths.obs
cargo run -p lm-cli -- overworld-metadata world.lmowmeta normalized.lmowmeta metadata.obs
cargo run -p lm-cli -- level-bundle level.lmlevel normalized.lmlevel level.obs
cargo run -p lm-cli -- level-bundle-edit level.lmlevel auxiliary.lmedit edited.lmlevel
cargo run -p lm-cli -- native-level-file level.lmlvl sprite-lengths.bin normalized.lmlvl level.obs
cargo run -p lm-cli -- custom-object-library objects.mw0 objects.mw0t normalized.mw0 normalized.mw0t objects.obs
cargo run -p lm-cli -- custom-sprite-library sprites.mw2 sprites.mwt sprite-lengths.bin normalized.mw2 normalized.mwt sprites.obs
cargo run -p lm-cli -- native-map16-sidecar s16 sprites.s16 normalized.s16 sprites.obs
cargo run -p lm-cli -- layer3-file layer3.lmlayer3 normalized.lmlayer3 layer3.obs
cargo run -p lm-cli -- layer3-workspace-apply 0xc028 workspace.bin decoded-gfx.bin updated-workspace.bin workspace.obs
cargo run -p lm-cli -- graphics-remap-file remap.bin normalized-remap.bin remap.obs
cargo run -p lm-cli -- graphics-remap-apply remap.bin scratch-map.bin updated-map.bin apply.obs
cargo run -p lm-cli -- layer3-plane-file plane.lml3frame normalized.lml3frame plane.obs
cargo run -p lm-cli -- animation-frame-file frame.lmanfrm normalized.lmanfrm frame.obs
cargo run -p lm-cli -- appearance-file entities.lmentapp normalized.lmentapp entities.obs
cargo run -p lm-cli -- overworld-appearance-file sprites.lmowapp normalized.lmowapp sprites.obs
cargo run -p lm-cli -- graphics-file graphics.lmgfx normalized.lmgfx graphics.obs
cargo run -p lm-cli -- palette-file palette.lmpal normalized.lmpal palette.obs
cargo run -p lm-cli -- smw-palette-file SMW_Shared.smwpal normalized.smwpal shared-palette.obs
cargo run -p lm-cli -- tpl-palette-file palette.tpl normalized.tpl tpl-palette.obs
cargo run -p lm-cli -- raw-palette-file palette.bin normalized.bin raw-palette.obs
cargo run -p lm-cli -- palette-mask-file palette.palm normalized.palm palette-mask.obs
cargo run -p lm-cli -- rgb-palette-file palette.pal normalized.pal rgb-palette.obs
cargo run -p lm-cli -- map16-page-file page.map16 normalized.map16 page.obs
cargo run -p lm-cli -- exanimation-file animation.lmexan modes.bin 20 normalized.lmexan animation.obs
cargo run -p lm-cli -- overworld-file world.lmow modes.bin 20 normalized.lmow world.obs
cargo run -p lm-cli -- map16-set-file all.lm16set normalized.lm16set all-map16.obs
cargo run -p lm-cli -- quantize-rgb24 pixels.rgb 10 palette.lmpal pixels.idx
cargo run -p lm-cli -- import-indexed-map16 pixels.idx base.lmgfx base.occ 3 130 20 result.lmgfx result.occ result.map16
cargo run -p lm-cli -- import-rgb-map16 page.rgb base.lmpal palette.access base.lmgfx base.occ 2 130 20 result.lmpal result.lmgfx result.occ result.map16
cargo run -p lm-cli -- import-rgba-map16 page.rgba base.lmpal palette.access base.lmgfx base.occ 2 130 20 result.lmpal result.lmgfx result.occ result.map16
cargo run -p lm-cli -- import-png-map16 page.png base.lmpal palette.access base.lmgfx base.occ 2 130 20 result.lmpal result.lmgfx result.occ result.map16
cargo run -p lm-cli -- oracle-coverage fixtures version:3.40 operation:level-save argument:mapper=lorom argument:header=copier argument:fixture_family=ecosystem-modified
cargo run -p lm-cli -- render-map16-page graphics.lmgfx palette.lmpal page.map16 page.png
cargo run -p lm-cli -- render-graphics graphics.lmgfx palette.lmpal 0 10 graphics.png
cargo run -p lm-cli -- render-palette palette.lmpal 10 c palette.png
cargo run -p lm-cli -- render-level level.lmlevel all.lm16set graphics.lmgfx palette.lmpal 10 20 0 0 level.png
cargo run -p lm-cli -- render-level level.lmlevel all.lm16set graphics.lmgfx palette.lmpal entities.lmentapp 10 20 0 0 level-with-entities.png
cargo run -p lm-cli -- render-level level.lmlevel all.lm16set graphics.lmgfx palette.lmpal none layer3.lml3frame 10 20 0 0 level-with-layer3.png
cargo run -p lm-cli -- render-overworld world.lmow modes.bin 0x20 all.lm16set graphics.lmgfx 0 world.png
cargo run -p lm-cli -- render-overworld world.lmow modes.bin 0x20 all.lm16set graphics.lmgfx sprites.lmowapp 0 world-with-sprites.png
cargo run -p lm-cli -- render-overworld world.lmow modes.bin 0x20 all.lm16set graphics.lmgfx none tick.lmanim 0 world-at-tick.png
cargo run -p lm-cli -- address lorom snes-to-pc 0x058000
cargo run -p lm-cli -- codec lz2-decode input.bin output.bin
cargo run -p lm-cli -- codec lz3-decode input.bin output.bin
cargo run -p lm-cli -- codec lz3-encode raw.bin packed.bin
cargo run -p lm-cli -- codec-observe lz3 packed.bin 0x10000 packed.obs
cargo run -p lm-cli -- codec-observe rle-sized packed.rle 0x2000 packed-rle.obs
cargo run -p lm-cli -- planar decode 3 layer3.3bpp layer3.idx
cargo run -p lm-cli -- planar encode 3 layer3.idx layer3-roundtrip.3bpp
cargo run -p lm-cli -- codec rle-sized-encode raw.bin packed.bin
cargo run -p lm-cli -- codec rle-sized-decode packed.bin raw.bin 0x2000
cargo run -p lm-cli -- diff before.smc after.smc
cargo run -p lm-cli -- oracle-verify case.manifest before.smc after.smc
cargo run -p lm-cli -- oracle-verify case.manifest before.smc after.smc before.obs after.obs
cargo run -p lm-cli -- oracle-verify-suite oracle-fixtures
cargo run -p lm-cli -- oracle-release-gate oracle-fixtures version:3.63 operation:open-save operation:render-level operation:level-edit operation:lunar-magic-reopen operation:emulator-boot argument:mapper=lorom argument:header=headerless argument:region=us argument:revision=smw-us-v1 argument:rom_size=expanded argument:fixture_family=clean argument:subsystem=rom argument:subsystem=codecs argument:subsystem=rats argument:subsystem=levels argument:subsystem=map16 argument:subsystem=sprites argument:subsystem=graphics argument:subsystem=palettes argument:subsystem=exanimation argument:subsystem=overworld argument:subsystem=rendering argument:subsystem=application
cargo run -p lm-cli -- oracle-capture level-105-move 3.63 move-object before.smc after.smc before.obs after.obs changed-rats case.manifest level=105 object=7
cargo run -p lm-cli -- mwl-corpus oracle-work/lm363/pristine-us/levels
cargo run -p lm-cli -- mwl-transfer-optional-assets source.mwl target.mwl modes.bin 20 combined.mwl
cargo run -p lm-cli -- mwl-edit-optional-assets input.mwl modes.bin 20 edits.txt output.mwl
cargo run -p lm-cli -- mwl-observe-optional-assets output.mwl modes.bin 20 optional-assets.obs
cargo run -p lm-cli -- lm16-map16-file AllMap16.map16 normalized.map16
cargo run -p lm-cli -- rats-observe installed.smc installed-rats.obs
cargo run -p lm-cli -- profile smw-us.lmrev game.smc
cargo run -p lm-cli -- profile-export native-assets game.smc smw-us.lmrev 0x105 level-assets.lmna
cargo run -p lm-cli -- profile-import native-assets game.smc edited.smc smw-us.lmrev 0x105 level-assets.lmna 0x300000 0x400000
cargo run -p lm-cli -- native-assets-file level-assets.lmna smw-us.lmrev normalized.lmna level-assets.obs
cargo run -p lm-cli -- profile-export level game.smc smw-us.lmrev 0x105 level.lmlvl
cargo run -p lm-cli -- profile-export layer2 game.smc smw-us.lmrev 0x105 layer2.bin
cargo run -p lm-cli -- profile-export map16 game.smc smw-us.lmrev 0x00 page.lm16
cargo run -p lm-cli -- profile-import map16 game.smc edited.smc smw-us.lmrev 0x00 page.lm16 0x300000 0x400000
cargo run -p lm-cli -- patch input.smc output.smc 0x1234 deadbeef
cargo run -p lm-cli -- checksum-auto input.smc output.smc
cargo run -p lm-cli -- rom-expand input.smc expanded.smc lorom 0x100000 0xff
cargo run -p lm-app -- game.smc
cargo run -p lm-app -- --rom game.smc --profile smw-us.lmrev --ui-config frontend.lmuicfg --tools-config tools.lmtools --recent-state recent.lmrecent
cargo run -p lm-app -- --rom game.smc --profile smw-us.lmrev --script edit-session.lmscript
cargo run -p lm-app -- --rom game.smc --allow-in-place-rom-write
cargo run -p lm-app -- --help
cargo run -p lm-app
```

`codec-observe lz2|lz3|rle-terminated|rle-sized INPUT OUTPUT_BOUND OBSERVATION` produces a canonical `LMOBS1` semantic
snapshot for differential fixtures. It records codec identity, decoded length/hash, and the length
and hash of the deterministic canonical re-encoding. It deliberately omits the input stream's
physical hash, so distinct valid command sequences emitted by Lunar Magic and Rust compare equal
when they decode identically. Input and decoded sizes are bounded, termination must consume the
complete stream, canonical output is reopened before publication, and observations are create-new.
For LZ2, LZ3, and terminated RLE, `OUTPUT_BOUND` is a maximum; for sized RLE it is the exact decoded
length supplied by the containing format. This preserves the two RLE contracts instead of treating
an absent terminator as interchangeable with a terminated stream.

`planar decode BPP INPUT OUTPUT` converts complete SNES planar tile sequences into one row-major
byte per indexed pixel; `planar encode` performs the checked inverse. Depths 1 through 8 follow the
generic decoder recovered at `0x00455040`: plane pairs are interleaved and the last plane at odd
depths is stored as eight contiguous row bytes. Partial tiles, invalid depths, out-of-range pixels,
oversized conversions, aliases, and existing outputs are rejected.

The runnable shell disables replacement of the opened ROM by default. Use `save-as PATH` for the
normal create-new workflow. `save` and equivalent toolbar/shortcut actions require the explicit
`--allow-in-place-rom-write` startup capability and announce that elevated policy at startup; the
flag takes no value and duplicate occurrences are rejected.

The native GUI performs every ROM and standalone-document write on a persistence worker. Single
documents and the paired custom-object/custom-sprite sidecars share immutable controller snapshots;
the paired form remains one all-or-nothing filesystem transaction. Save controls reject overlap,
windows cannot close while their write is active, successful completion acknowledges only the exact
request, and failure releases that request without advancing the saved baseline.
Interactive ROM opening likewise reads, maps, identity-checks, and constructs the candidate project
on a bounded background worker. The worker returns an opaque `PreparedRomOpen`; the application's
exact pending-open token remains live until `complete_prepared_open` atomically installs it, while
any read or preparation failure cancels that token. Headless callers retain the synchronous
`complete_open` convenience path with identical validation semantics.
Bounded opens for palette, DSC, Layer 3, expanded settings, MWL, entity and overworld appearances,
overworld paths and metadata, native Map16 sidecars, graphics, Map16 pages and sets, ExAnimation,
native and complete level documents, native level-assets and complete-overworld aggregates, and
both paired custom sidecar editors likewise use a reusable background loader. Multi-file loads
retain request order and produce either the complete group or one error; editors reject a second
open and defer closing until the active load reports completion.
The shared reader rejects symlinks and non-regular paths, opens one checked file handle, and streams
at most the declared maximum plus one byte. A file that grows after metadata inspection is therefore
still rejected without an unbounded allocation, and exact-bound files remain valid.
Interactive frontend and external-tool configuration installation uses the same bounded worker
model. Decoding completes before the application invokes its validated whole-configuration setter,
so a malformed or failed load cannot partially replace localization, toolbar, shortcuts, or tools.
Interactive revision-profile installation is also a bounded background operation. UTF-8 and strict
`LMREVPRO1` validation complete before the existing identity-audited application command runs;
install and clear actions reject overlap, and successful replacement invalidates native rendering.
Interpretation-bound ExAnimation and aggregate loads validate their exact 256-byte size-mode and
1024-byte sprite-length tables before presenting revision-specific record-count configuration.
ROM graphics, palette, native level-assets, and complete-overworld ownership evidence is also loaded in the background
against an immutable profiled controller snapshot. A shared completion guard requires the captured
revision to still match the live project before ownership is decoded or a ROM-backed controller is
exposed, preventing a delayed file read from binding to a newer ROM state. The overworld workflow
additionally retains its validated hexadecimal profile slot across loading and restores the
configuration request after a read or decode failure.
Every ownership-backed reclamation commit—native level assets, Map16, graphics, palette,
ExAnimation, and overworld—uses a separate asynchronous `LMRATS01` loader. The loader binds the
manifest request to the current project revision, rejects stale completion before canonical
manifest decoding, and returns the evidence to the editor only for its domain-specific immutable
commit preparation.

The portable application shell exposes level, overworld, Map16, graphics, palette, ExAnimation,
and per-level Layer 3 editor modes. Layer 3 selections use distinct lossless tilemap-byte and
remap-byte clipboard domains, so unknown commands are never reinterpreted while crossing a native
platform clipboard. Its terminal frontend is intentionally minimal; native frontends consume
the same commands, effects, capability states, and deterministic render/project APIs. Editor
controllers commit serializer-produced `RomWrite` batches through `CommitRomWrites`; the shell
validates and applies the complete batch atomically as one undo entry, then emits `ProjectChanged`
so every frontend discards stale decoded/render caches. Each decoded controller result carries the
current project revision; stale asynchronous results are rejected before mutation. Undo and redo
advance the revision and emit the same invalidation. `ControllerSnapshot` captures the revision,
editor mode, detected ROM identity, document path, and immutable file bytes in one value for
background decode/render jobs. The shell exposes only read-only project access; mutation cannot
bypass the revision-checked command boundary.

After opening a ROM, installing its audited revision profile, and selecting a level, the terminal
frontend can exercise a real native edit with
`level-header FIELD VALUE SEARCH_START SEARCH_END`. Supported fields are `background-palette`,
`mode`, `background-color`, `sprite-palette`, and `foreground-palette`; numeric arguments are
hexadecimal. The explicit search range is converted into a bank-aware policy that protects all 16
profile tables and the complete 64-byte internal-header/vector block. The command decodes through
`LevelController`, stages a typed edit, allocates and repairs the checksum on a private image, then
dispatches the prepared mutation through the authoritative revision check. It is therefore
undoable and cannot bypass normal dirty-state/save handling.

For complete native level-controller batches, `level-edit SCRIPT SEARCH_START SEARCH_END` reads a
bounded UTF-8 `LMLEDIT1` script. It supports all five recovered header fields; object
insert/replace/remove/move plus typed command-ID, parameter, coordinate-nibble, and screen-advance
edits and exact packed screen-jump targets; sprite-header replacement;
and native sprite-token
insert/replace/remove/move for raw records, expanded screen changes, and expanded control tokens.
Indexes are decimal, field and token values are hexadecimal, and raw records use contiguous hex.
For example:

```text
LMLEDIT1
header mode 03
object replace 0 020001
object command 0 22
object parameter 0 7f
object coordinates 0 0e 0d
object screen-advance 0 true
object screen-jump-target 1 0a1b
sprite-header 10
sprite insert 1 screen 12
sprite insert 2 control 90
```

The whole script stages on a cloned controller. Both object and revision-sized sprite streams are
encoded and reparsed for exact equality before commit, closing the gap where universally bounded
raw records could still disagree with their native command/length tables. A late command,
noncanonical record, allocation failure, or stale revision leaves the application unchanged.
Typed object edits reject values that would change the recovered encoded record length or collide
with the stream terminator; callers must explicitly replace the whole raw record in those cases.
The coordinate pair remains orientation-neutral at the record boundary: absolute X/Y requires the
level layout and preceding screen-transition stream, so it is not guessed from an isolated record.
Command-zero records with parameter `01` or `03` are classified as Lunar Magic's two recovered
screen-jump encodings and expose their exact packed target without assigning it an unsupported axis.
The same table-aware serializer is shared by controller validation and ROM persistence; exhaustive
tests cover all four selectors and all 256 sprite IDs.

`level-edit-owned SCRIPT SEARCH_START SEARCH_END OWNERSHIP_MANIFEST` adds safe relocation for
previously tagged level data. The controller captures the exact RATS descriptors for both the
object and sprite streams from the immutable decode snapshot, requires both descriptors to match
the `LMRATS01` ownership evidence, then allocates, repoints, reclaims, repairs the checksum, and
records history as one transaction. Undo restores both original tagged blocks and their pointers.

`map16-edit SCRIPT SEARCH_START SEARCH_END` provides the corresponding complete-workspace native
Map16 path through bounded UTF-8 `LMM16ED1` scripts. All numeric values are hexadecimal. Commands
replace a complete tile, one `tl`/`tr`/`bl`/`br` subtile, or an Acts Like target; each command also
declares its graph-resolution limit. For example:

```text
LMM16ED1
tile 01 02 0001 0002 0003 0004 0000 10000
subtile 01 02 br 0005 10000
acts-like 01 02 0000 10000
```

The controller validates every command against the complete parallel graphics/Acts Like workspace,
serializes every declared page through one transactional allocation, repairs the checksum, and
reloads the native set in tests. A late dangling link or cycle rolls back preceding commands in the
same script and never advances the application revision.

`map16-edit-owned SCRIPT SEARCH_START SEARCH_END OWNERSHIP_MANIFEST` extends that complete-set path
with exact allocation ownership. Decode captures the RATS descriptor for every graphics and Acts
Like page plane from one immutable snapshot. Commit requires those descriptors in the `LMRATS01`
manifest, then allocates, repoints, reclaims displaced planes, and repairs the checksum in one
history entry. Unchanged planes may be retained through exact tagged-payload reuse; undo restores
every displaced block and pointer.

`palette-edit SCRIPT SEARCH_START SEARCH_END` adds ownership-aware exact-word editing through
bounded UTF-8 `LMPALED1` scripts. A script must declare the complete palette shape and default owner,
then may mark individual entries `fixed` or `exanimation`-owned before any edits. It can apply a
unique change batch or replace a contiguous range while preserving the full raw BGR555 word,
including bit 15:

```text
LMPALED1
owners 100 editable
owner 00 fixed
owner 10 exanimation 0002
changes 01 1234 02 9234
range 03 8001 7fff
```

Ownership is never inferred from palette contents. The declared shape must exactly match the
profile-decoded palette, protected colors reject the complete staged script, and successful commits
remain checksum-repaired, natively reloadable, revision-checked, and undoable.

`palette-edit-owned SCRIPT SEARCH_START SEARCH_END OWNERSHIP_MANIFEST` adds explicit allocation
ownership to that application workflow. The controller captures the exact tagged palette block
from its immutable snapshot and overrides any caller previous-block option with it. The bounded
`LMRATS01` proof, allocation, pointer rewrite, displaced-block erasure, checksum repair, and
application revision commit are all-or-nothing; undo restores the complete original tag and
payload.

`graphics-edit SCRIPT SEARCH_START SEARCH_END` uses bounded `LMGFXED1` scripts for compressed native
4bpp files. Like palettes, a script declares the complete tile ownership shape and any fixed or
ExAnimation-owned overrides before editing. Each tile is written as exactly 64 hexadecimal pixel
nibbles in row-major 8×8 order; `changes` performs a unique indexed batch and `range` replaces
contiguous tiles:

```text
LMGFXED1
owners 3 editable
owner 0 fixed
owner 2 exanimation 0007
changes 1 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
range 1 bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
```

The controller enforces ownership and 4bpp bounds, recompresses deterministically, allocates through
the protected profile policy, repairs the checksum, and reloads the native file. A protected tile,
duplicate target, malformed pixel row, ownership mismatch, or late failure rolls back the complete
script.

`graphics-edit-owned SCRIPT SEARCH_START SEARCH_END OWNERSHIP_MANIFEST` is the allocation-owning
application-shell variant. The controller retains the exact tagged block descriptor decoded from
its immutable ROM snapshot and uses that descriptor as the previous block regardless of caller
save options. The bounded `LMRATS01` manifest must make exactly that block reclaimable. Compression,
allocation, repointing, old-block erasure, checksum repair, and application history publication
then form one revision-checked undoable commit; an untagged, stale, foreign, retained, or
overlapping claim leaves the application unchanged.

`graphics-recompress lz2|lz3 SEARCH_START SEARCH_END` exposes complete-table compression migration
through the cross-platform application shell. It derives the source codec, pointer layout,
checksum address, and protected ranges from the installed identity-qualified revision profile.
Every source slot is decoded before allocation; all target streams, pointers, and checksum bytes
then commit as one application revision and one undo entry only after semantic reopen succeeds.
Equal-codec requests are no-ops, while stale revisions or late allocation failures preserve ROM
bytes, history, and application revision. The installed profile's in-memory effective codec advances
with the migration and follows that exact history entry through undo and redo, so subsequent
graphics controllers never reopen the migrated ROM with stale compression metadata; the external
profile artifact itself remains unchanged.

`exanimation-edit SCRIPT SEARCH_START SEARCH_END` exposes the compact native animation collection
through bounded `LMEXAED1` scripts. Numeric values are hexadecimal. Records declare their transfer
kind, revision size-mode index, destination, destination flag, explicit `single`/`double` frame
width, and source words; frame commands can then insert, replace, remove, or reorder frames:

```text
LMEXAED1
setting 05
header deadbeef
trigger 00 clear
trigger 02 aa
record insert 1 02 00 2345 0 single 4444
frame replace 0 1 2222
frame insert 0 2 3333
record move 1 0
```

The controller checks the explicit frame width against the profile's recovered 256-entry size-mode
table, preserves untouched bytes that the compact format represents, clears stale source words when
a frame list shrinks, serializes the variable-offset compact payload, allocates it transactionally,
and repairs the checksum. It rejects disabled-trigger values and workspace-record bytes that the
compact format would silently discard. Collection and frame commands stage together, so an invalid
late index, width, record limit, allocation, or stale revision leaves the ROM unchanged.

`exanimation-edit-owned SCRIPT SEARCH_START SEARCH_END OWNERSHIP_MANIFEST` binds that semantic
workflow to the exact tagged variable-length block captured by the immutable controller snapshot.
The recovered size-mode table remains part of decoding and re-encoding, while the bounded
`LMRATS01` proof authorizes only allocation reclamation. Repointing, displaced-block erasure,
checksum repair, revision publication, and undo are one atomic application operation.

`overworld-edit SCRIPT SEARCH_START SEARCH_END` applies a bounded `LMOWEDT1` transaction across all
nine modeled native overworld payloads. The script declares the native pointer-table slot and exact
palette ownership shape before edits. It can change both tile layers, event reveals, endpoints,
message tiles, fixed-shape sprites, palette words, and nested `ExAnimation` commands:

```text
LMOWEDT1
slot 0
palette-owners 100 editable
palette-owner 02 fixed
layer 2 01 02 1234
event 00 0003 0004
endpoint 00 0005 0006 02
message 00 01 02 44
sprite 00 0007 0008 0009 06 ccdd
palette 03 9234
animation trigger 04 aa
animation frame replace 00 00 2222
```

Sprite extension bytes must exactly match the revision record shape. Every domain is decoded from
the same immutable snapshot, staged on one aggregate clone, serialized into one nine-payload RATS
transaction, checksum-repaired, and committed as one undo entry. Any late cross-domain validation,
allocation, mapper, or revision failure preserves the original ROM.

`overworld-edit-owned SCRIPT SEARCH_START SEARCH_END OWNERSHIP_MANIFEST` adds exact reclamation to
the same nine-domain operation. The controller captures both layers, both event planes, endpoints,
messages, sprites, palette, and ExAnimation RATS descriptors from its immutable decode snapshot.
Only descriptors proven by the `LMRATS01` manifest may be displaced; allocation, all nine pointer
updates, reclamation, checksum repair, and history publication remain one transaction. Undo restores
every reclaimed payload and pointer.

Layer 3 remains split at a narrower evidence boundary. Lunar Magic 3.63's installed 32-byte
expanded-level record uses word 0 bit `$2000` to enable the custom tilemap path and word 1 as a
packed graphics descriptor: a 12-bit GFX/ExGFX file, two-bit requested-length selector, and two-bit
destination-offset selector. `Layer3TilemapGraphicsDescriptor` exposes those fields without
canonicalizing selector aliases; setters preserve every unrelated bit and all other record words.
`Layer3TilemapWorkspace` independently implements the recovered exact `$2000`-byte decoded range
operation for all sixteen selector pairs: it clips at the workspace boundary, preserves bytes
outside the selected range, rejects short decoded inputs without mutation, and extracts the same
selected range for save-time encoding. `layer3-workspace-apply PACKED WORKSPACE DECODED_GFX OUTPUT
[OBSERVATION]` exposes this component through bounded, create-new batch I/O and emits exact
descriptor fields plus before/source/selected/after SHA-256 digests for differential fixtures.
The legacy path's generic graphics-remap stream is separately modeled by
`GraphicsRemapCommandStream`, based on the recovered `DecodeGraphicsRemapCommandStream` instruction
sequence. Four-byte headers encode a 15-bit destination word, a 14-bit length field, literal versus
repeated payloads, and one-word versus 32-word stride. The interpreter preserves the exact
`$8000`-word wrap, odd literal/repeat byte behavior, noncanonical terminator header, and the
`$8000`-byte command-consumption stop condition. `graphics-remap-file` provides canonical
prefix/observation output, while `graphics-remap-apply` applies a stream to an exact little-endian
scratch map and records before/after digests. Ownership remains intentionally generic because the
recovered Lunar Magic helper parses into private scratch storage while restoring its caller buffer.
The standalone `expanded-settings-layer3 INPUT on|off FILE LENGTH_SELECTOR OFFSET_SELECTOR OUTPUT`
and MWL `mwl-edit-layer3-settings ...` commands use that shared type, while
`mwl-observe-layer3-settings` emits relocation-neutral field observations. The native expanded
settings window presents the same semantic controls above its lossless raw-word view. Direct
clean-ROM installation now has an identity-bound, clean-room SMW US revision-0 implementation:
the runtime and expanded-settings table are generated as six independently constrained RATS
allocations, 78 guarded writes, checksum repair, and one atomic undo step. Other ROM revisions still
require independently verified runtime templates and guarded-write maps.
`LMLAY3V1` continues to provide the portable state, editing, clipboard, oracle, and rendering
contracts.
Already-installed expanded settings tables now have a narrower native boundary: callers may provide
an explicit mapper, table offset, entry count, and stride to load or atomically save an exact
32-byte per-level record with checksum repair. All sixteen words remain lossless. The generic table
API does not infer a ROM revision. The revision-specific `layer3-install` workflow performs identity
checking and installs the SMW US revision-0 runtime and expanded-settings table.
The grouped native level-assets save can include this direct record alongside the object, sprite,
palette, and ExAnimation RATS payloads. Allocation, pointer publication, the protected table write,
checksum repair, and undo-history publication then form one transaction.
Its symmetric aggregate loader returns one coherent owned snapshot, and revision profiles build the
matching layout plus profile-wide copy-on-write save options. The CLI and future graphical
frontends consequently share the same protection authority instead of duplicating pointer-table
ranges.
`expanded-settings-export ROM MAPPER SLOT TABLE_OFFSET ENTRIES STRIDE OUTPUT` extracts that exact
record. `expanded-settings-import INPUT_ROM OUTPUT_ROM MAPPER SLOT TABLE_OFFSET ENTRIES STRIDE
RECORD CHECKSUM_FIELD` writes it to a new ROM, repairs the checksum in the same transaction, and
requires an exact semantic reopen before publication.

The application shell can also edit an exported exact record without opening a ROM:
`expanded-settings-open FILE`, `expanded-settings-edit-file SCRIPT`, `expanded-settings-undo`,
`expanded-settings-redo`, `expanded-settings-status`, `expanded-settings-save`,
`expanded-settings-close`, and `expanded-settings-discard`. Its bounded exact-record history uses
monotonic revision tokens, preserves the saved baseline, and invalidates divergent redo. A bounded
script uses the `LMXSETED1` header followed by `word INDEX VALUE` commands; indexes and values are
hexadecimal, and duplicate indexes are rejected atomically. This workflow deliberately preserves
unknown words and does not claim to discover or install Lunar Magic runtime patches.
The installed-record type and the complete-level `ExpandedLevelHeader` have explicit lossless
conversions over the same sixteen-word shape, allowing portable and native workflows to exchange
the recovered record without assigning meanings or normalizing unknown values.

`LMREVPRO1` profiles may carry that installed table as `expanded_settings.offset`,
`expanded_settings.entries`, and `expanded_settings.stride`; the all-or-none extension preserves
compatibility with profiles created before this domain was recovered. Validation requires an exact-record
stride, mapper-addressable final record, and no overlap with any declared pointer table; ROM audit
also rejects intersection with the internal header or an out-of-image table. Profile allocation
policies protect the complete direct table. Consequently `profile-export expanded-settings` and
`profile-import expanded-settings` use the same identity-bound layout, checksum repair, semantic
reopen, and create-new publication policy as the other native domains.

With a qualified profile installed and a level selected, the application command
`expanded-settings-word INDEX VALUE` edits one of the sixteen hexadecimal native words directly in
the open ROM. The profile-driven controller decodes from an immutable application snapshot,
preserves every other byte, repairs the checksum in the same transaction, and commits through the
normal revision/undo/redo boundary. Profiles without the optional table capability fail explicitly.
`expanded-settings-edit SCRIPT` applies the same bounded `LMXSETED1` format used by standalone
documents as one duplicate-free native transaction; a bad late command leaves every word and the
application history unchanged.

`expanded-settings-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]` provides the corresponding
oracle boundary. It requires exactly 32 input bytes and atomically publishes an exact normalized
record together with a canonical observation containing the raw bytes and all sixteen indexed
little-endian words. Output aliases and pre-existing destinations cannot leave a partial evidence
pair.

The runnable application can edit `LMLAY3V1` directly with `layer3-open FILE`,
`layer3-edit-file SCRIPT`, `layer3-undo`, `layer3-redo`, `layer3-status`, `layer3-save`,
`layer3-close`, and `layer3-discard`. Its 100-state canonical history uses monotonic revision tokens,
retains the independent saved baseline, and clears redo after a divergent edit.
Bounded `LML3EDT1` scripts cover every proven standalone field, including all reserved bytes and
opaque remap commands:

```text
LML3EDT1
start fe
size 03
liquid 81
flags a5
graphics 2 abc
reserved 5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a
tilemap 00010203
tilemap-range 1 aabb
remap fe0708
```

`-` represents an empty tilemap or remap buffer. Whole-buffer replacement can change the bounded
workspace length, while range edits must remain inside the staged buffer. The revisioned document
controller refuses enable/disable transitions, since a standalone artifact is always present, and
keeps failed edits or persistence attempts atomic and retryable. Layer 3, overworld path,
overworld metadata, and paired custom-object document saves each carry a monotonic request ID
separate from the edit revision; delayed acknowledgement or cancellation from an earlier save
cannot consume a newer snapshot taken at the same document revision.

Controllers whose serializer may allocate beyond the current image use `RomMutation::between` and
`CommitRomMutation`. The prepared result carries its exact source logical length, compact disjoint
changed runs, mapper identity, and the appended tail. The shell checks project revision, logical
length, mapper agreement, complete 32-KiB bank alignment, and mapper addressability before committing
growth and writes as one history entry; undo shrinks exactly and redo restores the tail. Ordinary
fixed-size edits can continue using the smaller `CommitRomWrites` path.

`LevelController` is the first complete decoded-controller implementation over that boundary. It
requires a revision layout and sprite-length table, decodes only the level selected by its immutable
snapshot, stages ordered legacy-header/object/native-sprite edits, and runs the project allocator and
serializer against a private ROM copy. Preparation repairs the SNES checksum and returns a
`PreparedRomCommit`; dispatch still performs the authoritative revision and length checks. Mapper,
mode, edit, allocation, and stale-result failures cannot change either controller or application.

`Map16Controller` applies the same contract to the complete pair of native graphics and Acts Like
planes. It loads every page declared by parallel revision tables, stages graph-validated tile,
subtile, and Acts Like edits, serializes all page pairs in one allocator transaction, repairs the
checksum, and produces a prepared commit. Native reload equality covers the entire set rather than
only the edited page; wrong modes/mappers/table shapes, invalid graphs, late commands, and stale
results remain failure-atomic.

`GraphicsController` loads the native compressed file selected by editor mode and requires an exact
per-tile ownership map before exposing it. Ordered tile batches reject fixed or ExAnimation-owned
targets, duplicate indexes, invalid 4bpp pixels, and malformed ranges on a staged clone. Commit
preparation performs the revision profile's deterministic LZ2 or LZ3 encoding, bounded allocation/repointing, checksum repair,
and native decompression/reload verification through the shared prepared-mutation boundary.

`PaletteController` provides the corresponding exact-word path for native palettes. It validates
the nonzero revision-declared color count and a complete ownership map, stages individual or
contiguous color changes without masking preserved bit 15, and rejects fixed or
ExAnimation-owned entries. Native loading requires both tagged and untagged payloads to match the
revision's exact byte shape; a valid RATS header cannot conceal a short or oversized palette.
Prepared commits allocate/repoint the native palette, repair the checksum, and prove exact BGR555
reload equality through the same revision-safe application transaction.

`ExAnimationController` loads the compact slot selected by editor mode only with an exact recovered
256-entry size-mode table. It stages setting/header/trigger and record insert, replace, remove, and
reorder operations on a clone, preserving every field represented by the compact native encoding.
Each edit revalidates canonical compact representability by decoding its staged encoding, rejecting
trailing inactive records or revision-limit overflow before changing controller state. Shared
project I/O applies the same exact 256-mode and encode/decode identity checks, and tagged loads must
consume their complete RATS payload rather than hiding trailing bytes. Preparation validates
record/encoded limits, allocates and repoints the payload, repairs the checksum, and proves native
compact reload equality without guessing undocumented transfer sizes.

`OverworldController` spans the complete nine-payload native transaction: both layers, both event
reveal planes, endpoints, fixed-shape messages, revision-shaped sprites, exact palette, and compact
ExAnimation. Because revision layouts declare fixed collection counts, it exposes bounded field and
replacement operations rather than unserializable resizing. Mixed-domain edits stage on one clone;
preparation validates every shape, allocates all nine payloads together, repairs the checksum, and
proves complete native reload equality before the application accepts one revision-bound commit.

Every decoded controller retains an immutable semantic baseline from its snapshot. Preparing an
untouched model—or one whose edits were fully reverted—returns `RomMutation::unchanged` before any
serializer or allocator runs. Dispatching that mutation creates no relocation, checksum write,
dirty range, history entry, revision increment, or frontend cache invalidation. This prevents a
copy-on-write save policy from turning a semantic no-op into incidental ROM churn.

`RevisionProfile` gathers the layouts required by every controller into one canonical `LMREVPRO1`
document. The strict parser rejects unknown, duplicate, and missing keys; malformed 1,024-byte
sprite-length and 256-bit ExAnimation-mode tables; inconsistent mappers and overworld shapes;
unsafe pointer strides; arithmetic overflow; and tables outside the mapper address space. No ROM
offset defaults are built in: profiles are audited, external compatibility data derived from legal
fixtures, and parsing one never modifies a project. Profiles are bound to detected game, region,
revision, and mapper identity. Profile-driven factories cover every decoded controller and reject
identity or profile-validation failures before dereferencing any revision-provided ROM address.
The application installs and clears profiles through commands that advance its shared background
revision without dirtying ROM bytes or creating undo history. `ProfiledControllerSnapshot` captures
the ROM and profile together; replacing either invalidates older controller results. The terminal
frontend exposes `profile PATH` and `profile-clear`, while native frontends receive a typed
`RevisionProfileChanged` cache-invalidation effect. Installation runs the same exhaustive pointer
audit as CLI qualification before exposing `ProfileStatus::Loaded`; failure preserves the previous
profile, revision, selection, ROM bytes, dirty state, and history.

The profile model lives in the frontend-neutral `lm-profile` crate rather than `lm-app`, avoiding a
CLI-to-UI dependency. `lm-cli profile PROFILE [ROM]` strictly parses and summarizes a profile and,
when a ROM is supplied, performs a non-mutating audit of every entry in all required pointer tables
plus the optional native Layer 2 table and,
when declared, the complete direct expanded-settings table span. It rejects incompatible identity, unreadable
table bytes, invalid mapped addresses, targets beyond the logical image, or metadata overlap, then
reports per-pointer-table entry counts, unique targets, and target ranges.
When the optional direct table is present, the qualification report also records its exact offset,
entry count, stride, and half-open byte span; legacy profiles report its absence explicitly in the
typed API rather than fabricating a layout.
`profile-export DOMAIN ROM PROFILE SLOT OUTPUT` uses the same validated
profile to export `native-assets`, `level`, `layer2`, `map16`, `graphics`, `palette`, `exanimation`,
`expanded-settings`, or `overworld` native
data without repeating raw offsets, mapper names, sprite-length tables, or animation-mode files.
Exports refuse to alias either input and publish through the create-new atomic output path. Both
terminal and native application frontends consume the same parser.
The text codec is internally separated into a shared key schema, strict decoder, and canonical
encoder, keeping the externally stable `LMREVPRO1` format from drifting between directions while
avoiding a monolithic profile source file.
Revision-owned clean-room runtime code uses a separate canonical `LMPAT001` binary template. The
template binds game, region, revision, and mapper; carries bounded tagged payload bodies, exact hook
preconditions/replacements, and address-independent cross-payload fixups; and contains no allocator
addresses. `revision-patch-install INPUT_ROM OUTPUT_ROM PROFILE TEMPLATE SEARCH_START SEARCH_END
FILL` qualifies both identities, audits the profile, constructs its complete metadata-protection
policy, allocates and fixes up every payload, verifies all reopened RATS tags, repairs the checksum,
and atomically publishes a new ROM. Search ranges may request mapper-valid growth. Existing,
aliased, malformed, wrong-revision, or partially applicable outputs are never published.
`revision-patch-file INPUT [NORMALIZED_OUTPUT [OBSERVATION]]` provides the corresponding
create-new inspection boundary. It strictly decodes and canonically re-encodes `LMPAT001`, then
optionally emits relocation-neutral semantic observations for identity, payload and hook counts,
body lengths and SHA-256 digests, exact hook offsets and precondition/replacement digests, and every
cross-payload fixup. This allows independently authored clean-room runtimes to be compared in
automation without treating allocator-selected addresses—or Lunar Magic's emitted executable
bytes—as source material.
Runtime authoring now begins in `lm-snes` rather than opaque byte arrays. The SMW US revision-0
Layer 3 contract has independently generated fragments for the verified vanilla mode-dispatch
fallback at logical `$00201F`, the conditional status dispatch at `$002153`, and the complete
mode-value initialization hook at `$0094B6`, plus the `$1403`/custom-mode level dispatcher at
`$02C40C`. The status fragment includes its stacked-return adjustment and the level dispatcher
includes both absolute redirects and its direct-return arm. A composed runtime bundle rebases all
four hook targets for clean-room instruction-level tests. The complete installer separately covers
the full `$4C0` source payload, its 75 allocation-relative relocations, and all four external entry
points.
The first-hook implementation also has a longer generated setup fragment. It reproduces the
verified custom-mode dispatch and all state initialization through the first table-driven helper
boundary, matching `$6A` bytes in the retained Wine fixture.
The following horizontal and vertical dispatch tables are exposed as typed
`Layer3ScrollFormula` selectors. Their 32 entries distinguish fixed, full-speed, divided-speed,
dynamic, and SNES/SA-1-backed divide-by-five calculations, and fixture tests validate every semantic entry
against the installed runtime rather than exporting its relocated routine addresses.
Typed horizontal and vertical dynamic-state transitions cover the remaining formulas, including
fixed-point phase accumulation, signed engine offsets, reset-bit behavior, cross-axis dispatch,
camera clamping, and the original helper's double-step/restore path. Fields without a proven engine
identity retain their SMW RAM addresses in the public contract.
`Layer3ScrollHelperLibrary` lowers every ordinary formula into independently generated 65C816 code
with direct local entry offsets. Checked long branches and relocatable local call/jump labels avoid
copying the original bank-relative pointer tables; the generated divide-by-five helper includes
both SNES and SA-1 arithmetic-register paths.
The horizontal accumulator and both vertical dynamic routines are now generated too, including
reset, cross-axis, Layer-2 scratch, camera clamp, and double-step restoration paths. Consequently
all 64 selector entries are local generated targets; their internal calls and jumps use validated
payload-relative relocations.
The generated selector continuation now supplies normalization, phase-constant override rules,
horizontal/vertical calls, camera snapshots, phase initialization, `$13D5` status behavior, and the
width-sensitive return to setup offset `$06`. Setup, continuation, and helpers are flattened into a
self-contained first-hook payload. The complete runtime installation combines the `$4C0` main
patch, `$3D0` main runtime, `$20+$20` compatibility pair, and `$370` extended runtime.
The `$6E00` expanded-settings allocation itself is now modeled exactly as a `$2D00`-byte `$FF`
prefix plus 520 lossless 32-byte records. Its canonical initializer, current-layout normalization,
standard/special record split, offset calculations, validation, and byte-exact Wine-fixture
round-trip are exposed by `SmwUsV1ExpandedSettingsAllocation`. Its generated runtime blocks,
revision hooks, pointer publications, typed allocation fixups, checksum repair, and failure-atomic
installation are complete. `layer3-install` groups that separately aligned plan with the five
runtime allocations, preserving the `$087FF8` RATS header / `$088000` payload boundary while
committing all six allocations as one undoable project operation.
The cross-platform GUI exposes three recovered installation paths through a revision-bound
**Install Built-in Runtime** dialog. It can install the expanded-settings family alone, the
complete Layer 3 family (which includes expanded settings), or the expanded shared/custom-palette
runtime with its 512-entry per-level pointer table. The dialog displays the exact supported
identity, rejects a stale project revision, and closes only after application acceptance. Native
tests route every selection through the same application commands, semantically reopen the
installed subsystem, and prove exact input restoration with one undo.
Complete Layer 3 installation also detects an already-valid expanded-settings allocation. In that
state it reuses the prerequisite and installs only the five missing Layer 3 allocations, avoiding
the guarded-hook collision that would otherwise follow selecting expanded settings first. The
settings-only snapshot and pristine source remain separately reachable through two exact undo
steps.
The twelve copied runtime blocks now also have a byte-level oracle that permits differences only in
recovered relocation/configuration spans. No Lunar Magic payload bytes are embedded in the Rust
sources. Relocation-free descriptor blocks `$213` and `$219` are independently emitted from
semantic 65C816 builder calls. Address-dependent blocks `$173` and `$216` are generated too and
match both placeholder and Wine-resolved oracle forms, bringing independent coverage to four of
twelve copied blocks. Descriptor `$72` is now independently generated as well, including its
primary reset loop and secondary flag-gated helper entry, raising coverage to five of twelve.
The `$69` selector dispatcher, `$19F` indexed scratch helper, and `$172` record selector are also
independently generated and differentially verified, bringing coverage to eight of twelve blocks.
The `$1DB` pointer-domain dispatcher and configurable `$215` DMA runtime are now generated as well.
Both embedded and installed `$215` continuation modes are verified. Descriptor `$220` is generated
from five focused semantic emitters and exact fixed tables, and descriptor `$21C` is generated by
the separate transfer-runtime builder. All twelve copied blocks therefore have independent
semantic emitters and embedded/installed differential checks.
The cross-platform application reaches the same boundary through
`revision-patch-install-file SPEC`. Its bounded `LMRPINS1` specification supplies a template path
relative to the specification plus hexadecimal `search-start`, `search-end`, and `fill` fields.
The application command binds the template to the current profile and project revision, rejects
pending saves and stale background results, and exposes the entire installation as one ordinary
undo/redo/save revision.
The native **Profile → Install Revision Patch** workflow now provides the same boundary without
blocking the render loop. It bounded-reads one canonical template on the document worker, rejects
wrong-profile templates before opening an installer, displays identity and payload/write counts,
and accepts an explicit hexadecimal search range and expansion fill. Its workspace is bound to the
revision captured before file loading; a ROM or profile change makes it stale, and only successful
application dispatch closes it.
The required `graphics.compression=lz2|lz3` field binds native graphics decoding and saving to the
ROM revision. Profile-driven CLI and application controllers use it automatically; direct
`graphics`, `graphics-export`, and `graphics-import` commands accept the same token before their
optional observation or output path, while the legacy tokenless forms remain LZ2-compatible.
Profiles omit `graphics.pointer_encoding` for the legacy contiguous three-byte table. Recovered
parallel-byte layouts use `graphics.pointer_encoding=split_planes` together with
`graphics.pointer_high_offset` and `graphics.pointer_bank_offset`; the ordinary `graphics` table
continues to declare the low-plane offset, entry count, and stride. Parsing requires the complete
form, validation rejects inconsistent programmatic layouts and overlapping components, ROM audit
reassembles every 24-bit pointer from the three physical planes, and allocation policy protects
each plane independently.
`graphics-recompress` migrates the complete declared pointer table in one copy-on-write operation.
It decodes every source slot before mutation, derives previous tagged ownership, protects the full
pointer table and checksum field, stages every allocation/repoint, repairs the checksum, and
reopens all target streams before publishing. The project API applies the resulting image as one
undoable mutation; allocation or semantic failure leaves ROM bytes and history unchanged.

Profile text is treated as untrusted configuration. The shared bounded reader consumes at most one
byte beyond the 16-KiB file limit, requires UTF-8, and caps line count, line length, and profile-name
length before cloning field values. Pointer-table entry counts are capped before an audit allocates
or iterates. Direct in-memory parsing enforces the same limits, so the CLI
and application shell cannot accidentally bypass them with different file-loading behavior.

`profile-import DOMAIN INPUT_ROM OUTPUT_ROM PROFILE SLOT ASSET SEARCH_START SEARCH_END` supports
the same eight domains as profile export. It derives the checksum field from the detected ROM,
protects each complete pointer table using the profile's actual entry count and stride, allocates
transactionally, protects every profile table plus the complete 64-byte SNES internal
header/vector block, repairs the checksum, semantically reopens the result, and only then publishes a
new output ROM. Complete overworld import protects and commits all nine payload tables together.
Every profile import protects all 16 pointer tables and the direct expanded-settings table,
including metadata owned by other editor domains,
so allocation for one asset cannot consume metadata needed by another editor. Profiles with any
overlapping table-byte spans are rejected before audit or mutation.
The older raw-offset import commands remain available for fixture diagnostics.

`native-assets` uses the bounded `LMNATAS1` aggregate. It embeds canonical `LMLVL1`, `LMPAL1`, and
`LMEXAN1` sections plus the optional exact 32-byte expanded-settings record, requires every nested
source identity to agree, and preserves revision-dependent sprite and animation interpretation.
Import validates profile shape, commits all four tagged payloads and the optional direct record in
one checksum-inclusive transaction, then reloads and compares the complete aggregate before
create-new publication.
The application library exposes the same boundary through `NativeLevelAssetsController`. A
profile-qualified level snapshot can stage ordered object/sprite, ownership-checked palette,
ExAnimation, and exact settings-word edits on one aggregate clone. Late cross-domain failure rolls
back the complete batch, and commit preparation emits one revision-bound ROM mutation using the
same grouped serializer as CLI import.
When the active revision profile declares a native Layer 2 pointer table, the same controller now
loads that selected level's mode-dependent Layer 2 representation as a fifth payload. Object modes
support ordered lossless object insertion, replacement, and removal; tilemap modes expose all
1,024 little-endian words. Layer 2 participates in late-failure rollback, semantic reopen,
checksum repair, copy-on-write allocation, and exact five-payload `LMRATS01` reclamation.
`NativeLevelAssetsDocumentController` owns a portable `LMNATAS1` file without a ROM. It binds the
sprite-length and ExAnimation mode tables for the document lifetime, reuses the identical staged
cross-domain edit engine, canonically reopens each accepted revision, and exposes immutable,
request-correlated save snapshots so an older completed write cannot hide newer edits.
The native application also opens standalone `LMLVL1` native-level stream documents from the
Documents menu. Opening requires the exact 1,024-byte sprite-length table that defines record
boundaries for that revision. The editor is split into controller-facing and form modules and
supports lossless object-record and sprite-token insertion, replacement, and removal, sprite-header
editing, revisioned undo/redo, atomic replacement saves, and the shared unsaved-document quit guard.
The source-level identity and legacy/expanded framing are displayed rather than guessed or silently
rewritten.
The same menu exposes a modular `LMNATAS1` aggregate editor. Its open workflow binds the document
to an exact sprite-length table, 256-entry ExAnimation size-mode table, and explicit maximum
animation-record count. Focused panels edit the native object and sprite streams, every palette
entry, ExAnimation slot settings/triggers/records, and all sixteen expanded-settings words. All
panels share one `NativeLevelAssetsDocumentController`, so a cross-domain undo, redo, canonical
save, dirty baseline, or quit decision applies to the aggregate atomically rather than letting its
nested files drift apart.
For an open profile-qualified ROM, the native Editors menu exposes the installed expanded-settings
record for the selected level. The window stages all sixteen exact words through
`ExpandedSettingsController`, detects any intervening project revision, and emits a single
checksum-inclusive `CommitRomMutation` into the application's ordinary undo/redo history. It does
not bypass the profile's optional-table declaration or retain a stale controller after committing.
The same menu now exposes explicit Level and Layer 3 navigation entries instead of leaving the
existing `ShowLayer3` application command reachable only from the terminal shell.
The profile-qualified ROM workspace can also open a native level-assets editor for the selected
level. It reuses the aggregate domain panels while retaining a `NativeLevelAssetsController`
against the immutable application revision. Object/sprite, palette, ExAnimation, and optional
expanded-settings edits are staged together. Profiles with Layer 2 add a dedicated tab for
object-record copy/paste/editing or indexed 16-bit tilemap-word editing according to the level
mode. Commit requires an explicit logical-PC allocation
search range, derives a profile-wide protection policy, allocates and repoints every changed tagged
payload, repairs the checksum, semantically reopens the complete aggregate, and dispatches one
revision-checked mutation into application history. A changed ROM makes the window stale and blocks
both further staging and commit; staged cross-domain changes also participate in the quit guard.
An opt-in Wine differential edits the installed level-105 compressed Layer 2
tilemap through Rust, expands into a fresh bank, and proves Lunar Magic 3.63 re-exports the exact
decoded payload. The standalone Layer 2 writer exposes the same checksum-atomic transaction for
callers that do not need the full aggregate.
In Map16 mode the native Editors menu now opens a ROM-backed complete-set editor. It addresses every
profile-declared page and tile, edits each packed 8×8 quadrant and exact Acts Like word, and routes
changes through `Map16Controller` so graph validation covers the entire set. Persistence requires an
explicit end-exclusive logical-PC allocation range and uses one profile-derived protection policy
for both planes; all graphics/Acts Like page pairs are allocated, repointed, checksum-repaired, and
committed through one stale-revision-protected application mutation. The editor never fabricates a
page beyond the profile's parallel pointer-table counts.
The ROM Map16 adapter separates packed tile/subtile and clipboard interaction from profile-backed
workspace lifecycle and paired graphics/Acts-Like allocation construction. Normal and reclaiming
commits therefore share the exact same page-shaped save options and protection policy.
Palette mode now has a ROM-backed swatch editor over the profile-declared native palette. It
displays the exact retained BGR555 word beside the platform color picker, stages changes through
`PaletteController`, and retains controller ownership checks. Like the other relocatable native
assets, committing requires an explicit logical-PC allocation range and derives protected metadata
from the active profile; the compressed/tagged payload update, pointer write, checksum repair, and
application history entry remain one stale-revision-checked transaction.
The ROM palette adapter separates its swatch coordinator from bounded `LMPALOWN` acquisition,
profile decoding, dirty-close lifecycle, and shared allocation/reclamation commit construction.
Malformed or stale evidence therefore never reaches the interactive controller workspace.
Graphics mode likewise exposes a ROM-backed 4bpp pixel editor. It decodes the selected native GFX
slot with the profile-selected LZ2/LZ3 codec, obtains palette zero through the same profile, and
reuses the portable editor's nearest-neighbor tile painter and hit testing. Pixel changes are
validated by `GraphicsController`; commit recompresses deterministically, requires an explicit
logical-PC allocation range, repoints the selected slot, repairs the checksum, verifies native
reload equality, and enters application history as one revision-bound mutation.
The native ROM graphics adapter keeps pixel/tile interaction in its coordinator, while focused
modules own bounded `LMGFXOWN` acquisition plus profile decoding and allocation/reclamation commit
construction. This keeps filesystem lifecycle and ROM placement policy out of the painter path.
ExAnimation mode now has a corresponding ROM-backed editor for exact slot settings, all sixteen
trigger-mask/value pairs, and the complete ordered record collection. Record forms use the active
profile's 256-entry size-mode table, so ordinary frame payloads retain their proven one- or
two-source-word width while special transfer kinds remain non-editable rather than being
reinterpreted. Insert, replacement, and removal pass through `ExAnimationController`; an explicit
allocation range governs compact encoding, repointing, checksum repair, semantic reload, and the
single application-history commit.
The ROM ExAnimation adapter has separate lifecycle, clipboard, and workspace/allocation modules;
the main coordinator owns only record, trigger, and slot-setting forms. Normal and reclamation
commits share one profile-derived save-options constructor, preventing placement-policy drift.
Overworld mode now completes the profile-backed aggregate surfaces. Opening asks for the explicit
hexadecimal profile slot, then decodes both tile layers, both event-reveal planes, endpoints,
messages, fixed-shape sprites, palette, and ExAnimation through `OverworldController`. The native
window reuses the portable editor's focused record, palette, and animation panels and also exposes
coordinate-addressed Layer 1/Layer 2 tile editing. Commit requires an explicit logical-PC search
range, applies one profile-derived protection policy to all nine payloads, and performs grouped
allocation, repointing, checksum repair, semantic reload, and revision-checked application history
as a single transaction. Dirty and stale aggregate states cannot be silently discarded or saved.
The ROM-overworld adapter is divided into focused orchestration modules: asynchronous ownership
acquisition and close/error lifecycle, allocation and optional reclamation commit preparation, and
the main nine-domain form coordinator. The domain controller remains the sole mutation authority.
The native File menu also exposes two project-wide operations without duplicating their binary
logic in the frontend. **Expand ROM…** derives the mapper and checksum field from the open ROM,
requires an explicit hexadecimal target size and fill byte, preserves any copier header, and enters
the bank-aligned expansion as one undoable application transaction. **Migrate Graphics
Compression…** requires an installed identity-audited profile, selects LZ2 or LZ3, derives protected
metadata ranges from that profile, and atomically decodes, recompresses, reallocates, repoints,
checksum-repairs, and semantically reopens every declared graphics slot. Its explicit end-exclusive
logical-PC allocation bounds prevent the graphical shell from guessing where free space may live.
The graphical **Tools** menu installs the same bounded `LMTOOLS1` configuration used by the
application shell and lists tools in configuration order. Clicking a tool is the explicit execution
boundary: placeholders are expanded by `lm-app`, while the native adapter launches the executable
with independent Unicode-safe arguments and an optional working directory, never a command shell.
Automatic open/save/level-change effects remain diagnostics rather than silently starting external
programs; launch failures and nonzero exits are reported in the application window.
The same menu installs bounded aggregate `LMUICFG1` frontend bundles atomically. Once installed,
the native toolbar is generated from stable typed actions and the complete catalog supplies its
localized labels; live `AppState` capabilities disable invalid actions. Keyboard input is translated
from `egui` into the portable character, F1–F24, editing, navigation, and platform-neutral modifier
model before resolving the configured shortcut. Toolbar and shortcut commands therefore share the
same application dispatch path. Copy, cut, and paste activations remain visibly rejected until the
active editor provides the corresponding typed selection payload, instead of falling back to an
untyped platform clipboard operation. The configured `AppTitle` and `StatusReady` values also drive
the live native window title and empty status bar, so those visible chrome surfaces no longer retain
hard-coded English while a validated catalog is active.
The first editor-owned graphical clipboard slice is implemented for palette colors in both the
portable palette document and profile-backed ROM palette windows. Copy serializes exactly one
selected BGR555 word through `ClipboardPayload::PaletteColors`, wraps the canonical bounded payload
in a platform-text `LMCLIP1` envelope, and publishes it through `egui`. Paste explicitly requests
the platform clipboard, rejects ordinary text, malformed hex, wrong domains, and multi-color
payloads, then applies the one color through the existing ownership-aware controller transaction.
ROM palette paste remains disabled for stale snapshots; portable paste participates in controller
undo/redo and dirty-save state. The envelope codec and one-color adapter live in a focused native
module and are tested independently of the window toolkit. The shared aggregate level-assets panel
uses the same adapter for its embedded palette, so both `LMNATAS1` documents and profile-backed ROM
level editing apply pasted colors through the aggregate palette controller, including exact
ownership checks and cross-domain transaction rollback.
Graphics tiles now use the same editor-owned path in the portable and ROM-backed graphics windows.
The selected 8×8 tile is encoded as one validated 64-pixel `GraphicsTiles` record; paste rejects
wrong domains, multiple tiles, malformed pixels, empty destinations, and stale ROM controllers.
Successful replacement passes through `GraphicsDocumentController` or `GraphicsController` with
the full editable-ownership shape, preserving portable undo/redo and native staged-commit behavior.
The portable document, pristine-layout ROM surface, and profile-backed installed ROM window also
expose horizontal and vertical transforms of the selected tile. Each transform materializes an
exact flipped 64-pixel tile and enters the same controller path as painting and paste: portable
documents receive one undoable revision, while installed fixed and ExAnimation-owned tiles remain
read-only and stale or active-file-worker ROM workspaces cannot flip.
Their eight-column tile sheets also share focus-scoped keyboard navigation. Unmodified arrow keys
move by one tile or one row, Home/End move to the bounded ends of the current row, and Page Up/Down
move by eight rows. Every move clamps to the available tile count, transfers keyboard focus, and
scrolls the destination into view; keys are not consumed unless the selected tile owns focus.
Level objects and sprites have corresponding typed adapters that retain their complete lossless
record bytes. The complete portable level editor, interpretation-bound native-stream document, and
shared aggregate level-assets panel can copy or replace one selected record without parsing it as
ordinary text. Aggregate paste is reused by the profile-backed ROM editor, where the active object
and sprite-length interpretation revalidates the replacement before its transaction advances; only
actual sprite records are copyable, so screen and control tokens are never silently retyped.
Layer 3 tilemaps and literal remap-command streams remain separate clipboard domains. Both the
standalone `LMLAY3V1` editor and the complete-level advanced panel copy their exact byte sequences;
paste rejects the opposite Layer 3 domain and applies one `ReplaceTilemap` or
`ReplaceRemapCommands` controller edit. Size bounds, canonical reopen, document revision, undo, and
dirty-save state are therefore enforced without round-tripping binary data through the visible hex
form or interpreting still-opaque remap commands.
Map16 clipboard handling likewise transports one complete ten-byte semantic tile: four packed
subtiles plus its exact Acts Like word. Portable page paste is one revisioned `ReplaceTile` and
invalidates both its form and raster caches; as designed for a page-local artifact, it retains
external Acts Like targets without pretending to own a complete graph. ROM complete-set paste uses
`Map16Controller::ReplaceTiles` with the full profile-declared resolution limit, so dangling links
and cycles are rejected before staged state changes. Stale ROM windows and wrong-domain or
multi-tile envelopes remain non-mutating failures.
ExAnimation clipboard support copies one complete fixed record in both portable and ROM-backed
windows. This retains transfer kind, frame count, size mode, destination/flag, ordinary frame words,
and every reserved byte rather than reconstructing a record from visible form fields. Paste decodes
the `ExAnimationRecords` domain and replaces only the selected record through the controller; the
bound 256-entry size-mode interpretation and maximum record policy revalidate it before revision
advancement. Portable form caches are invalidated after success, ROM paste is stale-protected, and
special transfer kinds remain lossless even when their ordinary frame editor is disabled.
The standalone editor, dedicated profile-backed ROM editor, and shared aggregate level-assets panel
also expose one selected ordinary frame as the distinct `ExAnimationFrames` domain. Its payload
records whether it contains one or two source words, and paste uses `EditRecordFrames::Replace` so
the destination record's recovered size mode must agree before any state changes. The aggregate
panel also supports whole-record transfer and is reused by both `LMNATAS1` documents and ROM level
assets. Whole-record data cannot be pasted as a frame, special transfer kinds stay disabled, and
profile-backed ROM frame paste retains the same stale-snapshot guard as record replacement.
Overworld record panels also own typed clipboard adapters for one sprite or one complete message.
Sprite transfer retains its identifier, coordinates, submap, and revision-shaped extension bytes.
Message transfer retains all 18-by-8 tiles and uses one `ReplaceMessage` controller command rather
than 144 independent cell edits, so validation, revision advancement, and rollback remain atomic.
The shared palette and animation panels likewise expose one exact color, whole fixed animation
records, and width-bearing ordinary frames through their existing typed domains. The shared panels
serve portable and profile-backed ROM editors; ROM paste is stale-protected and aggregate undo or
commit continues to cover the complete overworld transaction.
`lm-native` now consumes the same OS-string-safe startup grammar as the terminal application. A ROM,
identity profile, `LMUICFG1`, `LMTOOLS1`, and bounded recent-state store can be supplied without
lossy path conversion; initialization validates and installs them in dependency order before the
first frame. The recent store is shared library code rather than a binary-local duplicate and is
atomically rewritten only when its canonical value changes. **File → Open Recent** queues the chosen
path into the ordinary correlated open request, including dirty-project confirmation, request-ID
checking, bounded regular-file reading, failure cancellation, and MRU update. Cancelling the discard
prompt clears the queued path, so a later unrelated Open action cannot accidentally consume it.
The runnable shell exposes that lifecycle through `native-assets-open-file`,
`native-assets-edit-file`, `native-assets-render-file`, `native-assets-status`,
`native-assets-undo`, `native-assets-redo`, `native-assets-save`, `native-assets-close`, and
`native-assets-discard`. Open uses a bounded
`LMNADOC1` specification whose `document` and
`profile` paths are resolved relative to the specification, ensuring sprite lengths, animation
size modes, and record limits remain explicit for the complete session:

```text
LMNADOC1
document Level 105.lmnat
profile Profiles/US revision 0.txt
```

Unsaved aggregate documents participate in the application-wide EOF and quit guard.
Accepted aggregate batches record up to 100 canonical states. Undo and redo are revision-checked,
advance the monotonic document revision, retain the independent saved baseline, and invalidate redo
after a divergent edit; status reports both history capabilities.
`native-assets-render-file` consumes the existing `LMPALDR1` swatch-grid specification and renders
the palette from the exact current aggregate revision, including unsaved edits. It uses the shared
viewport/overlay adapter and deterministic PNG encoder and publishes only to a new output path.
The runnable shell exposes it as
`native-assets-edit SPEC SEARCH_START SEARCH_END`. A bounded `LMNATED1` specification resolves
optional `level`, `palette`, `exanimation`, and `expanded-settings` child scripts relative to the
specification file, for example:

```text
LMNATED1
level=Level edits.txt
palette=Palette edits.txt
exanimation=Animation edits.txt
expanded-settings=設定 edits.txt
```

Every child retains its established strict format and limits. The shell parses all children before
dispatch, stages every domain in memory, and publishes one checksum-valid undo entry; a late child
or edit failure preserves the complete application project.

`native-assets-edit-owned SPEC SEARCH_START SEARCH_END OWNERSHIP_MANIFEST` adds exact reclamation
for the four tagged members of that aggregate. The immutable controller snapshot supplies the
level-object, sprite, palette, and ExAnimation RATS descriptors; the expanded-settings record
remains a protected direct table write. The manifest must prove precisely the displaced tagged
blocks. Allocation, four pointer updates, reclamation, the settings write, checksum repair, and
history publication then succeed or fail as one transaction, and undo restores both the blocks and
the direct record.

`native-assets-file INPUT PROFILE [NORMALIZED [OBSERVATION]]` is the standalone differential
boundary. The profile supplies the exact sprite-length and ExAnimation size-mode interpretation;
normalization and the field-complete observation publish as one create-new batch. Observation paths
retain the aggregate source slot and settings presence and then address every nested object, sprite
token, palette color, animation field/record, settings byte, and settings word.
Built-process coverage also performs `profile-export native-assets` followed by `profile-import
native-assets` through Unicode/space-containing paths, then reopens every native domain and checks
the detected checksum. Repeating either create-new publication cannot replace its prior output.

The terminal frontend can start without a document and supports `open PATH`, `close`, `save`, and
`save-as PATH`; path arguments retain spaces and Unicode. Open/close/quit use the same dirty-state
confirmation protocol as graphical frontends. End-of-input never implicitly discards a modified ROM,
and a pathless or failed save releases its pending snapshot so the operation remains retryable.
Successful named opens and acknowledged Save As destinations also update a ten-entry most-recently-
used list. `recent` displays it and `open-recent INDEX` reuses the normal dirty-state/open-request
protocol. The runnable shell loads and atomically republishes the bounded, versioned `LMRECNT1`
encoding when started with `--recent-state FILE`; failed opens, cancelled
or failed saves, anonymous documents, and mere path selection never change the list.

Byte-identical transaction writes are discarded at the transaction boundary. A wholly no-op
controller commit creates no undo entry, dirty range, cache invalidation, or revision increment.
Write batches must use disjoint ranges, preventing order-dependent shadowing from making final ROM
state, history, and revision tracking disagree.

External tools are stored in the bounded, deterministic `LMTOOLS1` configuration format. Argument
templates support `{rom}`, `{project_dir}`, `{level_hex}`, and `{level_dec}` plus doubled literal
braces. Each argument is expanded independently: the core never invokes a command shell or starts a
process. The installed graphics editor additionally supplies `{graphics}` only after it has created
the private staged GFX/ExGFX file; persisted tools using that placeholder are selectable there and
retain the ordinary ROM/project/level values. Selection requires `{graphics}` in a direct argument;
using the staged file as a working directory is rejected before any temporary workspace exists.
Tool identifiers and per-tool event subscriptions must be unique, so configuration
encoding is canonical and cannot silently collapse duplicate subscriptions. It emits a
`LaunchExternalTool` effect for an explicit command or subscribed project event,
leaving permission prompts, process lifetime, and platform sandbox policy to the native frontend.
The runnable frontend accepts `tools-config FILE`, lists configured identifiers with `tools-status`,
and resolves typed requests with `tool-run ID` or `tool-event opened|saved|level`. Those preview
commands print the executable, working directory, and every argument on separate lines. Explicit
`tool-exec ID` and `tool-event-exec opened|saved|level` commands execute the same resolved request
cross-platform using direct process arguments rather than a command shell, wait for completion, and
report failed starts, signals, and nonzero exit codes. Event-driven effects remain previews in this
terminal shell so opening or saving a project never launches third-party software without an
explicit execution command; graphical/native frontends retain control over their own permission and
sandbox policy.
Normal interactive open completion, successful save acknowledgement, and actual level transitions
also emit their subscribed effects automatically. A malformed event template produces a typed
`ExternalToolFailed` diagnostic after the user operation succeeds, so optional integration cannot
block document lifecycle or navigation. Re-selecting the current level does not emit a false
level-change event.

Render consumers should construct untrusted or user-sized targets with `Canvas::try_new`, draw
through the pure scene APIs, and call `lm_render::encode_png` for a deterministic non-interlaced
RGBA artifact. The encoder uses filter-none scanlines and stored DEFLATE blocks, making PNG bytes
stable across platforms and suitable for exact golden hashes as well as ordinary image viewers.
Native frontends can instead call `lm_render::render_portable_level` for the complete validated
portable pipeline. It checks layer shapes and Map16 references, resolves optional entity
appearances, binds any supplied Layer 3 plane to exact source bytes, validates every scene graphics
tile and palette row, bounds dimensions/allocation, and returns a toolkit-neutral `Canvas`.

`oracle-capture` creates a replayable `LMORACLE1` manifest from legally supplied before/after ROMs
and canonical `LMOBS1` semantic snapshots. The `none` ownership policy claims no tagged blocks;
`changed-rats` claims only complete validated RATS blocks that were added, removed, changed, or
relocated. Capture records exact SHA-256 identities and changed ranges, never embeds ROM bytes, and
immediately replays the generated manifest before writing it. Additional `KEY=VALUE` arguments are
stored as ordered operation metadata.

`oracle-verify-suite` recursively discovers case directories in lexical order. Each case contains
`case.manifest`, `before.smc`, and `after.smc`; semantic cases additionally contain both
`before.obs` and `after.obs`. It evaluates every discovered case instead of stopping at the first
failure, prints a deterministic summary, and exits unsuccessfully if any hash, changed-range,
owned-RATS, or semantic assertion fails. Fixture ROMs remain external and are never committed by
the project. A parent case does not hide manifests in descendant directories. Suite roots and all
manifest, ROM, and observation inputs must be real regular files/directories rather than symlinks,
so a fixture tree cannot import mutable evidence from outside its audited root.

Passing cases do not by themselves prove corpus breadth. `oracle-coverage` separately requires any
declared Lunar Magic versions (`version:VALUE`), operations (`operation:VALUE`), and manifest
argument tags (`argument:NAME=VALUE`). Use tags such as `mapper`, `header`, `region`, and
`fixture_family` during capture. The audit rejects empty suites, missing dimensions, malformed
requirements, and duplicate case IDs without requiring ROM data to be checked into the repository.

`oracle-release-gate` combines replay and coverage and requires matching `before.obs` and
`after.obs` semantic snapshots for every case. Its policy must name a Lunar Magic version; all five
workflow operations (`open-save`, `render-level`, `level-edit`, `lunar-magic-reopen`, and
`emulator-boot`); and explicit `mapper`, `header`, `region`, `revision`, `rom_size`, and
`fixture_family`, and `subsystem` dimensions. The policy requires explicit corpus coverage for ROM,
codecs, RATS, levels, Map16, sprites, graphics, palettes, ExAnimation, overworld, rendering, and
application lifecycle; omitting any one fails the release decision. Cases recording Lunar Magic
errors are rejected. Repeated requirements can enforce every supported value in one executable
release decision.
Independent dimension coverage is not sufficient for compatibility qualification: every named
subsystem must have its own `lunar-magic-reopen` case and its own `emulator-boot` case. Removing one
operation/subsystem pair fails the gate even when both the operation and subsystem still occur in
other valid cases.
Those dimensions are also mandatory, nonempty metadata on every individual release case. Coverage
cannot be satisfied by tagging one case completely while leaving other workflow results without
their mapper, header, region, revision, ROM-size, or fixture-family provenance.
Subsystem labels use the fixed release vocabulary above; unknown labels and duplicate provenance
keys invalidate the individual case.
Each case must also bind that label to real decoded evidence with
`release/subsystem/NAME/observation-sha256`. The gate independently hashes every canonical
non-`release/` observation entry, rejects empty semantic observations, and requires an exact digest
match. A subsystem tag without accompanying model evidence therefore cannot satisfy qualification.
Operation labels alone are insufficient. The bound `after.obs` must also record affirmative
operation-specific evidence: reopen/checksum/unchanged-region results for open-save; PNG SHA-256
and positive dimensions for rendering; semantic-change/reopen/preservation results for edits;
reopen and semantic equality for Lunar Magic; and the emulator identity plus a successful boot.
Missing, false, empty, or malformed evidence fails qualification.
`render-level` release cases must additionally provide a bounded regular `render.png`. The gate
checks its actual SHA-256, PNG signature, complete chunk framing and CRCs, non-interlaced 8-bit RGBA
IHDR, image-data/IEND presence, exact consumption, and dimensions against `after.obs`. A textual
digest claim, malformed image, trailing payload, or symlinked render artifact cannot qualify.
`emulator-boot` cases likewise require a positive observed frame count, the exact SHA-256 of their
`after.smc`, and a bounded regular `emulator.png`. The screenshot's digest, dimensions, complete PNG
chunk framing, CRCs, and exact termination are verified, so a detached boot assertion or screenshot
from a different case cannot qualify.
ROM fixture identities and release-render PNG identities have one canonical lowercase SHA-256
spelling; uppercase textual aliases are rejected during parsing, programmatic validation, and
replay.

`checksum` also accepts an explicit logical checksum-field offset when working with a layout that
has already been independently identified. ROM mutation and inspection utilities—including
checksum, automatic checksum, patch, identity/RATS inspection, native level/Map16 inspection, and
binary diff—share the 32 MiB bounded ROM reader rather than loading arbitrary files eagerly. The
  same single loader is used by native level, Map16, graphics, palette, ExAnimation, and overworld
  transfers, whole-graphics migration, profile audit/import/export, asset inspection, and RATS
  reclamation. Fixed-shape ExAnimation size-mode and sprite-length lookup tables use exact bounded
  256-byte reads, so length validation never first loads an arbitrarily large file.
Standalone normalization and observation commands likewise use each portable format's authoritative
maximum encoded length. This covers graphics, palettes, Map16 pages and sets, complete levels,
Layer 3 settings and materialized planes, materialized animation frames, entity and overworld
sprite appearances, overworld paths, and overworld metadata; oversized inputs are rejected before
decoding or creating any output.
The same rule now covers composed render inputs, native-ROM transfer imports, profile-driven
imports, MWL inspection/normalization, custom-object sidecars, overworld layout descriptors, and
raw RGB/RGBA/indexed/PNG Map16 workflows. Fixed-shape pixel, ownership, and occupancy inputs use
exact reads derived from the selected model, while variable portable assets use their defining
crate's public maximum. Production CLI paths contain no eager whole-file `fs::read` calls.
The application shell applies the same 32 MiB ROM ceiling to startup, interactive open, and recent
open. An oversized interactive open cancels its request so the application cannot remain wedged in
a pending state. Command scripts and recent-state files stream through their declared limits, and
portable render shells use the individual graphics, palette, Map16, appearance, animation-frame,
level, and overworld model limits instead of a generic asset allowance.

`rom-expand INPUT OUTPUT MAPPER TARGET_LOGICAL_SIZE FILL` exposes bank-aligned expansion and
checksum repair as one project transaction and one copy-on-write workflow. It requires a supported
ROM whose detected mapper matches the explicit policy, reopens the complete target image, preserves
a copier header exactly, and publishes only a new path. Equal/smaller, unaligned,
mapper-unaddressable, oversized, aliased, and colliding targets fail without changing the source.
This low-level clean-room operation does not fabricate Lunar Magic's attribution, runtime-patch, or
checksum-compensation metadata blocks; those remain evidence-gated separately.
The runnable application exposes the same transaction as `rom-expand TARGET_LOGICAL_SIZE FILL`.
It derives mapper and checksum location from the open supported ROM, checks the application
revision, advances that revision once, participates in ordinary undo/redo, and remains dirty until
the normal Save As lifecycle publishes an immutable snapshot.

`map16-import` is copy-on-write: the input and output ROM paths must differ. It validates the
standalone page, protects both complete pointer tables and the checksum field from allocation,
reuses/replaces existing tagged blocks when possible, saves both planes transactionally, repairs
the checksum, and reopens the staged output to prove semantic equality before writing the file.

`graphics-import` follows the same safety boundary for `LMGFX4BP` files. It validates decoded and
compressed size limits and every 4bpp pixel before mutation, performs explicitly selected
deterministic LZ2 or LZ3
compression, protects the complete pointer table and checksum field, repairs the checksum, and
verifies decompression after reopening. Raw and interchange encoders report the exact tile and
pixel instead of masking an invalid index into a different color.

`graphics-import-owned` is the explicit reclamation variant. Its final bounded `LMRATS01`
manifest must validate against the input ROM and make exactly the current tagged graphics block
reclaimable. Allocation, pointer replacement, old-block erasure, and checksum repair are staged as
one project transaction and one undo batch before semantic reopen. A missing, stale, retained,
foreign, overlapping, or broader ownership claim is rejected without mutation; pointer
reachability and a `STAR` tag are never treated as ownership evidence.
The corresponding `palette-import-owned` and `exanimation-import-owned` commands use the same
shared one-transaction proof boundary for their native payloads; their ordinary import commands
remain copy-on-write.
`overworld-import-owned` extends that boundary to the complete nine-payload overworld transaction.
Its manifest must make exactly the unique currently tagged blocks reclaimable. Blocks reused by
any new pointer are retained, while every displaced block, pointer change, and checksum repair is
committed in the same undo batch before semantic reopen.
`level-import-owned` and `map16-import-owned` apply the same exact-set rule to their two native
streams. They reclaim displaced object/sprite or graphics/Acts-Like blocks, retain any block reused
through deterministic deduplication, and commit allocation, repointing, reclamation, and checksum
repair as one undoable transaction before verifying the complete semantic result after reopening.

Portable `LMGFX4BP` files are also first-class application documents. `gfx-open`, `gfx-edit-file`,
`gfx-render-file`, `gfx-status`, `gfx-undo`, `gfx-redo`, `gfx-save`, `gfx-close`, and `gfx-discard` provide the complete
lifecycle. Native-ROM and document controllers share one staged `LMGFXED1` ownership-aware edit
engine, and document revisions advance only after canonical encode/reopen. `LMGFXDR1` selects a
spec-relative `LMPAL1` file, decimal palette row, tile-sheet column count, and create-new output;
the public renderer validates empty assets, palette shape/row, dimensions, and canvas bounds. Its
optional six-field camera group uses the shared signed-origin/exact-zoom preview boundary.
Immutable save snapshots and the portable-session registry protect newer edits, dirty close, quit,
and scripted EOF. Bounded canonical history restores saved graphics revisions and invalidates redo
after a divergent ownership-checked batch.

Bitmap palette generation uses a focused variance-minimizing Wu quantizer over the 33×33×33
cumulative RGB histogram recovered from Lunar Magic's import path. It deterministically chooses
red/green/blue cuts, rounds representative colors onto SNES BGR555, removes duplicate rounded
colors, and maps every source pixel to a one-byte palette index. Inputs are bounded to 16 Mi pixels
and palettes to 256 colors. `quantize-rgb24` accepts packed RGB triples and atomically publishes a
canonical `LMPAL1` palette with its raw index plane; partial pixels, aliases, invalid color counts,
oversized inputs, and existing output paths fail before a partial output pair can remain.

Portable `LMPAL1` files are first-class application documents through `pal-open`, `pal-edit-file`,
`pal-render-file`, `pal-status`, `pal-undo`, `pal-redo`, `pal-save`, `pal-close`, and `pal-discard`. Both native and
portable controllers call one staged `LMPALED1` ownership-aware edit engine and validate ownership
shape even for empty batches. `LMPALDR1` renders the current unsaved exact BGR555 words as an
opaque swatch grid with explicit decimal columns and cell size; empty palettes, zero layout,
overflow, excessive canvases, and existing output paths fail. The same optional camera group can
crop and scale the current swatch canvas without a palette-specific transform. Canonical reopen, immutable save
snapshots, and portable dirty-shutdown policy match the other independent asset documents.
The same 100-state canonical history provides monotonic revision-safe palette undo and redo.

The next bitmap-import stage is also isolated in `lm-graphics`. It divides bounded indexed images
into row-major 8×8 tiles, searches occupied graphics slots in stable order for exact, horizontal,
vertical, then dual-flip equivalence, and allocates unmatched tiles into the lowest editable free
slot below the SNES 10-bit tile limit. Graphics, occupancy, and placements are cloned and validated
before publication, so protected slots, malformed shapes or pixels, and late exhaustion cannot
partially consume space. `import-indexed-map16` applies that planner to one 256×256 page, assembles
all 256 Map16 definitions with the requested palette row and Acts Like value, then publishes the
updated `LMGFX4BP`, canonical 0/1 occupancy map, and `LM16PAGE` together or leaves all absent.

`import-rgb-map16` completes that recovered pipeline for an opaque 256×256 RGB24 page. It runs the
Wu quantizer at a 15-color ceiling, preserves palette entry zero as the SNES transparent color,
offsets every generated pixel index into 1–15, and installs only generated colors into an explicitly
editable destination row. A one-byte-per-color access file (`1` editable, `0` protected) prevents
fixed or animation-owned palette words from being overwritten; unused row entries and all other
rows remain exact. Palette validation precedes graphics loading, and palette, graphics, occupancy,
and Map16 outputs are published as one four-file no-overwrite group.

`import-rgba-map16` provides the corresponding packed RGBA32 path for artwork with real
transparency. Fully transparent pixels become index zero without contributing a palette color;
fully opaque pixels are quantized into entries 1–15. Fractional alpha is rejected because SNES 4bpp
tiles provide binary, not blended, transparency. The palette ownership check, stable tile reuse and
allocation, Map16 assembly, and atomic four-file publication are identical to the RGB workflow.

`import-png-map16` exposes the same RGBA transaction for ordinary 256×256 PNG artwork. It accepts
8-bit RGB, RGBA, grayscale, grayscale-alpha, and expanded indexed PNG input, applies bounded decode
limits, and rejects malformed images or other dimensions before reading or publishing destination
artifacts. PNG alpha retains the same exact binary-alpha rule as raw RGBA input.

`level-export` and `level-import` transfer the exact native layer-1 object and sprite streams in a
versioned `LMLVL1` file. Import atomically allocates and repoints both streams, protects both complete
512-entry pointer tables and the checksum field, repairs the checksum, and semantically reloads the
saved level before writing the output ROM. Use `standard` for unextended three-byte sprite records,
or supply the ROM/tool-specific 1,024-byte sprite-length table so custom extra bytes remain lossless.
Native sprite persistence uses a checked framing boundary: records must retain their universal
three-byte base, legacy streams cannot contain controls or `FF`-prefixed records, expanded screen
markers remain seven-bit, and controls exclude reserved `FE`/`FF` terminator/escape values.
`LMLVL1` decoding also rejects any bytes hidden after an inner object or sprite terminator instead
of accepting a model whose canonical re-encoding would silently discard them. The shared native
project save additionally reparses objects with recovered command lengths and sprites with the
explicit revision-specific 1,024-entry length table before any allocation or pointer mutation, so
directly constructed public models cannot bypass controller validation.
The application can edit these transfer files independently through an interpretation-binding
`LMNLDOC1` open specification. `native-level-open SPEC`, `native-level-edit-file SCRIPT`,
`native-level-status`, `native-level-undo`, `native-level-redo`, `native-level-save`,
`native-level-close`, and `native-level-discard` reuse
the same bounded `LMLEDIT1` object, header, and sprite edits as native-ROM editing. The exact
four-table sprite-length artifact—or an explicit `standard` declaration—remains attached for the
document lifetime. Atomic canonical reparse, revision checks, immutable saves, failed-save retry,
dirty close/EOF protection, and source-level preservation are enforced by focused controller,
specification, and shell modules. Bounded canonical history retains the exact sprite-length
interpretation across monotonic undo and redo.
Native secondary-exit encoding likewise requires all 8,192 records and rejects destination levels,
screen/X/Y coordinates, or overlapping flag bits that cannot be represented by the six parallel
planes. Packed MWL exit records use the same checked field boundary.

`LMLEVEL2` is the complete revision-independent semantic level bundle. It includes legacy and
expanded headers, Layer 1 and Layer 2 objects and raw tilemaps, lossless variable-width sprites,
entrances, screen and secondary exits, Map16 overrides, and unknown extension blobs. Parsing is
bounded and requires exact consumption. Map16 overrides are stable-key records, so duplicate tile
indexes are rejected during both encode preflight and decode rather than entering the editor with
ambiguous lookup semantics. `level-bundle` can emit a deterministic normalized copy and
a canonical `LMOBS1` observation, with every destination required to differ from every input/output.
`level-bundle-edit INPUT SCRIPT OUTPUT` applies a bounded `LMAUXED1` script across entrances,
screen exits, secondary exits, and stable-key Map16 overrides as one staged batch. Insert, replace,
remove, and move-before operations use ordered indexes; Map16 supports keyed upsert/removal. The
CLI verifies canonical encoding by semantically reopening the complete edited bundle before
create-new publication, so a late bad command or serialization failure leaves no partial output.
Native frontends can use `CompleteLevelDocumentController` for the same atomic edit model with
exact revision checks. Its immutable, request-correlated save snapshots prevent stale completion
events from clearing newer work and keep failed persistence retryable. The shared bounded portable
history retains 100 canonical revisions; undo and redo use exact revision tokens, advance the
monotonic revision counter, preserve the saved baseline, and clear redo after divergent edits.
The runnable application exposes that lifecycle as `bundle-open FILE`, `bundle-edit-file SCRIPT`,
`bundle-render-file SPEC`, `bundle-status`, `bundle-undo`, `bundle-redo`, `bundle-save`,
`bundle-close`, and `bundle-discard`.
Rendering consumes the current in-memory revision, so previewing does not require publishing
unsaved edits. Both the CLI normalization workflow and application shell consume the same bounded
`LMAUXED1` parser from `lm-level`, avoiding grammar drift between frontends.
`LMBNDR1` render specs are line-oriented, relative to the spec directory, and preserve the entire
remainder of path lines, including spaces and Unicode:

```text
LMBNDR1
map16 assets/all.lm16set
graphics assets/all.lmgfx
palette assets/colors.lmpal
output previews/Level 105.png
layer1-width 16
layer1-height 27
layer2-width 16
layer2-height 27
viewport-origin-x -16
viewport-origin-y 0
viewport-width 1280
viewport-height 896
zoom-numerator 2
zoom-denominator 1
```

Optional `appearances` and `layer3-plane` lines enable resolved entities and source-bound Layer 3.
An optional `overlays` line points to a bounded `LMOVLY01` screen-decoration artifact.
The six viewport lines are also optional as a group. When present, they render the current
world-space revision through the shared signed-origin nearest-neighbor camera; dimensions must be
nonzero and zoom must remain inside the recovered inclusive 100–5000% range. Omitting the group
retains full-world output for backward compatibility.
Provider-resolved `LMENTAPP` entity appearances can also be edited as independent application
documents with `entity-app-open`, `entity-app-edit-file`, `entity-app-undo`, `entity-app-redo`,
`entity-app-status`, `entity-app-save`, `entity-app-close`, and `entity-app-discard`. Bounded
canonical history restores the complete painter order with monotonic revisions, an independent
saved baseline, and divergent redo invalidation. Bounded `LMENTED1` scripts insert, replace, remove,
or move painter-ordered Layer 1, Layer 2, and sprite records while retaining signed placement,
palette, tile, and flip fields. Batches are revision checked, canonical-reopened, and atomic;
immutable saves and dirty quit/EOF protection match the other portable editors.
Provider-resolved `LMOWAPP1` overworld sprite appearances have the parallel `world-app-open`,
`world-app-edit-file`, `world-app-undo`, `world-app-redo`, `world-app-status`, `world-app-save`,
`world-app-close`, and `world-app-discard` lifecycle. Its bounded canonical history restores stable
definition identity and nested painter order together, with monotonic revisions, an independent
saved baseline, and divergent redo invalidation. Bounded `LMOWAED1` scripts insert, remove, or reorder definitions
by stable 16-bit sprite ID and insert, replace, or remove their ordered tile parts. Revision checks,
atomic batches, canonical reopen, immutable saves, and dirty shutdown protect the keyed document;
the built application exercises the workflow through paths containing spaces and Unicode.
Output publication is create-new and cannot replace an existing file.
The shell keeps custom-object, overworld-metadata, overworld-path, Layer 3, and complete-level
sessions in one portable-document registry. End-of-input refuses to abandon any modified session;
explicit `quit` names all dirty portable documents and requires affirmative discard confirmation.
This complements rather than changes `LMLVL1`: native ROM writes remain limited to tables whose
revision layout has been established.

Custom level-object libraries use a synchronized `.mw0`/`.mw0t` model in a focused `lm-level`
module. Binary entries retain complete variable-width object records; Unicode descriptions retain
their optional UTF-8 BOM, LF/CRLF convention, trailing-line framing, and empty final descriptions.
Both files are independently capped at the recovered 32-KiB buffer boundary. Insert, replace,
remove, and reorder keep both halves paired and commit only after both encoded sizes validate.
`custom-object-library` inspects a pair and can atomically publish two normalized outputs plus a
canonical semantic observation without
overwriting either input.

Custom sprite-placement libraries use the distinct synchronized `.mw2`/`.mwt` model. The binary
half retains its one-byte header and groups one or more variable-width sprite records by the
recovered bit-zero placement boundary. Record sizes come from an explicit 1,024-byte revision
length table rather than guessed three-byte records. The text half retains its optional UTF-8 BOM,
LF/CRLF framing, Unicode descriptions, and trailing newline. Insert, replace, remove, move, and
Unicode search operate on whole multi-sprite placements. `custom-sprite-library` performs bounded
inspection and atomically publishes a normalized pair plus a placement- and record-addressable
observation.

Native Map16 sidecars have a separate raw-entry boundary. `.m16` is exactly `0x2000` bytes, exposed
as 2,048 lossless little-endian 32-bit entries. `.s16` loads any prefix up to `0x1C000` bytes into
a zero-filled 28,672-entry workspace. Its canonical writer matches Lunar Magic by retaining
through the last nonzero dword, rounding upward to `0x800` bytes, and emitting one `0x800`-byte
block for an all-zero workspace. `native-map16-sidecar m16|s16` performs bounded inspection,
normalization, and sparse entry-addressable observation without assigning unverified meanings to
the raw dword fields.

Custom-display `.dsc` text is modeled separately from the binary Map16 sidecars. The lossless
reader preserves the complete source, optional UTF-8 BOM, line endings, malformed records, unknown
flag bits, and Unicode bytes while exposing Lunar Magic's valid description, display-mapping, and
alternate-mapping records. Its resolved 32,768-entry view reproduces page expansion, ordered
replacement, mapping masks, and native flag-byte construction. Recovered description escapes are
`\\`, `\n`, `\r`, and the six-hex-digit `\b`, `\d`, `\f`, and `\m` style directives.
`dsc-sidecar INPUT [LOSSLESS_OUTPUT [OBSERVATION]]` provides bounded inspection and differential
evidence without claiming that Lunar Magic has a native `.dsc` writer.

`render-map16-dsc GRAPHICS PALETTE MAP16_SET DSC PAGE FIRST_FEATURE FIRST_SUPPRESSED
SECOND_FEATURE OUTPUT_PNG` applies the recovered direct-display resolver before deterministic
rasterization. Feature arguments are `0` or `1`. Mapped IDs must exist in the supplied complete
Map16 set, and blended entries use Lunar Magic's averaged-pixel path against the sheet's black
background. Alternate `.dsc` mappings are deliberately not applied here because the binary consumes
them while materializing level cells, not while directly rendering a Map16 definition.

The level renderer exposes that second stage through `render_portable_level_with_dsc`. It resolves
expanded Acts Like chains, materializes position- and mode-dependent alternate mappings, then
applies direct display substitutions before scene construction. Averaged custom displays are
carried alongside painter-ordered tile instances, so they blend with the pixels beneath them at
the correct layer boundary rather than darkening an already composed image. The materializer's
parallel `0x20` flags remain an editor-diagnostic overlay concern and are not conflated with the
gameplay-oriented render output.

The headless command is `render-level-dsc LEVEL MAP16 GRAPHICS PALETTE APPEARANCES|none
LAYER3|none DSC CUSTOM MARKERS FIRST SUPPRESSED SECOND LEVEL_MODE L1_WIDTH L1_HEIGHT L2_WIDTH
L2_HEIGHT OUTPUT_PNG`; its five switches are strictly `0` or `1`. The application's `LMBNDR1`
render specification accepts the equivalent all-or-nothing fields `dsc`, `dsc-custom-display`,
`dsc-special-markers`, `dsc-first-feature`, `dsc-first-suppressed`, `dsc-second-feature`, and
`dsc-level-mode`. Paths remain specification-relative and bounded. Omitting the complete group
continues through the ordinary renderer unchanged.

The application owns `.dsc` files through `dsc-open`, `dsc-replace`, `dsc-undo`, `dsc-redo`,
`dsc-status`, `dsc-save`, `dsc-close`, and `dsc-discard`. Replacement validates and reparses a
complete external document as one atomic revision; saving publishes the exact replacement bytes
through an immutable snapshot. Undo and redo retain up to 100 exact source states, use monotonic
revision tokens, preserve the saved baseline, and invalidate redo after a divergent replacement.
This whole-document boundary keeps ecosystem-authored formatting and unknown records lossless
instead of synthesizing an unproven canonical writer.

The application shell owns either native sidecar as an independent revisioned document.
`native-sidecar-open SPEC` reads `LMN16DC1` with `kind m16|s16` and a relative `file` path;
`native-sidecar-edit SCRIPT` consumes bounded `LMN16ED1` `set HEX_ENTRY HEX_DWORD` batches.
`native-sidecar-undo`, `native-sidecar-redo`, `native-sidecar-status`, `native-sidecar-save`,
`native-sidecar-close`, and `native-sidecar-discard` provide a 100-state canonical history,
immutable save snapshots, canonical `.s16` persistence, kind-preserving navigation, failed-save
retry, and dirty-close/EOF protection.

The runnable application owns the same pair through a revision-checked document controller.
`custom-sprite-open SPEC` reads a bounded `LMSPDOC1` specification containing `data` and
`sprite-lengths` paths relative to the specification; the companion `.mwt` path is derived from
the `.mw2` data path. `custom-sprite-edit SCRIPT` applies one atomic `LMSPRED1` batch whose grouped
record token uses `+` between complete hexadecimal sprite records. The script also supports
remove, move, header, and retained text-format edits. `custom-sprite-undo`, `custom-sprite-redo`,
`custom-sprite-status`, `custom-sprite-save`, `custom-sprite-close`, and `custom-sprite-discard`
provide bounded canonical history, paired snapshot persistence, retry after failed publication,
and dirty-close/quit protection. History retains the exact immutable sprite-length interpretation
while restoring the header, grouped placements, descriptions, and representable text framing.

`lm-app` exposes the same model through a separate custom-object document controller. Native
frontends receive immutable paired save snapshots, may continue accepting edits while persistence
is in flight, and acknowledge or cancel the exact snapshot request. Acknowledging an older saved
snapshot never marks newer edits clean; overlapping saves, stale edit revisions, aliased paths,
revision exhaustion, and late failures preserve the active library and retryable save state. The
shared application persistence module stages and synchronizes both files, retains both originals
as backups until the complete pair publishes, preserves permissions, rejects symlinks, and checks
canonical paths plus platform file identity so alternate spellings and hard links cannot disguise
an alias. It captures the staged and original file identities before publication; rollback and
backup cleanup remove a name only when it still identifies the expected file. It attempts two-file
rollback before reporting any retained recovery backup. For create-new Save As, the successful
destination link is the authoritative commit point; failure to remove the private staging link
afterward cannot falsely leave the application pathless and dirty despite a complete saved file.

The runnable shell exposes that controller directly. `custom-open PATH.mw0` derives the adjacent
`PATH.mw0t`; `custom-edit SCRIPT`, `custom-undo`, `custom-redo`, `custom-status`, `custom-save`,
`custom-close`, and the explicit dirty-state escape hatch `custom-discard` manage an independent
sidecar document session. Its bounded canonical history restores binary records, Unicode
descriptions, and representable BOM/newline framing as one paired value. Bounded
`LMCUSED1` scripts use hexadecimal indexes, raw object bytes, and UTF-8 description bytes, avoiding
locale- or quoting-dependent command parsing:

```text
LMCUSED1
replace 0 020004 4368616e676564
insert 1 030005 5365636f6e6420e29883
move 1 0
format no-bom lf trailing
```

Normal close refuses unsaved changes. Save acknowledges the controller revision only after both
sidecars publish; a failed paired write releases its pending token and remains immediately
retryable without marking the edited model clean.

`LM16SET1` stores all Map16 graphics pages followed by all Acts Like pages with an explicit bounded
page count. `map16-set-file` can inspect it, validate every Acts Like chain in near-linear time,
write a normalized copy, and emit a page/tile-addressable oracle observation. Inspection reports
cycles, stale targets, or non-256-tile public pages; normalization refuses them. Standalone page
encoding is fallible for the same shape reason. Project saves validate the entire set shape and
Acts Like graph before allocating, then save all page pairs as one atomic ROM/history transaction;
late invalid graphs leave both ROM bytes and history unchanged.

The application shell can own an independent complete Map16 document with `map16-set-open`,
`map16-set-edit-file`, `map16-set-render-file`, `map16-set-status`, `map16-set-undo`,
`map16-set-redo`, `map16-set-save`, `map16-set-close`, and `map16-set-discard`. It reuses
`LMM16ED1`, applies the entire ordered edit
batch to a staged workspace, canonically encodes and reopens before advancing an exact revision,
and saves immutable snapshots. Its shared 100-state canonical history provides monotonic,
revision-checked undo/redo with saved-baseline restoration and divergent redo invalidation. Dirty
documents block close, quit, and scripted EOF. `LMM16DR1`
selects a decimal page plus spec-relative `graphics`, `palette`, and `output` paths, allowing the
current unsaved revision to be previewed through the shared renderer with create-new publication.
It accepts the common optional camera group used by standalone `LMM16R1` previews.

Standalone `LM16PAGE` files have a separate application lifecycle through `map16-page-open`,
`map16-page-edit-file`, `map16-page-render-file`, `map16-page-status`, `map16-page-save`,
`map16-page-undo`, `map16-page-redo`, `map16-page-close`, and `map16-page-discard`. Bounded
`LMPGEDT1` scripts replace complete tiles or individual subtiles and
Acts Like words by page-local tile index. Each batch is staged, index checked, canonically reopened,
and revisioned; immutable save and dirty-shutdown policies match complete workspaces. Arbitrary
16-bit Acts Like values remain lossless because one page cannot prove the existence or cycles of
external targets. Full graph validation remains an `LM16SET1` responsibility.
The shared 100-state history provides revision-safe page-local undo/redo without inventing
cross-page graph semantics.
`LMPGDR1` binds graphics, palette, and create-new output paths and renders the current unsaved page
revision through the shared portable Map16 renderer rather than rereading the document from disk;
its optional camera fields use the identical validated viewport path.

`render-map16-page` joins decoded `LMGFX4BP`, `LMPAL1`, and `LM16PAGE` inputs into a deterministic
256×256 RGBA PNG. It validates all 1,024 subtile graphics references, requires at least eight exact
16-color palette rows, preserves transparent color zero and flip attributes, and refuses to
overwrite any input. The model-to-raster path is the public `lm-render::render_portable_map16_page`
API rather than CLI-owned logic, so native frontends receive the same validation and pixels. The
reference fixture has an exact SHA-256 golden PNG assertion.

The runnable cross-platform shell exposes the same workflow as `map16-render-file SPEC`. Its
bounded, spec-relative `LMM16R1` format uses `graphics`, `palette`, `page`, and `output` fields;
remainder-of-line path values retain spaces and Unicode. Output publication is create-new, so a
preview cannot overwrite an input or an earlier render.

`render-level` joins `LMLEVEL2`, `LM16SET1`, `LMGFX4BP`, and `LMPAL1`. Layer grid dimensions are
explicit hexadecimal arguments because ROM revisions encode level modes differently. It validates
both layer shapes and every referenced Map16, graphics, and palette entry, paints Layer 2 then
Layer 1 with priority ordering, bounds the output canvas, and produces a golden-hashed PNG. Raw
tilemaps are rendered exactly; object/sprite previews remain definition-provider driven and are not
guessed from custom records. An optional `LMENTAPP` file carries provider-resolved object and sprite
tiles in painter order with source identity, signed coordinates, palette, and flip attributes.

`palette-export` and `palette-import` use a versioned `LMPAL1` file containing exact little-endian
SNES BGR555 words. Import requires the file's color count to equal the target revision layout,
protects the complete 512-entry pointer table and checksum field, performs tagged allocation,
repairs the checksum, and requires semantic reopen equality before writing the output ROM.

`exanimation-export` and `exanimation-import` use a versioned `LMEXAN1` wrapper around the canonical
compact animation payload. Both commands require the revision-specific 256-byte size-mode table;
this prevents frame lengths from being guessed. Import enforces record and encoded-size limits,
protects the complete pointer table and checksum field, allocates transactionally, repairs the
checksum, and semantically reloads the animation before emitting the output ROM.

Frame editing uses that same recovered size-mode table. Ordinary records expose one or two
little-endian source words per frame and support atomic insert, replace, remove, and move batches.
The editor preserves unknown fixed-record bytes, enforces the compact 0x200-byte payload capacity,
and refuses inactive or special no-payload transfer kinds rather than guessing their runtime
meaning. Declared frame counts are checked against their one- or two-word width before decoding,
so a double-width count above 128 is rejected instead of being silently truncated to 0x200 bytes;
the record-shape tests cover every transfer kind at both widths and all count boundaries. At
compact persistence boundaries it separately rejects unknown bytes that have no native
representation, rather than reporting a successful save that loses them. Both level and overworld
controllers route these frame edits through their existing revision-bound transactional commit
paths.

`exanimation-frames` applies the same operations headlessly to an `LMEXAN1` file. Its bounded UTF-8
script accepts hexadecimal `insert INDEX WORD[,WORD]`, `replace INDEX WORD[,WORD]`, `remove INDEX`,
and `move FROM BEFORE` lines, with blank lines and `#` comments allowed. The command validates the
complete script and resulting compact record before atomically creating a distinct output file,
which makes frame transformations reproducible in oracle-fixture and build pipelines.

The application owns independent compact animation documents through `ex-open-file`,
`ex-edit-file`, `ex-status`, `ex-undo`, `ex-redo`, `ex-save`, `ex-close`, and `ex-discard`. `LMEXDOC1` binds the
`LMEXAN1` path to its exact spec-relative 256-byte size-mode table and decimal maximum-record
limit. The controller retains those interpretation inputs for edits, frame inspection, canonical
reopen, and immutable save snapshots; dirty close, quit, and scripted EOF preserve the file.
Canonical history retains the same interpretation-bound animation values for undo and redo.
There is intentionally no guessed animation renderer: graphical previews consume provider-resolved
`LMANFRM` artifacts until native transfer execution is independently verified.
`animation-frame-file` strictly decodes and canonically normalizes those artifacts and can publish
a target-addressable observation of the tick, exact 4bpp tile pixels, and exact BGR555 overrides.
Output aliases and collisions are rejected before the normalized file and observation are created
as one batch.

The application clipboard assigns frames their own `ExAnimationFrames` domain instead of treating
them as whole records or raw bytes. Each clipboard entry carries an explicit one- or two-word
width, round-trips in the existing versioned MIME payload, and is accepted only in ExAnimation
mode. Malformed widths and cross-editor pastes fail before a controller mutation is requested.
Toolkit-independent controller helpers canonicalize copy selections, remove cuts in descending
index order, and insert pasted frames in clipboard order. The entire cut or paste is staged as one
frame batch, so width, capacity, or late-index failure cannot leave a partially edited record.

`overworld-export` and `overworld-import` transfer both tile layers, both event-reveal planes,
endpoints, messages, sprites (including unowned extension bytes), palette, and compact ExAnimation
in one self-describing `LMOWFULL` file. The target revision is supplied separately as a strict
key/value layout descriptor containing these required keys:

```text
layer1_table=0x...
layer2_table=0x...
event_source_table=0x...
event_destination_table=0x...
endpoint_table=0x...
message_table=0x...
sprite_table=0x...
palette_table=0x...
animation_table=0x...
width=...
height=...
event_reveals=...
endpoints=...
messages=...
sprites=...
sprite_record_len=...
palette_colors=...
animation_max_records=...
animation_max_encoded=0x...
```

Unknown, duplicate, missing, and malformed keys are rejected. Import requires the file shape to
match the target descriptor, protects all nine complete 512-entry pointer tables and the checksum
field, stages all nine tagged payloads atomically, repairs the checksum, and reloads the entire
aggregate model before writing the output ROM.

`LMOWPATH` is the portable semantic interchange for general overworld navigation. It has bounded node and
edge counts, exact framing, stable node identifiers, optional level/exit links, and preserved raw
flags. Structural validation rejects duplicate IDs and directions, self-links, and stale endpoints;
an additional reciprocity gate distinguishes deliberate one-way routes from missing reverse links.
It remains distinct from the engine's special path-link table rather than being silently injected
into a mismatched ROM structure.

SMW US revision 0's native special path-link table is independently modeled by `LMOWLN1`. The
engine stores fourteen records as three fixed planes at PC `$21964`, `$219AA`, and `$219F0`:
five-byte source endpoints, five-byte destination endpoints, and two target-coordinate bytes.
`smw-overworld-path-export` and `smw-overworld-path-import` perform identity-bound, create-new CLI
workflows. Import writes all three planes, repairs the checksum, semantically reopens the table, and
commits as one undoable operation. The application shell exposes the same boundary through
`overworld-native-path-export` and `overworld-native-path-import`; ordinary undo, redo, and Save As
then apply to imported changes. Tables above the fourteen-entry fixed capacity now install the
recovered Lunar Magic runtime at logical hook `$21A35`. Its tagged 112-byte body carries the
`LM 00 01` marker, count-minus-one and five-byte-stride immediates, and eight independently
validated long operands into one contiguous `5N + 5N + 2N` RATS allocation. Installed tables can
grow or shrink transactionally; exact runtime/table ownership, semantic reopen, checksum repair,
optional expansion, and one-step undo are enforced by both CLI and application workflows.
The graphical path-link workspace exposes every endpoint word/submap byte and target coordinate,
supports the complete 0–128 record range, and fills newly added records with native sentinels.
Growing the pristine table past fourteen entries exercises the recovered runtime installation;
stale revisions, changed selections, dirty close, and application shutdown retain staged state.

SMW's separate native overworld warp/exit endpoints are modeled losslessly by `LMOWWR1`. Lunar
Magic's active pristine-ROM descriptor identifies four adjacent 27-word planes at logical PC
`$20431`, `$20467`, `$2049D`, and `$204D3`: source packed-vertical, source horizontal-tile,
destination packed-vertical, and destination horizontal-tile. The packed vertical word remains
opaque until its submap bit fields are independently proven. `smw-overworld-warp-export` and
`smw-overworld-warp-import` provide identity-bound create-new CLI workflows; the corresponding
application commands are `overworld-native-warp-export` and `overworld-native-warp-import`.
The graphical warp-link workspace likewise exposes all four opaque words for every record and
supports 0–256 entries without inventing bit fields for packed vertical coordinates. Growing past
the pristine 27-entry capacity installs the current runtime, while recognized legacy storage still
migrates through the same transactional application command.

Native overworld level names now decode both the pristine SMW three-segment dictionary and Lunar
Magic's expanded 19-byte direct records. The loader validates both long-call hooks, the fixed
runtime, its relocatable pointer, and exact RATS ownership. `smw-overworld-name-export` writes an
`LMOWMETA` file containing names only; `smw-overworld-name-import` installs or transactionally
grows the compatible runtime, repairs the checksum, semantically reopens the result, and always
creates a distinct output ROM. Native import rejects noncanonical level ordering, gaps, raw flags,
and unrelated metadata domains instead of silently losing information. The application-shell
commands `overworld-native-name-export` and `overworld-native-name-import` expose the same boundary
inside an open project, with stale-revision checks and one-step undo.
The graphical ROM editor exposes this complete boundary as hexadecimal tile records selected by
canonical level number. Editing an expanded-only level materializes every intervening positional
record as `$1F` blanks, so installation cannot introduce a gap or silently renumber later names.
Import stages all four planes plus checksum as one undoable transaction and semantically reopens
the result before publication.

Imports above the vanilla 27-link capacity now install Lunar Magic's recovered current runtime
instead of rejecting the table. The installer uses the exact two 64-byte runtime templates, guards
the original entry and return hooks at logical PC `$20509/$20566`, allocates one tagged runtime
and one contiguous tagged four-plane payload, publishes all six long pointers, and expands by a
LoROM bank when required. Existing current-variant installations are detected through both `JSL`
hooks and the `LM 10 01` marker; growth replaces the validated RATS allocation, republishes all
plane pointers, repairs the checksum, and remains one undo operation. The older `0xFF`-marked
variant is migrated failure-atomically: both the legacy runtime and contiguous four-plane table
must be exact RATS allocations, their hooks and decoded pointers must agree, and only then are the
old blocks reclaimed in a staging image. Reclaimed space is tried before optional expansion; the
current runtime, table, hooks, count, pointers, and checksum publish as one undoable operation.
Built CLI and application tests reopen the migrated ROM as the current variant and prove rollback.

`path-open FILE`, `path-edit SCRIPT`, `path-undo`, `path-redo`, `path-status`, `path-save`,
`path-close`, and `path-discard` make that model editable as a separate application document. Its
bounded canonical history restores nodes, edges, and reciprocity-valid topology together while
retaining an independent saved baseline. `LMOPEDT1` batches stable-key node and
`(source, direction)` edge upserts/removals while retaining raw flags and optional links:

```text
LMOPEDT1
node upsert 1 123 456 0 105 81
node upsert 3 7 8 6 none 20
edge upsert 2 3 down none 00
edge upsert 3 2 up fe 00
```

Final reciprocity validation permits explicitly one-way edges and lets one batch repair both halves
of a route. Removing a node removes incident edges atomically. The revisioned controller refuses
dirty close and acknowledges a save only after recoverable file replacement; failed writes remain
dirty and retryable.

`LMOWMETA` provides the same bounded portable boundary for 19-tile level names, player start
positions, and music/palette/scroll settings keyed by submap. Duplicate levels, players, or submaps
are rejected, while raw flags and unknown revision bytes round-trip exactly. CLI normalization is
copy-on-write. The SMW-US native Mario/Luigi start boundary is now recovered as a 22-byte
runtime-options block with retained adjacent bytes and redundant tile coordinates; custom starts
enable Lunar Magic's three-byte runtime path and save transactionally. The bounded `LMOWST1`
boundary and `smw-overworld-start-export/import` or application
`overworld-native-start-export/import` commands preserve both players and the four unowned option
bytes, validate tile-centered coordinates, and semantically reopen every write. The native
graphical workspace edits either player through this same revision-checked command and keeps the
unowned bytes read-only. Native submap
settings are
the seven exact 32-byte expanded-level records at slots `$200..$206`, rather than the portable
12-byte abstraction. `LMOWSET1` round-trips those records losslessly, and
`smw-overworld-settings-export/import` plus the application shell's
`overworld-native-settings-export/import` install or update them transactionally. The shared
profile detector materializes recovered pristine defaults only when no allocation marker exists
and rejects malformed present ownership; the graphical workspace edits all sixteen words per
record and uses the same install/update transaction.

The application shell can edit that portable document independently with `metadata-open FILE`,
`metadata-edit SCRIPT`, `metadata-undo`, `metadata-redo`, `metadata-status`, `metadata-save`,
`metadata-close`, and `metadata-discard`. Bounded canonical history restores all three stable-key
collections and retained unknown bytes together, with monotonic revisions and divergent redo
invalidation. Bounded `LMOMEDT1` scripts use stable keys and retain every unowned byte:

```text
LMOMEDT1
name upsert 105 81 12121212121212121212121212121212121212
start upsert 0 1234 5678 6 a1
settings upsert 0 7 8 9 a 9234 0a0b0c0d0e
name remove 106
```

The toolkit-neutral controller applies each script as one atomic batch, tracks an exact document
revision and saved baseline, refuses dirty close, and acknowledges persistence only after the
existing file has been recoverably replaced. Failed publication releases its pending snapshot for
an immediate retry.

## Compatibility policy

The original executable is used only as a behavioral oracle. Unknown bits and tagged payloads are
preserved unless an operation explicitly owns them. Allocation and pointer changes are staged before
commit, and a failed operation must leave the source image and undo history unchanged.
For identity-qualified projects, successful commits and application history navigation also refresh
the cached stored/computed checksum evidence. Controller snapshots and status consumers therefore
observe checksum metadata from the same project revision as the ROM bytes; stable game/revision and
mapper identity remains tied to the opened document until explicit redetection.

`rats-reclaim` accepts only an explicit `LMRATS01` ownership artifact, validates every descriptor
against the input ROM, rejects overlapping/nested claims and any reclamation intersecting the SNES
internal-header/vector block, erases all proven-dead blocks and repairs the checksum in one
reversible project transaction, reopens and verifies the output, and publishes a new path
atomically. `rats-plan` performs the same ownership validation without mutation. The CLI
deliberately does not infer or author ownership: manifests must come from a revision-specific
reference index or another subsystem that can prove exclusive ownership.
The native **File → Reclaim Owned RATS Blocks** workflow uses the same policy. It bounded-loads one
canonical manifest, binds it to the current project revision, validates every descriptor before
showing a reclaim/retain byte summary, and accepts an explicit erase fill. The application command
revalidates the proof, protects the complete internal-header/vector block, repairs the checksum,
and publishes one undoable revision. Stale or forged evidence leaves ROM, history, and revision
unchanged; a manifest retaining every owned block is an exact no-op.

See [REIMPLEMENTATION_ARCHITECTURE.md](REIMPLEMENTATION_ARCHITECTURE.md) and
[REIMPLEMENTATION_TEST_MATRIX.md](REIMPLEMENTATION_TEST_MATRIX.md) for the compatibility tiers and
fixture requirements. A production-ready editor still requires legal external fixtures covering
clean, headered, expanded, SA-1, and ecosystem-modified ROMs; write support should remain opt-in and
target a new file until those differential gates pass.

## Current boundary

The native frontend now has an opt-in, self-capturing `visual-smoke` build. It waits until the
workspace has rendered across multiple frames, requests the real Glow viewport through egui, and
publishes the returned framebuffer through `lm-render`'s bounded PNG encoder. This avoids relying
on macOS Screen Recording or Accessibility permissions and distinguishes visible composition from
controller-only tests. The first supplied-ROM capture found a real pristine level `$105` startup
failure: the `$FF` no-Layer-2 bank sentinel was mapped to expanded PC `$3FD900`, producing an empty
error page. The SMW-US profile now resolves Layer 2 per level, returns `None` for the sentinel,
retains authenticated layouts for present pointers, and rejects out-of-range slots. Focused profile
and native-editor tests cover both branches; the post-fix framebuffer contains the populated level
workspace. Captures remain local because they include ROM-derived graphics.
The level workspace was subsequently changed from one unbounded vertical document into a
window-filling split view. Its 280–380-point tool column owns an independent vertical scrollbar and
the level canvas owns more than 57% of every supported 720-point-or-wider viewport, plus the
remaining editor height and its own two-axis scrolling. A real 1100×720-point macOS framebuffer
shows the canvas beside—not below—the Map16 and settings controls. A pure width-allocation test
guards the minimum, default, large, and high-resolution window cases.
The automatically detected workspace now also authenticates every pristine main and special
graphics payload before constructing an editor controller. A modified pointer or incompatible
compressed payload is rejected with an explicit audited-profile requirement instead of partially
rendering through vanilla addresses. An exhaustive eight-worker gate decodes the level model,
Layer 2 layout, and complete built-in render assets for all 512 pristine level slots.

This repository now provides a tested implementation foundation and useful headless workflows; it
is not yet honest to call it complete feature parity with the mature Lunar Magic application. The
first real Lunar Magic 3.63 differential corpus now covers all 512 MWL exports from pristine US SMW:
all files parse and re-encode byte-for-byte, and `mwl-corpus` makes that gate reproducible. Real
complete-Map16, shared-palette, and 52-file GFX exports also pass exact Rust round trips. The
retained Lunar Magic 3.63 corpus now replays eleven complete ROM transitions: two level saves plus
MWL frame, Layer 3, optional-asset, and semantic edits; overworld, title-screen, and credits
transfers; and first-time palette and ExAnimation installation. The level-save fixtures cover
Lunar Magic's pristine-ROM expansion, 13-block RATS installation, checksum repair, and
content-preserving MWL pointer relocation. The palette and ExAnimation oracles prove the optional
hook markers, allocated pointer tables, 257-color MWL palette layout and TPL rotation, and
byte-identical compact ExAnimation MWL/ROM payload. Both canonical `case.manifest` fixtures and
retained `oracle.manifest` captures participate in recursive suite replay; canonical manifests take
precedence when both names exist.
The
remaining work depends on differential fixtures from legally supplied ROMs: identifying each
supported ROM revision's pointer-table layout, validating every object/sprite extension and editor
patch, extending interactive coverage for behavior not yet represented by a verified controller,
and proving byte-level save compatibility. The core
keeps those unknowns behind explicit layout descriptors and lossless byte-preserving models so they
can be added without coupling the portable application shell to one ROM revision.

Complete binary MWL level import now has a ROM-backed semantic coordinator rather than a collection
of independent section copies. `MwlNativeLevel` decodes and canonically encodes all eight sections;
the installed SMW-US import stages Layer 1, Layer 2, sprites, the 257-color palette, ExAnimation,
the expanded header, all recovered main and separate-midway entrance fields, the expanded
level-mode byte, and every secondary exit targeting the selected level before producing one
revision-bound ROM mutation. A failure in any late global table therefore publishes no partial
edit. The shell exposes the operation as
`level-mwl-import FILE SEARCH_START SEARCH_END`, automatically retargeting the MWL to the selected
level and deriving every allocation policy from the active revision profile.

Ghidra's `ExportBinaryMwlLevelFile`, `ImportBinaryMwlLevelFile`,
`LoadLfix3LevelRuntimeFields`, `WriteLfix3LevelRuntimeFields`, and expanded-level-mode helpers are
the ground truth for the 64-byte MWL header boundary. The four vanilla entrance planes and four
Lfix3 planes agree with all 512 retained MWLs. Controlled Wine imports located the separately
allocated 512-byte level-mode table through two agreeing JSL hooks: lower seven bits persist, while
bit 7 is recomputed by Lunar Magic and an MWL edit containing only that bit causes no ROM change.
A reciprocal Wine test confirms Lunar Magic exports a Rust-written persistent mode value. Header
bytes 7 and 8 produce no persistent difference even at boundary values under 3.63, and byte 13 is
reserved zero padding; they remain losslessly preserved in the MWL container rather than being
invented as ROM state.

Complete native ROM-to-MWL export now closes the reciprocal boundary. The controller materializes
all eight semantic sections from one immutable installed-ROM snapshot, retains allocator-dependent
Layer 1/2/sprite/palette/ExAnimation source addresses, exports every secondary exit targeting the
selected level, and derives the transient high level-mode bit with the recovered Lunar Magic rule.
`ExportBinaryMwlLevelFile` (`004797D0`) proves that tilemap Layer 2 writes the in-memory
`0x800`-byte two-plane workspace directly; MWL does not flatten it into a separate 32-column
raster. `LoadSpriteDataPcOffsetTable` (`004810E0`) additionally proves that opcode `$22` at logical
`$02D8F5` selects the installed 512-byte per-level sprite bank table at `$077100`, while pristine
storage uses the shared bank operand at `$02D8F6`. The SMW-US profile now resolves that generation
at runtime instead of reading relocated sprites through the obsolete shared bank.

The installed 257-word palette payload and Lunar Magic's MWL working buffer differ by one exact
word rotation; export rotates left and import rotates right, making the transformation reciprocal.
A live ignored Wine oracle now performs Rust installed-ROM export, Lunar Magic import, and Lunar
Magic re-export, then compares header, Layer 1, Layer 2 descriptor/data, sprites, palette,
secondary exits, ExAnimation, expanded settings, and final ROM checksum. The terminal shell exposes
`level-mwl-export FILE` beside `level-mwl-import`, and the profile-qualified native level-assets
window exposes complete MWL import/export actions with bounded asynchronous reads and the existing
atomic import coordinator.

Batch MWL transfer now has an explicit shell workflow. `level-mwl-export-all TEMPLATE` strips the
template extension and publishes `base 000.mwl` through `base 1FF.mwl`; all payloads are staged and
synchronized before collision-safe grouped publication, with inode-checked rollback limited to
files created by that call. `level-mwl-export-modified TEMPLATE` uses Ghidra's recovered Layer 1
pointer predicate and accepts a zero-document result. Its retained installed fixture selects
exactly level `000`, matching live Lunar Magic mode `1`. `level-mwl-import-dir DIRECTORY
SEARCH_START SEARCH_END` enumerates visible regular MWLs deterministically, takes each destination
from its validated header, commits each successful import as its own complete ROM transaction, and
continues after malformed or unsavable files. Batch export generation now lives in the shared
toolkit-independent application layer, so the shell and native frontend consume the same immutable
profiled snapshot and publication code. The native level-assets window runs both all-level and
modified-only generation on a background worker and reports completion without freezing the event
loop. The native multi-level insert dialog enumerates through the same shared filesystem contract,
prepares one revision-bound transaction at a time from the current ROM snapshot, advances only
after dispatch acknowledgement, continues after read/decode/save rejection, reports progress, and
supports button or Escape cancellation. Broader installed-runtime and ROM-revision fixtures remain.

Native IPS creation now mirrors the recovered `CreateIpsPatch` (`0041F0B0`) selection order:
original ROM, modified ROM, then output patch. A background worker performs bounded regular-file
reads, compares logical ROM bytes after copier-header normalization, uses the shared deterministic
normal/RLE IPS encoder, rejects canonical input/output aliases, and atomically creates or replaces
the selected `.ips` file without freezing the frontend.

The current SMW US revision-0 profile also contains the recovered multi-bank overworld-message
installation boundary used once more than 96 level-name slots are enabled. It installs the fixed
version-1.10 renderer, allocates a three-byte pointer for each of the even 194–512 messages, packs
strings into independently owned 192-record pools, shares each pool's empty terminator, repairs the
checksum, and requires a complete semantic reopen. `LMOWMSG1` retains the exact fixed-size editor
records; `smw-overworld-message-install` installs it into a create-new ROM and
`smw-overworld-message-export` materializes either pristine or installed storage. The pristine
decoder reproduces the original 23-entry level/trigger selector, 25 relative text pointers, and
high-bit row termination into all 97×2 fixed editor records, so installing the expanded runtime
does not erase vanilla messages. Detection validates the hook, runtime
marker, adjacent table operands, RATS table length, exact pool ownership, pointer mappings, and
every `$FE`-terminated or complete 144-byte string instead of treating arbitrary patched ROM bytes
as messages. Re-importing replaces or grows every installed pool and the pointer table, republishes
both runtime operands, repairs the checksum, semantically reopens, and commits as one undoable
application revision. A modular graphical workspace edits any tile in all 8×18 records, grows or
shrinks the even 194–512 record table, rejects the reserved `$FE` terminator, and preserves staged
state across stale revisions, failed dispatch, close, and shutdown.

The native main overworld event-reveal tables are also writable through the recovered Lunar Magic
layout. `LMOWEVT1` stores one to 255 reveal pairs without losing the source-plane little-endian or
destination-plane big-endian semantics. `smw-overworld-event-export ROM FILE` reads either the
pristine 112-entry fixed tables, Lunar Magic's tagged-source/fixed-destination overworld-transfer
layout, or an exact two-plane expanded installation;
`smw-overworld-event-import INPUT_ROM FILE OUTPUT_ROM` installs or grows two RATS-owned planes,
repairs the checksum, and publishes only a new ROM. The application shell exposes the same
revisioned operation as `overworld-native-event-export/import`, including no-op detection,
undo/redo, bounded reads, semantic reopen, and Save As. The compressed event-tilemap runtime is a
different subsystem and is not installed for these main reveal planes.
The supplied 3.63 executable now verifies the hybrid layout through a real Wine
`-TransferOverworld` transition. The complete operation expands the destination to 1 MiB and adds
23 RATS blocks. For main reveals, Lunar Magic relocates only the source plane to a `$F0`-byte
owner, derives a 120-record workspace from that allocation, and continues reading 120 destination
words through the fixed operand. `overworld-event-file INPUT [NORMALIZED [OBSERVATION]]`
canonically reopens this representation and emits entry-addressable `LMOBS1` evidence. The
fixture binds all 124 physical changed ranges and owners while clearly scoping decoded evidence to
main reveals. A second combined observation, produced by
`smw-overworld-transfer-observe ROM OBSERVATION`, now binds all four transferred event domains:
the unchanged 96-entry event-number map, unchanged 24 special-event records, and the materialized
event-tilemap streams in addition to the 120 main reveals. The transferred legacy tilemap becomes
92 indexed entries with 74 nonzero auxiliary bytes and a zero secondary-high plane.
`smw-overworld-transfer-full-observe ROM OBSERVATION` extends the same allocation-independent
snapshot to thirteen recovered domains. The real pristine transfer preserves all 14 special-path
links, both player starts, seven expanded-settings records, 194 messages, and seven boss-sequence
messages; it materializes 54 warp links from the stock 27-link representation and 96 direct level
names from the stock 93-name representation. `oracle-full.manifest` binds these decoded meanings
to the same 124 changed ranges and 23 exact RATS owners.
The same transaction's Map16 installation is independently decoded by
`smw-transferred-map16-observe ROM OBSERVATION`. It follows the exact split operands in the
revision-0 runtime, validates all three RATS owners, decodes the back-to-back even/odd definition
planes into 8,192 semantic words, and combines the 2,884 raw-low/compressed-high acts-like entries.
The observer excludes pointer spelling, RATS placement, compression packets, and trimming so
future differential cases compare the editor-visible words.
`smw-installed-map16-remaps-observe ROM OBSERVATION` covers the two remap families installed by
the same save. It follows the patched runtime pointers, verifies every RATS owner, reconstructs
120 groups containing 371 source/destination range pairs, and reconstructs 120 groups containing
44 flag/source/destination records stored in parallel planes.

The recovered native custom-overworld-sprite stream now has a separate model from the portable
fixed-record sprite boundary. `native-overworld-sprites ROM MAPPER POINTER RECORD_SIZES
OBSERVATION` follows a RATS-owned stream using Lunar Magic's 128-byte ID-to-record-size table.
The model preserves seven independently offset map lists, the 24-record-per-map limit, packed
8-pixel coordinates, screen metadata, variable extension bytes, zero terminators, and empty-map
offset aliases. Project save/load is copy-on-write and undoable; semantic observations exclude
allocation placement and packed spelling.

`ExAnimation` slot options are also represented as their own native seven-byte table rather than
being folded into animation records. Use
`exanimation-slot-options ROM MAPPER POINTER OBSERVATION` to inspect a RATS-owned table. Rust
preserves each opaque low nibble and exposes the four inverted high bits as positive option states;
transactional saves relocate the complete owner and publish the pointer atomically.
The cross-platform application exposes the same state through
`ExAnimationSlotOptionsController`: edits are duplicate-checked and staged, commits are bound to
the immutable application revision, repair the SNES checksum, and participate in ordinary
application undo. Late commits are rejected without changing the open ROM.

The per-level Super GFX animation switches are a distinct one-byte feature record. Their four
inverted high bits enable palette animation, vanilla animated tiles, global ExAnimation, and level
ExAnimation; the low nibble is unrelated state. `ExAnimationFeatureOptions` supplies named positive
states while round-tripping all 256 byte values. Binary MWL files carry the byte in the low byte of
ExAnimation metadata word 0, so typed GUI and `exanimation-features on|off on|off on|off on|off`
script edits replace only bits 4–7 and preserve both the low nibble and the metadata word's upper
24 bits.

Installed ROMs use `ExAnimationFeatureRomLayout`, which names the first byte of the 512-entry table.
The preceding byte is decoded as Lunar Magic's representation sentinel. Planning a write retains
the exact legacy migration behavior—including the post-write level `$110 = $30` assignment—and
returns `requires_runtime_installation` whenever the resulting feature byte is nonzero. The
standalone save API refuses that case unless the caller has separately proved the expanded
ExAnimation runtime installed, then updates the data and SNES checksum in one undoable transaction.
This is the runtime ensured by `WriteExAnimationFeatureFlag`, not the separate feature-control patch
installed at `004606b0`.

`InstalledExAnimationFeatureRomLayout` follows the expanded-animation hook's mapped runtime target
to the feature-table operand. Its installed load/save entry points first resolve Lunar Magic's
primary/fallback expanded-ExAnimation installation gate, then use the resolved storage. That outer
gate is the proof that the runtime consuming nonzero feature bytes is active.

Revision profiles opt into that installed path with `exanimation.features=installed`. Each
primary/fallback variant reuses the corresponding expanded-ExAnimation hook and first operand,
while declaring only its feature-table displacement. Validation rejects mismatched gate shapes or
locator origins. ROM-aware profile audit and allocation planning resolve and protect the first
operand, final operand, sentinel, and complete 512-byte table; allocator searches therefore cannot
overwrite any part of the feature contract.

The profile-native level-assets controller loads that installed feature record beside its existing
payload aggregate and treats it as revision-bound staged state. The Settings panel exposes the same
four positive checkboxes as the MWL editor. Applying them preserves the low nibble; committing runs
the checked installed-feature save after the grouped payload save on the private project image, and
the live application receives their combined byte delta as one revision-checked mutation and one
undo step. A focused integration test covers load, semantic edit, aggregate commit, canonical
reopen, checksum validity, and exact undo. An ignored live-oracle test writes byte `$5B` through
that installed Rust path in the retained Lunar Magic-created ROM, asks Lunar Magic 3.63 under Wine
to export level `$000`, and recovers `$5B` from MWL ExAnimation metadata word 0. This verifies all
four inverted high bits plus preservation of the unrelated low nibble through the actual executable.
The installed-assets Super GFX preview is a live staged view rather than a one-shot diagnostic:
enabling it renders the current aggregate, and every accepted settings, level, Layer 2, palette, or
ExAnimation-feature edit invalidates and regenerates the texture from the controller's new staged
state. Preview failures are retained as diagnostics without retrying every GUI frame, and closing
or reopening the workspace clears the live-preview lifecycle state.
Disabling bypass no longer makes that preview unavailable. For the supported SMW-US revision-0
profile, the same path reads the recovered four-byte object-tileset table at `$00292B` and
sprite-tileset table at `$0028C3`, resolves each selected native GFX file through the profile's
compression/pointer layout, materializes four FG/BG slots plus two blank native slots and all four
SP slots, and renders the staged level with those active legacy assignments. Profiles whose legacy
assignment tables have not been recovered are rejected explicitly instead of borrowing SMW-US
offsets.
When the staged per-level feature byte enables vanilla animation, the live installed preview also
runs those resolved tiles through the recovered eight-phase GFX32/GFX33 frame-table interpreter
using the active object tileset. Its 60 ms presentation clock regenerates only when the phase
changes. Disabling the vanilla-animation option leaves the staged base cache static and stops
that tile-domain mutation immediately. The independent staged palette-animation option applies the
recovered eight-color Dragon Coin cycle to CGRAM entry `$64`; either enabled built-in domain keeps
the shared clock active, while disabling both stops repaint scheduling. A failed phase render
suspends timed retries until a subsequent accepted edit invalidates the preview, preserving
actionable diagnostics without a GUI retry loop. The retained pristine-ROM gate proves adjacent
phases produce distinct native tile caches, and focused tests prove the two staged switches gate
their domains independently while the palette pass changes no unrelated color.
Installed graphics profiles that address files `$32` and `$33` now supply the player/animation and
animated-display sources directly; only pristine-size tables that end before those file numbers
use the recovered fixed startup pointers. This keeps relocated Lunar Magic graphics installations
out of the pristine-pointer path. The retained fixture mirrors the two authentic special pointers
into a 52-entry profiled table and requires profiled and pristine decoding to produce identical
tiles before comparing animation phases.
The same live preview now presents the staged full-level canvas through
`rasterize_canvas_viewport`, rather than relying on toolkit texture scaling. Its fixed 512×448
screen raster supports exact 50%, 100%, 200%, 300%, and 400% nearest-neighbor zoom plus bounded
pixel camera origins. Camera limits are derived from the viewport's exact visible-world rectangle,
so the view cannot pan beyond the last fully visible source region; short axes clamp to zero.
Changing camera or zoom invalidates one preview frame without disturbing staged assets, and
workspace reopen resets the view. Zoom changes use the shared exact `Viewport::zoom_at` transform
around the screen center instead of jumping to a new upper-left world point. The raster itself is
drag-sensitive: a captured pointer/world origin converts the total screen displacement through the
selected rational scale, clamps the result, and avoids accumulating fractional per-frame error.
Ctrl/Command-wheel over the raster advances one exact zoom entry per raw wheel event and uses the
pointer's clamped raster coordinate as the stationary world anchor. Unmodified and Alt-modified
wheel input is left to the enclosing document, so ordinary vertical page scrolling is not captured
by the preview.
An optional Map16 grid is composed through the shared `draw_editor_overlays` path after viewport
sampling. Its 16×16-world-pixel origin and spacing are transformed by the same exact viewport, so
lines remain one screen pixel wide while tracking non-cell-aligned pans and every supported zoom.
The toggle invalidates only the live texture and resets with the workspace; disabling it leaves the
sampled viewport unchanged.
Clicking the raster resolves the pointer through `Viewport::screen_to_world`, selects the containing
16×16 Map16 cell, and adds the shared half-open marching selection overlay after the optional grid.
The selected cell is reported in hexadecimal, follows pan/zoom through fresh screen bounds, clamps
when a staged level-mode change shrinks the world, and can be cleared explicitly. Selection state
resets with the workspace. Its 60 ms outline clock is separated from the staged asset-animation
phase: selection-only refreshes never enable disabled vanilla-tile or palette animation, nor do
they trigger the unsupported-profile built-in-animation gate.
Each successful staged render also materializes a semantic inspection for that selected cell from
the exact Layer 2 and Layer 1 placement slices supplied to the framebuffer. It retains every
duplicate placement in painter order instead of inventing a single top tile, decodes the 14-bit
Map16 number and whole-definition X/Y flips, and reports the definition bank plus four raw subtile
words. Object-backed placements resolve the foreground namespace and its Acts-Like data; compressed
Layer 2 placements resolve Lunar Magic's separately loaded `$8000-$FFFF` background namespace,
which has no Acts-Like table. Their definition identity is kept separate from the exact stored
word: the installed descriptor's three-bit active bank supplies bits 12–14 and the cell supplies
its low 12 bits, while raw attribute and whole-definition flip bits remain intact. The inspector
reports the resulting global `$8000-$FFFF` number. Object-backed paints instead retain all 15 bits
of their foreground `$0000-$7FFF` identity and carry explicit false outer flips; definition bit 14
is never reinterpreted as a Layer 2 attribute. Each subtile is additionally reported at its post-placement visual
quadrant with its original definition quadrant, ten-bit tile number, CGRAM row, priority, and
effective X/Y flips after composing the whole-definition flips. It also invokes
`Map16Set::resolve_acts_like` with the complete installed tile count as its traversal bound,
displaying the exact chain and self-linked terminal or preserving the typed cycle, out-of-range,
and resolution-limit failure. Missing definitions and cells with no sparse placement are explicit.
Object-backed placement slices come from the standard renderer's authenticated write history
rather than a scan of its `$25`-initialized cache. Each object's repeated internal writes to one
cache index collapse to that object's final value, while later objects that overwrite the same
visible cell remain separate entries in native stream order. Consequently unwritten initialization
cells are absent from both the framebuffer input and the selected-cell inspection.
Each placement also carries an explicit composition mode. The installed SMW-US preview selects
Lunar Magic's recovered averaged-pixel path for Map16 `$027`–`$02A` only in object tileset 4,
covering both object-backed paints and native Layer 2 tilemap words. Object paints compare their
complete 15-bit foreground definition identity, so `$4027` does not alias `$0027`; compressed
Layer 2 instead removes its two outer-flip bits before comparing the local word. Nonzero source
pixels clear the low bit of each source and destination RGB channel before adding their halves;
transparent source pixels preserve the prior framebuffer. The inspector reports `opaque` versus
`average` from the same placement value consumed by the raster.
The main level editor applies the same identity rule to OSC display parts. Their complete 15-bit
tile number is passed to the bounded 1,024-definition M16 lookup, so an unavailable `$4001`
definition remains an unresolved `$4001` marker instead of aliasing M16 entry `$0001`.
Its compressed Layer 2 presentation also retains the raw word's whole-cell X/Y flips. Shared
background planes reverse source pixels during precomposition, ordinary atlas cells reverse their
texture coordinates, and M16-backed cells permute visual quadrants while composing the outer flips
with each subtile's own flags.
Standard-object cache and visual-catalog paints now use the same bounded Map16 source resolver as
OSC display parts. Entries below `$0200` select the vanilla atlas, `$0200-$03ff` may resolve through
the loaded M16 sidecar, and unavailable or higher 15-bit identities remain explicit unresolved
markers instead of sampling outside the atlas or aliasing a low definition. Catalog previews and
placed artwork therefore cannot disagree about the source of one rendered object cell.
The OSC visual catalog also paints that same four-digit unresolved marker when a definition or
required texture is unavailable; it no longer collapses an unresolved display part into blank
space before placement.
The distinct recovered background-half-color flag is also installed without treating it as
destination averaging. Level modes `$0C` and `$0D` mark every native Layer 2 tilemap placement as
`half-color`; its nontransparent source RGB channels shift right once independently of the
framebuffer beneath them, while color zero remains transparent. This mode takes precedence over
the tileset-specific average rule because Lunar Magic renders the background through its dedicated
`RenderTransparentLevelBackgroundMap16Tile` path.
The framebuffer accepts an explicit palette-routing rule for every layer while retaining direct
routing as the public default. The installed preview selects the recovered low-row-plus-four rule
only for object-backed Layer 2 under object tileset 3: encoded rows 0–3 address CGRAM rows 4–7,
encoded rows 4–7 remain direct, tilemap Layer 2 remains direct, and Layer 1 is always direct. The
inspector reports both the encoded palette row and effective CGRAM row from that same rule.
The same inspection retains provenance on every materialized 16×16 standard-sprite preview part:
original serialized token index, sprite ID, recovered source class, per-sprite part ordinal, preview
definition index, signed pixel origin, and all four tile words. Parts are included when their
half-open pixel bounds overlap the selected Map16 cell, remain in the same vector order used by the
post-layer sprite painter, and exclude right/bottom edge-only contact. Native-empty and
custom-display sprite IDs still produce no invented part. Each raw word is also decoded in the
native renderer's column-major quadrant order into its nine-bit tile number, ordinary-SP versus
animated-GFX33 page, CGRAM row 8–15, priority flag, and X/Y flips.
Accepted aggregate edits and failed renders clear stale inspection state before it can be shown.
Focused raster coverage proves a camera-cropped 200% view repeats the selected source pixel buckets
exactly; interaction-state coverage proves anchored 200% zoom, 200% and 50% drag conversion, and
extreme edge clamping. It also proves cross-platform modifier filtering, discrete zoom boundaries,
pointer-coordinate clamping, and a noncentral stationary anchor. Overlay integration coverage fixes
the expected grid residues after odd-pixel pans at 200% and 50%, checks both axes and intersections,
and requires the disabled path to retain the unmodified source color. Selection coverage fixes
pointer-to-cell mapping, exact transformed bounds and clipping, distinct marching phases, and the
independent refresh/asset/selection phase matrix. Inspection coverage requires duplicate Layer 2
writes followed by Layer 1 writes, preserves an unavailable definition as a distinct final hit,
and distinguishes a truly empty cell. Acts-like coverage requires a three-definition chain, direct
self-link, two-definition cycle, and three distinct out-of-range targets.
Map16 subtile coverage checks all four whole-definition flip combinations, their visual-to-source
quadrant permutations, and every raw tile/palette/priority/effective-flip field.
Layer-routing coverage checks both layers independently, the 3→7 and 4→4 boundary, typed routing
count mismatch, and the exact object-backed/tileset-3 activation predicate.
Sprite inspection coverage requires materialization to retain its original sprite/part provenance
and requires the overlap filter to accept top-left, bottom-right, and exact-cell intersections in
painter order while rejecting left, right, and bottom edge-only contact.
Tile-word coverage fixes every decoded bit field and the renderer's top-left, bottom-left,
top-right, bottom-right word order.

The global- and level-ExAnimation feature switches remain persistence/runtime controls in this
workspace, not a claim that compact records have been interpreted for rendering. Their transfer
types still require an oracle-backed `LMANFRM` provider as documented by the portable renderer;
the installed preview does not invent tile or palette overrides from lossless record fields alone.
The graphical reveal-table workspace covers all 1–255 records, preserves the mixed native
endianness behind the model boundary, initializes growth with zero reveals, and rejects source
tiles above `$07FF` before they can be normalized on reopen. Growing pristine storage from 112 to
200 records exercises allocation, checksum repair, and semantic reopen through the shared command.

The independent event-number mapping has its own `LMOWMAP1` boundary. A pristine ROM is decoded
from Lunar Magic's eight legacy source/value pairs; installed maps use the recovered version-1.10
32-byte runtime and either its 96-byte fixed table or complete 256-byte extended table.
`smw-overworld-event-map-export/import` and
`overworld-native-event-map-export/import` expose the same bounded operation through the CLI and
application. Installation verifies the original hook and reserved runtime bytes, repairs the
checksum, semantically reopens before commit, supports undo/redo, and never replaces its ROM input.
The graphical ROM editor exposes all 256 entries, including the stored-prefix length and the
extended-only tail, through the same revision-checked command.

The 24 special-event reveal records use `LMOWSPC1`: little-endian source words, big-endian
destination words, and one lossless direction byte per record. Saving them installs Lunar Magic's
complete two-runtime compatibility family and three independently owned table planes as one
transaction, rather than merely repointing editor-only data. Use
`smw-overworld-special-event-export/import` or
`overworld-native-special-event-export/import`; both paths validate all five RATS owners and every
runtime hook, repair the checksum, reopen semantically, preserve the input ROM, and support
installed-table updates. The graphical ROM editor exposes every mixed-endian logical record and
direction byte without reproducing the three-plane serialization in the frontend. Runtime and
table pointers preserve Lunar Magic's low-bank LoROM mirror spelling during both installation and
copy-on-write updates.

The compressed game-visible event tilemaps have a separate `LMOWTIL1` model. It preserves the
exact owned `$800 + $800` planar primary bytes and `$800` secondary high bytes; callers can overlay
those high bytes onto the independently owned base word plane. The project layer validates the
complete four-fragment Lunar Magic runtime, both hooks, the opcode repair, split pointers, exact
RATS ownership, and fixed LZ2/LZ3 output extents. Pristine installation and installed updates are
checksum-valid, semantically reopened, undoable transactions. Use
`smw-overworld-event-tilemap-export/import` or
`overworld-native-event-tilemap-export/import` for the same bounded operation in the CLI and app.
A typed detector now verifies all eight pristine runtime/hook fragments before materializing zero
workspaces, or validates either installed LZ2/LZ3 representation; callers no longer infer storage
from a single opcode. The exact Lunar Magic 3.63 low-bank hooks and fixed loader call operands are
retained rather than normalized to byte-equivalent `$80+` bank mirrors. The graphical editor
addresses all 2,048 primary low/high bytes and all
2,048 secondary high bytes, with explicit load-before-apply and revision/dirty-lifecycle guards.

Boss-sequence text uses the separate `LMOWBOS1` boundary: seven messages, each containing eight
rows of 24 glyph bytes. `smw-overworld-boss-sequence-export` reads either pristine SMW row pointers
or Lunar Magic's combined allocation; `smw-overworld-boss-sequence-import` writes the canonical
single RATS allocation, republishes all 56 pointers, repairs the checksum, and reopens the result
semantically before creating the output ROM.

Credits tilemaps use `LMCREDT1`, containing the complete 256×32 little-endian word model.
`smw-credits-tilemap-export/import` and `credits-native-tilemap-export/import` expose the strict
CLI and application workflows. Imports migrate pristine 202-row storage to Lunar Magic's exact
expanded runtime and RATS-owned record stream when needed; subsequent updates reclaim the proven
previous owner transactionally. The separate legacy writer remains deliberately capacity-safe and
rejects expanded-only rows instead of truncating or overwriting adjacent ROM data. The graphical
ROM workspace edits every materialized word, including the expanded-only tail, through the same
revision-checked application command.
The supplied Lunar Magic 3.63 executable now independently verifies that boundary through a real
Wine `-TransferCredits` transition. It expands a headered pristine destination to 1 MiB, installs
one `$751`-byte RATS payload, and preserves the exact canonical 8,192-word model, including the 54
blank rows materialized beyond pristine storage. `credits-tilemap-file INPUT [NORMALIZED
[OBSERVATION]]` provides bounded canonical reopening and row-addressable `LMOBS1` evidence; its
built-process test also replays the exact Wine hashes, 144 changed ranges, ownership, and semantic
before/after observations.

Title-screen Layer 3 tilemaps use `LMOWLYR1`, containing both materialized 29×32 word planes.
`smw-title-tilemap-export/import` reads the pristine graphics-remap command stream or Lunar
Magic's canonical RATS-owned literal stream. Imports publish the low-bank 24-bit loader pointer,
repair the checksum, semantically reopen the result, and reclaim only a previously proven owner
during updates. The application command boundary performs the same change as one undoable project
operation. The graphical ROM workspace exposes both materialized planes by exact row and column
and installs the same canonical stream on commit.
The supplied Lunar Magic 3.63 Wine executable now contributes a reproducible
`-TransferTitleScreen` differential fixture. A pristine-to-pristine transfer expands the
destination to 1 MiB and installs one `$745`-byte RATS payload. It also proves that Lunar Magic
normalizes 518 untouched primary-plane blank words from `$00FC` to `$38FC` while leaving the blank
secondary plane at `$00FC`; pristine and installed Rust decoding now produce the same exact
1,856-word model. `layer-tilemap-file INPUT [NORMALIZED [OBSERVATION]]` provides bounded canonical
reopen and allocation-independent `LMOBS1` plane hashes for this and future title/overworld-style
tilemap evidence.

Title-screen playback recordings use the separate `LMTITL01` allocation-independent boundary.
The detected ROM path distinguishes pristine SMW from Lunar Magic's exact `$60`-byte playback
runtime, requires RATS ownership for both runtime and movement data, validates the initial
continuation and all three biased data operands, and replaces only the proven recording owner.
`smw-title-recording-export/import`, `smw-title-recording-zst-export/import`, and
`smw-title-recording-s9x-import` cover native files, exact minimal ZSNES V143 states, and both
plain and gzip Snes9x tagged snapshots. Matching application-shell commands route the same change
through a revision-checked undoable command.

Lunar Magic's fixed ROM attribution and feature metadata use the lossless `LMROMMD1` boundary.
`smw-lm-metadata-export/import` and the application shell's `lm-metadata-export/import` preserve
the exact 160-byte attribution, VRAM patch version, and 25-byte packed feature record. Imports are
SMW US revision-0 only, repair the checksum, semantically reopen the three fixed regions, and commit
as one undoable transaction. Oracle observations expose the recovered feature bits, configurations,
markers, five 24-bit runtime pointers, checksum-status nibble, and an attribution hash without
depending on copier-header placement.
The modular graphical workspace loads only metadata already present in a supported ROM and exposes
the attribution, VRAM-version, and feature-record regions as indexed lossless bytes alongside the
recovered packed summary. It requires explicit load-before-apply identity, revalidates the stable
signature and reserved checksum bits after every staged edit, rejects stale commits, and preserves
dirty changes during editor close or application shutdown. A real LM 3.63 Wine-produced fixture
proves edit, revision-checked commit, exact semantic reopen, and accepted-commit lifecycle behavior.

Expanded secondary exits use `LMSEXIT1`, preserving all six 8,192-byte logical planes. The native
order recovered from Lunar Magic is destination-low, position/method, screen/Y,
destination-high/flags, X/overworld, and additional flags. `smw-secondary-exit-export/import` and
`secondary-exit-native-export/import` expose the CLI and application workflows. Detection reads
pristine SMW's four `$200`-byte tables or LM 3.63's exact reader network, requires RATS ownership
for every variable plane, and accepts both the four-fixed-plus-two-tagged compact form and the
all-tagged form. Installed updates trim every plane to one common used length, reclaim only proven
owners, repair the checksum, semantically reopen, and commit as one undoable revision. Clean-ROM
imports now install the recovered shared Lfix3/secondary-exit runtime in the same transaction. The
profile owns the `$510`-byte relocatable Lfix3 body, initialized tables, fixed helpers, base and
extended secondary-exit runtimes, compatibility hooks, and all six reader operands. Tables whose
used range is at most `$200` use the compact form; larger tables allocate all six planes. CLI and
application imports from pristine ROMs semantically reopen, repair the checksum, preserve the input
file, and undo as one operation.

`render-overworld` joins `LMOWFULL`, the recovered 256-byte ExAnimation size-mode table,
`LM16SET1`, and `LMGFX4BP` into a deterministic PNG. The completed-reveal argument applies that
many source/destination substitutions in table order. The palette is read from `LMOWFULL`.
Optional `LMOWAPP1` data supplies `.ovssc`-equivalent multi-tile appearances keyed by the full
16-bit sprite ID, including signed offsets, palette rows, and flips. Without it, overworld sprite
records are retained but not drawn; the renderer never invents sprite graphics. Decoding and file
publication remain frontend work, while `lm-render::render_portable_overworld` owns reveal
application, appearance resolution, optional animation materialization, reference validation, and
bounded rasterization for both CLI and future graphical frontends.

`overworld-render-file SPEC` exposes that shared pipeline from the application shell. `LMOWRND1`
requires `overworld`, `size-modes`, `maximum-animation-records`, `map16`, `graphics`,
`completed-reveals`, and `output`, with optional `appearances` and `animation-frame` fields. Paths
are relative to the specification, reads are format-bounded, the size-mode table is exactly 256
bytes, and PNG publication is create-new. Both this specification and the current-document
`LMOWDRN1` form accept the same optional six-field signed-origin, nonzero-dimension, exact-zoom
camera group as complete-level previews. Camera validation is shared, and output is sampled through
the public editor viewport adapter rather than a domain-specific scaling implementation. A
built-binary `--script` test renders both portable
views through paths containing spaces and Unicode and verifies repeat execution preserves outputs.

Complete `LMOWFULL` artifacts also have an independent application lifecycle. `world-open-file`
accepts a bounded `LMOWDOC1` specification that binds the document path to its exact 256-byte
ExAnimation size-mode table and maximum record count. `world-edit-file` reuses `LMOWEDT1` and the
same staged nine-domain edit engine as native-ROM editing; the script slot must match the file's
source slot and palette ownership is validated even without a palette edit. `world-render-file`
uses `LMOWDRN1` to preview the unsaved revision, while `world-status`, `world-save`, `world-close`,
`world-undo`, `world-redo`, and `world-discard` provide revisioned document lifecycle operations.
The shared 100-state canonical history is monotonic and stale-token protected, restores saved
baselines, and invalidates redo after divergent nine-domain edits. Canonical reopen precedes
revision advancement, saves use immutable snapshots, and dirty quit or scripted EOF preserves the
underlying file.

The ten-argument form additionally accepts a versioned `LMANFRM` materialized-animation frame
between the appearance file (use `none` when absent) and completed-reveal count. This provider-neutral
artifact contains unique absolute tile and palette-color overrides for an explicit tick. Its decoder
checks exact framing, count limits, reserved fields, duplicate targets, 4bpp pixels, and BGR555 words;
application validates every destination before cloning and changing either asset. It therefore gives
oracle-backed and future verified ExAnimation interpreters one deterministic rendering boundary
without pretending that currently undocumented transfer types have already been recovered.

Level rendering follows the same evidence boundary for Layer 3. `LML3FRM1` contains a bounded,
painter-ordered set of signed tile instances, an explicit placement behind Layer 2, between Layers
2 and 1, above Layer 1, or above entities, and the SHA-256 of its canonical `LMLAY3V1` source. The
CLI refuses stale planes whose digest does not match the level bundle. Oracle-backed tools can thus
materialize native remap behavior without duplicating or guessing that interpreter in the renderer.

Expanded graphics profiles with 129 through 4,096 entries expose installed ExGFX transfer in the
native graphics editor. The standard/extended boundary is file `$080`; canonical names are
`ExGFX80.bin` through `ExGFXFFF.bin`. ROM audit requires every standard `$000..$033` pointer and
permits all-zero sentinels only in later optional slots, while every nonzero pointer retains the
normal mapping, bounds, and metadata-alias checks.
Extraction walks the immutable pointer table, skips only those sentinels, preserves decompressed
2bpp/3bpp/4bpp bytes, and publishes the complete create-new group or nothing. Import rejects
noncanonical or out-of-table ExGFX-like names, sorts the sparse slots, bounds every read, preserves
the existing raw length for replacements, and admits new slots only at the native `$800`, `$C00`,
or `$1000` byte depths. Compression, allocation, pointer writes, checksum repair, and revision-bound
publication remain one transaction; omitted slots and all pointer-table storage remain protected.

The installed graphics editor also owns a scoped external-edit round trip. It exports the current
staged controller bytes—not a stale ROM decode—to a uniquely reserved private temporary directory
under the canonical GFX/ExGFX filename. Persisted `LMTOOLS1` tools whose templates reference
`{graphics}` appear in a selector; expansion occurs only after staging and can also consume the
normal ROM, project-directory, and level context. A direct executable chooser remains as a fallback.
A second native permission window displays the executable and every exact argument before launch. The worker does
not use a command shell, waits outside the UI thread, accepts only successful process termination,
reopens only a nonsymlink regular file at the exact original byte length, and removes the private
directory before returning bytes. The completion is bound to the application revision captured at
staging and passes through the controller's normal raw-import validation, so it becomes an ordinary
uncommitted graphics edit. While pending or running, pixel mutation, other graphics file work,
commit, and close/shutdown are gated. Cancellation, launch/read/shape failures, disconnected workers,
template-expansion failures, and dropped unapproved prompts clean up without changing the controller.
On macOS, an approved executable path ending in an existing `.app` directory is routed through the
system `/usr/bin/open` tool with wait and new-instance flags; `--args` preserves each expanded
argument as a separate process value. Ordinary executable paths retain the direct launch path on
every platform, and neither route invokes a command shell.
