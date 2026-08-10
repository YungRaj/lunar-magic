use std::fmt;

/// Number of four-byte entries in Lunar Magic's legacy standard-GFX bypass list.
pub const LEGACY_GRAPHICS_BYPASS_ENTRIES: usize = 256;

/// Number of entries the original dialogs expose. Entry `$FF` is retained losslessly but is not
/// selectable; both original dialog procedures enumerate only `$00..=$FE`.
pub const LEGACY_GRAPHICS_BYPASS_SELECTABLE_ENTRIES: usize = 255;

/// Four standard GFX file numbers in display/VRAM order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyGraphicsAssignment(pub [u8; 4]);

/// The exact `$400`-byte legacy bypass table selected by object-stream command `$24`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyGraphicsBypassTable {
    entries: [LegacyGraphicsAssignment; LEGACY_GRAPHICS_BYPASS_ENTRIES],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyGraphicsBypassTableError {
    Length(usize),
    EntryOutOfRange(usize),
}

impl fmt::Display for LegacyGraphicsBypassTableError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid legacy graphics-bypass table: {self:?}")
    }
}

impl std::error::Error for LegacyGraphicsBypassTableError {}

impl LegacyGraphicsBypassTable {
    pub const ENCODED_LEN: usize = LEGACY_GRAPHICS_BYPASS_ENTRIES * 4;

    /// Decodes all 256 rows. Lunar Magic stores each row in the reverse of the dialog/VRAM order.
    pub fn decode(bytes: &[u8]) -> Result<Self, LegacyGraphicsBypassTableError> {
        if bytes.len() != Self::ENCODED_LEN {
            return Err(LegacyGraphicsBypassTableError::Length(bytes.len()));
        }
        let mut entries = [LegacyGraphicsAssignment::default(); LEGACY_GRAPHICS_BYPASS_ENTRIES];
        for (entry, stored) in entries.iter_mut().zip(bytes.chunks_exact(4)) {
            entry.0 = [stored[3], stored[2], stored[1], stored[0]];
        }
        Ok(Self { entries })
    }

    #[must_use]
    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut bytes = [0; Self::ENCODED_LEN];
        for (stored, entry) in bytes.chunks_exact_mut(4).zip(self.entries) {
            stored.copy_from_slice(&[entry.0[3], entry.0[2], entry.0[1], entry.0[0]]);
        }
        bytes
    }

    pub fn entry(
        &self,
        index: usize,
    ) -> Result<LegacyGraphicsAssignment, LegacyGraphicsBypassTableError> {
        self.entries
            .get(index)
            .copied()
            .ok_or(LegacyGraphicsBypassTableError::EntryOutOfRange(index))
    }

    pub fn set_entry(
        &mut self,
        index: usize,
        assignment: LegacyGraphicsAssignment,
    ) -> Result<(), LegacyGraphicsBypassTableError> {
        let target = self
            .entries
            .get_mut(index)
            .ok_or(LegacyGraphicsBypassTableError::EntryOutOfRange(index))?;
        *target = assignment;
        Ok(())
    }
}

impl Default for LegacyGraphicsBypassTable {
    fn default() -> Self {
        Self {
            entries: [LegacyGraphicsAssignment::default(); LEGACY_GRAPHICS_BYPASS_ENTRIES],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_table_round_trip_reverses_each_dialog_row_only_at_storage_boundary() {
        let mut bytes = [0_u8; LegacyGraphicsBypassTable::ENCODED_LEN];
        bytes[5 * 4..5 * 4 + 4].copy_from_slice(&[3, 4, 2, 1]);
        bytes[255 * 4..].copy_from_slice(&[0xdd, 0xcc, 0xbb, 0xaa]);
        let mut table = LegacyGraphicsBypassTable::decode(&bytes).unwrap();
        assert_eq!(table.entry(5).unwrap().0, [1, 2, 4, 3]);
        assert_eq!(table.entry(255).unwrap().0, [0xaa, 0xbb, 0xcc, 0xdd]);
        table
            .set_entry(5, LegacyGraphicsAssignment([0x10, 0x20, 0x30, 0x40]))
            .unwrap();
        let encoded = table.encode();
        assert_eq!(&encoded[5 * 4..5 * 4 + 4], &[0x40, 0x30, 0x20, 0x10]);
        assert_eq!(&encoded[255 * 4..], &[0xdd, 0xcc, 0xbb, 0xaa]);
    }

    #[test]
    fn malformed_shapes_and_indexes_reject_without_partial_mutation() {
        assert_eq!(
            LegacyGraphicsBypassTable::decode(&[0; 1023]).unwrap_err(),
            LegacyGraphicsBypassTableError::Length(1023)
        );
        let mut table = LegacyGraphicsBypassTable::default();
        let before = table.clone();
        assert_eq!(
            table
                .set_entry(256, LegacyGraphicsAssignment([1, 2, 3, 4]))
                .unwrap_err(),
            LegacyGraphicsBypassTableError::EntryOutOfRange(256)
        );
        assert_eq!(table, before);
    }
}
