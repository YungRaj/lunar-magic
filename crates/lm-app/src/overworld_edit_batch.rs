use crate::exanimation_controller::{ExAnimationControllerEditFailure, apply_animation_edits};
use crate::{OverworldControllerEdit, OverworldLayerId};
use lm_graphics::{PaletteBatchEditError, PaletteOwnership};
use lm_overworld::{EventRevealTable, OverworldEditError, OverworldSprite};
use lm_project::CompleteOverworldData;

#[derive(Debug)]
pub enum OverworldEditBatchError {
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
}

pub(crate) struct OverworldEditContext<'a> {
    pub sprite_record_len: usize,
    pub maximum_animation_records: usize,
    pub double_size_modes: &'a [bool; 256],
    pub palette_ownership: &'a PaletteOwnership,
}

pub(crate) fn apply_overworld_edit_batch(
    data: &mut CompleteOverworldData,
    edits: &[OverworldControllerEdit],
    context: &OverworldEditContext<'_>,
) -> Result<(), OverworldEditBatchError> {
    let mut staged = data.clone();
    staged
        .palette
        .apply_changes(&[], context.palette_ownership)
        .map_err(|error| OverworldEditBatchError::Palette { command: 0, error })?;
    for (command, edit) in edits.iter().enumerate() {
        apply_one(&mut staged, edit, command, context)?;
    }
    *data = staged;
    Ok(())
}

fn apply_one(
    staged: &mut CompleteOverworldData,
    edit: &OverworldControllerEdit,
    command: usize,
    context: &OverworldEditContext<'_>,
) -> Result<(), OverworldEditBatchError> {
    match edit {
        OverworldControllerEdit::SetLayerTile { layer, x, y, tile } => {
            let target = match layer {
                OverworldLayerId::Layer1 => &mut staged.layers.layer1,
                OverworldLayerId::Layer2 => &mut staged.layers.layer2,
            };
            target
                .set_tile(*x, *y, *tile)
                .map_err(|error| OverworldEditBatchError::Edit { command, error })?;
        }
        OverworldControllerEdit::ReplaceEventReveal { index, reveal } => {
            EventRevealTable {
                entries: vec![*reveal],
            }
            .validate()
            .map_err(|error| OverworldEditBatchError::Edit {
                command,
                error: OverworldEditError::EventReveal(error),
            })?;
            replace(&mut staged.event_reveals.entries, *index, *reveal)
                .map_err(|error| OverworldEditBatchError::Edit { command, error })?;
        }
        OverworldControllerEdit::RelocateEventReveals {
            selection,
            delta_x,
            delta_y,
        } => {
            staged
                .event_reveals
                .relocate_selection(selection, *delta_x, *delta_y)
                .map_err(|error| OverworldEditBatchError::Edit {
                    command,
                    error: OverworldEditError::EventRevealMove(error),
                })?;
        }
        OverworldControllerEdit::ReplaceEndpoint { index, endpoint } => {
            replace(&mut staged.endpoints, *index, *endpoint)
                .map_err(|error| OverworldEditBatchError::Edit { command, error })?;
        }
        OverworldControllerEdit::SetMessageTile {
            message,
            column,
            row,
            tile,
        } => {
            let len = staged.messages.len();
            staged
                .messages
                .get_mut(*message)
                .ok_or(OverworldEditBatchError::Edit {
                    command,
                    error: OverworldEditError::IndexOutOfBounds {
                        index: *message,
                        len,
                    },
                })?
                .set_tile(*column, *row, *tile)
                .map_err(|error| OverworldEditBatchError::Edit { command, error })?;
        }
        OverworldControllerEdit::ReplaceMessage { index, message } => {
            replace(&mut staged.messages, *index, message.clone())
                .map_err(|error| OverworldEditBatchError::Edit { command, error })?;
        }
        OverworldControllerEdit::ReplaceSprite { index, sprite } => {
            OverworldSprite::encode_all(std::slice::from_ref(sprite), context.sprite_record_len)
                .map_err(|error| OverworldEditBatchError::Edit {
                    command,
                    error: OverworldEditError::Sprite(error),
                })?;
            replace(&mut staged.sprites, *index, sprite.clone())
                .map_err(|error| OverworldEditBatchError::Edit { command, error })?;
        }
        OverworldControllerEdit::PaletteChanges(changes) => staged
            .palette
            .apply_changes(changes, context.palette_ownership)
            .map_err(|error| OverworldEditBatchError::Palette { command, error })?,
        OverworldControllerEdit::Animation(animation_edits) => apply_animation_edits(
            &mut staged.animation,
            animation_edits,
            context.maximum_animation_records,
            context.double_size_modes,
        )
        .map_err(
            |(animation_command, error)| OverworldEditBatchError::Animation {
                command,
                animation_command,
                error,
            },
        )?,
    }
    Ok(())
}

fn replace<T>(values: &mut [T], index: usize, value: T) -> Result<(), OverworldEditError> {
    let len = values.len();
    let target = values
        .get_mut(index)
        .ok_or(OverworldEditError::IndexOutOfBounds { index, len })?;
    *target = value;
    Ok(())
}
