use super::{OverworldController, OverworldControllerError};
use crate::{ControllerSnapshot, EditorMode};
use lm_graphics::PaletteOwnership;
use lm_project::{
    CompleteOverworldRomLayout, LevelPointerTable, PayloadLoadError, PayloadReadPolicy, Project,
};
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomImage};

impl OverworldController {
    /// Loads every modeled native overworld domain and validates palette ownership and size modes.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldControllerError`] for wrong mode/mapper, a non-256-entry size table,
    /// ownership mismatch, or any domain/layout/payload failure.
    pub fn decode(
        snapshot: &ControllerSnapshot,
        slot: usize,
        layout: CompleteOverworldRomLayout,
        double_size_modes: &[bool],
        palette_ownership: PaletteOwnership,
    ) -> Result<Self, OverworldControllerError> {
        if snapshot.mode != EditorMode::Overworld {
            return Err(OverworldControllerError::WrongMode(snapshot.mode));
        }
        if snapshot.identity.mapper != layout.layers.mapper {
            return Err(OverworldControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: layout.layers.mapper,
            });
        }
        let modes: [bool; 256] = double_size_modes
            .try_into()
            .map_err(|_| OverworldControllerError::SizeModeCount(double_size_modes.len()))?;
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(OverworldControllerError::Rom)?;
        let project = Project::new(image);
        let mut data = project
            .load_complete_overworld(slot, layout, &modes)
            .map_err(OverworldControllerError::Io)?;
        let previous_blocks = [
            snapshot_block(&project, layout.layers.layer1, slot, layout.layers.mapper)?,
            snapshot_block(&project, layout.layers.layer2, slot, layout.layers.mapper)?,
            snapshot_block(
                &project,
                layout.event_reveals.sources,
                slot,
                layout.event_reveals.mapper,
            )?,
            snapshot_block(
                &project,
                layout.event_reveals.destinations,
                slot,
                layout.event_reveals.mapper,
            )?,
            snapshot_block(
                &project,
                layout.endpoints.pointers,
                slot,
                layout.endpoints.mapper,
            )?,
            snapshot_block(
                &project,
                layout.messages.pointers,
                slot,
                layout.messages.mapper,
            )?,
            snapshot_block(
                &project,
                layout.sprites.pointers,
                slot,
                layout.sprites.mapper,
            )?,
            snapshot_block(
                &project,
                layout.palette.pointers,
                slot,
                layout.palette.mapper,
            )?,
            snapshot_block(
                &project,
                layout.animation.pointers,
                slot,
                layout.animation.mapper,
            )?,
        ];
        data.palette
            .apply_changes(&[], &palette_ownership)
            .map_err(|error| OverworldControllerError::Palette { command: 0, error })?;
        Ok(Self {
            revision: snapshot.revision,
            slot,
            layout,
            checksum_field_offset: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            double_size_modes: modes,
            palette_ownership,
            baseline: data.clone(),
            data,
            previous_blocks,
        })
    }
}

fn snapshot_block(
    project: &Project,
    table: LevelPointerTable,
    slot: usize,
    mapper: Mapper,
) -> Result<Option<RatsBlock>, OverworldControllerError> {
    let pointer_offset = table
        .pointer_offset(slot)
        .map_err(OverworldControllerError::Layout)?;
    match project.load_payload(pointer_offset, mapper, &PayloadReadPolicy::Tagged) {
        Ok(payload) => Ok(payload.block),
        Err(PayloadLoadError::PointerNotTagged { .. }) => Ok(None),
        Err(error) => Err(OverworldControllerError::Payload(error)),
    }
}
