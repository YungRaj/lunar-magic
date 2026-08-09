use crate::{
    PayloadLoadError, PayloadPointer, PayloadSaveError, PayloadSaveRequest, PayloadSaveResult,
    Project,
};
use lm_overworld::{
    CUSTOM_OVERWORLD_SPRITE_ID_COUNT, NativeCustomOverworldSpriteError,
    NativeCustomOverworldSpriteTable,
};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCustomOverworldSpriteRomLayout {
    pub mapper: Mapper,
    pub pointer_offset: usize,
    pub maximum_payload_len: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedNativeCustomOverworldSprites {
    pub table: NativeCustomOverworldSpriteTable,
    pub block: Option<RatsBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCustomOverworldSpriteSaveOptions {
    pub allocation: AllocationPolicy,
    pub previous_block: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Debug)]
pub enum NativeCustomOverworldSpriteIoError {
    Rom(lm_rom::RomError),
    Load(PayloadLoadError),
    MissingOwnership,
    Codec(NativeCustomOverworldSpriteError),
    Save(PayloadSaveError),
}

impl fmt::Display for NativeCustomOverworldSpriteIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native custom overworld sprite I/O failed: {self:?}"
        )
    }
}

impl std::error::Error for NativeCustomOverworldSpriteIoError {}

impl From<PayloadLoadError> for NativeCustomOverworldSpriteIoError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}

impl From<lm_rom::RomError> for NativeCustomOverworldSpriteIoError {
    fn from(value: lm_rom::RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<NativeCustomOverworldSpriteError> for NativeCustomOverworldSpriteIoError {
    fn from(value: NativeCustomOverworldSpriteError) -> Self {
        Self::Codec(value)
    }
}

impl From<PayloadSaveError> for NativeCustomOverworldSpriteIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl Project {
    /// Loads a RATS-owned Lunar Magic custom-overworld-sprite stream.
    ///
    /// # Errors
    ///
    /// Rejects invalid pointers or ownership, malformed native records, and invalid size tables.
    pub fn load_native_custom_overworld_sprites(
        &self,
        layout: NativeCustomOverworldSpriteRomLayout,
        record_sizes: &[u8; CUSTOM_OVERWORLD_SPRITE_ID_COUNT],
    ) -> Result<LoadedNativeCustomOverworldSprites, NativeCustomOverworldSpriteIoError> {
        let pointer = self.rom.read(layout.pointer_offset, 3)?;
        if pointer == [0, 0, 0] || pointer == [0xff, 0xff, 0xff] {
            return Ok(LoadedNativeCustomOverworldSprites {
                table: NativeCustomOverworldSpriteTable {
                    maps: std::array::from_fn(|_| Vec::new()),
                },
                block: None,
            });
        }
        let payload = self.load_tagged_payload(layout.pointer_offset, layout.mapper)?;
        let table = NativeCustomOverworldSpriteTable::decode(&payload.bytes, record_sizes)?;
        let block = payload
            .block
            .ok_or(NativeCustomOverworldSpriteIoError::MissingOwnership)?;
        Ok(LoadedNativeCustomOverworldSprites {
            table,
            block: Some(block),
        })
    }

    /// Encodes and relocates a native custom-overworld-sprite stream transactionally.
    ///
    /// # Errors
    ///
    /// Rejects invalid semantic records, payload limits, allocation failures, and unsafe pointer
    /// or checksum writes.
    pub fn save_native_custom_overworld_sprites(
        &mut self,
        table: &NativeCustomOverworldSpriteTable,
        record_sizes: &[u8; CUSTOM_OVERWORLD_SPRITE_ID_COUNT],
        layout: NativeCustomOverworldSpriteRomLayout,
        options: &NativeCustomOverworldSpriteSaveOptions,
    ) -> Result<PayloadSaveResult, NativeCustomOverworldSpriteIoError> {
        let payload = table.encode(record_sizes)?;
        Ok(self.save_tagged_payload(&PayloadSaveRequest {
            description: "save native custom overworld sprites".into(),
            payload,
            pointer: PayloadPointer::contiguous(layout.pointer_offset),
            mapper: layout.mapper,
            allocation_policy: options.allocation.clone(),
            previous_block: options.previous_block.clone(),
            reuse_identical: options.reuse_identical,
            maximum_payload_len: layout.maximum_payload_len,
            erase_fill: options.erase_fill,
        })?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::NativeCustomOverworldSprite;
    use lm_rats::ProtectedRange;
    use lm_rom::{RomImage, compute_snes_checksum};

    const CHECKSUM: usize = 0x7fdc;

    fn layout() -> NativeCustomOverworldSpriteRomLayout {
        NativeCustomOverworldSpriteRomLayout {
            mapper: Mapper::LoRom,
            pointer_offset: 0x20,
            maximum_payload_len: 0x400,
        }
    }

    fn sizes() -> [u8; CUSTOM_OVERWORLD_SPRITE_ID_COUNT] {
        [4; CUSTOM_OVERWORLD_SPRITE_ID_COUNT]
    }

    fn table() -> NativeCustomOverworldSpriteTable {
        NativeCustomOverworldSpriteTable {
            maps: std::array::from_fn(|map| {
                if map == 2 {
                    vec![NativeCustomOverworldSprite {
                        id: 5,
                        x: 0x80,
                        y: 0x118,
                        screen: 0x20,
                        extra: vec![0xaa],
                    }]
                } else {
                    Vec::new()
                }
            }),
        }
    }

    fn options() -> NativeCustomOverworldSpriteSaveOptions {
        NativeCustomOverworldSpriteSaveOptions {
            allocation: AllocationPolicy {
                search: 0x100..0x7fdc,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![
                    ProtectedRange(0x20..0x23),
                    ProtectedRange(CHECKSUM..CHECKSUM + 4),
                ],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    #[test]
    fn vanilla_empty_pointer_opens_as_seven_editable_empty_maps() {
        for sentinel in [[0_u8; 3], [0xff_u8; 3]] {
            let mut bytes = vec![0xff; 0x8000];
            bytes[0x20..0x23].copy_from_slice(&sentinel);
            let loaded = Project::new(RomImage::from_bytes(bytes).unwrap())
                .load_native_custom_overworld_sprites(layout(), &sizes())
                .unwrap();
            assert!(loaded.block.is_none());
            assert!(loaded.table.maps.iter().all(Vec::is_empty));
        }
    }

    #[test]
    fn save_load_and_undo_are_transactional() {
        let mut bytes = vec![0xff; 0x8000];
        let checksum = compute_snes_checksum(&bytes, CHECKSUM).unwrap().encoded();
        bytes[CHECKSUM..CHECKSUM + checksum.len()].copy_from_slice(&checksum);
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let original = project.save_snapshot();
        let saved = project
            .save_native_custom_overworld_sprites(&table(), &sizes(), layout(), &options())
            .unwrap();
        assert_eq!(
            project
                .load_native_custom_overworld_sprites(layout(), &sizes())
                .unwrap()
                .table,
            table()
        );
        assert_eq!(
            project
                .load_native_custom_overworld_sprites(layout(), &sizes())
                .unwrap()
                .block,
            Some(saved.block)
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn invalid_record_is_atomic() {
        let mut bytes = vec![0xff; 0x8000];
        let checksum = compute_snes_checksum(&bytes, CHECKSUM).unwrap().encoded();
        bytes[CHECKSUM..CHECKSUM + checksum.len()].copy_from_slice(&checksum);
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let original = project.save_snapshot();
        let mut invalid = table();
        invalid.maps[2][0].x = 1;
        assert!(matches!(
            project.save_native_custom_overworld_sprites(&invalid, &sizes(), layout(), &options()),
            Err(NativeCustomOverworldSpriteIoError::Codec(
                NativeCustomOverworldSpriteError::CoordinateNotGridAligned { .. }
            ))
        ));
        assert_eq!(project.save_snapshot(), original);
    }
}
