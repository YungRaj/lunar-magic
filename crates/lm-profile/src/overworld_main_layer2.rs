//! Playable SMW US revision-0 main-overworld Layer 2 tilemap storage.

use lm_codec::{CodecError, decode_sized_rle_prefix, encode_interleaved_sized_rle};
use lm_overworld::{OverworldLayer, OverworldLayerEncodingError};
use lm_project::{
    PayloadPointer, PayloadSaveError, PayloadSaveRequest, PayloadSaveResult, Project, RomWrite,
};
use lm_rats::{AllocationPolicy, HEADER_LEN, HeaderError, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, pc_to_snes, snes_to_pc};
use std::fmt;

pub const SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH: usize = 128;
pub const SMW_US_V1_MAIN_OVERWORLD_LAYER2_HEIGHT: usize = 64;
pub const SMW_US_V1_MAIN_OVERWORLD_LAYER2_BYTES: usize = 0x4000;
pub const SMW_US_V1_OVERWORLD_LAYER2_PLANE_WIDTH: usize = 64;
pub const SMW_US_V1_OVERWORLD_LAYER2_PLANE_HEIGHT: usize = 64;
pub const SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD: usize = 0x02_5c72;
pub const SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK: usize = 0x02_5c79;
pub const SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD: usize = 0x02_5c8d;
pub const SMW_US_V1_MAIN_OVERWORLD_LAYER2_PRISTINE_LOW: usize = 0x02_2533;
pub const SMW_US_V1_MAIN_OVERWORLD_LAYER2_PRISTINE_HIGH: usize = 0x02_402b;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1MainOverworldLayer2Storage {
    Pristine,
    Installed(RatsBlock),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSmwUsV1MainOverworldLayer2 {
    pub layer: OverworldLayer,
    pub storage: SmwUsV1MainOverworldLayer2Storage,
    pub high_stream_offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1MainOverworldLayer2SaveOptions {
    pub allocation: AllocationPolicy,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Debug)]
pub enum SmwUsV1MainOverworldLayer2Error {
    Rom(RomError),
    Header(HeaderError),
    Codec(CodecError),
    Layer(OverworldLayerEncodingError),
    Save(PayloadSaveError),
    UnknownUntaggedStorage {
        low: usize,
        high: usize,
    },
    StreamBoundary {
        expected: usize,
        actual: usize,
    },
    InstalledExtent {
        expected: usize,
        actual: usize,
    },
    Shape {
        width: usize,
        height: usize,
        tiles: usize,
    },
    PointerBank {
        low: u32,
        high: u32,
    },
    AllocationPrediction {
        expected: usize,
        actual: usize,
    },
}

impl fmt::Display for SmwUsV1MainOverworldLayer2Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SMW US main-overworld Layer 2 failed: {self:?}")
    }
}

impl std::error::Error for SmwUsV1MainOverworldLayer2Error {}

impl From<RomError> for SmwUsV1MainOverworldLayer2Error {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<HeaderError> for SmwUsV1MainOverworldLayer2Error {
    fn from(value: HeaderError) -> Self {
        Self::Header(value)
    }
}

impl From<CodecError> for SmwUsV1MainOverworldLayer2Error {
    fn from(value: CodecError) -> Self {
        Self::Codec(value)
    }
}

impl From<OverworldLayerEncodingError> for SmwUsV1MainOverworldLayer2Error {
    fn from(value: OverworldLayerEncodingError) -> Self {
        Self::Layer(value)
    }
}

impl From<PayloadSaveError> for SmwUsV1MainOverworldLayer2Error {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

/// Loads the two gameplay-consumed LC_RLE2 planes and materializes `$7F4000-$7F7FFF`.
///
/// Untagged storage is accepted only at the two pristine US offsets. Relocated storage must have
/// one exact RATS owner whose payload begins at the low-byte stream and ends after the high-byte
/// stream.
pub fn load_smw_us_v1_main_overworld_layer2(
    project: &Project,
) -> Result<LoadedSmwUsV1MainOverworldLayer2, SmwUsV1MainOverworldLayer2Error> {
    let bytes = project.rom.logical_bytes();
    let low = split_pointer(bytes, SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD)?;
    let high = split_pointer(bytes, SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD)?;
    let owner = exact_owner(bytes, low)?;
    let storage = match owner {
        Some(block) => SmwUsV1MainOverworldLayer2Storage::Installed(block),
        None if low == SMW_US_V1_MAIN_OVERWORLD_LAYER2_PRISTINE_LOW
            && high == SMW_US_V1_MAIN_OVERWORLD_LAYER2_PRISTINE_HIGH =>
        {
            SmwUsV1MainOverworldLayer2Storage::Pristine
        }
        None => {
            return Err(SmwUsV1MainOverworldLayer2Error::UnknownUntaggedStorage { low, high });
        }
    };
    let low_plane = decode_sized_rle_prefix(bounded_stream(bytes, low)?, 0x2000)?;
    let expected_high = low
        .checked_add(low_plane.consumed)
        .ok_or(RomError::RangeOutOfBounds {
            offset: low,
            len: low_plane.consumed,
            image_len: bytes.len(),
        })?;
    if matches!(storage, SmwUsV1MainOverworldLayer2Storage::Installed(_)) && high != expected_high {
        return Err(SmwUsV1MainOverworldLayer2Error::StreamBoundary {
            expected: expected_high,
            actual: high,
        });
    }
    let high_plane = decode_sized_rle_prefix(bounded_stream(bytes, high)?, 0x2000)?;
    if let SmwUsV1MainOverworldLayer2Storage::Installed(block) = &storage {
        let actual = high + high_plane.consumed;
        if block.payload.end != actual {
            return Err(SmwUsV1MainOverworldLayer2Error::InstalledExtent {
                expected: block.payload.end,
                actual,
            });
        }
    }
    let mut decoded = Vec::with_capacity(SMW_US_V1_MAIN_OVERWORLD_LAYER2_BYTES);
    for (&low, &high) in low_plane.bytes.iter().zip(&high_plane.bytes) {
        decoded.push(low);
        decoded.push(high);
    }
    let storage_layer = OverworldLayer::decode_le(
        SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH,
        SMW_US_V1_MAIN_OVERWORLD_LAYER2_HEIGHT,
        &decoded,
    )
    .map_err(|_| SmwUsV1MainOverworldLayer2Error::Shape {
        width: SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH,
        height: SMW_US_V1_MAIN_OVERWORLD_LAYER2_HEIGHT,
        tiles: decoded.len() / 2,
    })?;
    let layer = OverworldLayer::new(
        SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH,
        SMW_US_V1_MAIN_OVERWORLD_LAYER2_HEIGHT,
        storage_to_canvas_tiles(&storage_layer.tiles),
    )
    .map_err(|tiles| SmwUsV1MainOverworldLayer2Error::Shape {
        width: SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH,
        height: SMW_US_V1_MAIN_OVERWORLD_LAYER2_HEIGHT,
        tiles: tiles.len(),
    })?;
    Ok(LoadedSmwUsV1MainOverworldLayer2 {
        layer,
        storage,
        high_stream_offset: high,
    })
}

/// Replaces the complete playable Layer 2 tilemap, repoints both runtime streams, and repairs the
/// checksum as one commit.
pub fn save_smw_us_v1_main_overworld_layer2(
    project: &mut Project,
    layer: &OverworldLayer,
    checksum_field: usize,
    options: &SmwUsV1MainOverworldLayer2SaveOptions,
) -> Result<PayloadSaveResult, SmwUsV1MainOverworldLayer2Error> {
    if layer.width != SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH
        || layer.height != SMW_US_V1_MAIN_OVERWORLD_LAYER2_HEIGHT
        || layer.tiles.len()
            != SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH * SMW_US_V1_MAIN_OVERWORLD_LAYER2_HEIGHT
    {
        return Err(SmwUsV1MainOverworldLayer2Error::Shape {
            width: layer.width,
            height: layer.height,
            tiles: layer.tiles.len(),
        });
    }
    let loaded = load_smw_us_v1_main_overworld_layer2(project)?;
    let storage = OverworldLayer::new(
        SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH,
        SMW_US_V1_MAIN_OVERWORLD_LAYER2_HEIGHT,
        canvas_to_storage_tiles(&layer.tiles),
    )
    .map_err(|tiles| SmwUsV1MainOverworldLayer2Error::Shape {
        width: SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH,
        height: SMW_US_V1_MAIN_OVERWORLD_LAYER2_HEIGHT,
        tiles: tiles.len(),
    })?;
    let bytes = storage.encode_le()?;
    let payload = encode_interleaved_sized_rle(&bytes)?;
    let low_stream_len = lm_codec::decode_sized_rle_prefix(&payload, 0x2000)?.consumed;
    let previous_block = match loaded.storage {
        SmwUsV1MainOverworldLayer2Storage::Pristine => None,
        SmwUsV1MainOverworldLayer2Storage::Installed(block) => Some(block),
    };
    let mut allocation = options.allocation.clone();
    allocation.bank_size = Some(0x8000);
    let request = PayloadSaveRequest {
        description: "save playable main-overworld Layer 2".into(),
        payload,
        pointer: PayloadPointer::Split {
            low_word_offset: SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD,
            bank_offset: SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK,
            shared_bank: false,
        },
        mapper: Mapper::LoRom,
        allocation_policy: allocation,
        previous_block,
        reuse_identical: options.reuse_identical,
        maximum_payload_len: 0x8000,
        erase_fill: options.erase_fill,
    };

    // Predict the deterministic allocation so the dependent high-plane pointer can participate in
    // the real save's single atomic transaction.
    let mut prediction = project.clone();
    let predicted = prediction
        .save_tagged_payloads("predict main-overworld Layer 2", &[request.clone()])?
        .remove(0);
    let high_pc = predicted.block.payload.start + low_stream_len;
    let high_snes = pc_to_snes(Mapper::LoRom, high_pc)?;
    if predicted.snes_pointer >> 16 != high_snes >> 16 {
        return Err(SmwUsV1MainOverworldLayer2Error::PointerBank {
            low: predicted.snes_pointer,
            high: high_snes,
        });
    }
    let high_bytes = high_snes.to_le_bytes();
    let mut staged = project.clone();
    let mut saved = staged.save_tagged_payloads_with_checksum_and_writes(
        "save playable main-overworld Layer 2",
        &[request],
        &[RomWrite {
            offset: SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD,
            bytes: high_bytes[..2].to_vec(),
        }],
        checksum_field,
    )?;
    let saved = saved.remove(0);
    if saved.block.payload.start != predicted.block.payload.start {
        return Err(SmwUsV1MainOverworldLayer2Error::AllocationPrediction {
            expected: predicted.block.payload.start,
            actual: saved.block.payload.start,
        });
    }
    *project = staged;
    Ok(saved)
}

fn storage_to_canvas_tiles(storage: &[u16]) -> Vec<u16> {
    let plane_len =
        SMW_US_V1_OVERWORLD_LAYER2_PLANE_WIDTH * SMW_US_V1_OVERWORLD_LAYER2_PLANE_HEIGHT;
    let mut canvas = vec![0; storage.len()];
    for plane in 0..2 {
        for y in 0..SMW_US_V1_OVERWORLD_LAYER2_PLANE_HEIGHT {
            for x in 0..SMW_US_V1_OVERWORLD_LAYER2_PLANE_WIDTH {
                let screen = y / 32 * 2 + x / 32;
                let storage_index = plane * plane_len + screen * 0x400 + y % 32 * 32 + x % 32;
                let canvas_index = y * SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH
                    + plane * SMW_US_V1_OVERWORLD_LAYER2_PLANE_WIDTH
                    + x;
                canvas[canvas_index] = storage[storage_index];
            }
        }
    }
    canvas
}

fn canvas_to_storage_tiles(canvas: &[u16]) -> Vec<u16> {
    let plane_len =
        SMW_US_V1_OVERWORLD_LAYER2_PLANE_WIDTH * SMW_US_V1_OVERWORLD_LAYER2_PLANE_HEIGHT;
    let mut storage = vec![0; canvas.len()];
    for plane in 0..2 {
        for y in 0..SMW_US_V1_OVERWORLD_LAYER2_PLANE_HEIGHT {
            for x in 0..SMW_US_V1_OVERWORLD_LAYER2_PLANE_WIDTH {
                let screen = y / 32 * 2 + x / 32;
                let storage_index = plane * plane_len + screen * 0x400 + y % 32 * 32 + x % 32;
                let canvas_index = y * SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH
                    + plane * SMW_US_V1_OVERWORLD_LAYER2_PLANE_WIDTH
                    + x;
                storage[storage_index] = canvas[canvas_index];
            }
        }
    }
    storage
}

fn split_pointer(
    bytes: &[u8],
    word_offset: usize,
) -> Result<usize, SmwUsV1MainOverworldLayer2Error> {
    let word = bytes
        .get(word_offset..word_offset + 2)
        .ok_or(RomError::RangeOutOfBounds {
            offset: word_offset,
            len: 2,
            image_len: bytes.len(),
        })?;
    let bank =
        *bytes
            .get(SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK)
            .ok_or(RomError::RangeOutOfBounds {
                offset: SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK,
                len: 1,
                image_len: bytes.len(),
            })?;
    Ok(snes_to_pc(
        Mapper::LoRom,
        u32::from_le_bytes([word[0], word[1], bank, 0]),
    )?)
}

fn exact_owner(
    bytes: &[u8],
    payload: usize,
) -> Result<Option<RatsBlock>, SmwUsV1MainOverworldLayer2Error> {
    let Some(header) = payload.checked_sub(HEADER_LEN) else {
        return Ok(None);
    };
    match parse_at(bytes, header) {
        Ok(block) if block.payload.start == payload => Ok(Some(block)),
        Ok(_) | Err(HeaderError::Signature) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn bounded_stream(bytes: &[u8], offset: usize) -> Result<&[u8], SmwUsV1MainOverworldLayer2Error> {
    let end = (offset | 0x7fff).saturating_add(1).min(bytes.len());
    bytes.get(offset..end).ok_or_else(|| {
        RomError::RangeOutOfBounds {
            offset,
            len: end.saturating_sub(offset),
            image_len: bytes.len(),
        }
        .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SMW_US_V1_CHECKSUM_FIELD;
    use lm_rats::ProtectedRange;
    use lm_rom::{RomImage, compute_snes_checksum};
    use std::{fs, path::Path};

    fn fixture(name: &str) -> Project {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/overworld-transfer-positive")
            .join(name);
        Project::new(RomImage::from_bytes(fs::read(path).unwrap()).unwrap())
    }

    fn options() -> SmwUsV1MainOverworldLayer2SaveOptions {
        SmwUsV1MainOverworldLayer2SaveOptions {
            allocation: AllocationPolicy {
                search: 0x80_000..0x10_0000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![
                    ProtectedRange(0x7fc0..0x8000),
                    ProtectedRange(
                        SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD
                            ..SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD + 2,
                    ),
                    ProtectedRange(
                        SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK
                            ..SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK + 1,
                    ),
                    ProtectedRange(
                        SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD
                            ..SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD + 2,
                    ),
                ],
            },
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    #[test]
    fn runtime_planes_are_presented_side_by_side_without_row_interleaving() {
        let storage = (0_u16..0x2000).collect::<Vec<_>>();
        let canvas = storage_to_canvas_tiles(&storage);
        assert_eq!(canvas.len(), storage.len());
        assert_eq!(&canvas[..32], &storage[..32]);
        assert_eq!(&canvas[32..64], &storage[0x400..0x420]);
        assert_eq!(&canvas[64..96], &storage[0x1000..0x1020]);
        assert_eq!(&canvas[96..128], &storage[0x1400..0x1420]);
        assert_eq!(&canvas[128..160], &storage[32..64]);
        assert_eq!(&canvas[32 * 128..32 * 128 + 32], &storage[0x800..0x820]);
        assert_eq!(canvas_to_storage_tiles(&canvas), storage);
    }

    #[test]
    fn pristine_and_lunar_magic_installed_streams_have_identical_playable_tiles() {
        let before = load_smw_us_v1_main_overworld_layer2(&fixture("before.smc")).unwrap();
        let after = load_smw_us_v1_main_overworld_layer2(&fixture("after.smc")).unwrap();
        assert_eq!(before.layer, after.layer);
        assert_eq!(before.layer.tiles.len(), 0x2000);
        assert_eq!(before.layer.tiles[..4], [0x1c75; 4]);
        assert_eq!(before.storage, SmwUsV1MainOverworldLayer2Storage::Pristine);
        assert!(matches!(
            after.storage,
            SmwUsV1MainOverworldLayer2Storage::Installed(_)
        ));
    }

    #[test]
    fn complete_tile_edit_repoints_reopens_and_repairs_checksum() {
        let mut project = fixture("before.smc");
        let mut layer = load_smw_us_v1_main_overworld_layer2(&project)
            .unwrap()
            .layer;
        layer.tiles[0] ^= 1;
        project
            .expand_rom(Mapper::LoRom, 0x10_0000, 0xff, SMW_US_V1_CHECKSUM_FIELD)
            .unwrap();
        save_smw_us_v1_main_overworld_layer2(
            &mut project,
            &layer,
            SMW_US_V1_CHECKSUM_FIELD,
            &options(),
        )
        .unwrap();
        let reopened = load_smw_us_v1_main_overworld_layer2(&project).unwrap();
        assert_eq!(reopened.layer, layer);
        let checksum =
            compute_snes_checksum(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD).unwrap();
        assert_eq!(
            project.rom.read(SMW_US_V1_CHECKSUM_FIELD, 4).unwrap(),
            checksum.encoded()
        );
    }

    #[test]
    fn malformed_shape_is_atomic() {
        let mut project = fixture("before.smc");
        let snapshot = project.save_snapshot();
        let layer = OverworldLayer {
            width: 1,
            height: 1,
            tiles: vec![0],
        };
        assert!(matches!(
            save_smw_us_v1_main_overworld_layer2(
                &mut project,
                &layer,
                SMW_US_V1_CHECKSUM_FIELD,
                &options()
            ),
            Err(SmwUsV1MainOverworldLayer2Error::Shape { .. })
        ));
        assert_eq!(project.save_snapshot(), snapshot);
    }
}
