use crate::{LevelControllerError, NativeLevelEdit};
use lm_level::{
    HeaderValueError, LegacyHeaderEdit, LevelEditError, LevelObjectData, NativeSpriteHeader,
    NativeSpriteStream, SpriteLengthTable, SpriteToken,
};
use lm_project::LoadedLevelSlot;

pub(crate) fn apply_loaded_level_edits(
    level: &mut LoadedLevelSlot,
    edits: &[NativeLevelEdit],
    sprite_lengths: &SpriteLengthTable,
) -> Result<(), LevelControllerError> {
    let mut layer1 = level.layer1.clone();
    let mut sprites = level.sprites.clone();
    apply_native_level_edits(&mut layer1, &mut sprites, edits, sprite_lengths)?;
    level.layer1 = layer1;
    level.sprites = sprites;
    Ok(())
}

pub(crate) fn apply_native_level_edits(
    layer1: &mut LevelObjectData,
    sprites: &mut NativeSpriteStream,
    edits: &[NativeLevelEdit],
    sprite_lengths: &SpriteLengthTable,
) -> Result<(), LevelControllerError> {
    if edits.is_empty() {
        return Ok(());
    }
    let mut staged_layer1 = layer1.clone();
    let mut staged_sprites = sprites.clone();
    for (command, edit) in edits.iter().enumerate() {
        match edit {
            NativeLevelEdit::LegacyHeader(edit) => apply_header_edit(&mut staged_layer1, *edit)
                .map_err(|error| LevelControllerError::HeaderEdit { command, error })?,
            NativeLevelEdit::SetCustomTime(settings) => {
                let vertical =
                    lm_profile::smw_us_v1_level_mode(staged_layer1.header.level_mode()).vertical;
                staged_layer1
                    .objects
                    .set_custom_time(vertical, *settings)
                    .map_err(|error| LevelControllerError::CustomTimeEdit { command, error })?;
            }
            NativeLevelEdit::ClearObjects => staged_layer1.objects.records.clear(),
            NativeLevelEdit::Objects(edits) => staged_layer1
                .objects
                .apply_edits(edits)
                .map_err(|error| LevelControllerError::ObjectEdit { command, error })?,
            NativeLevelEdit::ClearSprites => staged_sprites.tokens.clear(),
            NativeLevelEdit::SetSpriteHeader(header) => staged_sprites.header = *header,
            NativeLevelEdit::SetSpriteHeaderProperties {
                memory,
                buoyancy_1,
                buoyancy_2,
            } => {
                staged_sprites.header = NativeSpriteHeader::from_raw(staged_sprites.header)
                    .with_properties(*memory, *buoyancy_1, *buoyancy_2)
                    .map(NativeSpriteHeader::raw)
                    .map_err(|error| LevelControllerError::SpriteHeaderEdit { command, error })?;
            }
            NativeLevelEdit::SetSpriteFields { index, fields } => {
                let vertical =
                    lm_profile::smw_us_v1_level_mode(staged_layer1.header.level_mode()).vertical;
                staged_sprites
                    .set_record_fields(*index, *fields, vertical, sprite_lengths)
                    .map_err(|error| LevelControllerError::SpriteEdit { command, error })?;
            }
            NativeLevelEdit::InsertSprite { index, token } => staged_sprites
                .insert(*index, token.clone())
                .map_err(|error| LevelControllerError::SpriteEdit { command, error })?,
            NativeLevelEdit::ReplaceSprite { index, token } => {
                replace_sprite(&mut staged_sprites, *index, token.clone())
                    .map_err(|error| LevelControllerError::SpriteEdit { command, error })?;
            }
            NativeLevelEdit::RemoveSprite { index } => {
                staged_sprites
                    .remove(*index)
                    .map_err(|error| LevelControllerError::SpriteEdit { command, error })?;
            }
            NativeLevelEdit::MoveSpriteBefore { from, before } => staged_sprites
                .move_before(*from, *before)
                .map_err(|error| LevelControllerError::SpriteEdit { command, error })?,
            NativeLevelEdit::SortLegacySpritesByScreen { selected } => {
                staged_sprites
                    .sort_legacy_records_by_screen(*selected)
                    .map_err(|error| LevelControllerError::SpriteEdit { command, error })?;
            }
            NativeLevelEdit::PlaceSpriteAtPosition {
                record,
                screen,
                x,
                y,
            } => {
                let vertical =
                    lm_profile::smw_us_v1_level_mode(staged_layer1.header.level_mode()).vertical;
                staged_sprites
                    .place_record_at_position(
                        record.clone(),
                        *screen,
                        *x,
                        *y,
                        vertical,
                        sprite_lengths,
                    )
                    .map_err(|error| LevelControllerError::SpriteEdit { command, error })?;
            }
            NativeLevelEdit::RelocateSpritePosition {
                selected,
                screen,
                x,
                y,
            } => {
                let vertical =
                    lm_profile::smw_us_v1_level_mode(staged_layer1.header.level_mode()).vertical;
                staged_sprites
                    .relocate_record_position(*selected, *screen, *x, *y, vertical, sprite_lengths)
                    .map_err(|error| LevelControllerError::SpriteEdit { command, error })?;
            }
            NativeLevelEdit::RelocateExpandedSprite {
                selected,
                screen,
                x,
                y,
            } => {
                let vertical =
                    lm_profile::smw_us_v1_level_mode(staged_layer1.header.level_mode()).vertical;
                staged_sprites
                    .relocate_expanded_record(*selected, *screen, *x, *y, vertical, sprite_lengths)
                    .map_err(|error| LevelControllerError::SpriteEdit { command, error })?;
            }
        }
    }
    staged_sprites
        .canonicalize_for_orientation(staged_layer1.header.is_vertical())
        .map_err(LevelControllerError::SpriteCanonicalization)?;
    let encoded_layer = staged_layer1
        .encode()
        .map_err(LevelControllerError::InvalidObjectEncoding)?;
    let reparsed_layer = LevelObjectData::parse(&encoded_layer)
        .map_err(LevelControllerError::InvalidObjectEncoding)?;
    if reparsed_layer != staged_layer1 {
        return Err(LevelControllerError::NonCanonicalObjectEncoding);
    }
    let encoded_sprites = staged_sprites
        .encode_for_table(sprite_lengths)
        .map_err(LevelControllerError::InvalidSpriteSerialization)?;
    let reparsed_sprites =
        NativeSpriteStream::parse(&encoded_sprites, staged_sprites.expanded, sprite_lengths)
            .map_err(LevelControllerError::InvalidSpriteEncoding)?;
    if reparsed_sprites != staged_sprites {
        return Err(LevelControllerError::NonCanonicalSpriteEncoding);
    }
    *layer1 = staged_layer1;
    *sprites = staged_sprites;
    Ok(())
}

fn replace_sprite(
    stream: &mut NativeSpriteStream,
    index: usize,
    token: SpriteToken,
) -> Result<(), LevelEditError> {
    if index >= stream.tokens.len() {
        return Err(LevelEditError::IndexOutOfBounds {
            index,
            len: stream.tokens.len(),
        });
    }
    let mut staged = stream.clone();
    staged.remove(index)?;
    staged.insert(index, token)?;
    *stream = staged;
    Ok(())
}

fn apply_header_edit(
    layer1: &mut LevelObjectData,
    edit: LegacyHeaderEdit,
) -> Result<(), HeaderValueError> {
    match edit {
        LegacyHeaderEdit::BackgroundPalette(value) => layer1.header.set_background_palette(value),
        LegacyHeaderEdit::LastScreen(value) => layer1.header.set_last_screen(value),
        LegacyHeaderEdit::LevelMode(value) => layer1
            .header
            .set_level_mode(lm_level::lunar_magic_canonical_level_mode(value)),
        LegacyHeaderEdit::BackgroundColor(value) => layer1.header.set_background_color(value),
        LegacyHeaderEdit::SpriteTileset(value) => layer1.header.set_sprite_tileset(value),
        LegacyHeaderEdit::DefaultMusicSelector(value) => {
            layer1.header.set_default_music_selector(value)
        }
        LegacyHeaderEdit::TimeLimitSelector(value) => layer1.header.set_time_limit_selector(value),
        LegacyHeaderEdit::SpritePalette(value) => layer1.header.set_sprite_palette(value),
        LegacyHeaderEdit::ForegroundPalette(value) => layer1.header.set_foreground_palette(value),
        LegacyHeaderEdit::ObjectTileset(value) => layer1.header.set_object_tileset(value),
        LegacyHeaderEdit::Layer1VerticalScroll(mode) => {
            layer1.header.set_layer1_vertical_scroll(mode);
            Ok(())
        }
    }
}
