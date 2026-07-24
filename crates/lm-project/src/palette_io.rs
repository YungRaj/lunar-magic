use crate::{
    InstalledLayout, InstalledLayoutError, LevelLoadError, LevelPointerTable, PayloadLoadError,
    PayloadReadPolicy, PayloadReclamation, PayloadSaveError, PayloadSaveRequest, PayloadSaveResult,
    Project,
};
use lm_graphics::{Palette, PaletteEncodingError};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaletteRomLayout {
    pub mapper: Mapper,
    pub pointers: LevelPointerTable,
    pub colors_per_palette: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaletteSaveOptions {
    pub allocation: AllocationPolicy,
    pub previous_block: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Debug)]
pub enum PaletteIoError {
    Layout(LevelLoadError),
    InvalidLayoutColorCount(usize),
    SizeOverflow,
    ColorCount { actual: usize, expected: usize },
    Load(PayloadLoadError),
    Decode(usize),
    Encode(PaletteEncodingError),
    Save(PayloadSaveError),
    Installation(InstalledLayoutError),
}

impl fmt::Display for PaletteIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "palette I/O failed: {self:?}")
    }
}

impl std::error::Error for PaletteIoError {}

impl From<LevelLoadError> for PaletteIoError {
    fn from(value: LevelLoadError) -> Self {
        Self::Layout(value)
    }
}

impl From<PayloadLoadError> for PaletteIoError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}

impl From<PayloadSaveError> for PaletteIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl From<InstalledLayoutError> for PaletteIoError {
    fn from(value: InstalledLayoutError) -> Self {
        Self::Installation(value)
    }
}

impl From<PaletteEncodingError> for PaletteIoError {
    fn from(value: PaletteEncodingError) -> Self {
        Self::Encode(value)
    }
}

impl Project {
    /// Loads a palette only when its Lunar Magic installation marker resolves.
    ///
    /// # Errors
    ///
    /// Returns a marker bounds error or the ordinary palette decoding errors.
    pub fn load_installed_palette(
        &self,
        palette_number: usize,
        installation: InstalledLayout<PaletteRomLayout>,
    ) -> Result<Option<Palette>, PaletteIoError> {
        installation
            .resolve(&self.rom)?
            .map(|layout| self.load_palette(palette_number, layout))
            .transpose()
    }

    /// Saves through the marker-selected palette table and refuses to invent an installation.
    ///
    /// # Errors
    ///
    /// Returns [`InstalledLayoutError::NotInstalled`] when no declared marker matches.
    pub fn save_installed_palette(
        &mut self,
        palette_number: usize,
        palette: &Palette,
        installation: InstalledLayout<PaletteRomLayout>,
        options: &PaletteSaveOptions,
    ) -> Result<PayloadSaveResult, PaletteIoError> {
        let layout = installation
            .resolve(&self.rom)?
            .ok_or(InstalledLayoutError::NotInstalled("per-level palette"))?;
        self.save_palette(palette_number, palette, layout, options)
    }

    /// Loads one fixed-shape SNES palette from vanilla data or a RATS relocation.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteIoError`] for invalid dimensions, tables, pointers, or color data.
    pub fn load_palette(
        &self,
        palette_number: usize,
        layout: PaletteRomLayout,
    ) -> Result<Palette, PaletteIoError> {
        let encoded_len = encoded_palette_len(layout.colors_per_palette)?;
        let payload = self.load_payload(
            layout.pointers.pointer_offset(palette_number)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: encoded_len },
        )?;
        Palette::decode_snes(&payload.bytes).map_err(PaletteIoError::Decode)
    }

    /// Saves one fixed-shape SNES palette through transactional RATS allocation.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteIoError`] when the color count differs from the layout or saving fails.
    pub fn save_palette(
        &mut self,
        palette_number: usize,
        palette: &Palette,
        layout: PaletteRomLayout,
        options: &PaletteSaveOptions,
    ) -> Result<PayloadSaveResult, PaletteIoError> {
        Ok(self.save_tagged_payload(&palette_save_request(
            palette_number,
            palette,
            layout,
            options,
        )?)?)
    }

    /// Saves a palette and repairs the SNES checksum in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteIoError`] when validation, allocation, mapping, or checksum repair fails.
    pub fn save_palette_with_checksum(
        &mut self,
        palette_number: usize,
        palette: &Palette,
        layout: PaletteRomLayout,
        checksum_field: usize,
        options: &PaletteSaveOptions,
    ) -> Result<PayloadSaveResult, PaletteIoError> {
        let request = palette_save_request(palette_number, palette, layout, options)?;
        Ok(self
            .save_tagged_payloads_with_checksum(
                &request.description,
                std::slice::from_ref(&request),
                checksum_field,
            )?
            .remove(0))
    }

    /// Saves, reclaims the exactly owned previous palette block, and repairs checksum atomically.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteIoError`] for validation, ownership, allocation, overlap, mapping, or
    /// checksum failure without mutation.
    pub fn save_palette_with_checksum_and_reclamation(
        &mut self,
        palette_number: usize,
        palette: &Palette,
        layout: PaletteRomLayout,
        options: &PaletteSaveOptions,
        reclamation: PayloadReclamation<'_>,
    ) -> Result<PayloadSaveResult, PaletteIoError> {
        let request = palette_save_request(palette_number, palette, layout, options)?;
        Ok(self
            .save_tagged_payloads_with_checksum_and_reclamation(
                &request.description,
                std::slice::from_ref(&request),
                reclamation.checksum_field,
                reclamation.manifest,
            )?
            .remove(0))
    }
}

pub(crate) fn palette_save_request(
    palette_number: usize,
    palette: &Palette,
    layout: PaletteRomLayout,
    options: &PaletteSaveOptions,
) -> Result<PayloadSaveRequest, PaletteIoError> {
    let _ = encoded_palette_len(layout.colors_per_palette)?;
    if palette.colors.len() != layout.colors_per_palette {
        return Err(PaletteIoError::ColorCount {
            actual: palette.colors.len(),
            expected: layout.colors_per_palette,
        });
    }
    let payload = palette.encode_snes()?;
    Ok(PayloadSaveRequest {
        description: format!("save palette {palette_number:02x}"),
        maximum_payload_len: payload.len(),
        payload,
        pointer: layout.pointers.pointer_offset(palette_number)?.into(),
        mapper: layout.mapper,
        allocation_policy: options.allocation.clone(),
        previous_block: options.previous_block.clone(),
        reuse_identical: options.reuse_identical,
        erase_fill: options.erase_fill,
    })
}

fn encoded_palette_len(colors: usize) -> Result<usize, PaletteIoError> {
    if colors == 0 {
        return Err(PaletteIoError::InvalidLayoutColorCount(colors));
    }
    colors.checked_mul(2).ok_or(PaletteIoError::SizeOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::Bgr555;
    use lm_rats::ProtectedRange;
    use lm_rom::RomImage;

    fn layout() -> PaletteRomLayout {
        PaletteRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x20,
                entries: 2,
                stride: 3,
            },
            colors_per_palette: 16,
        }
    }

    fn options() -> PaletteSaveOptions {
        PaletteSaveOptions {
            allocation: AllocationPolicy {
                search: 0x100..0x8000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![ProtectedRange(0x20..0x26)],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    fn palette() -> Palette {
        Palette {
            colors: (0_u16..16).map(Bgr555).collect(),
        }
    }

    #[test]
    fn palette_save_load_and_undo_are_lossless() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        project
            .save_palette(1, &palette(), layout(), &options())
            .unwrap();
        assert_eq!(project.load_palette(1, layout()).unwrap(), palette());
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn marker_gated_palette_absence_is_explicit_and_write_safe() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let installation = InstalledLayout::Alternatives {
            primary: crate::GatedLayout {
                marker: crate::InstallationMarker {
                    offset: 0x10,
                    expected: 0xc2,
                },
                layout: layout(),
            },
            fallback: None,
        };
        assert_eq!(
            project.load_installed_palette(0, installation).unwrap(),
            None
        );
        assert!(matches!(
            project.save_installed_palette(0, &palette(), installation, &options()),
            Err(PaletteIoError::Installation(
                InstalledLayoutError::NotInstalled("per-level palette")
            ))
        ));
        assert_eq!(project.save_snapshot(), original);
        assert_eq!(project.history.undo_len(), 0);
    }

    #[test]
    fn matching_palette_marker_enables_the_declared_table() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        project.rom.write(0x10, &[0xc2]).unwrap();
        let installation = InstalledLayout::Alternatives {
            primary: crate::GatedLayout {
                marker: crate::InstallationMarker {
                    offset: 0x10,
                    expected: 0xc2,
                },
                layout: layout(),
            },
            fallback: None,
        };
        project
            .save_installed_palette(1, &palette(), installation, &options())
            .unwrap();
        assert_eq!(
            project.load_installed_palette(1, installation).unwrap(),
            Some(palette())
        );
    }

    #[test]
    fn wrong_palette_shape_does_not_mutate_the_project() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let short = Palette {
            colors: vec![Bgr555(0); 15],
        };
        assert!(matches!(
            project.save_palette(0, &short, layout(), &options()),
            Err(PaletteIoError::ColorCount {
                actual: 15,
                expected: 16
            })
        ));
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn loads_an_untagged_fixed_palette() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x81, 0x80]);
        bytes[0x100..0x120].copy_from_slice(&palette().encode_snes().unwrap());
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert_eq!(project.load_palette(0, layout()).unwrap(), palette());
    }

    #[test]
    fn tagged_palette_payload_must_match_revision_shape_exactly() {
        for payload in [vec![0; 30], vec![0; 34], vec![0; 31]] {
            let mut bytes = vec![0xff; 0x8000];
            let block = {
                let mut allocator = lm_rats::FreeSpaceAllocator::new(
                    &mut bytes,
                    AllocationPolicy {
                        search: 0x100..0x8000,
                        bank_size: Some(0x8000),
                        fill_bytes: vec![0xff],
                        protected: vec![ProtectedRange(0x20..0x26)],
                    },
                );
                allocator.allocate(&payload).unwrap()
            };
            let pointer = lm_rom::pc_to_snes(Mapper::LoRom, block.payload.start)
                .unwrap()
                .to_le_bytes();
            bytes[0x20..0x23].copy_from_slice(&pointer[..3]);
            let project = Project::new(RomImage::from_bytes(bytes).unwrap());
            assert!(matches!(
                project.load_palette(0, layout()),
                Err(PaletteIoError::Load(PayloadLoadError::TaggedLengthMismatch {
                    actual,
                    expected: 32,
                    ..
                })) if actual == payload.len()
            ));
        }
    }

    #[test]
    fn zero_color_layout_is_rejected_before_read_or_save() {
        let mut zero = layout();
        zero.colors_per_palette = 0;
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        assert!(matches!(
            project.load_palette(0, zero),
            Err(PaletteIoError::InvalidLayoutColorCount(0))
        ));
        assert!(matches!(
            project.save_palette(0, &Palette { colors: vec![] }, zero, &options(),),
            Err(PaletteIoError::InvalidLayoutColorCount(0))
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }
}
