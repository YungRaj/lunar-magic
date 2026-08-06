use crate::{
    LevelLoadError, LevelPointerTable, PayloadLoadError, PayloadReadPolicy, PayloadSaveError,
    PayloadSaveRequest, PayloadSaveResult, Project, RomWrite,
};
use lm_codec::{CodecError, decode_terminated_rle_prefix, encode_terminated_rle};
use lm_level::{
    LEGACY_LAYER2_TILEMAP_LEN, Layer2Storage, MwlLayer2Descriptor, NATIVE_LAYER2_TILEMAP_LEN,
    NativeLayer2Data, NativeLayer2Error, compact_legacy_layer2_tilemap,
    expand_legacy_layer2_tilemap, interleave_layer2_tilemap_planes, level_mode_layer2_storage,
    split_layer2_tilemap_planes,
};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::{Mapper, SnesPointer24};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelLayer2RomLayout {
    pub mapper: Mapper,
    pub pointers: LevelPointerTable,
    /// Optional bank substituted when a pointer entry uses `$FF` to identify a shared background.
    pub background_bank_substitution: Option<u8>,
    /// Optional pristine-ROM redirect selected by a parallel per-level pointer sentinel.
    pub legacy_pointer_redirect: Option<LevelLayer2PointerRedirect>,
    /// Format-$103 one-byte descriptor table. `None` selects pristine/legacy synthesized state.
    pub descriptor_table: Option<LevelLayer2DescriptorTable>,
    pub maximum_compressed_len: usize,
    pub tilemap_encoding: LevelLayer2TilemapEncoding,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelLayer2PointerRedirect {
    pub selector_pointers: LevelPointerTable,
    pub selector_value: [u8; 3],
    pub source_value: [u8; 3],
    pub target_value: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelLayer2DescriptorTable {
    pub offset: usize,
    pub entries: usize,
    pub stride: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedLevelLayer2 {
    pub data: NativeLayer2Data,
    /// Lossless installed-table descriptor after Lunar Magic's load-time normalization.
    pub descriptor: Option<MwlLayer2Descriptor>,
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
    DescriptorLayout,
    DescriptorSlot {
        slot: usize,
        entries: usize,
    },
    DescriptorOffsetOverflow,
    DescriptorValue(u32),
    DescriptorRom(lm_rom::RomError),
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
impl From<lm_rom::RomError> for LevelLayer2IoError {
    fn from(value: lm_rom::RomError) -> Self {
        Self::DescriptorRom(value)
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
        Ok(self
            .load_level_layer2_with_descriptor(level, level_mode, layout)?
            .data)
    }

    /// Loads native Layer 2 plus a lossless installed format-$103 descriptor when configured.
    ///
    /// # Errors
    ///
    /// Rejects the same pointer, codec, and model failures as [`Self::load_level_layer2`], plus an
    /// invalid or out-of-range descriptor table.
    pub fn load_level_layer2_with_descriptor(
        &self,
        level: usize,
        level_mode: u8,
        layout: LevelLayer2RomLayout,
    ) -> Result<LoadedLevelLayer2, LevelLayer2IoError> {
        let pointer = layout.pointers.pointer_offset(level)?;
        let mut pointer_bytes: [u8; 3] = self
            .rom
            .read(pointer, 3)?
            .try_into()
            .map_err(|_| LevelLayer2IoError::DescriptorLayout)?;
        if let Some(redirect) = layout.legacy_pointer_redirect {
            let selector_offset = redirect.selector_pointers.pointer_offset(level)?;
            if self.rom.read(selector_offset, 3)? == redirect.selector_value
                && pointer_bytes == redirect.source_value
            {
                pointer_bytes = redirect.target_value;
            }
        }
        let substituted_pointer = layout
            .background_bank_substitution
            .filter(|_| pointer_bytes[2] == 0xff)
            .map(|bank| {
                SnesPointer24::decode(&[pointer_bytes[0], pointer_bytes[1], bank])
                    .map_err(|_| LevelLayer2IoError::DescriptorLayout)
            })
            .transpose()?;
        let raw_descriptor = layout
            .descriptor_table
            .map(|table| read_layer2_descriptor(self, level, table))
            .transpose()?;
        let storage = if substituted_pointer.is_some() {
            Layer2Storage::CompressedTilemap
        } else {
            level_mode_layer2_storage(level_mode)
        };
        match storage {
            Layer2Storage::Objects => {
                let policy = PayloadReadPolicy::TaggedOrTerminated {
                    terminator: vec![0xff],
                    maximum_len: 0x8000,
                    bank_size: Some(0x8000),
                };
                let payload = if let Some(pointer) = substituted_pointer {
                    self.load_payload_from_pointer(pointer, layout.mapper, &policy)?
                } else {
                    self.load_payload(pointer, layout.mapper, &policy)?
                };
                Ok(LoadedLevelLayer2 {
                    data: NativeLayer2Data::decode_mwl(level_mode, &payload.bytes)?,
                    descriptor: raw_descriptor,
                })
            }
            Layer2Storage::CompressedTilemap => {
                let policy = PayloadReadPolicy::TaggedOrBounded {
                    maximum_len: layout.maximum_compressed_len,
                    bank_size: Some(0x8000),
                };
                let payload = if let Some(pointer) = substituted_pointer {
                    self.load_payload_from_pointer(pointer, layout.mapper, &policy)?
                } else {
                    self.load_payload(pointer, layout.mapper, &policy)?
                };
                let mut decoded =
                    decode_terminated_rle_prefix(&payload.bytes, NATIVE_LAYER2_TILEMAP_LEN)?.bytes;
                // The original loader writes shared-background RLE directly into the low-byte
                // plane and only consumes its first 0x360 bytes. A small number of pristine
                // streams emit one trailing padding byte before the terminator.
                if (substituted_pointer.is_some() || raw_descriptor.is_some())
                    && decoded.len() == LEGACY_LAYER2_TILEMAP_LEN + 1
                {
                    decoded.truncate(LEGACY_LAYER2_TILEMAP_LEN);
                }
                let (tilemap, descriptor) = match decoded.len() {
                    LEGACY_LAYER2_TILEMAP_LEN => {
                        let high_byte = if substituted_pointer.is_some()
                            && u16::from_le_bytes([pointer_bytes[0], pointer_bytes[1]]) >= 0xe8fe
                        {
                            0x01
                        } else if substituted_pointer.is_some() {
                            0x00
                        } else {
                            match layout.tilemap_encoding {
                                LevelLayer2TilemapEncoding::Legacy { high_byte } => high_byte,
                                LevelLayer2TilemapEncoding::SplitPlanes => 0,
                            }
                        };
                        let high_byte =
                            raw_descriptor.map_or(high_byte, MwlLayer2Descriptor::active_bank);
                        let tilemap = expand_legacy_layer2_tilemap(&decoded, high_byte)?;
                        (
                            tilemap,
                            raw_descriptor.map(|descriptor| {
                                MwlLayer2Descriptor::from_raw(
                                    (descriptor.raw() & 0x0a) | MwlLayer2Descriptor::SPLIT_PLANES,
                                )
                            }),
                        )
                    }
                    NATIVE_LAYER2_TILEMAP_LEN => {
                        (interleave_layer2_tilemap_planes(&decoded)?, raw_descriptor)
                    }
                    actual => return Err(LevelLayer2IoError::DecompressedLength(actual)),
                };
                Ok(LoadedLevelLayer2 {
                    data: NativeLayer2Data::Tilemap(tilemap),
                    descriptor,
                })
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

    /// Saves Layer 2 and its optional format-$103 descriptor with one checksum transaction.
    ///
    /// The descriptor/layout pair must either both be present or both be absent. This is the
    /// narrow per-level equivalent of the full native-assets aggregate save path.
    ///
    /// # Errors
    ///
    /// Rejects invalid payloads, descriptor pairing, allocation, direct writes, or checksum
    /// fields without changing the ROM.
    pub fn save_level_layer2_with_descriptor_and_checksum(
        &mut self,
        level: usize,
        level_mode: u8,
        loaded: &LoadedLevelLayer2,
        layout: LevelLayer2RomLayout,
        options: &LevelLayer2SaveOptions,
        checksum_field: usize,
    ) -> Result<PayloadSaveResult, LevelLayer2IoError> {
        let request = level_layer2_save_request(level, level_mode, &loaded.data, layout, options)?;
        let descriptor_write = match (loaded.descriptor, layout.descriptor_table) {
            (None, None) => None,
            (Some(descriptor), Some(table)) => Some(level_layer2_descriptor_write(
                self, level, descriptor, table,
            )?),
            _ => return Err(LevelLayer2IoError::DescriptorLayout),
        };
        let writes = descriptor_write.into_iter().collect::<Vec<_>>();
        let mut saved = self.save_tagged_payloads_with_checksum_and_writes(
            format!("save level {level:03x} layer 2"),
            &[request],
            &writes,
            checksum_field,
        )?;
        Ok(saved.remove(0))
    }
}

fn descriptor_offset(
    slot: usize,
    table: LevelLayer2DescriptorTable,
) -> Result<usize, LevelLayer2IoError> {
    if table.entries == 0 || table.stride == 0 {
        return Err(LevelLayer2IoError::DescriptorLayout);
    }
    if slot >= table.entries {
        return Err(LevelLayer2IoError::DescriptorSlot {
            slot,
            entries: table.entries,
        });
    }
    slot.checked_mul(table.stride)
        .and_then(|relative| table.offset.checked_add(relative))
        .ok_or(LevelLayer2IoError::DescriptorOffsetOverflow)
}

fn read_layer2_descriptor(
    project: &Project,
    slot: usize,
    table: LevelLayer2DescriptorTable,
) -> Result<MwlLayer2Descriptor, LevelLayer2IoError> {
    let byte = project.rom.read(descriptor_offset(slot, table)?, 1)?[0];
    Ok(MwlLayer2Descriptor::from_raw(u32::from(byte)))
}

pub(crate) fn level_layer2_descriptor_write(
    project: &Project,
    slot: usize,
    descriptor: MwlLayer2Descriptor,
    table: LevelLayer2DescriptorTable,
) -> Result<RomWrite, LevelLayer2IoError> {
    let byte = u8::try_from(descriptor.raw())
        .map_err(|_| LevelLayer2IoError::DescriptorValue(descriptor.raw()))?;
    let offset = descriptor_offset(slot, table)?;
    project.rom.read(offset, 1)?;
    Ok(RomWrite {
        offset,
        bytes: vec![byte],
    })
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
            background_bank_substitution: None,
            legacy_pointer_redirect: None,
            descriptor_table: None,
            maximum_compressed_len: 0x8000,
            tilemap_encoding: LevelLayer2TilemapEncoding::SplitPlanes,
        }
    }

    #[test]
    fn legacy_background_expansion_leaves_native_page_gap_zeroed() {
        let tilemap =
            expand_legacy_layer2_tilemap(&vec![0x25; LEGACY_LAYER2_TILEMAP_LEN], 1).unwrap();
        for y in 0..32 {
            for x in 0..32 {
                let tile = lm_level::native_layer2_tilemap_index(x, y).unwrap();
                let expected = if tile < 0x1b0 || (0x200..0x3b0).contains(&tile) {
                    [0x25, 1]
                } else {
                    [0, 0]
                };
                assert_eq!(&tilemap[tile * 2..tile * 2 + 2], &expected);
            }
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
    fn installed_legacy_tilemap_uses_and_normalizes_its_descriptor() {
        let legacy = vec![0x34; LEGACY_LAYER2_TILEMAP_LEN];
        let compressed = encode_terminated_rle(&legacy);
        let mut bytes = vec![0xff; 0x8000];
        bytes[0x20..0x23]
            .copy_from_slice(&pc_to_snes(Mapper::LoRom, 0x100).unwrap().to_le_bytes()[..3]);
        bytes[0x40] = 0x18;
        bytes[0x100..0x100 + compressed.len()].copy_from_slice(&compressed);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let mut installed = layout(1);
        installed.tilemap_encoding = LevelLayer2TilemapEncoding::Legacy { high_byte: 0x7f };
        installed.descriptor_table = Some(LevelLayer2DescriptorTable {
            offset: 0x40,
            entries: 1,
            stride: 1,
        });

        let loaded = project
            .load_level_layer2_with_descriptor(0, 0, installed)
            .unwrap();
        let NativeLayer2Data::Tilemap(tilemap) = loaded.data else {
            panic!("level mode zero must decode as a tilemap");
        };
        assert_eq!(&tilemap[..4], &[0x34, 1, 0x34, 1]);
        assert_eq!(loaded.descriptor, Some(MwlLayer2Descriptor::from_raw(0x0c)));
    }

    #[test]
    fn installed_descriptor_accepts_pre_migration_legacy_padding_byte() {
        let mut legacy = vec![0x34; LEGACY_LAYER2_TILEMAP_LEN];
        legacy.push(0);
        let compressed = encode_terminated_rle(&legacy);
        let mut bytes = vec![0xff; 0x8000];
        bytes[0x20..0x23]
            .copy_from_slice(&pc_to_snes(Mapper::LoRom, 0x100).unwrap().to_le_bytes()[..3]);
        bytes[0x40] = 0x18;
        bytes[0x100..0x100 + compressed.len()].copy_from_slice(&compressed);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let mut installed = layout(1);
        installed.descriptor_table = Some(LevelLayer2DescriptorTable {
            offset: 0x40,
            entries: 1,
            stride: 1,
        });

        let loaded = project
            .load_level_layer2_with_descriptor(0, 0, installed)
            .unwrap();
        let NativeLayer2Data::Tilemap(tilemap) = loaded.data else {
            panic!("level mode zero must decode as a tilemap");
        };
        assert_eq!(&tilemap[..4], &[0x34, 1, 0x34, 1]);
        assert_eq!(loaded.descriptor, Some(MwlLayer2Descriptor::from_raw(0x0c)));
    }

    #[test]
    fn descriptor_tables_fail_closed_before_rom_access() {
        let project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        for table in [
            LevelLayer2DescriptorTable {
                offset: 0x40,
                entries: 0,
                stride: 1,
            },
            LevelLayer2DescriptorTable {
                offset: 0x40,
                entries: 1,
                stride: 0,
            },
        ] {
            assert!(matches!(
                read_layer2_descriptor(&project, 1, table),
                Err(LevelLayer2IoError::DescriptorLayout)
            ));
        }
        assert!(matches!(
            read_layer2_descriptor(
                &project,
                1,
                LevelLayer2DescriptorTable {
                    offset: 0x40,
                    entries: 1,
                    stride: 1,
                }
            ),
            Err(LevelLayer2IoError::DescriptorSlot {
                slot: 1,
                entries: 1
            })
        ));
        assert!(matches!(
            read_layer2_descriptor(
                &project,
                0,
                LevelLayer2DescriptorTable {
                    offset: 0x8000,
                    entries: 1,
                    stride: 1,
                }
            ),
            Err(LevelLayer2IoError::DescriptorRom(_))
        ));
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
