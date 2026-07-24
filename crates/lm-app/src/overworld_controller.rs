use crate::exanimation_controller::ExAnimationControllerEditFailure;
use crate::overworld_edit_batch::{
    OverworldEditBatchError, OverworldEditContext, apply_overworld_edit_batch,
};
use crate::{EditorMode, ExAnimationControllerEdit};
use lm_graphics::{PaletteBatchEditError, PaletteChange, PaletteOwnership};
use lm_overworld::{
    EventReveal, OverworldEditError, OverworldEndpoint, OverworldMessage, OverworldSprite,
};
use lm_project::{
    CompleteOverworldData, CompleteOverworldIoError, CompleteOverworldRomLayout, LevelLoadError,
    PayloadLoadError, TransactionError,
};
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomError};
use std::fmt;

mod commit;
mod load;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverworldLayerId {
    Layer1,
    Layer2,
}

/// One ordered fixed-shape mutation in the complete native overworld model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldControllerEdit {
    SetLayerTile {
        layer: OverworldLayerId,
        x: usize,
        y: usize,
        tile: u16,
    },
    ReplaceEventReveal {
        index: usize,
        reveal: EventReveal,
    },
    ReplaceEndpoint {
        index: usize,
        endpoint: OverworldEndpoint,
    },
    SetMessageTile {
        message: usize,
        column: usize,
        row: usize,
        tile: u8,
    },
    ReplaceMessage {
        index: usize,
        message: OverworldMessage,
    },
    ReplaceSprite {
        index: usize,
        sprite: OverworldSprite,
    },
    PaletteChanges(Vec<PaletteChange>),
    Animation(Vec<ExAnimationControllerEdit>),
}

#[derive(Debug)]
pub enum OverworldControllerError {
    WrongMode(EditorMode),
    MapperMismatch {
        snapshot: Mapper,
        layout: Mapper,
    },
    SizeModeCount(usize),
    Rom(RomError),
    Io(CompleteOverworldIoError),
    Layout(LevelLoadError),
    Payload(PayloadLoadError),
    Edit {
        command: usize,
        error: OverworldEditError,
    },
    Palette {
        command: usize,
        error: PaletteBatchEditError,
    },
    Animation {
        command: usize,
        animation_command: usize,
        error: ExAnimationControllerEditFailure,
    },
    Mutation(TransactionError),
}

impl fmt::Display for OverworldControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "overworld controller failed: {self:?}")
    }
}

impl std::error::Error for OverworldControllerError {}

/// All nine modeled native overworld payloads decoded from one immutable snapshot.
#[derive(Clone, Debug)]
pub struct OverworldController {
    revision: u64,
    slot: usize,
    layout: CompleteOverworldRomLayout,
    checksum_field_offset: usize,
    source_file_bytes: Vec<u8>,
    double_size_modes: [bool; 256],
    palette_ownership: PaletteOwnership,
    baseline: CompleteOverworldData,
    data: CompleteOverworldData,
    previous_blocks: [Option<RatsBlock>; 9],
}

impl OverworldController {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn data(&self) -> &CompleteOverworldData {
        &self.data
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.data != self.baseline
    }

    /// Applies mixed fixed-shape edits across every modeled domain on one staged aggregate clone.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldControllerError`] with the outer and, for animation, nested command
    /// indexes. Any failure rolls back changes made to every other domain in this call.
    pub fn apply_edits(
        &mut self,
        edits: &[OverworldControllerEdit],
    ) -> Result<(), OverworldControllerError> {
        apply_overworld_edit_batch(
            &mut self.data,
            edits,
            &OverworldEditContext {
                sprite_record_len: self.layout.sprites.record_len,
                maximum_animation_records: self.layout.animation.maximum_records,
                double_size_modes: &self.double_size_modes,
                palette_ownership: &self.palette_ownership,
            },
        )
        .map_err(map_batch_error)
    }
}

fn map_batch_error(error: OverworldEditBatchError) -> OverworldControllerError {
    match error {
        OverworldEditBatchError::Edit { command, error } => {
            OverworldControllerError::Edit { command, error }
        }
        OverworldEditBatchError::Palette { command, error } => {
            OverworldControllerError::Palette { command, error }
        }
        OverworldEditBatchError::Animation {
            command,
            animation_command,
            error,
        } => OverworldControllerError::Animation {
            command,
            animation_command,
            error,
        },
    }
}

#[cfg(test)]
#[path = "overworld_controller_tests.rs"]
mod tests;
