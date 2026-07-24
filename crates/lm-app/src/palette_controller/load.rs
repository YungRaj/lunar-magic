use super::{PaletteController, PaletteControllerError};
use crate::{ControllerSnapshot, EditorMode};
use lm_graphics::PaletteOwnership;
use lm_project::{PaletteIoError, PaletteRomLayout, PayloadReadPolicy, Project};
use lm_rom::RomImage;

impl PaletteController {
    /// Loads the palette selected by editor mode and validates exact ownership shape.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteControllerError`] for wrong mode/mapper, invalid native layout/data, or an
    /// ownership map that does not describe every decoded color.
    pub fn decode(
        snapshot: &ControllerSnapshot,
        layout: PaletteRomLayout,
        ownership: PaletteOwnership,
    ) -> Result<Self, PaletteControllerError> {
        let EditorMode::Palette(palette_number) = snapshot.mode else {
            return Err(PaletteControllerError::WrongMode(snapshot.mode));
        };
        if snapshot.identity.mapper != layout.mapper {
            return Err(PaletteControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: layout.mapper,
            });
        }
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(PaletteControllerError::Rom)?;
        let project = Project::new(image);
        let pointer = layout
            .pointers
            .pointer_offset(usize::from(palette_number))
            .map_err(|error| PaletteControllerError::Io(error.into()))?;
        let previous_block = project
            .load_payload(
                pointer,
                layout.mapper,
                &PayloadReadPolicy::TaggedOrFixed {
                    len: layout
                        .colors_per_palette
                        .checked_mul(2)
                        .ok_or(PaletteControllerError::Io(PaletteIoError::SizeOverflow))?,
                },
            )
            .map_err(|error| PaletteControllerError::Io(error.into()))?
            .block;
        let mut palette = project
            .load_palette(usize::from(palette_number), layout)
            .map_err(PaletteControllerError::Io)?;
        palette
            .apply_changes(&[], &ownership)
            .map_err(|error| PaletteControllerError::Edit { command: 0, error })?;
        Ok(Self {
            revision: snapshot.revision,
            palette_number: usize::from(palette_number),
            layout,
            checksum_field_offset: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            baseline: palette.clone(),
            palette,
            ownership,
            previous_block,
        })
    }
}
