use std::collections::HashSet;

use super::{SpriteAppearanceDefinition, SpriteAppearanceFile, SpriteAppearanceFileError};

pub(super) fn encoded_len(
    definition_count: usize,
    part_count: usize,
) -> Result<usize, SpriteAppearanceFileError> {
    let definitions_len = definition_count
        .checked_mul(SpriteAppearanceFile::DEFINITION_LEN)
        .ok_or(SpriteAppearanceFileError::Overflow)?;
    let parts_len = part_count
        .checked_mul(SpriteAppearanceFile::PART_LEN)
        .ok_or(SpriteAppearanceFileError::Overflow)?;
    SpriteAppearanceFile::HEADER_LEN
        .checked_add(definitions_len)
        .and_then(|length| length.checked_add(parts_len))
        .ok_or(SpriteAppearanceFileError::Overflow)
}

pub(super) fn validate(
    definitions: &[SpriteAppearanceDefinition],
) -> Result<(), SpriteAppearanceFileError> {
    if definitions.len() > SpriteAppearanceFile::MAX_DEFINITIONS {
        return Err(SpriteAppearanceFileError::TooManyDefinitions(
            definitions.len(),
        ));
    }
    let mut ids = HashSet::new();
    let mut total = 0_usize;
    for definition in definitions {
        if !ids.insert(definition.sprite_id) {
            return Err(SpriteAppearanceFileError::DuplicateSpriteId(
                definition.sprite_id,
            ));
        }
        if definition.parts.len() > usize::from(u16::MAX) {
            return Err(SpriteAppearanceFileError::TooManyDefinitionParts {
                sprite_id: definition.sprite_id,
                count: definition.parts.len(),
            });
        }
        total = total
            .checked_add(definition.parts.len())
            .ok_or(SpriteAppearanceFileError::Overflow)?;
        for part in &definition.parts {
            if part.palette_index > 7 {
                return Err(SpriteAppearanceFileError::PaletteOutOfRange(
                    part.palette_index,
                ));
            }
        }
    }
    if total > SpriteAppearanceFile::MAX_PARTS {
        Err(SpriteAppearanceFileError::TooManyParts(total))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SpriteAppearancePart;

    #[test]
    fn validation_rejects_invalid_palette_before_serialization() {
        let definitions = [SpriteAppearanceDefinition {
            sprite_id: 1,
            parts: vec![SpriteAppearancePart {
                tile_index: 0,
                palette_index: 8,
                x_offset: 0,
                y_offset: 0,
                x_flip: false,
                y_flip: false,
            }],
        }];
        assert_eq!(
            validate(&definitions),
            Err(SpriteAppearanceFileError::PaletteOutOfRange(8))
        );
    }
}
