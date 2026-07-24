use crate::{MwlExAnimationSection, MwlExAnimationSectionError};
use lm_graphics::{Bgr555, CompactExAnimation, Palette};
use lm_level::{MwlFile, MwlPaletteSection, MwlPaletteSectionError, MwlSectionKind};
use std::fmt;

/// Typed palette and `ExAnimation` content carried by one MWL level container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MwlOptionalLevelAssets {
    pub palette_metadata: [u32; 2],
    /// Native 257-word order: backdrop followed by Lunar Magic's rotated 256-color payload.
    pub palette: Palette,
    pub exanimation_metadata: [u32; 2],
    pub exanimation: Option<CompactExAnimation>,
}

impl MwlOptionalLevelAssets {
    pub const NATIVE_PALETTE_COLOR_COUNT: usize = 257;

    /// Decodes both optional-asset sections from one MWL container.
    ///
    /// # Errors
    ///
    /// Returns a typed palette or compact-animation shape error and requires complete consumption
    /// of a populated animation payload.
    pub fn decode(
        file: &MwlFile,
        maximum_animation_records: usize,
        double_size_modes: &[bool],
    ) -> Result<Self, MwlOptionalLevelAssetsError> {
        let palette = file.palette_section()?;
        let exanimation = MwlExAnimationSection::decode(
            file.section(MwlSectionKind::ExAnimation),
            maximum_animation_records,
            double_size_modes,
        )?;
        let mut colors = Vec::with_capacity(Self::NATIVE_PALETTE_COLOR_COUNT);
        colors.push(Bgr555(palette.backdrop));
        colors.extend(palette.stored_colors().iter().copied().map(Bgr555));
        Ok(Self {
            palette_metadata: palette.metadata,
            palette: Palette { colors },
            exanimation_metadata: exanimation.metadata,
            exanimation: exanimation.animation,
        })
    }

    /// Replaces both sections while preserving every unrelated MWL section and container field.
    ///
    /// # Errors
    ///
    /// Requires exactly 257 native palette colors and a canonically encodable compact animation.
    pub fn install_into(
        &self,
        file: &mut MwlFile,
        double_size_modes: &[bool],
    ) -> Result<(), MwlOptionalLevelAssetsError> {
        if self.palette.colors.len() != Self::NATIVE_PALETTE_COLOR_COUNT {
            return Err(MwlOptionalLevelAssetsError::WrongPaletteColorCount(
                self.palette.colors.len(),
            ));
        }
        let (backdrop, stored) = self
            .palette
            .colors
            .split_first()
            .ok_or(MwlOptionalLevelAssetsError::WrongPaletteColorCount(0))?;
        let stored: &[Bgr555; 256] = stored.try_into().map_err(|_| {
            MwlOptionalLevelAssetsError::WrongPaletteColorCount(self.palette.colors.len())
        })?;
        let palette = MwlPaletteSection::from_stored_order(
            self.palette_metadata,
            backdrop.0,
            stored.map(|color| color.0),
        );
        let exanimation = MwlExAnimationSection {
            metadata: self.exanimation_metadata,
            animation: self.exanimation.clone(),
        }
        .encode(double_size_modes)?;
        file.set_palette_section(&palette);
        file.set_section(MwlSectionKind::ExAnimation, exanimation);
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MwlOptionalLevelAssetsError {
    Palette(MwlPaletteSectionError),
    ExAnimation(MwlExAnimationSectionError),
    WrongPaletteColorCount(usize),
}

impl fmt::Display for MwlOptionalLevelAssetsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid MWL optional level assets: {self:?}")
    }
}

impl std::error::Error for MwlOptionalLevelAssetsError {}

impl From<MwlPaletteSectionError> for MwlOptionalLevelAssetsError {
    fn from(value: MwlPaletteSectionError) -> Self {
        Self::Palette(value)
    }
}

impl From<MwlExAnimationSectionError> for MwlOptionalLevelAssetsError {
    fn from(value: MwlExAnimationSectionError) -> Self {
        Self::ExAnimation(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::ExAnimationRecord;

    fn assets() -> MwlOptionalLevelAssets {
        MwlOptionalLevelAssets {
            palette_metadata: [7, 0x10_8031],
            palette: Palette {
                colors: (0_u16..257).map(Bgr555).collect(),
            },
            exanimation_metadata: [0, 0x10_97e9],
            exanimation: Some(CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: vec![
                    ExAnimationRecord::new(1, 0, 0, 0x100, false, &[0, 6], false).unwrap(),
                ],
            }),
        }
    }

    #[test]
    fn both_sections_round_trip_without_changing_unrelated_sections() {
        let modes = [false; 256];
        let mut file = MwlFile::default();
        file.set_section(MwlSectionKind::Layer1, vec![0xaa, 0xbb]);
        let expected = assets();
        expected.install_into(&mut file, &modes).unwrap();
        let unrelated = file.section(MwlSectionKind::Layer1).to_vec();
        assert_eq!(
            MwlOptionalLevelAssets::decode(&file, 32, &modes).unwrap(),
            expected
        );
        assert_eq!(file.section(MwlSectionKind::Layer1), unrelated);
    }

    #[test]
    fn wrong_palette_shape_is_rejected_before_either_section_changes() {
        let modes = [false; 256];
        let mut file = MwlFile::default();
        file.set_section(MwlSectionKind::Palette, vec![1, 2, 3]);
        file.set_section(MwlSectionKind::ExAnimation, vec![4, 5, 6]);
        let before = file.clone();
        let mut invalid = assets();
        invalid.palette.colors.pop();
        assert!(matches!(
            invalid.install_into(&mut file, &modes),
            Err(MwlOptionalLevelAssetsError::WrongPaletteColorCount(256))
        ));
        assert_eq!(file, before);
    }

    #[test]
    fn empty_animation_section_remains_distinct_from_an_active_payload() {
        let modes = [false; 256];
        let mut expected = assets();
        expected.exanimation = None;
        expected.exanimation_metadata = [0; 2];
        let mut file = MwlFile::default();
        expected.install_into(&mut file, &modes).unwrap();
        assert_eq!(file.section(MwlSectionKind::ExAnimation), &[0; 8]);
        assert_eq!(
            MwlOptionalLevelAssets::decode(&file, 32, &modes).unwrap(),
            expected
        );
    }
}
