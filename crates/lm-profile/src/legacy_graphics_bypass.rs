use lm_level::{LegacyGraphicsBypassTable, LegacyGraphicsBypassTableError};
use lm_rom::{RomError, RomImage};
use std::fmt;

/// Logical PC offset corresponding to Lunar Magic 3.63's SMW-US descriptor entry `+$194`.
///
/// The original descriptor value is physical `$07F400`; subtracting the canonical `$200` copier
/// prefix produces this header-independent logical offset.
pub const SMW_US_V1_LEGACY_GRAPHICS_BYPASS_TABLE_OFFSET: usize = 0x7f200;

#[derive(Debug)]
pub enum SmwUsV1LegacyGraphicsBypassError {
    Rom(RomError),
    Table(LegacyGraphicsBypassTableError),
}

impl fmt::Display for SmwUsV1LegacyGraphicsBypassError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "SMW-US legacy graphics-bypass table failed: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1LegacyGraphicsBypassError {}

/// Loads the complete legacy foreground/background or sprite assignment list.
///
/// Both dialogs use the same descriptor-selected `$400`-byte storage shape; the command determines
/// how the four values are interpreted, not a second physical table.
pub fn load_smw_us_v1_legacy_graphics_bypass_table(
    image: &RomImage,
) -> Result<LegacyGraphicsBypassTable, SmwUsV1LegacyGraphicsBypassError> {
    LegacyGraphicsBypassTable::decode(
        image
            .read(
                SMW_US_V1_LEGACY_GRAPHICS_BYPASS_TABLE_OFFSET,
                LegacyGraphicsBypassTable::ENCODED_LEN,
            )
            .map_err(SmwUsV1LegacyGraphicsBypassError::Rom)?,
    )
    .map_err(SmwUsV1LegacyGraphicsBypassError::Table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::LegacyGraphicsAssignment;

    #[test]
    fn logical_offset_is_copier_header_independent_and_retains_all_rows() {
        let mut logical = vec![0; 0x80_000];
        let row = SMW_US_V1_LEGACY_GRAPHICS_BYPASS_TABLE_OFFSET + 5 * 4;
        logical[row..row + 4].copy_from_slice(&[3, 4, 2, 1]);
        let headerless = RomImage::from_bytes(logical.clone()).unwrap();
        let mut physical = vec![0x5a; 512];
        physical.extend_from_slice(&logical);
        let headered = RomImage::from_bytes(physical).unwrap();
        for image in [&headerless, &headered] {
            let table = load_smw_us_v1_legacy_graphics_bypass_table(image).unwrap();
            assert_eq!(
                table.entry(5).unwrap(),
                LegacyGraphicsAssignment([1, 2, 4, 3])
            );
        }
    }
}
