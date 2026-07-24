use super::{
    CompleteLevelFile, CompleteLevelFileError, ExpandedLevelHeader, LayerData, LegacyLevelHeader,
    Level, LevelCollection, MAGIC, Map16Tile, ObjectRecord,
};

pub(super) fn encoded_file_len(
    level: &Level,
    encoded_layer3: Option<&[u8]>,
) -> Result<usize, CompleteLevelFileError> {
    let expanded_len = level
        .header
        .expanded
        .map_or(0, |_| ExpandedLevelHeader::ENCODED_LEN);
    let mut len = checked_file_add(
        0,
        MAGIC.len() + 2 + 2 + LegacyLevelHeader::ENCODED_LEN + 1 + expanded_len,
    )?;
    len = checked_layer_len(
        len,
        &level.layer1,
        LevelCollection::Layer1Objects,
        LevelCollection::Layer1Tiles,
    )?;
    len = checked_layer_len(
        len,
        &level.layer2,
        LevelCollection::Layer2Objects,
        LevelCollection::Layer2Tiles,
    )?;
    len = checked_file_add(len, 1)?;
    if let Some(layer3) = encoded_layer3 {
        len = checked_file_add(len, 4)?;
        len = checked_file_add(len, layer3.len())?;
    }
    len = checked_file_add(len, 1)?;
    len = checked_blobs_len(
        len,
        LevelCollection::Sprites,
        level
            .sprites
            .records
            .iter()
            .map(|record| record.encoded.as_slice()),
    )?;
    len = checked_fixed_records_len(len, LevelCollection::Entrances, level.entrances.len(), 9)?;
    len = checked_fixed_records_len(
        len,
        LevelCollection::ScreenExits,
        level.screen_exits.len(),
        4,
    )?;
    len = checked_fixed_records_len(
        len,
        LevelCollection::SecondaryExits,
        level.secondary_exits.len(),
        9,
    )?;
    len = checked_fixed_records_len(
        len,
        LevelCollection::Map16Overrides,
        level.map16_overrides.len(),
        4 + Map16Tile::GRAPHICS_LEN + 2,
    )?;
    checked_blobs_len(
        len,
        LevelCollection::UnknownExtensions,
        level.unknown_extensions.iter().map(Vec::as_slice),
    )
}

fn checked_layer_len(
    mut len: usize,
    layer: &LayerData,
    objects: LevelCollection,
    tiles: LevelCollection,
) -> Result<usize, CompleteLevelFileError> {
    len = checked_blobs_len(
        len,
        objects,
        layer.objects.records.iter().map(ObjectRecord::encoded),
    )?;
    checked_fixed_records_len(len, tiles, layer.raw_tilemap.len(), 2)
}

fn checked_blobs_len<'a>(
    mut len: usize,
    collection: LevelCollection,
    blobs: impl ExactSizeIterator<Item = &'a [u8]>,
) -> Result<usize, CompleteLevelFileError> {
    validate_count(collection, blobs.len())?;
    len = checked_file_add(len, 4)?;
    for blob in blobs {
        if blob.len() > CompleteLevelFile::MAX_RECORD_LEN {
            return Err(CompleteLevelFileError::RecordTooLarge {
                collection,
                len: blob.len(),
            });
        }
        len = checked_file_add(len, 4)?;
        len = checked_file_add(len, blob.len())?;
    }
    Ok(len)
}

fn checked_fixed_records_len(
    len: usize,
    collection: LevelCollection,
    count: usize,
    record_len: usize,
) -> Result<usize, CompleteLevelFileError> {
    validate_count(collection, count)?;
    let payload = count
        .checked_mul(record_len)
        .ok_or(CompleteLevelFileError::Overflow)?;
    checked_file_add(checked_file_add(len, 4)?, payload)
}

fn validate_count(collection: LevelCollection, count: usize) -> Result<(), CompleteLevelFileError> {
    if count > CompleteLevelFile::MAX_RECORDS {
        Err(CompleteLevelFileError::TooManyRecords { collection, count })
    } else {
        Ok(())
    }
}

pub(super) fn checked_file_add(
    current: usize,
    additional: usize,
) -> Result<usize, CompleteLevelFileError> {
    let total = current
        .checked_add(additional)
        .ok_or(CompleteLevelFileError::Overflow)?;
    if total > CompleteLevelFile::MAX_FILE_LEN {
        Err(CompleteLevelFileError::FileTooLarge(total))
    } else {
        Ok(total)
    }
}
