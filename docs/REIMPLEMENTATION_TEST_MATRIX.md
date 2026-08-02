# Lunar Magic compatibility and differential-test matrix

This document defines the behavioral boundary for a clean, cross-platform Rust reimplementation. The original executable is an oracle for observable formats and transformations, not a source architecture to reproduce.

## Test principles

- Never use the only copy of a ROM. Every test starts from an immutable fixture and writes to a temporary copy.
- Compare both semantic models and byte-level effects. A valid save may relocate tagged payloads, so compare decoded structures in addition to raw offsets.
- Assert the unchanged-region invariant: bytes outside the intended transaction, checksum fields, and intentionally relocated tagged allocations must remain identical.
- Preserve unknown tagged blocks, patches, and extension records unless the tested operation explicitly owns them.
- Test clean SMW, headered and unheadered images, LoROM, expanded LoROM/ExLoROM, SA-1 where supported, and ROMs modified by common ecosystem tools.
- Record the Lunar Magic version, input hash, operation script, output hash, changed ranges, decoded before/after models, warnings, and errors for every oracle fixture.

## Core matrix

| Subsystem | Read tests | Write/oracle tests | Failure and preservation tests |
|---|---|---|---|
| ROM identity and mapping | Detect copier header, explicit LoROM/FastROM/ExLoROM/SA-1 map mode, size, region, checksum, and supported expansion through the shared 32-MiB bounded ROM reader; require the mapper to represent the complete logical image | Expand and checksum as one undoable project mutation, exercise copy-on-write expansion through the built CLI and revision-checked expansion/undo/redo/Save As through the built application, compare address conversions, and validate both growing and write-only prepared mutations against their full target extent; add, replace, undo, and redo Lunar Magic 3.63's exact structured SMW-US copier header through the application/native route, create the same complete prefix through the built create-new CLI, and require Lunar Magic to preserve the production result | Reject oversized utility/mutation inputs before output, truncated images, HiROM/unknown modes, equal/shrinking/unaligned/oversized expansion, stale application requests, detected/declared mapper disagreement, mapper-incompatible tails even without detected project identity, ambiguous headers, aliases/collisions, unsupported canonical-header identities, unknown or ambiguous CLI header modes, bad exact-header lengths, and exact no-op forms without mutation/history/revision or replacing an open document; preserve unrelated copier headers exactly |

Cached identity checksum evidence is revision-coherent: tests require checksum-valid writes,
prepared commits, grouped payload saves, undo, and redo to refresh both stored and computed values.
Unqualified projects remain explicitly without identity rather than fabricating checksum metadata.
On macOS, the opt-in `snes9x_smoke` gates expand the supplied pristine fixture, insert/repoint a
standard Layer 1 object, install support patch B plus a forced `$ABC` custom-time command, update a
fixed-width standard sprite placement in place, and independently edit/repoint installed level
`$105` Layer 2 through the Rust project API. Another gate starts from
the pristine fixture, uses the snapshot-bound application controller to change a Map16 quadrant,
expands and installs all native split tables, and reopens the exact definition before launch. Every
path repairs the checksum, semantically reopens the generated state, launches only the temporary
output in Snes9x, requires the emulator process to survive initialization, and then terminates it
and removes the temporary copy. This is launch-time runtime evidence, not a substitute for
screenshot- or input-driven gameplay assertions.
The `lm-native` crate also has an opt-in `visual-smoke` feature. With
`LM_NATIVE_SCREENSHOT_TO` set, it waits eight rendered frames, requests a screenshot through the
native egui/Glow viewport, encodes the returned framebuffer through the bounded shared PNG writer,
and exits. A macOS run against the supplied pristine ROM exposed and then verified the fix for
level `$105`'s absent Layer 2 `$FF` pointer sentinel: the before image contained only a red
out-of-range error, while the after image contained the populated 92-object/34-sprite workspace and
decoded Map16 panel. A following framebuffer gate verifies the window-filling split workspace:
header, Layer 2, Map16, object, sprite, entrance, and commit tools scroll independently in a bounded
left column while the live canvas owns the majority of every supported window width and the
remaining editor height. These ROM-derived screenshots remain local rather than repository
fixtures.
The ROM level workspace also offers an overlay-free `Game pixels` presentation at native
16-pixel Map16 scale across the complete scrollable level, an optional entrance-framed 256×224
viewport, and a `Hide tools` control that gives the canvas the full editor width. Differential
inspection of SMW's
`BufferScrollingTiles_Layer2_Background` established that pristine shared backgrounds are two
row-major 16×27 screens rather than two column-major 27×16 regions. The corrected lossless
legacy/native codec, initial Layer 1/Layer 2 camera tables, and scroll-rate transform now assemble
level `$105`'s hills, clouds, bushes, ground, slope, and resolved sprites in the same geometry as
the Lunar Magic reference. The renderer also follows SMW's direct GFX33 pointer and recovered
`$7D00` animation-buffer layout to populate foreground VRAM slots `$60`–`$6B`, replacing static
placeholder pixels with the real question/music/turn-block artwork. Question and music blocks
advance through their four exact GFX33 frames on the editor's 125 ms presentation clock while the
ordinary turn block retains its static group. Focused codec, zero-gap, camera, animated-tile,
real-ROM, and native-framebuffer GUI tests cover the recovered invariants.
Standard and SSC sprite catalogs share the placed canvas's ordinary-versus-animated texture
routing: each subtile's `$0200` page bit selects the current GFX33 phase, while page-zero subtiles
remain on the ordinary SP atlas. Focused page classification covers both domains.
The standard catalog also shares the canvas's handler-scoped half-opacity rules for `$E1/$1B8`
and `$90/$1C0-$1F3`; SSC catalog and placed-canvas parts remain opaque under focused tint-domain
coverage even when their custom sprite number and definition index match those standard scopes.
Its dispatcher context additionally carries the form's exact native major and minor axes rather
than only the packed coordinate nibbles, with focused coverage at screen `$1E`, X `$F`, and minor
`$11` for coordinate-sensitive authenticated handlers. The same test distinguishes the handler's
`$1F` minor/major nibble argument from the unrelated serialized `yyyyEESY` byte.
Resolved SSC display offsets also determine placed selection and hit bounds before external
textures are available; focused negative/extended-offset coverage proves unresolved composite
geometry does not collapse to the encoded marker.
The installed native-assets preview reuses the same recovered common-animation interpreter after
resolving either Super GFX bypass files or legacy tileset assignments. Its staged
`VanillaAnimation` and `PaletteAnimation` options independently gate the recovered eight-phase tile
cache and Dragon Coin color cycle. Either domain drives the shared 60 ms repaint clock; disabling
both stops it. A retained pristine-ROM test requires adjacent materialized tile phases to differ,
while focused palette coverage requires every color except CGRAM `$64` to remain exact. The same
retained gate mirrors the authentic GFX32/GFX33 pointers into profiled files `$32/$33` and requires
that installed-table and pristine-pointer source resolution decode identical tiles.
The installed preview's final presentation passes through the shared exact viewport rasterizer:
camera origins are clamped against its visible-world span, 50–400% zoom uses rational
nearest-neighbor sampling, and a 512×448 200% integration fixture requires exact repeated pixels
after a nonzero camera crop. State tests additionally require a center anchor to retain the same
world pixel across a 100%→200% zoom, direct image drags to convert total displacement exactly at
200% and 50%, and extreme displacements to clamp at the world edge. Modified-wheel coverage
requires Ctrl, platform Command, and raw Mac Command to request a step while unmodified and
Alt-modified input remains page scroll; it also fixes finite/boundary handling, clamps the pointer
to the raster, and preserves a noncentral world anchor. Optional Map16-grid integration derives its
screen origin and 16-pixel world spacing through that same viewport before invoking the shared
overlay compositor. Odd-origin fixtures at 200% and 50% require exact X/Y residues, intersections,
and unmodified pixels when the grid is disabled. Click-selection coverage maps a cropped 200%
screen coordinate back to its exact Map16 cell, transforms the cell into half-open screen bounds,
checks both selection edges and the first outside pixel, and requires separated phases to animate
the marching outline without enabling disabled tile or palette animation. Selected-cell inspection
fixtures require two same-cell Layer 2 placements followed by two Layer 1 placements, retain outer
flip bits and the complete installed definitions, resolve tilemap Layer 2 against a conflicting
background definition bank without inventing Acts-Like data, combine an installed active-bank
descriptor with the stored low 12 bits while retaining the raw word, expose an unavailable definition
rather than dropping it, and distinguish an empty sparse cell. Foreground Acts-like observations use the installed tile
count as the resolution bound and require a three-node chain ending in a self-link, a direct
self-link, a two-node cycle, and out-of-range targets to remain typed failures. Sprite provenance
is retained through materialization as original token index/part ordinal, ID, source class,
definition, origin, and four tile words. Half-open overlap fixtures keep three parts in original
paint order while excluding parts that only touch the selected cell at its left/right/bottom
boundary. Object-backed Layer 2 with object tileset 3 routes encoded Map16 palette rows 0–3 through
CGRAM rows 4–7 while preserving rows 4–7, tilemap Layer 2, and Layer 1; focused pixels and the
selected-cell inspector share the same typed per-layer routing rule. Compact global/level
object-render fixtures also require an empty stream to produce no placements, retain duplicate
same-cell writes across serialized objects in painter order, and preserve only each object's final
value for a cache index rather than exposing internal construction rewrites.
Native-raster fixtures require averaged Map16 paints to match Lunar Magic's per-channel low-bit
clearing and floor behavior while leaving transparent pixels untouched. Installed fixtures scope
that composition to tiles `$027`–`$02A` in object tileset 4 for both object and tilemap placement
sources, retain opaque behavior outside the range/family, require foreground `$4027` not to alias
`$0027`, preserve the rule for flipped compressed-Layer-2 `$C02A`, and expose the chosen mode in
inspection.
Separate half-color fixtures require source-only RGB halving independent of the destination,
transparent-pixel preservation, exact selection by recovered level modes `$0C`/`$0D`, rejection
for adjacent modes, and precedence over the underground average rule on Layer 2 backgrounds.
An independent raster fixture gives the same tile index conflicting foreground and background
subtiles and requires a background-tagged placement above `$0FFF` to use the latter.
Another fixture places conflicting foreground definitions at `$0001` and `$4001` and requires an
object-backed `$4001` paint to select the high definition without an implicit outer flip.
ExAnimation remains provider-gated rather than being inferred from unrecovered transfer kinds.
| Copier-header conversion | Detect absent/present 512-byte file prefixes independently from logical ROM offsets and compare-replace exact optional prefixes | Add caller-filled headers and remove them through model, CLI, application specification, built-process, revision-checked open-project, and native-dialog workflows while preserving every logical byte; require header-only dirty/save state, byte-exact restoration of nonuniform removed headers, and guarded one-step undo/redo | Reject no-op states, stale dialogs/revisions, pending saves, mismatched history bytes, invalid exact-prefix shapes, oversized headered results, malformed specifications, aliases, and existing outputs without changing project/source state or publishing partial files |
| IPS patching | Parse exact `PATCH`/record/`EOF` framing, raw and RLE records, ordered overlaps, optional 24-bit truncate metadata, bounded application specifications, native apply previews, and native original/modified/output creation | Deterministically create and reapply patches for equal-size edits, zero-filled bank-aligned growth, bank-aligned shrinkage, RLE runs, and changes at the reserved `EOF` offset; exercise create-new CLI/application-shell workflows, background native creation, copier-header normalization, and revision-checked open-project application; require exact apply-time copier-header preservation, one-step undo/redo, checksum-evidence synchronization, and revision-profile invalidation after a successful arbitrary patch | Reject wrong magic, truncation, zero-length RLE, malformed trailers/specifications, 24-bit offset/result overflow, oversized images/patches, canonical input/output aliases, stale revisions, stable-identity changes, partial-bank results, and no-op commits without mutation or partial publication |
| Lunar restore points | Decode the exact `$130`-byte `LR` archive prefix and linked `$100`-byte `DIRL` records with 64-bit reciprocal links, latest ID/sequence, exact record extents, record IDs, packed timestamps, ROM sizes, descriptions, payload bounds, raw-DEFLATE flag, stored-record and decoded-command byte-sum checksums, logical-ROM CRC-32, thirteen header timestamps, and thirteen record sidecar offset/size pairs in the Ghidra-confirmed `msc`, `dsc`, `ssc`, `m16`, `s16`, `mwt`, `mw2`, `sscov`, `s16ov`, `lmtbl`, `mw0t`, `mw0`, `osc` order; decode and encode compact raw commands with 24/32-bit destinations and one- through four-byte lengths | Reconstruct a selected record by applying the complete linked delta chain to the original ROM and resizing to every record's declared ROM size; inherit unchanged sidecar slots and replace changed slots using their owning record's compression flag and the pre-3.21 large-file end-offset rule; create native one-record full archives with caller-controlled metadata, compression, producer, ROM delta, and sidecars; synthetic compressed/uncompressed creation round-trips, the authenticated Lunar Magic 3.63 archive's two record extents reach the next record and exact file end, record 2 expands to 525 commands and reproduces the 2,097,664-byte target ROM exactly, and the native closed-project workflow lists all recovered columns then publishes the chosen ROM and optional associated files off-thread as one rollback-safe mixed create/replace group | Reject short/bad headers, invalid signatures, record cycles, broken previous links, mismatched last links or record extents, overlapping/out-of-bounds headers, descriptions, payloads, or sidecars, embedded-NUL creation descriptions, failed deflation/inflation/stored checksum/decoded checksum/ROM hash, oversized inflated sidecars, malformed/truncated/unterminated commands, trailing decoded bytes, arithmetic overflow, legacy sidecar address underflow, unknown record IDs, aliased archive/original/target/publication paths, symlink/non-file publication targets, missing ROM replacement targets, and outputs above the configured bounds |
| SNES/PC addressing | Golden vectors at bank edges and mapped/unmapped regions | Round-trip every valid address class | Reject overflow, WRAM, hardware, and unmapped holes |
| LZ2, LZ3, and related codecs | Decode LZ2 literals, fills, incrementing runs and all back-reference directions; decode the separately recovered LZ3 zero fill, short-relative/absolute operands, bit-reversed copy, and both reverse commands; distinguish sized/terminated RLE packets | Compare compressed output through canonical decoded-hash/re-encoding observations independent of physical command choice; require deterministic LZ2/LZ3/RLE encode/decode equivalence and bounds, including LZ3 relative-source selection beyond the absolute `0x7fff` ceiling; exercise bounded 16-MiB create-new transforms and the observer through the built CLI; prevent terminated RLE data packets from colliding with `FF FF` while retaining that packet in sized containers | Truncation, invalid commands and absolute/relative references, overlap behavior, input/output limits, oversized sized-RLE declarations, missing terminators, trailing data, existing/hard-link output aliases, observation aliases/collisions, and 128/129/255/256/257-byte `FF` run boundaries |
| RATS/tagged allocation | Enumerate valid blocks and payload spans; decode/normalize bounded deterministic `LMRATS01` ownership artifacts | Validate allocation authority before deduplication; allocate, reuse identical unprotected same-bank data wholly inside the authorized search range, replace, erase, expand, repair ranges, dry-run/commit explicitly owned garbage collection as one undoable operation, and atomically publish header-addressable ownership observations beside normalized manifests; copy-on-write CLI repair verifies the checksum and retained/reclaimed ranges; the revision-checked application and bounded graphical workflow preview exact counts/bytes, preserve retained tags, apply the selected fill, repair checksum, and undo to exact source bytes | False tags, nested tags, bad complements, invalid search/bank/fill/protected policies even when duplicates exist, out-of-search and cross-bank tags during reuse, cross-bank replacement/erase, any complete internal-header/vector overlap, malformed/truncated/trailing manifests, stale/duplicate/overlapping/forged ownership proofs, foreign retained blocks, stale graphical workspaces, output aliases/collisions, and rollback without ROM/history/revision mutation; an all-retained manifest is a no-op |
| Relocatable runtime patches | Canonically decode bounded identity-bound `LMPAT001` templates with exact hook preconditions, tagged payload bodies, and bounded 24-bit cross-payload/hook fixups; decode bounded relative-path `LMRPINS1` application specifications; construct independently authored 65C816 fragments with checked typed labels, relative branches, explicit instructions, and relocation records | Allocate mutually referring payloads in two passes, apply SNES-address fixups after placement, optionally grow through the selected mapper, repair checksum, commit the entire installation as one undoable project revision, group independently allocated plans without weakening their placement policies, publish through create-new CLI output, exercise revision-checked application undo/redo/save integration, expose all three recovered built-in families and bounded external templates through revision-bound graphical installers, and reproduce the complete recovered SMW-US-v1 Layer 3, expanded-settings, and expanded shared/custom-palette installations; graphical tests route every built-in selection through the application, reopen the installed subsystem, undo to exact source bytes, decode only a single canonical external template, bind it to the captured profile/revision, and retain its workspace until acknowledgement | Reject wrong template game/region/revision/mapper before graphical workspace creation, truncation/trailing bytes, excessive counts/bytes, ambiguous loader groups, empty payloads/writes, unequal hook shapes, unexpected original bytes, overlapping writes/fixups, missing or out-of-payload targets, duplicate/unbound/far code labels, incomplete analysis fragments promoted as templates, malformed specifications/ranges/fills, stale application revisions and installer workspaces, pending saves, disallowed growth fill, invalid policies/mappings/checksum fields, output aliases, and late single/group-plan failures without ROM, history, revision, or length mutation |
| Level headers and entrances | Decode all 512 level slots, modes, scroll settings, music, entrances, secondary exits, and exact 32-byte installed expanded-settings records | Change one field at a time and compare decoded output; atomically script ordered entrance, screen-exit, secondary-exit, and Map16-override edits in complete portable bundles; profile-driven expanded-settings export/import repairs checksums and semantically reopens; exact records normalize with raw-byte and sixteen-word oracle observations; exercise standalone edit/undo/redo/save/close through the built application; require Lunar Magic 3.63 to export Rust-written main-entrance planes identically from both headerless and canonical-header ROMs while observing its exact headerless-to-canonical physical transition | Preserve unknown bits and unrelated level tables; reject late malformed scripts, invalid indexes, duplicate/missing stable keys, stale standalone history tokens, aliases, noncanonical output, partial observation publication, and expanded-settings layouts overlapping pointer tables/internal headers before publication |
| Level objects | Parse standard, extended, screen-exit, custom/extension commands, and both command-zero screen-jump encodings; expose the recovered distributed six-bit command ID, command-specific parameter, orientation-neutral coordinate nibbles, screen-advance bit, and packed jump target while retaining extension bytes | Insert, delete, reorder, move, bit-preserving typed field/jump-target mutation in pristine, MWL, and interpretation-bound custom-record editors, enumerate `$01`–`$3F` through a hex-filterable active-tileset visual picker with authenticated-footprint or explicit labeled fallback cells, enumerate active-variant OSC displays by hexadecimal pair or description and construct their command-derived native extension shape, navigate a fixed-scale two-axis ROM canvas, visually insert ordinary objects at an absolute tile, and drag them across all 32 native screens on either orientation, stably reorder by absolute screen, regenerate minimal advance/jump transitions, preserve extensions and trailing opaque controls, copy/paste, serialize boundary-sized streams, canonically normalize/observe interpretation-bound `LMLVL1` transfer files, and prove cross-screen relocation by reciprocal Lunar Magic 3.63 MWL import/export | Bank limit, terminator handling/collisions, command-zero visual insertions, invalid command IDs/coordinate nibbles/jump targets/lengths, off-canvas/minor-overflow/out-of-range-screen drops, interleaved unknown command-zero controls, typed edits that change record shape or reinterpret control objects, confusing OSC compact metadata length with native stream width, output aliases/collisions, wrong sprite-length interpretation, and rollback |

Canvas object artwork now dispatches each already-resolved placement in native stream order instead
of rendering all standard objects before all OSC displays. The single-placement standard renderer
retains absolute screen coordinates without reconstructing control records; an explicit OSC display
suppresses built-in artwork even when its part list is empty, while description-only metadata
retains the authenticated standard fallback. A real-ROM test constructs a two-part OSC object,
inserts its command-specific native record into level `$105`, commits and reopens the exact record,
and resolves the same display after reopen.
OSC parts below `$200` use the base Map16 atlas, while higher parts resolve only
against an available M16 custom-Map16 definition. Missing definitions, S16-only
metadata, out-of-range indexes, and unavailable foreground assets now produce a
bounded red marker labeled with the unresolved tile ID instead of silently
disappearing. The M16 lookup retains the complete 15-bit OSC foreground identity, so
out-of-range `$4001` cannot alias sidecar entry `$0001`; focused tests distinguish
base, resolved-custom, and unresolved cases.
Native-canvas selection and drag hit-testing unions the encoded placement footprint
with every resolved OSC display tile, including negative and extended offsets, while
an explicitly empty display retains the encoded target. Focused geometry coverage and
the real-ROM level-`$105` custom-object commit/reopen test verify those interactive bounds.
Compressed Layer 2 presentation retains both whole-cell flip bits in every main-editor route:
precomposed shared-background pixels, ordinary atlas UVs, and M16 visual quadrants all cover the
four X/Y combinations, with M16 outer flags XOR-composed into the selected subtiles.
Standard-object cache paints, visual-catalog cells, and OSC display parts share one bounded source
classification: vanilla `$0000-$01ff`, M16-backed `$0200-$03ff`, and explicit unresolved higher or
unavailable identities. This prevents both standard-object presentations from addressing beyond
the vanilla atlas or disagreeing about loaded M16 artwork.
OSC visual-catalog and placed-canvas parts likewise retain the same four-digit unresolved identity
when their M16 definition or required texture is unavailable, rather than presenting a blank
catalog preview for a visibly unresolved placement.
Tileset-dispatched standard objects likewise retain the bounds of every nonempty Map16
cell from the exact cache already painted for that placement, avoiding a second render
pass while making extended authenticated artwork selectable. A pristine-ROM sweep across
all 512 level slots requires a recovered object whose artwork exceeds its generic packed
parameter span and proves the complete encoded footprint remains interactive.
The renderer exposes the active family handler's authenticated resize encoding as a
typed model: two parameter nibbles, major-only nibble, one- or two-tile fixed-major
minor nibble, three-tile fixed-major full byte, independent command-`$27` mode-`$C0`
seven-bit horizontal/vertical fields, or fixed size. Native Layer 1 and
object-backed Layer 2 forms edit tile counts through that model, preserve ignored
parameter bits, reject every out-of-domain axis, and suppress standard resize controls
for OSC-selected custom records. Selected nonfixed standard objects also expose an
orientation-aware canvas handle: dragging its authenticated axis updates the same
lossless parameter field for Layer 1 or object-backed Layer 2, while fixed and OSC
custom objects expose no misleading handle. Handle geometry uses the handler's fixed
axis rather than ignored parameter bits and retains the complete 1–256-tile full-byte
minor range. Geometry tests cover horizontal and vertical axes, fixed-bit preservation,
movement before the origin on an unowned axis, invalid owned-axis/out-of-canvas drags,
and handle placement. A pristine-ROM fixture expands once, resizes a discovered
nonfixed object, commits and semantically reopens it, then undoes to the exact expanded
baseline.
The extended command-`$27` form is oracle-bound to Lunar Magic 3.63 functions at
`0043C2B0`, `0043A000`, and `00435650`; focused tests cover seven- and eight-byte
records, 1/128 bounds, atomic rejection, typed model selection, form conversion, and
one lossless replacement retaining every unrelated extension byte. A supplied pristine-ROM
fixture inserts the eight-byte form, edits both sizes through the native form, commits,
resizes it again through the canvas, semantically reopens the exact record, and undoes to
the expanded byte baseline. Live gesture tracing through `004880A0` and `0043C560` proves
the fields remain physical X/Y in vertical modes; canvas tests cover both orientations,
bottom-right handle geometry, bounded extents, 1/128 rejection, and preservation of the
optional eighth byte and unrelated extensions.
Layer 1 and object-backed Layer 2 additionally expose a lossless hexadecimal native-record editor
for every three- through eight-byte object shape. The editor derives the required length from the
encoded command and mode before replacement, so malformed `$22`, `$23`, `$27`, `$29`, and `$2D`
records cannot desynchronize the following stream. Focused tests round-trip every native length
class and reject short, long, and nonhexadecimal input without mutation; the supplied pristine-ROM
command-`$27` fixture applies raw extension-byte changes before semantic resize, commits, reopens
the exact eight bytes, and undoes to the expanded baseline.

Profile-qualified native level assets additionally treat Layer 2 as a mode-selected fifth payload.
The automatically detected pristine/installed SMW-US level editor also binds Layer 2 to its primary
level controller instead of retaining a display-only copy: compressed tiles can be selected or
painted on the main canvas by exact 16-bit Map16 word, while object-backed levels expose their
ordered native record list, lossless field/insert/remove operations, direct canvas insertion,
selection, cross-screen dragging in either orientation, stable move-up/move-down ordering, and the
same typed native-object copy/paste framing as Layer 1. Selected custom-width Layer 2 object
templates retain every extension byte during list, canvas, or clipboard duplication; malformed and
cross-domain clipboard text fails before mutation. Both storage classes share Layer 1/sprite undo
and redo and commit through one revision-checked mutation. The pristine-ROM native-GUI fixture
combines canvas insertion and relocation with typed paste and ordered movement, commits the complete
object-backed Layer 2 stream, semantically reopens it, and undoes to the exact expanded baseline.
The save path reopens all changed level streams semantically; format-$103 saves persist the
normalized descriptor beside the relocated tilemap and checksum.
Object-backed levels expose the same lossless ordered record operations and native clipboard
framing; compressed-tilemap levels expose all 1,024 little-endian words through the recovered
32×32 canvas order. The native panel supports ordinary single-cell selection and Shift-extended
rectangles; one fill emits a duplicate-free ordered edit batch and commits the whole rectangle
atomically through the aggregate controller. A dedicated typed clipboard record preserves the
rectangle's width, height, complete 16-bit words, and visual row-major order independently from
the native two-plane storage order. Copy/paste rejects malformed shapes, cross-domain payloads,
and destinations crossing the 32×32 edge before producing an edit batch. Cut first publishes the
same typed payload and then atomically writes Lunar Magic's recovered `$0000` deletion word to
every selected cell.
Flood fill compares complete 16-bit source words over a bounded four-connected region and
emits a single aggregate edit using the recovered 12-bit Map16 replacement normalization. The
native panel supports both a single replacement word and a retained visual row-major rectangle;
the latter repeats from the region's independent minimum X/Y bounds exactly as the recovered
Lunar Magic routine does.
Selected rectangles can also move one cell in any cardinal direction. Movement snapshots all
source words before clearing, gives overlapping destination cells precedence, preserves reversed
anchor/cursor orientation, and commits the duplicate-free source/destination diff as one aggregate
edit. Invalid sources and any destination crossing the 32×32 edge fail before controller mutation.
All four rectangle edges can independently grow or shrink by one cell. Resize restores/clears the
old source, repeats its captured visual pattern from the resized top-left corner with 12-bit
normalization, preserves reversed endpoints, and rejects inversion below 1×1 or any crossed edge
before emitting one aggregate edit.
An exhaustive independent reference test covers all 512 binary 3×3 topologies from every
start cell, including edge isolation, irregular pattern anchoring, malformed dimensions, and the
native two-plane storage bijection. Focused gates require
five-payload semantic reopen, checksum validity, exact ownership reclamation, and rollback when a
late edit targets the wrong storage mode, repeats a tile index, or exceeds the tilemap.
The recovered background-remap gate additionally parses the bounded `$8000`–`$FFFF` textual
language into a 32,768-entry transformation table and covers absolute/ranged replacement,
saturating addition/subtraction, `M` linear and two-dimensional generation, `R` source rectangles,
global offset, selection versus whole-map application, 12-bit storage normalization, and active
bank transitions. It rejects malformed pairs, out-of-domain values, oversized scripts, invalid or
duplicate selection indexes, malformed tilemaps, invalid banks, and excessive offsets before
emitting edits. The native aggregate controller and GUI apply bank-0 whole-map or selected-rectangle
programs as one revision-bound edit and require semantic ROM reopen. Cross-bank results are rejected
before mutation on pristine/legacy layouts. Format-$103 profiles load and atomically persist the
recovered one-byte descriptor table, so installed-ROM cross-bank remaps update the tilemap,
descriptor, checksum, history, semantic reopen, and exact undo as one transaction. Typed MWL access
losslessly retains both metadata words, exposes descriptor storage bits and active bank, and tests
the exact native post-remap normalization independently from the lossless bank-only setter.
The opt-in Wine gates additionally relocate an edited installed level-105 tilemap into a newly
expanded bank and discover an object-backed level whose first ordinary object's coordinate is
changed through the lossless typed field API. Each path performs one checksum-atomic Rust save and
requires Lunar Magic 3.63 to re-export the exact decoded Layer 2 payload.
| Layer 3 | Decode bounded `LMLAY3V1` settings, four 12-bit graphics IDs, tilemap workspace, reserved bytes, and remap commands; read legacy `LMLEVEL2` v1 bundles; decode the verified installed-record `$2000` enable bit and packed 12-bit file/two-bit length/two-bit destination descriptor; independently apply/extract every recovered selector range in the exact `$2000`-byte decoded workspace; decode/encode and generically execute the recovered legacy graphics-remap command stream | Canonical standalone and embedded v2 round trips, failure-atomic settings/full-buffer/range edits, all sixteen descriptor selector combinations with clipping and outside-range preservation, exact remap literals/repeats/strides/wrapping/odd-byte writes/termination, revisioned application-document open/edit/undo/redo/save/close workflows through the built binary, saved-baseline restoration and divergent redo invalidation, field-complete standalone and embedded oracle observations, atomic CLI normalization/observation publication, workspace and remap-scratch before/after digest observations, application-mode navigation, distinct lossless tilemap/remap clipboard round trips, semantic standalone/MWL expanded-record edits, bounded `LMMWLL31` application-shell transactions with undo/redo/save, native-GUI edits, and exact Lunar Magic Wine import/re-export observations | Reject every truncation, trailing data where full artifacts require exhaustion, wrong workspace/scratch length, short decoded graphics range, unrepresentable remap command fields or premature stream-limit endings, oversized tilemaps/remap streams, invalid graphics IDs, standalone enable/disable transitions, bad slots/ranges, stale history tokens, malformed scripts/specifications/clipboard records, missing MWL expanded sections, dirty close, output aliases/collisions, failed grouped publication, and cross-editor clipboard kinds; preserve noncanonical remap terminators, selector aliases, unrelated flag bits, all bytes outside the selected workspace range, all opaque expanded-record words, reserved bytes, remap bytes, and unrelated MWL sections |
| Sprites | Parse legacy/current `yyyyEESY / XXXXssss / NNNNNNNN` records, expanded upper-Y/control tokens, escaped `FF` records, and exact-bounded revision-table-sized extra bytes | Insert, move, reorder, delete, losslessly edit X/Y/screen/extra-bit/number fields in pristine, MWL, and interpretation-bound custom-record editors, derive direct-ROM four-table framing authority from SSC selectors, enumerate the authenticated `$00`–`$ED` standard dispatch table plus deduplicated description-searchable SSC defaults in context-aware visual pickers, materialize exact declared custom widths, keep both native sprite rows visible on fixed-scale scrollable horizontal and vertical canvases, visually insert and drag placements, stably restore Lunar Magic's cross-screen legacy ordering without changing within-screen priority, compatibility conversion, checked native/project/interchange serialization, semantic reopen, direct-ROM Wine relocation/edit/export equivalence, and target-addressable observations of native record/upper-Y/control tokens | Per-level limits, conflicting SSC widths for one number/table, unknown custom sprite records, extension preservation, short public records, legacy control/terminator collisions, visual insertion of control tokens, expanded/control stream position sorting, out-of-range upper-Y tokens and canvas drops, custom length-table shape changes, reserved `FE`/`FF` controls, malformed/oversized length tables, and bytes hidden after inner stream terminators; reject before ROM/history mutation |

Sprite preview gates additionally require all eight encoded vanilla sprite palette rows, flip-bit
preservation, source-ordered SSC global graphics-base and palette-source ranges, exact target
biases for all four `$10000` modes, and explicit retention of external-page/custom-palette
requirements. A vanilla-only atlas must reject those external sources rather than silently render
the unremapped definition. The complete 256-entry native dispatch domain is classified
exhaustively: `$29`, `$30`, `$EE`, `$F0`, and `$F1` remain intentional artwork-free native
handlers, `$F6`–`$FF` require custom-display data, and every other ID must produce recovered
built-in artwork. Native canvases must not fabricate red artwork for intentional empty handlers,
but must retain an explicit unresolved marker when custom-display data is absent or unusable.
Standard and SSC sprite-part offsets are interpreted in native 16-pixel units and scaled to the
active canvas cell size; selection outlines, pointer hit-testing, and drag starts use the union of
the placement marker and every rendered part for ordinary-atlas and external-texture displays.
Focused geometry coverage plus the pristine level-`$105` relocation/reopen fixture prove a
composite sprite retains expanded interactive bounds after a native ROM save.
| Map16 | Decode definitions, Acts Like links, pages, selections, exact 0x2000-byte `.m16`, zero-filled 0x1C000-capacity `.s16`, and 256×256 indexed bitmap pages | Edit subtiles/attributes and raw sidecar dwords, import/export pages and complete sets, canonically block-trim `.s16` through its last nonzero dword, assemble four imported 8×8 placements per Map16 tile, publish graphics/occupancy/page artifacts together, atomically normalize/observe source-identified page and native-sidecar files, and own revisioned complete-set plus native-sidecar application documents with current-memory previews where applicable | Acts Like cycles, invalid palette rows, locked pages, wrong `.m16` length, oversized `.s16`, invalid raw-entry indexes, malformed sidecars/specifications/scripts, stale document revisions/saves, dirty close/EOF, aliased/existing outputs, and unknown section retention |
| Graphics | Decode/encode recovered SNES 1-through-8-bpp planar tiles with interleaved pairs and contiguous odd final planes, joined/separate 4bpp GFX files, revision-selected LZ2/LZ3 native payloads, and exact per-tile `LMGFXOWN` ownership evidence | Round-trip every planar depth through reusable APIs and built CLI conversion, decode legacy v1 ownership and canonically normalize v2 generic/original/level/global animation provenance, entry-addressably observe ownership through the built CLI, edit only explicitly editable ROM tiles while retaining protected tiles for preview/copy, extract/insert GFX and ExGFX, select compression through profiles or explicit CLI policy, atomically migrate a complete native pointer table between codecs with one undo step, copy-on-write CLI publication, and a typed revision-checked application command whose effective profile codec follows mixed undo/redo history, transactionally allocate/repoint/checksum/reopen native data under separate allocation authority, reuse exact/horizontal/vertical/dual-flip bitmap tiles, allocate unmatched tiles in stable order, own revisioned portable graphics documents with current-memory tile-sheet previews, and atomically normalize/observe portable files | Invalid depths, partial planar/indexed tiles, malformed/truncated/trailing ownership, unknown/noncanonical owner records, out-of-range animation slots, ownership output aliases/collisions and grouped-publication failure, depth-specific pixel overflow, wrong dimensions/counts, non-4bpp pixels, unknown/equal compression policies, ownership/stale-revision/profile-layout shape mismatch, missing palette rows/files, compressed/decompressed or protected/free-slot exhaustion, dirty close/EOF, input/output aliases, create-new output collision, 10-bit tile limit, stale migration requests, and late multi-slot failure without partial allocation, history, profile metadata, or application revision |
| Palettes | Decode shared/custom palettes, exact per-color `LMPALOWN` ownership evidence, both recovered complete `.smwpal` backend layouts (`0x7e2` legacy and `0x800+0x10` expanded), exact 256-color version-2 TPL, exact 257-word raw SNES palettes, lossless 257-byte `.palmask` selectors, and exact 256-color RGB24 `.pal` with recovered expansion detection; build a 33³ cumulative RGB histogram | Edit colors/rows, permit ROM-backed mutation only for explicitly editable ownership records and a separate allocation range, convert RGB/BGR555 using high-bit or nearest bit-replicated conventions, failure-atomically apply selected raw entries and clear selected row-zero colors, losslessly normalize and color/selector-addressably observe native palette formats through the built CLI, variance-split RGB24/RGBA32/PNG into deterministic SNES palettes/index planes, reserve index zero for transparent alpha, install generated colors 1–15 into one owned row, atomically publish complete bitmap-to-Map16 artifacts, own revisioned portable palette documents with current-memory swatch previews, and atomically normalize/observe portable files | Wrong/truncated/trailing ownership/native backend/raw/mask/RGB lengths, unknown ownership kinds, noncanonical ownership payloads, TPL magic/version/color-count errors, malformed source/destination/mask shapes, fixed/reserved/animation-owned colors, ownership/stale-revision mismatch, dirty close/EOF, empty or excessive grids, fractional alpha, wrong PNG dimensions/color framing, zero or over-256 quantization bounds, over-16-Mi-pixel inputs, partial pixels, aliases/existing outputs, duplicate rounded colors, uniform/all-transparent images, and unknown palette extensions |

The global native shared/custom-palette command is also reachable through a dedicated graphical
workspace without a revision profile. It pages across every legacy or expanded backend color,
supports canonical BGR555 and RGB editing, preserves every unselected byte, and exposes the exact
16-byte expanded auxiliary region. Pristine color edit/commit/reopen/undo and expanded auxiliary
shape/stale/dirty-close tests bind that GUI to the transactional ROM command.
| ExAnimation | Decode legacy and expanded global/per-level records, distinguishing lossless fixed-workspace bytes from compact-native fields | Atomically insert/replace/delete/reorder size-table-driven source-word frames through native and revisioned portable-document controllers and bounded CLI scripts, including shared MWL aggregate commands used by both the application shell and native panel; clear stale payload tails after shrinking, edit record collections, migrate formats, save global/local sets, atomically normalize/observe interpretation-bound portable files, canonically normalize/observe provider-resolved `LMANFRM` ticks by absolute tile/palette target, require exact relocation-neutral record/frame/source-word observations after Rust semantic MWL edits are imported and re-exported by Lunar Magic under Wine, and have Lunar Magic export the exact Rust-written per-level animation-feature byte including its unrelated low nibble | Invalid script syntax/UTF-8/limits, aliases/collisions, stale revisions/snapshots, dirty close/EOF, wrong size-mode count, invalid frame/type combinations and indexes, single/double payload capacity, lazy-load state, rollback, fixed-record unknown-byte preservation, disabled-trigger values, malformed materialized-frame pixels/colors/reserved fields/duplicates, and workspace bytes omitted by compact encoding |
| Native title/credits tilemaps | Decode pristine remap-backed or installed two-plane 32×29 title data and pristine 202-row or installed 256-row credits data into complete word models; canonical `LMOWLYR1`/`LMCREDT1` and allocation-independent `LMOBS1` observations replay the real Lunar Magic 3.63 `-TransferTitleScreen` and `-TransferCredits` Wine transitions, including 518 primary-plane `$00FC`→`$38FC` title blank normalizations and all 54 materialized credits-tail rows | Install/update each recovered native format transactionally with checksum repair, semantic reopen, exact undo, built-CLI round trips, Wine-fixture hash/range/ownership/semantic equivalence, and graphical hexadecimal coordinate editing through the shared revision-checked application commands; cover the title secondary plane and credits expanded-only row tail | Reject foreign pointers, missing RATS ownership, altered runtimes/hooks, malformed or trailing portable data, aliased/colliding outputs, partial publication, malformed streams/rows, capacity overflow, invalid graphical rows/columns/planes/words, unloaded selection apply, stale revisions, and dirty close/quit without discarding staged state or mutating ROM/history |
| Title recording/playback | Decode exact `$4..=$8000` movement payloads, `LMTITL01`, minimal ZSNES V143 SRAM placement, and plain/gzip Snes9x tagged RAM blocks | Detect pristine or exact two-owner playback installations; install/update the `$60` runtime and recording transactionally; validate all three biased pointers, checksum, semantic reopen, undo, CLI/native-shell import/export, container-independent oracle equality, and graphical pristine/installed hexadecimal editing through the shared revision-checked command | Reject altered hooks/runtime bytes, missing RATS ownership, pointer disagreement, bad continuation targets, malformed/truncated/oversized native or savestate inputs, missing `$FF`, absent RAM blocks, stale app revisions, invalid graphical tokens, dirty close/quit, aliases, existing outputs, and late allocation failure without mutation |
| Overworld | Decode layer tiles, events and recovered invalid-source normalization, paths, warps, sprites, names, messages, starts; recognize pristine, tagged-source/fixed-destination transfer, and paired-tagged main event-reveal storage; combined entry-addressable/hash-addressable `LMOBS1` evidence binds main reveals, event-number mapping, special-event reveals, and compressed event tilemaps from a real Lunar Magic 3.63 `-TransferOverworld` Wine transition | Edit each native domain in one complete save; reject event models that would change meaning on reopen at standalone, complete-file, native-save, and controller boundaries; atomically edit portable path nodes/directional edges, metadata stable keys, and complete nine-domain files through revisioned application documents; publish standalone path/metadata and interpretation-bound complete-file normalization plus semantic observations as atomic groups; replay all 124 physical Wine ranges, 23 new RATS owners, the 112→120 main-reveal materialization, unchanged 96-entry event map and 24 special reveals, and the 92-entry legacy event-tilemap conversion while preserving Lunar Magic's low-bank LoROM pointer mirrors | Reciprocal/one-way links, duplicate edit targets, stale destinations, output aliases/collisions, source-slot and size-mode mismatch, palette ownership, odd/excessive transferred event owners, destination-only tagging, event limits and invalid source tiles, malformed low-bank runtime hooks/pointers, dirty-close/quit/EOF protection, failed-save retry, stale snapshots, and cross-domain rollback after late failures |

The revision-bound overworld-message increment additionally verifies Lunar Magic 3.63's retained
`$110`-byte version-1.10 runtime, guarded fixed hook, RATS-owned `3M` pointer table, and separately
allocated pools for each 192-message group. A synthetic 200-message installation reopens every
trimmed/terminated record exactly and undoes to the pristine ROM. Detection rejects altered hooks,
runtime targets/markers, non-adjacent operands, untagged or non-three-byte-aligned tables, odd or
out-of-range counts, unowned pools, pointers outside their group pool, unmappable pointers, and
short unterminated strings. The bounded
`LMOWMSG1` artifact and built CLI process additionally prove create-new installation, checksum
repair, semantic ROM reopen, exact export equality, source preservation, and 200-to-400-message
growth. Project/profile tests prove table and pool repointing plus exact undo. The application
state and built shell prove revision-checked install, growth, undo/redo, Save As, and export.
Pristine coverage additionally executes the original 23-selector/25-pointer/high-bit-row format
into all 194 logical records, proves nonblank vanilla content survives expanded installation and
semantic reopen, and confirms exact undo. The modular graphical adapter edits any 8×18 tile,
resizes through the full even 194–512 range, rejects `$FE`, invalid coordinates, unloaded
selections, and stale revisions, and grows a pristine table to 200 before reopening its final tile.
| Undo/transactions | Decode only where persisted; validate model snapshots internally | Property tests for apply/undo/redo identity | Shared-buffer ownership, history trimming, failed-operation rollback |
| Clipboard and sidecars | Parse every recovered native format/version, including synchronized `.mw0` object data, UTF-8 `.mw0t` descriptions, grouped headered `.mw2` sprite placements with revision-selected widths, synchronized `.mwt` descriptions, and distinct one/two-word ExAnimation frames | Round-trip retained BOM/newline framing, atomically edit paired object and multi-sprite placement records, revalidate sprite widths at staged commit/save, own both pair types through revisioned application documents and recoverable paired persistence, restore complete custom-object and interpretation-bound custom-sprite pairs through saved-baseline undo/redo in the built application, Unicode-search descriptions, preserve explicit frame widths, canonically copy/cut/paste ordered frame selections, and publish normalized pairs plus record-addressable semantic observations together | Reject incompatible versions, malformed/trailing objects or sprites, empty/bad placement boundaries, stale revisions/snapshots/history tokens, divergent redo, dirty close/EOF, cross-editor kinds, invalid/duplicate frame indexes and widths, late paste failures, mixed line endings, invalid UTF-8, count/length-table mismatches, noncanonical unrepresentable text framing, oversized 32-KiB sidecars, aliases, collisions, and partial output publication; retain reserved bytes where required |
| Application lifecycle | Track ROM and paired custom-object document paths, dirty baselines, pending snapshots, editor context, capabilities, bounded `LMRECNT1` recent files, bounded level-and-viewport navigation, and bounded command scripts | Open, Save, Save As, close, quit, continued edits during persistence, exact asynchronous acknowledgements, back/forward level movement with signed-origin/exact-zoom restoration over the recovered 100–5000% range, viewport-render current unsaved complete-level revisions, forward-branch invalidation and lifecycle reset, MRU deduplication/trimming and atomic session publication, Unicode round trips, recent-file reopen, process-level scripted command execution, and staged identity-checked paired publication | Confirm replacement/discard from interactive or scripted lines, reject partial camera groups, zero viewport dimensions, zero/sub-100%/over-5000% zoom, viewport updates outside the level editor, overlapping saves, malformed/duplicate/non-Unicode recent entries, malformed/oversized/non-file scripts and state stores, stale edits/acknowledgements, lexical/canonical/hard-link aliases before staging, retain newer edits and adopted destinations, propagate script command failures, reject dirty EOF, and ensure failed/cancelled/anonymous operations do not alter recent state |
| Revision profiles | Bounded-read and parse canonical identity-bound `LMREVPRO1` controller layouts and recovered lookup tables in the shared `lm-profile` crate | Exact deterministic round trip of every controller input; exhaustive non-mutating audit of all 16 pointer tables; CLI qualification and profile-driven import/export for all six native domains; construct each controller through the profile; audit then install/clear through revisioned application commands | Reject oversized/non-UTF-8 files, excessive/long lines and names, excessive table entries, overlapping table spans, tables overlapping the internal header/vector block, payload targets inside any profile table or internal header, wrong game/region/revision/mapper before ROM access, unreadable pointer bytes, invalid/out-of-image targets, unknown/duplicate/missing keys, malformed tables, mapper/shape disagreement, unsafe strides, overflow, input/output aliasing, stale controllers and stale open requests; failed application qualification preserves the active profile and revision; every profile import protects all 16 actual table shapes plus the complete 64-byte internal header/vector block, repairs checksums and reopens before publication; profile changes never dirty ROM or history |
| External tools/plugins | Parse bounded `LMTOOLS1` configuration and event subscriptions; install/list it through the runnable shell | Test shell-free command-template expansion, explicit preview and execution, automatic application-owned open/confirmed-save/real-level-change event routing, separately displayed typed launch fields, direct argument-vector process construction, working directories, and successful completion | Duplicate IDs, truncation/oversize, unknown placeholders/events, Unicode and whitespace paths, missing context, repeated same-level selection, multi-token identifiers, atomic replacement failure, non-blocking typed event diagnostics, failed process starts, signals, and nonzero exits; automatic event effects require an explicit shell execution command |
| Localization, toolbars, and shortcuts | Decode complete typed Unicode `LMLOC001` catalogs, stable-action `LMTBAR01` layouts, logical-key `LMSHORT1` bindings, and aggregate `LMUICFG1` frontend bundles | Canonical deterministic round trips, bounded runnable-shell installation/status, portable gesture parsing, Unicode/function/navigation gesture lookup, all-or-nothing multi-component application-state replacement, shared capability-driven enablement, exact named/shortcut command activation, and typed clipboard requests | Reject every truncation, trailing bytes, oversized or corrupt aggregate sections, invalid UTF-8, unknown/duplicate/missing keys, invalid locale/text limits, malformed identifiers, edge/consecutive separators, invalid/duplicate modifiers, invalid characters/function keys, duplicate gestures, excessive items, reserved bytes, unknown actions/labels, and actions disabled by missing projects/selections or pending I/O without replacing active configuration |
| Rendering | Golden decoded scene graph, RGBA raster, deterministic PNG snapshots, public portable graphics/palette/Map16-page/complete-level/overworld model-to-raster APIs, bounded `LML3FRM1` provider-resolved Layer 3 planes, `LMENTAPP`/`LMOWAPP1` provider appearances, invariant-sealed canvases, an application-connected exact viewport rasterizer, canonical `LMOVLY01` toolkit-neutral screen-space overlays, context-complete standard-sprite dispatch with native placement bytes, level mode, orientation, and bounded animation phase, and a bounded interpretation-only native-level placement canvas | Compare representative tile-sheet, swatch-grid, level, Map16, and overworld views with tolerances and exact PNG hashes where appropriate; exercise standalone CLI publication for every portable renderer; canonically normalize/observe provider planes, appearances, and painter-indexed overlays through atomic create-new workflows and built binaries; preserve painter/part order, identities, signed placements, palettes, tiles, flips, colors, phases, and source-digest binding; construct canvases only through bounded fallible APIs with read-only dimensions; crop and nearest-neighbor scale level, overworld, standalone/current-document Map16, graphics, and palette canvases through one shared signed-origin rational-zoom parser and adapter; consume overlays from every application preview family; draw grids, screen boundaries, decoded object footprints, standard-sprite composite geometry, explicit unresolved/custom markers, and phased selection outlines after sampling; select placements and commit orientation-aware object/sprite canvas moves through the same atomic edit contracts, including canonical reconstruction of expanded upper-coordinate transitions; drive animated standard sprites at a deterministic four-phase 8 Hz cadence without continuously repainting static levels; verify built PNG dimensions | Empty assets, zero layout values, missing palette rows, layer shape/mode, reveal bounds, animation targets, DPI, partial camera groups, zero viewport dimensions, zero grid/dash lengths, empty/inverted selections, unknown overlay records, every truncated prefix, trailing overlay bytes, excessive overlay counts, input/output aliases and collisions, zoom extremes, fractional sampling buckets, signed and unsigned origin overflow/clipping, transparent out-of-source pixels, alpha blending, invalid Map16/graphics/palette references, stale/missing Layer 3 sources, absent optional artwork providers, malformed plane or appearance flags/placement/counts/palettes/duplicates, invalid canvas selections/destinations, opaque expanded sprite controls, dimension and pixel-count overflow, oversized allocation requests, inconsistent rasters, multi-block PNG output |

The `LMENTAPP` application controller additionally proves atomic painter-order insert, replace,
remove, and move batches; late index and palette failures; canonical reopen; stale revisions and
save acknowledgements; saved-baseline undo/redo, stale history rejection, divergent redo
invalidation; dirty shutdown; real-file retry/discard; and built-binary Unicode-path
edit/undo/redo/save lifecycle through bounded `LMENTED1` scripts.
The `LMOWAPP1` controller proves the same history and lifecycle properties over stable sprite-ID
definitions and nested painter-ordered parts, including a built-binary Unicode-path
edit/undo/redo/save workflow through bounded `LMOWAED1` scripts.

The `LMOWAPP1` application controller likewise proves atomic definition insertion, removal, and
movement by stable sprite ID; ordered nested-part insertion, replacement, and removal; duplicate
identity, index, palette, and count failures; stale revisions and save acknowledgements; canonical
reopen; dirty close/discard; and a built-binary Unicode-and-space-path lifecycle through bounded
`LMOWAED1` scripts.

The native standard-object canvas now keeps Lunar Magic's semantic Map16 `$0025` initial cell
value separate from a transient per-cell write mask. Neighbor-sensitive object handlers therefore
observe the same blank Map16 state as the native renderer, while the GUI composites only cells an
object routine actually wrote and retains explicit writes of `$0025`. A real level `$105`
framebuffer audit covers the resulting diagonal terrain, extended object, shared-background,
standard-sprite, and entrance-camera composition; focused cache tests prove that serialization
does not confuse transient write provenance with the debugger-visible 0x3800-word tile cache.
The same framebuffer audit caught two presentation-only corruptions. Extended objects whose
authenticated Map16 artwork was already present are no longer repainted as one marker tile
stretched across their complete interactive bounds. Pristine shared backgrounds retain the native
one-source-column-per-editor-column scale while preserving entrance-derived vertical alignment;
gameplay horizontal scroll rates are camera behavior and no longer duplicate Map16 columns in the
editor. The resulting level `$105` view visibly contains the Yoshi's Island hill field, grass
ground, diagonal dirt slope, bushes, Banzai Bill, Rex, and message block without synthetic artwork.

Before the external release corpus qualifies, runnable-shell ROM persistence is create-new by
default. Tests require `save` and UI-equivalent save actions to reject replacement before creating
a persistence request, preserve the source bytes, accept strict non-duplicated
`--allow-in-place-rom-write` authority, and retain `save-as` as the ordinary atomic path.

## Required fixture families

1. Pristine regional SMW ROMs with known hashes.
2. Headered and unheadered equivalents.
3. Minimally modified ROMs with exactly one Lunar Magic feature enabled.
4. Version-stepped ROMs saved by multiple Lunar Magic releases.
5. Expanded LoROM, ExLoROM, and SA-1 fixtures.
6. Fixtures containing GPS, PIXI, AddmusicK, UberASMTool, and common patch combinations.
7. Corruption fixtures for every parser and allocator boundary.
8. Maximum-size fixtures for objects, sprites, Map16, animation, messages, and overworld events.

## Oracle capture format

Each captured operation should produce a manifest containing:

```text
case_id
lunar_magic_version
input_sha256
output_sha256
operation
operation_parameters
changed_ranges[]
expected_warnings[]
expected_error
decoded_before
decoded_after
owned_allocations_before[]
owned_allocations_after[]
```

The Rust test runner should be able to replay a manifest without Lunar Magic installed. Tests that invoke the original program are fixture-generation tools and should be kept separate from normal CI.

The `oracle-capture` workflow canonicalizes both decoded observations, hashes but never embeds its
ROM inputs, records exact changed ranges, and immediately replays the generated manifest. Allocation
ownership is explicit: `none` claims nothing, while `changed-rats` includes only complete validated
blocks added, removed, changed, or relocated by content identity.

The complete overworld transaction currently batches nine revision-described payloads: both tile
layers, both event-reveal planes, endpoints, messages, sprites, palette, and ExAnimation. All
domain validation and allocation staging must succeed before any pointer or ROM byte is committed.

The `LMOWFULL` interchange serializes those nine modeled domains with explicit dimensions, counts,
sprite record width, palette size, and animation length. A separate strict layout descriptor keeps
revision pointer addresses out of the semantic file. CLI import requires file/layout shape equality,
protects all nine full pointer tables plus checksum bytes, commits through the existing aggregate
transaction, and requires complete semantic reopen equality.
Its portable controller tests monotonic revision-safe undo/redo, saved-baseline restoration, and
divergent branch invalidation. The real application process performs edit/undo/redo/render/save
through the interpretation-bound document and verifies the reopened saved layer.

Undo history is operation-count bounded and can be reconfigured or disabled without exposing its
internal stacks. Lowering the limit drops the oldest undo entries. Undo and redo move an entry
between stacks only after every recorded ROM edit succeeds; a stale or out-of-range history entry
therefore leaves both the ROM and the retryable history state unchanged. A new edit clears the redo
branch, and explicit history clearing changes no ROM bytes.

General navigation graphs additionally use the standalone `LMOWPATH` semantic interchange. Its
tests cover exact round trips, raw-flag preservation, one-way
and reciprocal links, stale destinations, duplicate/self edges, malformed enums, truncation, and
node deletion without dangling edges. Controller and built-application tests additionally cover
canonical saved-baseline undo/redo, stale tokens, divergent redo invalidation, and Unicode paths.
Oracle observations expose every node and edge field. Separately, the confirmed SMW US revision-0
native special path-link table round-trips its fourteen five-byte source records, fourteen
five-byte destination records, and fourteen two-byte target records at the exact pristine offsets.
Its bounded `LMOWLN1` file, CLI, and application-shell workflows preserve sentinels, require exact
identity and shape, update all three planes plus checksum atomically, semantically reopen, and undo
as one project operation.
The graphical path-link adapter addresses every source/destination endpoint and target byte,
resizes through 0–128 entries with sentinel initialization, and proves a pristine 14-to-15 record
runtime install followed by semantic reopen. The warp-link adapter preserves all four opaque words,
resizes through 0–256 records, and proves a pristine 27-to-28 record install. Both require explicit
load-before-apply identity, reject stale commits, and keep dirty state open for close/shutdown
confirmation.

Names, start positions, and portable submap settings use the standalone `LMOWMETA` interchange.
The separately confirmed native boundaries use direct/expanded level-name storage, the exact
22-byte `LMOWST1` runtime-options block, and seven lossless 32-byte `LMOWSET1` records at expanded
slots `$200..$206`. `LMOWMETA` enforces exact 19-tile names, bounded record counts,
unique stable keys, valid submaps, exact file consumption, and lossless raw/unknown fields; every
semantic field and unknown-byte hash is available to oracle observations.
Native graphical coverage additionally loads either player start by stable player index, preserves
the four adjacent option bytes, rejects unaligned coordinates, and semantically reopens a
revision-checked commit. A typed profile detector distinguishes pristine overworld-setting defaults
from an exact installed allocation, rejects malformed present ownership, and feeds a seven-record,
sixteen-word graphical editor. Both workspaces reject unloaded selections and stale revisions and
protect staged changes during close and application shutdown.
The same seven-record overworld workspace now exposes the recovered semantic Layer 3 view:
custom-tilemap/custom-graphics flags, 12-bit tilemap file, two-bit size and position, eight
address-layout words, and four 12-bit graphics files. It dispatches the dedicated semantic
application command while preserving every opaque feature bit, reserved byte, and graphics-word
high nibble; a pristine fixture installs, semantically reopens, and undoes to exact source bytes.
The level-name graphical adapter selects by canonical SMW level number and exact tile index,
requires load-before-apply identity, fills any newly materialized positional prefix with `$1F`,
and tests a pristine-ROM installation followed by semantic reopen. Invalid number gaps, tile
indexes, stale revisions, and dirty editor/application close cannot publish or discard implicitly.
Boss-sequence graphical coverage addresses every tile in all seven 24×8 messages, validates
message/row/column bounds and loaded-selection identity, rejects stale commits, and installs an
edit into a pristine ROM before semantically reopening the exact staged tile. Form, workspace,
and lifecycle tests exercise the independently modularized frontend boundary.
Fixed Lunar Magic ROM metadata graphical coverage addresses all 160 attribution bytes, the one-byte
VRAM version, and all 25 feature-record bytes without assigning semantics to unknown fields.
Validation prevents signature or reserved checksum-bit corruption; loaded-selection identity,
stale revision, dirty-close, and accepted-commit lifecycle rules match the other ROM workspaces.
A real LM 3.63 Wine-produced ROM test stages an opaque attribution edit, commits it through the
application boundary, and semantically reopens the exact byte.

Metadata editor batches upsert or remove names, starts, and settings by their level/player/submap
stable keys on a staged clone. Replacements retain list position, additions append deterministically,
and duplicate command targets, missing removals, count overflow, or final validation failure leave
all three domains unchanged. Oracle tests prove semantic fields change independently while preserved
unknown-byte hashes remain stable, and edited metadata round-trips through `LMOWMETA` exactly.
Controller and built-application tests cover canonical saved-baseline undo/redo, stale tokens,
divergent redo invalidation, and Unicode paths without normalizing retained unknown fields.

The `LM16PAGE` interchange file stores one exact 256-tile graphics/Acts Like page with a source-page
identifier and explicit format version. CLI import is accepted only after transactional allocation,
checksum repair, and semantic reopen verification against the decoded interchange model.
Its independent application controller tests atomic mixed tile/subtile/Acts Like batches, late
index rollback, source-page preservation, arbitrary external Acts Like retention, no-op and stale
revisions, immutable overlapping saves, stale acknowledgement, failed-save cancellation, dirty
close/discard and shutdown registration, bounded saved-baseline undo/redo, stale history tokens,
and divergent redo invalidation. Its built-binary Unicode-path lifecycle exercises undo/redo before
render and save. Page-local
editing never substitutes incomplete graph checks for the complete-set validator.
Focused and process tests also prove `LMPGDR1` renders the current unsaved controller value using
bounded graphics/palette inputs and create-new output, without changing the underlying page file.

`LM16SET1` stores the complete parallel graphics and Acts Like planes. Its gate covers bounded page
counts, exact page shapes and file consumption, deterministic plane ordering, every truncated
prefix, reserved/trailing bytes, whole-workspace stale-link and cycle detection, copy-on-write CLI
normalization, and canonical page/tile-addressable observations. Directly constructed public pages
with any size other than 256 tiles fail before standalone encoding, graph indexing, or ROM
allocation. Complete native saves validate every Acts Like chain first; dangling or cyclic graphs
leave ROM bytes and undo history unchanged, while valid saves are reloaded for semantic equality.

Editor-grade Map16 mutation uses unique `(page, tile)` addresses and stages multi-tile replacements
on a cloned workspace before committing. Subtile number, palette, priority, and flip setters preserve
the other packed fields. Tile replacement and last-page append/removal revalidate the complete Acts
Like graph, so duplicate targets, cycles, dangling links, malformed public page vectors, and the
256-page namespace boundary fail without changing the original set. Edited workspaces round-trip
through `LM16SET1` exactly.
The complete-set document controller additionally verifies revision-safe undo/redo, saved-baseline
restoration, and divergent branch invalidation. The built application performs
edit/undo/redo/render/save before reopening the graph-valid saved set.

Decoded level object and sprite collections support ordered insert, replace, remove, and move
batches on staged clones. Object batches additionally support typed command-ID, parameter,
coordinate-nibble, and screen-advance edits that preserve unrelated bits and extension bytes and
reject implicit record-shape or terminator-collision changes. Both recovered screen-jump variants
have exact packed-target decoding and mutation, while ordinary objects reject jump-only edits. Object
batches must still fit the native 32-KiB terminated stream. Sprite
batches use an explicit policy so revision-native bank limits and the larger bounded `LMLEVEL2`
semantic boundary remain distinct; every raw record retains its revision-specific extension bytes.
Late index failures, malformed records, count/length overflow, and final encoded-size failure leave
the original ordering and bytes unchanged. Successful native batches are reparsed to prove lossless
serialization. The direct pristine-ROM editor wraps successful mixed-domain batches in a bounded
staged history: exact baseline restoration, redo, divergent-branch invalidation, no-op suppression,
failed-batch isolation, and form/canvas resynchronization are independently exercised before ROM
commit. Its object form distinguishes both native screen-jump encodings from ordinary objects,
exposes only their packed target, and routes edits through the encoding-preserving semantic
operation so an exit cannot be accidentally rewritten as an ordinary object. The sprite form
derives original/staged serialized lengths with the active SSC table and identifies the exact
in-place or shared-bank copy-on-write save path; a live Lunar Magic 3.63 Wine oracle reopens the
grown RATS-owned stream and exports the identical canonical decoded sprite sequence. Native
screen-exit records expose their five-bit source screen and exact 16-bit destination/flags value;
semantic edits set Lunar Magic's required `$0400` flag, canonically switch between parameter-0
compact and parameter-2 extended shapes, preserve the unrelated new-screen bit, reject non-exit
records/out-of-range screens atomically, reload the visible semantic and raw forms from the staged
canonical record after every accepted edit, and round-trip all source-screen and representation
boundaries through real Lunar Magic 3.63 MWL import/re-export cycles.

Auxiliary level editing stages entrances, screen exits, secondary exits, and Map16 overrides in one
cross-domain transaction. The first three retain their explicit sequence ordering; Map16 overrides
use their 32-bit tile index as a unique stable key, replace in place, and append new keys
deterministically. Invalid sequence indexes, missing keyed removals, duplicate preexisting keys, or
portable collection-count overflow roll back every domain. Successful mixed batches round-trip all
preserved raw fields through `LMLEVEL2`.

Native expanded-secondary-exit coverage additionally verifies pristine four-plane decoding, both
installed six-plane storage layouts, exact reader signatures, low-bank operands, and RATS ownership.
The pristine installer covers the `$510` Lfix3 body and all 107 relocations, its three initialized
tables and fixed helpers, the base/extended/compatibility runtime family, shared hooks, and either
two or six equally trimmed plane allocations. Tests require semantic reopen for compact and
all-tagged installations, checksum repair, exact undo, rollback after a late hook mismatch,
application revision integration, and a built-CLI pristine import that preserves the source ROM.
The native graphical adapter additionally opens the complete profile-derived table, selects any
entry by hexadecimal index, stages all six logical fields through the shared secondary-exit form,
rejects unloaded selections and stale project revisions, dispatches exactly one
`ReplaceNativeSecondaryExits` command, and protects staged changes during close and application
shutdown. Acceptance—not preparation—clears the editor workspace.
Current Lfix3 acceptance additionally authenticates both fixed helpers, every immutable hook, all
seven hook-to-runtime addends, exact `$510` RATS ownership, and the complete runtime after rebuilding
all 107 relocations at its actual address. The three per-level tables remain mutable by design.
Marker-only, damaged-marker, fixed-hook, ownership, and payload corruption must reject before an
installed secondary-exit update; duplicate native installation must leave application history
unchanged.
Legacy-generation probing must reproduce Lunar Magic's descriptor-selected priority while
authenticating every immutable generation boundary. Generation 1 requires its exact `$02D7CE` JSL
and complete fixed `$02DC50` helper; generation 2 requires its exact `$240` RATS runtime, six hooks,
fixed helpers, `$0110` marker, and the legitimately retained generation-1 hook; generation 3
requires complete current `$0111` authentication. Both pristine hook sites added only by the
current runtime must also authenticate before generation-2 migration. Corrupt candidates reject.
The recovered generation-1 table conversion must cover both `$20` flag branches across all 512
entries. Its migration must require exact pristine preconditions for every later fixed write and
new table destination, bind and convert the live packed source, initialize the third plane, then
authenticate as current, commit once, and undo exactly. The generation-2 replacement must reclaim
only its authenticated RATS owner, preserve all three live
tables byte-for-byte, authenticate as current, repair checksum, commit once, undo exactly, and keep
staged reclamation private on a late failure. Application/native installation may route that proven
migration.
The user-requested Sprite 19 ASM-fix gate requires the exact SMW-US-v1 hook at `$00E762`, fixed
`$20`-byte helper at `$01BCA0`, and branch removal at `$0020A0` recovered from Lunar Magic 3.63
command `$26AC`. Pristine and authenticated shared-helper-only sources must both install to the
same current state; duplicate, truncated, partial, or modified forms must reject without history
changes. Native and CLI routes must reopen the authenticated state, repair the checksum, and undo
or preserve their input byte-exactly.

Native Map16 runtime installation now routes the authenticated Lunar Magic 3.63 pristine-ROM
transformation through the revision-bound built-in runtime dialog. The exact fixed ranges and
`$8000`-byte auxiliary payload remain Wine-derived evidence; the variable bank operand is a typed
relocation into the recovered 512-KiB-to-1-MiB allocation window. Tests require exact source-byte
preconditions, semantic secondary-Map16 reopen, checksum repair, one application history entry,
stale-revision rejection, and byte-exact undo. Compatibility-stage detection and migration remain
outside this pristine-install boundary.
Current-format acceptance additionally authenticates every recovered fixed replacement byte except
the checksum and typed bank relocation, resolves that bank into exact RATS ownership, and compares
the complete `$8000` auxiliary payload. Marker-only and modified fixed/payload fixtures must reject;
an authenticated installation must be recognized by the application, native dialog, and built CLI
without recording a duplicate history entry.

Level property batches edit only the nine proven legacy-header bitfields while preserving every
unowned bit, and treat expanded-header fields as opaque 16-bit values. Raw Layer 1/2 tile changes
require explicit caller-provided dimensions because shape is level-mode and revision dependent;
coordinate edits first prove the existing vector has that exact shape, while whole-map replacement
validates its new shape and portable count limit. Mixed header and cross-layer changes commit
atomically and round-trip through `LMLEVEL2`; invalid fields, missing expanded records, overflowed
dimensions, malformed shapes, and out-of-range coordinates roll back the entire batch.
The three-bit default-music selector is independently writable while preserving byte 2's sprite
tileset and unrelated high bit, and is reachable through both native level editors and terminal
scripts. The two-bit original time-limit selector is independently writable while preserving both
palette fields in byte 3 and has the same native and terminal routes. An opt-in Wine gate changes
all nine fields plus the native sprite header in one revision-bound,
checksum-repaired ROM transaction, reopens the exact Rust result, and requires Lunar Magic 3.63 to
export the same complete legacy header and sprite stream. Lunar Magic may canonically regenerate
mode-dependent screen-exit controls, so this header oracle deliberately does not claim byte-exact
identity for the unrelated Layer 1 object stream after a mode change.

Palette paste/import uses an explicit ownership entry for every decoded color. Atomic batches reject
mismatched ownership shapes, duplicate or absent indexes, fixed colors, and colors generated by a
specific ExAnimation record before changing any entry. Contiguous row/range replacement goes through
the same validation path, while the lossless raw palette model continues preserving all 16 encoded
bits for round-trip compatibility. Successful edits round-trip through `LMPAL1` exactly.

Graphics paste/import follows the same explicit per-tile ownership boundary. Atomic replacement
rejects ownership-shape mismatches, duplicate or missing targets, fixed/ExAnimation-owned tiles, and
any pixel above the 4bpp range before changing the file. Flip-aware deduplication searches existing
tiles in stable index order and prefers exact, horizontal, vertical, then dual-flip equivalence at
each index. Successful batches round-trip through `LMGFX4BP` exactly.
Portable graphics, palette, and ExAnimation controller tests additionally cover saved-baseline
restoration, monotonic undo/redo, stale revision rejection, and divergent redo invalidation. The
built application exercises edit/undo/redo before rendering and saving each applicable asset.

The `LMLVL1` interchange file stores the native layer-1 object and sprite streams for one slot with
an explicit legacy/expanded sprite flag. Its CLI import saves both payloads in one transaction,
protects both full pointer tables and the checksum field, accepts revision-specific sprite-length
tables for custom extra bytes, and requires semantic reopen equality before emitting a ROM.
Its application controller binds that interpretation for the entire session and shares the exact
staged edit engine used by ROM-backed levels. Tests cover mixed object/header/sprite edits, late
cross-stream rollback, custom record lengths, source identity, no-op and stale revisions, immutable
save acknowledgement with newer edits, failed-save cancellation, malformed open specifications,
dirty shutdown registration, saved-baseline undo/redo, stale history tokens, and divergent redo
invalidation. A built-binary workflow through Unicode and space-containing paths exercises
open/edit/undo/redo/save/close while retaining the bound sprite-length interpretation.

The `LMNATAS1` aggregate embeds canonical level, palette, and ExAnimation interchange sections plus
an optional exact installed expanded-settings record. Its framing gate covers bounded total and
section lengths, reserved flags, nested source-slot agreement, revision sprite/animation tables,
and exact consumption. Profile import validates sprite format, palette shape, and settings
availability, then publishes objects, sprites, palette, ExAnimation, settings, pointers, and
checksum as one transaction only after a complete semantic reopen succeeds.
Standalone aggregate normalization uses the exact revision profile and publishes its canonical file
and field-complete observation as one batch. Tests require source/settings metadata plus every
nested observer path, observation text round trips, single-field domain-addressable differences,
input/output alias rejection, and preservation of existing destinations.
Built-binary tests exercise both that normalization path and profile-backed ROM export/import
through paths containing spaces and Unicode. The latter audits every profile table, exports all
five domains, imports through one grouped transaction, reopens the complete aggregate, validates
the detected checksum, and proves a second create-new attempt leaves the first ROM byte-identical.
The profile-driven application aggregate controller additionally proves mixed-domain staging,
palette ownership, late settings failure rollback, one prepared mutation, checksum validity, and
complete native semantic reopen from the committed image.
The portable aggregate document controller tests the same mixed edit engine without a ROM,
including canonical reopen, stale and no-op revisions, whole-batch rollback after a late domain
failure, malformed interpretation inputs, overlapping/mismatched save requests, cancellation, and
acknowledgement of an older immutable snapshot while newer edits remain dirty.
Shell and built-process coverage resolves `LMNADOC1`, its revision profile, aggregate file, and all
four `LMNATED1` child scripts through paths containing spaces and Unicode. It verifies all domains
after canonical save, renders the edited palette from the live revision to a dimension-checked PNG,
exercises undo and redo through the real process, and proves dirty EOF rejection leaves the last
saved aggregate byte-identical. Controller tests additionally cover stale history requests,
monotonic revisions, saved-baseline restoration, redo invalidation, and the 100-state retention
bound.
The runnable `LMNATED1` shell workflow additionally covers relative paths containing spaces and
Unicode, strict child formats, all four editable domains in one operation, checksum repair, and
byte-exact restoration of the entire pre-edit ROM through one undo.
Its ownership-backed route reads a file-backed `LMRATS01` manifest through the real shell
dispatcher, reclaims the exact palette and ExAnimation snapshot blocks, preserves semantic reopen,
rejects reuse of the now-stale evidence for a later edit without changing a byte, and restores the
complete original ROM through one undo.

Current binary MWL parsing retains the exact eight section payloads and fixed attribution/header
fields while bounding total and per-section allocation. Directory entries must remain outside the
header, empty entries use zero offsets, and nonempty ranges are pairwise disjoint. The normalization
CLI decodes fully before create-new publication, so malformed/truncated/overlapping input, aliases,
and existing outputs cannot produce a partial artifact.
Canonical MWL observations expose version, flags, attribution, all section lengths/hashes, and only
the validated fixed-header/common-prefix fields. A one-byte opaque change is therefore visible to
oracle comparison without assigning unsupported meanings to the rest of its section.
Typed optional-asset transfer decodes the palette and compact ExAnimation sections together,
preflights both before mutation, and preserves every unrelated target section. A retained Lunar
Magic 3.63 differential fixture imports a Rust-generated combined MWL, verifies both installed
hooks and all 19 newly allocated tagged blocks, then re-exports with zero semantic MWL differences.
The two positive ExAnimation fixtures also require distinct dynamically resolved pointer-table
addresses, preventing tests from accidentally accepting a hard-coded allocator result.
The complete installed-ROM exporter compares every semantic domain for retained Level 000,
including allocator provenance, and a reciprocal ignored Wine test imports that Rust-generated
MWL into Lunar Magic 3.63 and re-exports it before comparing the header, Layer 1, native two-plane
Layer 2 plus descriptor, sprites, rotated 257-word palette, secondary exits, ExAnimation, expanded
settings, and final ROM checksum. The native level-assets window uses a bounded background reader
for complete MWL import and exposes collision-safe complete export; the shell exposes the same
profile-qualified boundary as `level-mwl-import` and `level-mwl-export`.
Batch shell coverage additionally checks Lunar Magic's exact `base %03X.mwl` naming, extension
stripping, Unicode paths, hidden-file skipping, case-insensitive MWL selection, directory
rejection, create-new collisions, staged multi-file rollback, canonical path aliases, and the
modified-only pointer predicate against both sides of a retained live Wine fixture. Directory
import auto-targets the validated MWL header and continues through per-file decode/save failures;
the native frontend shares batch generation/publication through a non-blocking worker and advances
directory imports only after one exact dispatch acknowledgement. Coordinator tests cover successful
and rejected commit accounting plus close-state cleanup; broader dynamic directory fixtures remain.
The application-shell form binds its source and exact animation interpretation through a bounded
`LMMWLOPT1` specification, commits both sections as one revision, and exercises undo, redo, save,
Unicode/space-containing paths, unrelated-section preservation, and failure rollback through the
built process.
The native MWL window uses a two-file bounded worker request and a focused import module; tests
verify request-shape validation, one-revision commit, unrelated-section preservation, undo, invalid
record-limit rejection, and complete rollback for malformed source data.
An independently loadable semantic interpretation exposes palette/ExAnimation metadata, 257
colors, globals, triggers, and compact records. Direct replacement tests require the maximum-record
bound before history mutation, while shared-command tests ensure focused edits cannot discard the
sibling optional asset or its provenance.
Toolkit-neutral semantic edit batches cover animation creation, metadata, colors, globals,
triggers, and record collection operations. Tests require structured invalid-index failures, late
batch rollback, one-revision commit, record-limit enforcement, and undo. The bounded
`LMMWLOES1`/`LMMWLOE1` shell route is exercised through the built process alongside import,
undo/redo, persistence, Unicode paths, and semantic reopen.
The standalone CLI consumes the same domain parser and edits, preserves unrelated sections,
canonically reopens before create-new publication, and rejects late invalid targets, malformed
scripts, existing outputs, and input/output aliases without publishing a partial file.

The application MWL controller independently owns open/edit/undo/redo/status/save/close/discard lifecycle.
Tests cover exact-revision atomic mixed edits, duplicate script targets, late malformed-header
rollback, bounded section replacement, canonical reopen equality, immutable overlapping-save
exclusion, stale acknowledgement retention, failed-save cancellation, whole-container saved-baseline
restoration, stale history tokens, divergent redo invalidation, dirty close/discard, and shared
shutdown detection. The built application exercises edit/undo/redo/save through space-containing
paths. Controller, bounded `LMWLEDT1` parser, shell adapter, and tests remain
focused source modules.

The separate `LMLEVEL2` semantic bundle covers the whole decoded `Level`: both header forms and
layers, raw tilemaps, variable-width sprites, entrances, screen/secondary exits, Map16 overrides,
and unknown extensions. Tests require deterministic exact round trips, rejection of every truncated
prefix and trailing byte, bounded counts/blobs, invalid-enum rejection, and canonical observation
output. Its application controller additionally tests exact revision tokens, atomic multi-domain
failure, semantic no-ops, immutable overlapping-save exclusion, stale acknowledgement/cancellation,
newer edits during an in-flight save, counter overflow, monotonic undo/redo, saved-baseline
restoration, and divergent redo invalidation. Shell tests cover open/edit/dirty-close, save/reopen,
discard, invalid-script atomicity, and a real-process edit/undo/redo/render/save path. It does not
imply native writes for revision tables that have not been identified.

Portable application documents share a shutdown registry. Process tests require dirty portable
state to reject implicit end-of-input, require affirmative discard on explicit quit, and leave the
underlying artifact byte-identical when an in-memory edit is discarded.

The `LMPAL1` interchange file stores exact SNES BGR555 words with an explicit source slot and color
count. CLI import rejects target-shape mismatches, allocates and repoints transactionally, protects
the complete pointer table and checksum field, and requires semantic reopen equality.

The `LMEXAN1` interchange file wraps a canonical compact ExAnimation payload and is decoded only
with an explicit 256-entry revision size-mode table. Its parser requires exact payload consumption;
CLI import enforces record/encoded limits, protected transactional allocation, checksum repair, and
semantic reopen equality.

The cross-platform shell stages saves in the destination directory and syncs the complete snapshot
before publication. Save As uses create-new publication and never overwrites an existing entry;
ordinary Save replaces only an existing regular file, preserves its permissions, rejects symbolic
links, and uses backup/restore on platforms without replace-by-rename semantics. All failed writes
remove staging files and cancel the pending application snapshot so the user can retry.

The terminal frontend exercises the same request-bound Open, Close, Save, Save As, and Quit effects
as a native toolkit. It can launch without a document, preserves whitespace and Unicode in path
arguments, cancels an open request when reading fails, and requires explicit dirty-state discard
confirmation before replacement or closure. EOF is accepted only for a clean/closed document and
cannot silently abandon modified ROM state. The shell lists or reopens MRU entries through the same
request protocol. The portable recent-document file is capped at ten unique nonempty UTF-8 paths
and rejects every truncation and trailing byte.

The terminal also provides one end-to-end native editing workflow: `level-header` requires an
installed audited profile, a selected level, a typed recovered header field, a bounded byte value,
and an explicit hexadecimal allocation range. Profile-wide allocation policy construction protects
all 16 complete pointer-table spans plus the full internal header, refuses empty/out-of-image or
unmapped ranges, and never infers free space from fill bytes alone. Tests install a profile against
an identity-valid expanded image, stage and revision-check the edit, reload the native stream,
verify the repaired checksum, and undo to the original semantic header.

`LMLEDIT1` extends that terminal boundary to every native level-controller edit variant without
embedding a large command language in the interactive parser. The 64-KiB UTF-8 script caps lines,
line length, commands, raw record bytes, and strict arity; it supports typed header, object, sprite
header, record, screen, and control-token operations. Ordered scripts stage on a cloned controller,
then both complete native streams must encode and reparse identically under command-derived object
lengths and the profile's recovered sprite-length table. Integration tests cover every operation,
expanded tokens, checksum repair, native reload, undo, invalid token ranges, noncanonical record
lengths, invalid/shape-changing typed object fields, late-index rollback, and script size/count
limits. Absolute placement remains contextual to orientation and prior screen-transition records;
tests expose the proven encoded placement fields without inventing standalone X/Y coordinates.

Bounded `LMM16ED1` scripts likewise expose complete native Map16 tile, quadrant, and Acts Like edits
through the runnable shell. Hexadecimal page/tile/value fields and explicit graph-resolution limits
are strictly framed; scripts cap total bytes, lines, line length, and command count. The end-to-end
fixture installs an audited expanded-ROM profile, decodes all 128 declared pages, applies every edit
shape, transactionally saves both planes, verifies checksum and native reload, and undoes to the
original workspace. A two-command late cycle proves earlier changes, ROM bytes, history, and project
revision remain untouched.

`LMPALED1` scripts expose the palette controller without discarding its per-color ownership model.
They require a complete bounded ownership shape, allow unique fixed and ExAnimation overrides only
before edits, and retain exact 16-bit SNES words in individual batches and contiguous ranges. Tests
cover every edit form, raw bit-15 preservation, ownership length mismatch, duplicate/out-of-range
overrides, late protected-color rollback, UTF-8/size/line/command limits, profile-wide allocation,
checksum repair, native reload, and undo.

`LMGFXED1` provides the equivalent ownership-complete application boundary for compressed native
graphics. It accepts only exact 64-nibble 8×8 tiles, supports indexed batches and contiguous ranges,
and requires fixed/ExAnimation overrides before edits. Tests use an actual profile-referenced LZ2
payload and cover deterministic recompression, protected metadata allocation, checksum repair,
native reload, undo, late fixed-tile rollback, ownership-shape mismatch, malformed pixels,
duplicate/late ownership declarations, and all parser limits.

Background serializers can prepare a `RomMutation` by differencing exact logical before/after
images. It coalesces changed runs in the existing extent and carries its mapper and any appended tail
separately; shrink preparation is rejected. Application commit validates controller revision,
snapshot logical length, mapper agreement, complete 32-KiB bank alignment, mapper addressability,
overlap, and result ranges before mutation, then applies append plus writes as one undo batch. Tests
prove partial-bank, beyond-mapper, and wrong-mapper growth cannot change state; late failure cannot
leave a tail behind; undo restores the old length; redo restores new-tail bytes; and every successful
transition invalidates frontend caches with a new revision.

The native level controller consumes one immutable snapshot plus explicit `LevelRomLayout` and
`SpriteLengthTable`, then losslessly loads the selected object/header and sprite streams. Mixed
header, object, sprite-header, token insert/replace/remove/move edits stage on a clone. Commit
preparation runs the existing RATS allocator and native serializer on a private project, repairs the
checksum, and produces a `PreparedRomCommit`. End-to-end tests force bank expansion, dispatch the
result through `AppState`, reload both streams for semantic equality, undo back to the original ROM
length, and redo the exact expanded result; wrong modes/mappers, legacy-incompatible tokens, late
edit failures, and stale revisions remain mutation-free.

The Map16 controller similarly consumes the immutable snapshot and explicit parallel graphics/Acts
Like table layout, loading every declared page before editing. Controller batches call the existing
complete-workspace graph validator for tile replacement, quadrant changes, and Acts Like changes.
Preparation saves every page pair through the multi-payload allocator, repairs the checksum, and
returns one expandable commit. Tests force expansion, compare a full native reload with the edited
set, exercise exact undo/redo length restoration, and prove that late invalid addresses, stale
revisions, mapper/mode disagreement, and unequal table counts cannot partially change state.

The ordinary SMW-US Map16 workspace no longer requires an external revision profile. Its separate
snapshot-bound controller presents all 256 graphical-definition pages through the native GUI:
128 foreground pages with Acts-Like values and 128 background pages without them. Commit stages the
base transferred tables, eight foreground blocks, both raw Acts-Like halves, and eight background
blocks on one private project before publishing one revision-checked mutation. Real pristine-ROM
application and frontend-routing tests edit a built-in foreground definition, an Acts-Like value,
an expanded foreground definition, and a background definition, install the runtime into an
expanded ROM, reopen every 65,536 definition and all 32,768 Acts-Like words, and undo to the exact
original definitions. Background Acts-Like edits are rejected before controller mutation. The same
GUI renders each complete 16×16-tile page from native 4bpp graphics under a
user-selected level, object-tileset, and foreground-palette context; click hit-testing selects the
exact tile, staged subtile changes invalidate the texture, and fixture coverage proves the
256×256 render changes before commit. A pristine 512-KiB GUI workspace supplies explicit
`80000..100000` allocation defaults; commit preparation expands on its private project, installs
and semantically reopens every coordinated table, dispatches expansion plus edits as one
application revision, and one undo restores the exact original ROM length and definitions.

The graphics controller loads and decompresses the file selected by `EditorMode::Graphics` through
an explicit pointer-table and size layout. Decode immediately validates an exact per-tile ownership
map. Ordered change/range batches preserve that ownership boundary across the complete call, so a
late fixed or ExAnimation-owned target rolls back earlier editable tiles. Preparation uses the
deterministic native LZ2 encoder, transactional tagged allocation and repointing, checksum repair,
and compact mutation generation. Tests force expansion, reload and decompress the committed file
for exact tile equality, exercise undo/redo length restoration, and reject wrong modes, mappers,
ownership shapes, and stale results without mutation.

The palette controller loads the slot selected by `EditorMode::Palette` with an explicit pointer
table and exact color count, then requires ownership for every decoded entry. Ordered individual or
range changes operate on raw `Bgr555` words: untouched bit 15 and all unknown encoded values remain
lossless, while fixed and ExAnimation-owned colors reject the complete controller batch. Preparation
uses transactional allocation/repointing, checksum repair, and compact mutation generation. Tests
force expansion, compare exact native reload words, verify preserved high bits and undo/redo length
restoration, and reject wrong modes, mappers, ownership shapes, and stale results atomically.

The ExAnimation controller requires exactly the recovered 256-entry double-size mode table before
decoding the slot selected by `EditorMode::ExAnimation`. Ordered setting, opaque header, trigger,
and record insert/replace/remove/move commands stage on a clone and enforce the revision record
limit. Preparation encodes only genuine compact semantics, validates encoded bounds, performs
tagged allocation/repointing, repairs the checksum, and generates one expandable commit. Tests force
expansion, compare native compact reload equality, restore lengths through undo/redo, and reject
wrong modes, mappers, table lengths, late invalid triggers, disabled-trigger values, unrepresented
workspace bytes, and stale results without mutation. Standalone, native-project, complete-overworld,
and controller tests prove those lossy states fail before allocation or earlier staged edits commit.

The overworld controller binds the existing nine-payload native aggregate to one immutable
snapshot. Revision-fixed layer dimensions and collection counts remain fixed; the controller offers
bounded layer-tile, reveal, endpoint, message-tile, revision-shaped sprite, ownership-aware palette,
and compact-animation replacement/field edits. A single staged clone makes late failures in any
domain roll back earlier changes in every other domain. Preparation validates all domain shapes,
uses the shared nine-request allocator transaction, repairs the checksum, and emits one expandable
commit. Tests mix six domains, force expansion, compare a complete native reload, restore exact ROM
lengths through undo/redo, and reject malformed sprite extensions, protected palette entries,
wrong modes/mappers/size tables, and stale results without partial mutation.

All decoded controllers retain the exact semantic model loaded from their immutable snapshot and
expose `is_modified`. Commit preparation compares against that baseline before invoking native
serialization or allocation. Untouched and fully reverted models produce an explicitly empty
length-bound `RomMutation`; application dispatch emits no project-changed effect and creates no
dirty bytes, checksum rewrite, allocation, history entry, or revision increment. Each controller is
tested with an allocation policy that has space only beyond the current ROM, proving this gate runs
before copy-on-write relocation.

Decoded snapshots use the canonical `LMOBS1` path/value format exposed by `lm-oracle`. This keeps
semantic comparison independent of RATS placement and serialization order: missing, added, and
changed model paths are reported separately for the before and after images. The CLI accepts these
snapshots as optional fourth and fifth `oracle-verify` inputs; release-gate fixtures must supply them.

Oracle manifest and observation parsing is resource bounded before hex decoding or collection
growth. `LMORACLE1` rejects duplicate scalar fields instead of accepting last-write-wins ambiguity;
both formats cap total text, decoded component size, and record count. Recursive suite verification
uses capped streaming reads for manifests, observations, and ROMs, rejects symlinked roots and
fixture artifacts, follows no directory symlinks, continues discovery below parent cases so nested
manifests cannot be hidden, and bounds visited directories and discovered cases before replay.
Standalone portable normalizers use model-owned maximum encoded lengths for graphics, palettes,
Map16 pages/sets, complete levels, Layer 3 files and materialized planes, materialized animation
frames, entity/overworld appearances, paths, and metadata. Representative oversized-input tests
also assert that atomic normalized and observation outputs are not published on rejection.
Composed renderers, native and profile transfer imports, MWL commands, custom-object sidecars,
overworld layout descriptors, and bitmap-to-Map16 workflows share those bounds. Fixed pixel planes,
palette-access maps, graphics-occupancy maps, sprite tables, and ExAnimation mode tables are read at
their exact expected size. Sparse oversized fixtures verify render and multi-output sidecar failures
publish nothing.
Application startup and interactive/recent ROM opening enforce the same 32-MiB ceiling. Oversized
interactive-open tests prove request cancellation permits a subsequent open request. Command-script
and recent-state reads remain regular-file checked and bounded, while application render documents
use each referenced portable model's authoritative maximum rather than a blanket frontend limit.

Corpus breadth can be inspected separately with `oracle-coverage`. The combined
`oracle-release-gate` requires semantic observations for every case, replays the complete suite,
then audits caller-selected Lunar Magic versions, operation names, and `NAME=VALUE` argument tags.
This lets a legal external corpus prove mapper, copier-header, region,
revision, fixture-family, and ecosystem coverage without committing ROMs. An empty corpus, any
missing dimension, malformed requirement, or duplicate case ID fails independently of case replay.
Qualification specifically requires open-save, render-level, level-edit, Lunar Magic reopen, and
emulator-boot cases plus mapper/header/region/revision/ROM-size/fixture-family dimensions, and no
qualifying manifest may record a Lunar Magic error.
Every subsystem must be paired independently with both Lunar Magic reopen and emulator-boot
evidence. Existential operation coverage and existential subsystem coverage cannot be combined from
unrelated cases; adversarial tests remove one pair while retaining both dimensions elsewhere and
require release failure.
Each qualifying case must itself contain nonempty values for all six provenance dimensions. Tests
remove one dimension from a single case while preserving corpus-wide coverage and require the gate
to fail, preventing metadata from an unrelated workflow case from laundering incomplete evidence.
Required after-observation evidence includes open/save reopen, checksum and preservation booleans;
render PNG digest and dimensions; edit semantic-change, reopen and preservation booleans; Lunar
Magic reopen/equality booleans; and emulator name/boot success. Empty observations and declarative
operation labels without these values do not qualify.
Render qualification also requires the actual bounded `render.png`: its lowercase digest and IHDR
dimensions must match the observation, every chunk CRC and boundary must validate, image data and a
terminal IEND must exist, and corruption, truncation, trailing bytes, or symlink substitution fail.
Emulator cases additionally bind a positive frame count and exact output-ROM digest, plus a real
`emulator.png` whose digest, dimensions, chunks, CRCs, image data, and IEND validate. Tests corrupt
the screenshot and jointly alter the manifest/observation ROM claim, requiring both attacks to fail.

The complete portable Map16-page, level, and overworld renderers are public, focused `lm-render`
boundaries used by the CLI and available to native frontends. Existing golden and negative tests
therefore cover the shared APIs' page/layer shapes, reveal bounds, Map16 references, appearance
overlay order, animation application, Layer 3 source binding, graphics/palette references,
deterministic PNG output, dimension overflow, and bounded canvas construction.
Application-shell tests additionally render an open complete-level controller through a bounded,
spec-relative `LMBNDR1` file with space/Unicode paths, exercise the real batch executable, verify
PNG publication, and require a second render to reject the existing create-new destination.
The standalone `LMM16R1` and `LMOWRND1` application specifications provide the same bounded,
spec-relative policy for Map16 and overworld previews, including the exact 256-byte size-mode table
and optional sprite/animation providers. A built-binary batch test renders both outputs through the
public shared APIs and proves that rerunning the script cannot replace either PNG.

Native overworld warp/exit-link coverage includes the four independent little-endian coordinate
planes recovered from Lunar Magic's active Wine descriptor. Model tests preserve `0xFFFF`
sentinels, reject partial or unequal planes and more than 256 entries, and round-trip the canonical
`LMOWWR1` container. Project tests prove four-plane save, checksum repair, late-overlap rollback,
and one-step undo. The SMW profile test decodes and exactly re-encodes all four 27-word pristine
planes. CLI and application-shell parsing and dispatch are covered; a built CLI unchanged
export/import reproduces the source ROM digest exactly.

Expanded warp-table tests additionally verify both-hook detection, current and legacy marker
discrimination, all four pointer operands, malformed-pointer rejection, exact runtime installation
from pristine bytes, RATS ownership validation, growth relocation, pointer republication, semantic
reopen, checksum repair, ROM expansion, and whole-operation undo. The real CLI installs 30 links,
exports an equivalent `LMOWWR1`, then grows the installed table to 40 links and reopens the output.

Expanded special-path tests independently verify the single `JSL ...; RTS` hook at logical
`$21A35`, the exact recovered 112-byte runtime and `LM 00 01` marker, count-minus-one and
five-byte-stride immediates, all eight field-address operands, and the contiguous
`5N + 5N + 2N` allocation. Pristine installation, installed growth, exact RATS ownership,
checksum repair, semantic reopen, optional expansion, application undo, and built-CLI
install/export/grow workflows are covered.

The real Lunar Magic 3.63 `-TransferOverworld` process fixture now has a complete thirteen-domain
semantic observation in addition to its focused event observations. It independently addresses
all entries in transferred Map16 definitions and acts-like values, event reveals, event numbers,
special reveals, event tilemaps, special paths, warp links, level names, player starts, expanded
overworld settings, ordinary messages, and boss messages while the manifest separately proves all
124 physical ranges and 23 RATS owners.
Pristine conversion establishes the exact 27-to-54 warp and 93-to-96 level-name materializations.

Native SMW-US Map16 persistence additionally loads both the pristine tables and the exact
Lunar Magic 3.63 transferred fixture into identical editor meaning, edits the complete 0x2000-word
definition domain and 2884-entry Acts-Like domain, and installs the recovered split even/odd
sized-RLE definitions plus independent low-byte and terminated-RLE high-byte tables in protected
bank-confined RATS allocations. The save gate verifies pointer republication, the shared
definition-bank invariant, default Acts-Like normalization, checksum repair, exact semantic reopen,
and failure-atomic rejection of invalid table shapes.

The complete native persistence layer composes that base writer with all foreground and background
blocks in one staged transaction. Ghidra `SaveMap16BaseDataToRom` proves ownership of the first
`0x1000` definition bytes, while `SavePrimaryMap16DataBlocks` begins block-zero allocation at live
buffer `+0x1000`. Tests change both sides of that boundary and a background tile, then prove exact
semantic reopen, checksum repair, single-step undo, and pristine runtime installation.

The graphical ROM Map16 workspace exposes the corresponding complete Lunar Magic `.map16`
transfer through bounded background input and recoverable create-new output. Import stages all
65,536 tiles as one controller edit, retains the live protected `$0000`–`$01ff` definitions while
accepting their Acts-Like values, and normalizes background Acts-Like values to zero. Export writes
all 256 pages, emits Lunar Magic's zero definition prefix for those protected tiles, and preserves
auxiliary, selection, and editor-state bytes from an imported template. Focused tests reject
malformed live page shapes before indexing, block close during file I/O, byte-exactly round-trip
the retained Lunar Magic 3.63 `all.map16`, and commit/reopen representative protected, foreground,
Acts-Like, and background changes through the native ROM transaction.

The same graphical workspace also drives the native bitmap-import session rather than stopping at
the headless planner. A bounded PNG load or non-blocking system-clipboard image load uses the
selected level's actual object-tileset graphics and palette context. Clipboard RGBA dimensions,
pixel count, byte shape, and channels are validated before entering the shared session; the window
cannot close while that worker is outstanding. Both entry paths display original and converted
images without changing their aspect ratio and give the panes one synchronized pan state with
integer 1×–8× zoom. Reuse, flip matching,
8×8 optimization, 16×16 deduplication, and layer-priority changes recompute the converted preview
from immutable input. The color panel can reserve any nontransparent entry in the active row:
reserved colors remain byte-exact and participate in nearest-color matching, while the Wu
quantizer fills only editable entries in stable ascending order. Fully reserved rows remain usable
without palette mutation, ownership/shape failures retain the preceding valid preview, and the
default all-editable path retains the previously verified conversion algorithm. Acceptance
publishes palette, assigned GFX/ExGFX slots, complete Map16
pages, required runtime installation, pointers, and checksum as one revision-checked ROM command;
cancel and dirty-close retain no partial ROM mutation.

The recovered multi-row successor is represented by toolkit-independent color-option and
allocation primitives rather than being inferred from the preceding one-row workflow. Tests bind
all 128 entry states to Lunar Magic's exact free/reusable/reserved bits and initialized row layout,
enforce the recovered 1–128 color and 1–4 priority bounds, preserve transparency at index zero, and
cover the Popularity raw-frequency admission gate plus destination-aware, wrapping
distance-exponent priority levels. Weighted
unique 8×8 color sets, subset utility, reusable-color overlap, capacity selection, stable
installation, per-member pixel-frequency aggregation, and disjoint-row allocation are covered
independently. Application tests carry the
selected row for every 8×8 tile into all four packed Map16 subtile descriptors and reconstruct the
preview through those same rows; the original one-row path and complete ROM commit/reopen tests
remain green. The native dialog exposes both reduction choices and the complete 8×16 state grid.
Popularity also exposes both recovered high-color neighborhood methods, with method 1 enabled and
method 2 disabled by default. Focused tests cover method 1's first-neighbor replacement and method
2's sub-128 score aggregation and stronger-neighbor rejection. Median-cut currently uses the
bounded deterministic variance splitter. A disposable-process Wine audit now drives the original
clipboard-bitmap dialog, captures its pre/post palette and planar graphics buffers, and verifies
the conversion guard is restored. Its low-color fixture proves that the fast path and Popularity
histogram use Lunar Magic's source-bit-2 five-bit rounding; an exact Rust regression covers the
four resulting SNES words. Automated original-versus-Rust comparison across broader bitmap
variants remains a compatibility gate.

Native main event-reveal coverage verifies the two descriptor-derived long operands at logical
`$25A74/$25A84`, the pristine 112-entry fallback, little-endian source and big-endian destination
planes, source normalization bounds, and the 255-entry editor maximum. Detection rejects mixed
fixed/tagged storage, unequal or odd plane lengths, overlapping or inexact RATS ownership, and
invalid pointers. Profile and project tests cover pristine load, transactional 112-to-200
installation, growth to 255, checksum repair, semantic reopen, and byte-exact undo. Built CLI and
application-shell tests exercise bounded `LMOWEVT1` import/export, preserve the input ROM, save to
a new path, and reopen the detected model with a valid checksum.
The graphical adapter resizes the complete 1–255 record range, rejects `$0800+` sources and
unloaded/stale selections, and proves a pristine 112-to-200 installation with exact final-record
semantic reopen and dirty-shutdown protection.

Native event-number-map coverage converts all eight pristine source/value pairs into the 96-byte
semantic map, preserves meaningful prefixes through `LMOWMAP1`, and verifies the recovered
version-1.10 runtime, `JSL` target, marker, table operand, fixed 96-byte and extended 256-byte
regions. Detection fails closed on malformed hooks/runtime bytes, unowned relocated pointers, and
oversized RATS payloads. Profile tests cover install, extended-to-fixed switching, checksum repair,
semantic reopen, and byte-exact undo. Built CLI and application tests cover bounded export/import,
revisioned undo/redo, create-new saving, input preservation, and detected output reopen.
Graphical coverage selects any byte-keyed mapping, requires the selection to be loaded before
mutation, exercises entry `$FF` to install and reopen the complete 256-byte representation, and
retains staged state across invalid bytes, stale revisions, close, and application shutdown.

Special-event reveal coverage verifies all 24 little-endian source words, big-endian destination
words, and direction bytes, including source normalization and lossless `LMOWSPC1` framing. The
native installer checks every pristine hook byte and installs three table allocations, the exact
64-byte dual-entry runtime, exact 48-byte pointer runtime, fixed helper, inline fragment, byte
repairs, table/self fixups, and all four `JSL` entry points in one transaction. Detection requires
exact RATS ownership and every runtime relationship. Profile, CLI, and application tests cover
pristine load, first installation, installed copy-on-write update, checksum repair, semantic
reopen, two-step exact undo, revisioned undo/redo, create-new saving, and input preservation.
The graphical adapter selects all 24 records, validates the source normalization boundary before
publication, dispatches the complete table as one revision-checked command, and proves semantic
reopen of source, destination, and direction planes while guarding unloaded selections and dirty
shutdown.

Event-tilemap coverage verifies an exact typed pristine detector across the loader, index, reveal,
and state fragments, recognizes installed LZ2/LZ3 streams only through complete runtime and RATS
ownership validation, and exports pristine zero workspaces without opcode guessing. The graphical
adapter addresses primary low/high and secondary high bytes for all `$800` tiles; tests edit both
final primary-high and secondary-high bytes, install from pristine, semantically reopen them, and
reject invalid tiles, unloaded selections, stale commits, and dirty close/shutdown.

## Initial release gate

A first writable release is acceptable only when it can open representative fixtures, render levels accurately, make a bounded level edit, save transactionally, reopen its own output, reopen Lunar Magic output into an equivalent model, preserve unrelated bytes and unknown tagged data, and run the result in an emulator. Until then, write support should remain opt-in and always target a new file.
