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
