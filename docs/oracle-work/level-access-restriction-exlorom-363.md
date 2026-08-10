# Lunar Magic 3.63 ExLoROM level-access restriction oracle

The source is the authenticated copier-headered SMW-US level-save-000 image converted by Rust's
byte-exact 64-Mbit ExLoROM transaction. Lunar Magic 3.63 x86 opened that image under Wine 11.13.
The original feature is hidden until its character-sequence gate sets byte `00E27DDE`; the central
level-editor dispatcher maps command `$23A4` (not `$23A5`) to restriction case `$17`. The title was
set to `Codex Parity Test`, the optional IPS prompt was declined, and Lunar Magic reported
`Level Access Restriction Complete` after bulk-resaving the accessible level domain.

- Before SHA-256: `b1ee089c0426eb06e3ad4b37c4c36e54df6496ea389fb2418a92e6ef384c21be`
- After SHA-256: `ca37c9106db49f47c9cfff75b8fbf22b07b9b9a3f2bfaec5226f5a836912beef`
- Physical size: `8,389,120` bytes in both images
- Changed bytes: `314` across `33` contiguous physical ranges
- Recovered random material: per-save bytes `$32/$5D`, graphics word `$0B32`

The differential proves descriptor-routed hooks and payloads in the relocated SMW body, bank `$81`
in the per-save helper, both base and `+$400000` title/version copies, all 50 split standard-GFX
pointers plus GFX32/GFX33, the installed integrity records, and two bulk-save RATS-owner moves from
the relocated body into the lower ExLoROM allocation window. The first owner retains its exact
payload while XOR-protecting nine words; the second retains its exact five-byte payload. All three
references and the allocator cursor are rebased to the newly allocated payloads. The old owners are
zero-erased, the checksum is preserved through the descriptor-selected compensation area, and the
copier-header restriction byte becomes one.

`level_access_restriction::tests::exlorom_restriction_matches_authenticated_bulk_save_and_undoes_exactly`
requires complete physical-image equality. Companion tests prove headerless logical equivalence,
dynamic first-fit collision relocation, checksum validity, corrupt-owner rejection without mutation,
and byte-exact Undo/Redo.
