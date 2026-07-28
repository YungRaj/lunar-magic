//! Pristine SMW level Layer 3 selection and stripe-image materialization.

use lm_project::{Project, VanillaMainEntrance};
use lm_rom::{Mapper, RomError, SnesPointer24};
use std::fmt;

/// Headerless PC offset of SMW's 45-entry level Layer 3 stripe-image pointer table.
pub const SMW_US_V1_LAYER3_IMAGE_POINTER_TABLE_OFFSET: usize = 0x29000;
/// Headerless PC offset of the 16 tileset × 3 setting behavior table.
pub const SMW_US_V1_LAYER3_BEHAVIOR_TABLE_OFFSET: usize = 0x1f88;
pub const SMW_US_V1_LAYER3_IMAGE_COUNT: usize = 45;
pub const SMW_US_V1_LAYER3_TILEMAP_SIDE: usize = 64;
pub const SMW_US_V1_LAYER3_TILEMAP_WORDS: usize =
    SMW_US_V1_LAYER3_TILEMAP_SIDE * SMW_US_V1_LAYER3_TILEMAP_SIDE;

const LAYER3_VRAM_BASE: usize = 0x5000;
const LAYER3_BLANK_WORD: u16 = 0x38fc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1Layer3Behavior {
    LowTide,
    HighTide,
    Static { code: u8 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1LevelLayer3 {
    pub setting: u8,
    pub image_index: usize,
    pub behavior: SmwUsV1Layer3Behavior,
    pub initial_x: i16,
    pub initial_y: i16,
    /// A 64×64 row-major BG tilemap, normalized from the SNES screen-block layout.
    pub tilemap: Vec<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1Layer3Error {
    InvalidSetting(u8),
    TilesetOutOfRange(u8),
    ImageIndexOutOfRange(usize),
    PointerEncoding(usize),
    PointerOutOfRange(SnesPointer24),
    TruncatedStripe { offset: usize, len: usize },
    InvalidStripeAddress(u16),
    StripeWriteOutOfRange { address: usize },
    Rom(RomError),
}

impl fmt::Display for SmwUsV1Layer3Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot materialize pristine level Layer 3: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1Layer3Error {}

impl From<RomError> for SmwUsV1Layer3Error {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

/// Materializes the pristine level Layer 3 selected by the main entrance and object tileset.
///
/// The upper two bits of the entrance's vertical-settings plane select one of three entries for
/// each of SMW's sixteen object tilesets. A zero selector disables level Layer 3.
///
/// # Errors
///
/// Rejects invalid selectors, tilesets, pointers, stripe commands, or truncated ROM data.
pub fn load_smw_us_v1_level_layer3(
    project: &Project,
    entrance: VanillaMainEntrance,
    object_tileset: u8,
) -> Result<Option<SmwUsV1LevelLayer3>, SmwUsV1Layer3Error> {
    let setting = entrance.vertical_settings >> 6;
    if setting == 0 {
        return Ok(None);
    }
    if setting > 3 {
        return Err(SmwUsV1Layer3Error::InvalidSetting(setting));
    }
    if object_tileset >= 16 {
        return Err(SmwUsV1Layer3Error::TilesetOutOfRange(object_tileset));
    }
    let image_index = usize::from(object_tileset) * 3 + usize::from(setting - 1);
    if image_index >= SMW_US_V1_LAYER3_IMAGE_COUNT {
        return Err(SmwUsV1Layer3Error::ImageIndexOutOfRange(image_index));
    }
    let behavior_code = project
        .rom
        .read(SMW_US_V1_LAYER3_BEHAVIOR_TABLE_OFFSET + image_index, 1)?[0];
    let behavior = match behavior_code {
        1 => SmwUsV1Layer3Behavior::LowTide,
        2 => SmwUsV1Layer3Behavior::HighTide,
        code => SmwUsV1Layer3Behavior::Static { code },
    };
    let (initial_x, initial_y) = match behavior {
        SmwUsV1Layer3Behavior::LowTide => (0, 112),
        SmwUsV1Layer3Behavior::HighTide => (0, 64),
        SmwUsV1Layer3Behavior::Static { code: 0x81 } if matches!(object_tileset, 1 | 3) => (0, -64),
        SmwUsV1Layer3Behavior::Static { .. } => (0, -48),
    };
    let pointer_offset = SMW_US_V1_LAYER3_IMAGE_POINTER_TABLE_OFFSET + image_index * 3;
    let pointer_bytes = project.rom.read(pointer_offset, 3)?;
    let pointer =
        SnesPointer24::decode(pointer_bytes).map_err(SmwUsV1Layer3Error::PointerEncoding)?;
    let stripe_offset = pointer
        .to_pc(Mapper::LoRom)
        .map_err(|_| SmwUsV1Layer3Error::PointerOutOfRange(pointer))?;
    let screen_blocks = decode_stripe_image(project, stripe_offset)?;
    Ok(Some(SmwUsV1LevelLayer3 {
        setting,
        image_index,
        behavior,
        initial_x,
        initial_y,
        tilemap: normalize_screen_blocks(&screen_blocks),
    }))
}

fn decode_stripe_image(
    project: &Project,
    mut cursor: usize,
) -> Result<Vec<u16>, SmwUsV1Layer3Error> {
    let mut tilemap = vec![LAYER3_BLANK_WORD; SMW_US_V1_LAYER3_TILEMAP_WORDS];
    loop {
        let first = read(project, cursor, 1)?[0];
        if first & 0x80 != 0 {
            return Ok(tilemap);
        }
        let header = read(project, cursor, 4)?;
        let address = u16::from_be_bytes([header[0], header[1]]);
        let vertical = header[2] & 0x80 != 0;
        let fixed = header[2] & 0x40 != 0;
        let byte_len = usize::from(u16::from_be_bytes([header[2], header[3]]) & 0x3fff) + 1;
        cursor += 4;
        let words = byte_len.div_ceil(2);
        let increment = if vertical { 32 } else { 1 };
        if fixed {
            let source = read(project, cursor, 2)?;
            let word = u16::from_le_bytes([source[0], source[1]]);
            write_stripe_words(&mut tilemap, address, words, increment, |_| word)?;
            cursor += 2;
        } else {
            let source = read(project, cursor, byte_len)?;
            write_stripe_words(&mut tilemap, address, words, increment, |index| {
                let lo = source[index * 2];
                let hi = source.get(index * 2 + 1).copied().unwrap_or_default();
                u16::from_le_bytes([lo, hi])
            })?;
            cursor += byte_len;
        }
    }
}

fn read(project: &Project, offset: usize, len: usize) -> Result<&[u8], SmwUsV1Layer3Error> {
    project
        .rom
        .read(offset, len)
        .map_err(|_| SmwUsV1Layer3Error::TruncatedStripe { offset, len })
}

fn write_stripe_words(
    tilemap: &mut [u16],
    address: u16,
    words: usize,
    increment: usize,
    mut source: impl FnMut(usize) -> u16,
) -> Result<(), SmwUsV1Layer3Error> {
    let start = usize::from(address)
        .checked_sub(LAYER3_VRAM_BASE)
        .ok_or(SmwUsV1Layer3Error::InvalidStripeAddress(address))?;
    for index in 0..words {
        let target = start + index * increment;
        let Some(word) = tilemap.get_mut(target) else {
            return Err(SmwUsV1Layer3Error::StripeWriteOutOfRange {
                address: LAYER3_VRAM_BASE + target,
            });
        };
        *word = source(index);
    }
    Ok(())
}

fn normalize_screen_blocks(screen_blocks: &[u16]) -> Vec<u16> {
    let mut row_major = vec![LAYER3_BLANK_WORD; SMW_US_V1_LAYER3_TILEMAP_WORDS];
    for y in 0..SMW_US_V1_LAYER3_TILEMAP_SIDE {
        for x in 0..SMW_US_V1_LAYER3_TILEMAP_SIDE {
            let block = (y / 32) * 2 + x / 32;
            let source = block * 0x400 + (y % 32) * 32 + x % 32;
            row_major[y * SMW_US_V1_LAYER3_TILEMAP_SIDE + x] = screen_blocks[source];
        }
    }
    row_major
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::SpriteLengthTable;
    use lm_rom::RomImage;

    #[test]
    fn pristine_level_102_materializes_high_tide_from_rom_tables() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let entrance = project
            .load_vanilla_main_entrance(0x102, crate::smw_us_v1_vanilla_entrance_layout())
            .unwrap();
        let layer3 = load_smw_us_v1_level_layer3(&project, entrance, 8)
            .unwrap()
            .unwrap();
        assert_eq!(layer3.setting, 2);
        assert_eq!(layer3.image_index, 25);
        assert_eq!(layer3.behavior, SmwUsV1Layer3Behavior::HighTide);
        assert_eq!(layer3.initial_y, 64);
        assert_eq!(layer3.tilemap.len(), 64 * 64);
        assert!(layer3.tilemap.iter().any(|word| *word != LAYER3_BLANK_WORD));

        for image_index in 0..SMW_US_V1_LAYER3_IMAGE_COUNT {
            let entrance = VanillaMainEntrance {
                vertical_settings: (u8::try_from(image_index % 3).unwrap() + 1) << 6,
                ..VanillaMainEntrance::default()
            };
            let tileset = u8::try_from(image_index / 3).unwrap();
            let decoded = load_smw_us_v1_level_layer3(&project, entrance, tileset)
                .unwrap()
                .unwrap();
            assert_eq!(decoded.image_index, image_index);
            assert_eq!(decoded.tilemap.len(), SMW_US_V1_LAYER3_TILEMAP_WORDS);
        }
    }

    #[test]
    fn zero_selector_has_no_level_layer3() {
        let project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
        assert_eq!(
            load_smw_us_v1_level_layer3(&project, VanillaMainEntrance::default(), 0).unwrap(),
            None
        );
    }

    #[test]
    fn diagnostic_pristine_layer3_matches_lunar_magic_cache_when_requested() {
        let (Ok(slot), Ok(cache_path)) = (
            std::env::var("LM_LEVEL_SLOT"),
            std::env::var("LM_LEVEL_LAYER3_CACHE"),
        ) else {
            return;
        };
        let slot = usize::from_str_radix(&slot, 16).unwrap();
        let project = Project::new(
            RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap(),
        );
        let level = project
            .load_level_slot(
                slot,
                crate::smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let entrance = project
            .load_vanilla_main_entrance(slot, crate::smw_us_v1_vanilla_entrance_layout())
            .unwrap();
        let layer3 =
            load_smw_us_v1_level_layer3(&project, entrance, level.layer1.header.object_tileset())
                .unwrap()
                .unwrap();
        let live = std::fs::read(cache_path).unwrap();
        let live = live
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .collect::<Vec<_>>();
        assert_eq!(live.len(), SMW_US_V1_LAYER3_TILEMAP_WORDS);
        let live = normalize_screen_blocks(&live);
        let differences = layer3
            .tilemap
            .iter()
            .zip(&live)
            .filter(|(rust, live)| rust != live)
            .count();
        eprintln!(
            "level {slot:03X} Layer 3 setting={} image={} behavior={:?} position=({}, {}) differences={differences}",
            layer3.setting, layer3.image_index, layer3.behavior, layer3.initial_x, layer3.initial_y,
        );
        assert_eq!(differences, 0);
    }
}
