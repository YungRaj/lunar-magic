use crate::{
    EventTableError, EventTileChange, OverworldEndpoint, OverworldLayer, OverworldMessage,
    OverworldSprite, OverworldSpriteError,
};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldEditError {
    InvalidLayerShape {
        width: usize,
        height: usize,
        tiles: usize,
    },
    CoordinateOutOfBounds {
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    },
    IndexOutOfBounds {
        index: usize,
        len: usize,
    },
    Sprite(OverworldSpriteError),
    EventReveal(EventTableError),
}

impl fmt::Display for OverworldEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid overworld edit: {self:?}")
    }
}

impl std::error::Error for OverworldEditError {}

impl From<OverworldSpriteError> for OverworldEditError {
    fn from(error: OverworldSpriteError) -> Self {
        Self::Sprite(error)
    }
}

impl From<EventTableError> for OverworldEditError {
    fn from(error: EventTableError) -> Self {
        Self::EventReveal(error)
    }
}

impl OverworldLayer {
    /// Returns a tile from the rectangular row-major layer.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldEditError`] if the stored shape is inconsistent or the coordinate lies
    /// outside it.
    pub fn tile(&self, x: usize, y: usize) -> Result<u16, OverworldEditError> {
        let index = self.checked_index(x, y)?;
        Ok(self.tiles[index])
    }

    /// Replaces one tile without allowing malformed dimensions to hide an out-of-range write.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldEditError`] for an inconsistent layer or invalid coordinate.
    pub fn set_tile(&mut self, x: usize, y: usize, tile: u16) -> Result<u16, OverworldEditError> {
        let index = self.checked_index(x, y)?;
        Ok(std::mem::replace(&mut self.tiles[index], tile))
    }

    fn checked_index(&self, x: usize, y: usize) -> Result<usize, OverworldEditError> {
        let expected = self.width.checked_mul(self.height);
        if expected != Some(self.tiles.len()) {
            return Err(OverworldEditError::InvalidLayerShape {
                width: self.width,
                height: self.height,
                tiles: self.tiles.len(),
            });
        }
        if x >= self.width || y >= self.height {
            return Err(OverworldEditError::CoordinateOutOfBounds {
                x,
                y,
                width: self.width,
                height: self.height,
            });
        }
        y.checked_mul(self.width)
            .and_then(|row| row.checked_add(x))
            .ok_or(OverworldEditError::InvalidLayerShape {
                width: self.width,
                height: self.height,
                tiles: self.tiles.len(),
            })
    }
}

impl OverworldMessage {
    /// Changes one message tile and returns its previous value.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldEditError::CoordinateOutOfBounds`] outside the fixed 18×8 tilemap.
    pub fn set_tile(
        &mut self,
        column: usize,
        row: usize,
        tile: u8,
    ) -> Result<u8, OverworldEditError> {
        if column >= Self::COLUMNS || row >= Self::ROWS {
            return Err(OverworldEditError::CoordinateOutOfBounds {
                x: column,
                y: row,
                width: Self::COLUMNS,
                height: Self::ROWS,
            });
        }
        let index = row * Self::COLUMNS + column;
        Ok(std::mem::replace(&mut self.0[index], tile))
    }
}

pub trait OverworldRecord: Sized {
    /// Validates revision-dependent storage shape before insertion.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldEditError`] when the record cannot be encoded with the supplied native
    /// record length. Fixed-size semantic records always accept the operation.
    fn validate_for_insert(&self, _record_len: Option<usize>) -> Result<(), OverworldEditError> {
        Ok(())
    }
}

impl OverworldRecord for EventTileChange {}
impl OverworldRecord for OverworldEndpoint {}
impl OverworldRecord for OverworldMessage {}

impl OverworldRecord for OverworldSprite {
    fn validate_for_insert(&self, record_len: Option<usize>) -> Result<(), OverworldEditError> {
        if let Some(record_len) = record_len {
            OverworldSprite::encode_all(std::slice::from_ref(self), record_len)?;
        }
        Ok(())
    }
}

/// Inserts a record before `index`; `records.len()` appends.
///
/// Sprite callers should provide their revision's fixed record length so extension-byte shape is
/// validated before mutation. Other record types ignore `record_len`.
///
/// # Errors
///
/// Returns [`OverworldEditError`] for an invalid index or sprite record shape.
pub fn insert_record<T: OverworldRecord>(
    records: &mut Vec<T>,
    index: usize,
    record: T,
    record_len: Option<usize>,
) -> Result<(), OverworldEditError> {
    if index > records.len() {
        return Err(OverworldEditError::IndexOutOfBounds {
            index,
            len: records.len(),
        });
    }
    record.validate_for_insert(record_len)?;
    records.insert(index, record);
    Ok(())
}

/// Removes and returns one overworld record.
///
/// # Errors
///
/// Returns [`OverworldEditError::IndexOutOfBounds`] for an invalid index.
pub fn remove_record<T>(records: &mut Vec<T>, index: usize) -> Result<T, OverworldEditError> {
    if index >= records.len() {
        return Err(OverworldEditError::IndexOutOfBounds {
            index,
            len: records.len(),
        });
    }
    Ok(records.remove(index))
}

/// Moves a record before an index in the pre-move ordering; `records.len()` means the end.
///
/// # Errors
///
/// Returns [`OverworldEditError::IndexOutOfBounds`] for an invalid source or destination.
pub fn move_record_before<T>(
    records: &mut Vec<T>,
    from: usize,
    before: usize,
) -> Result<(), OverworldEditError> {
    let len = records.len();
    if from >= len {
        return Err(OverworldEditError::IndexOutOfBounds { index: from, len });
    }
    if before > len {
        return Err(OverworldEditError::IndexOutOfBounds { index: before, len });
    }
    if from == before || from.checked_add(1) == Some(before) {
        return Ok(());
    }
    let record = records.remove(from);
    records.insert(if before > from { before - 1 } else { before }, record);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EventId, Submap};

    #[test]
    fn layer_and_message_edits_are_bounded_and_return_previous_tiles() {
        let mut layer = OverworldLayer::new(2, 2, vec![1, 2, 3, 4]).unwrap();
        assert_eq!(layer.set_tile(1, 0, 9).unwrap(), 2);
        assert_eq!(layer.tile(1, 0).unwrap(), 9);
        let unchanged = layer.clone();
        assert!(layer.set_tile(2, 0, 8).is_err());
        assert_eq!(layer, unchanged);

        layer.tiles.pop();
        assert!(matches!(
            layer.tile(0, 0),
            Err(OverworldEditError::InvalidLayerShape { .. })
        ));

        let mut message = OverworldMessage([0; OverworldMessage::ENCODED_LEN]);
        assert_eq!(message.set_tile(17, 7, 0x44).unwrap(), 0);
        assert_eq!(message.row(7).unwrap()[17], 0x44);
        assert!(message.set_tile(18, 7, 1).is_err());
    }

    #[test]
    fn record_edits_preserve_order_and_fail_atomically() {
        let event = |value| EventTileChange {
            event: EventId(value),
            x: 0,
            y: 0,
            before: 0,
            after: 1,
            raw_flags: 0,
        };
        let mut events = vec![event(1), event(2), event(3)];
        move_record_before(&mut events, 0, 3).unwrap();
        assert_eq!(
            events.iter().map(|entry| entry.event.0).collect::<Vec<_>>(),
            [2, 3, 1]
        );
        insert_record(&mut events, 1, event(4), None).unwrap();
        assert_eq!(remove_record(&mut events, 2).unwrap().event, EventId(3));
        let original = events.clone();
        assert!(move_record_before(&mut events, 0, 5).is_err());
        assert_eq!(events, original);
    }

    #[test]
    fn sprite_extension_shape_is_checked_before_insertion() {
        let sprite = OverworldSprite {
            id: 1,
            x: 2,
            y: 3,
            submap: Submap::Main,
            extra: vec![0xaa],
        };
        let mut sprites = Vec::new();
        assert!(insert_record(&mut sprites, 0, sprite.clone(), Some(7)).is_err());
        assert!(sprites.is_empty());
        insert_record(&mut sprites, 0, sprite, Some(8)).unwrap();
        assert_eq!(sprites.len(), 1);
    }
}
