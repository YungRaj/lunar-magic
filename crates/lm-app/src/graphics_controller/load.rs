use super::{GraphicsController, GraphicsControllerError};
use crate::{ControllerSnapshot, EditorMode};
use lm_graphics::{GraphicsFile4bpp, GraphicsOwnership};
use lm_project::{GraphicsRomLayout, PayloadReadPolicy, Project};
use lm_rats::RatsBlock;
use lm_rom::RomImage;

pub(super) fn load_graphics(
    snapshot: &ControllerSnapshot,
    layout: GraphicsRomLayout,
) -> Result<(usize, GraphicsFile4bpp, Option<RatsBlock>), GraphicsControllerError> {
    let EditorMode::Graphics(file_number) = snapshot.mode else {
        return Err(GraphicsControllerError::WrongMode(snapshot.mode));
    };
    if snapshot.identity.mapper != layout.mapper {
        return Err(GraphicsControllerError::MapperMismatch {
            snapshot: snapshot.identity.mapper,
            layout: layout.mapper,
        });
    }
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(GraphicsControllerError::Rom)?;
    let project = Project::new(image);
    let pointer = layout
        .pointers
        .pointer_offset(usize::from(file_number))
        .map_err(|error| GraphicsControllerError::Io(error.into()))?;
    let previous_block = project
        .load_payload(
            pointer,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrBounded {
                maximum_len: layout.maximum_compressed_len,
                bank_size: Some(0x8000),
            },
        )
        .map_err(|error| GraphicsControllerError::Io(error.into()))?
        .block;
    let graphics = project
        .load_graphics_file(usize::from(file_number), layout)
        .map_err(GraphicsControllerError::Io)?;
    Ok((usize::from(file_number), graphics, previous_block))
}

impl GraphicsController {
    /// Loads and decompresses the graphics file selected by the snapshot's editor mode.
    ///
    /// The supplied ownership map must describe every decoded tile exactly; even an empty edit
    /// validates this invariant so protected editor regions cannot be bypassed.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsControllerError`] for wrong mode/mapper, native I/O, decompression, tile
    /// decoding, or ownership-shape failure.
    pub fn decode(
        snapshot: &ControllerSnapshot,
        layout: GraphicsRomLayout,
        ownership: GraphicsOwnership,
    ) -> Result<Self, GraphicsControllerError> {
        let (file_number, mut graphics, previous_block) = load_graphics(snapshot, layout)?;
        graphics
            .apply_tile_changes(&[], &ownership)
            .map_err(|error| GraphicsControllerError::Edit { command: 0, error })?;
        Ok(Self {
            revision: snapshot.revision,
            file_number,
            layout,
            checksum_field_offset: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            baseline: graphics.clone(),
            graphics,
            ownership,
            previous_block,
        })
    }

    /// Loads a native graphics file with every decoded tile initially editable.
    ///
    /// This is the read/display boundary for frontends that have not yet materialized contextual
    /// fixed or `ExAnimation` ownership. Editing workflows should install their exact ownership map
    /// with [`Self::decode`] instead.
    ///
    /// # Errors
    ///
    /// Returns the same native layout, mapper, decompression, and tile errors as [`Self::decode`].
    pub fn decode_editable(
        snapshot: &ControllerSnapshot,
        layout: GraphicsRomLayout,
    ) -> Result<Self, GraphicsControllerError> {
        let (file_number, graphics, previous_block) = load_graphics(snapshot, layout)?;
        let ownership = GraphicsOwnership::editable(graphics.tiles.len());
        Ok(Self {
            revision: snapshot.revision,
            file_number,
            layout,
            checksum_field_offset: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            baseline: graphics.clone(),
            graphics,
            ownership,
            previous_block,
        })
    }
}
