//! Pure composition of Lunar Magic's pristine 256-color level palette cache.

use crate::{
    SmwUsV1Layer3Behavior, SmwUsV1Layer3Error, load_smw_us_v1_level_layer3,
    smw_us_v1_shared_palette_layout, smw_us_v1_vanilla_entrance_layout,
};
use lm_graphics::{Bgr555, Palette, SmwPaletteFileError};
use lm_level::LegacyLevelHeader;
use lm_project::{Project, SharedPaletteIoError, VanillaEntranceIoError};
use std::fmt;

const CACHE_COLORS: usize = 257;
const CACHE_BYTES: usize = CACHE_COLORS * 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1LevelPalette {
    pub palette: Palette,
    pub backdrop: Bgr555,
}

#[derive(Debug)]
pub enum SmwUsV1LevelPaletteError {
    Shared(SharedPaletteIoError),
    SharedFile(SmwPaletteFileError),
    Entrance(VanillaEntranceIoError),
    Layer3(SmwUsV1Layer3Error),
    SourceRange { offset: usize, len: usize },
    PlayerPaletteOutOfRange(u8),
}

impl fmt::Display for SmwUsV1LevelPaletteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "pristine level palette composition failed: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1LevelPaletteError {}

impl From<SharedPaletteIoError> for SmwUsV1LevelPaletteError {
    fn from(value: SharedPaletteIoError) -> Self {
        Self::Shared(value)
    }
}

impl From<SmwPaletteFileError> for SmwUsV1LevelPaletteError {
    fn from(value: SmwPaletteFileError) -> Self {
        Self::SharedFile(value)
    }
}

impl From<VanillaEntranceIoError> for SmwUsV1LevelPaletteError {
    fn from(value: VanillaEntranceIoError) -> Self {
        Self::Entrance(value)
    }
}

impl From<SmwUsV1Layer3Error> for SmwUsV1LevelPaletteError {
    fn from(value: SmwUsV1Layer3Error) -> Self {
        Self::Layer3(value)
    }
}

/// Builds the normal-mode level palette from shared tables and proven header selectors.
///
/// `player_palette` selects one of the four ten-color player rows. Level 199 receives Lunar
/// Magic's recovered special foreground substitutions.
///
/// # Errors
///
/// Returns an error for malformed shared-palette storage, source ranges unavailable in that
/// storage, or a player-palette selector above three.
#[allow(clippy::too_many_lines)]
pub fn compose_smw_us_v1_level_palette(
    project: &Project,
    level: u16,
    header: LegacyLevelHeader,
    player_palette: u8,
) -> Result<SmwUsV1LevelPalette, SmwUsV1LevelPaletteError> {
    if player_palette > 3 {
        return Err(SmwUsV1LevelPaletteError::PlayerPaletteOutOfRange(
            player_palette,
        ));
    }
    let shared = project.load_shared_palette(smw_us_v1_shared_palette_layout())?;
    let source = shared.palette_bytes();
    let mut cache = [0; CACHE_BYTES];

    let background = usize::from(header.background_palette()) * 0x18;
    for (target, source_offset) in [
        (0x04, 0x10 + background),
        (0x08, 0x14 + background),
        (0x0c, 0x18 + background),
        (0x24, 0x1c + background),
        (0x28, 0x20 + background),
        (0x2c, 0x24 + background),
    ] {
        copy(&mut cache, target, source, source_offset, 4)?;
    }

    let foreground = usize::from(header.foreground_palette()) * 0x18;
    for (target, source_offset) in [
        (0x44, 0xf0 + foreground),
        (0x48, 0xf4 + foreground),
        (0x4c, 0xf8 + foreground),
        (0x64, 0xfc + foreground),
        (0x68, 0x100 + foreground),
        (0x6c, 0x104 + foreground),
    ] {
        copy(&mut cache, target, source, source_offset, 4)?;
    }

    let sprites = usize::from(header.sprite_palette()) * 0x18;
    for (target, source_offset) in [
        (0x1c4, 0x278 + sprites),
        (0x1c8, 0x27c + sprites),
        (0x1cc, 0x280 + sprites),
        (0x1e4, 0x284 + sprites),
        (0x1e8, 0x288 + sprites),
        (0x1ec, 0x28c + sprites),
    ] {
        copy(&mut cache, target, source, source_offset, 4)?;
    }

    for target in (0x02..0x102).step_by(0x20) {
        write_word(&mut cache, target, 0x7fdd);
    }
    for target in (0x102..0x202).step_by(0x20) {
        write_word(&mut cache, target, 0x7fff);
    }

    for group in 0..5 {
        let source_offset = 0x1b0 + group * 0x18;
        let target = 0x84 + group * 0x40;
        copy(&mut cache, target, source, source_offset, 12)?;
        copy(&mut cache, target + 0x20, source, source_offset + 12, 12)?;
    }

    for (target, source_offset) in [
        (0x10, 0xd0),
        (0x14, 0xd4),
        (0x18, 0xd8),
        (0x1c, 0xdc),
        (0x30, 0xe0),
        (0x34, 0xe4),
        (0x38, 0xe8),
        (0x3c, 0xec),
        (0x52, 0x5d4),
        (0x56, 0x5d8),
        (0x5a, 0x5dc),
        (0x5e, 0x5e0),
        (0x72, 0x5e2),
        (0x76, 0x5e6),
        (0x7a, 0x5ea),
        (0x7e, 0x5ee),
        (0x92, 0x5f0),
        (0x96, 0x5f4),
        (0x9a, 0x5f8),
        (0x9e, 0x5fc),
        (0x132, 0x5d4),
        (0x136, 0x5d8),
        (0x13a, 0x5dc),
        (0x13e, 0x5e0),
        (0x152, 0x5e2),
        (0x156, 0x5e6),
        (0x15a, 0x5ea),
        (0x15e, 0x5ee),
        (0x172, 0x5f0),
        (0x176, 0x5f4),
        (0x17a, 0x5f8),
        (0x17e, 0x5fc),
    ] {
        copy(&mut cache, target, source, source_offset, 4)?;
    }

    let entrance = project
        .load_vanilla_main_entrance(usize::from(level), smw_us_v1_vanilla_entrance_layout())?;
    let layer3 = load_smw_us_v1_level_layer3(project, entrance, header.object_tileset())?;
    if layer3
        .as_ref()
        .is_some_and(|layer3| layer3.behavior == SmwUsV1Layer3Behavior::Static { code: 0x80 })
    {
        // Ghidra InitializeLayer3ModeRenderingState @ 00464efd sets DAT_00600a8e
        // for behavior $80. BuildLevelPaletteEditorCaches then replaces CGRAM
        // colors $0C-$0F from the dedicated Layer 3 smash palette.
        copy(&mut cache, 0x18, source, 0x5cc, 8)?;
    }

    copy(
        &mut cache,
        0x10c,
        source,
        0x228 + usize::from(player_palette) * 0x14,
        0x14,
    )?;

    if level == 199 {
        for (target, source_offset) in [
            (0x10, 0x59c),
            (0x14, 0x5a0),
            (0x18, 0x5a4),
            (0x1c, 0x5a8),
            (0x30, 0x58c),
            (0x34, 0x590),
            (0x38, 0x594),
            (0x3c, 0x598),
        ] {
            copy(&mut cache, target, source, source_offset, 4)?;
        }
    }

    let background_color = usize::from(header.background_color()) * 2;
    let backdrop = read_word(source, background_color)?;
    write_word(&mut cache, 0x200, backdrop);
    let mut palette = Palette::decode_snes(&cache[2..]).map_err(SmwPaletteFileError::ColorData)?;
    palette.colors.rotate_right(1);
    for row in 1..16 {
        palette.colors[row * Palette::COLORS_PER_ROW] = Bgr555(0);
    }
    Ok(SmwUsV1LevelPalette {
        palette,
        backdrop: Bgr555(backdrop),
    })
}

fn copy(
    target: &mut [u8],
    target_offset: usize,
    source: &[u8],
    source_offset: usize,
    len: usize,
) -> Result<(), SmwUsV1LevelPaletteError> {
    let source_bytes = source.get(source_offset..source_offset + len).ok_or(
        SmwUsV1LevelPaletteError::SourceRange {
            offset: source_offset,
            len,
        },
    )?;
    target[target_offset..target_offset + len].copy_from_slice(source_bytes);
    Ok(())
}

fn read_word(source: &[u8], offset: usize) -> Result<u16, SmwUsV1LevelPaletteError> {
    let bytes = source
        .get(offset..offset + 2)
        .ok_or(SmwUsV1LevelPaletteError::SourceRange { offset, len: 2 })?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn write_word(target: &mut [u8], offset: usize, value: u16) {
    target[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smw_us_v1_vanilla_level_layout;
    use lm_graphics::TplPaletteFile;
    use lm_level::SpriteLengthTable;
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn level_zero_matches_lunar_magic_tpl_oracle() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let project = Project::new(
            RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap(),
        );
        let level = project
            .load_level_slot(
                0,
                smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let actual = compose_smw_us_v1_level_palette(&project, 0, level.layer1.header, 0).unwrap();
        let expected = TplPaletteFile::decode(
            &fs::read(
                root.join("oracle-work/lm363/pristine-us/palette-install-positive/level000.tpl"),
            )
            .unwrap(),
        )
        .unwrap();
        // Standalone TPL files serialize the backdrop after the 256 display colors, while the
        // editor/MWL cache exposes it as CGRAM color zero.
        let mut expected_colors = expected.palette.colors.clone();
        expected_colors.rotate_right(1);
        let differences: Vec<_> = actual
            .palette
            .colors
            .iter()
            .zip(&expected_colors)
            .enumerate()
            .filter_map(|(index, (actual, expected))| {
                (actual != expected).then_some((index, *actual, *expected))
            })
            .collect();
        assert_eq!(
            differences,
            [(2, Bgr555(0x1462), Bgr555(0x147d))],
            "the retained oracle intentionally edits this one shared color during installation"
        );
    }

    #[test]
    fn cookie_mountain_matches_lunar_magic_mwl_palette_oracle() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let project = Project::new(
            RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap(),
        );
        let level = project
            .load_level_slot(
                1,
                smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let actual = compose_smw_us_v1_level_palette(&project, 1, level.layer1.header, 0).unwrap();
        let mwl = lm_level::MwlFile::decode(
            &fs::read(root.join("oracle-work/lm363/pristine-us/levels/Level 001.mwl")).unwrap(),
        )
        .unwrap();
        let expected = mwl.palette_section().unwrap();
        let expected_colors = expected
            .tpl_order_colors()
            .into_iter()
            .map(Bgr555)
            .collect::<Vec<_>>();
        assert_eq!(actual.palette.colors, expected_colors);
    }
}
