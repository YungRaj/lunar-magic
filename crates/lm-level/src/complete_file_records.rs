use super::{CompleteLevelFile, CompleteLevelFileError, LevelCollection};
use crate::{
    ByteCursor, Entrance, EntranceKind, Layer3File, LayerData, ObjectRecord, ObjectStream,
};

pub(super) fn decode_optional_layer3(
    input: &mut ByteCursor<'_>,
    version: u16,
) -> Result<Option<crate::Layer3Data>, CompleteLevelFileError> {
    if version < 2 {
        return Ok(None);
    }
    match input.u8()? {
        0 => Ok(None),
        1 => {
            let len =
                usize::try_from(input.u32_le()?).map_err(|_| CompleteLevelFileError::Overflow)?;
            Ok(Some(Layer3File::decode(input.take(len)?)?.0))
        }
        value => Err(CompleteLevelFileError::InvalidLayer3Flag(value)),
    }
}

pub(super) fn encode_layer(
    output: &mut Vec<u8>,
    layer: &LayerData,
    objects: LevelCollection,
    tiles: LevelCollection,
) -> Result<(), CompleteLevelFileError> {
    encode_blobs(
        output,
        objects,
        layer.objects.records.iter().map(ObjectRecord::encoded),
    )?;
    encode_count(output, tiles, layer.raw_tilemap.len())?;
    for tile in &layer.raw_tilemap {
        push_u16(output, *tile);
    }
    Ok(())
}

pub(super) fn decode_layer(
    input: &mut ByteCursor<'_>,
    objects: LevelCollection,
    tiles: LevelCollection,
) -> Result<LayerData, CompleteLevelFileError> {
    let records = decode_blobs(input, objects)?
        .into_iter()
        .map(ObjectRecord::new)
        .collect::<Result<Vec<_>, _>>()?;
    let raw_tilemap = (0..decode_count(input, tiles)?)
        .map(|_| input.u16_le().map_err(Into::into))
        .collect::<Result<Vec<_>, CompleteLevelFileError>>()?;
    Ok(LayerData {
        objects: ObjectStream { records },
        raw_tilemap,
    })
}

pub(super) fn encode_entrances(
    output: &mut Vec<u8>,
    entrances: &[Entrance],
) -> Result<(), CompleteLevelFileError> {
    encode_count(output, LevelCollection::Entrances, entrances.len())?;
    for entrance in entrances {
        output.push(match entrance.kind {
            EntranceKind::Main => 0,
            EntranceKind::Midway => 1,
            EntranceKind::Secondary => 2,
        });
        push_u16(output, entrance.x);
        push_u16(output, entrance.y);
        output.extend_from_slice(&[entrance.screen, entrance.action]);
        push_u16(output, entrance.raw_flags);
    }
    Ok(())
}

pub(super) fn decode_entrances(
    input: &mut ByteCursor<'_>,
) -> Result<Vec<Entrance>, CompleteLevelFileError> {
    let count = decode_count(input, LevelCollection::Entrances)?;
    (0..count)
        .map(|record| {
            let value = input.u8()?;
            let kind = match value {
                0 => EntranceKind::Main,
                1 => EntranceKind::Midway,
                2 => EntranceKind::Secondary,
                _ => return Err(CompleteLevelFileError::InvalidEntranceKind { record, value }),
            };
            Ok(Entrance {
                kind,
                x: input.u16_le()?,
                y: input.u16_le()?,
                screen: input.u8()?,
                action: input.u8()?,
                raw_flags: input.u16_le()?,
            })
        })
        .collect()
}

pub(super) fn encode_blobs<'a>(
    output: &mut Vec<u8>,
    collection: LevelCollection,
    blobs: impl ExactSizeIterator<Item = &'a [u8]>,
) -> Result<(), CompleteLevelFileError> {
    encode_count(output, collection, blobs.len())?;
    for blob in blobs {
        if blob.len() > CompleteLevelFile::MAX_RECORD_LEN {
            return Err(CompleteLevelFileError::RecordTooLarge {
                collection,
                len: blob.len(),
            });
        }
        push_u32(
            output,
            u32::try_from(blob.len()).map_err(|_| CompleteLevelFileError::Overflow)?,
        );
        output.extend_from_slice(blob);
    }
    Ok(())
}

pub(super) fn decode_blobs(
    input: &mut ByteCursor<'_>,
    collection: LevelCollection,
) -> Result<Vec<Vec<u8>>, CompleteLevelFileError> {
    let count = decode_count(input, collection)?;
    (0..count)
        .map(|_| {
            let len =
                usize::try_from(input.u32_le()?).map_err(|_| CompleteLevelFileError::Overflow)?;
            if len > CompleteLevelFile::MAX_RECORD_LEN {
                return Err(CompleteLevelFileError::RecordTooLarge { collection, len });
            }
            Ok(input.take(len)?.to_vec())
        })
        .collect()
}

pub(super) fn encode_count(
    output: &mut Vec<u8>,
    collection: LevelCollection,
    count: usize,
) -> Result<(), CompleteLevelFileError> {
    if count > CompleteLevelFile::MAX_RECORDS {
        return Err(CompleteLevelFileError::TooManyRecords { collection, count });
    }
    push_u32(
        output,
        u32::try_from(count).map_err(|_| CompleteLevelFileError::Overflow)?,
    );
    Ok(())
}

pub(super) fn decode_count(
    input: &mut ByteCursor<'_>,
    collection: LevelCollection,
) -> Result<usize, CompleteLevelFileError> {
    let count = usize::try_from(input.u32_le()?).map_err(|_| CompleteLevelFileError::Overflow)?;
    if count > CompleteLevelFile::MAX_RECORDS {
        Err(CompleteLevelFileError::TooManyRecords { collection, count })
    } else {
        Ok(count)
    }
}

pub(super) fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}
