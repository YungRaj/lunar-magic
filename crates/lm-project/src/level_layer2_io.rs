use crate::{
    LevelLoadError, LevelPointerTable, PayloadLoadError, PayloadReadPolicy, PayloadSaveError,
    PayloadSaveRequest, PayloadSaveResult, Project,
};
use lm_codec::{CodecError, decode_terminated_rle_prefix, encode_terminated_rle};
use lm_level::{
    LEGACY_LAYER2_TILEMAP_LEN, Layer2Storage, NATIVE_LAYER2_TILEMAP_LEN, NativeLayer2Data,
    NativeLayer2Error, compact_legacy_layer2_tilemap, expand_legacy_layer2_tilemap,
    interleave_layer2_tilemap_planes, level_mode_layer2_storage, split_layer2_tilemap_planes,
};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelLayer2RomLayout {
    pub mapper: Mapper,
    pub pointers: LevelPointerTable,
    pub maximum_compressed_len: usize,
    pub tilemap_encoding: LevelLayer2TilemapEncoding,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LevelLayer2TilemapEncoding {
    Legacy {
        high_byte: u8,
    },
    #[default]
    SplitPlanes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevelLayer2SaveOptions {
    pub allocation: AllocationPolicy,
    pub previous_block: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Debug)]
pub enum LevelLayer2IoError {
    Layout(LevelLoadError),
    Load(PayloadLoadError),
    Model(NativeLayer2Error),
    Codec(CodecError),
    DecompressedLength(usize),
    StorageMismatch {
        level_mode: u8,
        actual: &'static str,
    },
    Save(PayloadSaveError),
}

impl fmt::Display for LevelLayer2IoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native Layer 2 I/O failed: {self:?}")
    }
}

impl std::error::Error for LevelLayer2IoError {}

impl From<LevelLoadError> for LevelLayer2IoError {
    fn from(value: LevelLoadError) -> Self {
        Self::Layout(value)
    }
}
impl From<PayloadLoadError> for LevelLayer2IoError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}
impl From<NativeLayer2Error> for LevelLayer2IoError {
    fn from(value: NativeLayer2Error) -> Self {
        Self::Model(value)
    }
}
impl From<CodecError> for LevelLayer2IoError {
    fn from(value: CodecError) -> Self {
        Self::Codec(value)
    }
}
impl From<PayloadSaveError> for LevelLayer2IoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl Project {
    /// Loads one level's native Layer 2 object stream or compressed tilemap.
    ///
    /// # Errors
    ///
    /// Rejects invalid pointers, malformed object/terminated-RLE data, and decoded tilemaps that
    /// cannot be normalized to exactly 0x800 bytes.
    pub fn load_level_layer2(
        &self,
        level: usize,
        level_mode: u8,
        layout: LevelLayer2RomLayout,
    ) -> Result<NativeLayer2Data, LevelLayer2IoError> {
        let pointer = layout.pointers.pointer_offset(level)?;
        match level_mode_layer2_storage(level_mode) {
            Layer2Storage::Objects => {
                let payload = self.load_payload(
                    pointer,
                    layout.mapper,
                    &PayloadReadPolicy::TaggedOrTerminated {
                        terminator: vec![0xff],
                        maximum_len: 0x8000,
                        bank_size: Some(0x8000),
                    },
                )?;
                Ok(NativeLayer2Data::decode_mwl(level_mode, &payload.bytes)?)
            }
            Layer2Storage::CompressedTilemap => {
                let payload = self.load_payload(
                    pointer,
                    layout.mapper,
                    &PayloadReadPolicy::TaggedOrBounded {
                        maximum_len: layout.maximum_compressed_len,
                        bank_size: Some(0x8000),
                    },
                )?;
                let decoded =
                    decode_terminated_rle_prefix(&payload.bytes, NATIVE_LAYER2_TILEMAP_LEN)?.bytes;
                let tilemap = match decoded.len() {
                    LEGACY_LAYER2_TILEMAP_LEN => {
                        let high_byte = match layout.tilemap_encoding {
                            LevelLayer2TilemapEncoding::Legacy { high_byte } => high_byte,
                            LevelLayer2TilemapEncoding::SplitPlanes => 0,
                        };
                        expand_legacy_layer2_tilemap(&decoded, high_byte)?
                    }
                    NATIVE_LAYER2_TILEMAP_LEN => interleave_layer2_tilemap_planes(&decoded)?,
                    actual => return Err(LevelLayer2IoError::DecompressedLength(actual)),
                };
                Ok(NativeLayer2Data::Tilemap(tilemap))
            }
        }
    }

    /// Compresses/encodes and transactionally saves one native Layer 2 payload.
    ///
    /// # Errors
    ///
    /// Rejects a model inconsistent with the level mode, invalid encoding, allocation failure, or
    /// an invalid pointer layout.
    pub fn save_level_layer2(
        &mut self,
        level: usize,
        level_mode: u8,
        data: &NativeLayer2Data,
        layout: LevelLayer2RomLayout,
        options: &LevelLayer2SaveOptions,
    ) -> Result<PayloadSaveResult, LevelLayer2IoError> {
        let request = level_layer2_save_request(level, level_mode, data, layout, options)?;
        Ok(self.save_tagged_payload(&request)?)
    }

    /// Compresses and saves one native Layer 2 payload while repairing the SNES checksum in the
    /// same undoable transaction.
    ///
    /// # Errors
    ///
    /// Rejects an incompatible model, invalid layout/allocation, or checksum field without
    /// changing ROM bytes or history.
    pub fn save_level_layer2_with_checksum(
        &mut self,
        level: usize,
        level_mode: u8,
        data: &NativeLayer2Data,
        layout: LevelLayer2RomLayout,
        options: &LevelLayer2SaveOptions,
        checksum_field: usize,
    ) -> Result<PayloadSaveResult, LevelLayer2IoError> {
        let request = level_layer2_save_request(level, level_mode, data, layout, options)?;
        let mut saved = self.save_tagged_payloads_with_checksum(
            format!("save level {level:03x} layer 2"),
            &[request],
            checksum_field,
        )?;
        Ok(saved.remove(0))
    }
}

pub(crate) fn level_layer2_save_request(
    level: usize,
    level_mode: u8,
    data: &NativeLayer2Data,
    layout: LevelLayer2RomLayout,
    options: &LevelLayer2SaveOptions,
) -> Result<PayloadSaveRequest, LevelLayer2IoError> {
    let payload = match (level_mode_layer2_storage(level_mode), data) {
        (Layer2Storage::Objects, NativeLayer2Data::Objects(_)) => data.encode_mwl()?,
        (Layer2Storage::CompressedTilemap, NativeLayer2Data::Tilemap(bytes)) => {
            if bytes.len() != NATIVE_LAYER2_TILEMAP_LEN {
                return Err(LevelLayer2IoError::DecompressedLength(bytes.len()));
            }
            let native = match layout.tilemap_encoding {
                LevelLayer2TilemapEncoding::Legacy { high_byte } => {
                    compact_legacy_layer2_tilemap(bytes, high_byte)?
                }
                LevelLayer2TilemapEncoding::SplitPlanes => split_layer2_tilemap_planes(bytes)?,
            };
            encode_terminated_rle(&native)
        }
        (_, NativeLayer2Data::Objects(_)) => {
            return Err(LevelLayer2IoError::StorageMismatch {
                level_mode,
                actual: "objects",
            });
        }
        (_, NativeLayer2Data::Tilemap(_)) => {
            return Err(LevelLayer2IoError::StorageMismatch {
                level_mode,
                actual: "tilemap",
            });
        }
    };
    let maximum_payload_len = match data {
        NativeLayer2Data::Objects(_) => 0x8000,
        NativeLayer2Data::Tilemap(_) => layout.maximum_compressed_len,
    };
    Ok(PayloadSaveRequest {
        description: format!("save level {level:03x} layer 2"),
        payload,
        pointer: layout.pointers.pointer_offset(level)?.into(),
        mapper: layout.mapper,
        allocation_policy: options.allocation.clone(),
        previous_block: options.previous_block.clone(),
        reuse_identical: options.reuse_identical,
        maximum_payload_len,
        erase_fill: options.erase_fill,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::ProtectedRange;
    use lm_rom::{RomImage, pc_to_snes};

    fn layout(entries: usize) -> LevelLayer2RomLayout {
        LevelLayer2RomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x20,
                entries,
                stride: 3,
            },
            maximum_compressed_len: 0x8000,
            tilemap_encoding: LevelLayer2TilemapEncoding::SplitPlanes,
        }
    }

    #[test]
    fn loads_both_recovered_storage_classes() {
        let tilemap = vec![0x12; NATIVE_LAYER2_TILEMAP_LEN];
        let compressed = encode_terminated_rle(&split_layer2_tilemap_planes(&tilemap).unwrap());
        let mut bytes = vec![0xff; 0x8000];
        bytes[0x20..0x23]
            .copy_from_slice(&pc_to_snes(Mapper::LoRom, 0x100).unwrap().to_le_bytes()[..3]);
        bytes[0x23..0x26]
            .copy_from_slice(&pc_to_snes(Mapper::LoRom, 0x300).unwrap().to_le_bytes()[..3]);
        bytes[0x100..0x100 + compressed.len()].copy_from_slice(&compressed);
        bytes[0x300..0x306].copy_from_slice(&[1, 2, 3, 4, 5, 0xff]);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert_eq!(
            project.load_level_layer2(0, 0, layout(2)).unwrap(),
            NativeLayer2Data::Tilemap(tilemap)
        );
        assert!(matches!(
            project.load_level_layer2(1, 1, layout(2)).unwrap(),
            NativeLayer2Data::Objects(_)
        ));
    }

    #[test]
    fn legacy_tilemap_load_applies_the_layout_high_byte() {
        let legacy = vec![0x34; LEGACY_LAYER2_TILEMAP_LEN];
        let compressed = encode_terminated_rle(&legacy);
        let mut bytes = vec![0xff; 0x8000];
        bytes[0x20..0x23]
            .copy_from_slice(&pc_to_snes(Mapper::LoRom, 0x100).unwrap().to_le_bytes()[..3]);
        bytes[0x100..0x100 + compressed.len()].copy_from_slice(&compressed);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let mut legacy_layout = layout(1);
        legacy_layout.tilemap_encoding = LevelLayer2TilemapEncoding::Legacy { high_byte: 0x12 };
        let NativeLayer2Data::Tilemap(tilemap) =
            project.load_level_layer2(0, 0, legacy_layout).unwrap()
        else {
            panic!("level mode zero must decode as a tilemap");
        };
        assert_eq!(&tilemap[..4], &[0x34, 0x12, 0x34, 0x12]);
    }

    #[test]
    fn saves_reopens_and_undoes_compressed_tilemap() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let options = LevelLayer2SaveOptions {
            allocation: AllocationPolicy {
                search: 0x100..0x8000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![ProtectedRange(0x20..0x23)],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let expected = NativeLayer2Data::Tilemap(vec![0x34; NATIVE_LAYER2_TILEMAP_LEN]);
        project
            .save_level_layer2(0, 0, &expected, layout(1), &options)
            .unwrap();
        assert_eq!(
            project.load_level_layer2(0, 0, layout(1)).unwrap(),
            expected
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn checksum_save_is_atomic_and_undoable() {
        let mut bytes = vec![0xff; 0x8000];
        bytes[0x7fdc..0x7fe0].fill(0);
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let original = project.save_snapshot();
        let options = LevelLayer2SaveOptions {
            allocation: AllocationPolicy {
                search: 0x100..0x7fdc,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![ProtectedRange(0x20..0x23), ProtectedRange(0x7fc0..0x8000)],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let expected = NativeLayer2Data::Tilemap(vec![0x34; NATIVE_LAYER2_TILEMAP_LEN]);
        project
            .save_level_layer2_with_checksum(0, 0, &expected, layout(1), &options, 0x7fdc)
            .unwrap();
        assert_eq!(
            project.load_level_layer2(0, 0, layout(1)).unwrap(),
            expected
        );
        let checksum = lm_rom::compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap();
        assert_eq!(
            &project.rom.logical_bytes()[0x7fdc..0x7fe0],
            &checksum.encoded()
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);

        assert!(!project.history.can_undo());
        assert!(
            project
                .save_level_layer2_with_checksum(
                    0,
                    0,
                    &NativeLayer2Data::Tilemap(vec![0x55; NATIVE_LAYER2_TILEMAP_LEN]),
                    layout(1),
                    &options,
                    usize::MAX,
                )
                .is_err()
        );
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }
}
