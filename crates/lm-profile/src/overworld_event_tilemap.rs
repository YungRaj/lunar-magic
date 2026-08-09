//! Lunar Magic overworld event-tilemap runtime for SMW US revision 0.

use lm_codec::{encode_lz2, encode_lz3};
use lm_overworld::EventTilemapBuffers;
use lm_project::{
    EventTilemapCompression, EventTilemapPatchError, EventTilemapPatchLocator, PatchFixup,
    PatchFixupEncoding, PatchPayload, PatchWrite, Project, RelocatablePatchPlan,
};
use lm_rats::AllocationPolicy;
use lm_rom::{Mapper, RomError};

use crate::SMW_US_V1_CHECKSUM_FIELD;

pub const SMW_US_V1_EVENT_TILEMAP_LOADER_MARKER: usize = 0x02_57f9;
pub const SMW_US_V1_EVENT_TILEMAP_SECONDARY_MARKER: usize = 0x02_5818;
pub const SMW_US_V1_EVENT_TILEMAP_PRIMARY_LOW_WORD: usize = 0x02_5803;
pub const SMW_US_V1_EVENT_TILEMAP_PRIMARY_BANK: usize = 0x02_5808;
pub const SMW_US_V1_EVENT_TILEMAP_SECONDARY_LOW_WORD: usize = 0x02_5822;
pub const SMW_US_V1_EVENT_TILEMAP_SECONDARY_BANK: usize = 0x02_5827;
pub const SMW_US_V1_EVENT_TILEMAP_SEARCH_START: usize = 0x08_0000;
pub const SMW_US_V1_EVENT_TILEMAP_SEARCH_END: usize = 0x09_0000;

const INDEX_HOOK: usize = 0x02_d8b1;
const INDEX_RUNTIME: usize = 0x02_dcd0;
const REVEAL_HOOK: usize = 0x02_0f8a;
const REVEAL_RUNTIME: usize = 0x01_ba10;
const REVEAL_OPCODE: usize = 0x02_1002;
const STATE_HOOK: usize = 0x02_1199;
const STATE_RUNTIME: usize = 0x01_ba50;

const PRIMARY_RUNTIME: [u8; 64] = [
    0xa2, 0x00, 0xd0, 0x86, 0x00, 0xa9, 0x7e, 0x85, 0x02, 0xa2, 0x66, 0x66, 0x86, 0x8a, 0xa9, 0x66,
    0x85, 0x8c, 0x08, 0x4b, 0x62, 0x06, 0x00, 0xf4, 0x4c, 0x80, 0x5c, 0xde, 0xb8, 0x00, 0x28, 0xa2,
    0x00, 0xc8, 0x86, 0x00, 0xa9, 0x7f, 0x85, 0x02, 0xa2, 0x66, 0x66, 0x86, 0x8a, 0xa9, 0x66, 0x85,
    0x8c, 0x08, 0x4b, 0x62, 0x06, 0x00, 0xf4, 0x4c, 0x80, 0x5c, 0xde, 0xb8, 0x00, 0x28, 0x80, 0x16,
];
const INDEX_RUNTIME_BYTES: [u8; 32] = [
    0xa8, 0xad, 0x09, 0x01, 0xf0, 0x06, 0x98, 0xf0, 0x02, 0xa9, 0x01, 0x6b, 0xa8, 0xad, 0xbf, 0x13,
    0xc9, 0x25, 0x90, 0x03, 0xe9, 0x24, 0xc8, 0x8d, 0xbb, 0x17, 0x85, 0x0e, 0x98, 0x6b, 0xff, 0xff,
];
const REVEAL_RUNTIME_BYTES: [u8; 48] = [
    0x08, 0xc2, 0x30, 0xa6, 0x04, 0xbf, 0x00, 0xd0, 0x7e, 0x29, 0xff, 0x00, 0xaa, 0xbd, 0xa2, 0x1e,
    0x29, 0x10, 0x00, 0xd0, 0x0d, 0xa9, 0x01, 0x00, 0xf0, 0x0d, 0xad, 0xc1, 0x13, 0xa2, 0x07, 0x00,
    0x28, 0x6b, 0xa9, 0x80, 0x00, 0x80, 0xf6, 0xa9, 0xff, 0x00, 0x80, 0xf1, 0xff, 0xff, 0xff, 0xff,
];
const STATE_RUNTIME_BYTES: [u8; 160] = [
    0x08, 0xc2, 0x30, 0xa5, 0x04, 0x48, 0x20, 0x82, 0xba, 0xa6, 0x04, 0x68, 0x85, 0x04, 0xbf, 0x00,
    0xd0, 0x7e, 0x29, 0xff, 0x00, 0xaa, 0xbd, 0xa2, 0x1e, 0x29, 0x80, 0x00, 0xf0, 0x0d, 0xbd, 0xa2,
    0x1e, 0x29, 0x20, 0x00, 0xf0, 0x05, 0xa9, 0xff, 0x00, 0x80, 0x03, 0xad, 0xc1, 0x13, 0x28, 0xc9,
    0x81, 0x6b, 0x08, 0xe2, 0x10, 0xc2, 0x20, 0xa5, 0x00, 0x48, 0xa5, 0x02, 0x48, 0xae, 0xd6, 0x0d,
    0xbd, 0x1f, 0x1f, 0x85, 0x00, 0xbd, 0x21, 0x1f, 0x85, 0x02, 0x8a, 0x4a, 0x4a, 0xaa, 0xa5, 0x00,
    0x29, 0x0f, 0x00, 0x85, 0x04, 0xa5, 0x00, 0x29, 0x10, 0x00, 0x0a, 0x0a, 0x0a, 0x0a, 0x65, 0x04,
    0x85, 0x04, 0xa5, 0x02, 0x0a, 0x0a, 0x0a, 0x0a, 0x29, 0xff, 0x00, 0x65, 0x04, 0x85, 0x04, 0xa5,
    0x02, 0x29, 0x10, 0x00, 0xf0, 0x08, 0xa5, 0x04, 0x18, 0x69, 0x00, 0x02, 0x85, 0x04, 0xbd, 0x11,
    0x1f, 0x29, 0xff, 0x00, 0xf0, 0x08, 0xa5, 0x04, 0x18, 0x69, 0x00, 0x04, 0x85, 0x04, 0x68, 0x85,
    0x02, 0x68, 0x85, 0x00, 0x28, 0x60, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

const PRISTINE_PRIMARY: [u8; 64] = [
    0xa9, 0x00, 0x85, 0x0d, 0xa9, 0xd0, 0x85, 0x0e, 0xa9, 0x7e, 0x85, 0x0f, 0xa9, 0x00, 0x85, 0x0a,
    0xa9, 0xd8, 0x85, 0x0b, 0xa9, 0x7e, 0x85, 0x0c, 0xa9, 0x00, 0x85, 0x04, 0xa9, 0xc8, 0x85, 0x05,
    0xa9, 0x7e, 0x85, 0x06, 0xa0, 0x01, 0x00, 0x84, 0x00, 0xa0, 0xff, 0x07, 0xa9, 0x00, 0x97, 0x0a,
    0x97, 0x0d, 0x88, 0x10, 0xf9, 0xa0, 0x00, 0x00, 0xbb, 0xb7, 0x04, 0xc9, 0x56, 0x90, 0x11, 0xc9,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1EventTilemapStorage {
    Pristine,
    Installed(EventTilemapCompression),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSmwUsV1EventTilemaps {
    pub buffers: EventTilemapBuffers,
    pub storage: SmwUsV1EventTilemapStorage,
}

#[derive(Debug)]
pub enum SmwUsV1EventTilemapLoadError {
    Rom(RomError),
    PristineMismatch {
        offset: usize,
    },
    Installed {
        lz2: EventTilemapPatchError,
        lz3: EventTilemapPatchError,
    },
}

impl std::fmt::Display for SmwUsV1EventTilemapLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "SMW US event-tilemap load failed: {self:?}")
    }
}

impl std::error::Error for SmwUsV1EventTilemapLoadError {}

impl From<RomError> for SmwUsV1EventTilemapLoadError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

#[must_use]
pub fn smw_us_v1_event_tilemap_locator() -> EventTilemapPatchLocator {
    smw_us_v1_event_tilemap_locator_for_mapper(Mapper::LoRom)
}

/// Returns the event-tilemap locator in the mapper's active SMW body.
#[must_use]
pub fn smw_us_v1_event_tilemap_locator_for_mapper(mapper: Mapper) -> EventTilemapPatchLocator {
    let base = if mapper == Mapper::ExLoRom {
        0x40_0000
    } else {
        0
    };
    EventTilemapPatchLocator {
        mapper,
        loader_marker: base + SMW_US_V1_EVENT_TILEMAP_LOADER_MARKER,
        secondary_marker: base + SMW_US_V1_EVENT_TILEMAP_SECONDARY_MARKER,
        primary_low_word: base + SMW_US_V1_EVENT_TILEMAP_PRIMARY_LOW_WORD,
        primary_bank: base + SMW_US_V1_EVENT_TILEMAP_PRIMARY_BANK,
        secondary_low_word: base + SMW_US_V1_EVENT_TILEMAP_SECONDARY_LOW_WORD,
        secondary_bank: base + SMW_US_V1_EVENT_TILEMAP_SECONDARY_BANK,
        primary_runtime: patched_primary_runtime(),
        index_hook: base + INDEX_HOOK,
        index_hook_bytes: [0x22, 0xd0, 0xdc, 0x05],
        index_runtime: base + INDEX_RUNTIME,
        index_runtime_bytes: INDEX_RUNTIME_BYTES,
        reveal_hook: base + REVEAL_HOOK,
        reveal_hook_bytes: [0x22, 0x10, 0xba, 0x03, 0xea],
        reveal_runtime: base + REVEAL_RUNTIME,
        reveal_runtime_bytes: REVEAL_RUNTIME_BYTES,
        reveal_opcode: base + REVEAL_OPCODE,
        reveal_opcode_byte: 0x8c,
        state_hook: base + STATE_HOOK,
        state_hook_bytes: [0x22, 0x50, 0xba, 0x03],
        state_runtime: base + STATE_RUNTIME,
        state_runtime_bytes: STATE_RUNTIME_BYTES,
    }
}

#[must_use]
pub fn smw_us_v1_event_tilemap_update_policy(image_len: usize) -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_EVENT_TILEMAP_SEARCH_START..image_len.saturating_add(0x8000).min(0x40_0000),
    )
}

/// Loads exact pristine zero workspaces or either recognized installed compression variant.
///
/// # Errors
///
/// Rejects any partial/altered pristine fixed fragment and installed runtimes that fail both LZ2
/// and LZ3 ownership, hook, pointer, and decoded-shape validation.
pub fn load_smw_us_v1_event_tilemaps(
    project: &Project,
) -> Result<LoadedSmwUsV1EventTilemaps, SmwUsV1EventTilemapLoadError> {
    load_smw_us_v1_event_tilemaps_for_mapper(project, Mapper::LoRom)
}

/// Loads event tilemaps from the mapper's active SMW body.
pub fn load_smw_us_v1_event_tilemaps_for_mapper(
    project: &Project,
    mapper: Mapper,
) -> Result<LoadedSmwUsV1EventTilemaps, SmwUsV1EventTilemapLoadError> {
    let locator = smw_us_v1_event_tilemap_locator_for_mapper(mapper);
    if project.rom.read(locator.loader_marker, 1)? == [0xa2] {
        let lz2 =
            project.load_event_tilemap_buffers_detected(locator, EventTilemapCompression::Lz2);
        if let Ok(loaded) = lz2 {
            return Ok(LoadedSmwUsV1EventTilemaps {
                buffers: loaded.buffers,
                storage: SmwUsV1EventTilemapStorage::Installed(EventTilemapCompression::Lz2),
            });
        }
        let lz2 = lz2.unwrap_err();
        let lz3 =
            project.load_event_tilemap_buffers_detected(locator, EventTilemapCompression::Lz3);
        if let Ok(loaded) = lz3 {
            return Ok(LoadedSmwUsV1EventTilemaps {
                buffers: loaded.buffers,
                storage: SmwUsV1EventTilemapStorage::Installed(EventTilemapCompression::Lz3),
            });
        }
        return Err(SmwUsV1EventTilemapLoadError::Installed {
            lz2,
            lz3: lz3.unwrap_err(),
        });
    }
    let base = if mapper == Mapper::ExLoRom {
        0x40_0000
    } else {
        0
    };
    for (local_offset, expected) in pristine_fragments() {
        let offset = base + local_offset;
        if project.rom.read(offset, expected.len())? != expected {
            return Err(SmwUsV1EventTilemapLoadError::PristineMismatch { offset });
        }
    }
    Ok(LoadedSmwUsV1EventTilemaps {
        buffers: EventTilemapBuffers::default(),
        storage: SmwUsV1EventTilemapStorage::Pristine,
    })
}

fn pristine_fragments() -> [(usize, &'static [u8]); 8] {
    [
        (SMW_US_V1_EVENT_TILEMAP_LOADER_MARKER, &PRISTINE_PRIMARY),
        (INDEX_RUNTIME, &[0xff; 32]),
        (INDEX_HOOK, &[0xf0, 0x02, 0xa9, 0x01]),
        (REVEAL_RUNTIME, &[0xff; 48]),
        (REVEAL_HOOK, &[0xa2, 0x07, 0xad, 0xc1, 0x13]),
        (REVEAL_OPCODE, &[0x89]),
        (STATE_RUNTIME, &[0xff; 160]),
        (STATE_HOOK, &[0xc9, 0x81, 0xf0, 0x4c]),
    ]
}

/// Builds Lunar Magic 3.63's exact four-fragment loader family and both compressed streams.
#[must_use]
pub fn smw_us_v1_event_tilemap_installation_plan(
    buffers: &EventTilemapBuffers,
    compression: EventTilemapCompression,
) -> RelocatablePatchPlan {
    let encode = |bytes: &[u8]| match compression {
        EventTilemapCompression::Lz2 => encode_lz2(bytes),
        EventTilemapCompression::Lz3 => encode_lz3(bytes),
    };
    let fixup = |offset, target_payload, encoding| PatchFixup {
        offset,
        target_payload,
        target_addend: 0,
        encoding,
    };
    RelocatablePatchPlan {
        description: "install native overworld event tilemaps".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy::lorom(
            SMW_US_V1_EVENT_TILEMAP_SEARCH_START..SMW_US_V1_EVENT_TILEMAP_SEARCH_END,
        ),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![
            PatchPayload {
                bytes: encode(&buffers.encode_primary_stream()),
                fixups: Vec::new(),
            },
            PatchPayload {
                bytes: encode(&buffers.encode_secondary_high_stream()),
                fixups: Vec::new(),
            },
        ],
        writes: vec![
            PatchWrite {
                offset: SMW_US_V1_EVENT_TILEMAP_LOADER_MARKER,
                expected: PRISTINE_PRIMARY.to_vec(),
                replacement: patched_primary_runtime().to_vec(),
                fixups: vec![
                    fixup(0x0a, 0, PatchFixupEncoding::Low16),
                    fixup(0x0f, 0, PatchFixupEncoding::Bank8),
                    fixup(0x29, 1, PatchFixupEncoding::Low16),
                    fixup(0x2e, 1, PatchFixupEncoding::Bank8),
                ],
            },
            fixed_write(INDEX_RUNTIME, &[0xff; 32], &INDEX_RUNTIME_BYTES),
            fixed_write(
                INDEX_HOOK,
                &[0xf0, 0x02, 0xa9, 0x01],
                &[0x22, 0xd0, 0xdc, 0x05],
            ),
            fixed_write(REVEAL_RUNTIME, &[0xff; 48], &REVEAL_RUNTIME_BYTES),
            fixed_write(
                REVEAL_HOOK,
                &[0xa2, 0x07, 0xad, 0xc1, 0x13],
                &[0x22, 0x10, 0xba, 0x03, 0xea],
            ),
            fixed_write(REVEAL_OPCODE, &[0x89], &[0x8c]),
            fixed_write(STATE_RUNTIME, &[0xff; 160], &STATE_RUNTIME_BYTES),
            fixed_write(
                STATE_HOOK,
                &[0xc9, 0x81, 0xf0, 0x4c],
                &[0x22, 0x50, 0xba, 0x03],
            ),
        ],
    }
}

fn patched_primary_runtime() -> [u8; 64] {
    let mut bytes = PRIMARY_RUNTIME;
    bytes[0x18..0x1a].copy_from_slice(&0x804c_u16.to_le_bytes());
    bytes[0x1b..0x1e].copy_from_slice(&[0xde, 0xb8, 0x00]);
    bytes[0x21] = 0xc8;
    bytes[0x37..0x39].copy_from_slice(&0x804c_u16.to_le_bytes());
    bytes[0x3a..0x3d].copy_from_slice(&[0xde, 0xb8, 0x00]);
    bytes
}

fn fixed_write<const EXPECTED: usize, const REPLACEMENT: usize>(
    offset: usize,
    expected: &[u8; EXPECTED],
    replacement: &[u8; REPLACEMENT],
) -> PatchWrite {
    debug_assert_eq!(EXPECTED, REPLACEMENT);
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn exact_pristine_install_reopens_and_undo_restores_the_original() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            lm_project::Project::open_supported(RomImage::from_bytes(original.clone()).unwrap())
                .unwrap();
        let mut buffers = EventTilemapBuffers::default();
        buffers.primary_bytes_mut()[0] = 1;
        buffers.primary_bytes_mut()[0x800] = 0x59;
        buffers.secondary_high_bytes_mut()[0] = 0xab;
        let compression = EventTilemapCompression::Lz2;
        let plan = smw_us_v1_event_tilemap_installation_plan(&buffers, compression);
        project
            .install_event_tilemap_buffers(
                &buffers,
                smw_us_v1_event_tilemap_locator(),
                compression,
                &plan,
            )
            .unwrap();
        assert_eq!(
            project
                .load_event_tilemap_buffers_detected(
                    smw_us_v1_event_tilemap_locator(),
                    compression,
                )
                .unwrap()
                .buffers,
            buffers
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn typed_loader_distinguishes_exact_pristine_and_installed_storage() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            lm_project::Project::open_supported(RomImage::from_bytes(original).unwrap()).unwrap();
        let pristine = load_smw_us_v1_event_tilemaps(&project).unwrap();
        assert_eq!(pristine.buffers, EventTilemapBuffers::default());
        assert_eq!(pristine.storage, SmwUsV1EventTilemapStorage::Pristine);
        let mut buffers = pristine.buffers;
        buffers.primary_bytes_mut()[0x123] = 0x45;
        let compression = EventTilemapCompression::Lz2;
        let plan = smw_us_v1_event_tilemap_installation_plan(&buffers, compression);
        project
            .install_event_tilemap_buffers(
                &buffers,
                smw_us_v1_event_tilemap_locator(),
                compression,
                &plan,
            )
            .unwrap();
        let installed = load_smw_us_v1_event_tilemaps(&project).unwrap();
        assert_eq!(installed.buffers, buffers);
        assert_eq!(
            installed.storage,
            SmwUsV1EventTilemapStorage::Installed(EventTilemapCompression::Lz2)
        );
    }

    #[test]
    fn wine_transfer_overworld_materializes_legacy_event_tilemaps() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive");
        let after = fs::read(fixture.join("after.smc")).unwrap();
        let mut project =
            Project::open_supported(RomImage::from_bytes(after.clone()).unwrap()).unwrap();
        let loaded = load_smw_us_v1_event_tilemaps(&project).unwrap();
        assert_eq!(
            loaded.storage,
            SmwUsV1EventTilemapStorage::Installed(EventTilemapCompression::Lz2)
        );
        assert_eq!(
            loaded.buffers.primary_bytes()[..EventTilemapBuffers::WORD_COUNT]
                .iter()
                .filter(|byte| **byte != 0)
                .count(),
            92
        );
        assert_eq!(loaded.buffers.primary_bytes()[60], 1);
        assert_eq!(
            loaded.buffers.primary_bytes()[EventTilemapBuffers::WORD_COUNT + 60],
            0xc0
        );
        assert!(
            loaded
                .buffers
                .secondary_high_bytes()
                .iter()
                .all(|byte| *byte == 0)
        );

        let mut edited = loaded.buffers;
        edited.secondary_high_bytes_mut()[0x7ff] = 0x80;
        project
            .save_event_tilemap_buffers_detected(
                &edited,
                smw_us_v1_event_tilemap_locator(),
                EventTilemapCompression::Lz2,
                &smw_us_v1_event_tilemap_update_policy(project.rom.logical_len()),
                crate::SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        assert_eq!(
            load_smw_us_v1_event_tilemaps(&project).unwrap().buffers,
            edited
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), after);

        project.rom.write(INDEX_HOOK + 3, &[0x85]).unwrap();
        assert!(matches!(
            load_smw_us_v1_event_tilemaps(&project),
            Err(SmwUsV1EventTilemapLoadError::Installed { .. })
        ));
    }
}
