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
  lossless 257-byte `.palmask` selection masks with failure-atomic masked-import semantics, and exact
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
  close or quit requires confirmation. The same workspace stages clear-current only when the
  displayed index is the loaded form, and offers confirmation-gated clear-all across all 8,192
  native zero records. Clear-all rechecks the application revision when confirmation is delivered,
  reloads the selected form, and remains an ordinary dirty edit until the atomic commit succeeds.
  First-time compact installation compares each fixed-plane write against the detected pristine
  source table rather than the requested destination table. This preserves exact transactional
  preconditions while permitting clear/import operations to intentionally replace those bytes.
  A separate title-screen recording window exposes the recovered `$4..=$8000`-byte movement
  payload without interpreting unknown input commands. It detects pristine ROMs or the exact
  two-RATS-block Lunar Magic playback installation, displays installed bytes in canonical
  sixteen-byte hexadecimal rows, validates two-digit tokens and the final `$FF`, and dispatches
  the same transactional install/update command used by the CLI. Invalid text, stale revisions,
  and rejected commits retain the editable payload; dirty close and quit require confirmation.
  The window also exposes the lossless interchange codecs directly: bounded background reads
  import native `LMTITL01`, exact ZSNES states, and plain or gzip Snes9x states into the staged
  text for review, while create-new background writes export native and minimal ZSNES V143 files.
  Imports recheck the captured ROM revision before replacing staged text, exports snapshot the
  validated payload, active file work gates close/shutdown, and no imported recording reaches the
  ROM until the ordinary explicit commit succeeds.
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
  separate allocation range. Version 2 distinguishes generic ExAnimation records from original,
  level, and global animation slots while retaining version-1 decode compatibility. Fixed and
  every animation-owned tile remain previewable and copyable,
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
  low-first or high-first encoding. The GUI presents the real five-bit first and four-bit second
  encoded components, so it cannot construct holes outside the encoding's `$0F1F`/`$1F0F` mask;
  accepted edits reload the staged semantic and raw forms, while programmatic invalid targets still
  reject atomically.
  Native screen-exit objects likewise have a dedicated source-screen and destination/flags form.
  Editing follows Lunar Magic's recovered command-zero parameter-0/parameter-2 compact and extended
  representations, can change record shape without losing the unrelated new-screen bit, and reloads
  the semantic and raw controls from the accepted staged record so required-flag canonicalization is
  immediately visible. A reciprocal Lunar Magic 3.63 MWL import/re-export oracle covers both shapes.
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
  Reciprocal mode loads the exact reverse edge into separate exit/flag fields and atomically
  upserts or removes both directional keys, so strict validation never requires an impossible
  one-edge intermediate state. Toggling deliberate one-way and reciprocal modes is mutually
  exclusive and preserves every flag outside the owned one-way bit.
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
  The recovered `.ff7` compatibility action independently hides standard and extended entries
  that do not match the active object tileset and loaded BG1/FG3 files. Standard commands combine
  Lunar Magic's shared metadata with the selected five-family overlay; extended selectors use
  their dedicated 256-entry metadata and the `$7F` tileset-4/5/D override. Filtering remains
  disabled until foreground assets are loaded, and custom OSC entries retain their own external
  compatibility semantics rather than inheriting built-in requirements.
  The final two catalog-strip controls now expose Lunar Magic's separate preview surface and zoom
  menu for both objects and sprites. Each preview begins as a 256×256 logical canvas, uses the exact
  100/200/300/400/600/800-percent presets, accepts 100-percent relative steps within the recovered
  100–5000-percent bounds, and scrolls instead of forcing the surrounding level canvas to resize.
  Hiding the preview keeps its zoom and layout choices but disables those controls until the pane
  is restored. Standard and extended objects, OSC composites, standard sprites, SSC atlas parts,
  and externally remapped SSC parts all render from the currently selected placement template.
  The optional `.ffx`, `.ffx2`, and `.ffxhd` GUI bitmaps now back the level, overworld, and
  background-map canvases. They use repeat sampling at native pixel scale and are painted only in
  the portion of a resized viewport outside valid editable content. Valid game pixels are composited
  over them, and clicks in the tiled margin cannot select or mutate a level/overworld/background
  cell. The compact background preview maps selection through Lunar Magic's two 16×32 storage
  planes rather than treating its bytes as a visually row-major array.
  Authenticated level-editor user-toolbar aliases now enter the same current-level editors and
  integrated tool panels as the native menu. Graphics, ExAnimation, and Layer 3 settings retain the
  active level number; background/Layer 2, sprite-data, properties, other settings, Layer 1/2
  settings, and entrance actions restore the tool column and reopen the matching collapsed section.
  Entrance-view commands `$23F8/$23F9/$23FA/$2414` now preserve the Ghidra-recovered independent
  primary/secondary/midway flags and separate aggregate state. The live editor draws referenced
  secondary targets and the non-overlapping midway node from the same authenticated coordinate
  helpers used by the full-level raster audit.
  Shared-palette toolbar commands `$239D/$239E` directly start exact `.smwpal` export/import from
  the native shared/custom workspace. They reuse staged colors, reject a stale ROM revision or
  overlapping file job, and preserve the recovered `$7E2` legacy and `$810` expanded artifacts.
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
  The binary side retains Lunar Magic's five reserved header bytes, and new-screen markers divide
  the following variable-width records into one-or-more-object collection groups. The window edits
  complete groups (semicolon-separated records) and one-line Unicode descriptions,
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
  `copier-header-add` and `copier-header-remove` provide the same
  bounded create-new conversion from the CLI. The application equivalents consume `LMHDRAD1`
  (`input`, `output`, and either decimal `fill` or `mode lunar-magic-smw-us-v1`) and `LMHDRRM1`
  (`input`, `output`) specifications. Canonical mode identity-checks SMW-US revision 0 and emits
  the complete recovered structured header; both modes retain Unicode/space-containing relative
  paths and refuse no-op, ambiguous, or colliding conversions.
  The graphical **Convert Copier Header…** workflow operates on the open project instead. It
  displays the current physical state and unchanged logical size, adds a caller-selected exact
  fill, installs Lunar Magic 3.63's canonical SMW-US prefix, or removes and retains all 512 existing
  bytes, and enters that physical-prefix conversion into ordinary revisioned history. Undo restores
  nonuniform removed headers byte-for-byte, redo is compare-guarded, dirty/save state includes
  header-only changes, and pending saves or stale dialogs cannot mutate the document.

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
`file`, `length-selector`, and `offset-selector` fields plus an optional exact 32-bit
`expanded-mode`. Omitting the new field preserves the current mode for backward compatibility.
The document controller and native MWL form apply the complete setting as one typed change; mode
editing replaces only the high nibbles of words 8–15, preserving their adjacent low 12-bit fields
and all unrelated bits, words, and sections through canonical reload, undo/redo, recoverable save,
and dirty-close protection.
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

The workspace declares Rust 1.85 as its minimum supported version. Editor/model crates forbid
unsafe code. The optional `lm-libretro` executable is a separate process with an audited unsafe
libretro ABI boundary; the safe editor exchanges only bounded `LMEMU001` records with it.

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
cargo run -p lm-cli -- palette-mask-file palette.palmask normalized.palmask palette-mask.obs
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
The installed complete-overworld workspace also materializes the native SMW overworld graphics
slots `GFX1C` through `GFX1F` in their four consecutive `$80`-tile VRAM regions and pairs them
with the profile-decoded complete Map16 set. Its exact staged nine-domain aggregate is rendered as
a nearest-neighbor map canvas; clicking a rendered tile updates the same Layer 1/Layer 2 coordinate
and hexadecimal Map16 form used by the typed edit transaction. Event-reveal preview count and every
successful edit invalidate the raster key, while render failure leaves the property editor usable.
The installed canvas can also retain a visually selected Map16 brush and paint either layer by
clicking or dragging. Bresenham traversal covers intermediate grid cells when pointer samples skip
coordinates, unchanged cells are omitted, and each sampled segment is submitted as one ordered
typed controller batch. Rectangle painting normalizes either drag direction, retains the original
pointer-down cell across the drag threshold, and emits row-major edits. Four-connected flood fill
validates the exact rectangular layer shape, never crosses a different 16-bit tile value, and
orders the bounded result deterministically before the same atomic edit boundary. A collapsible
16-by-16 visual page picker uses the live overworld palette,
native graphics atlas, and complete decoded Map16 set; selecting a cell updates the exact 16-bit
brush value, while pages that reference graphics outside the overworld VRAM remain unavailable
without disabling hexadecimal selection or property editing.
The opt-in cross-platform Snes9x suite now exercises the same nine-payload project serializer as an
aggregate launch-safety gate. It expands a verified pristine ROM, writes private extension pointer
tables, allocates all nine tagged payloads, repairs the checksum, semantically reopens the complete
aggregate, writes a temporary ROM, and requires Snes9x to remain alive for eight seconds before the
guard terminates and reaps it. Those smoke-only pointer tables intentionally remain outside the
gameplay runtime, so this is evidence for transaction/container/emulator initialization safety and
not yet input-driven proof that the game rendered the edited map.
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
`last-screen`, `mode`, `background-color`, `sprite-tileset`, `music`, `time`, `sprite-palette`,
`foreground-palette`, `object-tileset`, and `layer1-scroll`; numeric arguments are hexadecimal.
The four scroll values `0..=3` are checked strictly rather than masked. The explicit search
range is converted into a bank-aware policy that protects all 16 profile tables and the complete
64-byte internal-header/vector block. The command decodes through `LevelController`, stages a typed edit,
allocates and repairs the checksum on a private image, then dispatches the prepared mutation
through the authoritative revision check. It is therefore undoable and cannot bypass normal
dirty-state/save handling.

For complete native level-controller batches, `level-edit SCRIPT SEARCH_START SEARCH_END` reads a
bounded UTF-8 `LMLEDIT1` script. For the authenticated North American SMW revision-0 LoROM layout,
this command shares the native GUI's built-in fallback and does not require an external revision
profile. It detects the sprite-pointer representation and expanded framing, protects the Layer 1
pointer table and internal header, keeps pristine shared-bank sprite relocation inside its original
bank, repairs the checksum, and honors the caller's bounded Layer 1 allocation range. Other ROM
identities still require an audited profile rather than accepting US offsets speculatively.
It supports all eleven recovered header fields, including strict
`header layer1-scroll VALUE`; object
insert/replace/remove/move plus typed command-ID, parameter, coordinate-nibble, and screen-advance
edits and exact packed screen-jump targets; raw sprite-header replacement plus semantic
`sprite-properties MEMORY BUOYANCY_1 BUOYANCY_2` editing;
and native sprite-token
insert/replace/remove/move for raw records, expanded screen changes, and expanded control tokens.
`custom-time VALUE FORCE_RESET` accepts a hexadecimal 12-bit timer plus canonical boolean and
`custom-time disabled` removes the bypass. The typed constructor rejects `$1000+` and the
non-persistable zero-without-force form before an edit escapes; serialization uses the staged
header's horizontal or vertical nibble order and collapses existing command-`$28` duplicates only
when this explicit semantic edit is applied.
Semantic sprite properties accept memory `$00..=$12` plus canonical booleans and preserve the
serializer-owned expanded-framing bit `$20` from the staged stream. Invalid memory and boolean
values reject before application, while controller-side validation keeps direct callers atomic.
Canvas-grade absolute object edits are available as `object place RECORD SCREEN FIRST SECOND
PERPENDICULAR_HIGH` and `object relocate-position INDEX SCREEN FIRST SECOND PERPENDICULAR_HIGH`.
The record/screen/coordinates are hexadecimal, indexes are decimal, and the high-coordinate flag
is a canonical boolean. Scripts reject screens above `$1F` and coordinate components above `$0F`
before controller mutation. The shared relocation model rejects command-zero controls, preserves
ordinary extension bytes, sets the perpendicular bit explicitly, stably orders objects by absolute
screen, and regenerates the minimum owned advance/jump transitions while retaining trailing opaque
controls.
Named ordinary-object fields use `object fields INDEX COMMAND PARAMETER SCREEN FIRST SECOND
PERPENDICULAR_HIGH`. The index is decimal; command, parameter, screen, and coordinate fields are
hexadecimal; and the final value is a canonical boolean. Command `$00..=$3F`, screen `$00..=$1F`,
and coordinate-nibble bounds are enforced by the parser and again by the shared
`set_ordinary_fields` transaction. That transaction preserves extension bytes, rejects
command/parameter changes that alter native record width, applies the absolute position and
perpendicular high bit together, canonically regenerates transitions, and returns the reordered
record index so installed and portable forms retain selection.
Canvas-grade sprite edits use `sprite place RECORD SCREEN X Y` and `sprite relocate-position INDEX
SCREEN X Y`. Records, screens, X, and Y are hexadecimal; indexes are decimal. Both commands route
through the same absolute-position model as installed and portable canvas drag/drop. Screens are
limited to `$1F`, X to `$0F`, legacy Y to `$1F`, and expanded Y to `$0FFF`. The model preserves the
sprite number, extra bits, and extension bytes, stably sorts legacy records by screen, and rebuilds
expanded upper-Y transitions with the orientation-aware comparator. Invalid records, coordinates,
indexes, controls, or revision-table width changes reject the complete edit batch atomically.
Named record-field edits use `sprite fields INDEX Y_LOW EXTRA_BITS SCREEN X SPRITE_NUMBER`.
The index is decimal and all five fields are hexadecimal, bounded to their native packed widths
(`$1F`, `$03`, `$1F`, `$0F`, and `$FF`). This command shares `set_record_fields` with both native
GUI workspaces: it preserves custom extension bytes and an expanded record's current upper-Y band,
rejects revision-table width changes before mutation, then performs the same stable legacy or
orientation-aware expanded reorder as canvas relocation. GUI selection follows the edited record
when that reorder changes its token index.
Existing screen-exit objects can be edited semantically with `object screen-exit INDEX SCREEN
DESTINATION_AND_FLAGS`. The decimal record index selects a recognized command-zero exit, while the
source screen and 16-bit destination/flags value are hexadecimal. The shared typed object edit
rejects source screens above `$1F` and non-exit records atomically, unconditionally adds Lunar
Magic's required `$0400` flag, preserves the unrelated advance-screen bit, and canonically changes
between the four-byte parameter-0 and five-byte parameter-2 shapes.
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
object screen-exit 2 1f bcde
object place 090855 1f 0c 0b true
object relocate-position 0 1e 0a 09 true
object fields 0 22 55 1d 0c 0b true
sprite place 080047 1f 0c 009d
sprite relocate-position 0 1e 0a 008f
sprite fields 0 1c 02 1d 0b 47
sprite-header 10
sprite-properties 12 true false
sprite insert 1 screen 12
sprite insert 2 control 90
```

The selected level's pristine-table main entrance and optional installed separate-midway record
have a distinct bounded `entrance-edit SCRIPT` route because they are owned by the entrance
transaction rather than the relocatable Layer 1/sprite streams. `LMENTR1` accepts exact four-byte
`main POSITION VERTICAL SCREEN_METHOD MODE_SCREEN`, semantic `layer2-scroll TABLE`, and installed
`midway FLAGS POSITION ADDITIONAL_FLAGS HIGH_POSITION` commands. Ordered controller batches stage
privately: a late scroll index above `$0F` or a midway command without an authenticated installed
table preserves every earlier entrance edit. The Layer 2 preset changes only the high nibble of
the staged main-position byte. Main-only scripts do not probe the optional midway runtime, while a
midway command requires strict installed-hook/table authentication. The physical four-plane layout
uses the detected LoROM or ExLoROM mapper so expanded projects do not fail the native editor's
entrance controller on a stale pristine-layout mapper tag.

The complete 8,192-entry native secondary-exit table has a separate `secondary-exit-edit SCRIPT`
route with bounded `LMSEXED1` input. Ordered `set INDEX DESTINATION POSITION SCREEN X Y
DESTINATION_FLAGS X_FLAGS ADDITIONAL`, `clear INDEX`, and `clear-all` commands operate on one
private detected-table clone. Indexes are limited to `$0000..=$1FFF`; the complete staged table is
then passed through native six-plane validation, which enforces destination `$0000..=$1FFF`,
screen `$00..=$1F`, X/Y `$00..=$0F`, and the recovered flag-bit ownership. Only after that check
does the existing application command install or update the authenticated runtime in one
revision-bound checksum-repaired transaction. Thus a late malformed entry cannot publish an
earlier clear or edit, and `clear-all` can be followed by selected replacements in one operation.

The whole script stages on a cloned controller. Both object and revision-sized sprite streams are
encoded and reparsed for exact equality before commit, closing the gap where universally bounded
raw records could still disagree with their native command/length tables. A late command,
noncanonical record, allocation failure, or stale revision leaves the application unchanged.
Typed object edits reject values that would change the recovered encoded record length or collide
with the stream terminator; callers must explicitly replace the whole raw record in those cases.
The coordinate pair remains orientation-neutral at the record boundary: absolute X/Y requires the
level layout and preceding screen-transition stream, so it is not guessed from an isolated record.
Command-zero records with parameter `01` or `03` are classified as Lunar Magic's two recovered
screen-jump encodings and expose their exact packed target. Live horizontal and vertical imports
with nonzero five-bit and four-bit components prove that Lunar Magic adds the two values to resolve
the absolute major-axis screen regardless of their low-first or high-first storage order. Placement,
extent, relocation, import normalization, rendering, and canvas sizing share that interpretation
while the raw packed target remains lossless and independently editable. Both complete-level and
native-level semantic observations publish the packed target and resolved screen independently, so
release evidence detects regressions in either representation or interpretation.
The low-first horizontal form uses Lunar Magic's recovered `$1B0`-cell screen stride and `$200`-
cell row stride rather than plain component addition; the high-first vertical form uses equal
`$200` strides. Thus the maximum live pair `$1F/$0F` resolves to `$30`. Lunar Magic preserves that
raw jump and its following object on MWL import/re-export but excludes a nonadvancing out-of-range
object from automatic extent, leaving last-screen `$00`. If the following object advances, the
five-bit primary cursor wraps first and the same layout mapping places its rendered cell on screen
`$11`; Lunar Magic retains both raw records and stores last-screen `$11`. Rust now applies those
same cursor, visibility, and lossless-normalization rules while stored/raw extent remains bounded
to 32 screens. Both native jump editors show the resolved screen and mark values beyond `$1F` as
retained lossless data rather than silently presenting them as valid canvas locations.
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
4bpp files. Like palettes, a script declares the complete tile ownership shape and any fixed,
generic ExAnimation, original-animation, level-ExAnimation, or global-ExAnimation overrides before
editing. Each tile is written as exactly 64 hexadecimal pixel
nibbles in row-major 8×8 order; `changes` performs a unique indexed batch and `range` replaces
contiguous tiles:

```text
LMGFXED1
owners 3 editable
owner 0 fixed
owner 2 original-animation 007e
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
`Layer3ExpandedModeFlags` additionally retains the exact 32-bit value packed from the high nibbles
of words 12–15 and 8–11. Its typed row resolver reproduces the recovered enable and level/tileset
gates, split 11-bit sign extension, type-specific twelve-row bias, and row-30 clamp selection.
`observe-expanded-settings` publishes both the packed value and enable bit so retained ROM records
can be compared without assigning meanings to the unresolved slot-assignment and painter flags.
The installed preview and image-export provider now resolve that row state against the active
Layer 3 setting and object tileset. An enabled override supersedes the vanilla editor position;
ordinary types translate the repeating plane, while clamped types perform target-cell-to-source
lookup and repeat both 8x8 source rows belonging to editor row 30 for every later 16-pixel cell.
`Layer3ExpandedComposition` additionally exposes the authenticated dispatcher subset when supplied
with Lunar Magic's active level-mode composition byte: primary versus alternate source route,
additive composition, and source half-color. It first applies packed bit 30 as an exact replacement
for that byte's Layer 3 input bit, then evaluates the recovered sign and `$44`/`$60` masks.
`observe-expanded-settings` exposes bit 31's alternate route and bit 30's primary additive input.
`lunar_magic_level_layer_slots` retains the three complete 32-entry tables captured from a live
Lunar Magic 3.63 process and reproduces `ConfigureLevelLayerSlotAssignments` as five typed slots,
including low/high Layer 3 priority splitting from legacy-header byte 2 bit 7. Installed rendering
now walks those slots in painter order and applies the slot's exact Layer 3 additive and half-color
state; half-color is applied to the source channels before either opaque or saturating-additive
composition. The same slot flags now drive Layer 1/2 whole-layer addition through the shared native
rasterizer. It renders only the final placement for each Map16 cache coordinate, saturating-adds
ordinary source channels, and halves averaged/half-color source channels before addition exactly as
`RenderMap16TileToPixelBuffer` does while `DAT_0060028D` is active.
The retained `level-layer-slots/slot-arrays.tsv` oracle invokes the real dispatcher for every valid
mode, both legacy priority states, the unmodified table state, and all four packed bit-30/31
combinations. Its 200 rows compare every byte of all five slot arrays against the Rust model.
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
script uses the `LMXSETED1` header followed by raw or semantic commands. `word INDEX VALUE` retains
the original hexadecimal exact-word route. `layer3-tilemap ENABLED FILE LENGTH DESTINATION`
accepts a canonical boolean, twelve-bit hexadecimal GFX/ExGFX file, and selectors `0..=3`;
`layer3-mode PACKED` accepts the exact 32-bit hexadecimal expanded mode,
`super-gfx ENABLED FG1 FG2 FG3 BG1 BG2 BG3 SP1 SP2 SP3 SP4` accepts all ten twelve-bit bypass
files, and `boundary-air ENABLED` changes the recovered interaction bit. Commands declare exact bit
masks rather than whole shared words: tilemap owns word 0 `$2000` plus word 1, bypass owns word 0
`$8000` plus the low twelve bits of words 2–11, expanded mode owns the high nibbles of words 8–15,
boundary interaction owns word 8 `$4000`, and raw writes own all bits of their selected word. This
allows tilemap, bypass, and mode to compose while mode versus boundary and any intersecting raw
write reject before mutation.
Any true overlap, out-of-range word, invalid boolean, file, or selector rejects during parsing before an
edit escapes. Resolution starts from the current record, preserves every unowned bit/word, and is
shared by installed shell editing, portable-document history, and aggregate ROM automation. This
workflow does not claim to discover or install Lunar Magic runtime patches.
The standalone native editor now exposes every proven projection over that exact record: custom
Layer 3 tilemap enable/descriptor, exact 32-bit expanded mode, all ten Super GFX Bypass files, and
sprite boundary interaction. Each button derives its bounded word batch from the currently loaded
exact form, commits through the same document controller, and immediately reloads canonical state.
Mode changes preserve the eight adjacent low 12-bit fields, bypass changes preserve word 1 and
words 12–15, and boundary changes preserve every bit except word 8's `$4000` flag.
The focused installed-ROM exact-record editor exposes the same projections and stages their shared
form batches through `ExpandedSettingsController`. Its staging helper independently rechecks the
captured project revision even though stale buttons are disabled, then reloads the canonical record
after every accepted action. Commit remains one checksum-repaired application mutation with exact
semantic reopen and whole-ROM undo.
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
`expanded-settings-edit SCRIPT` applies the same bounded semantic `LMXSETED1` format used by
standalone documents as one duplicate-free native transaction; a bad late command leaves every
word and the application history unchanged.

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
`$087FF8` is an allocation search start rather than an ownership signature. Lunar Magic's retained
ordinary level-save ROM places an unrelated `$8000`-byte RATS payload there before the expanded
settings runtime exists. Detection therefore treats a valid wrong-sized block as unrelated only
when every fixed expanded-settings runtime destination still equals its authenticated pristine
precondition. Once any member of that runtime family is installed or modified, a wrong-sized owner
continues to reject. This distinction lets modified level-save ROMs use pristine expanded-setting
defaults without weakening the damaged-installed-runtime gate.
The cross-platform GUI exposes five recovered installation paths through a revision-bound
**Install Built-in Runtime** dialog. It can install the expanded-settings family alone, the
complete Layer 3 family (which includes expanded settings), or the expanded shared/custom-palette
runtime with its 512-entry per-level pointer table. The fourth path promotes the complete Lfix3
core from an indirect prerequisite to an explicit native transaction: it consumes the bundled
pre-relocation template, applies all 107 typed self-relocations, allocates the `$510`-byte runtime,
publishes the fixed helpers and ten identity-checked hook sites, initializes all three 512-entry
shared tables, and repairs the checksum as one application revision. The dialog displays the exact supported
identity, rejects a stale project revision, and closes only after application acceptance. Native
tests route every selection through the same application commands, semantically reopen the
installed subsystem, and prove exact input restoration with one undo.
Current Lfix3 detection authenticates the runtime rather than trusting its fixed marker. Either the
marker or primary hook activates strict validation of both fixed helpers, all immutable hooks, and
the exact `$510`-byte RATS owner reached by the primary long address. The detector rebuilds every
hook operand and all 107 internal relocations for that actual address before comparing the complete
runtime payload. It deliberately excludes the three 512-byte tables from byte equality because
normal level edits change them. Marker-only, damaged-marker, modified-hook, malformed-owner, and
modified-runtime states reject; authenticated current installs disable duplicate installation, and
installed secondary-exit edits authenticate this shared prerequisite before mutation.
The migration coordinator's three-way signature probe is now modeled separately from full current
authentication. Generation 3 requires the exact `$0111` marker/helper plus the authenticated
runtime; generation 2 is selected by a JSL at logical `$02DA17`, and generation 1 by a JSL at
`$02D7CE`. Authentic Lunar Magic 3.01 output proves generation 2 retains that generation-1 hook,
so generation 2 is classified by exact `$240`-byte RATS-runtime, fixed-helper, marker, and relocated
hook authentication instead of treating simultaneous hooks as ambiguous. Generation 1 now requires
both its exact JSL and complete `$30`-byte helper at `$02DC50`. Its recovered 512-entry conversion
moves packed bit `$10` into a new plane and clears it only when the legacy flag's `$20` bit is clear.
Generation 1 now migrates through a strict current-runtime plan. Its immutable hook/helper identity
is checked first; every later fixed write and new table destination must retain the exact pristine
precondition installed over by Lunar Magic's migration callees. The live `$02DE00` plane is bound
as an exact source precondition, converted with the recovered 512-entry loop, and paired with the
new `$037C00` plane while `$037E00` receives `$1A` defaults. The application and native dialog
route the single checksum-repaired transaction, current authentication verifies the result, and
undo restores the complete generation-1 image.
Generation 2 now migrates through an explicit owned-block replacement: the old `$240` RATS block is
validated and reclaimed only in staging, its free run is reused for the current `$510` runtime,
all three live tables are rebound as exact precondition-preserving writes, and checksum plus every
hook commits in one undo batch. Late failure exposes neither reclamation nor history. The
application and native dialog route that transaction.
The adjacent Sprite 19 user-requested fix is recovered from Lunar Magic command `$26AC`, its
`PromptAndInstallSprite19AsmFix`/`InstallSprite19AsmFixRuntime` control flow, and a matched
pristine-ROM Wine transaction. On SMW-US-v1 it replaces the six-byte hook at logical `$00E762`,
installs the fixed `$20`-byte helper at `$01BCA0`, and removes the old three-byte branch at
`$0020A0`. Detection distinguishes pristine, authenticated shared-helper-only, and complete forms;
the middle state receives only the final branch removal, matching Lunar Magic's reuse path. Every
other partial or modified combination rejects before mutation. Application, CLI, and native dialog
routes authenticate after one checksum-repaired transaction and preserve exact undo.
The fifth route promotes the Wine-authenticated pristine Map16 runtime transformation to the same
native transaction boundary. It applies all recovered fixed changes under exact byte
preconditions, places the exact `$8000`-byte auxiliary payload at the recovered aligned expansion
location, relocates its low-bank operand through a typed fixup, repairs the checksum, and reopens
the installed secondary Map16 model before acceptance. This route intentionally covers pristine
installation only; the four named compatibility-stage detectors and migrations remain separate
evidence-gated work.
Current Map16 runtime detection uses the complete recovered patch as its authentication boundary,
not the historically convenient `$22` marker alone. It derives every fixed replacement byte from
the embedded IPS evidence, excludes only the checksum field and the one typed bank relocation,
resolves that operand through LoROM mapping, requires an exact `$8000`-byte RATS owner, and compares
the complete auxiliary payload. The application and CLI both require this check after installation;
the native dialog recognizes an authenticated current install and disables duplicate installation,
while marker-only, modified-hook, malformed-owner, and modified-payload states reject explicitly.
Complete Layer 3 installation also detects an already-valid expanded-settings allocation. In that
state it reuses the prerequisite and installs only the five missing Layer 3 allocations, avoiding
the guarded-hook collision that would otherwise follow selecting expanded settings first. The
settings-only snapshot and pristine source remain separately reachable through two exact undo
steps.
The twelve copied current-runtime blocks now also have a byte-level oracle that permits differences
only in recovered relocation/configuration spans. They are independently generated in the Rust
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
The expanded-settings installer also has a strict generation-1.02 replacement route. It recognizes
only marker `4C 4D 02 01`, resolves the legacy `$6E00` allocation through descriptor `$173+$33`,
authenticates the exact RATS owner and complete fixed runtime family including the retained
historical `$220` compatibility body, then applies the recovered reference-only upgrade to the 512
ordinary records. The eight trailing records are retained exactly. The old owner is reclaimed and
the current allocation and runtime are installed through `replace_relocatable_patch`, so allocation,
fixed-write, checksum, or reopen failure publishes nothing and successful migration is one
byte-exact undoable revision. Synthetic corruption tests and an externally supplied Lunar Magic
2.42-created ROM exercise the same migration function; the external ROM remains outside the
repository.
Generation 1.01 is a separate pre-Layer-3 route rather than an alias for 1.02. It requires the
legacy graphics marker at `$06FF37`, an absent current marker, an exact `$6D00` owner, and SHA-256
authentication of all twelve runtime destinations plus every base helper, pointer publication, and
hook after canonicalizing only six proven relocation operand groups. The live base and record-table
addresses are rederived through LoROM and checked before hashing. Its 512 records receive the
reference-only conversion; a current default allocation supplies the eight historically absent
special slots. Both the original `$0801E0` placement and a forced `$088000` relocation from real
Lunar Magic 2.22 saves pass replacement, checksum/reopen, and exact undo.
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
That ROM window now also renders the recovered custom Layer 3 tilemap controls already present in
the portable editor: enablement, twelve-bit GFX/ExGFX file, requested-length selector, and
destination selector. Applying the semantic form changes only word 0 bit `$2000` and exact word 1,
then reloads both semantic and raw controls from the staged record so a later raw apply cannot
silently restore stale values.
The same menu now exposes explicit Level and Layer 3 navigation entries instead of leaving the
existing `ShowLayer3` application command reachable only from the terminal shell.
The profile-qualified ROM workspace can also open a native level-assets editor for the selected
level. It reuses the aggregate domain panels while retaining a `NativeLevelAssetsController`
against the immutable application revision. Object/sprite, palette, ExAnimation, and optional
expanded-settings edits are staged together. Its Settings tab exposes the same custom Layer 3
enable/file/length/destination form and submits one typed aggregate edit; installed and portable
aggregate history preserve every word beyond the two owned fields. Profiles with Layer 2 add a dedicated tab for
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
The same native editor now exposes Lunar Magic's recovered legacy current-page pair for editable
foreground pages `$02–$7F`. It prefix-reads up to `0x800` bytes of `Map16Page.bin` definition data
together with up to `0x200` bytes from the automatic same-stem `Map16PageG.bin` Acts-Like sibling.
Short planes overlay the current page suffix, trailing bytes are ignored, and a missing `G` sibling
retains the current Acts-Like plane while still applying the definition prefix.
same worker supports Lunar Magic's complete legacy planes: `Map16FG.bin` contains `0x40000`
foreground definition bytes, `Map16FGG.bin` contains `0x10000` foreground Acts-Like bytes, and
`Map16BG.bin` contains `0x40000` background definition bytes. Complete foreground publication is
an all-or-nothing create-new pair; background publication is create-new, and every import captures
the initiating ROM revision before bounded prefix loading. Complete legacy planes share the same
short/trailing/missing-companion behavior as the current-page pair.
The selected page and application revision are captured before bounded background loading; completion
decodes all 256 tiles and submits one complete-page replacement through the existing set-wide graph
validation. Built-in pages `$00–$01` and background pages reject this foreground-only boundary.
Export snapshots the selected staged page and publishes both exact files create-new and
all-or-nothing. Active legacy or complete-file I/O gates Map16 mutation, bitmap import, ROM commit,
and close.
The modern selected-range `.map16` route now follows the recovered
`WriteSelectedMap16ExportFile`/`ReadMap16ImportFile` structure rather than treating every LM16 file
as complete. The 64-byte header retains rectangle width/height, column, and band-relative row;
flags 2/4/8 preserve the `$0000–$3FFF`, `$4000–$7FFF`, `$8000–$BFFF`, and `$C000–$FFFF`
namespace ambiguity exactly. Compact definition and Acts-Like sections contain one row-major record
per selected tile. Native export anchors a hexadecimal width/height rectangle at the selected tile;
native import can restore the file origin or use the destination captured when loading starts.
Row wrapping, namespace overflow, complete-container substitution, malformed semantic lengths, and
stale completion reject before staged mutation. Built-in graphics `$0000–$01FF` remain protected
and background Acts-Like words canonicalize to zero. Original prompt gestures and a retained Wine
selected-range fixture remain oracle gaps.
Installed bitmap-to-Map16 graphics lookup is profile-routed rather than SMW-US hardcoded. Lunar
Magic's ROM-open path loads the complete 64-byte object-tileset assignment table from active
ROM-layout descriptor field `+0x94` into the live 16-by-4 workspace. Revision profiles therefore
carry `graphics.object_tileset_assignments_offset`; allocation and ROM audit protect its entire
logical span, and bitmap sessions reject an absent declaration. The direct pristine SMW-US route
continues to use its separately authenticated fixed table. The complete profile-backed import now
crosses all 48 supported identity, map-mode, copier-header, and starting-checksum forms with real
palette/GFX/Map16 mutation, checksum repair, semantic reopen, logical header equivalence, and exact
physical Undo/Redo. Its checksum target comes from the authenticated identity's internal-header
offset rather than the SMW-US constant.

Complete structured `.map16` loading uses the same asynchronous safety boundary: the editor records
the application revision only after the bounded loader accepts the request, consumes that token once
when the result arrives, and rejects the decoded file before controller mutation if the staged ROM
revision no longer matches. Opening or clearing a workspace also clears any pending token.
Native bitmap PNG and system-clipboard loading share a single start-bound request token. It records
the application revision plus the parsed level, first Map16 tile, optional graphics slots, and
palette row only after the selected worker starts. Worker completion consumes that token exactly
once and rejects a changed ROM revision before constructing the import session, so edits to visible
controls cannot silently retarget an in-flight image. Active image loading or an open bitmap preview
gates ordinary Map16 edits, legacy/complete transfers, another bitmap request, and ROM commit.
Loading blocks close; an open preview instead participates in the existing discard confirmation.
All three Map16 editing surfaces now correlate asynchronous system-paste delivery with the tile that
requested it. The installed ROM editor stores its source revision and full page/tile address; the
portable page and complete-set editors store their monotonic document revision plus local tile or
full address. Selection changes cannot retarget delivery, stale portable revisions reject without
mutation, unsolicited paste events are ignored, and opening or clearing a workspace drops a pending
target. Successful delivery still uses each surface's existing complete-tile controller edit.
The installed Map16 surface additionally implements Lunar Magic's rectangular `Lunar Magic 16x16
Tiles` clipboard boundary. Its exact 0xA0-byte header records three canonical section offsets,
selected count, width, height, source Map16 index, and the legacy alternate-word-order flag;
row-major sections carry eight-byte definitions, two-byte Acts-Like values, and four-byte source
indexes generated with the native 16-column stride. Native copy snapshots the configured rectangle
at the selected tile. Paste captures the destination, ROM revision, and monotonic staged revision
before requesting clipboard delivery, then submits the complete rectangle through one existing
validated replacement edit. Page-row wrapping, workspace overflow, malformed or noncanonical
headers, stale delivery, protected built-in graphics, and background behavior all reject or
canonicalize at the same established boundaries. Direct Map16 level-object creation from these
dimensions remains a distinct unfinished interaction.
The installed ROM Map16 workspace additionally wraps every successful staged controller mutation in
a bounded 100-entry complete-set history. Manual subtile/Acts-Like edits, clipboard replacement,
legacy page-pair import, and complete structured import each produce at most one predecessor
snapshot; semantic no-ops produce none. Undo and redo restore the exact set through one validated
complete replacement, advance a separate monotonic staged revision, invalidate pending clipboard
delivery, and preserve the opposite history. A divergent mutation clears redo. Opening, clearing,
or committing the workspace discards this staging history, while the resulting ROM commit retains
the application's existing single checksum-valid undo transaction.
The pristine/native SMW controller no longer bypasses the complete-set mutation boundary used by
the profile controller. Replacement, subtile, and Acts-Like commands now honor their supplied
resolution limit and validate the resulting complete Acts-Like graph before publication. Missing
targets or multi-node cycles therefore leave the entire ordered edit batch unchanged; background
replacement tiles still canonicalize their non-semantic Acts-Like field to zero before validation.
Palette mode now has a ROM-backed swatch editor over the profile-declared native palette. It
displays the exact retained BGR555 word beside the platform color picker, stages changes through
`PaletteController`, and retains controller ownership checks. Like the other relocatable native
assets, committing requires an explicit logical-PC allocation range and derives protected metadata
from the active profile; the compressed/tagged payload update, pointer write, checksum repair, and
application history entry remain one stale-revision-checked transaction.
The ROM palette adapter separates its swatch coordinator from bounded `LMPALOWN` acquisition,
profile decoding, dirty-close lifecycle, and shared allocation/reclamation commit construction.
Malformed or stale evidence therefore never reaches the interactive controller workspace.
Its native transfer panel now also accepts and create-new exports the recovered raw 257-word,
version-2 TPL, and RGB24 formats without blocking the UI. The retained LM 3.63 installation fixture
proves that installed word 0 plus words 2 through 256 are supported-file words 0 through 255, while
installed word 1 remains the separate backdrop. TPL/RGB import preserves that backdrop and clears
only supported row-zero entries 16 through 240, matching the live import/re-export artifact. RGB
imports retain the detected high-bit or replicated-bit expansion convention for reciprocal export.
Every route automatically discovers and optionally consumes the exact lossless 257-byte
same-basename `.palmask` selector, matching the bundled 3.63 help and executable extension string.
Only a missing sibling is ignored; malformed, non-regular, oversized, or unreadable siblings reject
the import. Supported-file selectors use their natural 256-color order: entry 0 addresses installed
word 0, entries 1–255 address installed words 2–256, and selector entry 256 has no supported-file color to address.
Thus the separate installed backdrop remains unreachable and retained. Masked RGB expansion
detection considers only selected triplets, and only selected supported row-zero entries are
cleared. All routes compute only actual word differences before the controller's immutable
ownership check, so a late protected color rejects the complete staged import; revision, worker,
commit, and close gates remain shared.
The editor retains that selector as transient workspace state, initially enables it completely on
each ROM palette open, draws disabled colors with `X`, and reproduces the documented click and
Alt-click whole-row gestures plus enable-all/disable-all controls. Its 257th entry forms a bounded
one-color final row. Exports with any disabled entry snapshot and create-new publish the selected
palette format plus its same-basename `.palmask` as one rollback-safe group; all-enabled exports
publish only the palette and recoverably remove an existing regular `.palmask`, matching the
LM 2.42 changelog fix for stale masks. A palette collision restores that sidecar and publishes
nothing; masked exports likewise replace neither destination.
The installed palette surface also carries exact 16-word palette rows in its existing typed system
clipboard envelope. Ctrl+left/right performs color copy/paste and adding Alt addresses the complete
aligned row, with equivalent explicit controls. The standalone 257th installed word is not treated
as a row. Paste requests retain their color/row target, are cleared across workspace lifecycles,
reject stale or import-locked delivery, and submit the row as one ownership-atomic edit batch.
The installed level-assets and both overworld palette panels reuse that same row envelope and
gesture/control model, returning one aggregate ownership-aware edit for all sixteen changes. The
shared/custom editor applies the row by decoding its exact legacy or expanded backend once, so the
operation is all-or-nothing and expanded auxiliary bytes remain untouched; stale/import-busy paste
delivery is rejected and clipboard targets clear when its workspace closes.
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
The installed-SMW compression coordinator also retains Fast-LoROM `$30` as a distinct verified
header value while using the shared LoROM address transform. A fresh Lunar Magic 3.63
`-ChangeCompression ... LC_LZ3` oracle and `-ExportGFX` round trip cover headered and headerless
physical inputs. Rust produces one identical logical LZ3 result, retains or omits the copier prefix
to match each source, keeps all 52 decoded files equal to the original oracle, repairs the checksum,
and restores each exact physical input with Undo. Lunar Magic subsequently reports both Rust
results as already LZ3 and exports the same 52 files.

The compression coordinator now also follows Lunar Magic's active-body convention for 8-MiB
ExLoROM. Runtime metadata/hooks, all three ordinary pointer planes, GFX33/GFX32 startup operands,
compressed ExGFX pointers, ROM-size metadata, and installed event-tilemap hooks are addressed in
the relocated upper SMW body; the inactive lower compatibility mirror is not rewritten. ExLoROM
fixups retain mapper-significant bank bits instead of applying LoROM's low-bank alias. An authentic
Lunar Magic LZ2/LZ3 pair proves every one of the 52 standard files and both populated ExGFX files
decode identically across the transition. Rust performs same-size LZ2→LZ3→LZ2 replacements with
valid identity checksums, and two Undo operations restore the exact original ROM. The application
path exposes the transition as one revision and one Undo step, while original Lunar Magic reopens
the Rust LZ3 result and re-exports all 54 streams byte-exactly. SA-1 and historical runtime-family
variants remain separate compression gates.

The authentic expanded SA-1 Pack family is now covered for standard graphics plus populated
ordinary ExGFX. SA-1's LZ2 decoder is an allocation-relative subroutine inside the authenticated
`$4806` owner rather than a standalone RATS block; both observed metadata forms (`0` and `1`) bind
to that exact owner/addend/runtime checksum. Its LZ3 decoder is a distinct 780-byte `LM 01 01`
runtime, not the 683-byte LoROM/ExLoROM payload. Mapper-aware plans preserve SA-1 pointer banks,
reuse proven same-size ownership, and leave the original embedded LZ2 owner intact. A retained
2-MiB 4bpp SA-1 project proves all 52 GFX files plus `ExGFX80` survive the Rust migration, the
checksum remains valid, and Undo restores the exact copier-headered source. Original Lunar Magic
reopens the Rust result and re-exports both directories byte-exactly.

SA-1 installed overworld-event streams are now part of the same codec transaction. An authentic
`-TransferOverworld` capture establishes that SA-1 retains the fixed loader/hook locations and
instruction skeleton while rebasing the runtime RAM operands from `$7E/$7F`, `$13xx`, `$0Dxx`, and
`$1Fxx` into the Pack's `$40/$41`, `$73xx`, `$6Dxx`, and `$7Fxx` workspaces. The mapper-aware
locator authenticates those immutable operands while continuing to exclude the six allocated
stream-pointer bytes. Retained LZ2/LZ3 captures decode to identical 92-entry buffers. Rust migrates
both owned streams to LZ3 in the same replacement as the decoder and graphics, reopens the exact
buffers, validates the checksum, and restores the complete physical LZ2 image with Undo. A
non-installed SA-1 Pack runtime remains a typed pristine/no-stream state without being compared to
LoROM's replaced code skeleton.
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
On Windows, the same actions additionally publish and consume Lunar Magic's registered singular
`Lunar Magic 8x8 Tile` format. Its payload is exactly the selected 64-byte indexed-pixel buffer;
larger allocations are accepted with trailing bytes ignored, while short or non-4bpp values reject
before mutation. Copy publishes that native block and the portable Unicode envelope atomically, so
Rust-to-Rust text fallback does not prevent direct exchange with the original editor.
The retained isolated-Wine single-tile graphics oracle independently exercises Lunar Magic's named
copy and paste entry points through a second process, proving an exact 64-byte copy plus a
four-row asymmetric paste/copy round trip. The fixture is compiled into the native tests so a
record-order or documentation regression fails automatically.
Portable, pristine-layout, and installed tile sheets expose Lunar Magic's Ctrl+left-click copy
source gesture. Shift or Alt alongside Ctrl falls back to ordinary left-click selection, as do
other non-Ctrl left clicks. Direct executable evidence further proves that right-click routing tests
only Ctrl: without it the active edited tile is copied over the target, while any Ctrl combination
requests a system-clipboard tile and selects the target only after a valid payload is applied.
Installed right-click paste admits only an editable owner in a current, idle workspace; fixed,
ExAnimation-owned, stale, and file-worker targets cannot enter either paste path.
The portable document, pristine-layout ROM surface, and profile-backed installed ROM window also
expose horizontal and vertical transforms of the selected tile. Each transform materializes an
exact flipped 64-pixel edit buffer; pristine and installed backing changes only through a permitted
sheet paste, while portable documents retain their independently revisioned extension behavior.
Their tile sheets also share focus-scoped keyboard navigation. Unmodified Up selects the same
in-page offset on the previous 256-tile graphics page, while Down selects it on the next;
missing pages are no-ops and a partial final page clamps to its last tile. Every move transfers
keyboard focus; unmodified Left/Right remain unconsumed and
keys are not consumed unless the selected tile owns focus. The active display palette is an explicit
default-or-row state: Page Up advances from the recovered default palette through row zero and the
remaining available rows, while Page Down reverses that path and stops at the default palette. The
default selection uses the original fixed sixteen-entry RGBQUAD table, including its exact channel
ordering, for the picker, tile sheet, color-map previews, and pixel editor on all three surfaces.
Only the active page is materialized as a seamless 16×16 array of 16-logical-pixel cells, matching
the original 256×256 backing canvas. F1 consumes all modifier forms and requests an immediate
repaint on every graphics surface, matching the native `$1B59` dependent-editor redraw fanout
without changing model state or status. F8 toggles the initially hidden tile grid. Ctrl+Alt+F8 changes
its retained color between the recovered white and black DWORDs without changing visibility and
publishes `Tile grid color 1.` or `Tile grid color 2.` exactly.
Visible previous/next page and previous/next palette controls synthesize the same four native
navigation actions as Up/Down and Page Up/Down. They share selection clamping, focus transfer,
default-palette traversal, and exact page, boundary, and rendered-palette status with the keyboard
routes on all three graphics surfaces.
Both pristine SMW-US and profile-backed installed ROM editors additionally reserve unmodified F9
for the recovered current-level replacement. They resolve the globally active level independently
of the selected graphics page, present the native confirmation text, require the ROM-sibling
`Graphics` directory to contain the complete `GFX00.bin` through `GFX33.bin` standard set, and
replace only that level's ordered FG/BG and sprite assignments as one recoverable group. Expanded
selectors `$34+` except `$7F` use canonical `ExGFX` names in sibling `ExGraphics`; all selected
extended files must already exist and participate in the same preflight and grouped replacement.
Expanded Super GFX bypass records contribute six plus four slots; the recovered vanilla SMW-US
tables contribute four plus four. Repeated filenames are collapsed because publication is a file
set. Every output is a decoded `$1000`-byte 4bpp GFX/ExGFX file, and the active staged graphics
controller replaces its ROM source when it belongs to the set. The existing background group
writer remains available through the separate visible create-new-directory extraction action.
The View menu's non-persistent `Special World Passed Graphics` option mirrors the native global
display flag. It loads legacy GFX31 as half-size 3bpp data (or accepts its already expanded 4bpp
form), pads the synthesized SP2 working slot, and invalidates both built-in and installed level previews. Current-level F9 export
then omits the ordinary SP2 assignment before duplicate collapse because the preview buffer contains
GFX31 rather than that assigned file; GFX31 itself is not published by this command.
The same tile-grid adapter draws a distinct outline around the current nonselected hover target.
Its event-driven status state reproduces the original tile/address and palette hover text, selected
tile and foreground/background-color messages, rendered-palette messages, viewed-page messages, and exact
start/end boundary diagnostics. A stationary pointer does not overwrite a later keyboard message;
changing or leaving its tracked tile, palette, or pixel-editor region updates or clears the status.
Actual movement within the same tile also recomputes modifier-dependent animation attribution, while
a stationary modifier change alone leaves the last action message intact.
The same movement boundary applies inside a palette swatch and the selected-tile pixel canvas:
movement republishes their native hover text, but a stationary frame preserves a later select,
sample, navigation, or palette message.
Because the three toolkit regions are visited sequentially, an inactive region is a no-op unless it
was the previously tracked region; it cannot erase hover text emitted by another active region in
the same movement frame.
All three graphics surfaces initialize foreground color 1 and background color 0. Primary and
secondary palette clicks select those colors; primary and secondary pixel gestures paint with them;
Ctrl-primary and Ctrl-secondary sample the pointed pixel into foreground and background respectively.
Sampling remains enabled for installed read-only tiles because it changes only editor state.
Installed Ctrl+Shift tile hover consumes the same ownership evidence. Canonical v2 records retain
the original-animation slots `$00-$7E` and level/global ExAnimation slots `$00-$3F`, producing the
exact `OrigAnim`, `ExAnim Level`, and `ExAnim Global` messages while keeping every such tile
read-only. Version-1 generic ExAnimation records still decode and retain their conservative generic
tile message because they do not contain a recoverable native slot class.
Each pixel editor uses Lunar Magic's fixed 256×256 logical selected-tile canvas, making each source
pixel exactly 32 logical pixels wide. The toolkit applies monitor DPI scaling to that logical square,
as the native window does; there is no user-facing graphics-editor zoom selector. The same square is
passed to both rasterization and normalized hit testing.
The recovered Shift+Arrow shortcut performs a lossless one-pixel wrap left, right, up, or down.
Native Left/Right accept Shift with any other modifiers. Up/Down accept Shift without Ctrl/Alt only
when the selected tile is editable; every other modifier/ownership combination falls through to the
ordinary previous/next graphics-page route. Page Up accepts every modifier combination, and Page
Down does the same except that Ctrl+Shift is reserved for Lunar Magic's unmodeled internal-cache
unlock. Portable wraps are independently undoable, pristine wraps enter the staged graphics
controller, and installed wraps additionally require editable ownership plus a nonstale workspace
with no active graphics file worker.
The graphics character commands follow native `WM_CHAR` translation: D/M/R/X/Y work as lowercase
input and as Shift-produced uppercase input. Ctrl and Alt chords remain outside that dispatcher.
Installed animation-attribution hover activates whenever Ctrl and Shift are both held; the native
mouse-move branch does not exclude Alt, so Rust accepts Ctrl+Shift+Alt there as well.
Pixel editing follows the native mouse-down capture boundary rather than reclassifying every drag
frame. An ordinary primary/secondary press paints immediately and retains its FG/BG mode until
release even if Ctrl changes; a Ctrl press samples once and cannot become paint during that press.
Palette FG/BG selection likewise occurs on primary/secondary button-down on all three graphics
surfaces, independent of keyboard modifiers, rather than waiting for a completed toolkit click.
Tile-sheet selection, copy, selected-tile paste, and clipboard paste also dispatch on the initiating
primary/secondary button-down while retaining their native Ctrl/Shift/Alt routing.
Direct `WM_KEYDOWN` virtual-key tracing corrects the prior off-by-one function-key interpretation:
unmodified F9 is the original current-level GFX publication command, not a ROM-commit command.
Pristine and installed ROM editors route it only through the confirmed replacement workflow above;
their explicit commit buttons remain a Rust extension. The portable standalone document retains
F9 for its own document save workflow. Modified F9 combinations remain available to other routing.
The recovered global `Use joined AllGFX.bin files` option is command `$24BD` and persists across
application launches. In joined mode, F9 requires exact existing `$36D00`-byte
`Graphics/AllGFX.bin`, seeks by Lunar Magic's 52-entry native-size table, and replaces only the
selected standard ranges. Selected ExGFX files retain their sibling `ExGraphics` destinations and
join the same preflight and recoverable publication; internal selector `$7F` is ignored. The native
mode checkbox, bounded exact-shape read, prefix-sized range patch, and persistent preference all
have focused tests.
The executable's character jump table covers the inclusive `D` through `Y` range twice, for
uppercase and lowercase input, and maps only five entries: D applies the selected color map, M
opens the map editor, R rotates the active tile 90 degrees clockwise, X flips horizontally, and Y
flips vertically. The shared focus-scoped adapter accepts only unmodified forms of those keys.
Every mutating form joins the existing undoable or ownership-guarded tile replacement path; M
opens the same transactional color-map draft used by the visible button. Rotate 90°, horizontal
flip, and vertical flip are also visible controls on every graphics surface, matching the native
command dispatcher; buttons and R/X/Y resolve through the same transform action and therefore the
same portable revision or installed ownership/stale-worker gates.
The recovered color-map subsystem is represented as sixteen independent sixteen-entry mappings.
Every filter defaults to identity; each edit strictly selects one 4-bit source and destination,
Reset restores only the active mapping, and tile application masks each stored pixel to its low
nibble before replacing all 64 indexes. The shared native dialog shows the original base and
mapped preview concepts, keeps a draft of all filters, commits it only on OK, and drops it on
Cancel or window close. Applying the selected filter enters the same revisioned tile replacement
as paint, paste, flips, and shifts; the installed editor retains its ownership and stale-worker
guards. Focused model tests cover all sixteen identity filters, independent replacement, reset,
all index bounds, and the portable controller's undo path.
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
On Windows, the standalone page, complete-set, and installed-ROM editors also publish Lunar Magic's
registered `Lunar Magic 16x16 Tile` format in the recovered native layout: four little-endian
subtile words in top-left/top-right/bottom-left/bottom-right order followed by one little-endian
Acts Like word. Native paste is preferred, requires at least ten bytes, and ignores an allocation
suffix exactly like the original; the semantic Unicode envelope remains the portable fallback.
The retained isolated-Wine single-tile oracle independently exercises Lunar Magic's named copy and
paste entry points through a second process, proving an exact ten-byte copy and an asymmetric
paste/copy round trip. The fixture is compiled into the native test suite so documentation drift or
record-order regressions fail automatically.
The separate retained Map16 editor interaction oracle drives the original `$232F` dialog through
page and drag selection, four subtile values, Acts Like, palette, priority, both flips, and the full
nine-step Undo/Redo cycle. Its fixture is also compiled into the native tests; combined with the
48-case Rust publication matrix, it closes the core browse/edit workflow's original-behavior gate.
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
optional `level`, one of `layer2-objects` or `layer2-tilemap`, `palette`, `exanimation`,
`expanded-settings`, and semantic `sprite-spawn`
child scripts relative to the
specification file, for example:

```text
LMNATED1
level=Level edits.txt
layer2-objects=Layer 2 objects.txt
palette=Palette edits.txt
exanimation=Animation edits.txt
expanded-settings=設定 edits.txt
sprite-spawn=Spawn settings.txt
```

Every child retains its established strict format and limits. The shell parses all children before
dispatch, stages every domain in memory, and publishes one checksum-valid undo entry; a late child
or edit failure preserves the complete application project.
`layer2-objects` uses the strict `LML2OBJ1` header and accepts only the authoritative `LMLEDIT1`
`object ...` grammar. It therefore exposes insert/remove/move, parameter, absolute placement,
screen-exit, and complete ordinary-field edits without duplicating or drifting from Layer 1's
typed model. The installed profile must resolve the selected level to object-backed Layer 2;
tilemap-backed or portable aggregates reject the child atomically. The aggregate save plan carries
Layer 2's independent copy-on-write allocation policy through checksum repair and application undo.
The mutually exclusive `layer2-tilemap` child uses strict `LML2TIL1` framing. `word INDEX WORD`
paints one of the 1,024 native little-endian words, while `remap OFFSET SELECTION PROGRAM` runs the
recovered Lunar Magic remapper with a signed decimal global offset and either `all` or a canonical
comma-separated decimal selection. The remap program retains Lunar Magic's displayed
`$8000`–`$FFFF` source/destination syntax. Bounds, duplicate selection indexes, malformed native
programs, object-backed storage, and unrepresentable bank changes reject the complete aggregate;
word paints, selection-scoped remaps, descriptor changes, allocation, checksum, and undo otherwise
commit together.
`sprite-spawn` uses `LMSPAWN1` followed by one or both unique semantic commands. `settings RANGE
SMART` accepts decimal range `0..=3` and canonical boolean `SMART`; `boundary-air ENABLED` accepts
another canonical boolean. Neither carries a raw shared word. The aggregate controller reads the
authenticated current Lfix3 flags for `settings`, replaces only bits 0–2, and preserves the five
shared flags. `boundary-air` reads installed expanded-settings word 8, replaces only bit `$4000`,
and preserves its low 12-bit GFX selector and remaining flags. Missing storage, duplicate commands,
invalid ranges/booleans, and late mixed-command failure all reject before application history.
Portable aggregate documents accept `boundary-air` when their aggregate contains expanded
settings, with the same lossless history semantics, but deliberately reject `settings` because a
portable aggregate has no authenticated external Lfix3 plane. Either command rejects when its
required storage is absent.

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
The graphical **Test ROM in Emulator** route instead captures the exact current in-memory physical
ROM and revision in the application effect. The native frontend writes that immutable snapshot to a
private create-new temporary directory before showing the ordinary direct-argument approval prompt.
A selected executable receives the ROM path as its sole argument; configured tools must reference
`{rom}` directly and may additionally consume `{level_hex}` or `{level_dec}`. The worker polls the
child without blocking the UI. Stop or frontend teardown terminates and reaps the owned process,
and every denial, launch failure, exit, or stop drops the private directory. This provides safe
whole-ROM emulator testing but does not claim LMSW's direct selected-level injection, live reload,
pause/step, or synchronized viewport behavior.

The first backend-independent live-session component now reproduces Lunar Magic 3.63's recovered
pause aggregation exactly. `EmulatorSessionState` accumulates the level-transition (`$01`), manual
(`$02`), viewport (`$04`), input (`$08`), main-window (`$20`), and editor-mode (`$40`) hard-pause
reasons independently. Any hard reason selects backend pause mode 2; focus loss alone selects soft
mode 1; otherwise the backend runs in mode 0. Updates are inert while stopped and duplicate state
notifications emit no backend operation. Single-frame stepping first establishes the manual-pause
bit and applies hard pause, then emits exactly one step; later steps do not redundantly reapply the
pause mode. Stop clears both hard and soft state before reuse. This pure contract is intentionally
separate from the existing one-shot external-process launcher so native libretro and optional LMSW
bridge backends can share the same verified lifecycle semantics. It does not by itself claim ROM,
level, sprite, video, audio, or input transport.

`LMEMU001` is the corresponding shell-free backend transport. Every command and event is one
self-framed little-endian record with an eight-byte version magic and an exact payload length.
Commands cover initialize with immutable revision/ROM/selected-level/sprite data, ROM reload,
level load, sprite reload, pause mode, single step, four option flags, signed viewport state, and
stop. Events cover backend capabilities, acknowledgement, active state, viewport synchronization,
bounded RGBA frames, and bounded UTF-8 diagnostics. ROMs are limited to 32 MiB, sprite payloads to
1 MiB, diagnostics to 4 KiB, and frames to 512×478 RGBA; empty ROMs, invalid pause values,
zero/oversized geometry, inconsistent raster lengths, bad UTF-8, unknown tags, every truncation,
and trailing bytes reject before backend state changes. The codec is consumed by the separately
unsafe-isolated `lm-libretro` process. That backend resolves the libretro-v1 ABI and loads ordinary
cores directly from the editor-owned immutable bytes. For a core requiring a path, it writes those
same bytes to a private `.smc` snapshot, retains the path and file for the loaded-game lifetime,
and removes it after unload, failure, stop, or process teardown. It permits one bootstrap frame for
deferred memory publication, then rejects any core that still lacks exact 128-KiB SMW system RAM
instead of falsely advertising selected-level/runtime capabilities. It balances game/core
teardown and advertises ROM load, RGBA frame, pause, step, and viewport capabilities. Its
callbacks bound geometry, pitch, and allocation before copying and convert XRGB8888, RGB565, or
XRGB1555 frames to RGBA. The capability set now also includes joypad input and selected-level load.
The backend observes exact 128-KiB system RAM and reports game mode, Lunar Magic's two-byte
sublevel, vanilla translevel, and camera position with each frame. Initialization automatically
follows SMW's title/save/intro path until overworld mode, writes only `$0100`, `$0109`, and
`$010B/$010C`, and enters the selected 9-bit level through modes `$0F..$14`; later `LoadLevel`
commands use the same transition without restarting the core. The localized native Tools action
chooses a core, starts the sibling backend without a shell, validates Ready before Initialize,
drives frames at a bounded cadence, and exposes Pause/Resume, single Step, Stop, and a standard
keyboard-to-SNES joypad mapping. `tools/lm-libretro-smw-oracle.py` retains the end-to-end gate
against a supplied official Snes9x core and vanilla ROM. Closing the project or leaving level-editor
mode stops/reaps the session. A level-only editor transition sends `LoadLevel`; a committed
revision sends `ReloadRom` with the exact current in-memory physical ROM followed by `LoadLevel`,
clears the stale texture, and reuses the same worker/window. This makes every committed ROM domain,
including edited sprites, visible without a manual restart while preserving the immutable revision
boundary. Portable packaging includes the sibling backend. The running RGBA texture is also
composited into the level canvas when Game pixels/SNES viewport mode is active. Its rectangle uses
the same centered cover scale as deterministic game pixels and is recomputed from the available
canvas on every layout pass, so horizontal, vertical, and full-screen resizing remain synchronized
without replacing the canvas interaction response. Ghidra's `RenderLevelEditorViewportRegion` at
`$004530A0` proves Lunar Magic redraws only Map16 cells carrying selection flags `$60` after the
LMSW viewport pass. Rust's default-on Selection over game pass follows that order for selected
Layer 1/Layer 2 objects and sprites, preserving transparency and selection outlines while filtering
all nonselected entities; the toggle changes only this final pass. `RuntimeFrameAudio` extends the
bounded transport without changing the older frame tags or its one-command/one-response invariant.
The backend reads `retro_get_system_av_info`, validates 8–384 kHz, retains at most 8,192 stereo
frames from either libretro audio callback, and advertises capability bit `$80`. Native playback
uses a maximum two-second queue and linear rate conversion to the default stereo device, supports
signed, unsigned, and floating device formats, and clears latency state on mute, pause, reload,
stop, or teardown. The official Snes9x oracle proves 533 nonuniform stereo frames at 32,040 Hz per
captured video frame, distinct `$105`/`$106` hashes, and exact `$105` audio reproduction after ROM
reload; the opt-in hardware gate proves native stream creation, resampling, mute, and teardown.
LMSW's optimized sprite-only edit boundary is now implemented from the recovered
`SerializeSpriteDataForInternalEmulator`/`ReloadLmswSpriteData`/
`RefreshLmswSpritesAfterEdit` chain. The built-in editor proves that only sprites changed, emits the
canonical stream after its one-byte header, and leaves mixed Layer 1/2 commits on the full snapshot
path. The backend uses a bounded, fully restored save-RAM mirror as a portable libretro-visible
buffer, redirects `$CE..$D0`, clears the twelve regular status slots and 128 record load flags, and
keeps the active core in game mode `$14`. Its newer ROM snapshot is deferred until a later level
transition, avoiding any disguised unload/reload during the edit. The official Snes9x mutation
oracle proves the edited record becomes a live Goomba in `$009E` with nonzero `$14C8/$1938` state,
without leaving mode `$14`, and that a subsequent direct switch reaches a distinct level frame.
Native OS focus loss
now drives the original focus-only soft pause, while minimizing the main viewport drives hard-pause
reason `$20`; manual hard pause retains
precedence through the shared aggregate. A collapsed live window drives viewport reason `$04`.
An open egui popup drives input reason `$08`; closure retains the recovered 100-millisecond timer
grace from `MainFrameWindowProc`/`RenderLmswViewportOverlay` before clearing it. Editor-mode hard
pause reason `$40` follows `HandleLevelEditorCommand` exactly: disabling the shared level-editor
animation clock sets it and resuming the clock clears it. The frontend now drives every recovered
pause input into the already-proven aggregate. Broader platform runtime variants remain incomplete.
System RAM first uses libretro memory ID 2. As a standards-compliant fallback, the environment
callback copies at most 256 memory-map descriptors and selects only a non-constant exact
`$7E:0000`/128-KiB mapping with a bounded offset; all pointers remain process-isolated and are
discarded at unload. The official ARM64 bsnes 2014 Accuracy core passes the complete live oracle
with legitimate 512×224 doubled-width output, 32,041-Hz audio, direct levels, sprite hot reload,
runtime sprite tables, switching, and exact reload reproduction. Current Snes9x and Snes9x 2010
retain their prior exact 256×224 hashes. Cross-platform runtime evidence continues below.
The same source cross-compiles as an optimized x86-64 Windows GNU PE32+ executable and executes
through Wine 11.13 against the official Windows Snes9x 2010 DLL. `--backend-runner` lets the retained
oracle prepend exactly one direct runner executable; it never introduces a command shell or
reparses arguments. The Windows process passes every `$1FF` assertion and exactly reproduces the
native ARM64 Snes9x 2010 frame/audio hashes. The final platform boundary is closed by a separate
hosted Ubuntu job: it builds `lm-libretro`, compiles a warning-clean deterministic libretro-v1 core, and
executes a shell-free Python driver through all `$1FF` capabilities. The core supplies exact WRAM,
SRAM, video, audio, level-transition, and sprite-loader state without containing proprietary game
data. This evidence is intentionally orthogonal: Linux proves the process/ABI/platform path, while
the native macOS and Windows vanilla-ROM runs prove actual SMW behavior. Together with all four
packaged targets, this completes the supported LMSW runtime product.
The portable-release matrix now builds `lm-libretro` explicitly beside `lm-native` and `lm-cli`
for Linux x86-64, Windows x86-64, Apple Silicon macOS, and Intel macOS before invoking the strict
packager. Ordinary CI repeats the complete three-binary build and package operation on Linux,
Windows, and Apple Silicon macOS, so a missing or incorrectly suffixed live backend fails before a
release tag. The deterministic bundle manifest continues to hash all three executables. This closes
the previously unverified release-workflow wiring but is not runtime evidence for additional
libretro cores.
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
cumulative RGB histogram recovered from Lunar Magic's import path. Lunar Magic first rounds every
source component onto the 0, 8, …, 248 SNES lattice, retains equal-score cuts in native BGR byte
order, and reproduces the x87-to-binary32 split-score boundary. It removes duplicate BGR555 cluster
means and maps the same lattice-normalized pixels to one-byte palette indexes. Inputs are bounded to 16 Mi pixels
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

Before palette-row records are allocated, high-color 8×8 tiles cross a second native reduction
boundary. The importer computes the largest free capacity among the eight rows. A tile at or above
that capacity is reduced when the winning row's first reusable color cannot satisfy it: colors with
more than two occurrences gain one point for each matching pixel immediately across the tile's
top, bottom, left, and right borders; a qualifying exact first-entry color gains `$80`; the strongest
colors are retained with stable source-color ties; and every tile pixel is remapped with the native
`4R² + 3G² + 2B²` metric before color-set aggregation. Because tiles are processed row-major, an
already reduced upper or left neighbor participates in the next tile's border score. This recovered
stage prevents an impossible 12/13-color record from reaching a row with only 12 free entries and
matches the retained Lunar Magic 16-color palette and complete graphics workspace byte-for-byte.

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
planes. Packed MWL records retain their complete 16-bit index and opaque byte 7 for lossless file
editing. Installed binary-MWL import separately matches Lunar Magic: it consumes at most 8,192
records, skips indexes above `$1FFF`, applies duplicate keys last-wins, and ignores byte 7. The
retained Wine oracle exercises all four rules, reopens both endpoint indexes from the installed
table, then proves an empty MWL set clears both records and leaves a checksum-valid ROM.

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
by stable 16-bit sprite ID and insert, replace, remove, or directly reorder their painter-ordered
tile parts. The native form exposes the same move-before/end operation without reconstructing part
records. Its composition preview uses the renderer's exact 8×8 signed-offset geometry and painter
order, retains the sprite origin, identifies tile/palette/flip fields, and selects the topmost
overlapping part on click. Dragging previews pixel-snapped signed offsets without changing the
document, clamps both axes to the encoded `i16` range, and publishes one typed replacement only
when the pointer is released after actual motion. A focused preview additionally consumes unmodified
arrows as one-pixel selected-part nudges and Shift+arrow as exact eight-pixel selected-part tile
steps. Alt+arrow translates the complete composition by one pixel and Alt+Shift+arrow translates it
by eight pixels through one first-class `TranslateParts` edit. The controller stages every translated
signed offset before assignment, so one overflowing part rejects the complete operation without a
revision; blocked selected-part and composition edge movement is history-neutral. Ctrl/Command
chords remain available to the surrounding application. The native part
panel also copies one complete part through a distinct fixed-width typed clipboard domain, capturing
tile, palette, both signed offsets, and both flips. Paste-over and paste-after bind delivery to the
requested sprite ID, part index, mode, and document revision; selection changes cannot redirect the
operation, while stale, malformed, multi-part, or cross-domain delivery leaves the document unchanged.
Duplicate and both paste forms submit exactly one controller edit and therefore create one undoable
revision. The same typed domain carries a nonempty complete painter-ordered composition: the native
panel can copy every part, replace or append a composition on an existing stable sprite ID, or
insert a new definition at the requested index using the form's target ID. `ReplaceParts` is a
first-class controller command, while new-definition paste submits insertion and complete-part
replacement as one staged batch. Delivery retains the requested revision, stable ID, operation,
and insertion index; stale revisions, duplicate IDs, invalid palettes, excessive aggregate counts,
empty payloads, and wrong clipboard domains preserve every definition. Revision checks, atomic
batches, canonical reopen, immutable saves, and dirty shutdown protect the keyed document; the
built application exercises the workflow through paths containing spaces and Unicode.
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

The ignored `custom_object_collection_wine` integration gate now starts authenticated Lunar Magic
3.63 on a pristine-ROM copy with a Rust-authored multi-object collection, selects the exact custom
description through the live Add Objects window, synchronizes an unoccluded compositor capture of
the original preview control, right-clicks the level canvas, saves, and exports both the pristine
and modified levels through Lunar Magic itself. It requires the two-record placement delta to be
exactly `[06 06 10]`, `[07 0E 10]`, matching the original's recovered custom-selector and relative-
coordinate conversion; every other MWL domain, the ROM identity, and the repaired checksum remain
unchanged. Failure cleanup owns only its nonce-scoped ROM, sidecars, helper, and captures.
`PopulateCustomObjectTemplateList` additionally proves the original commits a picker row only when
it consumes LF. The live no-final-newline gate observes zero custom rows and an unchanged ROM;
`lunar_magic_picker_entries()` reproduces that view without discarding the final description from
the lossless editor or its byte-exact re-encoding.

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

Native crash recovery is deliberately separate from normal ROM persistence. Every new dirty
application revision captures both the exact accepted physical save baseline and the exact current
physical ROM in a bounded `LMRECOV1` record. Each process reserves a unique record and holds an
advisory session lock, so concurrent editors neither overwrite one another nor mistake a live
record for a crashed session. A worker publishes through a synchronized temporary file in the
platform-local application-data directory, keeping frame rendering independent of ROM size. The
record carries explicit lengths, active-level context, and CRC-32 framing; malformed, oversized,
clean, non-regular, and unsupported records cannot replace application state. Startup boundedly
queues up to 16 stale sessions and offers each for independent recovery or discard. Recovery
installs an unnamed project whose current bytes remain dirty relative to the retained baseline,
forcing the first publication through Save As. Clean save/close and explicit discard remove only
the corresponding record and lock. Editor-local forms that have not yet dispatched a project
mutation and undo history remain outside this recovery boundary.

Compatibility diagnostics are generated in `lm-app`, not inferred from native widget state. The
report revalidates the current physical ROM identity against the identity accepted at open time,
compares stored and computed checksums, distinguishes copier-header and logical sizes, counts
changed logical ranges and valid RATS payloads, and reruns any installed revision-profile audit.
For the SMW-US LoROM/ExLoROM family it classifies Layer 2 formats `$100`–`$103` and uses the same
complete immutable probes as the patch migration routes for Map16 stages one through four and all
three Lfix3 generations. Other mappers/families are explicitly `not-applicable`; a partial or
unknown runtime becomes a warning and never a guessed generation. The native dialog snapshots the
bounded path-free text when opened, scrolls it, and copies that immutable report for support use.

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
edge reciprocal 2 3 down none 80 fe c0
```

Final reciprocity validation permits explicitly one-way edges and lets one batch repair both halves
of a route. `edge reciprocal` owns only the one-way bit and carries independent forward/reverse
exit links and raw flags; `edge remove-reciprocal` removes both stable keys or neither. Removing a
node removes incident edges atomically. The revisioned controller refuses
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
unchanged; a manifest retaining every owned block is an exact no-op. The same transaction now has
full mapper-family coverage: ordinary LoROM, 8-MiB ExLoROM, and 6-MiB SA-1 all reclaim and retain
the intended logical-PC blocks, repair the active identity's checksum field, redetect and reopen
with the same mapper, and traverse byte-exact Undo/Redo. The ExLoROM case additionally proves that
the complete physical copier prefix remains outside the reclamation boundary.

See [REIMPLEMENTATION_ARCHITECTURE.md](REIMPLEMENTATION_ARCHITECTURE.md) and
[REIMPLEMENTATION_TEST_MATRIX.md](REIMPLEMENTATION_TEST_MATRIX.md) for the compatibility tiers and
fixture requirements. A production-ready editor still requires legal external fixtures covering
clean, headered, expanded, SA-1, and ecosystem-modified ROMs; write support should remain opt-in and
target a new file until those differential gates pass.

## Current boundary

The native menu bar now includes a bounded **Help → About Lunar Magic Rust** surface. It identifies
the running Cargo package version, the clean-room Rust implementation, its Lunar Magic 3.63
workflow-compatibility target, dual license, and source repository without presenting itself as the
original executable. The About surface can copy its public source URL, and **Help → Build
diagnostics** presents/copies a fixed fourteen-line, non-sensitive report with product version,
compatibility target, target OS/architecture, debug/release kind, license, project dirty state,
active editor, profile/save state, undo/redo availability, and current level. It never includes a
document path, ROM identity/hash/bytes, or user status text. Original CHM topic launching, deeper
ROM/runtime diagnostics, and Wine-observed About behavior remain open parity gates.

**Tools → Keyboard Shortcuts** is a Rust extension over Lunar Magic's fixed per-editor key
dispatch. It provides a staged native editor for the typed shortcut configuration, covers all
twelve frontend actions and every portable character, function, navigation, and editing key
family, validates the complete binding set before applying it, and rejects duplicate gestures
without changing active bindings. Active shortcuts are suppressed while the editor is open so
typing a candidate cannot invoke an existing command. Applying either a populated or cleared
binding set persists the canonical `LMSHORT1` configuration in native application storage and
restores it on the next launch. Cancel and window close discard staged edits. Because Lunar Magic
3.63 has no corresponding global shortcut-configuration dialog, this extension is not counted as
an original feature-parity workflow; original fixed keys are verified with their owning editors.

**Tools → Customize Toolbar** now stages the current portable toolbar as an ordered sequence of
actions and separators. It preserves each action's stable ID and independently selected typed
localization key, exposes every action and text key, supports bounded reorder/removal operations,
and validates the whole layout before replacing the live toolbar. A separate built-in-toolbar action
restores the native default instead of encoding an invented empty custom layout. Canonical
`LMTBAR01` bytes persist through native application storage, while configured layouts without a
localization catalog use a complete English fallback table rather than disappearing.

This dialog is a Rust extension, not an original parity requirement. The authenticated Lunar Magic
3.63 CHM exposes toolbar customization through executable-adjacent `usertoolbar.txt`, and the
complete 318-slot Ghidra command table contains no global customization-dialog command. The
original file-driven surface is complete: retained Wine proves a distinct two-button
`ToolbarWindow32` beside the 52-button main toolbar; parser, image, shortcut, lifecycle,
notification, and all 317 named-command routes pass; isolated Wine exercises Windows icon and key
ABIs; and both `lm-windows` and the complete native frontend cross-compile for
`x86_64-pc-windows-gnu`. The matrix therefore counts the original GUI/variant boundary as Pass
without pretending the optional Rust dialog existed in Lunar Magic.

The separate original `usertoolbar.txt` surface is now modeled independently rather than being
conflated with that Rust-only editor. The bounded UTF-8 parser implements Lunar Magic's documented
five-line definitions, implicit `***START***` termination, spacers, internal and external targets,
icon/tooltip/options/shortcut/working-directory fields, image-list and base directives, and global
visibility/configuration flags. The native process discovers the file beside its executable at
startup, adds a distinct wrapped second toolbar when visible, dispatches the recovered common
internal names, and routes external argument vectors through the permission-gated shell-free
launcher after expanding `%1`–`%8` ROM/executable/level context. Parsed shortcut overrides are
active even under `LM_NO_TOOLBAR`, suppress a matching built-in binding, and deliberately dispatch
every duplicate user assignment like the original. The token bridge covers quoted characters,
generic and sided Ctrl/Shift/Alt names, F1–F24, the available editing/navigation keys, numpad
digits, and raw equivalent Windows virtual-key codes. The portable `LMSHORT1` key enum appended the
missing insert/home/end/page/tab/space kinds without changing any existing discriminant. A retained
LM 3.63 Wine oracle observed the original create a visible 52-button built-in toolbar and distinct
two-button user toolbar from the committed fixture.

User-toolbar BMPs are loaded through the existing bounded Windows bitmap decoder. The first strip
height or an effective pre-image `LM_SETIMAGE_SIZE` establishes the square cell; horizontal strips
are divided exactly, the first RGB pixel is the transparent color key, and textures use nearest
sampling. The parser records the active image base on every button, including signed indexes and
`LM_NEWIMAGE`, previous, and global base transitions, so later directives cannot retroactively
change earlier buttons. Missing/non-regular/oversized/malformed strips and icon-list overflow reject
the image set without partially publishing textures, while the parsed toolbar and shortcuts remain
available with textual fallback. The main toolbar additionally discovers the documented
`Lunar Magic.ff4` file, requires its exact 41-square-cell geometry, and uses retained live 3.63
`TBBUTTON` bitmap indexes 1/3/5/6 for native Open/Save/Undo/Redo. Missing or invalid overrides keep
the default text controls rather than publishing a partial strip. Other editor-specific `.ff*`
strips, the exhaustive internal/options table, remaining mouse distinctions, and process
notification/lifecycle options remain open.

On Windows, external buttons without an image-list or force override now use the documented icon
field as an `ExtractIconExW` resource index (default zero) against the executable token. Quoted and
relative paths plus `%4` executable-directory expansion are resolved before extraction. A narrow
safe wrapper rasterizes the retained icon at the configured toolbar size into a top-down 32-bit DIB
against both black and white backgrounds, reconstructs unpremultiplied RGBA for both legacy masks
and modern alpha icons, and releases every icon, bitmap, and memory-DC handle before publication.
Missing/non-file/invalid resources retain the existing text fallback. Focused mock-resolution tests,
Windows cross-compilation, and an isolated Wine ABI test against its icon-bearing `notepad.exe`
cover precedence, index/path selection, bounded raster shape, and nonempty alpha output.

The user-toolbar shortcut bridge now preserves the formerly collapsed `VK_PAUSE` and numeric
keypad `VK_MULTIPLY`, `VK_ADD`, `VK_SEPARATOR`, `VK_SUBTRACT`, `VK_DECIMAL`, and `VK_DIVIDE`
families, including their raw `0x13` and `0x6A..=0x6F` virtual-key forms. `LMSHORT1` appends seven key
kinds after the already published mouse-key discriminants, so every earlier configuration retains
its exact encoding; the native shortcut editor parses and formats stable names for all seven. Since
egui 0.31 does not expose Pause or keypad location, the Windows frontend supplements its normal
event stream with a narrow `GetAsyncKeyState` bridge. It tracks every key while unfocused, emits
only focused rising edges, and therefore neither repeats a held key nor creates a false activation
when focus returns. Schema, parser, editor, focus/hold/release edge tests, an isolated Wine test, and
the complete Windows frontend cross-build cover the path.

The first option semantics are now active rather than merely retained strings. `LM_NO_BUTTON`
hides a control without disabling its shortcut. `LM_USEIMAGE_FORCE` sequentially assigns global
images to external buttons that would otherwise extract executable icons, and the `_ALL` form
assigns every non-spacer button. Explicit line-five working directories take precedence; otherwise
`LM_DIR_ROM`, `LM_DIR_LM`, and the original default/`LM_DIR_PROGRAM` external executable directory
are resolved before the direct argument vector reaches the permission prompt. Association-open,
console-window, multi-instance, close/autorun, and notification lifecycle options remain open.

Internal user-toolbar routing now includes the original open, save/save-as, next/previous level,
exit, undo/redo, overworld, 8x8 graphics, 16x16 Map16, palette, ExAnimation-slot, and Layer 3 editor
names. Commands that own a level use the actual active level; Layer 3 rejects without one instead of
inventing a target, while graphics and palette follow the native menu's slot-zero entry behavior.
The editor-local `LM_VIEW_LAYER_1`, `LM_VIEW_LAYER_2`, `LM_VIEW_LAYER_3`, `LM_VIEW_SPRITES`, and
`LM_VIEW_SPECIAL_WORLD` names now toggle the authoritative canvas visibility or Special World
rendering state and invalidate both level-render preview paths. They reject when no level is open.
The same local route now covers `LM_VIEW_ZOOM_TOGGLE`, `LM_VIEW_ZOOM_DEFAULT`,
`LM_VIEW_ZOOM_PLUS`, and `LM_VIEW_ZOOM_MINUS`. Ghidra's `SetUniformEditorZoomPercent` at
`$0048B6E0`, `CommitEditorZoomChange` at `$0048B600`, and `InitializeEditorZoomState` at
`$0048B760` prove the 100–5000 percent bounds, normal 100-point adjustment, 100-percent default,
and initially 200-percent remembered nondefault value. The native canvas stores that previous
value, so toggling to default and back restores the exact last nondefault zoom. `LM_VIEW_ZOOM`
now opens the recovered `$2440` popup surface with the original 100, 125, 150, 175, 200, 300,
400, 600, and 800 percent radio choices plus zoom-in, zoom-out, and filter controls.
`LM_VIEW_ZOOM_FILTER` routes command `$2444`, defaults enabled like the executable image, and
invalidates the level preview when toggled. The native popup and command/check state are complete.
Final-surface filtered presentation remains separate: applying linear sampling directly to the
packed Map16 and sprite atlases would blend unrelated neighboring atlas cells, while Lunar Magic
filters the already-composited editor surface. Rust intentionally retains nearest atlas sampling
until that final-surface compositor stage is available.
The animation command trio is also live. `LM_VIEW_ANIMATION` (`$2404`) pauses or resumes the one
clock consumed by the Map16, background, standard-sprite, existing-sprite, and custom-sprite
catalog previews. Pausing freezes the currently observed frame; resuming preserves it without an
absolute-time jump. `LM_VIEW_INCREASE_FRAME` (`$2403`) advances one recovered 60 ms editor timer
tick even while paused. `LM_VIEW_RESET_ANIMATION` (`$240E`) follows the actual dispatcher rather
than its published name: Lunar Magic reloads the current graphics caches and redraws without
zeroing its frame counters, so Rust invalidates and rebuilds the preview while preserving the
shared clock.
The four switch-state toolbar commands are renderer inputs rather than cosmetic button state.
`LM_VIEW_GREEN_SWITCH`, `LM_VIEW_YELLOW_SWITCH`, `LM_VIEW_BLUE_SWITCH`, and
`LM_VIEW_RED_SWITCH` default on independently, matching the executable's four initialized bytes.
The first two select the `$06A/$16A` and `$06B/$16B` forms of extended objects `$87/$8E`; the
second pair select `$06C/$16C` and `$06D/$16D` for mapped standard-object handlers 24/25. The
state is applied to both Layer 1/Layer 2 canvas rendering and the standard/extended placement
catalogs, so the chosen preview is the object that will visibly be placed.
`LM_VIEW_SILVER_POW` (`$2405`) now drives the default-off Silver POW sprite-preview state used by
Lunar Magic's standard handlers. The live sprite canvas, existing-sprite picture picker, and new
sprite catalog all receive the same state. When active, every authenticated conditional handler
uses definition `$115` at its recovered position; SSC custom displays remain authored displays
rather than being rewritten as built-in standard sprites. The same state now selects trigger-one
vanilla animation group 9, matching the shared original dispatcher state.
`LM_VIEW_POW` (`$2406`) now drives the default-off Blue POW animation state. The pristine logical
ROM mode table at `$02B96B` marks groups 6–13 as conditional, while the trigger entries consumed
from the table based at `$02B97D` are `0,0,0,1,0,2,2,0`. Blue POW therefore selects replacement
bank `$26` for trigger-zero groups 8 and 13 in addition to groups 6, 7, and 10 already selected by
the original default-on Invisible POW Objects state. Trigger one remains Silver POW, and trigger
two remains governed by the separately default-on On/Off state. Both POW flags participate in the
graphics-preview cache key and rebuild the full foreground/background phase set without modifying
ROM data.
The neighboring original-compatible names are now renderer state rather than inert toolbar entries.
`LM_VIEW_INVISIBLE` (`$2400`) defaults on and half-blends Map16 `$027-$02A` plus trigger-zero
animation groups 6, 7, and 10 while Blue POW is clear. `LM_VIEW_INVISIBLE_2` (`$2401`) defaults on,
maps `$021/$022` to `$114`, `$023` to `$113`, and `$024` to `$115`, and draws the four exact
half-blended overlays for `$06F-$072`. Those overlays were recovered directly from Lunar Magic's
64×16 24bpp PE resource type 500, ID 501; blue pixels are transparent exactly as in
`RenderMap16TileToPixelBuffer`. `LM_VIEW_LINE_ON` (`$2402`) controls the visible On/Off trigger-map
selection used by animation groups 11 and 12. `LM_VIEW_CDM16` (`$2409`) defaults on and adds `$100`
to the source bank of Direct Map16 `$27/$29` records whose output-width and source-control high
bits both mark the conditional form. The native canvas routes these states through source-aware
Layer 1 and object-backed Layer 2 painting; serialized object bytes remain untouched. The shared
Map16 atlases themselves remain raw. Baking conditional substitutions into those atlases caused
ordinary background cells with overlapping numeric IDs to be remapped, producing 60,571 wrong
pixels in complete vanilla Level `$105`. The raw-atlas correction is byte-identical to all 488
previously verified Rust corpus PNGs and zero-pixel-different from Lunar Magic's full Level `$105`
oracle; a deterministic raster-hash regression test now guards that boundary.
`LM_VIEW_512HEIGHT_BG` (`$2407`) now switches the background's vertical source period from the
original 27 Map16 rows (432 pixels) to the complete 32 rows (512 pixels). The game-camera preview
uses the same period for both the precomposed background plane and its Map16 fallback, while the
horizontal period remains 32 columns/512 pixels. The default remains off and the toggle is presentation
state only; it does not rewrite the level's Layer 2 tilemap.
`LM_VIEW_TRANSLUCENT` (`$2415`) now applies one half-opacity presentation state to editor-only
overlays: object selection/resize geometry, unresolved sprite markers and sprite selections, the
Map16 and screen grids, exit and boundary annotations, and entrance labels/warnings/markers. Level,
sprite, and background artwork remain opaque, and game-preview mode continues to omit editor
overlays entirely. This mirrors the original's save/draw/average operation without changing any
serialized level state.
`LM_VIEW_TILE_GRID` is separately recovered as command `$2408`. The original command toggles
`DAT_00E278E5`; `RenderLevelEditorViewportRegion` at `$00453D90` calls
`DrawLevelMap16ScreenGrid` only while that byte is set, and the pristine executable initializes it
to zero. The native view model now carries the same default-off flag, the toolbar name toggles it,
and the canvas emits its Map16 grid only when both the flag and editor-overlay mode are active.
The separate `LM_VIEW_SCREEN_GRID` name is command `$23F6`, backed by original flag
`DAT_00E278E6`. `DrawLevelScreenLabelsAndOutlines` at `$004518F0` proves that this is not the tile
grid: it outlines and labels each horizontal screen's Top/Bottom regions or each vertical screen's
Left/Right regions with a two-digit hexadecimal screen number. The native display model therefore
keeps it as a separate default-off screen-overlay mode and paints those orientation-aware bounded
regions. A second activation returns to no screen overlay. The mutually exclusive `$23F5` Screen
Exits member is now implemented as well. `DrawLevelScreenExitAnnotations` at `$004525B0` and
`DrawScreenExitAnnotation` at `$00452240` prove the orientation-aware screen rectangles, red
outlines, last keyed exit per screen, and exact direct, midway, secondary-slot, resolved
secondary-destination, and overworld labels. The canvas joins its lossless Layer 1 exit records with
the existing detected pristine/installed 8,192-entry secondary-exit table; malformed installed
storage is surfaced rather than silently producing false targets. That immutable table is decoded
once per ROM revision and reused across level navigation; the exhaustive 512-level materialization
gate therefore retains bounded load time. `$23F5` and `$23F6` replace one
another, and activating the current member returns to no screen overlay. `$23F7` Boundary Guide is
now the third member. `DrawLevelBoundaryGuideOverlay` at `$00451EC0` selects 256×232 pixels for
ordinary/vertical modes, 352×232 for alternate horizontal modes, and 448×224 for alternate
vertical modes from the same recovered mode flags used by the renderer. The native guide converts
those exact dimensions to the responsive canvas scale and anchors the outline at the current
entrance/camera viewport. `LM_VIEW_SCREEN_GRID_2` selects it, another group member replaces it, and
a second activation clears it, completing the original three-way mutually exclusive display state.
Remaining original names that operate other editor-local modes, dialogs, toggles, clipboard
payloads, or installers stay explicitly unsupported until their corresponding typed frontend
action exists.

User-toolbar process lifecycle now observes exact closed/open document transitions. Per-button
`LM_AUTORUN_ON_NEW_ROM` enqueues once for each newly opened ROM through the native permission gate,
while global `LM_NO_AUTORUN` suppresses it. `LM_CLOSE_ON_NEW_ROM`, `LM_CLOSE_ON_CLOSE`, and both
documented `_FORCE_ALL` variants remove matching pending approvals and signal only the corresponding
running user-toolbar process; the cancellable worker kills and reaps that child. Repeated frames do
not retrigger autorun. Original cross-process window-message notifications and the opt-out
`LM_NO_AUTORUN` interaction with programs started before a configuration reload remain to verify.

**Tools → Language** now exposes the active locale and installs standalone `.lmlang` catalogs using
the canonical `LMLOC001` decoder on the existing bounded, non-blocking document worker. The public
maximum encoded size exactly covers a valid catalog with the longest locale and every maximum-size
text entry. A successful load replaces the complete catalog atomically and persists its exact
canonical Unicode bytes; malformed, oversized, truncated, or incomplete catalogs leave the active
language unchanged. **Use Built-in English** removes the custom catalog explicitly and persists that
choice. Persisted localization, shortcut, and toolbar hex payloads are preflighted against their
public canonical maximum sizes before byte-vector allocation. Native title, status, configured
toolbar text, the complete application menu surface, Help, About, Compatibility diagnostics,
crash recovery, unsaved-change confirmation, global-error acknowledgement, and Undo History
consume the catalog today. The executable-adjacent `sysLMLanguage` route deterministically scans
at most 64 bounded canonical `.lmlang` regular files, exposes each valid locale, and skips invalid
siblings. Automatic selection remains persistently distinct from fixed built-in English and an
explicit catalog; it compares at most the original 64 entries from the bounded Windows
preferred-UI-language list (or ordered locale environment values elsewhere), prefers an exact
normalized locale, then a primary-language match,
and safely retains English when no catalog matches. Applying recovered original DLL dialog-template
text throughout the remaining native editor surface and retained live Wine evidence remain open.

The portable localization core now implements the prerequisite original-DLL validation ABI rather
than trusting or executing a candidate module. It reproduces all three offset-selected byte
transforms from `ValidateLanguageModuleChecksum` at `$004D7010`, wrapping 32-bit accumulation, the
excluded 64-byte trailer, and the stored dword at `file_size - $38`. It also decodes the
`$01F4:$0DB7` marker and bounded `$01F4:$0DB6` BOM/CRLF metadata into display name, version, locale,
and code-page fields. Its non-executing PE reader accepts PE32 and PE32+, bounds section counts,
headers, resource-directory entry counts, relative offsets, RVA-to-file mappings, raw section
extents, language leaves, data entries, and payload slices before exposing either resource. Short
modules, checksum mismatches, malformed/truncated images, missing resources, wrong markers,
oversized metadata, invalid UTF-8, and incomplete fields reject. Native startup separately scans
the exact executable-adjacent `sysLMLanguage` directory for at most 64 `.dll` regular files, reads
at most 64 MiB per candidate, skips invalid siblings, and retains checksum-validated metadata in
deterministic display-name/path order. Original modules remain distinct from selectable `.lmlang`
catalogs at this prerequisite stage; the subsequent string conversion and selection implementation
below closes that boundary.

The portable core now decodes the original localized string payload as well. It reads `$0DAC`,
`$0DAD`, and `$0DAE` through the validated PE resource tree, reproduces the recovered chained
byte transform, requires a complete raw-DEFLATE stream, caps inflated output at 32 MiB, and applies
the original minimum of declared count, complete offset/length table extents, and 5,869 entries.
Every offset/length addition is checked; entries without an in-range trailing NUL are unavailable,
and valid entries must be UTF-8 before publication. The exact 272 single-index and 22 half-open
range guards recovered from `$005E6420` and `$005E6398` then clear strings whose byte length reaches
the original fixed-buffer ceiling. Thirty-one evidence-backed menu/action equivalents now map into
the typed catalog with mnemonic, accelerator, ellipsis, and About-template normalization; missing
slots and Rust-only workflows retain English independently. Native startup converts valid original
DLLs without executing them, exposes their metadata display names in Tools → Language, includes
their locale tags in the exact-then-primary auto-detection pass, installs a selected catalog
atomically, and persists its canonical typed bytes. Remaining original dialog-template mapping and
retained live-module evidence kept localization incomplete at that milestone; the dispatch mapping
is recovered in the next milestone, while native dialog-control application remains open.

The original dialog-resource dispatch boundary is now recovered too. Two 107-word tables map
built-in dialog IDs to language-DLL type-5 IDs; the Rust table reproduces both independent Ghidra
memory dumps byte-for-byte. The portable PE reader can query an explicit resource type, and the
public dialog decoder first validates checksum plus `$DB7/$DB6` module identity, then returns only
mapped type-5 resources actually present in the DLL. Missing individual templates are omitted so
callers retain the original's per-dialog built-in fallback; malformed resource bounds still reject
the module. Synthetic two-type PE tests cover the first and last mappings, exact borrowed payloads,
missing mappings, wrong marker, and invalid RVA. Native control-text application and retained
live-module/Wine evidence remain open, so Configuration/Localization stays `Partial`.

The dialog payload is no longer opaque. A platform-neutral bounded decoder supports standard and
extended Win32 templates, including style-dependent font tails, ordinal-or-Unicode values,
DWORD-aligned item records, both control-ID widths, and arbitrary bounded creation data. It
publishes literal Unicode captions only. Synthetic standard/extended fixtures cover Unicode outside
the BMP, named and ordinal classes, ordinal titles, creation bytes, and rejection of every truncated
prefix, invalid extended versions, invalid UTF-16, and trailing data. A local-only ignored gate then
parses all 107 original resources from both Lunar Magic 3.63 architectures. The converted DLL
catalog now uses the recovered Language dialog's OK/Cancel and About dialog's three semantic button
IDs, with mnemonic normalization and independent English fallback. Remaining control-to-native-form
bindings and a retained localized DLL gesture keep the row `Partial`.

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
Raw and semantic sprite replacements now share one post-apply refresh boundary. After the staged
controller accepts a replacement, the selected token is decoded back into the raw text and packed
semantic controls, including whether the result remains an ordinary editable record. This prevents
a raw token-shape change from leaving stale coordinates or an incorrectly enabled semantic action.
The parallel object-backed Layer 2 list now has the same staged-state rule across all of its
ordinary actions. Button insertion selects the computed new index before decoding it; raw and
semantic replacement decode the accepted record; and deletion clamps to the surviving range or
resets the form and extension-preserving placement template when no record remains.
Layer 1 list insertion now uses `object_record_for_placement` as well, closing a native-only path
that previously reconstructed three ordinary bytes and discarded the extension selected from an
OSC catalog entry. Successful insertion selects the computed index; replacement and deletion
reload the canonical staged record and its placement template, while failures restore selection.
That template invariant now applies to every real-record selection route rather than only action
buttons: load/history refresh, list and canvas clicks, typed paste, reorder, drag/resize selection
and completion, and canvas insertion retain the selected Layer 1 or object-backed Layer 2 record.

Authentic modified-ROM coverage now reaches object-backed Layer 2 allocation as well as Layer 1
and sprites. `external_lunar_magic_rom_layer2_object_insertion_is_isolated_and_undoes` discovers a
compatible level through the installed runtime, inserts an ordinary object through the native
canvas, and requires the save to repoint only that level's Layer 2 stream. The selected level's
Layer 1 and sprite pointers and a neighboring Layer 2 pointer remain byte-exact. Rust reopens the
complete staged Layer 2 value, Lunar Magic 3.63 re-exports the baseline-canonicalized stream plus
the same insertion, the repaired checksum validates, and one application undo restores the exact
logical ROM.

The authentic Layer 1 boundary now covers existing-record lifecycle operations in addition to
growth. `external_lunar_magic_rom_object_move_and_delete_round_trip_exactly` drags one ordinary
object through the native canvas relocation path and removes a second through the native action
path. The resulting RATS-owned publication changes only level `$102`'s Layer 1 pointer, reopens as
the complete staged Rust value, and exports from Lunar Magic 3.63 as the baseline-canonicalized
stream with the identical relocation and removal. Layer 2, sprites, the neighboring Layer 1
pointer, checksum repair, and exact application undo are independently asserted.

Command-zero records are no longer conflated at the canvas boundary. Parameters `$00–$03` retain
their control meanings and reject placement, while `$04+` are positioned extended objects and now
flow through the same canonical insertion/relocation machinery as standard commands. This applies
to both Layer 1 and object-backed Layer 2. The authentic modified-ROM insertion gate now places a
standard object, places a command-zero extended object, edits that extended selector through the
semantic form, and requires Lunar Magic 3.63 to reproduce both changes exactly.

The Layer 1 and object-backed Layer 2 tools now expose positioned standard, extended, and resolved
OSC custom records through visual catalogs rather than requiring raw-byte entry or duplication of
an existing Layer 2 record. The extended catalog enumerates only recovered definitions for the active object tileset,
supports hexadecimal filtering, renders each Map16 pattern through the same tileset-aware
definition set as the canvas, and stages the selected `$00 00 xx` record as the
extension-preserving placement template. Standard selection intentionally creates a fresh
three-byte template. Selector `$17` has a retained test proving its tileset-0 substitution differs
from the shared preview before placement into a real decoded level. The custom catalog shares the
active OSC variant/filter/display rules across layers and retains the selected command-specific
native width as its placement template. The Layer 2 workflow commits, reopens, and undoes one
record from each catalog, including a four-byte command `$22` custom record with its extension byte
intact.
Selecting a new standard catalog command remains the explicit boundary that creates a fresh
ordinary record and clears prior extension provenance.
The parallel SSC catalog now has a complete application-backed mixed-table save fixture. One `$F0`
sprite ID selects four-, five-, six-, and seven-byte records through extra-bit tables zero through
three; each preserves the current packed placement and materializes zero-filled extension bytes at
the declared width. Semantic coordinate edits retain all four selectors and widths in one stream,
native allocation and checksum repair reopen every record byte-exactly, and one application undo
restores the expanded pre-edit image.
The native bitmap-to-Map16 chooser now accepts PNG and BMP through one signature-detected decoder.
Its Windows path implements bounded indexed 1-/4-/8-bit, default 16-bit RGB555, 24-bit BGR, 32-bit
BGRX, 16-/32-bit `BI_BITFIELDS`, `BI_RLE4`, and `BI_RLE8` DIB images. This includes packed indexed
pixels, bounded BGRA palette tables, validated nonoverlapping contiguous channel masks with rounded
eight-bit scaling, four-byte raw-row padding, and signed-height top-down versus bottom-up storage.
The RLE state machine bounds encoded and absolute runs, word padding, delta movement, row endings,
and mandatory bitmap termination before writing any target coordinate; compressed top-down DIBs
reject as invalid. Reserved palette and BGRX bytes are opaque rather than interpreted as alpha.
Unsupported planes, depths or compression, dimensions, palette shapes or indexes, bit masks, RLE
coordinates or termination, offsets, truncated data, and unknown signatures reject before
constructing the existing preview.
The BMP boundary also accepts the Windows `BI_JPEG` and `BI_PNG` embedded-image modes. Both require
bit depth zero, a nonzero exact `biSizeImage` payload, and decoded dimensions matching the DIB
header. PNG retains RGBA semantics; bounded JPEG grayscale, RGB, 16-bit luminance, and CMYK output
is normalized into the same RGBA model. Truncated payloads, malformed codecs, dimension mismatch,
and decoder output beyond the existing memory limit fail before preview construction.

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

The shared modeled-asset staging boundary also applies Lunar Magic's reserved legacy-mode repair
before any orientation-dependent import work. MWL modes `$12` through `$1D` become `$00` while the
background-color bits remain unchanged; sprite ordering, object-control canonicalization, extent,
single import, and directory batch import therefore all observe the repaired mode in the same
transaction. Low-level MWL decoding remains lossless until this editor-semantic import boundary.

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

The pristine-layout native level editor now uses that same generation-aware sprite-pointer
resolution. This matters immediately after Lunar Magic saves a level: Layer 1 can still decode
through the original low-word table while sprites require the installed per-level bank. Previously
the `$102` canvas therefore combined the requested level with `$001`'s shared-bank sprite stream.
`vanilla_level_editor::tests::builtin_editor_resolves_lunar_magic_per_level_sprite_banks` installs
a synthetic bank table and proves the canvas decodes its unique `$47` placement. An authenticated
Lunar Magic 3.63 level-$102 capture now shows all three jumping-fish previews at their exact native
coordinates. The aligned 656×448 phase-0 comparison improves from 9,273 to 6,096 differing pixels
and from 2.175570 to 1.205272 mean absolute channel error. The audit comparator also treats the
observed 656×448 DIB as the same fixed `(870,338)` editor crop as the existing 656×464 capture,
avoiding a false heuristic offset.

That installed pointer generation is now covered in the write direction as well. The native
semantic sprite form applies a legacy coordinate replacement and Lunar Magic's stable screen sort
as one controller batch, tracks the selected record to its new index, and reloads its canonical
fields; expanded streams retain their explicit token order. The focused
`semantic_legacy_sprite_position_edit_sorts_and_tracks_the_selected_record` regression covers that
interaction. The ignored `external_lunar_magic_rom_sprite_edit_saves_reopens_and_undoes` gate opens
an authentic 2 MiB Lunar Magic-modified ROM, moves one level-$102 sprite to the next screen through
that form, inserts another sprite through the canvas, and commits the grown stream through the
installed per-entry bank layout. Growth must relocate level `$102` to a different SNES pointer,
while level `$101`'s low word and bank still resolve to its exact baseline pointer. The controller
now serializes changed streams independently for nonshared layouts, so this sprite-only operation
also retains level `$102`'s exact Layer 1 pointer and payload instead of needlessly reallocating it.
`level_controller::tests::sprite_only_nonshared_commit_preserves_layer1_pointer_and_payload`
proves the same isolation on a compact synthetic ROM. Rust reopens the complete grown/sorted
aggregate. Lunar Magic 3.63 then exports both the untouched input and
Rust-written ROM: their complete Layer 1 aggregates agree after Lunar Magic's own legacy
screen-exit canonicalization, the untouched sprite export equals Rust's baseline, and the edited
sprite export equals Rust's complete grown stream. The physical output retains a valid checksum,
while one native undo restores the complete logical input byte-for-byte.

The reciprocal Layer 1 growth boundary now passes on that same authentic ROM. Expanded SMW ROMs
use long zero-filled free-space runs, so the built-in level allocation policy now accepts Lunar
Magic's `$00` expansion fill as well as `$FF`; pristine 512 KiB ROMs retain their stricter
`$FF`-only policy. `expanded_zero_fill_is_available_for_layer1_growth` requires a grown payload to
land behind a valid RATS tag in a synthetic zero-filled expansion. The ignored
`external_lunar_magic_rom_object_insertion_grows_only_layer1_and_undoes` gate inserts an ordinary
object through the level-$102 canvas, requires only `$102`'s Layer 1 pointer to relocate, preserves
its sprite pointer and level `$101`'s Layer 1 pointer exactly, and semantically reopens the Rust
stream. Lunar Magic's before/after exports differ by exactly the canonically positioned inserted
object after applying its baseline screen-exit normalization; sprites remain identical, checksum
passes, and undo restores every logical input byte.

The installed level canvas now shares the portable editor's expanded-sprite relocation semantics
instead of sending every drag or placement through the legacy screen sorter. Dragging an expanded
record resolves its effective upper-Y state and submits `RelocateExpandedSprite`; placement inserts
the complete selected record and performs that relocation in the same atomic edit batch. Both
routes stably sort by five-bit screen and resolved upper-Y band, plus Lunar Magic's low-four-Y-bit
tie-breaker in vertical modes, rebuild only the minimal `$FF vv` transitions, retain record
identity/extensions and same-key priority, track the selected token across sorting and removed
controls, and reload the canonical form. Expanded `$FF 80..FD` pairs are retained by raw codecs
but stripped at semantic edit/export boundaries, matching Lunar Magic's ignored-control behavior.
The application-backed
`expanded_sprite_canvas_edits_rebuild_controls_commit_reopen_and_undo` fixture starts with a shared
upper-Y token, drags the record into the visible base band, inserts a second expanded record,
commits the canonical stream to ROM, reopens it exactly, and undoes to every baseline logical byte.

The built-in editor now derives that framing independently for each selected level from the
serialized sprite header's Lunar Magic-owned `$20` bit after resolving the active shared-bank or
per-level-bank pointer. It no longer leaves `expanded_sprites` at the pristine default merely
because an installed bank table was detected. Encoding reciprocally clears `$20` for legacy
streams and sets it for expanded streams, keeping the header, control grammar, and terminator in
one canonical state. `builtin_editor_detects_sprite_framing_from_each_stream_header` places legacy
and expanded streams behind the same pointer-table generation and proves that both decode through
their correct codec; the existing expanded canvas fixture covers commit, exact reopen, and undo.
Binary MWL projection follows the same per-stream rule rather than treating the container's opaque
32-bit flags as a sprite-format discriminator. Encoding now preserves those flags byte-for-byte;
legacy bundle conversion and native exports initialize them independently of `.mw2` framing. The
retained `lunar_magic_exports_rust_installed_expanded_sprite_framing` Wine oracle moves a level
`$105` record to screen `$10`, upper-Y band 2, and an escaped `$FF` first byte; observes Lunar Magic
export header `$24`, `$FF 02`/`$FF FF` tokens, and container flags zero; matches the complete
screen/band-sorted stream; and reopens Rust's exact ROM payload with a valid checksum.
`lunar_magic_matches_vertical_expanded_sprite_ordering` independently discovers a real vertical
level in that installed ROM, moves two same-screen records into upper band 2, forces an otherwise
ambiguous low-Y-nibble reorder, and requires Lunar Magic's complete export to equal the
orientation-aware Rust stream. The native edit batch derives orientation from the staged level
mode, including a mode change earlier in the same atomic batch.

Sprite framing is now canonicalized after semantic edits and at native/MWL export boundaries.
The installed aggregate, pristine-ROM, portable native-level, binary MWL, and complete-level
editors all expose the original sprite-memory settings `$00..=$12` and both buoyancy modes as typed
controls. `NativeSpriteHeader` partitions those user-facing fields from the serializer-owned
expanded-framing bit `$20`, so changing gameplay properties cannot silently change the stream
grammar. Lunar Magic 3.63's dialog procedure at `$00412CE0` proves that its independent checkbox
control `$1B4` (buoyancy 1) owns header bit `$80`, while `$1B5` (buoyancy 2) owns bit `$40`; the
shared semantic model follows that non-numeric ordering and tests each bit independently. A shared
native form keeps all document workspaces on that interpretation and reloads
canonical controller state after edits and history navigation. Focused model/workspace coverage
rejects memory setting `$13` and preserves `$20`, while an application-backed regression exercises
edit, undo/redo, checksum repair, exact ROM reopen, and ROM-history undo.
The adjacent recovered properties remain in their actual independent storage domains. Binary MWL
header byte 6 now has a lossless typed view whose low two bits select the horizontal-level vertical
spawn range and whose bit 2 enables Smart Spawn; its five shared Lfix3 flags survive semantic edits,
canonical reopen, and whole-document history. An authenticated current-Lfix3 ROM attaches the same
four external fields to the installed aggregate controller. The Level panel exposes both spawn
properties only in that state; edits preserve the flags byte's high five bits and share aggregate
dirty state, bounded undo/redo, private-project serialization, checksum repair, exact ROM reopen,
and application ROM undo. Absent and unauthenticated Lfix3 states keep the controls disabled and
reject programmatic edits before history. The beyond-boundary air/water choice is expanded
settings word 8 bit `$4000`, as proved by Lunar Magic's packed-high-nibble helper. Its installed
aggregate control preserves the low 12-bit GFX selector and all other high flags, commits through
the ordinary checksum-repaired aggregate mutation, reopens semantically, and undoes exactly.
If no upper-Y/control token or escaped `$FF` record remains, Rust clears header bit `$20`, switches
to the one-byte legacy `$FF` terminator, and permits the saved level to reopen through the newly
selected grammar; inserting a token that requires expanded grammar performs the reciprocal
transition. Low-level parsing and raw encoding remain lossless for fixture inspection. The live
`lunar_magic_downgrades_unneeded_expanded_sprite_framing` oracle writes a deliberately unnecessary
expanded level-$105 stream directly to ROM and proves Lunar Magic 3.63 exports the same records
with `$20` clear and legacy termination. Focused level-save and installed-canvas tests cover both
framing transitions, semantic reopen, and exact undo.

The previously opaque expanded-control range is now executable- and oracle-bound.
`ParseSerializedLevelSpriteStream` advances past `$FF 80..FD` without changing the active upper-Y
value or creating a sprite node. The live
`lunar_magic_strips_ignored_expanded_sprite_controls` oracle injects `$80` and `$FD` around a real
upper-Y transition and proves Lunar Magic removes both while retaining the transition and every
record. Rust semantic normalization now does the same; low-level raw parse/encode remains exact.

The same modified-ROM boundary now covers startup GFX32/GFX33 relocation. Lunar Magic can leave the
pristine packed pointer data at `$003882` untouched while rewriting GFX33's low-word operand at
`$00388B`, GFX32's at `$0038D8`, and their shared bank operand at `$003890`. The SMW-US resolver
authenticates the surrounding startup opcodes, exposes the two non-contiguous sources as bounded
one-entry layouts, and feeds validation, entrance graphics, common animation, sprite-display, and
installed level rendering from those live operands. Synthetic relocation/corruption tests cover
the resolver, and a local 2 MiB Lunar Magic-created ROM now opens level `$102` without the former
`InvalidBackReference` GFX33 rejection and produces a complete editor capture.

The native **Extract/Insert GFX32/GFX33** directory actions now consume that resolver as well.
Extraction maps each public filename to its independent live one-entry layout instead of treating
the stale `$003882` bytes as a two-entry table. Insertion validates both exact decompressed sizes,
searches only complete 32 KiB LoROM bank intersections, and uses one atomic multi-payload request:
GFX33 publishes the new shared bank plus its low word, while GFX32 publishes its low word only after
the allocator proves the same mapped bank. Pointer storage and checksum bytes remain protected;
wrong code, insufficient same-bank space, late allocation failure, or reopen mismatch yields no
commit. `graphics_io::tests::explicit_graphics_pointers_share_one_bank_and_commit_atomically`,
`graphics_batch_import::tests::special_import_repoints_both_live_operands_into_one_bank_and_reopens`,
`special_import_rejects_corrupt_code_and_missing_same_bank_space`, and
`graphics_batch::tests::batch_uses_per_file_layouts_for_noncontiguous_special_sources` cover the
portable boundary. The ignored `external_lunar_magic_rom_special_graphics_repoint_and_reopen` gate
also passes against the local authentic 2 MiB Lunar Magic-created fixture.

The retained eight-phase audit identifies phase 2 as Lunar Magic's captured animation state, with
5,452 unmasked differences and 0.911269 mean absolute channel error. Every delta above one is
confined to three measured Lunar Magic-only overlay rectangles: the screen-number labels and
entrance annotation. `LM_COMPARE_IGNORE_LIVE_RECTS` accepts explicit, bounded
`x,y,width,height` rectangles, reports the ignored-pixel count in the TSV, and rejects malformed,
empty, negative, or out-of-image masks. It is deliberately forwarded only to comparison, never to
capture or rendering. Excluding exactly those 3,824 reference-only pixels leaves 290,064 artwork
pixels: 2,011 differ by one channel step, zero differ by more than one, the maximum channel delta
is one, and mean absolute channel error is 0.002311. `compare-level-render-audit.test.mjs` covers
lossless multi-rectangle parsing and unsafe shapes.

The adjacent Layer 2 runtime migration now accepts all three legacy table generations. Ghidra's
`DetectLayer2DataTableFormat` selects `$100`, `$101`, and `$102` from hook byte 9, while
`MigrateLayer2ObjectDataTable` proves `$100` and `$101` share the same descriptor-flag
normalization before the common 512-entry pointer conversion. The recovered `$100` hook uses
`LDA #$05 / PHA / PLB` to select its hard-coded data bank; `$101` substitutes `PHK / PLB` and
retains the same instruction tail. Rust authenticates the complete 64-byte source hook for every
generation, captures both pointer tables and the descriptor table as exact preconditions, commits
the `$103` conversion as one revision, and restores the legacy image byte-for-byte on undo.

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

The adjacent native level-image exporter follows the bundled Lunar Magic 3.63 help boundary for
both single and multiple PNG/24-bit-BMP output. Single-level export recomputes the major-axis screen
count from the final visible Layer 1 object or sprite. Multiple export accepts a Unicode filename
template and appends ` %03X`, defaults to levels whose Layer 1 pointer is in expanded ROM space,
optionally recomputes each screen count without changing the ROM, and skips levels that cannot be
rendered. Stored sizing retains the highest serialized Layer 1 transition, while automatic sizing
ignores control-only tails such as the custom-time command. Horizontal and vertical canvases crop
along their recovered major axis through the same stride-preserving bounded raster operation.
Both paths snapshot the active animation phase and Special World state at dispatch, then carry
those immutable view inputs through background rendering. This matches the documented current-view
boundary without letting a long batch mix view states when the frontend clock or toggle advances.
Installed SMW profiles additionally treat zero per-level palette and ExAnimation pointers as the
original tool's intentional "use vanilla defaults" state rather than malformed addresses. The
shared profile controller composes the vanilla 257-word working palette and empty compact
ExAnimation while retaining installed data for either nonempty domain. Full-level rendering also
uses the complete native SMW Map16 controller, so editor and exported-image definitions cannot
diverge after Lunar Magic installs or migrates its Map16 runtime.
The shared View menu additionally owns independent Layer 1, Layer 2, Layer 3, and sprite visibility. The
primary canvas and installed preview omit hidden artwork and its interaction targets; inspection
and image export receive the same immutable visibility snapshot. Visible raster layers retain the
native Layer 2-before-Layer 1 painter order instead of rebuilding a reordered filtered list.

MWL import uses that same visible-extent calculation for the five-bit last-screen field stored in
legacy-header byte zero. A live Lunar Magic 3.63 control/import oracle reduced a screen-`$13`
source to `$12` when sprites were the highest remaining content, retained `$12`, and expanded
exactly through `$13`, `$14`, and `$1F` as injected Layer 1 objects required. A deliberately
backward `$1F`→`$00` object stream remained byte-for-byte ordered, proving that import updates the
extent without globally sorting raw objects. Installed Rust MWL import now stages those two effects
separately: canonical sprites participate in automatic extent, the header receives `count - 1`,
and the source object sequence is retained exactly. Direct ROM-to-MWL export instead preserves the
stored field even when a raw sprite lies beyond it, as confirmed by a separate live oracle. Both
native level editors, edit scripts, and the terminal expose this independent `last-screen` value;
generic ROM save/export therefore does not incorrectly substitute the import-only calculation.
Command-zero screen exits are excluded from automatic artwork extent: even an absolute
screen-`$1F` marker in an otherwise empty imported level leaves Lunar Magic's field at `$00`.
Its byte-zero high bit still advances stream state for following visible content, so the same exit
followed by an ordinary object produces `$01`. Lunar Magic transfers that transition onto the
ordinary object, moves the exit behind the positional stream, and clears the exit's now-redundant
bit. Multiple exits populate a 32-slot table in source order, so duplicate screens use the last
record and the surviving tail is emitted in ascending absolute-screen order. The shared placement
walker applies control advances before later ordinary objects, while installed MWL import performs
the same stream-state rewrite without sorting ordinary object order or treating the editor-only
marker itself as artwork. Two additional live probes establish that consecutive exit advances
accumulate before the next visible object and that an earlier duplicate's advance remains part of
the stream state even when a later same-screen record wins the keyed exit slot.
Raw MWL screen-exit shape is also semantic at this boundary. Any record lacking required flag
`$0400` gains both default flag `$0100` and `$0400`, so a live zero-high-byte case becomes `$0534`.
Its original compact or extended representation is retained, while already-flagged records keep
their other bits. The separate typed setter continues to select compact versus extended shape from
an explicitly edited destination.
The same orientation-aware import pass removes every raw command `$28`, decodes the last custom-time
value, and appends its canonical encoding after the keyed screen-exit tail. Lunar Magic's live
combined-control export therefore orders ordinary objects, screen exits, then custom time; duplicate
or non-trailing custom-time records collapse without disturbing ordinary object order. A live raw
duplicate probe with distinct `$0123` and `$0456` settings confirms that the later `$0456` value and
its force-reset bit are the single surviving terminal record. Repeating the same raw duplicate case
after changing the imported header to vertical mode `$03` produces Lunar Magic's swapped-nibble
vertical `$28` bytes exactly, proving orientation comes from the imported level rather than the
destination's previous mode.
The `LMLEDIT1` semantic `custom-time` command shares this exact setter. A shell integration test
creates `$ABC` horizontal and vertical forms, requires their raw records to differ while both
decode to the same typed value, verifies checksum-valid reopen and byte-exact undo for each, and
proves zero-without-force rejection leaves the complete ROM unchanged.

The opt-in `rust_custom_time_and_support_patch_b_are_applied_in_snes9x_gameplay` gate closes the
initialization-only gap for this field. It installs the exact support-patch-B runtime, publishes a
forced `$456` command in both deterministic new-game starting-level candidates, reopens both, and
boots the resulting ROM through the supplied driver and an official Snes9x libretro core. The
driver advances the real title/file/intro flow, presses controller A on the overworld, and captures
state only after game mode `$14` exposes timer digits `4/5/6` at WRAM `$0F31..$0F33`; Rust also
requires a bounded nonuniform rendered frame. The companion standard-header gate supplies
discriminating runtime coverage for ordinary time, music, sprite memory, buoyancy, and Layer 1
scroll. Together with the retained Lunar Magic suite that edits/reopens every five-byte field and
exhausts reserved-mode canonicalization, plus renderer coverage for mode/palette/tileset effects,
this closes the aggregate Oracle gate without requiring each visually redundant field to own a
separate emulator scenario.

`rust_standard_time_music_and_sprite_headers_are_applied_in_snes9x_gameplay` independently removes
the custom-time runtime from the equation. It sets ordinary time selector 3, music selector 7,
sprite memory `$0B`, clears both buoyancy controls, and selects Layer 1 scroll mode 3; explicitly
removes command `$28`; reopens both
candidate levels; and follows the same controller-driven entry path. The captured gameplay state
requires live timer digits `4/0/0`, active song `$12` at WRAM `$0DDA`, sprite memory `$0B` at
`$1692`, cleared buoyancy flags `$00` at `$190E`, and the exact disabled/conditional scroll pair
`$00/$00` at `$1411/$1412`. The earlier mode-2 probe produced `$01/$02`, proving those two runtime
bytes jointly distinguish the selected scroll behavior. These discriminating values differ from the
previous `$03/$12/$C0` live tuple, proving the controller route entered the edited level rather
than merely observing plausible vanilla defaults. This proves the ordinary timer, music, and complete
sprite-header properties plus the custom bypass reach genuine gameplay with their expected values.

For authenticated ordinary SMW-US Layer 3, the installed preview and both image-export paths load
the source level's stripe tilemap and active profile graphics, honor editor start offsets and
horizontal/vertical plane repetition, split the priority planes, and preserve normal or
background/Layer-3/foreground painter order plus additive pixels. Enabled custom installed
tilemaps use the recovered packed descriptor to load the active profile's GFX/ExGFX file into an
all-`$38FC` 4,096-word workspace, including exact length/offset clipping and the `$07F` no-file
sentinel. Short or inaccessible files still fail closed rather than publishing an image with a
partially materialized plane.
Encoding or destination failures still abort the grouped create-new publication, and cancellation
before publication leaves no output files visible.

Native IPS creation now mirrors the recovered `CreateIpsPatch` (`0041F0B0`) selection order:
original ROM, modified ROM, then output patch. A background worker performs bounded regular-file
reads, materializes Lunar Magic's physical headered IPS coordinate space, uses the shared
deterministic normal/RLE IPS encoder, rejects canonical input/output aliases, and atomically creates
or replaces the selected `.ips` file without freezing the frontend. Existing copier prefixes enter
the comparison byte-for-byte; supported headerless ROMs receive the recovered synthesized prefix.
Native application performs the reciprocal conversion: apply to that physical form, discard only
the temporary prefix, and route the resulting logical change through the revision-checked project
transaction so the open ROM's exact physical header state remains unchanged.

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
to the same 124 changed ranges and 23 exact RATS owners. The full observer now also retains four
typed Layer 3 composition fields for each of the seven expanded-settings records: mode enabled,
packed mode, alternate-source route, and primary additive input. Regenerating both observations
from the unchanged authentic ROM pair adds exactly 28 paths without changing any existing value,
ROM digest, changed range, argument, or owner. The built-process gate compares parsed observations,
prints at most the first sixteen semantic differences on failure, separately requires canonical
text, and explicitly checks the four slot-zero values; this replaces an unusable multi-megabyte
string assertion while preserving complete equality.
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

`NativeCustomOverworldSpriteController` promotes that model into the application mutation
boundary. It accepts ordered insert, replace, remove, and move-before batches on a private table,
validates the complete result by canonical encode/decode, reuses only the authenticated prior RATS
owner, repairs the checksum, and returns a revision-bound mutation. Tests cover all four edits in
one transaction, semantic reopen, application Undo, stale rejection, and atomic failure for an
invalid ID or the 25th sprite on one map. The SMW-US profile now derives the stream operand from
descriptor field `+$114 + $0D` and the record-size operand from field `+$BFC`, including upper-body
ExLoROM and relocated All-Stars routes. An installed `$42` size-table marker must resolve to an
exact 127- or 128-byte RATS owner; its bytes are normalized exactly like Lunar Magic, while an
uninstalled vanilla ROM uses 128 four-byte records. Vanilla `000000`/`FFFFFF` stream sentinels
open as seven empty maps so the first placement can be installed transactionally. The installed
overworld GUI now exposes every map-local list with insert, replace, delete, and ordering controls;
it can copy the current canvas selection into an entry's pixel position, renders staged placements
through the native appearance pipeline, and commits the stream with its operand and authenticated
size-table owner protected from allocation. Terrain, ordinary records, animation options, and the
native stream now publish together: the stream allocator runs against the materialized earlier
mutation, semantically reopens its result, repairs the final checksum, and returns one application
mutation. A growth test places an earlier staged allocation directly after the old stream owner,
proves the stream relocates around it, reopens both domains, and undoes both in one step.
The map canvas also exposes a dedicated native-sprite tool while that panel is active. A click on
the main plane or shared submap plane converts to the selected map's local `0..$1F8` coordinates,
then replaces the selected record or inserts at the list end. Invalid cross-plane clicks are
rejected, leaving the stream unchanged; leaving the panel returns the canvas to Select mode.

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

Legacy expanded ExAnimation data now has the corresponding installation-migration transaction.
`migrate_legacy_exanimations` resolves the already-installed current pointer table, requires all
512 destination pointers to be empty, and preloads every old slot before mutation. It applies the
recovered bank-byte presence rule, `$3F` count mask and 32-record clamp, converts each `$23`-byte
record, trims only trailing inactive records, and emits current compact payloads with setting zero,
header value `$FFFF`, and empty trigger configuration. All new RATS allocations and pointers, the
complete `$600` old pointer table, the `$140` auxiliary table, Lunar Magic's exact count-origin
`count * $23` source erasures, and checksum repair form one history entry. Source/destination,
checksum, and duplicate payload overlaps reject before commit; late allocation failure also leaves
the ROM and history unchanged. Focused tests reopen the migrated semantics and prove exact
Undo/Redo across LoROM, ExLoROM, SA-1, and headered/headerless physical forms. The surrounding
fresh runtime installation and the separate old pointer-hook rewrite remain distinct workflows;
both now have retained original-editor evidence.

The retained current runtime also corrected an earlier synthetic-only assumption about global
storage. `ResolveGraphicsRuntimeDataOffset` (`0045C790`) does not read one contiguous operand at
`+$5C`: it combines the bank byte there with the low word at `+$65`. Global load/save and exact
owner reclamation now use that split pointer, and ROM-aware allocation policy protects its one-byte
and two-byte components independently. The retained Lunar Magic 3.63 installed ROM begins with
both components zero; a focused live-fixture test recognizes that as empty, allocates and reopens a
canonical global payload through the split fields, then restores the exact source ROM with Undo.

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
Aggregate automation exposes the same intent without an opaque byte. An `LMNATED1`
`exanimation-features=PATH` child points to a bounded `LMEXFT1` script containing exactly
`features PALETTE VANILLA GLOBAL LEVEL`, with canonical `true`/`false` values. Parsing rejects a
missing, duplicate, malformed, or oversized command before mutation. The aggregate controller
sets only those four named states on the currently loaded record, preserving the unrelated low
nibble; absent profile storage is a typed late failure that rolls back earlier child edits.
The profile-qualified application-shell integration fixture uses a real shared chained runtime
operand for the installed ExAnimation pointer table and a distinct displaced feature-table
operand. Starting from feature byte `$A5`, the complete `LMNATED1` batch reopens `$55`, retains
low nibble `$5`, repairs the ROM checksum, and restores the entire pre-edit image through one
application undo. A malformed `LMEXFT1` retry is rejected before any domain mutates the ROM.
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
for every nonempty variable plane, and accepts the four-fixed-plus-two-tagged compact form, the
all-tagged form, and Lunar Magic's empty compact form with four fixed zero planes plus two null
reader pointers. Installed updates trim every plane to one common used length, reclaim only proven
owners, publish null tail pointers instead of artificial one-byte owners when the table is empty,
repair the checksum, semantically reopen, and commit as one undoable revision. Clean-ROM
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

The installed-ROM complete-overworld editor also imports and exports the complete nine-domain
`.lmow` aggregate without blocking the UI. Import is bounded and requires the active profile shape,
animation limit and size-mode table, canonical re-encoding, and palette ownership before replacing
the staged controller in one atomic operation. The file's source slot is provenance, so intentional
cross-slot copies remain possible. Export snapshots the current staged aggregate and publishes only
to a new destination. Loading or persistence gates commit and close, and a ROM revision change while
loading rejects the import. Original Lunar Magic prompt and file behavior and broader variants
remain incomplete.

The same installed editor's Animation panel exposes a focused `.lmexan` transfer alongside the
aggregate. It uses the active profile's exact 256-entry size-mode table and maximum-record bound.
Import canonically encodes and reopens the portable file before replacing only the animation
domain, treats its source slot as provenance, and leaves the other eight staged domains unchanged.
Export snapshots only the active compact animation and publishes to a create-new destination.
Both routes reuse the complete-transfer worker, stale-revision, edit, commit, and close gates, so a
late animation read cannot overwrite an edit made while file I/O was active.

The installed native-level-assets editor now transfers its complete staged palette through portable
`.lmpal`, Lunar Magic's exact 257-word raw format, version-2 TPL, and RGB24. Reads are bounded and
non-blocking. Raw/TPL/RGB imports automatically request the same-basename optional `.palmask` and
reuse the authenticated decoders. Raw applies the recovered 257-word operation; TPL/RGB apply the
exact installed `[0],[2..256]` to supported `[0..255]` mapping, retain installed backdrop word 1,
and clear only selected supported row-zero entries. RGB imports retain their detected channel-bit
expansion for reciprocal export. Full exports snapshot staged colors, publish only to a new
destination, and recoverably remove a stale `.palmask` because this surface exports every color.
Every imported result becomes one aggregate ownership-aware palette replacement, so fixed or
ExAnimation-owned differences reject the whole operation without disturbing any level domain. The
`.lmpal` source level remains provenance rather than a destination lock, permitting explicit
cross-level copies. Palette loading gates aggregate editing, commit, and close; export persistence
gates commit and close, and a ROM revision change during loading rejects the result. Original row
dialogs and the complete Wine ownership/animation-reservation matrix remain.

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

Expanded sprite semantic publication now reconstructs the control stream from each record's
effective upper-Y state. A live Lunar Magic 3.63 export proved that a leading `$FF 00`, repeated
`$FF 02`, ignored `$FF 80`/`$FF FD`, and a trailing `$FF 02` are all discarded while one required
`$FF 02` is retained immediately before the affected records. The shared canonicalizer reproduces
that minimum transition sequence for ROM, MWL, native-level, CLI, and graphical saves while the raw
parser and checked encoder remain byte-lossless for forensic interchange.

A direct-ROM Lunar Magic 3.63 oracle additionally writes level `$105`'s first two legacy sprite
records in descending screen order without invoking an editor gesture. Lunar Magic's export
stably restores complete screen order, proving this is a load/serialization invariant rather than
only a coordinate-edit side effect. Semantic publication now applies that stable sort after
framing canonicalization; malformed short records retain their original indexes for precise typed
errors. Expanded ordering remains in the orientation-aware positional-edit path because its
vertical comparator has a distinct low-Y-nibble tie-breaker.
The native raw-replace, insert, and clipboard-paste routes predict that stable order before
dispatch, then reload the same record at its canonical index; custom-width SSC records therefore
retain both their extension bytes and active selection when insertion moves them earlier.

The equivalent direct-ROM expanded oracle injects only two records in descending screen and
upper-Y order into horizontal level `$105`, without calling either editor's movement path. Lunar
Magic 3.63 exports the stable `(screen, resolved upper-Y)` order and minimum transitions. Expanded
semantic publication therefore uses an atomic orientation-aware canonicalizer everywhere an
aggregate supplies its Layer 1 header: vertical modes add the recovered low-Y-nibble tie-breaker.
ROM saves, binary and legacy MWL exports, native-level files, document/controller replacements,
imports, and encoded-size prediction all share this path. Invalid controls or short records leave
the source value unchanged and return typed errors; the standalone raw stream codec remains
byte-lossless for forensic use.

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

The standard-directory and joined-file insertion entry points now also share the recovered legacy
format-transition guard. An authenticated SMW-US ExGFX runtime without both `$32` regular-GFX
format markers opens Lunar Magic 3.63's exact `Graphics Format Change Warning!` text before a worker
is started. The pending request retains its already selected immutable source and directory or
`AllGFX.bin` target. No/Escape drops it without reading a file or preparing a mutation; Yes dispatches
that same request. Merely expanded, partially marked, truncated, or foreign-hook ROMs do not acquire
the warning. Yes authenticates every fixed patch, RAM reference, and runtime payload; snapshots the
reserved, ordinary, and extended ExGFX pointer tables; preserves the existing ROM size; installs the
52 replacement standard files; and rejects publication unless all three tables and every standard
file reopen exactly. A live Wine gate clears only the two format markers in a Rust-created runtime,
performs the migration, and requires Lunar Magic 3.63 to re-export all 52 standard files byte-for-byte
while Rust reopens the retained `ExGFX80` payload unchanged. First installation now follows the
original prerequisite order: zero-filled expansion, expanded-settings/runtime ownership, then GFX
allocation. The allocator protects the complete `$088000..$08ACFF` extended pointer table; first
compressed ExGFX insertion zero-initializes both compressed pointer domains and publishes the
recovered `$002A47 = EA EA` expanded-format marker. The same Wine gate first verifies Lunar Magic's
own import/export control, then proves `-ExportExGFX` enumerates and byte-matches the Rust-created
`ExGFX80`, closing the original-tool recognition gap without copying unrelated ROM metadata.
First ExGFX insertion also probes the independent expanded ExAnimation generation and, when absent,
installs its recovered `$C30`, `$600`, `$20`, and `$30` owners plus fixed graphics/shared-palette
hooks in the same unpublished staging project. Allocation accepts the route's zero fill, skips every
RATS owner, and protects the extended ExGFX table. Runtime detection no longer mistakes a legitimate
reserved `$60..$63` pointer for corruption: nonzero entries must resolve to bounded RATS-owned
payloads, while the four padding bytes and empty entries remain canonical zeroes.
The retained Wine transaction runs that full path twice from the same authenticated logical ROM:
once with Lunar Magic's canonical 512-byte copier prefix and once without it. Rust preserves the
prefix byte-for-byte or preserves its absence, produces identical logical 2-MiB results, and Lunar
Magic re-exports all 52 regular files plus `ExGFX80` from both outputs.
The same live transaction now repeats under internal map mode `$30`; the Rust result retains its
Fast-LoROM identity and checksum, and Lunar Magic re-exports the complete GFX/ExGFX set byte-exactly.
The authentic SA-1 Pack v1.40 route is separate from the large-ROM mapper-compatible ExAnimation
allocator. After Rust's verified standard-GFX installation, first `ExGFX60`, `ExGFX80`, and
`ExGFX100` reproduce the three distinct fixed-marker transitions and initialize
`$07F200..$07F77F`. Reserved `$60` uses its exact raw RATS owner and `$10:CF7A` pointer. Ordinary
`$80` expands to 2 MiB and allocates its LZ2 stream at `$0FFFF8` with pointer `$20:8000`. Extended
`$100` stores its pointer at the start of the expanded-settings owner; Rust resolves the concrete
owner from the authenticated SA-1 runtime operand at `$07F873`, so the route remains correct when
the standard-GFX allocator moves the owner beyond canonical `$087FF8`. The LZ2 encoder matches
Lunar Magic's earliest-source tie break for equal-length dictionary matches. Three retained
before/after gates compare the complete logical ROM byte-for-byte, and the live Wine gate requires
Lunar Magic 3.63 to reopen every independently generated Rust result and re-export all three files
exactly. The first-import transition is selected from the complete seven-case domain mask rather
than merely the highest file number. Four additional byte-exact oracles cover `$60+$80`,
`$60+$100`, `$80+$100`, and `$60+$80+$100`. They prove that a reserved file changes ordinary
allocation to the lower first-fit gap, while an import without reserved files allocates extended
before ordinary storage. The original editor re-exports all three files from Rust's mixed result.
Subsequent full-directory synchronization uses a distinct public transaction from intentional
sparse insertion. It semantically reopens every populated old pointer, authenticates each exact
RATS owner and native raw size, deduplicates shared owners, reclaims only that proven set, resets
the reserved/ordinary/extended tables to their domain-specific sentinels, and republishes only the
files present in the selected directory. Omitted pointers therefore disappear exactly as they do
in Lunar Magic, while the sparse API continues to retain omitted slots for individual edits. The
replacement-all and only-`ExGFX80` SA-1 results match retained Lunar Magic ROMs byte-for-byte,
including allocation order and the original checksum-compensation region; synthetic images whose
stored-checksum delta exceeds that bounded region retain the already valid recomputed checksum.
The native directory worker routes through this synchronization boundary, and its entire reclaim,
replacement, deletion, checksum repair, semantic reopen, and publication remains one revision and
one Undo step. An unowned, malformed, or incorrectly shaped old pointer rejects before publication.
An ExGFX insertion request also upgrades either authenticated legacy ExAnimation generation inside
the same unpublished project. The pointer-hook generation advances its owned fragments before the
new file is allocated; the global-table generation migrates its complete record set into current
storage first. A failure in either migration prevents publication of the combined mutation.

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

The built-in runtime installer reports the exact authenticated legacy generation it will migrate.
Lfix3 generations 1 and 2 distinguish packed-table conversion from three-plane preservation;
Map16 stages `$0100`, `$0101`, and `$0111` identify their `$0112` target; and Layer 2 formats
`$100`, `$101`, and `$102` identify their `$103` target. The same generation predicates control
the migration button and typed command, so the dialog cannot describe one source format while
dispatching another. Focused coverage enumerates every supported legacy generation.

The native Map16 bitmap-import color dialog now exposes Lunar Magic's recovered bulk palette-row
actions. Each of the eight rows has explicit Free, Reusable, and Reserved buttons alongside the 16
individual state cells. The shared bounded helper changes exactly one complete row, reports no-op
updates accurately, and rejects overflow or rows outside the 128-entry workspace. Focused UI and
bitmap-planning tests pass, followed by the 221-test renderer suite and both pristine all-512-level
materialization/dimension gates.

The Wine bitmap-import oracle now rejects processes without a loaded level, records the exact live
level, and reloads that slot through Lunar Magic before capturing Map16 buffers. This prevents a
restored pre-ROM modeless window from supplying stale palette/graphics evidence. Popularity audits
also drive and record the priority and 1..128 maximum-color controls. Cross-captures identified the
changing level-105 palette cell `$64` as animation state—not a reduction result—while the converted
graphics remained byte-identical; parity assertions therefore exclude that false signal.

Popularity reduction now models Lunar Magic's independent unique-color-priority checkbox. The
native control and global gate are recovered as `$6E`/`DAT_005e55ce`; when disabled, candidate
scores remain pure histogram frequencies, while enabling it applies the existing level-1..4
nearest-color weighting. The native Map16 dialog exposes the switch only with Popularity selected,
and focused coverage proves disabling it removes distance weighting without changing the priority
value itself.

Multi-row allocation now includes Lunar Magic's previously missing default weighted partial-set
extension. Exact-fit allocation completes each row before seeding the next and ranks candidates by
overlap plus direct occurrence weight. After exact-fit sets are exhausted, rows with capacity
greedily choose uncovered sets by existing-color overlap and aggregate subset weight, then install
missing colors by their direct pixel weights. The recovered Maintain Detail control skips this extension pass and also changes the earlier
global index assignment: exact source matches claim reduced-palette indexes first, after which each
remaining palette color claims its globally nearest unused source color before ordinary nearest
mapping. Lunar Magic prepends the zero/transparent sentinel to that candidate list; Maintain Detail
therefore lets it claim the globally nearest unused source color as index zero. A constrained
four-color/three-slot test proves default allocation fills the two remaining
entries while Maintain Detail retains only the reusable exact color, and a separate two-color test
proves the distinct-source claim prevents both source colors from collapsing to one palette index.
`maintain_detail_zero_sentinel_claims_the_nearest_unused_source_color` locks the recovered sentinel
behavior.

Palette-aware bitmap reduction now performs Lunar Magic's preserved-color substitution before
source pixels are mapped. It greedily selects the globally nearest unmatched reduced/reusable pair,
uses the exact HSL240 hue/saturation/lightness admission branches from `ProcessBitmapGraphicsImport`,
retires a rejected source color without trying a second candidate, and consumes a reusable RGB555
value only once. The live default at `DAT_005e5600` is 45; the native editor exposes the complete
0–240 range, where 240 accepts every nearest pair. Focused tests cover close-hue acceptance,
distant-hue rejection, the unlimited setting, neutral-color bypass, and the native free-entry gate.

The bitmap color model now also carries the native `$74` unmarked-color policy. Disabling it skips
source quantization and reusable-color substitution, collects every non-reserved destination word,
and reproduces `CollectUniqueAvailablePaletteColors`' tail-swap duplicate removal. Those existing
colors may still move through normal free-slot row allocation, as confirmed by a normalized live
Wine capture; the mode prevents new colors rather than freezing palette indexes. Calling the
palette-free reduction API in this mode returns a typed missing-context error. Control `$65` is
modeled separately as Lunar Magic 3.63's checked, disabled, conversion-neutral preference: Ghidra
finds only its dialog load/store references, and the native UI mirrors that disabled state. The
audit harness now normalizes all persistent checkboxes on every run so prior oracle sessions cannot
silently contaminate option evidence.

The full existing-colors-only differential additionally proves that exact colors retained in an
earlier row do not globally suppress the weighted extension of a later row. Initial exact placement
keeps matching free words in place, while the later-row pass treats the same source records as
available again and rearranges only existing words. Final tile assignment prefers exact movable
assignments over equally accurate retained words and uses the mode's reverse equal-entry scan, so a
duplicated palette word selects its later index. This closes the observed palette from 24
different bytes to zero and graphics from 269 different bytes to zero without admitting any new
color.

The retained bitmap-import capture now also records Lunar Magic's effective 128-byte palette-entry
map before conversion. The three 16-color Popularity differentials exposed and closed the ordinary
weighted-RGB555 mapping, exact-row allocation order, subset-weight lifetime, partial-extension
weight source, and HSL ordering gaps. Exact allocation completes one palette row before seeding the
next, ranks candidates by existing-color overlap and direct occurrence weight, and excludes already
covered subsets when recomputing aggregate weights. The partial pass still ranks records by their
aggregate utility but chooses individual missing colors by direct pixel weight. Every HSL run starts
at its lowest-lightness color and uses `3L² + 2S² + 8H²`, or `3L² + S²` when both saturations are
below 16. The active two palette rows and all `$300` native graphics tiles now compare byte-for-byte
for no-neighborhood, method 1, and method 2 captures. A boundary capture immediately before row
allocation also proves the Method 2 source-color plane is byte-identical (`07e0db077220846d...`) in
both implementations, independently localizing and then closing the allocator defect.

The native bitmap preview now exposes the complete recovered Other Options state: first and blank
8×8 tile, first and reserved blank Map16 tile, new/existing 8×8 optimization, 16×16 deduplication,
blank 8×8/16×16 shortcuts, and layer priority. Blank 8×8 inputs can reference the configured fixed
tile without an allocation or ownership write. Blank 16×16 detection reads actual decoded pixels,
then uses the configured reserved definition only in deduplicated mode. The original always checks
all flip orientations when existing-tile optimization is enabled, so the native Rust dialog no
longer presents a fabricated independent flip toggle. The Wine audit now captures the complete
Other Options dialog and its control states on every run.

Native bitmap previews now enter Lunar Magic's eight-row color allocator by default; the earlier
single-row opt-in switch was a Rust-only workflow and is no longer presented as original behavior.
Every accepted color and Other Options edit is retained in the Map16 editor's process-local state,
including across Cancel, successful import, and closing/reopening the Map16 window. A newly launched
preview restores every choice. The native launcher no longer overrides First Map16 with the visible
page or exposes an irrelevant single-row palette selector: its first preview now genuinely starts at
the recovered `$8200`, and later previews use the last value accepted in conversion options.

The Wine oracle can now drive every Boolean Other Options control independently instead of merely
photographing the defaults. Its manifest records requested new/existing 8×8 optimization, 16×16
deduplication, layer priority, and both blank shortcuts alongside the observed five-byte native
flag block and priority byte. An all-inverted live capture proves all six widgets accepted their
requested states and the corresponding globals became `00 00 00 00 00` plus priority `01`.
The same oracle now drives the four bounded hexadecimal edit controls. A nondefault live capture
submitted first/blank 8×8 values `$220/$0F9` and first/reserved Map16 values `$8300/$8001`; the
four documented globals reopened as little-endian `00000220`, `000000F9`, `00008300`, and
`00008001` respectively, and conversion allocated graphics from the requested `$220` boundary.

The recovered SNES tileset importer is intentionally separate from legacy `Map16Page.bin` /
`Map16PageG.bin` transfer. Its pure staging model accepts a short graphics set only by zero-padding
to the native `$8000`-byte workspace, but requires complete tile-map and optional palette-row files
instead of reproducing Lunar Magic's unsafe truncated reads. Materialization is side-effect free:
it applies the caller's complete 1,024-entry graphics remap, preserves tile attributes, and returns
one graphics workspace plus one 256-definition page in native 32×32 quadrant geometry. Acts Like
is left as a placeholder because the eventual installed-ROM application must preserve the target
definition's value. UI loading, allocation, and the multi-domain transaction remain a subsequent
milestone and are not counted as complete parity yet.

Applying a materialized SNES tileset to a destination page has two explicit modes. Direct mode
copies only the four graphics words at every index, retaining all 256 destination Acts Like values.
Optimized mode performs stable first-occurrence deduplication across the imported page, counts exact
four-word `$1004` blanks on the selected page before writing, and rejects without mutation unless
every unique definition fits. Unique definitions then occupy blank indexes in ascending order, and
all 256 sources receive page-qualified global assignments for the later background index-grid paste.

The installed Map16 editor starts this workflow with Lunar Magic's ordered graphics-set, screen-map,
and optional palette-row dialogs. Each accepted request captures the application revision, selected
page, and direct/optimized choice before a bounded worker reads the files. Completion rejects stale
ROM state and builds one retained preview containing decoded graphics, the candidate Map16 page,
optional palette row, and all 256 global assignments. While loading or previewing, Map16 edits,
other transfers, commit, and close are gated. The preview deliberately has no partial Apply action.
Publishing runs through one cross-domain ROM transaction so graphics, palette, definitions, and
the optional background index grid cannot diverge.

SNES tileset materialization now accepts the combined native remap offset and one optional 16-entry
color map. The offset wraps before lookup, while remap destinations remain range-checked. Filtering
runs only after all source copies and only once per referenced destination, preserving the native
last-source-wins result when remap aliases exist. The installed preview captures both separate
offset controls, their ten-bit sum, the selected one of sixteen persistent color maps, and every
map entry before background loading; later UI changes cannot alter an in-flight request.

Apply now produces one revision-bound ROM mutation across the imported Map16 page, only the
referenced tiles of all eight active FG/BG/SP graphics slots, and the optional selected palette
row. Graphics ownership comes from the request-captured level's exact legacy object and sprite
tilesets. Duplicate GFX file assignments coalesce when their local writes agree and reject when two
VRAM slots demand nonrepresentable different pixels. Pristine palettes install the recovered custom
palette runtime inside the same private project before saving the row. Every saved GFX file, palette,
and Map16 result is reopened before publication; one application Undo restores the complete original
ROM. For optimized imports into pages `$80..$FF`, the same transaction also reproduces
`PasteMap16IndexGridIntoLevelLayer`: it requires at least one assignment in the selected active
`$1000`-tile background bank, masks every assignment to twelve bits, and writes the 16×16 grid at
the native Layer 2 storage index `((x >> 4) * 31 + y) * 16 + x`. Non-tilemap Layer 2 retains the
other imported domains, matching Lunar Magic's Cannot Modify path. The save authorizes only the
exact installed descriptor table, reopens Layer 2, and remains covered by byte-exact Undo.

Graphics persistence now follows Lunar Magic's 8×8-editor workspace rather than treating all
1,024 imported tiles as eight consecutive level files. Tiles `$000..$2FF` own FG/BG slots in VRAM
order `FG1, FG2, BG1, FG3, BG2, BG3`; legacy levels back the first four, enabled Super GFX Bypass
backs all six, and the unavailable fourth page `$300..$3FF` owns no file. Installed expanded
settings are authenticated through their built-in owner even without an external revision profile.
The bypass record's dialog order is converted to VRAM order before staging, duplicate-file conflict
checks remain atomic, every changed file reopens, and one Undo restores the combined installed ROM.

A disposable Lunar Magic 3.63 Wine oracle now authenticates the original SNES tileset-import
interaction as well as its decompiled implementation. The audit opens the hidden command from the
Map16 render child, captures the default checked Optimize control, switches to direct placement,
and supplies the graphics-set and `$0800`-byte screen-map files through the two native file dialogs.
On selected page `$00`, the retained clean run changed 5,491 of 65,536 decoded graphics bytes and
1,819 of 2,048 Map16 bytes. Its manifest records all four before/after SHA-256 values, selected page,
and exact buffer address. A separate observed optimized failure changed graphics before reporting
that the page lacked enough blank definitions; the Rust transaction deliberately offers stronger
failure atomicity rather than reproducing that partial mutation.

The native Map16 editor now also follows the dialog's process-lifetime state boundary. Optimize is
enabled on the first open, matching the retained original control snapshot, and every accepted
palette, placement, offset, filter-selector, and 16×16 color-map choice survives closing and
reopening the Map16 editor. Opening another ROM does not fabricate a fresh original process, so it
does not reset those choices either. Pending file work and previews remain editor-lifetime state and
are still cleared at close.

The original overworld appearance sidecar is `.sscov`, not `.ovssc`. The official Lunar Magic 3.63
help and `LoadCustomOverworldSpriteSidecar` agree on a UTF-8, ROM-basename-adjacent text file. Rust
now models its regular and `$100`-qualified custom sprite IDs, tooltip/position-text flags,
point-based Sprite Map16 compositions, optional editor shadow, translucent tile bit, positioned text
labels, and the `$10000`/`$20000` external graphics and palette range records. Decoding accepts the
native UTF-8 BOM, enforces Lunar Magic's 256-part, signed `$2FFF`, Sprite Map16 `$CFF`, and range
`$BFF` bounds, and applies later tooltip/appearance lines over earlier definitions. Canonical encode
round-trips the complete semantic value. The portable `LMOWAPP1` format remains separate because it
stores already-resolved 8×8 tile/palette parts. Native `.s16ov` is now modeled as the distinct
zero-filled `0x4000`-byte store for eight custom Sprite Map16 pages. Renderer resolution maps native
indexes `$000..$3FF` through the caller's built-ins and `$400..$BFF` through `.s16ov`, expands all
four 8×8 quadrants, and preserves labels, translucency, and shadows. The concrete raster route uses
packed-channel averaging for translucent parts, the authenticated editor font for labels, and the
authenticated dynamic definition/glyph/palette cache for internal `$C00..$CFF` definitions.

The portable overworld appearance editor now exposes bounded native-pair import and atomic
create-new export for `.sscov` plus its same-basename `.s16ov`. Export allocates one custom Sprite
Map16 definition per exact ordered 2×2 quadrant group; reciprocal import expands those definitions
back to the original four portable 8×8 parts. Import replaces the open document in one revision and
therefore supports one-step undo. Conversion rejects incomplete geometry, priority, translucency,
shadows, text labels, excessive definitions, and tile/coordinate overflow instead of narrowing
them. Original native pairs resolve `$000..$3FF` through Lunar Magic 3.63's exact embedded
8,192-byte resource type 500, ID 508, while `$400..$BFF` continues through sibling `.s16ov` data.

`NativeOverworldAppearanceController` now owns `.sscov` and `.s16ov` as one lossless revisioned
document rather than converting through `LMOWAPP1`. Typed atomic batches replace regular/custom
tooltips, tile or label appearances, graphics/palette ranges, and any custom Sprite Map16
definition. Canonical paired reopen validates both files before publication; history restores both
domains together, and immutable paired save snapshots retain native shadows, translucency, labels,
priority bits, range kinds, exact custom Map16 words, and the `.s16ov` loaded prefix length.

The native frontend recognizes `.sscov` in the appearance-document chooser and opens its required
same-basename `.s16ov` through one bounded two-file worker. A distinct native editor mode binds
directly to the lossless paired controller: regular and custom sprite IDs can edit tooltip enable,
position-text suppression, shadow, positioned labels, ordered Map16 parts, signed offsets, and
translucency, while the custom definition panel edits all four exact subtile words across
`$400..$BFF`. Undo/redo spans both files. Save uses the shared paired persistence worker and only
acknowledges the controller after both existing files have been replaced atomically; a disk reopen
test compares the complete semantic pair.

The lossless native mode also edits both ordered external-resource range families. Each graphics or
palette record retains its full 16-bit kind and base plus the inclusive native `$000..$BFF` tile
span; add/remove/apply replaces exactly one family in one revision, and canonical reopen rejects a
reversed or out-of-band span without value or history mutation. Map16 composition controls can move
the selected 16x16 part one position backward or forward in retained painter order. Focused form
tests load maximum-width range fields without narrowing.

`native-overworld-appearance-file` brings the same lossless pair to the built CLI. It boundedly
decodes both distinct inputs, optionally publishes canonical `.sscov` and exact-prefix `.s16ov`
outputs as one create-new batch, and can add one semantic observation to that same atomic group.
The observation addresses every tooltip flag/text, shadow, label, ordered tile part, signed offset,
translucency flag, graphics/palette range field, loaded Map16 prefix length, nonzero entry, and exact
prefix digest. All input/output/observation paths must be pairwise distinct. A built-process test
round-trips native-only fields through Unicode paths, while malformed or oversized input publishes
nothing.

Native overworld sprite resolution now carries Lunar Magic's exact per-Sprite-Map16 graphics and
palette routes. `NativeOverworldSpriteResourceMap` initializes `$000..$BFF` to `$1C00/$FFFF`,
retains `$3100/$FFFE` for internal `$C00..$CFF`, applies the four Ghidra-proved graphics transforms,
ignores adjusted graphics bases at `$4600+` and palette bases at `$0400+`, and preserves later-range
overwrite behavior. Each expanded 8x8 element retains the selected parent route alongside its tile,
palette, priority, flips, and translucency. Focused tests cover defaults, all transforms, ignored
limits, overlapping records, and internal sentinels; this is the source-selection contract used by
the installed-overworld preview integration rather than a guessed direct tile-array index.

The profiled installed-overworld open transaction now derives `.sscov` and `.s16ov` from the
actual ROM document path and requests both through the same non-blocking bounded loader as the
required palette-ownership evidence. A genuinely absent sibling is omitted independently; an
existing malformed, oversized, non-regular, or unreadable sibling rejects the whole workspace
before publication. Either native file can exist alone because Lunar Magic initializes the missing
domain to an empty definition map or zero-length Sprite Map16 prefix. The workspace retains the
lossless native pair and reports its appearance, tooltip, and exact loaded-prefix counts. Focused
tests cover no sidecars, either sidecar independently, Unicode basenames, malformed definitions,
duplicates, unexpected paths, and sidecars returned without an owning ROM path.

The installed-overworld preview now consumes that retained native pair instead of merely reporting
it. `ReloadOverworldGraphicsAssets` and `RenderOverworldLinkedTileOverlays` establish three exact
seven-map cache families: base graphics at `$0000/$0400/.../$1800`, active sprite graphics at
`$1C00/$1E00/.../$2800`, and animated graphics at `$2A00..$3000`. Rust loads the base words in
native reverse order 7 through 0, SP1-SP4 from words 11 through 8, and the animated source from
word 0; `$7F` remains a zero slot. Native placements carry their actual submap through Map16
expansion, retain all ten tile bits, and raster through the selected global cache with the recovered
active-palette `$00/$80` color offsets. The same bounded open transaction discovers Lunar Magic's
nearest `ExternalGraphics` directory, prefers `ExSpritePalette00.mw3` over `.pal`, and loads
`ExSpriteGFX00.bin`; `$4200` graphics routes and palette bases below `$0400` then resolve through
those decoded assets without reducing RGB24 colors to BGR555. Internal `$3100/$FFFE` definitions
continue through the authenticated editor-text cache. `resource_routes_materialize_all_recovered_submap_cache_tables`,
`resource_raster_retains_ten_bit_tiles_and_external_rgb_assets`, the authentic-ROM overworld open,
and the full 230-test renderer suite cover this live composition boundary.

Binary MWL import/export is physical-ROM invariant for the supported installed SMW-US runtime.
An authentic Lunar Magic level export with a semantic Layer 1 header change crosses the
profile-derived transactional import on both headered and headerless copies, retains or omits the
copier header exactly, repairs each checksum, and yields identical logical ROMs. Existing batch
export evidence covers all 512 levels across the same two physical forms.

Directory MWL insertion uses that same physical-ROM invariant transaction repeatedly. A retained
variant gate applies authentic edited levels `$000` and `$001` with a malformed MWL between them,
proving that a per-file failure changes no bytes and does not prevent the next level from
committing. Both headered and headerless installed inputs retain their physical framing, finish
checksum-valid, and converge on the same logical ROM.

Title-screen tilemap replacement is now explicitly invariant across canonical headered and
headerless SMW-US ROMs. The same application transaction installs pristine tilemap storage into
the owned expanded form, updates that installed payload a second time, repairs and reopens both
results, and preserves the physical copier prefix exactly. Both forms converge logically and two
undo steps restore their exact original physical images.

Credits tilemap replacement has the same explicit physical-ROM proof across its distinct storage
families. The application expands the original 202-row legacy form into the owned complete
256-row runtime, updates that owned payload again, repairs and reopens both results, and preserves
the canonical copier prefix only when present. Headered and headerless results are logically
identical, and two undo steps restore both original physical images byte-for-byte.

The SNES graphics-set/Map16 importer now has a combined optimized-plus-palette transaction gate.
On both headered and headerless pristine ROMs, blank-definition deduplication, referenced graphics
publication, background Map16 replacement, palette-row import, and Layer 2 index-grid paste commit
as one mutation. Every domain reopens exactly, checksums and physical copier framing remain valid,
the logical outputs match byte-for-byte, and one undo restores each original image.

The complete overworld event workflow is now crossed with both supported physical ROM forms.
Four sequential application transactions edit the event-number map, install the main reveal table,
install all special reveals and directions, and install both event tilemap planes. Each domain
reopens from the final checksum-valid ROM, headered and headerless results match logically while
retaining their exact copier framing, and four undo steps restore each physical original.

The complete overworld metadata workflow now covers both installation and installed-runtime
updates on headered and headerless ROMs. Seven sequential transactions install settings, level
names, and messages, edit player starts, then update each installed variable-sized runtime.
Settings, names, messages, and starts all reopen exactly from checksum-valid results; physical
copier framing is preserved, logical outputs match, and seven undos recover each original ROM.

The level renderer now implements Lunar Magic's two independent Map16 outline commands rather than
approximating them with vector borders. `LM_VIEW_SURFACE_OUTLINE` (`$2410`) and
`LM_VIEW_LINE_GUIDE_OUTLINE` (`$2411`) toggle the state consumed by the live canvas. PE resource
500/524 is retained as a compact text-encoded PNG and decoded once into the original 1,808×16 atlas
of 113 16×16 glyphs, with the original magenta transparency key. Surface lookup reproduces the
recovered 512-byte initialization table and object-tileset substitutions; line-guide lookup covers
the vanilla `$76..$99` roots and pristine `$95/$62` conditional. Custom Map16 cells follow bounded
Acts Like chains to a vanilla root. Both object-stream caches and tilemap-backed Layer 2 composite
the glyph after the Map16 artwork, matching `RenderMap16TileToPixelBuffer` ordering.

The native level canvas now implements the default-off `LM_VIEW_BLOCK_CONTENTS` presentation
state. Its public materializer exposes Lunar Magic's exact built-in Acts Like/position mapping
words without discarding `$4000/$8000`, while the compositor draws the mapped definition after the
ordinary cell through the extracted 8,192-byte default M16 bank. Color-zero pixels remain
transparent, `$4000` uses the recovered three-quarter source composition, and `$8000` uses the
half-color path. The user-toolbar name routes to the same state and never mutates authored object
or Map16 data.

The companion default-off `LM_VIEW_BLOCK_EXITS` view runs as a post-artwork cell pass. It uses the
recovered built-in Acts Like roots and mode-1 exception, collapses painter history to each final
Map16 value, and draws Lunar Magic's four-pixel black/red/black double outline at logical offsets
0–3 and 12–15. It intentionally bypasses the general translucent-overlay opacity, matching the
native routine's temporary flag clear. Custom `.dsc` flag-eight markers remain pending live DSC
ownership in the installed canvas.

## Temporary title-movement joypad recorder parity

The title-recording editor now exposes Lunar Magic's separate temporary recording runtime instead
of treating playback-data import as the entire workflow. The project model authenticates pristine
or installed hook pairs, requires exact RATS ownership and all 178 runtime bytes, installs with the
original low-bank LoROM pointer convention, preserves the stored checksum through the recovered
`$07EFA3..$07F08D` additive run, and removes only a fully authenticated owner. Install, uninstall,
Undo, Redo, stale revisions, and native warning text are covered. A retained PID-scoped Wine
oracle binds the exact 347-byte Lunar Magic 3.63 mutation, complete output hash, reciprocal removal,
and Cancel atomicity. The independent vanilla oracle additionally proves Lunar Magic's 1 MiB
expansion shape: internal ROM-size byte, fixed metadata, boundary RATS allocation, checksum
compensation, and complete headerless SHA-256
`663f824b807c8addc81be50b35cd6d2b5f714427063107ddc52aa037c962341f`. Rust produces that exact
image and undoes it byte-for-byte.

The temporary runtime is now exercised rather than only inspected. The supplied deterministic
libretro driver boots that exact Rust/original-equivalent image in an official Snes9x core, follows
the real title/file/intro/overworld path into level mode, waits 600 frames, then supplies B, A, and
release input. `rust_title_recorder_captures_real_joypad_input_in_snes9x` requires the runtime's
`$0042` marker at WRAM `$7F:FFFC`, its bounded encoded length at `$7F:FFF8`, and the exact 25-byte
recording `00 00 00 00 00 00 00 00 58 80 08 01 80 00 0B 80 C0 01 80 80 08 00 00 07 FF`.
The three leading records prove the runtime's 256-frame zero-duration convention rather than
mistaking it for malformed output. The gate also requires a bounded nonblank gameplay PNG, installs
the captured bytes into the playback runtime, and semantically reopens the same recording.

The playback-import half now has its own retained Lunar Magic 3.63 oracle. A deterministic
four-byte movement payload inside a minimal ZSNES V143 state produces a 335-byte mutation whose
complete ROM SHA-256 matches Rust byte-for-byte. That comparison corrected four coupled errors:
the runtime's `+9` word remains fixed at zero; the recording owner is allocated before the runtime;
only zero-filled expanded-ROM space at or above the original 512 KiB boundary is eligible; and an
already expanded ROM is not grown merely because the fallback search range permits another bank.
The transaction also uses Lunar Magic's checksum-compensation run to retain the stored checksum.
Both the confirmation Cancel and common-file-dialog Cancel paths are byte-identical.
Lunar Magic's `-ImportTitleMoves` and `-ExportTitleMoves` batch routes close the remaining file
boundary: valid import reproduces the GUI/Rust ROM hash, export recreates the minimal ZSNES state
byte-for-byte, a 12-byte state rejects without mutation, and export without installed playback
creates no file.
On a 512 KiB vanilla source, the same route follows Lunar Magic's confirmed 1 MiB expansion path:
it updates the internal ROM-size byte, initializes the fixed metadata/padding family, allocates at
the new boundary, retains the copier prefix and stored checksum through compensation, and produces
the exact authenticated Lunar Magic output. Expansion and installation remain one undo operation.
Replacement zero-fills the prior recording owner and may consume the entire compensation run
through `$07F09F`; the retained 257-byte update matches Lunar Magic byte-for-byte and remains a
second independently undoable transaction.

## ExLoROM standard graphics and ExGFX conversion parity

The original-tool conversion gate exposed a pointer-canonicalization bug that ordinary LoROM
reopen tests could not detect. Rust's generic `pc_to_snes(LoRom, ...)` intentionally chooses the
fast `$80..$FF` mirror. Standard graphics split planes and the shared-bank GFX32/GFX33 startup
operands had inherited that representation. It is equivalent in LoROM, but address bit 23 becomes
mapper-significant in ExLoROM, so a copied `$90:xxxx` pointer selected the low 4 MiB half instead of
the relocated SMW body and Lunar Magic exported zero-filled graphics.

Payload publication now has explicit low-bank forms for split-byte and split-word/shared-bank
pointers. Graphics layouts select those forms only for LoROM; ExLoROM and SA-1 preserve their
canonical significant high banks. The mapper-aware ExGFX route resolves all three relocated pointer
tables, upgrades a converted relocated `$C30` expanded-ExAnimation runtime to the metadata-selected
`$C50` family without losing its exact `$600` pointer table, and reopens every inserted payload
before publication. The live Wine gate covers headered, headerless, Fast LoROM, and the complete
8 MiB ExLoROM transition, requiring Lunar Magic 3.63 to export all 52 GFX files plus `ExGFX80` and
newly inserted `ExGFX81` byte-for-byte.

## Historical optimized-LZ2 runtime generation

A patch-derived 2 MiB LoROM supplies an exact pre-3.63 optimized-LZ2 runtime generation. Its
metadata value 1 and `$0038E3` hook select a `$1AF`-byte RATS payload with CRC-32 `b5f7eda1` and
the generation trailer `LM 00 01`. Detection now accepts that exact length/checksum/trailer tuple
alongside the current `$1C0`/`LM 01 01` tuple; it does not weaken authentication to metadata or a
trailer prefix. Synthetic corruption and wrong-generation tests reject, while the opt-in authentic
ROM gate decodes all 50 ordinary files plus GFX33/GFX32 without changing the image.

Original Lunar Magic 3.63 accepts the historical ROM and converts it to LZ3, but its conversion is
not codec-only. All 54 ExGFX exports and 51 standard GFX exports remain exact, while GFX17 gains
an opaque fourth plane on tiles `$00/$01/$10/$11`. Rust now reproduces that exact upgrade. The
legacy `$100..$FFF` ExGFX table follows the live `$07F873` operand into an authenticated relocated
`$6D00` expanded-settings owner rather than the current fixed `$088000` assumption; the 54 live
files include two bounded `$FFF`-byte streams that must remain lossless during codec migration.

The same source uses an earlier event-tilemap loader. Its primary runtime has two distinct branch
bytes, its three JSL hooks use the equivalent high LoROM mirror, and its reveal runtime carries the
older zero constant. Both the historical LZ2 ROM and Lunar Magic's LZ3 result retain that exact
generation. Rust authenticates either event-loader family, migrates both event streams, matches the
original editor's 52 standard and 54 ExGFX exports, repairs the checksum, and restores the exact
source through Undo. Lunar Magic reports the Rust result is already LZ3, leaves its SHA-256
unchanged, and re-exports every graphics file byte-for-byte. Exact provenance and hashes are
retained in `oracle-work/graphics-compression-lz2-speed-generation-100.md`.

The reciprocal boundary is verified too. Starting from that migrated historical LZ3 image, Rust
returns to both `LZ2 Orig` and `LZ2 Speed` without losing the upgraded GFX17 plane, either odd-sized
legacy ExGFX stream, or either event buffer. Both transactions retain ROM size, repair checksum,
semantically reopen, and Undo to the exact LZ3 input. Lunar Magic 3.63 recognizes each Rust output
as already using the requested LZ2 mode, leaves its SHA-256 unchanged, and exports the same 52 GFX
and 54 ExGFX files as its own corresponding reverse conversion.

## Mapper-aware built-in overworld animation sources

The installed overworld preview no longer assumes that Lunar Magic's 67-word built-in animation
table always lives at logical `$020000`. Original function `$004BA8D0` reads `$86` bytes through
active descriptor field `+$5C4`. The profile layer mirrors its three recovered destinations:
ordinary SMW/SA-1 `$020000`, ExLoROM `$420000` in the active upper SMW body, and All-Stars + World
`$1A0000`. Physical descriptor values include the `$200` copier prefix; all project offsets are
normalized to logical ROM bytes.

The selected table is mandatory and fail-closed. All 67 source words must address the recovered
VRAM cache interval `$2000..$C7FF`; truncation or one invalid word rejects the native open. No
fallback probes the LoROM location, which prevents a valid lower ExLoROM mirror from hiding a
corrupt active table. Profile routing tests distinguish all three tables, lifecycle tests retain a
valid ordinary-table decoy while corrupting ExLoROM/All-Stars, and authentic pristine/full-raster plus
built-in phase tests retain the renderer boundary.

Lightning sources now follow descriptor fields `+$904` and `+$90C` instead of fixed SMW-US
constants. The layout resolver covers the eight-byte-shifted Japanese routine, relocated
All-Stars + World routine, ExLoROM upper body, and unchanged SA-1 location. It derives the selector
start from the mask operand, then authenticates the delay/color bounds and exact routine prologue.
Focused lifecycle tests copy the authentic source family into every selected layout, corrupt that
selected prologue, and retain other plausible families as decoys. Existing wrapping-counter,
pre-decrement-color, submap-isolation, and authentic pristine gates keep the rendered sequence
unchanged.

Overworld ExAnimation ownership navigation is derived rather than supplied by a detached manifest.
The native editor materializes each valid destination span with the renderer's transfer helpers,
caps local and global domains at 32 records, applies the current map's enable switches, and records
writers in painter order so later local records and then global records win overlaps. Ctrl+Shift
clicks on palette colors and the rendered 8x8 graphics cache select the exact owner. Global owners
use a read-only global form in this aggregate editor, preventing a global record from entering the
local-overworld edit command; ordinary local owners remain editable. Alt is intentionally ignored
for this chord, matching the two independent native Ctrl/Shift key-state tests.

## Native overworld sprite canvas-plane routing and selection

The installed-overworld renderer must translate native custom-sprite coordinates differently from
ordinary overworld records. Map 0 owns canvas pixels `0..=511`; maps 1–6 share the right-hand
canvas plane and therefore add 512 to their map-local X coordinate only while rendering. The ROM
model remains map-local, so saving and reopening do not absorb that visual offset.

The native-sprite canvas tool resolves the same appearance elements used for painting and hit-tests
them in reverse painter order. Resolved and internal-text subtiles use half-open 8x8 regions,
unresolved Map16 references retain 16x16 regions, and labels reuse the authenticated font's exact
advance widths and text height. Signed display offsets therefore participate in selection without
changing the record origin. Records with no drawable definition retain an 8x8 anchor fallback;
records with a drawable definition do not gain an invisible anchor target. An occupied click
selects the last/topmost matching record and reloads its complete variable-width form. A drag
captures that record identity and stages exactly one replacement at release; an empty click retains
insert/move behavior. Cross-plane points cannot select or mutate a record.

## User-toolbar coordinate adjustment

The authenticated Lunar Magic 3.63 internal-command table assigns `$2460..$2463` to increase and
decrease X/Y. The native toolbar routes all four commands into the installed level editor and
translates the complete active Layer 1, object-backed Layer 2, or sprite selection by one visible
tile. Horizontal levels map X/Y to native major/minor coordinates directly; vertical levels swap
those storage axes while preserving the screen-space meaning.

These actions share canvas dragging's semantic group-relocation edits. They regenerate owned
screen controls, reject invalid boundary movement atomically, and follow canonical post-rewrite
record indexes. `toolbar_coordinate_commands_nudge_objects_and_sprites_through_staged_history`
moves both domains, proves staged Undo, commits into an expanded vanilla ROM, semantically reopens
the moved sprite, and proves application Undo restores the byte-exact expanded baseline. The
authenticated native-route count therefore advances from 187 to 191 table slots.

## Legacy one-step level Z order

Lunar Magic's 3.63 CHM distinguishes the hidden legacy `LM_EDIT_ZORDER_UP`/`DOWN` commands from
the four newer menu commands. The 2.30 change log says Increase/Decrease Z Order move one raw
creation-order step, while Bring Forward/Send Backward skip non-overlapping records. The legacy
commands remain bound to Ctrl+Alt+Shift+Plus/Minus even though they were removed from the menu.

Rust routes authenticated commands `$245E/$245F` only through the one-step operation. A complete
Layer 1, object-backed Layer 2, or sprite selection moves stably past one adjacent unselected
record. Object serialization regenerates forward and backtracking screen jumps so creation order
can change without changing any absolute placement. Sprite serialization permits the step only
inside the same game-imposed screen and, for expanded streams, upper-Y/orientation group. This is
the original's documented “if possible” constraint.

Model tests cover multi-selection stability, cross-screen object backtracking, legacy sprite screen
boundaries, and expanded vertical grouping. The installed-editor test performs both object and
sprite actions against vanilla level `$105`, proves placement preservation and staged Undo, commits
the sprite ordering, semantically reopens it, and restores the byte-exact expanded ROM through
application Undo. Native authenticated routing advances from 191 to 193 table slots. The four
overlap-aware commands are implemented separately in the following milestone.

## Rendered-overlap-aware level Z order

The CHM defines Bring Forward and Send Backward as moving past at least one overlapping creation,
while Bring to Front and Send to Back traverse every overlapping creation. Non-overlapping records
on the same screen are skipped. Rust keeps these four commands separate from legacy one-step order.

The canvas caches the exact interactive regions already used for selection: complete built-in and
custom object artwork with encoded fallback regions, and complete standard, custom, external, or
unresolved sprite preview bounds. Touching edges are not an overlap. Forward/backward selects the
nearest eligible intersection; front/back selects the farthest. Stable reverse/forward traversal
preserves relative order for multi-selections. Sprites cannot cross a distinct native screen or an
expanded upper-Y/orientation sort group.

The resulting full identity permutation is applied as one semantic transaction. Object streams
regenerate all necessary forward/backtracking jumps while preserving absolute coordinates; sprite
streams rebuild minimum upper-Y controls and reject a noncanonical permutation. Tests distinguish
nearest/farthest traversal, skipped nonintersections, strict edge contact, incompatible sprite
groups, invalid permutations, position preservation, staged Undo, vanilla-ROM commit/reopen, and
byte-exact application Undo. Commands `$246A..$246D` raise authenticated native routing from 193 to
197 table slots.

## Ordinary graphics insertion option boundary

Lunar Magic's CHM and PE dialogs `$03EC`/`$03FE` distinguish ordinary GFX/ExGFX insertion from the
quiet toolbar buttons. Standard GFX defaults to physical PC `$40200` and offers pre-expansion to
1 MiB; ExGFX defaults to `$100200` and offers 2 MiB. Both expose the irreversible 3bpp-to-4bpp ASM
choice. The physical defaults include the original copier prefix, so Rust normalizes them to
logical `$40000`/`$100000` and adjusts the displayed address for headerless images.

Rust now has a typed, bounded modal for both authenticated ordinary commands. It accepts original
`$`, `0x`, or bare hexadecimal forms, rejects copier-prefix and 8 MiB overrun addresses, retains the
original expansion thresholds and reciprocal GFX/ExGFX format warnings, and opens directly from an
application ROM snapshot without requiring the graphics-editor window. On an already authenticated
4bpp installation, standard-GFX acceptance consumes the same fixed ROM-sibling
`Graphics`/`AllGFX.bin` source as Lunar Magic, starts allocation at the requested logical cursor,
optionally expands first, and combines expansion plus graphics writes into one original-length
mutation and Undo step. Clearing 4bpp after installation correctly leaves the irreversible runtime
installed.

`graphics_insertion_dialog::tests`,
`ordinary_options_expand_before_binding_the_exact_allocation_cursor`,
`expansion_and_prepared_graphics_write_combine_into_one_original_length_mutation`, and
`ordinary_insertion_dialog_opens_from_app_state_without_graphics_workspace` bind the recovered
defaults, header variants, bounds, expansion composition, and global route. First-time ordinary
4bpp insertion now authenticates every vanilla compressed stream before reclaiming only those
exact extents, expands and installs the recovered runtime, inserts GFX33/GFX32 first in one shared
LoROM bank at or after the requested cursor, inserts the remaining 50 files, reopens all 52, and
undoes to the byte-exact vanilla ROM. The quick route retains its established allocation order.
`quick_standard_insertion_commits_reopens_and_undoes_from_fixed_directory` crosses that behavior
through the installed modal worker from a pristine ROM. First-time ordinary 3bpp insertion derives
each native depth from the authenticated vanilla stream, discards only the editable fourth plane
for actual 3bpp files, preserves already-native files byte-for-byte, and never installs the 4bpp
runtime. Separate-file insertion at the default cursor remains 512 KiB; joined insertion with the
expansion option becomes 1 MiB. Both reopen every native file, while re-extraction reproduces all
52 editable input files exactly and one Undo restores vanilla. Ordinary ExGFX directory insertion
now passes the selected logical cursor through every native storage-domain allocation while
retaining domain order, synchronization/reclamation, semantic reopen, and one Undo. The installed
worker gate replaces `ExGFX80` at `$190000` or later and restores the exact prior ROM. That path
also corrected expanded-ExAnimation authentication to accept the canonical `$FF FF FF` empty
sentinels in reserved ExGFX slots `$61..$63`, while retaining the required four-byte zero trailer.
First-time ExGFX insertion now works directly from the vanilla ROM without first installing the
irreversible 4bpp runtime. Ghidra recovers the exact selector at `$0047E3C6..$0047E3E3` and the
matching extractor selector at `$0047F175..$0047F1A0`: `$80..$DFF` discards the odd fourth-plane
byte from each planar pair when the 4bpp markers are absent, while `$E00..$FFF` always selects zero
and remains byte-for-byte “as-is” before LZ2 compression. `$004410D0` proves extraction restores
the packed 3bpp tiles to editable 4bpp shape with zero fourth-plane bytes.

The same transition now installs the recovered 32-byte graphics data block through the long-call
hook at `$0013F7`, initializes the Ready marker, follows the relocated expanded-settings owner for
the `$100..$FFF` pointer table, expands to 2 MiB, and preserves the selected allocation cursor.
`pristine_first_exgfx_preserves_e00_but_round_trips_ordinary_files_as_3bpp` proves semantic
save/reopen and byte-exact Undo; the installed
`quick_exgraphics_insertion_supports_pristine_three_bpp_rom_and_exact_undo` gate additionally
inserts from the fixed `ExGraphics` directory, re-exports both converted `ExGFX80` and unchanged
`ExGFXE00`, and undoes to vanilla. The original interactive retry/prompt after cursor-space
exhaustion remains incomplete, so the broader ordinary-command interaction milestone stays partial.

## Expanded-settings allocation on pre-expanded ROMs

The original expanded-settings prerequisite does not limit its `$6E00` owner to the first MiB.
Ghidra's `AllocateRomSpaceWithExpansion` at `$004A8810`, called by the installer at `$00460C3A`,
searches the current mapper extent and invokes `RequestRomExpansionForAllocation` only after a
failed search. Rust now exposes the corresponding failure-atomic patch transaction: the initial
plan uses all current LoROM space (while retaining pristine SMW's authenticated one-MiB target),
then retries on a private staging image one 32-KiB mapper bank at a time through 4 MiB. Only the
successful attempt enters project history.

The application, CLI expanded-settings installer, and native-overworld-settings import route all
use the ROM-aware plan. Headered and headerless 2-MiB sources place the owner in available late
space without growing; an exhausted 1-MiB source grows by exactly one bank. Every route repairs the
checksum, resolves the installed table from its relocated runtime operand, preserves the copier
header, and restores the exact physical input with one Undo.

## Persisted original-dialog localization inventory

`LocalizationCatalog` keeps its original `LMLOC001` typed-key stream byte-compatible and appends
`LMDLG001` only when converted original dialog text exists. Each bounded record contains the
original dialog ID, exact template item position, original 32-bit control ID, and literal UTF-8
caption. Position is part of the identity because Win32 templates can repeat control IDs. A
canonical `(item = u16::MAX, control = u32::MAX)` record represents the dialog title. The decoder
accepts historical catalogs with no extension and otherwise requires one fully consumed,
duplicate-free extension of at most 4,096 entries.

Original DLL conversion inserts all safely decoded titles and literal control captions before its
smaller typed-key compatibility overrides. The native secondary-exit editor is the first form to
query this inventory: original dialog `$03F1` supplies the window title and exact `$66`, `$65`,
`$6C`, `$DB`, `$67`, and `$69` captions, while every absent caption retains its native English
fallback. This establishes the reusable binding path without falsely treating untranslated
native forms as complete Localization parity.

The same lookup boundary now drives the extracted undo-history form from original General Options
`$041F` and every equivalent control in graphics compression dialog `$0416`. Both helpers return
owned strings because typed localization can synthesize an owned fallback while original-template
text borrows the installed catalog. This prevents either lifetime source from escaping the frame
and keeps partial original modules safe: every absent title/control falls back independently.
Authenticated internal command `LM_OPTIONS_GENERAL` (`$24D7`) now opens that same `$041F` form at
the application's current undo-history limit. A dispatch-level gate proves the toolbar route opens
the real native workspace rather than merely counting the command as supported; Apply, Cancel,
bounded persistence, and restart behavior remain shared with the existing Tools-menu route.
Authenticated internal command `LM_OPTIONS_RESTORE` (`$24CE`) likewise opens the existing automatic
restore-point policy workspace. It loads the persisted interval-enabled/count, daily-full, and
pre-destructive-full defaults; the established Apply/Cancel path and archive tests remain shared.
The dispatch gate proves the command opens that workspace, and ten restore-point tests cover policy
encoding, daily decisions, associated files, publication, and archive continuity.

`GraphicsMigrationTarget` separates Lunar Magic's three visible compression choices from the two
payload codecs. LZ2 Orig and LZ2 Speed both map to `GraphicsCompression::Lz2`, but Speed routes to
the authenticated runtime command instead of becoming a same-codec no-op. Installed runtime
detection selects Speed from Orig and LZ3 from either installed Speed or LZ3 while preserving the
existing profile, identity, and revision checks.

## Native external-tool configuration editor

The native editor drafts the complete `ExternalTool` collection and publishes it only through
`AppState::set_external_tools`, reusing the canonical duplicate-ID, duplicate-subscription, empty
field, and placeholder validation boundary. Executable paths remain platform paths; arguments are
entered one per line and retained as independent argv elements; an empty working-directory field
maps to `None`; and three booleans reconstruct subscriptions in canonical event order. Cancel and
window close discard the draft without touching active tools. Setup Emulator resource `$0407`
supplies every equivalent caption while Rust-only safety and subscription controls retain explicit
English labels.

Emulator family identity is append-free: IDs beginning with canonical `gba-` select original GBA
dialog `$0408`; every other tool selects the SNES `$0407` form. New GBA drafts use
`gba-emulator-N`, so kind survives publication, configuration encode/decode, and editor reopen with
no new persisted field. Users can intentionally change the stable ID and thereby reclassify a tool.

The original 8.3 checkbox is represented by the explicit `{rom_8dot3}` placeholder rather than a
new configuration flag. Draft toggling reversibly rewrites `{rom}`/`{rom_8dot3}` while preserving
all other argv text. On Windows, `lm-windows::short_path` passes a NUL-terminated UTF-16 source and
a fixed 32,768-unit output buffer to `GetShortPathNameW`, rejects zero/oversized results, and returns
an owned platform path. No shell, lossy tokenization, registry lookup, or unbounded allocation is
involved. Non-Windows builds keep the configuration readable but reject execution with
`ShortRomPathUnavailable`.

User-toolbar external launch policy now carries two original options through the permission gate.
`LM_ALLOW_MULT_INSTANCES` and global `LM_ALLOW_MULT_INSTANCES_FORCE_ALL` bypass the default
same-button de-duplication; approved children run concurrently and retain independent cancellation
channels and UI identities. `LM_NO_CONSOLE_WINDOW` is applied only at the Windows process boundary
as `CREATE_NO_WINDOW`. Other systems accept the portable toolbar configuration but make no false
claim that a Windows console exists to hide.

`LM_OPEN_OTHER` branches only after the ordinary permission approval. Windows passes a
NUL-validated target, quoted parameter line, and optional working directory through the safe
`lm-windows::shell_open` wrapper to `ShellExecuteW`; the API intentionally yields no process handle.
macOS spawns `/usr/bin/open` and preserves optional application arguments after `--args`; other Unix
systems spawn `xdg-open` only for an argument-free target. These opener processes are dropped
immediately and the associated application is never entered into owned-child close/cancel state.

The default single-instance policy now also retains Lunar Magic's second-click behavior. The
authenticated 3.63 CHM says another click switches focus to the already open program. Native
workers publish their child PID; Windows enumerates visible top-level windows owned by that PID,
restores the first minimized candidate with `ShowWindow(SW_RESTORE)`, and makes the best-effort
`SetForegroundWindow` request instead of enqueuing another permission prompt or process. A child
that has not created a window yet remains a harmless no-op. `LM_ALLOW_MULT_INSTANCES` and its
force-all form continue to bypass this reuse path.

Nine additional authenticated internal-toolbar slots now route through native application state.
`LM_FILE_EMULATOR_RUN` selects the first configured non-GBA tool that explicitly consumes `{rom}`
or `{rom_8dot3}` and otherwise uses the same direct chooser as the Tools menu. Emulator and tile
editor setup both open the failure-atomic external-tool editor, and compression options open the
existing authenticated migration dialog. The five implemented internal-emulator controls call the
same `LiveEmulator` methods as its window: Run starts the core chooser, Unload tears down the owned
worker, Pause toggles the recovered manual hard-pause reason, Mute preserves video/session state,
and Frame Advance first establishes hard pause then emits exactly one step. Missing sessions report
an ordinary application error instead of silently pretending the command ran.

External-tool configuration now survives native application restarts and migrates the original
Windows profiles once when no native or explicit startup configuration exists. Eframe storage owns
canonical bounded `LMTOOLS1` bytes under a versioned key; decoding and publication are atomic, and
malformed native data never falls through to a registry profile that could silently replace the
active collection. Windows reads only the six recovered `REG_SZ` values and two option DWORDs from
Lunar Magic's per-user settings key through `lm-windows`; each UTF-16 value is type-checked,
NUL-checked, capped at the original 0x410 UTF-16-unit exchange shape, and rejected above 0x40f
UTF-8 payload bytes before conversion. Missing keys and values are ordinary
absence, while wrong types, invalid UTF-16, races beyond the probed size, and excess sizes fail.

Migration reproduces the recovered packed flags: `Options` bit 29 selects SNES custom arguments;
`Options2` bits 16/17 choose SNES/GBA 8.3 ROM paths, bit 18 selects GBA custom arguments, and bit 24
selects the tile-editor template. Disabled custom arguments deliberately ignore stored tails and
emit exactly one typed path placeholder. Enabled tails use Microsoft C-runtime quote/backslash
splitting, retain empty arguments, escape literal Rust-template braces, and translate every `%1`
to `{rom}`, `{rom_8dot3}`, or `{graphics}` without a command shell. Empty executable paths create
no phantom tool. A seeded isolated-Wine registry test exercises the actual Win32 ABI, while native
round-trip and failure-atomic tests cover all three profiles, every recovered launch bit, Unicode,
spaces, empty arguments, default arguments, and malformed preferences.

Lunar Magic notification payloads are a typed core value rather than scattered Win32 literals:
message `$BECA`, confirmation `$6942`, six-bit kinds 0 through 6, and a checked ten-bit variable.
The cancellable process worker publishes the OS PID after successful spawn. The Windows boundary
enumerates every top-level window owned by that PID and uses `PostMessageW`, intentionally applying
no visibility filter. Type 0 uses a retained hidden top-level STATIC caption window so cross-process
`GetWindowText(wParam)` returns the current ROM path; types 1 and 2 use zero `wParam`. Toolbar
option selection is external-only and exact, including the documented new-ROM/new-level/close
force-all directives. Installed level, secondary-exit/asset, Map16, and overworld commits set three
coalescing domain bits. Only a successful application ROM persistence acknowledgement consumes
those bits and publishes save-level 3, save-Map16 4, and save-overworld 5; failures retain dirty
state without notification, while undoing to clean clears stale bits.

The first native deletion layer now exists in `lm-project`: it requires the selected Layer 1
pointer to be in the expanded ROM area, redirects Layer 1 and sprite streams to caller-authenticated
original-area test pointers, reference-counts both complete pointer tables before reclaiming any
displaced tagged stream, repairs the checksum, and publishes the redirect plus erasures as one
undoable transaction. Shared tagged streams remain intact.

The aggregate deletion route now closes that wiring for Layer 1, sprites, Layer 2, palette,
ExAnimation, the Layer 2 descriptor, expanded settings, vanilla entrance planes, and installed
Lfix3 fields. It authenticates the original-area replacement slot, performs one application
revision and one Undo step, exposes a localized confirmation-gated File command, routes the
authenticated `LM_FILE_DELETE_LEVEL` toolbar command, and publishes notification type 6 only after
successful physical ROM persistence. Failed or cancelled saves retain the pending notification;
returning to a clean baseline without persistence clears it. The live `-DeleteLevels -LevelList 0`
oracle matches all modeled pointers and
direct records. The later multi-level parity pass also reproduces Lunar Magic's two zero-filled
`$1FE` secondary-exit reservation owners, their first-fit relocation, null-tail removal, and
checksum-compensation bytes, closing the earlier allocator-bookkeeping difference. This route
raises authenticated native user-toolbar coverage to 200 table slots.
## Legacy standard-GFX bypass lists (`$2520` / `$2521`)

Port-8089 recovery of `ManageSuperExGfxConfiguration` (`$0048E900`) binds selector 1/resource
`$03FB` to the standard FG/BG dialog and selector 0/resource `$03FC` to the sprite dialog. Both
dialogs read and write one exact `$400`-byte table through active-descriptor field `+$194`, expose
rows `$00..=$FE`, and retain row `$FF` without making it selectable. Physical rows reverse the four
dialog/VRAM slots. A retained Wine differential selected FG/BG row `$05`, entered files
`01,02,03,04`, and produced physical bytes `03 04 02 01` at logical `$07F200`.

The selected rows live in three-byte object-stream command `$24`. Zero disables a domain and an
enabled row is stored plus one. Horizontal levels store the sprite selector high nibble then low
nibble; vertical levels swap those nibbles. The FG/BG selector occupies byte three. Rust models the
complete shared table and both selectors, commits table and any required Layer 1 relocation as one
revision-checked mutation, repairs checksum, and verifies semantic reopen. Separate native dialogs
route `LM_LEVEL_BYPASS_FG` and `LM_LEVEL_BYPASS_SP`; pristine ROMs install the expanded-settings
prerequisite and resume the requested dialog. Headered and headerless transaction tests prove exact
logical equivalence and one-step Undo. The complete native suite passes 1,000 tests with 12 explicit
external-fixture ignores; the 512-slot renderer manifest remains byte-identical at SHA-256
`254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`.

## Graphics-insertion 4bpp patch default (`$24D8`)

The authenticated command table maps `LM_OPTIONS_4BPP_PATCH` to selector `$A1`. The original
handler toggles default-on, session-only byte `$005E7BA3`; it has no registry serialization
reference. Lunar Magic's retained help identifies the consumer precisely: the value initializes
the **Modify the ROM with ASM to use 4bpp tiles instead of 3bpp tiles** choice for both ordinary
GFX and ExGFX insertion. Once the irreversible runtime is already installed, clearing the option
cannot uninstall it.

Rust now routes the command to the real ROM-graphics insertion workspace. Pristine insertion
dialogs inherit the session toggle, while an authenticated installed 4bpp runtime forces the
choice on regardless of a later disabled default. Focused route, default-on, two-way toggle,
status, and insertion-dialog consumer tests pass. Authenticated native command coverage is now
311 of 317 named slots, leaving six ROM-patch options pending.

The complete native gate passes 1,149 tests with 13 explicit external-fixture ignores, the
renderer remains green at 235/235 tests, all 512 pristine levels materialize, and the regenerated
513-line semantic manifest remains byte-identical at SHA-256
`254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`, and the i686 Windows
cross-build passes.

## Other Super GFX bypass preference (`$24D2`) and route correction

Selector `$9B` toggles default-on byte `$005E7ADF`; `SynchronizeApplicationSettingsRegistry`
stores it as `Options` bit 9. The standard FG/BG and sprite bypass handlers use that choice to
select list-based versus alternate edit-field dialogs. Rust's existing persisted dialog-style
model and both real editor consumers are now bound to the correct authenticated
`LM_OPTIONS_OTHER_BYPASS` command.

This recovery also invalidates the earlier `$24C4` attribution. `LM_OPTIONS_INSTALL_VRAM` maps to
selector `$8E` and byte `$005E7AE3`; Ghidra cross-references show that byte participates in level
open/save, sprite serialization, level-mode enablement, and sprite-limit reporting, not bypass
dialog selection. The false route was removed and returned to pending rather than counted as
parity. The authenticated partition consequently remains 311/317: one correctly implemented
command replaces one disproven claim.

The corrected full native gate passes 1,150 tests with 13 explicit fixture ignores. Renderer
235/235, the all-512 traversal, byte-identical 513-line manifest, and i686 cross-build remain
green.

## Historical Install VRAM option (`$24C4`)

The selector-$8E route is the direct checked state for the same per-ROM runtime configured by the
newer `$24E8` dialog. `ValidateAndInitializeOpenedRom` reads `$0060905E`, replaces vanilla `$FF`
with 1, and stores its nonzero predicate in `$005E7AE3`. `SaveLevelToRom` gates the complete
`CheckVramPatchSignatureByte` / install / compatibility / replacement sequence and its Layer-3
support runtimes on that byte. The disabled branch skips installation without attempting to remove
an existing runtime. The byte also selects the original 0x54-versus-0x80 sprite warning limit and
the expanded-stream `$FF` escape path.

Rust now derives an automatic next-save choice from authenticated ROM state: pristine selects
Normal, recognized installed versions retain their selection, and unknown runtimes receive no
automatic mutation. The historical toolbar command toggles Normal versus None, while the full
options dialog opens on any pending compatible choice. A level save composes the default or
explicit choice into the existing revision-bound installation transaction; close/reload clears the
temporary override, and reopen redetects the installed runtime. Command coverage advances to
312/317, leaving five pending ROM options.

## Per-level music bypass (`$2522`)

Port-8089 recovery maps byte-table selector `$AC` to resource `$0400` and dialog procedure
`HandleMusicBypassDialog` at `$00413BE0`. The original decoder and serializer bind custom music to
three-byte object-stream command `$26`: byte three is zero when disabled and otherwise stores the
zero-based track plus one, with `$FE` therefore the largest selectable track. Repeated controls
overwrite in stream order. Serialization emits one canonical `$26` before custom-time command
`$28` while preserving its two opaque coordinate nibbles.

Rust models that boundary with `CustomMusicTrack`, a failure-atomic bank-size preflight, duplicate
collapse, last-record/disabled overwrite behavior, and retained opaque selector. The integrated
Settings panel exposes the enabled state and hexadecimal track, and `LM_LEVEL_BYPASS_MUSIC` routes
there directly. Commit repairs the checksum, semantically reopens command `$26`, and one Undo
restores the exact physical ROM. The preview no longer mistakes `$26` for an auxiliary graphics
file and suppresses only the canonical trailing `$26`/`$28` settings suffix; every earlier ordinary
object remains renderable. A complete pristine 512-level scan proves the discriminator has no
false vanilla matches, while the focused transaction gate proves enabling music changes neither
rendered cells nor render writes. The full native suite passes 1,000 tests with 12 explicit
external-fixture ignores, Windows cross-compilation passes, and the 513-line renderer manifest
retains SHA-256 `254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`.

## Reload ROM lifecycle (`$23BE`)

The authenticated command byte table maps `LM_FILE_RELOAD_ROM` to case `$27`. Its branch at
`$00498553` first runs `CheckCanProceedAfterCoreSavePrompts` (`$00455F50`), which sequences the
modified-level, shared-palette, and unsaved-level prompts. On success it calls
`OpenConfiguredLevelSourcePath` (`$00478C90`) with the current level retained in `EBP`; that path
reopens the configured source through `OpenLevelSourceByPath` and then reloads the same level.

Rust therefore models Reload separately from Open. It reads the existing document path directly,
retains the selected level, and never presents a ROM chooser. Clean reloads are revision-bound.
Dirty reloads require Save, Discard, or Cancel; Save resumes only after persistence succeeds, while
Discard authorizes replacement without first closing the project. In both cases the current
project remains installed until the worker has read, parsed, and identity-qualified the complete
replacement, so I/O failure, malformed input, cancellation, or stale completion cannot destroy the
editing session. Core and native lifecycle tests cover direct-path selection, dirty confirmation,
failure atomicity, exact level restoration, and the authenticated route. Native command coverage is
now 249 of 317 named table slots, leaving 68 pending.

## Pointer-aware Insert command (`$245A`)

The authenticated command inventory binds `LM_EDIT_INSERT` to `$245A`. Rust already implemented
the editor's Insert-key behavior through one domain-sensitive canvas transaction: the selected
Layer 1 object, object-backed Layer 2 object, or sprite template is placed at the pointer and then
normalized by the native stream serializer. The command route now calls that exact path.

Because activating a visible toolbar button moves the OS pointer away from the canvas, the editor
retains the last valid canvas location as native `(major, minor)` coordinates. It deliberately does
not retain a screen pixel: each activation rematerializes the cell center through the latest canvas
geometry, preserving the destination across resize, zoom, scroll, orientation, and fullscreen
changes. Loading or closing a level clears that retained coordinate. A missing coordinate produces
the same explicit Insert diagnostic as the keyboard path. Tests cover Layer 1, object-backed Layer
2, sprites, horizontal coordinate recovery, canonical control-stream changes, missing context,
commit/reopen, controller Undo, and application Undo. Authenticated native command coverage is now
250 of 317 named table slots, leaving 67 pending.

## Conditional and remapped Direct Map16 objects (`$2466/$2467`)

The bundled 3.63 Help topics and the labeled Ghidra program agree on the model boundary.
`ApplyExtendedObject27PropertiesToSelection` stores the conditional-presence bit in byte 2 bit 7,
the `$00..=$7F` RAM-bit index in the optional eighth byte's low seven bits, and Always Show in that
byte's high bit. Removing the check restores the ordinary seven-byte record. The renderer hides a
non-Always-Show object while the condition view is off; Always Show retains the object and selects
its second definition bank (`source + $100`) while the condition view is on.

`RemapDirectMap16ObjectReferences` invokes the remapping worker, reports a no-match outcome without
creating history, and otherwise rebuilds once, captures one undo snapshot, and reports the exact
changed-object count. Rust implements the Help grammar for single tiles/ranges, fixed destinations,
signed offsets, moving destinations, and rectangular ranges. A 32,768-entry pre-state table ensures
that mappings never cascade and that later duplicate sources supersede earlier ones. Grouped
objects match only their upper-left source and retain their complete pattern/output dimensions,
coordinates, screen flags, and optional condition byte. Layer 1 and object-backed Layer 2 publish
atomically through one controller snapshot. Focused tests cover every documented grammar example,
rectangle geometry, malformed/out-of-range atomicity, cross-namespace `$27/$29` conversion, mixed
selection, sprite rejection, both renderer states, one Undo boundary, and ROM save/reopen.
Authenticated native command coverage is now 252 of 317 named table slots, leaving 65 pending.

## Properties and Edit Manual selection editors (`$2468/$2469`)

These adjacent names have different lifecycles in the original. The Properties command belongs to
the outer-window auxiliary-editor state and toggles a checked modeless surface. The level command
lookup maps Edit Manual to the branch that calls `EditObjectAtSelectionOrCell` in object mode or
`EditSpriteAtSelectionOrCell` in sprite mode, so repeated activation opens/reuses an editor instead
of toggling it closed. Rust retains that distinction. The properties window follows the active
Layer 1, object-backed Layer 2, or sprite selection and publishes through the existing semantic
field transactions. Edit Manual exposes the complete native record, validates command-specific
width before publication, requires exactly one selected entity, and uses the same atomic object or
sprite replacement and Undo history as the integrated editor. Independent visibility/reuse tests,
render-without-selection coverage, and authenticated route-partition tests bind the native path.
Authenticated native command coverage is now 254 of 317 named table slots, leaving 63 pending.

The original internal-command table gives `LM_OPTIONS_TRANSLUCENT` and
`LM_VIEW_TRANSLUCENT` the identical `$2415` command ID. Both names now select the same native
half-opacity editor-overlay state and renderer input. This raises authenticated native command
coverage to 255 of 317 named table slots, leaving 62 pending.

## Background Map16 bank and tile-remap commands (`$252E/$252F`)

The bundled 3.63 Help establishes that one background selects exactly one of eight 4-KiB Map16
banks and that a remap may change that bank when a destination crosses a bank boundary. The native
level editor now routes both authenticated commands to separate original-shaped dialogs. Direct
bank changes replace only descriptor bits 4-6. Tile remapping reuses the recovered 32,768-entry
pre-operation translation table and supports the complete documented global-offset, range,
relative, moving, and rectangle grammar as one failure-atomic history operation.

This exposed and fixed a persistence hole: descriptor-only changes previously made the controller
dirty but did not enter Layer 2 save or semantic-reopen verification. The commit path now includes
data or descriptor changes, while pristine layouts still reject an unpersistable cross-bank edit
before mutation. The built-in renderer now loads the complete installed secondary Map16 namespace,
selects the staged bank, rasterizes all 4,096 definitions, and invalidates its composed background
after direct painting, remapping, or bank changes. Focused tests cover descriptor-only save/reopen,
Undo/Redo, cross-bank remapping, high-index composition, flips, and authenticated routing.
Authenticated native command coverage is now 257 of 317 named table slots, leaving 60 pending.

## Main-editor animation-rate option (`$24CB`)

The Lunar Magic 3.63 Help and executable strings distinguish the main-editor animation rate from
the overworld editor's separately persisted rate. The main selector has four exact choices—Low
(7.5 fps), Normal (15 fps), Medium (30 fps), and High (60 fps)—with Normal as the modern default.
Rust now models that independent setting, persists all four values through a bounded versioned
codec, exposes the original checked submenu in Tools, and routes authenticated toolbar command
`LM_OPTIONS_ANIM_RATE` to the same choice set.

The selected 120/60/30/15-millisecond cadence drives both the pristine/vanilla level canvas and
the installed level-assets preview. Wall-clock animation state is quantized before the level
renderer consumes it, preventing unrelated UI repaints from exposing intermediate frames. The
installed preview uses the selected cadence as its ExAnimation scheduler divisor, yielding exact
8/16/32/64 callback counts over 0.96 seconds and retaining independent asset-versus-selection
clock gating. Focused cadence, codec, persistence, routing, and localization tests pass, as does
the Windows cross-build. The full native gate passes 1,012 tests with 12 explicit external-fixture
ignores, and its all-512-level renderer traversal passes. The regenerated 513-line renderer
manifest remains byte-identical at SHA-256
`254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`. Authenticated native
command coverage is now 258 of 317 named table slots, leaving 59 pending.

## Open Level Number (`$238E`)

The bundled 3.63 Help fixes the semantic range at `$000..$1FF` and explicitly states that opening
a numbered slot abandons unsaved editor-local changes. Executable dialog resource `1000` provides
the exact `Open Level Number (in hex)` title, `Level Number (0-1FF)` label, edit control `$7F`,
label `$66`, and standard OK/Cancel controls. Rust now exposes that dialog through both the normal
File menu and authenticated `LM_FILE_OPEN_LEVEL` command. It starts at the active three-digit slot,
accepts bounded unprefixed hexadecimal case-insensitively, retains the dialog on invalid input, and
does nothing without an open ROM. Accepted values dispatch the existing `SelectLevel` transaction,
preserving its navigation history, complete editor teardown/reload, and 512-slot renderer boundary.

Focused parsing, resource-localization, closed-ROM, pristine-ROM, and route-partition tests pass.
The Windows cross-build passes. The full native gate passes 1,016 tests with 12 explicit
external-fixture ignores, including the all-512-level materialization traversal; the regenerated
513-line renderer manifest remains byte-identical at SHA-256
`254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`. Authenticated native
command coverage is now 259 of 317 named table slots, leaving 58 pending.

## Open Level From Address (`$238F`)

Executable dialog resource `1001` authenticates the 170×44 form, exact title, PC-address label,
edit control `$7F`, and standard OK/Cancel controls. The bundled 3.63 Help defines the unusual
lifecycle: parse one Layer 1 stream at an exact PC offset without following the main level table;
do not load sprites, entrances, or background; retain the last ordinary level number; and, on
Save, insert the staged Layer 1 into that ordinary slot without updating the raw source pointer.
The retained evidence and hashes are recorded in
`docs/oracle-work/lm363/open-level-address/PROVENANCE.md`.

Rust now uses a bank-bounded raw Layer 1 decoder that retains the ordinary slot's sprite stream
only as an unchanged semantic-save witness. The native editor binds the temporary controller to
the current revision and level, hides every non-Layer-1 domain, disables their editing panels, and
returns to an ordinary complete level load after the save revision. Empty, prefixed, nonhex,
trailing, out-of-ROM, truncated, and unterminated inputs cannot replace the editor state. A
pristine-ROM integration gate opens the Help-listed `$30263` stream; model tests prove Save/reopen
changes the destination Layer 1 alone while preserving both the destination sprites and raw source
bytes.

Focused parser, resource-localization, routing, pristine-ROM, range, save/reopen, and byte
preservation gates pass. The Windows cross-build passes. The complete native gate passes 1,021
tests with 12 explicit external-fixture ignores, including all-512-level materialization; the
regenerated 513-line renderer manifest remains byte-identical at SHA-256
`254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`. Authenticated native
command coverage is now 260 of 317 named table slots, leaving 57 pending.

## ROM user-area scan (`$2396`)

The native File menu and authenticated toolbar route now share a read-only scan of logical ROM
space above the original 512-KiB SMW image. Valid RATS structures require the complete eight-byte
`STAR` header, in-range payload, and complementary length words. The accounting unions overlapping
protected ranges while retaining every structure for conflict reporting; unprotected nonzero bytes
are used, zero runs of at most eight bytes are unusable, and longer runs are free. Largest raw free
area and the header-adjusted capacity within one 32-KiB bank are tracked independently.

Nested structures append one compatible `RATS.log` record each beside the ROM. Logged addresses
are physical file offsets, so headered ROMs add `$200`; the target must be absent or a regular file.
The report reads the Lunar Magic attribution from the ordinary or ExLoROM active body. Retained
normal and deliberately nested Wine captures live under
`docs/oracle-work/lm363/pristine-us/rom-user-area-scan/`. The core scanner models the fixed
historical unprotected-Map16 interval, but automatic discovery remains pending a genuine pre-1.64
ROM fixture. The full native suite passes 1,025 tests with 12 explicit external-fixture ignores,
the Windows target compiles, all 512 pristine slots materialize and capture, and the semantic
renderer manifest retains SHA-256
`254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`. Authenticated native
command coverage is now 261 of 317 named table slots, leaving 56 pending.

## Open Level From File (`$238D`)

The normal File menu and authenticated toolbar command now share a nonblocking direct level-file
importer. Binary `LM` containers remain on the exact binary decoder even when malformed, preventing
a damaged modern file from being reinterpreted as legacy text. A legacy manifest instead starts a
second bounded read for its required `.mw0`, `.mw1`, and `.mw2` files and an optional `.mw3` only
when declared. The optional-palette absence continues with the destination shared palette and
clears the imported custom-palette state; any missing required sidecar rejects the operation.

Both formats use the file's declared level, the profile-derived complete asset save plan, one
revision-bound mutation, checksum repair, and post-commit navigation. The legacy preparation helper
first loads the destination palette because that value is semantic input when `.mw3` is absent.
Authentic binary and custom-palette legacy fixtures pass across both copier-header forms with
identical logical results. The Windows cross-build and complete 1,028-test native gate pass, all
512 pristine levels materialize, and the 513-line renderer manifest retains SHA-256
`254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`. Authenticated native
command coverage is now 262 of 317 named table slots, leaving 55 pending.

## Old ExGFX bypass-list transfer (`$239B` / `$239C`)

The bundled 3.63 Help identifies these commands as transfer of the old FG/BG/SP assignment table,
not transfer of ExGFX binary files. A retained live Wine extraction is exactly `$400` bytes and its
SHA-256 matches the same complete range at physical `$07F400` in the loaded copier-headered ROM.
Rust therefore exports the exact stored table to the original default `Bypass.lst` name and accepts
only an exact `$400`-byte import.

Insertion preserves all 256 rows, including the dialog-inaccessible `$FF` row. On a pristine ROM it
installs the recovered expanded-settings prerequisite before replacing the table, then repairs the
checksum and verifies semantic reopen. Installation, any required ROM expansion, replacement, and
checksum repair are published as one revision-bound mutation and therefore one application Undo.
Headered and headerless tests require identical logical results and exact physical Undo. Native
file reads and writes remain bounded and nonblocking; both the normal File menu and authenticated
toolbar routes share them. Authenticated native command coverage is now 264 of 317 named table
slots, leaving 53 pending.

## Restrict Level Access authenticated toolbar route (`$23A4`)

The original `LM_FILE_ENCRYPT_LEVELS` user-toolbar command now opens the full native level-access
restriction state machine already shared with the normal File menu. It requires an authenticated
open ROM and does not mutate on activation; confirmation still owns key generation, mapper-specific
bulk migration, checksum repair, persistence, restore archive creation, the optional IPS workflow,
and final close. The retained live central-dispatcher capture corrects the active command to `$23A4`.
The historically named `$23A5` slot is deliberately not aliased without equivalent dispatcher
evidence. Authenticated native command coverage is now 265 of 317 named table slots, leaving 52
pending.

## Multi-level deletion and original-area clearing (`$23C3` / `$23A9`)

The native File menu and authenticated toolbar routes expose the original modified, unmodified,
and all-level categories plus the conditional clear-original-area option. Category discovery reads
all 512 Layer 1 pointers and partitions original versus expanded PC storage exactly. Input levels
are bounded, sorted, and deduplicated before any mutation; every redirect, reclaimed owner,
secondary-exit rewrite, clear-area write, checksum adjustment, and final save is one application
revision and one Undo step.

Deletion now reproduces Lunar Magic's secondary-exit allocation side effect rather than retaining a
known bookkeeping difference. Partial modified deletion forcibly republishes even semantically-zero
variable planes with the installed `$1FE` length into newly reclaimed first-fit holes. Deleting all
unmodified slots or all slots publishes the null-tail form and zero-erases both old owners. The
stored checksum is preserved by zeroing `$07EFC0..$07F09F`, then filling full `$FF` bytes and one
remainder byte in ascending order until the original checksum is restored.

The optional clear operation zeroes the recovered original-level gaps, installs a 32-byte
`Free Area DO NOT ERASE THIS TAG!` marker and seven exact protected RATS owners, preserves every
protected payload, changes the hardcoded test-sprite low word from `$C3EE` to `$E76D`, clears the
expanded secondary-exit tail, writes the `$AA` clear metadata, and is idempotent while its marker is
intact. Complete physical-image hashes match five live Lunar Magic 3.63 command-line oracles,
including an explicit mixed `0,1` list. Authenticated native command coverage is now 267 of 317
named table slots, leaving 50 pending.

## Insert All Graphics (`$23D7`)

This is one original transaction, not shorthand for activating the standard and ExGFX quick
buttons independently. The recovered dispatcher case `$3B` calls `$0047FC30`, whose standard phase
uses `joined | flags | 6`, whose second phase imports ExGFX, and whose finalization executes only
after both return success. The native toolbar route therefore resolves the fixed ROM-sibling
`Graphics` or `Graphics/AllGFX.bin` source and `ExGraphics` directory up front, retains the existing
format warning, and runs both phases through one cancellable worker. The standard mutation is
applied only to an unpublished staging image; ExGFX preparation consumes that image; one final
mutation spans the original to the fully staged ROM.

Focused tests require mismatched revisions to reject, a malformed late ExGFX file to publish no
standard commit, and a pristine combined import to produce one checksum-valid revision that
semantically reopens both runtimes and undoes/redoes byte-exactly. The retained Wine gate executes
Lunar Magic's own `-ImportAllGraphics` from the same files and requires Lunar Magic to re-export all
52 standard files and deterministic `ExGFX80` byte-identically from both results. The authenticated
native command partition is now 268 of 317 named slots, leaving 49 pending.

## Deprecated Decrypt Levels command (`$23A5`)

The authenticated toolbar table retains the historical name, but the 3.63 central dispatcher does
not retain an implementation. Its byte-table entry at `$00498978` is `$DF`; the recovered switch
ends at case `$DE`, so activation takes the successful default return. Rust models that distinction
with a typed no-op instead of aliasing `$23A5` to the active, irreversible restriction workflow at
`$23A4`. The focused native test requires complete history and ROM-byte stability and proves no
dialog or error is produced. Authenticated native command coverage is now 269 of 317 named slots,
leaving 48 pending.

## Integrated emulator option commands (`$23CC/$23CD/$23CF/$23D0`)

The central dispatch bytes at `$0049899F` map the five-command run to cases `$31..$35`; case `$33`
is the already-routed frame advance, leaving four option toggles. Rust now retains all four exact
behaviors in a persisted option model. User-toolbar shortcuts still take precedence over the
original unmodified F4 action. The selected-tile option and visible canvas toggle share one state,
paused internal-emulator frames receive the recovered half-alpha treatment, and level changes
stop or switch the live session according to the recovered option. Focused state, route,
save/reopen, opacity, and transition tests raise authenticated native command coverage to 273 of
317 named slots, leaving 44 pending. The Windows cross-build and complete native gate pass 1,043
tests with 13 explicit fixture ignores. A fresh 512-slot vanilla capture audit produces all images
and its 513-line manifest; normal, vertical, and `$02D` test-level samples match retained original
content. The isolated `c20496c` build reproduces `$02D`'s same viewport, and the retained semantic
manifest remains byte-identical at SHA-256
`254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`.

## Joined standard-GFX option (`$24BD`)

The authenticated command table maps `LM_OPTIONS_ATTACH_FILES` to dispatcher case `$88`. That case
toggles `$00E278C0`; registry synchronization stores it in `Options` bit 4, and all ordinary/quick
standard-GFX extraction and insertion handlers pass it as their joined-file flag. Rust now routes the
original internal command to its existing persisted `joined_graphics_files` state, so the toolbar,
the pristine and installed graphics editors, and every fixed-directory operation select the same
52-file or `AllGFX.bin` behavior. ExGFX remains separate, matching the original help contract.
Authenticated native command coverage is now 288 of 317 named slots, leaving 29 pending.

## Orientation-aware renderer audit offsets

`tools/render-audit.sh` interprets each requested screen as a major-axis offset for both output
styles. Game captures continue to use the preview-camera offset. Editor captures pass a distinct
major-axis tile offset that the level editor resolves after decoding orientation, preventing
horizontal assumptions from corrupting vertical audits. Explicit row/column origins from a retained
Lunar Magic reference manifest override the generic offset. This repairs the audit evidence path;
it does not alter ordinary interactive camera or scroll state.

## Auto-Set Number of Screens (`$24BC`)

The command table maps `LM_OPTIONS_AUTO_SCREENS` to dispatcher case `$87`, which toggles
`$005E76F9`. Unlike the adjacent checksum and joined-file flags, registry synchronization does not
serialize this byte; Rust consequently retains it as a default-on process-session option. Before an
ordinary built-in level commit, the editor clones its staged controller and replaces Last Screen
with the highest visible Layer 1 object or sprite screen through the shared
`LevelScreenExtentMode::Auto` model. The disabled path commits the staged header unchanged. The
temporary clone keeps auto-normalization inside the same checked save mutation without rewriting
the interactive Undo stack. Raw-address Layer 1 saves deliberately bypass the rule because they do
not represent an ordinary level-header workflow. Authenticated native command coverage is now 289
of 317 named slots, leaving 28 pending.

## Allow Fragmentation (`$24BA`)

The command table maps `LM_OPTIONS_ALLOW_FRAGMENT` to dispatcher case `$85`, toggling persisted
default-on byte `$005E76EB` (`Options` bit 1). The name refers to level object-list screen-position
fragments. With the option enabled, Layer 1 and object-backed Layer 2 group drags retain creation
order even when the moved objects cross screen anchors; the native encoder regenerates the required
forward/backward screen jumps. Z-order steps and overlap-aware permutations may likewise cross
screen anchors. With it disabled, moved groups are stably coalesced by screen, z-order steps stop at
a different screen, and complete permutations reject cross-screen inversions while still allowing
within-screen ordering. The setting is a persisted editor preference, defaults on when absent, and
does not alter sprite ordering or ROM allocator behavior. Authenticated native command coverage is
now 290 of 317 named slots, leaving 27 pending.

## Maintain Checksum (`$24BB`)

The command table maps `LM_OPTIONS_MAINTAIN_CHECKSUM` to dispatcher case `$86`, toggling persisted
default-on byte `$005E76FA` (`Options` bit 3). Rust enforces the option at the application mutation
boundary: enabled commits preserve the checksum bytes prepared by domain serializers, while the
disabled path splits any intersecting write around the active ROM identity's four-byte checksum and
complement field. This retains every non-checksum byte and any appended allocation tail, so the
option changes checksum maintenance without weakening transaction validation, semantic preparation,
or undo history. Authenticated native command coverage is now 291 of 317 named slots, leaving 26
pending.

## Silently Add Header to ROM (`$24C6`)

The authenticated internal table maps `LM_OPTIONS_AUTO_HEADER` to dispatch byte `$69`. Lunar
Magic's 3.63 help makes the lifecycle boundary explicit: a missing `$200` copier header is required,
the enabled option adds it silently during open, and the disabled option asks on each open. Rust's
ROM-loader worker now detects headerless supported images, synthesizes the recovered size/map-mode
header, re-reads and compares the selected file before replacement, publishes it atomically, and
installs only the resulting headered prepared project. The confirmation path retains the same
worker and publication boundary; Cancel or a changed source publishes neither a file nor a project.
Authenticated native command coverage is now 292 of 317 named slots, leaving 25 pending.

## Save Prompt (`$24C7`)

The authenticated command table maps `LM_OPTIONS_SAVE_PROMPT` to dispatch byte `$6A`, immediately
after the silent-header option. The Lunar Magic 3.63 help defines a default-on transition guard for
an unsaved current level or overworld when another level or ROM is opened. Rust persists the same
toggle and guards every editor-mode/document transition before dispatch. Save retains the requested
command and releases it only after the staged commit reaches its expected project revision; level
relocation may therefore span expansion plus commit, while built-in overworld terrain and route
links may span two ordered commits. Discard grants a one-use authorization for the exact command,
and Cancel leaves both the project and staged controller untouched. Authenticated native command
coverage is now 293 of 317 named slots, leaving 24 pending.

## Level mouse gestures (`$24C8/$24C9`)

The adjacent authenticated commands map `LM_OPTIONS_MOUSE_GESTURES` and
`LM_OPTIONS_SAVE_GESTURES` to dispatcher cases `$91/$92`. Rust retains their original defaults:
gestures enabled and gesture auto-save disabled. A dominant horizontal right-button stroke moves
back or forward in level history; Ctrl cancels; Shift+Alt selects the previous or next level; and
Alt+right follows the exit under the starting cell. The original has no minimum-distance threshold
and rejects vertical or exactly diagonal movement, which the native classifier preserves. Ordinary
right dragging remains object/sprite duplication while the tool panel is visible; modifier-forced
gestures and gestures with that panel hidden preserve the original conflict boundary.

`Options` registry bits 12 and 30 persist the two choices. When auto-save is enabled, gesture-driven
history, numeric navigation, and exit following enter the same checked staged-level save transaction
as the explicit Save path and release navigation only at the expected final revision. With auto-save
disabled, the ordinary Save Prompt policy remains in control. Authenticated native command coverage
is now 295 of 317 named slots, leaving 22 pending.

## Vertical-fireball buoyancy warning (`$24D9`)

The authenticated command maps `LM_OPTIONS_WARN_SPRITE_33` to dispatcher case `$A2`, which toggles
default-on byte `$005E7AE6`; registry synchronization stores it in `Options` bit 21. Lunar Magic's
`SaveLevelToRom` calls the dedicated check after undefined-exit, sprite-count, and object-placement
checks. Rust inserts the same gate at that point in its checked save pipeline. A warning appears
only when a native placement uses sprite `$33` and both sprite-header buoyancy bits `$80/$40` are
clear. Save Anyway releases the exact prepared commit; Cancel also clears deferred exit-follow,
editor-transition, and expansion intent. Disabling the persisted option bypasses only this warning.
Authenticated native command coverage is now 296 of 317 named slots, leaving 21 pending.

## Object-placement save warning (`$24D4`)

The authenticated command maps `LM_OPTIONS_WARN_OBJECT` to dispatcher case `$9D`, toggling
default-on byte `$005E7AE5`; registry synchronization stores it in `Options` bit 19. Lunar Magic's
`SaveLevelToRom` runs `ReportOutOfBoundsObjectPlacementWarning` after the sprite-count check and
before the vertical-fireball check. Rust now retains the renderer's two clipping flags while
leaving its pixels unchanged: bit 0 records a paint before the first top/left edge and bit 1 records
a paint beyond the last bottom/right edge. Layer 1 and object-backed Layer 2 contribute to one
save-time result. The persisted Tools option gates the original-shaped Save Anyway/Cancel dialog;
acceptance continues to the vertical-fireball check and cancellation clears deferred navigation,
editor transition, and expansion intent. Authenticated native command coverage is now 297 of 317
named slots, leaving 20 pending.

## Same-name IPS save warning (`$24CF`)

The authenticated command maps `LM_OPTIONS_WARN_IPS` to dispatcher case `$98`, toggling default-on
byte `$005E7AE4`; registry synchronization stores it in `Options` bit 17. Immediately before
opening the physical ROM backing stream, Lunar Magic replaces the ROM filename extension with
`.ips`, accepts any existing non-directory sibling, and warns because some emulators apply that
patch automatically. Rust now performs the same check before creating any ordinary ROM-save
request, including Save chosen from a dirty close/open/reload confirmation. Save Anyway releases
the exact deferred save once; Cancel leaves the dirty project in memory and cancels a deferred open
path. The persisted Tools option bypasses only this companion-file warning. Authenticated native
command coverage is now 300 of 317 named slots, leaving 17 pending. `LM_OPTIONS_CONVERT_BERRY`
(`$24D0`) reproduces Lunar Magic 3.63's default-on `Options` bit 10. The native menu, authenticated
toolbar route, and persisted preference share one toggle. Enabled graphics loads and standard GFX
exports and separate/joined standard-GFX insertions synthesize bitplane 3 only for GFX `$01`, `$17`, and `$31`, only for tiles `$00`, `$01`,
`$10`, and `$11`, and skip the complete group when any sampled high-plane bit already exists.
Disabling the option preserves the unconverted 3bpp indices in the live editor cache, exported
files, and insertion input; changing it invalidates the active level preview rather than retaining
stale pixels.

`LM_KEY_GRID_COLOR` (`$26AE`) routes the authenticated internal command to the same active
pristine/profile-backed 8×8 editor state as Ctrl+Alt+F8. It cycles white/black without changing
grid visibility and retains Lunar Magic's exact `Tile grid color 1.` / `Tile grid color 2.` status.

## Add selection to custom collection (`$26AF`)

The two original names `LM_KEY_ADD_CSPRITE` and `LM_KEY_ADD_CUSTOM` occupy the same command ID and
dispatcher case. Lunar Magic chooses the object or sprite path from its active edit-domain byte,
rejects an empty selection with `Nothing selected or couldn't open file.`, prompts for one
description, and appends the cloned selection to the same-stem ROM-adjacent `.mw0/.mw0t` or
`.mw2/.mwt` collection. Rust now follows that context-sensitive route. It derives a bounding-box
origin from complete selected placements, rewrites only native coordinate fields while preserving
object parameters and sprite extension bytes, carries the active revision's sprite-length table,
and emits native placement boundary markers. Empty input becomes `(not specified)` and the prior
text framing is retained while forcing a trailing separator so Lunar Magic's list-population loop
publishes the appended row. Both sidecars are created or replaced as one failure-atomic group;
malformed, incomplete, oversized, or unwritable pairs publish neither half. Exact success statuses,
creation, repeated append/reopen, malformed and incomplete failure, cancel, selection rebasing, and
both toolbar aliases are covered. Authenticated native command coverage is now 302 of 317 named
slots, leaving 15 pending.

## Diagnostic 2bpp viewing mode (`$26B0`)

The authenticated command-table selector `$D4` first displays `Switch 2bpp viewing mode?`; Yes
increments global session byte `$00E27888` modulo three, rebuilds loaded graphics and ExAnimation
caches, propagates Map16 change flags, and reports `2bpp view mode set to %X`. Disassembly of
`DecodeLoadedLevelGraphicsCaches` proves mode 1 reinterprets the first `$4000` raw bytes as `$400`
2bpp tiles, while mode 2 makes six `$80`-tile decodes from successive `$1000` source bands into
successive `$2000` pixel-cache bands. Rust performs those exact low/high-plane splits over the
ordinary 4bpp cache, including retained normal pixels behind destinations not overwritten by the
diagnostic decode. Map16 palette selection divides the encoded row by four and selects the recovered
foreground reduced-color half. The mode is deliberately session-only and changing it invalidates
the same level graphics path; standard GFX extraction while nonzero reports
`GFX saving not available in 2bpp mode.` Synthetic cache/palette tests and complete pristine Level
`$105` renders prove modes 0, 1, and 2 are visually distinct, with normal mode unchanged.
Authenticated native command coverage is now 303 of 317 named slots, leaving 14 pending.

## Layer 3 16x16 diagnostic view (`$26B6`)

The authenticated command-table selector `$D8` increments session byte `$00E27872` modulo three,
posts the Layer 3 refresh command, and reports one of Lunar Magic's exact normal, 512x512, or
1024x1024 status strings. `RenderLayer3TilemapCellAtCoordinates` proves that mode 0 retains the
ordinary four-screen 8x8-cell address mapping, mode 1 addresses one 32x32-word plane, and mode 2
addresses four 32x32-word pages as a 64x64 plane. In either nonzero mode,
`RenderLayer3TilemapRegionToPixelBuffer` interprets one tilemap word as a 16x16 metatile composed
from graphics offsets `+0`, `+1`, `+16`, and `+17`, applying the word's flips across the complete
metatile and retaining its shared palette and priority attributes. Rust now cycles the same
session-only state, invalidates the Layer 3 cache, produces exact 512- or 1024-pixel planes, and
uses the live texture extent for editor repetition and game-viewport wrapping. Normal mode remains
unchanged. Authenticated native command coverage is now 304 of 317 named slots, leaving 13 pending.

## Correct-fatal-errors option (`$24D5`)

The authenticated command maps to selector `$9E`, toggles default-on byte `$005E76EA`, and is
stored as `Options` bit 0. The object renderer reads the byte at `$0042F218`, `$0042F544`,
`$0042FE49`, `$00433FDD`, and `$004350BD`: invalid dispatch states increment the shared fatal
counter and, while enabled, rewrite the mutable layout record to a safe parameter or ordinary
object `$10`. `RebuildAndValidateLevelObjectLayout` and `RebuildAndValidateSpriteLayout` report
the resulting counts with the original `Fatal Error Detected!` strings. The sprite compositor's
corresponding `$004C3A00` branch clears the invalid display-node selector before redispatch; Rust's
typed sprite renderer has no indirect function-pointer state and already defines all 256 IDs.

Rust persists the same default-on preference, exposes it in Options and through
`LM_OPTIONS_CORRECT_FATAL_ERRORS`, and runs an explicit correction pass when a level is decoded.
The pass leaves viewing lossless when disabled, chooses only installed definitions, applies all
replacements through one staged controller/Undo boundary, and reports the exact corrected record
count. A synthetic malformed-handler test proves both standard and extended fallbacks; an
independent all-512 pristine audit proves no vanilla record is changed. Native preference,
toggle, command-partition, complete-suite, renderer, manifest, and i686 gates pass. Authenticated
command coverage is now 313/317 with four pending.

## Background-editor ownership preference (`$26B5`)

Selector `$D7` toggles persisted byte `$00E278C8`, stored as `Options2` bit 22. The only runtime
consumer is `ShowLevelBackgroundEditorWindow`: a false value supplies no owner when the modeless
background editor is first created, while true supplies the main window; reopening an existing
editor only restores, updates, and activates it, which explains both original statuses' “may
require restart” suffix. Rust persists the same default-off choice and exact status text. Its
background workspace is structurally hosted by the main native editor rather than a detached OS
window, so the owned lifecycle (main close, focus restoration, and no orphan window) is inherent;
the preference remains available for original-configuration compatibility. Authenticated native
command coverage is now 305 of 317 named slots, leaving 12 pending.

## Mario-region boundary overlay (`$26B4`)

Selector `$D6` toggles session byte `$005E7B12`, redraws only when the boundary-guide layer is
active, and reports exact on/off statuses. Its sole renderer consumer is
`DrawLevelBoundaryGuideOverlay`. After drawing the mode-dependent 256x232, 352x232, or 448x224
camera boundary, Lunar Magic adds two 16-pixel horizontal bands at Y `$78` and `$90`, plus four
16-pixel vertical bands centered around the view midpoint at offsets -88, -24, +8, and +72 pixels.
The 448x224 layout begins its vertical bands four pixels lower and limits them to 216 pixels.
Rust now draws those six translucent dashed regions through the same camera anchor and responsive
canvas scaling as its recovered boundary guide. The toggle is session-only and does not affect the
game-pixel view. Authenticated native command coverage is now 306 of 317 named slots, leaving 11
pending.

## Complete responsive game viewport

The native level editor no longer uses cover scaling for its 256×224 game viewport. Cover scaling
filled a nonmatching pane by silently cropping both edges of the limiting camera axis, which could
hide entrance-side content after a window resize. The editor now chooses the largest square-pixel
scale that contains the complete SNES camera frame, centers that frame, and paints neighboring
editable level space into the surplus pane axis. The same centered origin drives object/sprite hit
testing, selection overlays, and paused live-frame placement. Wide, tall, zoomed,
horizontal-resize, windowed, and full-screen unit cases pass; a fresh pristine level `$105` visual
capture retains both camera edges. The renderer remains green at 235/235 tests, the complete native
gate passes 1,121 tests with 13 explicit external-fixture ignores, all 512 pristine slots
materialize, and the semantic renderer manifest remains byte-identical at SHA-256
`254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`.

## Deprecated Select FG/BG commands (`$2473/$2474`)

Both authenticated central dispatch bytes are `$DF`, while the Lunar Magic 3.63 command switch
ends at `$DE`. Rust therefore preserves these two historical toolbar names as a successful typed
no-op rather than incorrectly mapping them to the active Layer 1/Layer 2 edit commands. Focused
tests require stable project revision, complete ROM bytes, status, and error state. Authenticated
native command coverage is now 275 of 317 named slots, leaving 42 pending.

## Mode-bounded level truncation (`$26B1`) and dump-data no-op (`$26B2`)

The native toolbar now reproduces Lunar Magic's destructive level-layout cleanup rather than
silently clipping only the rendered image. Its exact confirmation guards a staged operation that
walks resolved native placements and deletes entities whose screen is at or beyond the recovered
mode-specific capacity. Layer 1 objects, object-backed Layer 2 objects, and sprites are changed in
one controller history snapshot for both pristine and installed workspaces. Acceptance refreshes
forms, clears entity selections, invalidates graphics state, and reports per-domain removal counts;
No leaves the staged model unchanged. Focused tests cover the exclusive `$03`-mode screen-13
boundary and prove one Undo restores all three domains.

The authenticated selector table maps the adjacent deprecated dump-data name to `$DF`, while the
original command handler ends at `$DE`. Rust therefore exposes it as a distinct successful no-op,
with tests proving ROM bytes, revision, status, and error state remain unchanged. Authenticated
native command coverage is now 310 of 317 named slots, leaving only seven ROM-patch option commands
pending.

Verification passes 632 active `lm-app` library tests with 13 explicit fixture ignores, 1,147
active native tests with 13 explicit fixture ignores, the 235-test renderer suite, and the i686
Windows cross-build. All 512 pristine levels materialize, and the regenerated 513-line semantic
manifest remains byte-identical at SHA-256
`254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`.

## Prefer allocation above 2 MiB (`$24D3`)

The final generic allocation option is now authenticated rather than inferred from its toolbar
name. Original command case `$9C` toggles global byte `$005E7B23`; registry synchronization maps
it exactly to default-on `Options` bit 18. Both recovered allocation engines consult it only for
ordinary ROMs already beyond physical `$200200`, trying the upper interval first and falling back
to their existing lower-space and growth paths. The Rust frontend persists the same default-on
choice, exposes it through the authenticated toolbar route, propagates it into `AppState`, and
uses it for relocatable FastROM runtime placement. Focused toggle, persistence, route-partition,
and FastROM tests pass. Authenticated native command coverage is now 316 of 317 named slots; only
the ROM-scoped SA-1 RAM-remap option remains pending.

## SA-1 RAM remap (`$24D6`)

The last named original toolbar command is now routed. Ghidra case `$9F` binds it to the ROM's
packed feature bit 17, with loaded-level, ExLoROM, and installed-compatibility guards. The shared
relocation helpers prove that enabled SA-1 insertion adds `$6000` to authenticated IRAM operands
below `$2000` and remaps original `$7E/$7F` WRAM bank operands; the option is therefore persisted
ROM state rather than a frontend-only preference. Rust's metadata model toggles only that bit,
the application transaction is stale-safe, idempotent, checksum-repaired, reopenable, and exactly
undoable, and the native route selects current ROM state before dispatch. Focused tests pass and
the authenticated native command inventory is now 317 of 317 named slots with none pending.

The expanded-settings consumer now honors the persisted option as well as editing it. Its SA-1
plan keeps the two unconditional mapper adaptations separate from the fifteen IRAM operands and
one two-byte runtime operand controlled by bit 17. Both first-time settings and first-ExGFX
prerequisite installation read mapper-qualified metadata. Planner tests compare every conditional
byte, while application tests exercise bit-off and bit-on ROMs through checksum-valid install,
semantic reopen, and byte-exact Undo.

## Direct Layer 3 installation in converted ExLoROM

The complete Layer 3 and expanded-settings planners now have explicit ExLoROM forms. They relocate
every fixed write and allocation search interval into the active SMW body at `+$400000`, accept
the conversion's zero-filled free space as well as `$FF`, retain low-bank fixups so allocated code
is addressed through the high body, and leave the authoritative checksum field in the low first
bank. The application accepts converted SMW-US sources and selects either the six-owner combined
installation or the five-owner Layer 3-only path when settings survived a prior conversion.
Header variants, semantic reopen, checksum, Undo/Redo, and corruption atomicity pass. The renderer
remains 237/237 and the broad application library gate is 639 active passed with 13 explicit
fixture ignores.

## Exhaustive original toolbar route closure

The authenticated table contains 317 named entries followed by one null sentinel. Every named
entry now resolves through exactly one of the application-command, native-action, or local-view
routes; the public inventory intentionally omits the sentinel. The exhaustive partition test was
updated from its stale historical unsupported count to require an empty unsupported set. The full
native suite passes 1,157 active tests with 13 explicit fixture ignores, including all 512 pristine
level render-asset materializations. This closes named-command reachability while leaving the
Configuration/Toolbar row Partial for original customization-dialog interaction and platform
variants.

## Retained level-assets render fixtures promoted to default gates

Two native level-assets tests were still marked ignored even though their exact retained inputs
are present in the repository workspace. `retained_legacy_assignments_materialize_fixed_preview_slots`
now runs by default against `sysLMRestore/smwOrig.smc`, proving the four foreground/background and
four sprite assignments materialize into the expected fixed VRAM slots. The
`retained_level_zero_object_stream_materializes_nonblank_cells` gate now runs by default against
the Lunar Magic-created ExAnimation installation fixture, proving its object stream and sprite
stream produce nonblank preview placements without diagnostics. The complete native suite passes
1,160 active tests with 11 explicit external-fixture/device ignores, including the 512-level
materialization audit.

## Retained ExAnimation runtime template promoted to a default gate

The retained Lunar Magic 3.63 ExAnimation installation is available at
`oracle-work/lm363/pristine-us/exanimation-install-positive/after.smc`, so the exact relocated-core
comparison no longer remains ignored. The gate reads every mapping byte, SNES pointer, IRAM word,
and local-word base from the installed `$C30` core, reconstructs the runtime from the generated
template, and requires byte equality with the complete installed core. The full `lm-profile` run
passes 345 active library tests with 19 external-fixture ignores plus all 10 ROM integration tests.

## Crash recovery includes staged primary level edits

Crash recovery no longer depends solely on mutations already committed to `AppState`. The native
level editor mixes its independent controller revision into the recovery generation and composes
uncommitted Layer 1, Layer 2, and sprite edits into a validated recovery ROM. For a pristine
512 KiB source that needs level-data relocation, composition expands an isolated project clone to
1 MiB, rebases the cloned editor controller, applies the prepared mutation, and snapshots the
result; the live project remains clean and gains no history entry. Preparation failures now reach
the recovery-store error path and remain retryable instead of silently suppressing the revision.

The focused recovery tests cover exact staged-byte restoration, isolated expansion, reopen of the
edited object and sprite streams, clean live state, and worker retry after composition failure.
The complete gates pass 640 active `lm-app` library tests with 13 explicit fixture ignores, 1,162
active native tests with 11 explicit fixture/device ignores (including all 512 pristine levels),
and all 237 renderer tests.

## Crash recovery includes staged installed-ROM palettes

The installed ROM palette controller now contributes its independent revision to recovery and
prepares the same ownership-qualified allocation mutation used by an explicit commit. Recovery
applies that mutation only to an isolated project clone, validates the resulting physical ROM,
and retains the live application's clean baseline and empty history. The application coordinator
detects simultaneous staged level and palette editors and reports that unsupported composition
instead of publishing a plausible but incomplete record. The end-to-end native test changes a
color, recovers and reopens the expanded ExLoROM image, and reads the replacement through the
palette ROM layout while proving the live project was never committed.
The complete native gate passes 1,163 active tests with 11 explicit fixture/device ignores,
including all 512 pristine-level render materializations; the renderer remains 237/237.

## Crash recovery includes staged installed-ROM Map16

The installed Map16 editor now contributes controller revisions to the shared recovery generation
and prepares its normal profile-backed or native-SMW mutation against an isolated project clone.
This covers complete foreground/background definitions and foreground Acts Like data, including
the native 512 KiB to 1 MiB allocation path. Recovery does not expand or dirty the live ROM. The
coordinator now counts staged level, palette, and Map16 workspaces and refuses to publish a partial
record when more than one independently allocating editor is dirty. The focused test changes a
native subtile, reopens it through the complete SMW Map16 loader from the recovery image, and proves
the live project remains clean, unexpanded, and history-free.
The complete native gate passes 1,164 active tests with 11 explicit fixture/device ignores,
including the 512-level materialization audit; all 237 renderer tests pass.

## Crash recovery includes staged aggregate level assets

The installed aggregate level-assets workspace now contributes its independent controller
revision and prepares its normal atomic mutation on an isolated recovery clone. The boundary
covers every domain owned by that controller: the complete level record, optional Layer 2 object
or tilemap storage, per-level palette, ExAnimation and feature state, expanded settings, sprite
spawn/boundary flags, Layer 3 tilemap/graphics settings, and Super GFX bypass assignments. The
recovery record retains the selected level and never advances live history. The focused retained-
fixture test stages a palette change, recovers it, and decodes the complete aggregate while proving
the live application remains clean. Multi-editor collision detection now includes this workspace.
The complete native gate passes 1,165 active tests with 11 explicit fixture/device ignores,
including all 512 pristine levels; all 237 renderer tests pass.

## Left-origin full-level canvas regression repair

The native level editor's scroll-area child now uses an explicit top-left layout. Inheriting the
centered parent layout placed a wide level canvas several screens left of its nominal origin before
the requested scroll offset was applied; later screens consequently showed only the repeating
background even though their Layer 1 objects and sprites had decoded correctly. The canvas extent
is also capped by the declared level span plus genuinely staged placements, so a resizable object
clipped at the 32-screen renderer boundary cannot manufacture trailing blank screens.

A fresh pristine level `$105` editor audit at screen offsets `$0`, `$6`, and `$8` renders the
entrance and the distinct expected mid-level platforms, pipes, blocks, entrances, and sprites.
The complete native gate passes 1,166 tests with 11 explicit fixture ignores, including all 512
pristine-level materializations; the renderer remains green at 237/237.

## Crash recovery includes staged overworld terrain and aggregates

The installed overworld editor now contributes its full staged generation: complete-overworld
controller revision, native custom-sprite revision, and the exact seven animation-option bytes, or
the profile-free playable Layer 2 revision. A full profiled workspace reuses its combined atomic
commit planner, covering terrain, records, paths, events, endpoints, messages, sprites, palette,
ExAnimation options, and native sprite storage. The profile-free path recovers staged playable
main-map Layer 2 terrain and native route links independently or together. Combined recovery first
applies the prepared terrain mutation, then runs the same fixed/current-patch route persistence and
semantic reopen check against that staged project. A retained Lunar Magic overworld-transfer
fixture proves terrain-only, routes-only, and combined recovery/reopen while the live project stays
byte-identical and history-free.
The complete native gate passes 1,166 active tests with 11 explicit fixture/device ignores,
including all 512 pristine-level materializations; all 237 renderer tests pass.

## Crash recovery includes staged installed-ROM graphics

The installed graphics workspace now contributes its controller revision to the shared recovery
generation and prepares the same allocation-checked mutation used by ordinary Save. This covers
ownership-qualified standard GFX and ExGFX slots without committing, expanding, or adding history
to the live project. Recovery retains the selected level when one was active, and the coordinator
rejects a simultaneous independently allocating graphics workspace instead of publishing a
partial snapshot.

The focused test installs a four-tile graphics file, stages one tile replacement, creates and
reopens the recovery record, and decodes the replacement while proving the live application stays
clean and history-free. The complete native gate passes 1,167 tests with 11 explicit fixture
ignores, including all 512 pristine-level materializations; the renderer remains 237/237.

## Crash recovery includes staged installed-ROM ExAnimation

The installed ExAnimation workspace now contributes its active level or global controller revision
to the shared recovery generation and prepares the same allocation-checked atomic mutation used by
ordinary Save. The workspace's target-switch invariant guarantees the prepared controller is the
modified domain; an unexpected modified inactive domain is rejected rather than producing a
partial recovery record. Recovery preserves the selected level and leaves the live project clean
and history-free.

The focused test installs level ExAnimation into an expanded pristine ROM, stages a setting change,
creates and reopens the recovery record, and reloads the exact changed animation from the recovered
ROM. The complete native gate passes 1,168 tests with 11 explicit fixture/device ignores, including
all 512 pristine-level materializations; the renderer remains green at 237/237.

## Crash recovery includes staged title and credits tilemaps

The installed title-screen and credits tilemap workspaces now contribute content-sensitive staged
generations to the shared recovery coordinator. Each recovery path applies the same detected native
storage and allocation policy as its ordinary commit to an isolated project clone, validates the
result through the recovery reopen boundary, and leaves the live project and undo history untouched.
Stale workspaces reject, and simultaneous independently allocating editor work remains an explicit
collision instead of producing a partial record.

Focused tests stage and recover a primary title-screen tile word and a credits tile word, then load
each through its complete detected native loader after reopening. The complete native gate passes
1,170 tests with 11 explicit fixture/device ignores, including all 512 pristine-level
materializations; the renderer remains green at 237/237.

## Crash recovery includes staged installed expanded settings

The standalone installed expanded-settings workspace now contributes a content-sensitive recovery
generation covering its exact 32-byte record. Recovery reuses the controller's ordinary
checksum-inclusive mutation, rejects stale project revisions, preserves the selected level, and
applies the mutation only to the isolated recovery clone. This covers standalone Super GFX bypass,
custom Layer 3 settings, expanded mode flags, sprite-boundary interaction, and all losslessly
retained native words without changing the live project or its undo history.

The focused test stages one native word, recovers the selected level, and reloads the exact record
through the installed settings loader. The complete native gate passes 1,171 tests with 11 explicit
fixture/device ignores, including all 512 pristine-level materializations; the renderer remains
green at 237/237.

## Crash recovery includes staged legacy graphics bypass

Both standalone standard-GFX bypass dialogs now contribute content-sensitive recovery generations
covering the complete 255-row assignment table and both per-level selector domains. Recovery reuses
the workspace's ordinary combined mutation, including Layer 1 allocation/repointing, table storage,
checksum repair, and semantic reopen. Foreground/background and sprite dialogs are counted as
independent staged editors, so simultaneous divergent work rejects instead of silently choosing one.

The focused test stages a foreground/background selector plus all four row assignments, proves the
clean live project retains neither change, and then reloads both exactly from the recovered ROM at
the retained level. The complete native gate passes 1,172 tests with 11 explicit fixture/device
ignores, including all 512 pristine-level materializations; the renderer remains green at 237/237.

## Crash recovery includes staged title-screen recordings

The title-recording editor now contributes a content-sensitive recovery generation for its staged
movement text. Recovery validates the complete bounded hexadecimal payload, rejects stale or invalid
text visibly, and applies the same detected locator, allocation policy, checksum field, and reclaim
fill as the ordinary installed-ROM command to an isolated clone. Temporary recorder installation
and removal already mutate the live project immediately and therefore remain covered by ordinary
project recovery rather than being double-applied as editor-local state.

The focused test starts from a pristine ROM with no playback patch, stages a four-byte movement,
proves the live project remains clean and patch-free, then reopens the exact installed recording
from recovery. The renderer remains green at 237/237, and the 512-level materialization audit
completed without a rendering failure.

## Shared-palette editor uses the detected mapper and recovers staged data

The native shared/custom palette editor no longer assumes the LoROM table layout. Its workspace now
retains the authenticated open-ROM mapper and loads LoROM or ExLoROM storage through the matching
profile layout. Staged recovery hashes the complete encoded file, preserves legacy versus expanded
backend shape, and applies either the ordinary direct save or the exact expanded-runtime installation
plan to an isolated clone before requiring semantic reopen. Stale work rejects, and the live project
and history remain unchanged.

Focused tests recover an edited legacy LoROM color without upgrading its backend and convert a
pristine ROM to ExLoROM, install expanded shared palettes, open through the detected ExLoROM layout,
recover an edited color, and reopen the exact expanded backend. All 10 shared-palette editor tests
and all 237 renderer tests pass.

## Crash recovery includes complete staged overworld messages

The standalone overworld-message editor now contributes a content-sensitive generation over every
byte and the exact variable table length. Recovery validates the complete 194–512-message model and
uses the same two persistence routes as ordinary commit: installing the relocatable runtime from a
pristine selector table, or updating/reallocating detected installed storage. Both routes require
exact semantic reopen on an isolated clone and leave the live project and history untouched.

Focused tests recover a pristine table grown to 200 messages with a changed last tile, then recover
a distinct edit through the already-installed update path. All five overworld-message editor tests
and all 237 renderer tests pass.

## Crash recovery includes complete staged boss-sequence messages

The standalone boss-sequence editor now contributes a content-sensitive generation across all seven
24×8 messages. Recovery uses the same detected locator and update policy as ordinary persistence,
so a pristine ROM installs the native table while an installed ROM updates its existing storage.
Both paths require an exact semantic reopen of the complete table on an isolated clone and leave the
live project clean and history-free.

Focused tests recover a changed final tile through the pristine installation path and preserve a
previously committed message while recovering a distinct staged message through the installed path.
All five boss-sequence editor tests and all 237 renderer tests pass.

## Crash recovery includes the complete staged secondary-exit table

The standalone secondary-exit editor now contributes a content-sensitive generation across all
8,192 entries and every uncompressed public field. Recovery invokes the same shared persistence
operation as ordinary commit: pristine ROMs install the recovered Lfix3 runtime and complete table,
while authenticated installed ROMs update or reallocate their native planes. Both routes require an
exact complete-table reopen on an isolated clone and leave the live project clean and history-free.

Focused tests cover a field-complete edit at the `$1FFF` namespace boundary, installed recovery that
preserves an independently committed earlier entry, and invalid staged fields that report recovery
failure without panicking or mutating live state. All eight secondary-exit editor tests and both
application persistence tests pass; the renderer remains green at 237/237.

## Crash recovery includes complete staged overworld level names

The standalone overworld level-name editor now contributes a content-sensitive generation over the
exact record count, canonical level identity, all nineteen tile bytes, and retained raw flags.
Recovery invokes the same shared persistence operation as ordinary commit: vanilla storage installs
the expanded runtime, while installed storage updates or reallocates its authenticated RATS table.
Every changed route requires an exact semantic reopen on an isolated clone and leaves the live
project clean and history-free.

Focused tests grow both pristine and installed tables through level `$1DB`, tile `$12`, producing
the maximum 256 canonical records. The installed path also proves a previously committed level-000
tile survives the reallocation. All five level-name editor tests, the shared application persistence
test, and all 237 renderer tests pass.

## Crash recovery includes both staged overworld player starts

The standalone player-start editor now contributes a content-sensitive generation covering Mario
and Luigi's player identities, coordinates, submaps, raw flags, and all four adjacent reserved
runtime-option bytes. Recovery invokes the same identity-checked fixed-block save and exact semantic
reopen as ordinary commit on an isolated clone, while leaving the live project clean and
history-free.

The focused test stages distinct Mario and Luigi positions on different submaps, recovers and
reopens both records exactly, and proves the unowned reserved bytes remain unchanged. All three
player-start editor tests, the shared application persistence test, and all 237 renderer tests pass.

## Crash recovery includes all staged overworld global settings

The standalone overworld-settings editor now contributes a content-sensitive generation over all
seven 32-byte records, including semantic Layer 3 fields and every retained opaque word. Recovery
uses the same shared persistence route as ordinary commit: pristine ROMs install the recovered
expanded-settings runtime with bounded expansion retry, while installed ROMs update their detected
table. Both paths require exact seven-record reopen on an isolated clone and leave the live project
clean and history-free.

Focused tests recover independent edits in the first and last records through pristine installation,
then preserve a previously committed submap-2 edit while recovering a staged submap-6 edit through
installed storage. All six overworld-settings editor tests, four application persistence and variant
tests, and all 237 renderer tests pass.

## Crash recovery includes all staged overworld special events

The standalone special-event editor now contributes a content-sensitive generation over all 24
source tiles, destination tiles, and direction bytes. Recovery invokes the same shared detected
persistence as ordinary commit, choosing pristine two-runtime installation or authenticated
installed-table update/reallocation and requiring exact reopen on an isolated clone. The live
project remains clean and history-free.

Focused tests recover independent edits at entries 0 and 23 across all three planes through pristine
installation, then preserve a committed entry 0 while recovering entry 23 through installed storage.
All four special-event editor tests, the application install/update/two-Undo test, and all 237
renderer tests pass.

## Crash recovery includes staged overworld path links

The standalone path-link editor now contributes a content-sensitive generation over the table
length and every source endpoint, destination endpoint, submap, and engine target coordinate.
Recovery uses the existing isolated-project composition path shared with overworld terrain: the
14-link vanilla fixed table may be edited in place, resized tables install the relocatable runtime,
and installed tables update or reallocate under the ordinary bounded policy. Exact semantic reopen
is required while the live project stays clean and history-free.

Focused tests recover a pristine 14-to-15-link growth with distinct endpoint and target fields, then
preserve that installed tail link while recovering an independent edit to link zero. All four
path-link editor tests, both application persistence tests, and all 237 renderer tests pass.

## Crash recovery includes staged overworld warp links

The standalone warp-link editor now contributes a content-sensitive generation over the table
length and all four opaque coordinate words in every record. Recovery clones the project through a
shared application helper: the 27-link vanilla fixed table may update in place, resized tables
install the current relocatable runtime, installed tables update or reallocate, and legacy patches
migrate through the same authenticated runtime path used by ordinary commit. Every changed route
requires exact semantic reopen while the live project remains clean and history-free.

Focused tests recover a pristine 27-to-28-link growth with all four boundary-record words changed,
then preserve the installed tail link while recovering an independent edit to link zero. All four
warp-link editor tests, all four application storage/migration tests, and all 237 renderer tests
pass.

## Crash recovery composes simultaneous path and warp link editors

The native recovery coordinator no longer rejects the paired staged path-link and warp-link
editors. It validates both workspace revisions, then installs their complete tables sequentially
into one isolated project so two pristine relocatable runtimes allocate against the same evolving
ROM image rather than colliding through a bytewise merge. Other simultaneous staged-domain
combinations continue to fail visibly until they receive an equivalent typed composition route.

`simultaneous_pristine_path_and_warp_growth_allocate_and_recover_together` grows both vanilla
tables, reopens both exact installed tables from one recovery record, retains level `$105`, and
proves the live project remains clean and history-free. Both navigation editor families and all 237
renderer tests remain green.

## Crash recovery composes the complete staged overworld event family

The recovery coordinator now recognizes simultaneous event-number, ordinary reveal, special-event,
and event-tilemap workspaces and applies all four typed persistence routes sequentially to one
isolated project. This is allocation-aware: pristine full-map installation, reveal-table growth,
special-event runtime installation, and both compressed tilemap streams all see the preceding
staged allocation state. Workspace revisions are validated before composition; other unsupported
simultaneous combinations still fail visibly.

`simultaneous_pristine_event_family_installs_and_recovers_every_domain` installs all four domains
from one pristine ROM, reopens the complete 256-byte number map, 113-record reveal table, all 24
special records, and both 2,048-tile buffers exactly, retains level `$105`, and proves the live
project is clean and history-free. All twelve focused event-editor tests and all 237 renderer tests
remain green.

## Crash recovery includes the complete staged overworld event-number map

The standalone event-number editor now contributes a content-sensitive generation over the exact
native stored length and every stored mapping byte. Recovery invokes the same detected persistence
as ordinary commit, preserving the vanilla `$60`-byte representation until high events require the
installed 256-byte map, then requiring exact reopen on an isolated clone. The live project remains
clean and history-free.

Focused tests recover mappings at `$00` and `$FF` while growing pristine storage to all 256 bytes,
then preserve a committed low mapping while recovering a staged `$FF` update through installed
storage. All four event-number editor tests, the application semantic/Undo test, and all 237 renderer
tests pass.

## Crash recovery includes staged overworld event-reveal tables

The standalone event-reveal editor now contributes a content-sensitive generation over the record
count and every mixed-endian source/destination pair. Recovery clones the current project and uses
the same detected-storage persistence as ordinary commit: pristine fixed storage can grow into an
installed table, while existing transferred-source or expanded storage updates without losing
prior records. Exact semantic reopen is required and the live project stays clean and history-free.

Focused tests recover a pristine 112-to-200-record growth with a staged tail record, then preserve
that installed tail while recovering an independent edit to record zero. All four event-reveal
editor tests pass.

## Crash recovery includes all staged overworld event-tilemap planes

The standalone event-tilemap editor now contributes a content-sensitive generation over every byte
in the 4,096-byte primary low/high stream and 2,048-byte secondary high plane. Recovery clones the
current project and uses the ordinary detected persistence path: pristine zero workspaces install
the native LZ2 runtime, while installed LZ2/LZ3 streams update or reallocate under the same bounded
policy. Both paths require an exact semantic reopen and leave the live project clean and
history-free.

Focused tests recover edits at both tile boundaries across all three planes through pristine
installation, then preserve a previously installed primary-plane edit while recovering a staged
secondary-tail update. All four event-tilemap editor tests, the application install/update/Undo
test, and all 237 renderer tests pass.

## Crash recovery includes staged Lunar Magic fixed metadata

The standalone metadata editor now contributes a content-sensitive generation over all 160
attribution bytes, the VRAM-version byte, and all 25 feature-record bytes. Recovery clones the
current project and invokes the same authenticated SMW-US fixed-layout save as ordinary commit,
including checksum repair, while the metadata model continues to reject edits to the stable
signature and reserved checksum-status bits. Stale workspaces fail closed and the live project
remains clean and history-free.

The retained real Lunar Magic 3.63 fixture test stages independent attribution, VRAM-version, and
feature-record edits, recovers and reloads all three exactly, and proves the live project was not
mutated. All four metadata editor/workspace tests, the application install/Undo test, and all 237
renderer tests pass.

## Crash recovery composes staged overworld configuration editors

The recovery coordinator now recognizes simultaneous level-name, player-start, and seven-map
overworld-settings workspaces. It validates each workspace revision and runs their ordinary typed
persistence routes sequentially on one isolated project, allowing both pristine relocatable
installers to allocate against the same evolving ROM while the fixed player-start block is retained.
Other unsupported simultaneous combinations continue to fail visibly.

`simultaneous_pristine_overworld_configuration_installs_and_recovers_every_domain` changes all
three domains on a pristine ROM, reopens them exactly from one recovery record, retains level
`$105`, and proves the live project remains clean and history-free. The focused semantic test,
native application compile gate, and all 237 renderer tests pass.

## Crash recovery composes both staged overworld message editors

Ordinary overworld-message and boss-sequence persistence now share exported, identity-checked,
semantic-reopen application functions with their normal command routes. The recovery coordinator
uses those same functions sequentially when both editors are dirty, so pristine message-runtime
installation and boss-table persistence share one isolated evolving ROM instead of rejecting the
snapshot. Workspace revisions and message encodings are validated before composition.

`simultaneous_pristine_message_family_installs_and_recovers_both_tables` edits both tables on a
pristine ROM, reopens them exactly from one recovery record, retains level `$105`, and proves the
live project remains clean and history-free. Both ordinary command/Undo tests, native compilation,
and all 237 renderer tests pass.

## Crash recovery composes staged title and credits tilemaps

Title-screen and credits tilemap persistence now share exported, identity-checked application
functions with ordinary commands and require exact semantic reopen after every changed write. The
recovery coordinator validates both workspace revisions and applies the two complete tilemaps
sequentially to one isolated ROM, allowing their pristine expanded-storage installers to allocate
without collision.

`simultaneous_pristine_global_tilemaps_install_and_recover_exactly` changes both title planes and
the credits map, installs both runtimes from a pristine ROM, reopens all words exactly, retains
level `$105`, and leaves the live project clean and history-free. Both ordinary Undo tests, native
compilation, and all 237 renderer tests pass.

## Crash recovery composes installed and shared palette editors

Shared-palette persistence now has one exported production function used by ordinary commits,
standalone recovery, and simultaneous recovery. The installed palette editor exposes its exact
revision-bound prepared ROM mutation, while the shared editor exposes its validated complete
palette file. The coordinator applies the mutation first and then performs legacy update or
expanded-runtime installation on the same isolated ROM.

`simultaneous_pristine_palette_family_recovers_mutation_and_expanded_shared_palette` proves an
independent staged mutation survives first-time expanded shared-palette installation, exact reopen,
level `$105` retention, and a clean history-free live project. The ordinary shared-palette Undo
test, native compilation, and all 237 renderer tests pass.

## Crash recovery composes installed graphics and ExAnimation mutations

The graphics and ExAnimation controllers now expose semantic save-to-project operations in addition
to their exact revision-bound prepared mutations. When both installed editors are dirty, the native
recovery coordinator clones the live project, derives the graphics allocation policy against that
clone, persists graphics, then derives the ExAnimation policy against the resulting evolved ROM and
persists the active level/global animation. Each allocator therefore sees prior growth and tagged
ownership instead of attempting to merge two independently allocated byte mutations. The final
snapshot is checksum-valid, retains level context, and never commits either edit to the live project.

`semantic_graphics_exanimation_and_settings_share_one_growing_staging_project` forces the first
save past the source ROM end, proves the second allocation is disjoint on that expanded image,
applies an installed expanded-settings edit after both allocations, reopens all three exact
semantic edits, validates the checksum, and proves the live project stayed byte-identical.
The two `staged_rom_*_edit_is_recovered_without_committing_live_project` tests also exercise each
native adapter's semantic staging route directly. The lower-level raw-mutation composition helper
continues to validate overlap and repair the final checksum for same-size callers; it deliberately
rejects raw growth that lacks semantic allocation information. Native compilation, all 237 renderer
tests, and a fresh 488-image pristine corpus hash comparison pass.

Installed palette and Map16 recovery use the same semantic composition rule. Each controller now
has a save-to-project boundary; the native palette adapter derives its allocation policy against the
recovery clone, and the Map16 adapter subsequently derives both definition and Acts-Like policies
against that evolved image. The coordinator therefore preserves growth and allocation ownership
without merging independently prepared mutations. `palette_saves_semantically_on_a_growing_`
`recovery_project` and `map16_saves_semantically_after_independent_recovery_growth` prove exact
reopen, checksum repair, growth retention, and live-project isolation. Native compilation and all
237 renderer tests pass. `palette_and_map16_share_one_growing_recovery_project` additionally stages
both controllers on the same clone, forces allocation beyond the original image, reopens both exact
semantic results, validates the final checksum, and proves the live application bytes never change.

Secondary exits and fixed Lunar Magic metadata form another non-overlapping recovery family. Their
native adapters now expose semantic save-to-project operations, and the coordinator installs or
updates the complete exit table before applying attribution, VRAM-version, and feature-record bytes
on the evolved clone. `secondary_exits_and_metadata_share_one_recovery_project` exercises a retained
Lunar Magic 3.63 ROM, the final exit-table entry, all three metadata regions, exact semantic reopen,
checksum validity, and unchanged live bytes/history. Native compilation and all 237 renderer tests
pass.

## Complete overworld palette and ExAnimation variant persistence

`interactive_overworld_edits_match_every_supported_identity_and_layout_variant` now exercises the
complete semantic palette/ExAnimation editing surface across the accepted physical ROM product:
SMW North America, SMW Japan, and All-Stars + World North America; LoROM `$20`, Fast LoROM `$30`,
SA-1 `$23`, and ExLoROM `$32`; two independently placed pointer-table families; and copier-header
absence/presence. The animation transaction changes the setting, header value, absent and present
trigger endpoints, record replacement/insertion/reordering/removal, and frame
insertion/replacement/reordering/removal. Palette edits cover both ends of the owned range.

Each of the 48 physical cases reopens the complete aggregate through both the project decoder and a
fresh application controller, retains a valid checksum, preserves an exact 512-byte copier prefix,
restores and reapplies exact physical bytes through one Undo/Redo boundary, and produces identical
logical output across header forms. This closes the supported mapper/identity persistence evidence
for these two domains; the parity-matrix row remains Partial pending broader original-editor
behavioral evidence.

## Original language-template application in native dialogs

The locally supplied, SHA-256-authenticated Lunar Magic 3.63 executable now passes the portable
decoder across all 107 mapped original dialog resources in one opt-in gate. The native About
family consumes decoded titles and dismissal captions from original resources `$03F8`, `$0429`,
and `$042A`, while retaining complete typed-catalog fallbacks when a language module omits any
template. The ordinary standard-GFX and ExGFX insertion workflows now consume all equivalent
literal captions from resources `$03EC` and `$03FE`: title, expansion option, 4bpp runtime option,
reciprocal format note, physical PC-address label, OK, and Cancel.

The graphics editor passes the active application catalog into the modal instead of rendering a
hard-coded English island. Synthetic Unicode catalogs prove both resource families independently,
canonical catalog encode/decode preserves every dialog-text key, and absent-template tests retain
the family-specific English behavior. This advances template application but does not close the
Localization row: remaining mapped native forms and a retained live third-party language-DLL Wine
gesture are still required.

## Original animation-rate and custom-sprite collection templates

The native animation-rate form now consumes Lunar Magic 3.63 dialog resource `$0410` for its
title, computational-cost explanation, four exact rate captions, and OK/Cancel controls. The
native rate model remains the already authenticated 120/60/30/15-millisecond cadence; localization
changes presentation only. Missing template entries independently fall back to the built-in English
caption instead of making the form incomplete.

The Add to Custom Collection workflow now applies resource `$040A` to the matching sprite form:
title, original description instruction, OK, and Cancel. The Rust-only grouped-object extension
keeps its distinct object wording and deliberately does not misapply the sprite resource. Synthetic
Unicode catalogs and canonical encode/decode tests bind both forms through the same catalog used by
the running application. Localization remains `Partial` pending the remaining mapped native forms
and a retained live third-party language-DLL Wine gesture.

## Original level-access restriction template

The native restriction configuration now consumes Lunar Magic 3.63 dialog resource `$03FF` for
its title, both original permanence/weak-protection notices, 21-character ASCII ROM-title label,
and OK/Cancel controls. The additional explicit acknowledgement remains a Rust safety guard, while
the original captions flow from the active application catalog into the real restriction workflow.
Each missing template field falls back independently, and a Unicode catalog encode/decode test
proves the resource survives persisted configuration before application. The irreversible mutation,
restore-point policy, IPS offer, persistence, and close sequencing are unchanged. Localization
remains `Partial` pending the remaining mapped native forms and retained live language-DLL evidence.

## Original level-resource analysis template

The native LevelAnalysis workflow now consumes Lunar Magic 3.63 dialog resource `$0425` for its
title, Map16 analysis switch, defined-but-unused filter, graphics analysis switch,
inserted-but-unloaded filter, sprite and music switches, and OK/Cancel controls. Dynamic output
paths, progress, completion statistics, and diagnostics remain runtime data rather than translated
resource literals. The running application passes its active catalog into the actual background
analysis workflow, and a Unicode catalog encode/decode test proves every mapped key remains
available after persistence. Localization remains `Partial` pending the remaining native forms and
retained live third-party language-DLL/Wine evidence.

## Original ROM user-area scan-results template

The native ROM User Area Scan Results form now consumes Lunar Magic 3.63 resource `$0427` for its
title, OK caption, six protected/used/free-space labels, two conflict labels, three RATS/free-area
labels, and last-used-version label. The original stores those metrics in three multiline controls;
Rust validates each translated group has the exact required line count before assigning its lines
to individual grid rows. A malformed group falls back atomically, preventing a missing translation
line from shifting labels onto unrelated calculated values. Unicode catalog persistence and grouped
fallback tests cover that boundary; addresses, counts, detected version, and conflict-log results
remain untranslated runtime data. Localization remains `Partial` pending the remaining forms and
retained live third-party language-DLL/Wine evidence.

## Original multiple-level deletion template

The native multiple-level deletion workflow now consumes Lunar Magic 3.63 resource `$042C` for
its title, level-selection group label, modified/unmodified/all radio captions, clear-original-data
option, TEST-level replacement warning, and OK/Cancel controls. Dynamic category counts remain
runtime values appended to the translated captions, while the separate Rust clear-only entry point
retains its purpose-specific typed-catalog wording rather than misapplying the original deletion
form. Canonical Unicode catalog encode/decode and fallback tests cover the template boundary; exact
sorted selection, clear-area eligibility, revision binding, Undo, and deletion persistence are
unchanged. Localization remains `Partial` pending remaining mapped forms and retained live
third-party language-DLL/Wine evidence.

## Original restore-selection template

The loaded restore-archive selection form now consumes Lunar Magic 3.63 resource `$0411` for its
title, incremental/full/reference legend, associated-file restoration option, and OK/Cancel
controls. Archive/original/target paths, record IDs, timestamps, types, descriptions, atomic-target
warning, and completion/error details remain runtime data. Resources `$0412` and `$0413` are not
misapplied to the Rust automatic append-policy form because those original layouts represent
different workflows. A Unicode catalog encode/decode test covers the `$0411` binding and independent
fallback behavior; archive validation, associated-file restoration, failed-reversion recording,
and atomic publication are unchanged. Localization remains `Partial` pending remaining mapped forms
and retained live third-party language-DLL/Wine evidence.

## Original graphics color-map filter template

The standard, pristine-ROM, and installed-ROM graphics editors now pass the active application
catalog into their shared transactional color-map form. That form consumes Lunar Magic 3.63
resource `$0401` for its title, color-map selector label, original/mapped color headings, dynamic
color caption, Reset, OK, and Cancel. The portable document editor retains built-in English because
it has no application localization context. A Unicode catalog encode/decode test proves the mapped
controls and independent fallbacks; the draft remains isolated until OK, Cancel still discards it,
and applying a filter continues to mutate only the selected tile edit buffer before an eligible
paste publishes backing graphics. Localization remains `Partial` pending remaining mapped forms and
retained live third-party language-DLL/Wine evidence.

## Original tile-editor setup identity and template

Imported Lunar Magic tile-editor profiles and newly created native profiles now retain a stable
tile-editor identity instead of falling through to the SNES-emulator form. The configuration editor
uses resource `$0409` for the matching title, executable-path and argument labels, OK, and Cancel;
new profiles default to the private `{graphics}` placeholder used by the staged graphics round trip.
Identity survives the tool-config encode/decode boundary, and Unicode template tests cover resource
selection and fallback. The recovered palette-file replacement option is now a typed capability:
registry `Options2` bit 25 migrates into the tool, the `$0409` checkbox edits it, and canonical
`LMTOOLS2` persistence retains it while every prior `LMTOOLS1` file upgrades with the option off.
The original `$67` “Replace yychr.pal” and `$6B` “Set transparent colors to blue” controls are the
two mutually exclusive presentations of that same bit: the dialog click handler clears the peer,
acceptance stores `$67`, and setup derives `$6B` as its inverse. The native form therefore exposes
the exact two-choice control. Before launch, replacement mode creates an exact 768-byte RGB24
`yychr.pal` beside the private staged GFX file from the current 256-color SNES palette; blue mode
does not create that file and lets the tile editor use its blue transparency presentation.
Cancellation or completion removes the complete private workspace.

The built-in level editor's four pre-save validation prompts now reuse the authenticated General
Options resource `$041F` labels that name their enabling choices: controls `$22A9` (exit scan),
`$22AA` (sprite count), `$22AB` (object placement), and `$22AD` (vertical-fireball buoyancy). Those
localized strings drive both each warning title and its disable-option guidance. Dynamic safety
details remain native explanatory text rather than being falsely attributed to an original dialog;
the executable proves these phrases are option controls, not standalone warning resources.
The same four catalog entries now label their live Tools-menu checkboxes, so selection, persistence,
and the resulting warning all present one translated option identity.
Five additional live Tools controls now consume their byte-authenticated `$041F` entries:
Remember Window Size `$2294`, Show ID in Add Object/Sprite Editors `$2296`, Auto-Deselect on Editor
Select `$2298`, Convert Berry GFX Tile `$22A5`, and Check if ROMFileName.ips Exists `$22AC`.
The complete resource extraction also identifies `$22A1` as the shorter original “Correct Fatal
Errors” label; the native option now uses that authenticated text instead of its longer Rust-only
wording. No unrelated control is reused as localization evidence.
Five more authenticated and persisted General Options behaviors are now present on the ordinary
Tools options surface instead of being reachable only from a customized toolbar: Mouse Gestures
`$2292`, Auto-Save on Mouse Gestures `$2293`, Maintain ROM Checksum `$22A2`, Silently Add Header
to ROM `$22A7`, and Save Prompt `$22A8`. Each uses its original `$041F` catalog control label and
updates the same live application/editor state as the corresponding toolbar command. The dependent
auto-save control is unavailable while mouse gestures are disabled, matching the option hierarchy.
The same surface now exposes the remaining three directly implemented ROM-editing choices recovered
from that dialog: Standard GFX Bypass Dialogs `$2297`, Use Joined GFX Files `$22A4`, and Prefer
Saving in 2MB+ ROM Area `$22A6`. They update the canonical persisted state consumed respectively by
the legacy bypass editors, every standard-GFX batch route, and the ROM allocator.
Original toolbar command `LM_OPTIONS_GENERAL` `$24D7` no longer opens a misleading one-field
undo dialog. The resource-`$041F` Apply/Cancel workspace now stages the undo limit plus all eight
currently authenticated Program/ROM choices together. Cancel remains mutation-free; Apply updates
the canonical editor and application consumers in one publication point. Both the Tools entry and
the authenticated toolbar command seed that form from the same live preference snapshot.
That staged snapshot now also includes all ten already-implemented warning/editor controls from the
same resource: Remember Window Size, Show IDs, Auto-Deselect, Correct Fatal Errors, berry-GFX
conversion, exit scan, sprite count, object-placement checking, sibling-IPS detection, and vertical
fireball buoyancy checking. Apply routes each value through its canonical setter so editor workers,
graphics workers, pending warnings, and persisted state change together; Cancel publishes none.
The recovered `$2299` Allow Control + Mouse Wheel to Zoom choice is now separately persisted,
staged in that dialog, and consumed at the installed level-assets canvas's pointer-anchored wheel
route. Disabling it suppresses only modified-wheel zoom; explicit zoom controls remain available.
The `$229F` ROM File Name in Main Window Title Bar choice is likewise staged and persisted. When
enabled, the localized application title appends only the current ROM's final path component; when
disabled or no ROM is open, the base localized title remains unchanged.
The `$229E` Pause Animation When Inactive preference contributes an independent focus reason to
the existing continuity-preserving animation clock. Losing focus freezes the exact phase and
regaining it resumes without a wall-clock jump; manual pause retains precedence.

Crash-recovery composition update (2026-08-11, legacy standard-GFX bypass): the independently
open foreground/background and sprite bypass dialogs no longer compete as last writers for their
shared 256-row physical table and command-`$24` selector record. Recovery performs a three-way
merge against the exact revision-bound table and selectors: disjoint selector and row edits are
preserved in one mutation, identical overlap is accepted, and divergent edits to the same semantic
field reject visibly. `independent_dialog_edits_three_way_merge_without_losing_shared_table_rows`,
`competing_shared_row_edits_reject_instead_of_last_writer_winning`, and
`simultaneous_dialog_recovery_reopens_both_selectors_and_rows_without_live_mutation` prove combined
reopen, checksum validity, and unchanged live ROM/history. The native build passes and the renderer
gate remains green at 237/237.

Crash-recovery composition update (2026-08-11, title family): staged title-screen recording,
title-screen tilemap, and credits tilemap edits now persist in their real semantic order on one
isolated project clone. Every two- or three-domain combination is accepted only when it is the
complete staged-editor set; each adapter revalidates its source revision before writing. The
`recording_title_and_credits_tilemaps_share_one_recovery_project` gate installs a fresh recording,
changes both tilemaps, reopens all three exact values with a valid checksum, and proves the live ROM
and Undo history were not mutated. Both affected editor suites and the native build pass; the
renderer remains green at 237/237.

Crash-recovery composition update (2026-08-11, fixed-ROM family): installed palette, Map16,
secondary exits, and Lunar Magic metadata are no longer limited to their two original paired
branches. The coordinator recognizes any complete two-, three-, or four-editor subset and applies
the active semantic adapters in deterministic palette/Map16/exits/metadata order on one evolving
clone. Each adapter retains its revision, ownership, allocation, semantic-reopen, and checksum
guards. The existing `palette_and_map16_share_one_growing_recovery_project` and
`secondary_exits_and_metadata_share_one_recovery_project` stress the allocating and fixed-storage
halves respectively; the native build proves the unified coordinator route.

Overworld recovery-family audit (2026-08-11): navigation, event, configuration, and message
editors now enter one typed optional-domain dispatcher, so any subset inside one family composes on
the same evolving clone instead of requiring every sibling dialog to be dirty. Cross-family
composition deliberately rejects before touching the clone: a pristine path-plus-event-number
probe exposed a real guarded-hook collision at logical `$01BD90`, proving that simple sequential
installation is not sound. `cross_family_overworld_recovery_rejects_before_shared_hook_mutation`
locks this failure-atomic boundary; all six existing pristine family composition gates still pass.
A future cross-family route requires an authenticated combined shared-hook runtime, not write
reordering. Native compilation and the renderer gate remain green at 237/237.

Playable-overworld recovery update (2026-08-11): staged main-map Layer 2 terrain and its embedded
path edits can now compose with an independently open warp-link editor. The aggregate editor
exposes its revision-checked terrain mutation and optional path table; recovery applies terrain,
then path installation/update, then warp installation/update on one evolving clone. The
`terrain_mutation_path_and_warp_tables_recover_on_one_evolving_project` gate proves all three
domains reopen together while the live ROM and history remain unchanged. Native compilation and
all 237 renderer tests pass.

Primary/aggregate level recovery update (2026-08-11): the coordinator can now combine a staged
primary level edit with aggregate level-assets work when both target the same level, the aggregate
controller did not change Layer 1/Layer 2/sprites, both mutations share one physical baseline, do
not grow the ROM, and have no conflicting writes outside the recomputed checksum. The aggregate
controller exposes an explicit `level_streams_modified` ownership predicate; overlap and allocation
growth reject visibly instead of restoring stale level streams or merging independently planned
allocations. `level_and_asset_mutations_compose_disjoint_writes_and_reject_growth` covers successful
composition and the growth boundary. Native compilation and the 237-test renderer gate pass.

Crash-recovery composition update (2026-08-11, shared-palette fixed-ROM integration): the
shared/custom palette editor is now a first-class member of the fixed-ROM recovery family instead
of a special case limited to installed palette plus shared palette. Any complete subset of
installed palette, shared palette, Map16, secondary exits, and Lunar Magic metadata is applied in
deterministic semantic order on one evolving clone, so allocator and checksum changes made by an
earlier domain are visible to every later domain. The
`shared_palette_stages_after_fixed_metadata_and_reopens_both_without_live_mutation` gate proves
exact metadata and palette reopen, valid checksum, selected-level retention, and unchanged live
ROM/history. The Release row remains Partial for its separately listed distribution and remaining
recovery gaps; aggregate parity remains 60/65.

Release-publication update (2026-08-11): portable release builds no longer stamp every tag with
the unrelated hard-coded `0.1.0` version. Manual runs use an explicit development version, while
`v*` pushes derive the bundle/manifest version from the immutable tag. A least-privilege publish
job waits for all four platform builds, merges their artifacts, validates every neighboring SHA-256
file with strict parsing, records GitHub artifact provenance, and creates the tag-matched GitHub
Release with all archives/checksums and generated notes. The three deterministic packager tests
pass. A real tagged workflow run, installers, platform signing/notarization, and updates remain
required before the Release row can pass; aggregate parity remains 60/65.

Update-verification foundation (2026-08-11): the new safe-Rust `lm-update` trust core decodes a
bounded canonical `LMUPDATE1` manifest and binds an offered version, target triple, portable archive
name, exact length, and SHA-256 digest. It rejects malformed/oversized/ambiguous manifests,
noncanonical versions, path-like components, wrong platforms, replay/downgrade offers, length
mismatches, and digest mismatches. `lm-package` now derives this manifest directly from the final
archive bytes and publishes archive, checksum, and update manifest with create-new semantics; a
collision at either later file removes only newly created predecessors and preserves the existing
target. Producer/consumer, malformed-input, mismatch, and atomic-collision tests pass 6/6. Network
discovery, user consent, platform installation/relaunch, signing, and retained end-to-end update
evidence remain incomplete; Release stays Partial and aggregate parity remains 60/65.

Verified-update staging update (2026-08-11): `lm-update` now crosses the filesystem boundary only
after complete manifest/archive verification. It requires an existing directory, opens the exact
portable manifest filename with create-new semantics, writes and syncs all bytes, reopens the file,
and repeats platform/version/length/digest verification. Any write, reopen, or final-verification
failure removes only that newly created file; pre-verification failure creates nothing and a name
collision preserves the existing file byte-for-byte. Five trust-core tests plus three packager tests
pass. Discovery, consent UI, extraction/replacement/relaunch, signing, and live update evidence
remain incomplete; Release stays Partial and aggregate parity remains 60/65.

Streaming-update staging update (2026-08-11): durable update staging no longer requires buffering
an archive of up to 512 MiB in memory. The production path incrementally reads at most 64 KiB,
writes and hashes exactly the declared length, explicitly probes for forbidden trailing bytes,
syncs, reopens, and performs the complete verification again. Truncation, one-byte extension, and
equal-length tampering all remove the newly created destination and leave the staging directory
empty. The byte-slice entry point is now a thin wrapper over this same path. The update trust core
passes 6/6; Release remains Partial and aggregate parity remains 60/65.

Native update-consent update (2026-08-11): the Help menu now exposes a localized `Stage verified
update…` workflow. The user explicitly selects a bounded local `LMUPDATE1` manifest; the native
dialog resolves only its declared same-directory portable archive name, streams a complete
platform/version/length/digest preflight, and displays the verified version, platform, filename,
and size. No output exists until a separate consent action selects a destination directory. That
action invokes the create-new streaming stage/reopen/reverify core, and the dialog states that the
running application is never replaced automatically. Native gates prove exact staging after consent
and tamper rejection before consent; the menu localization invariant, native build, 7/7 trust-core
tests, and 237/237 renderer tests pass. Network discovery and platform replacement/relaunch remain
incomplete; Release stays Partial and aggregate parity remains 60/65.

Transactional update-extraction update (2026-08-11): a staged portable archive can now be decoded
into one create-new version/target directory without modifying the running installation. The
bounded gzip/tar reader accepts only regular files directly beneath the exact expected bundle
prefix, validates every tar checksum and octal size, rejects links/devices/nesting/traversal and
duplicates, limits individual and aggregate decompression, writes each file create-new and synced,
and requires `lm-native`, `lm-cli`, `lm-libretro`, and `RELEASE-MANIFEST.txt` (with Windows suffixes
where applicable). Any failure removes the new version directory; a destination collision preserves
the existing installation. Valid extraction, collision, traversal, missing-runtime, and cleanup
tests raise the update core to 9/9. Launcher switch/relaunch and retained platform evidence remain
incomplete; Release stays Partial and aggregate parity remains 60/65.

Rollback-safe version-selection update (2026-08-11): `lm-update` now activates an extracted
version without replacing any binary. It accepts only a direct portable child of the canonical
install root with a bounded regular `lm-native` executable, hashes that executable, and publishes a
bounded `LMCURRENT1` selector through a synced same-directory temporary rename. A previously valid
selector is retained as `LMCURRENT1.previous`; rollback validates it before republishing it. The
launcher resolver canonicalizes the selected path, requires it to remain exactly two levels below
the install root, and rehashes the executable before returning it. Tests prove two-version switch,
rollback, rejection of external directories, and fail-closed post-activation tampering, raising the
trust core to 11/11. A packaged launcher executable and live relaunch evidence remain incomplete;
Release stays Partial and aggregate parity remains 60/65.

Packaged launcher update (2026-08-11): the new safe-Rust `lm-launcher` executable locates its own
install root, resolves and rehashes `LMCURRENT1`, forwards every OS argument directly without shell
parsing, launches the selected immutable `lm-native`, and returns its exact supported exit code.
Process gates switch between two real executable fixtures, preserve an argument containing spaces,
observe distinct exit codes, roll back to the first version, and reject post-selection tampering.
`lm-package` now requires, hashes, manifests, and archives the launcher; CI and all four release
matrix targets build it. Launcher 2/2, update 11/11, packager 3/3, and renderer 237/237 gates pass.
Native extraction/activation consent and retained cross-platform release execution remain
incomplete; Release stays Partial and aggregate parity remains 60/65.

Native update-activation update (2026-08-11): after explicit staging consent, the native editor
now offers a second explicit choice to keep the archive only or select an install root. Installation
uses the bounded extractor to create one immutable version directory and publishes the checksummed
rollback-safe launcher selector; it never replaces or relaunches the running editor. The completion
dialog identifies both the installed directory and launcher-selected executable and instructs the
user to exit and restart through `lm-launcher`. A native end-to-end test builds a structurally real
portable gzip/tar bundle, installs it, resolves its selected executable, and verifies its exact
contents; a negative test proves activation failure removes the new directory and publishes no
selector. Native updater 4/4, launcher 2/2, update core 11/11, packager 3/3, and renderer 237/237
gates pass. Retained cross-platform release execution, hosted publication evidence, installers,
and platform signing/notarization remain incomplete; Release stays Partial and aggregate parity
remains 60/65.

Localization coverage update (2026-08-12, portable Layer 3 document): twenty-five appended
`Layer3Document*` keys cover four raw selectors, four graphics-file slots, sixteen reserved bytes,
bounded tilemap/remap payloads, domain-authenticated clipboard actions, atomic apply, history/save,
dirty-close, and error lifecycle. The application host passes the live catalog. A complete-form
test applies every field as one revision, proves canonical save/reopen equality, then atomic
Undo/Redo; form validation retains 12-bit graphics limits and exact reserved width. Focused form
tests pass 6/6, the clipboard-domain test passes 1/1 (including crossed-envelope rejection), and
controller tests pass 4/4 for state transitions, stale revisions/tokens, immutable snapshots,
later-edit preservation, history divergence, and request overflow. Localization passes 28/28
active cases (one provenance ignore), and renderer passes 237/237. Other native forms and retained
language-DLL evidence keep Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, custom-sprite placement library): twenty-nine appended
`CustomSprite*` keys cover synchronized placement count/navigation, library header, Unicode
search/descriptions, variable-width record groups, typed placement copy/paste,
replace/remove/insert/move, BOM and LF/CRLF/trailing-newline framing, paired save, history/dirty
state, close confirmation, and errors. The live catalog crosses both main and lifecycle modules.
A complete paired-file test uses mixed placement records under the controller's immutable
sprite-length table, changes header/order/framing in one revision, proves dual-file canonical
reopen including BOM/CRLF/trailing newline, then Undo/Redo while retaining the exact length table.
Editor/form tests pass 6/6 and controller tests 5/5 for width mismatch and late-command rollback,
stale revisions/tokens, aliases/overlapping saves, immutable snapshots, and history divergence.
Localization passes 28/28 active cases (one provenance ignore), and renderer passes 237/237. Other
native forms and retained language-DLL evidence keep Localization Partial; aggregate parity
remains 60/65.

Localization coverage update (2026-08-12, portable entity appearances): twenty-eight appended
`Appearance*` keys cover Layer 1 object, Layer 2 object, and sprite source choices; full-width
source/tile identifiers; signed offsets; palette/flips; painter-order selection, replacement,
removal, insertion and movement; history/save/dirty state; close confirmation; and errors. The
application host passes the live catalog and the obsolete fixed-English source-name array was
removed. A complete-workflow test applies all three source domains with full-width IDs and signed
offsets, reorders them in one revision, proves canonical save/reopen equality, then atomic
Undo/Redo. Focused native tests pass 23/23; controller tests pass 5/5 for ordered mixed edits,
late-command/file-validation rollback, stale revisions, immutable save snapshots, later-edit
preservation, and divergent history. Localization passes 28/28 active cases (one provenance
ignore), and renderer passes 237/237. Other native forms and retained language-DLL evidence keep
Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, portable ExAnimation document): thirty appended
`ExAnimationDocument*` identities cover the shell, open/profile-bound maximum configuration,
record list, slot globals, trigger form, record properties, lifecycle, and error dialogs. Identical
record/frame clipboard, trigger, destination-flag, and slot-apply semantics reuse the already
verified aggregate ExAnimation identities. Record indices, kinds, and trigger values remain exact
opaque substitutions. Four-file audits require every document-specific key and reject fixed window,
heading, label, button, prefix, and slider text. Both audits pass, localization passes 28/28 active
cases (one provenance ignore), native compile passes, and renderer passes 237/237. Other native
forms and retained live language-DLL evidence keep Localization Partial; aggregate parity remains
60/65.

Localization coverage update (2026-08-12, portable palette document): thirteen appended
`PaletteDocument*` identities cover window/lifecycle text, undo/redo/save icon tooltips, document
state, and exact selected-color summaries. Identical copy/paste color actions reuse the verified
aggregate palette identities. Color indices and BGR555 words remain exact substitutions; the grid's
single two-space caption is explicitly audited as a painted swatch rather than language text. Both
whole-surface audits pass, localization passes 28/28 active cases (one provenance ignore), native
compile passes, and renderer passes 237/237. Other native forms and retained live language-DLL
evidence keep Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, installed-ROM palette workflow): thirty-one appended
`RomPalette*` identities cover the editor shell, stale-ROM warning, allocation range, commit and
reclaim actions, staged state, mask editing, close/error dialogs, exact selected-row transfer,
257-word raw transfer, TPL v2, RGB24, and `.palmask` behavior. Identical ownership states and
color/row clipboard actions reuse the verified aggregate palette identities. Color/record indices
and BGR555 words remain exact substitutions; `X`, `•`, and empty swatch marks remain
language-neutral grid state symbols. Four-module audits require every ROM-palette key and reject
fixed window, heading, label, button, and help text. Both audits pass, localization passes 28/28
active cases (one provenance ignore), native compile passes, and renderer passes 237/237. Other
native forms and retained live language-DLL evidence keep Localization Partial; aggregate parity
remains 60/65.

Localization coverage update (2026-08-12, custom-object library): twenty-seven appended
`CustomObject*` keys cover synchronized entry count/search/navigation, multi-object group bytes,
Unicode descriptions, typed object copy/paste, replace/remove/insert/move, UTF-8 BOM and LF/CRLF
framing, trailing newline, paired-file save, history/dirty state, close confirmation, and errors.
The live catalog is routed through both the main form and its split lifecycle module. A complete
paired-file test applies grouped and single objects with Unicode descriptions, reorders them and
changes all framing fields in one revision, proves exact dual-file canonical reopen—including BOM,
CRLF, and trailing newline—then Undo/Redo. Editor/form tests pass 7/7 and controller tests 6/6 for
late-command rollback, stale revisions/tokens, aliased-path rejection, immutable snapshots,
later-edit preservation, framing history, and overflow. Localization passes 28/28 active cases
(one provenance ignore), and renderer passes 237/237. Other native forms and retained
language-DLL evidence keep Localization Partial; aggregate parity remains 60/65.

Cross-platform launcher-gate update (2026-08-11): every native portable-release matrix runner now
executes the package, update, and launcher contract suites before publishing its bundle. The
launcher suite no longer silently skips all process evidence on Windows: its Windows-only tests
copy the platform command processor into two immutable version directories, activate each through
the production checksummed selector, execute it through the production launcher boundary, verify
exact exit-status propagation, roll back to the prior selector, and reject post-activation binary
tampering. Native Unix execution remains covered by its exact argument/exit/switch/rollback tests;
the Windows test target cross-compiles locally. The public workflow currently has no retained run,
so hosted four-platform execution and a real tag publication remain required. Release stays Partial
and aggregate parity remains 60/65.

Localization-extension update (2026-08-11): the fixed `UiTextKey` prefix remains byte-for-byte
stable at its complete 256-ID capacity. New `ExtendedUiTextKey` identities now use a reserved,
typed namespace inside the existing bounded `LMDLG001` extension: one impossible original-dialog
ID/item tuple plus a 32-bit stable key ID. Original Win32 dialog titles/items remain independently
addressable, cannot inject into the reserved namespace, and continue to round-trip beside native
extensions. Missing extended translations fall back to each typed key's built-in English; duplicate
keys, unknown reserved IDs, oversized/empty text, malformed framing, and collisions fail closed.
The initial eight-key tilemap vocabulary proves the public model and native lookup adapter without
yet claiming that editor localized. Localization 28/28 focused model tests, native frontend 9/9,
and renderer 237/237 pass. Localization remains Partial and aggregate parity remains 60/65.

Localization coverage update (2026-08-11, complete Map16-set editor): the full document editor now
routes its window, page navigation, Undo/Redo/Save, copy/paste, page insertion/removal, modified
state, address, subtile fields, palette/priority/flips, Acts Like editor, preview fallback, unsaved
confirmation, and error acknowledgement through the active typed catalog. Existing semantically
identical Copy, Paste, Palette, Cancel, Discard, and OK keys are reused; eighteen new keys bring the
stable one-byte catalog to its exact 256-key capacity without changing any prior identifier.
Migration retains every historical 19/183/184/199/201/212/230/238-key translated prefix. A
four-file source audit prevents literal widget text across the complete editor surface. Localization
26/26, Map16-set editor 4/4, and renderer 237/237 pass. Remaining native forms require the catalog's
forward-compatible extension mechanism rather than overflowing the published byte key domain;
Localization stays Partial and aggregate parity remains 60/65.

Localization coverage update (2026-08-12, aggregate Layer 2 domain): thirty-five appended
`NativeAssetsLayer2*` identities plus shared object identities cover ordinary Layer 2 object-stream
editing and the complete 32×32 tilemap workflow: descriptor/bank context, rectangle selection,
storage index, tile-word load/fill, connected flood fill, move, patterned resize, pattern capture and
flood, cut/copy/paste, and Lunar Magic remap programs. Coordinates, cell counts, descriptors, banks,
tile words, and captured dimensions remain exact substitutions; directional and edge glyph buttons
remain language-neutral symbols with localized hover guidance. The complete Layer 2 source audit,
localization 28/28 active cases (one provenance ignore), native compile, and renderer 237/237 pass.
ExAnimation and Settings remain, so Localization stays Partial and aggregate parity remains 60/65.

Localization coverage update (2026-08-12, complete native level-assets aggregate): nineteen
appended `NativeAssetsAnimation*` identities cover slot globals, triggers, record semantics,
append/replace/remove, and record/frame clipboard workflows. Twenty-eight appended
`NativeAssetsSettings*` identities cover custom Layer 3 graphics and expanded mode, Super GFX
Bypass, sprite boundary interaction, all sixteen lossless raw words, and installed palette/vanilla/
global/level animation features. Stable slot names (`FG1`–`BG3`, `SP1`–`SP4`) remain hardware
identifiers rather than prose. Per-domain audits reject literal widget text, and a whole-surface
audit requires every `NativeAssets*` key across the shell and all five panels. Aggregate audits pass
6/6, localization passes 28/28 active cases (one provenance ignore), native compile passes, and
renderer passes 237/237. Other native forms and retained live language-DLL evidence keep
Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, verified-update family): all eighteen static and dynamic
texts across offer review, staging, immutable installation, activation, restart/rollback guidance,
and failure acknowledgement now route through typed `UiTextKey` entries and the active catalog.
Version, target, archive, byte count, and installed paths use localized templates rather than
concatenated English labels. Catalog decoding preserves each historical 19/183/184/199/201/212-key
translated prefix and appends English fallbacks for the new 230-key schema. A source audit rejects
literal window/button/label text in the complete update dialog, and a translated-template test
proves dynamic replacement. Localization 26/26 focused tests, native updater 6/6, and renderer
237/237 pass. Remaining native forms and retained original language-DLL behavior keep Localization
Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, aggregate Level domain): twenty-six appended
`NativeAssets*` identities plus the matching native-document identities now cover source/header
summary, every legacy header semantic, custom-time controls, staged header lifecycle, Layer 1
objects, sprite tokens, sprite memory/buoyancy, authenticated Lfix3 spawn settings, clipboard,
semantic fields, and ordering controls. The catalog-backed shared panel is exercised by both
portable and installed-ROM aggregate editors. A complete Level-panel source audit rejects literal
heading, label, button, slider, and collapsing captions. The focused audit passes, localization
passes 28/28 active cases (one provenance ignore), native compile passes, and renderer passes
237/237. The Layer 2, Palette, ExAnimation, and Settings aggregate domains remain, so Localization
and aggregate parity stay Partial and 60/65 respectively.

Localization coverage update (2026-08-12, aggregate Palette domain): ten appended
`NativeAssetsPalette*` identities cover exact color/index summaries, editable/fixed/ExAnimation/
invalid ownership status, color and complete-row clipboard actions, and modifier-key guidance.
Dynamic indices, BGR555 values, and owning ExAnimation record IDs remain exact opaque substitutions.
The panel audit rejects literal labels, buttons, and help text while explicitly allowing its one
empty painted color-swatch button. The focused audit passes, localization passes 28/28 active cases
(one provenance ignore), and renderer passes 237/237. Layer 2, ExAnimation, and Settings remain, so
Localization stays Partial and aggregate parity remains 60/65.

Localization coverage update (2026-08-12, lossless OSC custom-object metadata): seventeen
appended `Osc*` keys cover the complete source editor, record diagnostics, history/save/dirty
state, close confirmation, and error lifecycle, with the live catalog passed by the application
host. The UI/source suite passes 3/3 and proves exact BOM, mixed-line-ending, malformed, and
non-UTF-8 preservation across replacement/Undo/Redo. The controller suite passes 2/2 for stale
revision rejection, maximum-length atomicity, immutable save snapshots, wrong-request rejection,
and preservation of edits made after persistence begins. Localization passes 28/28 active cases
(one provenance ignore), and renderer passes 237/237. Other native forms and retained live
language-DLL evidence keep Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, portable overworld metadata): forty appended
`Metadata*` keys cover the complete three-panel editor: level-name records, player starts,
submap settings, every hexadecimal field, seven submap choices, upsert/remove actions,
history/save/dirty state, close confirmation, and error lifecycle. The application host passes the
live catalog through the parent form and every panel/helper. A domain-spanning test applies one
level name, one player start, and one submap-settings record—including raw flags and five unknown
bytes—as one revision, proves canonical encode/reopen equality, then atomic Undo/Redo. Native form
tests pass 9/9; controller tests pass 3/3 for revision binding, divergent history, immutable save
snapshots, stale acknowledgement retention, later-edit preservation, and request overflow.
Localization passes 28/28 active cases (one provenance ignore), and renderer passes 237/237.
Other native forms and retained language-DLL evidence keep Localization Partial; aggregate parity
remains 60/65.

Localization coverage update (2026-08-11, current-level palette transfer): the complete native
import/export format chooser now routes its export/import titles, format explanation, Raw/TPL/RGB
actions, optional `.palmask` guidance, Cancel/OK actions, and error title through the active typed
catalog. A source audit rejects literal window/button/label/small text in this whole transfer
surface. Catalog migration retains each historical 19/183/184/199/201/212/230-key translated
prefix and appends English fallbacks into the new 238-key schema. Localization 26/26, focused
palette-transfer 4/4, and renderer 237/237 gates pass. Remaining native forms and retained original
language-DLL evidence keep Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, lossless DSC sidecar): eighteen appended `Dsc*` keys
cover the complete source editor, including byte/record summary, preservation notice, complete
source replacement, recovered-record diagnostics, history/save/dirty state, close confirmation,
and error lifecycle. The application window loop passes the active catalog end to end. The source
mutation remains revision-bound and preserves its real binary domain rather than normalizing text:
a focused test proves BOM, CRLF/LF, malformed, and non-UTF-8 bytes across replacement, Undo, and
Redo. The DSC editor/form suite passes 4/4, localization passes 28/28 active tests (one provenance
ignore), and renderer passes 237/237. Other native forms and retained live language-DLL evidence
keep Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, lossless SSC custom-sprite metadata): twenty appended
`Ssc*` keys cover the complete sidecar window: byte/record summary, discovered external-graphics
slot counts, independently localized loaded/missing palette states, complete source replacement,
record diagnostics, history/save/dirty state, close confirmation, and error lifecycle. The
application host passes the live catalog. Technical filenames and asset decoder errors remain
opaque data. The source remains binary-preserving and revision-bound; the focused suite proves
BOM, mixed line endings, malformed and non-UTF-8 bytes across replacement/Undo/Redo, nearest
ExternalGraphics discovery, MW3-over-RGB palette precedence, slot/palette decode, and unknown-file
rejection. SSC tests pass 5/5, localization passes 28/28 active cases (one provenance ignore), and
renderer passes 237/237. Other native forms and retained language-DLL evidence keep Localization
Partial; aggregate parity remains 60/65.
Localization coverage update (2026-08-11, title/credits tilemap family): the first production use
of `ExtendedUiTextKey` covers both installed ROM tilemap editors as one complete family. Typed
extension text now controls title-versus-credits names, dynamic window/dimensions/confirmation/error
templates, stale-revision guidance, row/column/plane labels, primary/secondary selection, tile word,
load/apply/commit actions, staged state, and unsaved guidance; shared Cancel, Discard, and OK remain
in the stable prefix. The native call boundary supplies the active catalog to each editor without
changing other ROM-editor signatures. A two-module source audit proves every one of the nineteen
extension keys is consumed and rejects literal window/button/label text. Localization model 28/28,
tilemap editor 7/7 (including install/reopen and recovery), and renderer 237/237 pass. Localization
stays Partial for remaining forms and live original language-DLL evidence; aggregate parity remains
60/65.
Localization coverage update (2026-08-11, overworld event-number map): the complete 256-entry
installed ROM editor now consumes fourteen typed extension keys for its window, semantic/storage
description, dynamic stored-length template, stale warning, event/mapped-event fields, load/apply/
commit actions, staged state, unsaved confirmation, and errors; shared Cancel/Discard/OK remain in
the fixed catalog. Its active-catalog call path is isolated from other ROM editors. A source audit
rejects literal widget text and requires every `EventNumber*` key. Localization model 28/28,
event-number editor 5/5—including high-event install/reopen, invalid/stale handling, and pristine
plus installed recovery—and renderer 237/237 pass. Localization stays Partial for remaining forms
and live original language-DLL evidence; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, installed-ROM Map16 main editor): fifty-one typed
`RomMap16*` keys now cover the complete main canvas, preview level/object set/palette, grid/zoom/page
display, rectangle selection, page/tile/quadrant and address templates, native tile clipboard,
staged undo/redo, subtile graphics/palette/priority/flips, Acts Like, protected built-in pages,
allocation, commit/reclaim, staged state, discard, and error lifecycle. The active catalog is wired
from the application into the editor and lifecycle module; a two-module source audit requires every
key and rejects literal regressions. Localization model 28/28, Map16 editor 68/68—including complete,
selected, legacy, bitmap, SNES tileset, atomic graphics+Map16, Layer 2 placement, reopen/undo, and
recovery—and renderer 237/237 pass. Import/export subdialog catalog wiring remains, so Localization
stays Partial and aggregate parity remains 60/65.

Localization coverage update (2026-08-11, complete Graphics/ExGFX family): fourteen additional
typed keys close tile/cache diagnostics, level-GFX export and 3bpp/4bpp format confirmations, tile
clipboard actions, and Yes/No controls; seven keys close atomic batch extraction/insertion progress
and cancellation; five keys close toolbar standard/ExGFX completion and error dialogs using semantic
action/count/path state rather than pre-rendered English. The shared batch worker receives the active
catalog from ROM, vanilla, and toolbar entry points. A direct audit now finds no literal window,
button, label, small-text, combo-label, checkbox-label, or hover text across all seven ROM graphics
modules. Localization model 28/28, ROM graphics 45 active tests with one explicit Wine/original-LM
ignore, toolbar transfer 8/8, and renderer 237/237 pass. The complete Graphics/ExGFX UI family is
catalog-driven; other native forms and retained live language-DLL evidence keep Localization
Partial and aggregate parity at 60/65.

Localization coverage update (2026-08-11, graphics file operations): thirty-two additional typed
extension keys now cover the main ROM Graphics window, stale and palette-row state, joined-AllGFX
mode, configured/direct external-editor actions, raw GFX/ExGFX transfer, current-level FG/BG/SP
extraction, all standard GFX, authenticated GFX32/GFX33, installed ExGFX, joined AllGFX extraction
and insertion, allocation bounds, commit/reclaim, and staged state, including hover guidance. A
source audit requires the complete key group and rejects literal file-operation regressions.
Localization model 28/28, graphics editor 43 active tests with one explicit Wine/original-LM ignore,
and renderer 237/237 pass. Tile/pixel controls, warnings/confirmations, batch progress, and technical
status localization remain, so Localization stays Partial and aggregate parity remains 60/65.

Localization coverage update (2026-08-11, graphics lifecycle/ownership/external editing): nineteen
typed extension keys now cover the complete external-process consent and running-state dialogs,
including executable/staged paths and argument templates; all seven editable/fixed/animation/
invalid ownership classifications; and the graphics discard/error lifecycle. Shared Cancel,
Discard, and OK retain fixed-prefix keys. A three-module source audit requires every key and rejects
literal regressions. Localization model 28/28 and the broad graphics-editor family 42 active tests
pass with one explicit Wine/original-LM oracle ignore, covering process cleanup/reload, ownership,
standard and ExGFX insertion, atomic import, reopen/undo, and recovery. Renderer parity remains
237/237. The main editor, batch progress, insertion warnings, and remaining graphics controls still
need catalog wiring, so Localization and aggregate parity remain Partial and 60/65 respectively.

Localization coverage update (2026-08-11, shared/custom palettes): the complete main editor and
`.smwpal` transfer surface now use thirty typed extension keys for the window, dynamic backend/color
summary, stale state, exact import/export guidance, page/selection templates, raw/RGB channels,
preview, color/row clipboard actions, expanded auxiliary bytes, commit state, discard, and errors.
Shared Cancel/Discard/OK retain fixed-prefix keys. A two-module source audit requires every
`SharedPalette*` key and rejects representative literal bypasses. Localization model 28/28 and the
shared-palette family 12/12 pass: legacy/expanded backends, ExLoROM routing, exact native-file round
trips, safe backend upgrade, RGB/raw and row edits, auxiliary preservation, commit/reopen/undo, and
composed recovery. Renderer parity remains 237/237. Localization stays Partial for remaining forms
and live original language-DLL evidence; aggregate parity remains 60/65.
Localization coverage update (2026-08-11, overworld level names): the complete lossless 19-tile
name-table editor now uses fifteen typed extension keys for its window, record/count descriptions,
stale warning, level/tile/value fields, load/apply/commit actions, staged state, unsaved confirmation,
and errors; shared Cancel/Discard/OK remain fixed-prefix entries. Its active-catalog call boundary
is isolated from neighboring ROM editors. A source audit rejects literal widget text and requires
every `LevelName*` key. Localization model 28/28, level-name editor 6/6—including canonical growth
across the level-number gap, pristine install/reopen, stale selection safety, and pristine/installed
maximum-table recovery—and renderer 237/237 pass. Localization stays Partial for remaining forms
and live original language-DLL evidence; aggregate parity remains 60/65.
Localization coverage update (2026-08-11, overworld player starts): the complete two-player native
start editor now uses twenty-six typed extension keys for its window, exact-record/reserved-byte
descriptions, stale warning, player selector, coordinates, submap selector, seven submap names,
load/apply/commit actions, staged state, unsaved confirmation, and errors. Shared Cancel/Discard/OK
remain fixed-prefix entries. A source audit rejects literal widget text and requires every
`PlayerStart*` key; a translated-catalog test proves both end submap names route through the
extension while missing entries fall back to English. Localization model 28/28, player-start editor
5/5—including semantic reopen with reserved bytes, alignment/stale safety, and exact two-player
recovery—and renderer 237/237 pass. Localization stays Partial for remaining forms and live
original language-DLL evidence; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, overworld special-event reveals): the complete 24-record
special-event editor now uses fifteen typed extension keys for its window, exact-table description,
stale warning, index/source/destination/direction fields, load/apply/commit actions, staged state,
unsaved confirmation, and errors. Shared Cancel/Discard/OK retain their fixed-prefix keys. A source
audit rejects literal widget text and requires every `SpecialEvent*` key. Localization model 28/28,
special-event editor 5/5—including pristine install and complete three-plane reopen, invalid/stale
handling, pristine recovery, and installed-table preservation—and renderer 237/237 pass.
Localization stays Partial for remaining forms and live original language-DLL evidence; aggregate
parity remains 60/65.

Localization coverage update (2026-08-11, overworld event reveals): the complete variable-length
mixed-endian reveal editor now uses seventeen typed extension keys for its window, table/count
description, stale warning, index/source/destination/count fields, resize/load/apply/commit actions,
staged state, unsaved confirmation, and errors. Shared Cancel/Discard/OK retain their fixed-prefix
keys. A source audit rejects literal widget text and requires every `EventReveal*` key. Localization
model 28/28, event-reveal editor 5/5—including growth to 200 records and last-record semantic
reopen, invalid/stale safety, pristine expanded-table recovery, and installed-table preservation—
and renderer 237/237 pass. Localization stays Partial for remaining forms and live original
language-DLL evidence; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, global secondary exits): the complete 8,192-entry editor
now combines authenticated original dialog `$03F1` text for its title, Destination/Screen/X/Y, and
Clear Entry/Clear All controls with eighteen typed extension keys for the Rust-native description,
stale/status text, extra flag fields, apply/commit flow, clear-all confirmation, discard, and error
surfaces. Shared Cancel/Discard/OK retain fixed-prefix keys. The source audit requires every
`SecondaryExit*` key while the retained dialog-inventory test proves original resource overrides and
fallbacks. Localization model 28/28 and secondary-exit editor 9/9 pass, including final-entry
pristine recovery, installed-table preservation, invalid-state handling, and exact clear-one/
clear-all behavior. Renderer parity remains 237/237. Localization stays Partial for remaining forms
and live original language-DLL evidence; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, overworld event tilemaps): the complete 2,048-cell,
three-plane editor now uses twenty typed extension keys for its window, buffer description, dynamic
pristine/installed storage state, stale warning, tile/plane/value fields, all three plane choices,
load/apply/commit actions, staged state, unsaved confirmation, and errors. Shared Cancel/Discard/OK
retain fixed-prefix keys. A source audit rejects literal widget text and requires every
`EventTilemap*` key. Localization model 28/28, event-tilemap editor 5/5—including pristine install,
exact primary-high/secondary-high reopen, bounds/stale safety, complete three-plane recovery, and
installed-plane preservation—and renderer 237/237 pass. Localization stays Partial for remaining
forms and live original language-DLL evidence; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, overworld global settings): the complete seven-record
raw and semantic Layer 3 editor now uses twenty-six typed extension keys for its window, installed/
pristine runtime state, stale warning, submap selection, dynamic word/GFX labels, semantic fields,
preservation guidance, apply/commit actions, staged state, unsaved confirmation, and errors. Shared
Cancel/Discard/OK retain fixed-prefix keys. A source audit rejects literal window/button/label/
small/header text and requires every `OverworldSettings*` key. Localization model 28/28 and the
settings family 7/7 pass: all-record install/reopen, malformed/stale safety, semantic Layer 3
install/reopen/undo, opaque flag/byte/high-nibble preservation, and pristine/installed recovery.
Renderer parity remains 237/237. Localization stays Partial for remaining forms and live original
language-DLL evidence; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, Map16 exact transfers): twenty-one typed extension keys
now cover the complete-file core/template notices, selected-range dimensions and file origin,
rectangle clipboard actions, legacy page import/export, foreground/background pairs, and their
compatibility boundaries. The focused source audit separates these keys from the main Map16 editor
surface and rejects representative literal bypasses. Exact complete-file/template, selected-range,
legacy-plane, asynchronous revision binding, shortcuts, GUI commit/reopen, and recovery evidence
passes in the Map16 editor suite (69/69); localization model 28/28 and renderer 237/237 also pass.
Localization remains Partial pending the Map16 bitmap, sidecar, and SNES tileset subdialogs and
other native forms; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, Map16 sidecars and SNES tilesets): thirty typed extension
keys cover both associated custom-Map16 export buttons, the path-specific confirmation, SNES
graphics/map/palette options, offsets and color map, complete preview statistics, revision warning,
atomic apply, and discard. A dedicated source audit requires every key and rejects representative
literal regressions. Sidecar path/atomic replacement and shortcut evidence plus SNES materialize,
placement, palette, background index-grid, atomic commit/reopen, and undo evidence pass in the
focused Map16 suite (70/70); localization model 28/28 and renderer 237/237 also pass. Localization
remains Partial pending bitmap-converter and other native surfaces; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, Map16 bitmap conversion): forty-three typed extension
keys now cover the complete bitmap import/conversion UI, including progress, input controls,
allocation and optimization options, both reduction algorithms, disabled original preference and
its explanation, eight-row palette availability, synchronized previews, dynamic plan/allocation
statistics, exhaustion and stale warnings, and final actions. The dedicated source audit requires
every key and rejects representative literal regressions. Bitmap codec variants, clipboard bounds,
preview pixels/geometry, option retention, palette rows, request capture, and revision binding pass
in the focused Map16 suite (71/71); localization model 28/28 and renderer 237/237 also pass. This
completes typed coverage for the full Map16 editor family. Localization remains Partial for other
native forms and live original language-DLL evidence; aggregate parity remains 60/65.

Live localization evidence update (2026-08-11):
`every_original_363_dialog_resource_decodes_with_the_portable_template_parser` passes separately
against the locally supplied Lunar Magic 3.63 x86 and x64 executables, proving all 107 mapped dialog
resources decode through the bounded standard/extended Win32 parser in both binaries. The remaining
localization gap is native-form binding coverage, not executable-resource parsing; aggregate parity
remains 60/65.

Localization coverage update (2026-08-11, ROM expansion): thirty-two typed extension keys cover
the complete expansion UI across ordinary LoROM target/fill, 64-Mbit ExLoROM conversion warning,
fixed 6/8 MiB SA-1 targets, emulator compatibility, confirmations, and errors. A source audit
requires every `RomExpansion*` key and rejects literal window/action regressions. Exact preset,
eligibility, confirmation, and lifecycle evidence passes in the expansion suite (9/9), alongside
localization model 28/28 and renderer 237/237. Localization remains Partial for other native forms;
aggregate parity remains 60/65.

Localization coverage update (2026-08-12, portable graphics document): twenty-six appended typed
keys cover the complete portable graphics-document window and shared tile controls: title,
palette selection, undo/redo/save, typed copy/paste, dirty state, tile identity/empty state,
page and palette navigation, rotation/flips, color-map launch/application, close confirmation, and
error lifecycle. The application window loop now passes the active catalog end to end, and the
shared navigation/transform/color-map controls receive the same catalog across portable,
pristine-ROM, and installed-ROM graphics editors. Mutation and persistence behavior is unchanged:
pixel edits, shifts, transforms, color maps, typed clipboard paste, history, dirty close, and
bounded asynchronous save remain controller-revisioned. The focused graphics suite passes 41/41
active tests (one retained-ROM provenance ignore), localization passes 28/28 active tests (one
provenance ignore), and renderer passes 237/237. Other native forms and retained live
language-DLL evidence keep Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, level-access restriction): twenty typed extension keys
cover every Rust-only stage around the original `$03FF` dialog: acknowledgement, restore-point
policy, persistence before IPS, IPS choice, completion, save-and-close retries, and errors. The
source audit requires every `LevelRestriction*` key and rejects literal stage-window regressions;
the existing original-template test continues to bind all matching native captions. Workflow and
localization tests pass 6/6, localization model 28/28, and renderer 237/237. The full Localization
row remains Partial for other native forms; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, overworld ExAnimation records): twenty-five typed keys
cover the reusable animation-edit panel across its portable complete-overworld and installed-ROM
hosts. The catalog now reaches domain selection, global read-only ownership, settings/header,
triggers, records, special transfers, append/replace/remove, and typed record/frame clipboard
actions. A complete source audit rejects representative literal regressions. Panel localization and
ownership navigation pass 2/2; installed animation cadence/table checks pass 3/3, localization model
28/28, and renderer 237/237. Per-map runtime options, preview controls, and remaining profile
evidence keep the overworld animation row Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, installed overworld animation options and preview):
twenty-seven appended `ExtendedUiTextKey` values now cover the installed editor's map selector,
original/global/map animation switches, runtime installation states, preview playback/reset/step
controls, phase/tick reporting, trigger kind and state, event state, and empty-record guidance. The
shared-panel audit stops at the installed-only suffix boundary, while a dedicated installed-controls
audit requires every new key and rejects representative literal regressions. The installed audit
passes 1/1, the shared animation family passes 3/3, localization passes 28/28, and the renderer
passes 237/237. Remaining profile and mapper/runtime variant evidence keeps the overworld animation
row Partial and aggregate parity at 60/65.

Localization coverage update (2026-08-11, title-screen recording): twenty-nine appended
`ExtendedUiTextKey` values cover the complete installed-ROM recording form, including movement
payload state, normalization and commit, temporary recorder install/uninstall guidance, native and
emulator-state transfer actions, status, unsaved confirmation, and error lifecycle. The application
host now passes the active catalog into this editor. A dedicated audit requires every
`TitleRecording*` key and rejects representative literal regressions without translating opaque
parser or background-I/O errors. The focused family passes 7/7, localization passes 28/28, and the
renderer passes 237/237. Localization remains Partial for other native forms and retained live
language-DLL evidence; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, ROM message editors): thirty appended typed keys cover
the variable overworld-message table and fixed boss-sequence message table as one native family.
Shared row/load/apply/commit/status/dialog actions reuse `MessageEditor*`; storage/count/dimension
and domain-specific lifecycle text remain explicit `OverworldMessage*` and `BossMessage*` keys.
Both ROM-window call sites thread the active catalog, including the overworld save-notification
route. Dedicated source audits require each complete key family and reject representative literal
regressions. Overworld messages pass 6/6, boss messages pass 6/6, localization passes 28/28, and the
renderer passes 237/237. Other native forms and retained live language-DLL evidence keep
Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, Lunar Magic ROM metadata): twenty-four appended
`RomMetadata*` keys cover the fixed-region editor's title, description, four-value summary template,
stale state, compact and ranged region names, byte fields/actions, commit/status, unsaved dialog,
and error lifecycle. The application call site now threads the active catalog; raw metadata values
and validation/I/O errors remain technical payloads. A dedicated complete-family source audit
rejects representative literal regressions. The retained-LM-3.63 editor suite passes 5/5 across
protected bits, lossless opaque-byte edits, exact commit/reopen, and recovery of all three owned
regions without touching the live project; localization passes 28/28 and renderer 237/237. Other
native forms and live language-DLL evidence keep Localization Partial; aggregate parity remains
60/65.

Localization coverage update (2026-08-11, legacy standard-GFX bypass): nineteen appended
`LegacyBypass*` keys cover the shared FG/BG and sprite form's domain titles, enablement, native
255-row combo and historical regular-field modes, fallback/stale guidance, stage/commit/status,
close confirmation, and error lifecycle. `FG1`–`FG3`, `SP1`–`SP4`, and row assignments remain
technical data; transaction descriptions deliberately remain stable English history identifiers
rather than locale-dependent UI text. Both application call sites pass the active catalog. The
complete-family audit and semantic/recovery suite pass 7/7, including independent domain staging,
one-Undo combined commit, merged recovery, and exact 400-byte table transfer; localization passes
28/28 and renderer 237/237. Other native forms and retained live language-DLL evidence keep
Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-11, installed expanded settings): thirty typed extension
keys cover the complete profile-backed editor surface—exact record guidance, Layer 3 semantic
controls, ten bypass assignments, sprite-boundary behavior, sixteen raw words, staging/commit,
discard, stale, and error states. A source audit requires every `RomExpandedSettings*` key and
rejects representative literal regressions. Semantic commit/reopen/checksum/Undo plus staged crash
recovery pass in the focused suite (3/3); localization model 28/28 and renderer 237/237 also pass.
The broader runtime-patch row remains Partial for its documented mapper/identity variants, leaving
aggregate parity at 60/65.

Localization coverage update (2026-08-11, expanded-settings documents): sixteen document-specific
typed keys plus the shared ROM semantic keys cover the standalone 32-byte editor, including apply,
Undo/Redo, Save, modified state, close confirmation, and error lifecycle. Its source audit requires
every `ExpandedSettings*` key and rejects representative literal regressions. Exact semantic/raw
editing, history, and save-state evidence passes with both editor forms in the family suite (10/10),
alongside localization model 28/28 and renderer 237/237. The runtime-patch row remains Partial for
documented mapper/identity gaps; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, copier-header conversion): thirteen appended
`CopierHeader*` keys cover the complete transactional conversion form: logical length and current
physical-state templates, both target choices, fill byte, logical-address preservation guidance,
canonical Lunar Magic header synthesis, conversion/cancel actions, and error lifecycle. Dynamic
ROM lengths remain data substituted into localized templates. A complete-family source audit
requires every key and rejects representative literal regressions. The semantic conversion and
canonical-header suite passes 5/5, localization passes 28/28, and renderer passes 237/237.
Localization remains Partial for other native forms and retained live language-DLL evidence;
aggregate parity remains 60/65.

Localization coverage update (2026-08-12, built-in runtime family): forty-one appended
`BuiltInRuntime*` keys cover the complete ten-family installer: selector names and descriptions,
target identity, authenticated-current state, every Lfix3/Map16/ExAnimation/Layer-2 legacy
generation explanation, atomic expansion/install guidance, stale state, install/migrate actions,
and errors. Runtime identities and `$` format/version numbers remain intentional technical content
inside localized strings. The workspace exposes typed keys rather than English text, and the host
passes the active catalog. The complete-family source audit plus semantic suite pass 17/17 active
with three explicit retained-ROM ignores, covering exact route selection, authentication,
migration, header variants, occupied expansion, reopen, and byte-exact Undo. Localization passes
28/28 and renderer passes 237/237. Runtime patches remain Partial for documented mapper/identity
variants; Localization remains Partial for other native forms and retained live language-DLL
evidence; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, ROM loading): six appended `RomLoader*` keys cover the
complete asynchronous loader UI: missing-header confirmation, add/cancel actions, and bounded
read/validation progress. The effects host snapshots the active catalog before borrowing mutable
application state, so the prompt and worker window consistently use the selected language. Loader,
identity, filesystem, and cancellation errors remain technical payloads. The complete-family source
audit and semantic suite pass 6/6, including bounded supported/malformed reads, exact pending-request
routing, Lunar Magic copier-header synthesis, prompted and silent paths, and refusal to overwrite a
ROM changed after confirmation. Localization passes 28/28 and renderer passes 237/237. Other native
forms and retained live language-DLL evidence keep Localization Partial; aggregate parity remains
60/65.

Localization coverage update (2026-08-12, single-level MWL import): nine appended `MwlImport*`
keys cover the complete modern-binary and legacy-sidecar progress lifecycle: primary and sidecar
reads, optional-palette fallback, commit with diagnostics, success/failure, close, paths, and level
numbers. Paths, hexadecimal level numbers, and parser diagnostics remain data substituted into
localized templates; loader and semantic errors remain technical payloads. The application host
passes the active catalog into commit acknowledgement and failure. The complete-form audit passes
4/4 with stale revision and sidecar-order checks. Broader import evidence passes 3/3 across modern
binary MWL, legacy Layer 1/Layer 2/sprites/palette bundles, directory failure isolation, checksum,
and headered/headerless logical equivalence. Localization passes 28/28 and renderer passes 237/237.
Other native forms and retained live language-DLL evidence keep Localization Partial; aggregate
parity remains 60/65.

Localization coverage update (2026-08-12, batch MWL workflows): nineteen `MwlBatchImport*` and
nine `MwlBatchExport*` keys cover directory/counter/allocation controls, per-file read/prepare/
commit/failure diagnostics, cancellation and completion, export template/group-publication state,
and result/error lifecycle. Paths, counts, level numbers, and technical errors remain substituted
data. Commit acknowledgements and the export window now receive the active catalog from the host.
The complete paired suite passes 11/11, including revision-bound nonblocking reads, stale/cancelled
completion rejection, one-level acknowledgement progression, no partial publication on export
cancellation, modified-only installed export, all 512 builtin levels, and all 512 installed levels
across headered/headerless ROMs. Localization passes 28/28 and renderer passes 237/237. Other native
forms and retained live language-DLL evidence keep Localization Partial; aggregate parity remains
60/65.

Localization coverage update (2026-08-12, VRAM patch options): seventeen appended `VramPatch*`
keys cover the complete deferred-install surface: graphics-slot/vertical-resize purpose, next-save
semantics, None/Normal/two HD selectors and help, unknown-runtime protection, actions/errors, and
the three resulting application status messages. The active catalog is threaded through the dialog
and status route. The complete semantic suite passes 9/9, including pristine defaults, installed
and unknown choice gating, deferred None, one-Undo atomic composition of Normal installation with
the level save, authenticated old-generation replacement with exact Undo, and unknown-runtime
rejection without project mutation. Localization passes 28/28 and renderer passes 237/237. Other
native forms and retained live language-DLL evidence keep Localization Partial; aggregate parity
remains 60/65.

Localization coverage update (2026-08-12, legacy ExGFX bypass-list transfer): five appended
`LegacyBypassTransfer*` keys cover extraction completion/path fallback and the shared asynchronous
error/acknowledgement lifecycle. Paths remain data substituted into a localized template; loader,
persistence, and revision errors remain technical payloads. Native transfer tests pass 3/3,
including exact 400-byte envelope-free create/replace and concurrent-transfer rejection.
Application semantics pass 2/2, proving exact-length rejection, prerequisite installation,
transactional import, semantic reopen, byte-identical re-export, and atomic Undo. Localization
passes 28/28 and renderer passes 237/237. Other native forms and retained live language-DLL
evidence keep Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, IPS application): nine appended `IpsApply*` keys cover
the complete revision-bound IPS application form: logical-offset/header preservation, dynamic
source/target/change counts, identity and bank-shape requirements, stale state, transactional
apply/cancel actions, and error lifecycle. Counts remain data substituted into one localized
template. A complete-family source audit requires every key and rejects representative literal
regressions. Real-ROM transactional application, malformed/no-op rejection, stale-revision safety,
and loader framing pass 4/4; localization passes 28/28 and renderer passes 237/237. Localization
remains Partial for the create-IPS workflow, other native forms, and retained live language-DLL
evidence; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, IPS creation): eleven appended `IpsCreate*` keys cover
the complete asynchronous creation workflow, including original/modified ROM picker prompts,
running input/output paths, progress, output-path/size completion, and error lifecycle. Menu,
toolbar, level-restriction continuation, and window-loop call sites now pass the active catalog end
to end. Paths and byte counts remain data substituted into localized templates; technical worker,
filesystem, and codec errors remain opaque payloads. The complete-family source audit and semantic
suite pass 4/4, proving exact round-trip creation, Lunar Magic's headered IPS-coordinate convention,
input preservation, and alias/output-collision rejection; localization passes 28/28 and renderer
passes 237/237. Localization remains Partial for other native forms and retained live language-DLL
evidence; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, owned-RATS reclamation): ten appended `RatsReclaim*`
keys cover the complete manifest-bound reclamation preview, including ownership safety, dynamic
reclaim/retain counts, fill byte, manifest revalidation, atomic checksum-repaired commit, stale
state, actions, and error lifecycle. Counts remain data substituted into a localized template;
manifest/parser errors remain technical payloads. The complete-family source audit and workspace
suite pass 3/3, including shared manifest identity between preview and command, invalid fill and
stale-revision rejection, and acknowledgement cleanup. Localization passes 28/28 and renderer
passes 237/237. Localization remains Partial for other native forms and retained live language-DLL
evidence; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, revision-patch installation): thirteen appended
`RevisionPatch*` keys cover the complete profile-bound installer surface, including template
identity, payload/guarded-write counts, logical-PC allocation range, fill, protected-metadata and
atomic-install guarantees, stale state, actions, and error lifecycle. Template names and typed
game/region/mapper identities remain technical data substituted into localized templates. The
application host now passes the active catalog. The complete-family source audit and installer
suite pass 5/5, including bounded canonical decode, foreign-profile rejection, revision binding,
allocation/fill validation, and commit-only closure; localization passes 28/28 and renderer passes
237/237. The runtime-patch row remains Partial for its documented mapper/identity variants, and
Localization remains Partial for other native forms and retained live language-DLL evidence;
aggregate parity remains 60/65.
## Portable MWL document localization parity

The complete portable level document window now uses 139 typed extension keys across
recovered header and entrance fields, Layer 3 settings, all eight sections, exact Layer 1 object
header/records/semantic fields, legacy and expanded sprite streams, record-length interpretation,
palette and ExAnimation import, palette metadata/colors, Super GFX Bypass animation options,
ExAnimation globals/triggers/records/frames, history, persistence, dirty-close confirmation, and
errors. The application host carries the active catalog while standalone hosting retains the same
bounded English fallback. A five-module source audit prevents literal widget captions from
re-entering the workflow. Focused MWL and localization tests pass, and the clean-cache renderer
gate remains 237/237. Other native forms and retained live language-DLL evidence keep Localization
Partial; aggregate parity remains 60/65.

## Native Map16 sidecar localization parity

Thirty typed extension keys now cover the complete `.m16`/`.s16` window: format interpretation,
kind/size diagnostics, raw dword selection and mutation, decoded definition preview,
quadrant/subtile/palette/priority/flip editing, history, persistence state, dirty-close
confirmation, and errors. The application host passes the active catalog while technical
loader/parser errors remain untranslated payloads. Focused native UI tests pass 2/2; controller
tests pass 5/5 across both canonical formats, edit atomicity, stale-revision/save rejection, save
lifecycle, and Undo/Redo; localization model tests pass; and renderer remains 237/237. Other native
forms and retained live language-DLL evidence keep Localization Partial; aggregate parity remains
60/65.

## Toolbar customization localization parity

Twelve typed extension keys now cover the complete layout editor—window guidance, empty/default
state, movement tooltips, removal, button/separator creation, apply/default/cancel, and separator
rows. More importantly, all twelve action selectors and the complete typed label-key selector now
render through the active catalog instead of built-in English. Stable action IDs/slugs remain
untranslated persistence data. Focused toolbar tests pass 5/5, localization model tests pass, and
renderer remains 237/237. Other native forms and retained live language-DLL evidence keep
Localization Partial; aggregate parity remains 60/65.

## Restore-point localization parity

Thirty-one typed extension keys now cover automatic-point policy, archive/original/target
diagnostics, restore-table headings, record type and AM/PM presentation, atomic-replacement
warning, running state, completion summaries, and errors. The authenticated original
dialog-template title and controls remain preferred where available. Lunar Magic-compatible
persisted archive descriptions, restore identity filenames, technical errors, and record
descriptions remain deliberately untranslated. The focused suite passes 11/11 for associated-file
order, directory/readme publication, atomic replacement, failed-reversion markers, preferences,
full/delta/daily archives, original-template localization, and the visible/persisted-text boundary;
localization model tests pass and renderer remains 237/237. Other native forms and retained live
language-DLL evidence keep Localization Partial; aggregate parity remains 60/65.

## Level-usage analysis localization parity

Nine typed extension keys now cover output-path presentation, progress title/count/current level,
cancellation, completion summary, and error lifecycle. The authenticated original dialog `$0425`
continues to supply the title, Map16/graphics/sprite/music options, subordinate filters, and
OK/Cancel when mapped, with per-control English fallback. Technical scanner diagnostics and the
canonical `LevelAnalysis.txt` payload remain untranslated. Both focused localization/source-
boundary tests pass, localization model tests pass, and renderer remains 237/237. Other native
forms and retained live language-DLL evidence keep Localization Partial; aggregate parity remains
60/65.

## Graphics migration dialog localization parity

Five typed extension keys now cover logical-PC allocation guidance, start/end fields, and the
error lifecycle. Authenticated original dialog `$0416` continues to supply the title, codec
selector, all three codec choices, migration notice, transactional action, and Cancel on a
per-control basis. Allocation values and technical migration failures remain untranslated data.
Focused lifecycle, original-control, LZ2 Speed routing, and source-boundary tests pass 4/4;
localization model tests pass and renderer remains 237/237. Historical compression runtime
generations still keep Graphics Partial; other native forms and retained live language-DLL
evidence keep Localization Partial; aggregate parity remains 60/65.

## Keyboard-shortcut localization parity

Eight typed extension keys now cover the window guidance, portable-primary explanation, row
removal, creation, apply, clear-all, and cancel actions. All twelve action choices now share their
existing typed File/Edit/View keys with the active catalog instead of a parallel English table.
Canonical portable gesture tokens, modifier names, key names, and `+` framing remain untranslated
persistence syntax. Focused parser, ordering, duplicate-rejection, typed-action, and source-boundary
tests pass 5/5; localization model tests pass and renderer remains 237/237. Other native forms and
retained live language-DLL evidence keep Localization Partial; aggregate parity remains 60/65.

## Portable overworld-path localization parity

Thirty-eight typed extension keys now cover validation policy, Nodes/Edges navigation,
history/save state, every node and edge field, reciprocal/one-way mutation, direction choices,
create/remove actions, dirty-close, and errors. The seven submap choices reuse the existing typed
metadata vocabulary rather than creating parallel translations. Hex values, stable IDs, and
technical parser/controller errors remain data. Focused native form/source tests and the four
controller transaction tests pass, including reciprocal validation, failure atomicity, history,
canonical save, and stale-save rejection; localization model tests pass and renderer remains
237/237. Other native forms and retained live language-DLL evidence keep Localization Partial;
aggregate parity remains 60/65.

## External-tool lifecycle localization parity

Fourteen typed extension keys now cover permission, running-state, stop/deny/run actions,
direct-argument guidance, and completion status. Tool IDs, quoted arguments,
executable/working-directory paths, process IDs, and worker errors remain exact opaque data
substituted into localized templates. The active catalog now flows through the effect host. All 11
focused tests pass for bounded queueing, permission lifecycle, no-shell expansion, cancellation,
missing executables, multiple/focused instances, completion, and private emulator-workspace
cleanup; the source audit, localization model, and renderer 237/237 also pass. Other native forms
and retained live language-DLL evidence keep Localization Partial; aggregate parity remains 60/65.
Localization coverage update (2026-08-12, native level-stream document editor): forty-nine appended
`NativeLevelDocument*` identities now cover the complete document shell, source/framing summary,
object and sprite-token panels, semantic field forms, history/save state, close/error dialogs, and
the shared sprite-header form. The application supplies the live catalog to this editor, while the
shared header helper retains an explicit English-fallback wrapper for callers without localization
context; the MWL editor now supplies its catalog to that helper as well. Source audits require every
typed key and reject literal window, heading, label, button, and `Button` text. Localization passes
28/28 active cases (one provenance ignore), native-level document tests pass 17/17, and renderer
passes 237/237. Other native forms and retained live language-DLL evidence keep Localization
Partial; aggregate parity remains 60/65.

Localization infrastructure update (2026-08-12, native level-assets aggregate routing): twenty
appended `NativeAssets*` identities cover the portable aggregate shell, open configuration,
history/save state, close/error dialogs, and five-domain tab strip. The live catalog now reaches the
shared aggregate panel dispatcher from both the portable native-assets document and installed-ROM
level-assets editor, preventing divergent localization between the two consumers. Shell and tab
source audits pass, localization passes 28/28 active cases (one provenance ignore), native compile
passes, and renderer passes 237/237. The Level, Layer 2, Palette, ExAnimation, and Settings panel
captions still require conversion, so aggregate localization and the overall Localization row stay
Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, installed-ROM ExAnimation editor): eighteen appended
`RomExAnimation*` identities now cover the window, level/global target and switch state, dynamic
global-unavailable notice, commit/staged state, record mutations, dirty-close confirmation, and
error acknowledgement. The editor reuses existing typed ExAnimation field, record, frame,
allocation, stale-ROM, and reclamation vocabulary rather than creating duplicate translations.
The live catalog now flows through the application host, editor, clipboard controls, lifecycle
dialogs, and workspace target labels. Source audits require every ROM ExAnimation key and reject
literal window, heading, label, button, and frame-prefix text across the complete family.
Localization passes 28/28 active cases (one provenance ignore), the focused native source audits
pass 2/2, native compilation passes, and renderer remains 237/237. Remaining native forms and
retained live language-DLL evidence keep Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, installed-ROM expanded-settings editor): the final ten
Super GFX bypass slot captions now flow through the typed `RomExpandedSettingsGfxSlotFormat`
template instead of bypassing the active catalog as fixed `FG1`–`BG3` and `SP1`–`SP4` labels.
The editor's source audit now rejects every literal window, heading, label, button, small-text, and
`Button` caption in addition to requiring every `RomExpandedSettings*` key. The focused complete
surface audit passes, localization passes 28/28 active cases (one provenance ignore), and renderer
remains 237/237. Remaining native forms and retained live language-DLL evidence keep Localization
Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, installed-ROM level-assets shell and palette transfer):
twenty-six appended `RomNativeAssets*` identities now cover the installed aggregate window,
stale/busy/reserved-mode notices, Undo/Redo and modified state, allocation range, full `.lmpal`
and raw/TPL/RGB24 import/export controls and guidance, plus dirty-close and error dialogs. The live
catalog already reached the aggregate panels and now also reaches the palette-transfer and
lifecycle child surfaces. A family audit requires every new identity and rejects literal window,
heading, label, button, small-text, and `Button` captions in both child modules. The focused audit
passes, localization passes 28/28 active cases (one provenance ignore), native compilation passes,
and renderer remains 237/237. Preview controls, MWL/image transfer surfaces, commit controls, and
the Layer 2 mode-reset confirmation still need catalog wiring, so Localization and aggregate
parity remain Partial and 60/65 respectively.

Localization coverage update (2026-08-12, installed-ROM level-assets MWL transfers): ten appended
`RomNativeAssetsMwl*` identities cover complete and legacy single-level import/export, all/modified
batch export, the cancellable batch-progress window, dynamic output-template path, publication
notice, and cancelling state. The live catalog now flows through both MWL child modules, which join
the family key-coverage and fixed-widget source audit. The focused audit passes, localization
passes 28/28 active cases (one provenance ignore), and renderer remains 237/237. Image transfer,
preview, commit, and Layer 2 mode-reset controls remain before the installed aggregate family is
complete; Localization and aggregate parity remain Partial and 60/65 respectively.

Localization coverage update (2026-08-12, installed-ROM level-image exports): fourteen appended
`RomNativeAssetsImage*` identities cover the full-level image action, PNG/BMP batch actions,
expanded-area and automatic-screen filters, path/result/cancellation statuses, and the complete
cancellable progress dialog including selection, progress, staged-publication notice, and dynamic
format/path values. The live catalog now reaches the image worker, which joins the family
key-coverage and fixed-widget audit. The focused audit passes, localization passes 28/28 active
cases (one provenance ignore), and renderer remains 237/237. Preview, commit, and Layer 2
mode-reset controls remain before this installed aggregate family is complete; Localization and
aggregate parity remain Partial and 60/65 respectively.

Localization coverage update (2026-08-12, installed-ROM level-assets preview and commit): nineteen
appended `RomNativeAssets*` identities cover Super GFX validation, preview start/stop, camera axes,
reset, zoom-adjacent grid state, Map16 selection/clear/gesture help, ordinary and reclaiming commits,
aggregate staged state, and the complete Layer 2 storage-mode reset confirmation with dynamic mode
values. Long Map16/VRAM/sprite inspection rows remain exact technical diagnostics rather than UI
vocabulary. The complete key audit passes, localization passes 28/28 active cases (one provenance
ignore), native compilation passes, and renderer remains 237/237. Remaining localized batch
completion statuses and technical preview headings will be audited separately; Localization and
aggregate parity remain Partial and 60/65 respectively.

Localization coverage update (2026-08-12, installed-ROM level-assets final status audit): nine
appended `RomNativeAssets*` identities cover MWL completion/cancellation, legacy-import
compatibility prefixes, preview success/unresolved summaries, Map16 inspection heading/empty
state, and sprite-inspection heading/empty state. Parser diagnostics, acts-like chains, raw Map16
words, VRAM tiles, CGRAM rows, painter indices, and sprite token/part records remain exact opaque
technical evidence substituted after localized framing. The family key audit passes, localization
passes 28/28 active cases (one provenance ignore), native compilation passes, and renderer remains
237/237. This completes catalog routing for the installed-ROM level-assets editor family; retained
live original language-DLL behavior and other native forms keep Localization Partial and aggregate
parity at 60/65.

Localization coverage update (2026-08-12, installed-ROM overworld lifecycle and transfers):
eighteen appended `RomOverworld*` identities cover both installed editor window titles, the
profile-slot open form, playable-versus-complete dirty-close notices, error acknowledgement,
complete `.lmow` import/export, animation `.lmexan` import/export, and both transfer-scope notices.
The live catalog now reaches lifecycle and transfer child modules. Their source audit requires
every current `RomOverworld*` identity and rejects literal window, heading, label, button,
small-text, and `Button` captions. The focused audit passes, localization passes 28/28 active cases
(one provenance ignore), native compilation passes, and renderer remains 237/237. Terrain, route,
tile, custom-sprite, save-transition, and aggregate commit controls remain; Localization and
aggregate parity remain Partial and 60/65 respectively.

Localization coverage update (2026-08-12, playable overworld terrain and route links): thirty-two
appended `RomOverworld*` identities now cover stale state, playable-map description, allocation,
terrain commit/staged state, terrain-versus-route edit guards, complete route-link navigation and
field labels, all four localized direction choices, one-way behavior, canvas semantics, route
reload/apply/commit, and packed Layer 2 tile-word editing. The existing `PathEditorDirection*`
vocabulary is reused for choices rather than duplicated. The overworld family key audit passes,
localization passes 28/28 active cases (one provenance ignore), native compilation passes, and
renderer remains 237/237. Custom sprite, general tile/preview, save-transition, and aggregate commit
controls remain; Localization and aggregate parity remain Partial and 60/65 respectively.

Localization coverage update (2026-08-12, native custom overworld sprites): twenty-eight appended
`RomOverworld*` identities cover all four aggregate tabs plus the complete seven-map native-sprite
stream form, canvas selection/placement guidance, required extension-byte summary and fill action,
insert/replace/delete/reorder controls, dynamic map/count/selection status, and modal property
editing. The same typed field vocabulary is shared by the main form and property dialog. The
overworld family key audit passes, localization passes 28/28 active cases (one provenance ignore),
native compilation passes, and renderer remains 237/237. General tile/preview, save-transition,
and aggregate commit controls remain; Localization and aggregate parity remain Partial and 60/65
respectively.

Localization coverage update (2026-08-12, overworld save transition and aggregate commit): seven
appended `RomOverworld*` identities cover the staged-change transition prompt, all three decisions,
both ordinary and reclaiming aggregate commits, and the aggregate staged/clean state. The commit
range reuses the existing typed allocation and range-separator vocabulary. The overworld family
key audit passes, localization passes 28/28 active cases (one provenance ignore), native
compilation passes, and renderer remains 237/237. Shared tile/picker, preview-canvas, and animation
destination controls remain; Localization and aggregate parity remain Partial and 60/65
respectively.

Localization coverage update (2026-08-12, overworld shared canvas and pickers): twenty-three
appended `RomOverworld*` identities cover the direct 8x8 picker and palette row, Layer 1/Layer 2
selection, Map16 tile/page picker, unavailable-preview states, completed-event preview, all seven
canvas tools, and rendered ExAnimation destination navigation including dynamic owner tooltips.
The live catalog now reaches these controls in both playable and complete-overworld modes. The
family audit requires every identity and explicitly rejects reintroduction of the migrated fixed
widgets in the main module. The focused audit passes, localization passes 28/28 active cases (one
provenance ignore), native compilation passes, and renderer remains 237/237. Residual animation
unit/prefix vocabulary and non-widget diagnostics remain to be classified before Localization can
be promoted; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, overworld animation vocabulary): eight appended
`RomOverworldAnimation*` identities cover all four preview-rate choices, singular/plural substep
units, the trigger index prefix, and the manual-frame prefix. Rate labels now resolve through typed
keys instead of an embedded enum label, and the main-module regression audit rejects restoration of
the migrated constants. The focused audit passes, localization passes 28/28 active cases (one
provenance ignore), native compilation passes, and renderer remains 237/237. The retained live
language-DLL Wine fixture and a whole-frontend fixed-widget audit remain required before the
Localization row can pass; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, shared native preview lifecycle): two appended
`NativePreview*` identities cover the asynchronous preparing state and dynamic unavailable error
used by the graphics, palette, and Map16 native raster adapter. The application supplies its live
catalog at the shared render boundary, and a source audit rejects literal preview widgets while
requiring both typed identities. Native compilation, the focused audit, localization 28/28 active
cases (one provenance ignore), and renderer 237/237 pass. Other unaudited frontend modules and the
retained translated language-DLL gesture keep Localization Partial; aggregate parity remains
60/65.

Localization coverage update (2026-08-12, external-tool configuration): thirteen appended
`ExternalToolConfig*` identities cover SNES/GBA/tile-editor creation, removal, empty state, stable
ID and display name, argument guidance, working-directory template, and all automatic event
subscriptions. Original Lunar Magic dialog titles, executable/argument controls, platform options,
and Apply/Cancel continue to prefer recovered type-5 dialog translations, while these Rust-added
controls use the typed catalog. A complete surface audit rejects literal widget regressions and
requires every family key. Native compilation, the focused audit, localization 28/28 active cases
(one provenance ignore), and renderer 237/237 pass. Other unaudited frontend modules and the
retained translated language-DLL gesture keep Localization Partial; aggregate parity remains
60/65.

Localization coverage update (2026-08-12, overworld palette panel): eleven appended
`OverworldPalette*` identities cover dynamic color/BGR555 selection, animation and fixed/editable/
invalid ownership summaries, ExAnimation record attribution, color/row clipboard actions, and the
complete gesture notice. The live catalog now reaches the shared panel in both portable and
installed overworld editors. A complete surface audit rejects caption-bearing literal widgets,
requires every family identity, and explicitly permits only the two-space color-swatch glyph as
layout data. Native compilation, the focused audit, localization 28/28 active cases (one provenance
ignore), and renderer 237/237 pass. Other unaudited frontend modules and the retained translated
language-DLL gesture keep Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, overworld record panels): thirty-seven appended
`OverworldRecords*` identities cover all four shared tabs plus event-reveal editing and seam-aware
bulk relocation, endpoints, message tile/clipboard editing, sprite fields/extension bytes and
clipboard editing, every fixed-shape empty state, and all field/prefix/action guidance. The live
catalog reaches the shared panel from both portable and installed overworld editors. A complete
surface audit rejects literal button, label, small-text, slider, and prefix captions and requires
every family identity. Native compilation, the focused audit, localization 28/28 active cases (one
provenance ignore), and renderer 237/237 pass. Other unaudited frontend modules and the retained
translated language-DLL gesture keep Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, portable complete-overworld shell): twenty-one appended
`OverworldDocument*` identities cover document/open titles, maximum-record configuration, open,
Undo/Redo/Save and dirty state, tilemap heading/coordinate/Map16 editing, completed-event preview,
unavailable preview, dirty-close confirmation, and error acknowledgement. Existing installed-
overworld keys are reused for Layer 1/2 and the Records/Palette/Animation tabs. A two-module audit
requires every document identity and rejects literal window, button, label, heading, slider, and
button captions across the shell and tilemap module. Native compilation, the focused audit,
localization 28/28 active cases (one provenance ignore), and renderer 237/237 pass. Other unaudited
frontend modules and the retained translated language-DLL gesture keep Localization Partial;
aggregate parity remains 60/65.

Localization coverage update (2026-08-12, complete-level auxiliary panel): twenty-six appended
`LevelAux*` identities cover screen-exit, secondary-exit, and Map16-override tabs; all index
sliders; encoded/destination/position/screen/coordinate/flag and six Map16 fields; and the complete
append/replace/remove/upsert action family. The live catalog now routes from the application through
the portable complete-level shell and panel stack to this shared child without global state. A
complete panel audit rejects literal button, label, slider, and `Button` captions and requires
every family identity. Native compilation, the focused audit, localization 28/28 active cases (one
provenance ignore), and renderer 237/237 pass. Other unaudited frontend modules and the retained
translated language-DLL gesture keep Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, complete-level advanced panel): twenty-three appended
`LevelAdvanced*` identities cover expanded-header and Layer 3 tabs, recovered-default enablement,
all four Layer 3 selectors and dynamic graphics slots, reserved/tilemap/remap data, apply/disable
and clipboard actions, exact expanded-record enablement, Super GFX bypass, and dynamic raw-field
labels. The already-routed live catalog now reaches both advanced domains. A complete panel audit
rejects literal button, label, heading, slider, and `Button` captions and requires every family
identity. Native compilation, the focused audit, localization 28/28 active cases (one provenance
ignore), and renderer 237/237 pass. Other unaudited frontend modules and the retained translated
language-DLL gesture keep Localization Partial; aggregate parity remains 60/65.

Localization coverage update (2026-08-12, complete-level core panels): thirty-eight appended
`LevelCore*` identities cover the six parent tabs, Layer 1/2 object streams, object and sprite
record selectors and lossless byte forms, stream-header reporting, clipboard/sequence actions,
entrance kinds and fields, and every editable legacy-header property. The live catalog now reaches
the header, object, sprite, and entrance children through the existing complete-level panel stack,
with independent English fallback for every absent translation. A three-module source audit rejects
literal widget captions and requires every family identity. The focused audit passes, localization
passes 28/28 active cases (one provenance ignore), native compilation passes, and renderer remains
237/237. Other unaudited frontend modules and retained live language-DLL evidence keep Localization
Partial; aggregate parity remains 60/65.
