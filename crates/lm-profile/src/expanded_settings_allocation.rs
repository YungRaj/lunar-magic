//! SMW US revision-0 expanded-settings allocation and migration semantics.

use lm_level::ExpandedLevelSettingsRecord;

pub const SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_LEN: usize = 0x6e00;
pub const SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN: usize = 0x2d00;
pub const SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT: usize = 0x208;
pub const SMW_US_V1_EXPANDED_SETTINGS_STANDARD_LEVEL_COUNT: usize = 0x200;
pub const SMW_US_V1_EXPANDED_SETTINGS_SPECIAL_RECORD_OFFSET: usize = 0x6d00;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1ExpandedSettingsAllocation {
    records: Vec<ExpandedLevelSettingsRecord>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1ExpandedSettingsAllocationError {
    WrongLength(usize),
    NonFillPrefix { offset: usize, value: u8 },
    RecordOutOfRange(usize),
}

impl std::fmt::Display for SmwUsV1ExpandedSettingsAllocationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "expanded-settings allocation error: {self:?}")
    }
}

impl std::error::Error for SmwUsV1ExpandedSettingsAllocationError {}

impl SmwUsV1ExpandedSettingsAllocation {
    #[must_use]
    pub fn new_default() -> Self {
        let mut records = vec![
            smw_us_v1_default_expanded_settings_record();
            SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT
        ];
        for record in &mut records[0x200..0x207] {
            *record = smw_us_v1_default_special_expanded_settings_record();
        }
        Self { records }
    }

    /// Decodes the exact `$6E00` allocation and validates its `$2D00`-byte fill prefix.
    ///
    /// # Errors
    ///
    /// Rejects an incorrect allocation length or any non-`$FF` prefix byte.
    pub fn decode(bytes: &[u8]) -> Result<Self, SmwUsV1ExpandedSettingsAllocationError> {
        if bytes.len() != SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_LEN {
            return Err(SmwUsV1ExpandedSettingsAllocationError::WrongLength(
                bytes.len(),
            ));
        }
        if let Some((offset, value)) = bytes[..SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN]
            .iter()
            .copied()
            .enumerate()
            .find(|(_, value)| *value != 0xff)
        {
            return Err(SmwUsV1ExpandedSettingsAllocationError::NonFillPrefix { offset, value });
        }
        let records = bytes[SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN..]
            .chunks_exact(ExpandedLevelSettingsRecord::ENCODED_LEN)
            .map(|record| {
                let mut encoded = [0; ExpandedLevelSettingsRecord::ENCODED_LEN];
                encoded.copy_from_slice(record);
                ExpandedLevelSettingsRecord::from_encoded(encoded)
            })
            .collect();
        Ok(Self { records })
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = vec![0xff; SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN];
        for record in &self.records {
            bytes.extend_from_slice(record.encoded());
        }
        bytes
    }

    #[must_use]
    pub fn records(&self) -> &[ExpandedLevelSettingsRecord] {
        &self.records
    }

    /// Returns one of the 520 physical records.
    ///
    /// # Errors
    ///
    /// Rejects indexes above 519.
    pub fn record(
        &self,
        index: usize,
    ) -> Result<&ExpandedLevelSettingsRecord, SmwUsV1ExpandedSettingsAllocationError> {
        self.records
            .get(index)
            .ok_or(SmwUsV1ExpandedSettingsAllocationError::RecordOutOfRange(
                index,
            ))
    }

    /// Replaces one of the 520 physical records.
    ///
    /// # Errors
    ///
    /// Rejects indexes above 519.
    pub fn set_record(
        &mut self,
        index: usize,
        record: ExpandedLevelSettingsRecord,
    ) -> Result<(), SmwUsV1ExpandedSettingsAllocationError> {
        let destination = self.records.get_mut(index).ok_or(
            SmwUsV1ExpandedSettingsAllocationError::RecordOutOfRange(index),
        )?;
        *destination = record;
        Ok(())
    }

    #[must_use]
    pub const fn record_offset(index: usize) -> Option<usize> {
        if index < SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT {
            Some(
                SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN
                    + index * ExpandedLevelSettingsRecord::ENCODED_LEN,
            )
        } else {
            None
        }
    }
}

impl Default for SmwUsV1ExpandedSettingsAllocation {
    fn default() -> Self {
        Self::new_default()
    }
}

#[must_use]
pub fn smw_us_v1_default_expanded_settings_record() -> ExpandedLevelSettingsRecord {
    let mut bytes = [0; ExpandedLevelSettingsRecord::ENCODED_LEN];
    for index in 0..ExpandedLevelSettingsRecord::WORD_COUNT {
        write_word(&mut bytes, index, 0x7f);
    }
    write_word(&mut bytes, 8, 0xffff);
    write_word(&mut bytes, 12, 0x2b);
    write_word(&mut bytes, 13, 0x2a);
    write_word(&mut bytes, 14, 0x29);
    write_word(&mut bytes, 15, 0x28);
    ExpandedLevelSettingsRecord::from_encoded(bytes)
}

/// Recovered installation default for special records `$200..$206`.
#[must_use]
pub fn smw_us_v1_default_special_expanded_settings_record() -> ExpandedLevelSettingsRecord {
    let words = [
        0x0014, 0x007f, 0x007f, 0x007f, 0x001e, 0x0008, 0x001d, 0x001c, 0x001d, 0x001c, 0x000f,
        0x0010, 0x002b, 0x002a, 0x0029, 0x0028,
    ];
    let mut bytes = [0; ExpandedLevelSettingsRecord::ENCODED_LEN];
    for (index, word) in words.into_iter().enumerate() {
        write_word(&mut bytes, index, word);
    }
    ExpandedLevelSettingsRecord::from_encoded(bytes)
}

/// Applies Lunar Magic 3.63's recovered current-layout normalization in place.
pub fn smw_us_v1_normalize_expanded_settings_record(record: &mut ExpandedLevelSettingsRecord) {
    let source = *record.encoded();
    let mut bytes = source;
    write_word(
        &mut bytes,
        0,
        read_word(&source, 13) & 0x0fff | read_word(&source, 0) & 0x8000,
    );
    for (destination, source_index) in (2..=11).zip(3..=12) {
        write_word(&mut bytes, destination, read_word(&source, source_index));
    }
    for index in 2..12 {
        if index != 8 {
            let value = read_word(&bytes, index);
            write_word(
                &mut bytes,
                index,
                if value == 0xffff {
                    0x7f
                } else {
                    value & 0x0fff
                },
            );
        }
    }
    write_word(&mut bytes, 12, 0x2b);
    write_word(&mut bytes, 13, 0x2a);
    write_word(&mut bytes, 14, 0x29);
    write_word(&mut bytes, 15, 0x28);
    *record = ExpandedLevelSettingsRecord::from_encoded(bytes);
}

fn read_word(bytes: &[u8; ExpandedLevelSettingsRecord::ENCODED_LEN], index: usize) -> u16 {
    u16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]])
}

fn write_word(
    bytes: &mut [u8; ExpandedLevelSettingsRecord::ENCODED_LEN],
    index: usize,
    value: u16,
) {
    bytes[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn default_record_matches_recovered_initializer() {
        let record = smw_us_v1_default_expanded_settings_record();
        for index in 0..16 {
            let expected = match index {
                8 => 0xffff,
                12 => 0x2b,
                13 => 0x2a,
                14 => 0x29,
                15 => 0x28,
                _ => 0x7f,
            };
            assert_eq!(record.word(index).unwrap(), expected);
        }
        let allocation = SmwUsV1ExpandedSettingsAllocation::new_default();
        assert_eq!(allocation.encode().len(), 0x6e00);
        for index in 0x200..0x207 {
            assert_eq!(
                allocation.record(index).unwrap(),
                &smw_us_v1_default_special_expanded_settings_record()
            );
        }
        assert_eq!(
            allocation.record(0x207).unwrap(),
            &smw_us_v1_default_expanded_settings_record()
        );
        assert_eq!(
            SmwUsV1ExpandedSettingsAllocation::record_offset(0x200),
            Some(0x6d00)
        );
        assert_eq!(
            SmwUsV1ExpandedSettingsAllocation::record_offset(0x208),
            None
        );
    }

    #[test]
    fn normalizer_reorders_masks_sentinels_and_restores_trailing_defaults() {
        let source: [u8; 32] = std::array::from_fn(|byte| {
            let word = u16::try_from(byte / 2).unwrap() | 0xf000;
            word.to_le_bytes()[byte & 1]
        });
        let mut record = ExpandedLevelSettingsRecord::decode(&source).unwrap();
        record.set_word(0, 0x8001).unwrap();
        record.set_word(5, 0xffff).unwrap();
        smw_us_v1_normalize_expanded_settings_record(&mut record);
        assert_eq!(record.word(0).unwrap(), 0x800d);
        assert_eq!(record.word(1).unwrap(), 0xf001);
        assert_eq!(record.word(2).unwrap(), 3);
        assert_eq!(record.word(4).unwrap(), 0x7f);
        assert_eq!(record.word(8).unwrap(), 9 | 0xf000);
        assert_eq!(
            (12..16)
                .map(|index| record.word(index).unwrap())
                .collect::<Vec<_>>(),
            [0x2b, 0x2a, 0x29, 0x28]
        );
    }

    #[test]
    fn retained_wine_allocation_round_trips_exactly() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rom = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        assert_eq!(
            &rom[0x87ff8 + 0x200..0x88000 + 0x200],
            b"STAR\xff\x6d\x00\x92"
        );
        let bytes = &rom[0x88000 + 0x200..0x88000 + 0x200 + 0x6e00];
        let allocation = SmwUsV1ExpandedSettingsAllocation::decode(bytes).unwrap();
        assert_eq!(allocation.records().len(), 0x208);
        assert_eq!(allocation.record(0).unwrap().word(0).unwrap(), 0x207f);
        assert_eq!(allocation.record(0).unwrap().word(1).unwrap(), 0x2028);
        assert_eq!(allocation.encode(), bytes);
        assert!(allocation.record(0x208).is_err());
    }
}
