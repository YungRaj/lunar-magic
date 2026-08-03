use super::{OverworldAppearanceDocumentEdit, OverworldAppearanceEditError};
use lm_overworld::{SpriteAppearanceDefinition, SpriteAppearancePart};

pub(super) fn apply_edit(
    definitions: &mut Vec<SpriteAppearanceDefinition>,
    edit: &OverworldAppearanceDocumentEdit,
) -> Result<(), OverworldAppearanceEditError> {
    match edit {
        OverworldAppearanceDocumentEdit::InsertDefinition { index, sprite_id } => {
            if definitions
                .iter()
                .any(|value| value.sprite_id == *sprite_id)
            {
                return Err(OverworldAppearanceEditError::DuplicateSpriteId(*sprite_id));
            }
            if *index > definitions.len() {
                return Err(index_error(*index, definitions.len()));
            }
            definitions.insert(
                *index,
                SpriteAppearanceDefinition {
                    sprite_id: *sprite_id,
                    parts: Vec::new(),
                },
            );
        }
        OverworldAppearanceDocumentEdit::RemoveDefinition { sprite_id } => {
            definitions.remove(definition_index(definitions, *sprite_id)?);
        }
        OverworldAppearanceDocumentEdit::MoveDefinitionBefore { sprite_id, before } => {
            let from = definition_index(definitions, *sprite_id)?;
            let destination = before
                .map(|id| definition_index(definitions, id))
                .transpose()?
                .unwrap_or(definitions.len());
            if from == destination || from.checked_add(1) == Some(destination) {
                return Ok(());
            }
            let value = definitions.remove(from);
            definitions.insert(
                if from < destination {
                    destination - 1
                } else {
                    destination
                },
                value,
            );
        }
        OverworldAppearanceDocumentEdit::InsertPart {
            sprite_id,
            index,
            value,
        } => {
            let parts = parts_mut(definitions, *sprite_id)?;
            if *index > parts.len() {
                return Err(index_error(*index, parts.len()));
            }
            parts.insert(*index, *value);
        }
        OverworldAppearanceDocumentEdit::ReplacePart {
            sprite_id,
            index,
            value,
        } => {
            let parts = parts_mut(definitions, *sprite_id)?;
            let len = parts.len();
            *parts
                .get_mut(*index)
                .ok_or_else(|| index_error(*index, len))? = *value;
        }
        OverworldAppearanceDocumentEdit::ReplaceParts { sprite_id, values } => {
            *parts_mut(definitions, *sprite_id)? = values.clone();
        }
        OverworldAppearanceDocumentEdit::TranslateParts {
            sprite_id,
            delta_x,
            delta_y,
        } => {
            let parts = parts_mut(definitions, *sprite_id)?;
            let translated = parts
                .iter()
                .copied()
                .enumerate()
                .map(|(index, mut part)| {
                    part.x_offset =
                        translated_offset(*sprite_id, index, "x", part.x_offset, *delta_x)?;
                    part.y_offset =
                        translated_offset(*sprite_id, index, "y", part.y_offset, *delta_y)?;
                    Ok(part)
                })
                .collect::<Result<Vec<_>, _>>()?;
            *parts = translated;
        }
        OverworldAppearanceDocumentEdit::MovePartBefore {
            sprite_id,
            index,
            before,
        } => {
            let parts = parts_mut(definitions, *sprite_id)?;
            let len = parts.len();
            if *index >= len {
                return Err(index_error(*index, len));
            }
            let destination = before.unwrap_or(len);
            if destination > len {
                return Err(index_error(destination, len));
            }
            if *index == destination || index.checked_add(1) == Some(destination) {
                return Ok(());
            }
            let value = parts.remove(*index);
            parts.insert(
                if *index < destination {
                    destination - 1
                } else {
                    destination
                },
                value,
            );
        }
        OverworldAppearanceDocumentEdit::RemovePart { sprite_id, index } => {
            let parts = parts_mut(definitions, *sprite_id)?;
            if *index >= parts.len() {
                return Err(index_error(*index, parts.len()));
            }
            parts.remove(*index);
        }
    }
    Ok(())
}

fn translated_offset(
    sprite_id: u16,
    index: usize,
    axis: &'static str,
    offset: i16,
    delta: i32,
) -> Result<i16, OverworldAppearanceEditError> {
    i32::from(offset)
        .checked_add(delta)
        .and_then(|value| i16::try_from(value).ok())
        .ok_or(OverworldAppearanceEditError::PartOffsetOverflow {
            sprite_id,
            index,
            axis,
            offset,
            delta,
        })
}

fn definition_index(
    definitions: &[SpriteAppearanceDefinition],
    sprite_id: u16,
) -> Result<usize, OverworldAppearanceEditError> {
    definitions
        .iter()
        .position(|value| value.sprite_id == sprite_id)
        .ok_or(OverworldAppearanceEditError::UnknownSpriteId(sprite_id))
}

fn parts_mut(
    definitions: &mut [SpriteAppearanceDefinition],
    sprite_id: u16,
) -> Result<&mut Vec<SpriteAppearancePart>, OverworldAppearanceEditError> {
    definitions
        .iter_mut()
        .find(|value| value.sprite_id == sprite_id)
        .map(|value| &mut value.parts)
        .ok_or(OverworldAppearanceEditError::UnknownSpriteId(sprite_id))
}

const fn index_error(index: usize, len: usize) -> OverworldAppearanceEditError {
    OverworldAppearanceEditError::IndexOutOfBounds { index, len }
}
