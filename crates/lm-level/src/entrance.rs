#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntranceKind {
    Main,
    Midway,
    Secondary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Entrance {
    pub kind: EntranceKind,
    pub x: u16,
    pub y: u16,
    pub screen: u8,
    pub action: u8,
    pub raw_flags: u16,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScreenExit {
    pub encoded: u32,
}

impl ScreenExit {
    #[must_use]
    pub fn from_object_fields(destination_low: u8, destination_high_and_flags: u8) -> Self {
        Self {
            encoded: u32::from(destination_low)
                | ((u32::from(destination_high_and_flags) + 1) << 8),
        }
    }

    #[must_use]
    pub const fn destination_low(self) -> u8 {
        self.encoded.to_le_bytes()[0]
    }

    #[must_use]
    pub const fn destination_high_and_flags(self) -> u8 {
        (self.encoded.wrapping_sub(0x100) >> 8).to_le_bytes()[0]
    }

    #[must_use]
    pub const fn is_present(self) -> bool {
        self.encoded & 0x100 != 0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SecondaryExit {
    pub destination_level: u16,
    pub position_and_method: u8,
    pub screen: u8,
    pub x: u8,
    pub y: u8,
    /// Destination flags outside the level-high bit stored in bit 3.
    pub destination_flags: u8,
    /// Preserved high nibble containing overworld and X-position flags.
    pub x_and_overworld_flags: u8,
    pub additional_flags: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecondaryExitTable {
    pub entries: Vec<SecondaryExit>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecondaryExitEncodingError {
    WrongEntryCount { actual: usize, expected: usize },
    SizeOverflow { records: usize },
    DestinationLevelOutOfRange { entry: usize, value: u16 },
    ScreenOutOfRange { entry: usize, value: u8 },
    XOutOfRange { entry: usize, value: u8 },
    YOutOfRange { entry: usize, value: u8 },
    DestinationFlagsUseLevelBit { entry: usize, value: u8 },
    XFlagsUsePositionBits { entry: usize, value: u8 },
}

impl std::fmt::Display for SecondaryExitEncodingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid secondary-exit encoding: {self:?}")
    }
}

impl std::error::Error for SecondaryExitEncodingError {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MwlSecondaryExit {
    pub index: u16,
    pub exit: SecondaryExit,
    pub reserved: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MwlSecondaryExitDecodeError {
    WrongLength(usize),
    TargetLevelOutOfRange(u16),
}

impl std::fmt::Display for MwlSecondaryExitDecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid MWL secondary-exit decode: {self:?}")
    }
}

impl std::error::Error for MwlSecondaryExitDecodeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use mwl::mwl_secondary_exit_encoded_len;

    #[test]
    fn screen_exit_object_fields_round_trip() {
        let exit = ScreenExit::from_object_fields(0x34, 0x92);
        assert_eq!(exit.destination_low(), 0x34);
        assert_eq!(exit.destination_high_and_flags(), 0x92);
        assert!(exit.is_present());
    }

    #[test]
    fn secondary_exit_planes_round_trip_losslessly() {
        let mut bytes = vec![0; SecondaryExitTable::ENTRY_COUNT * SecondaryExitTable::PLANE_COUNT];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = index.wrapping_mul(37).to_le_bytes()[0];
        }
        let table = SecondaryExitTable::decode(&bytes).unwrap();
        assert_eq!(table.encode().unwrap(), bytes);
    }

    #[test]
    fn secondary_exit_native_planes_follow_lunar_magic_memory_order() {
        let mut bytes = vec![0; SecondaryExitTable::ENTRY_COUNT * SecondaryExitTable::PLANE_COUNT];
        bytes[7] = 0x34;
        bytes[SecondaryExitTable::ENTRY_COUNT + 7] = 0xa5;
        bytes[SecondaryExitTable::ENTRY_COUNT * 2 + 7] = 0xbc;
        bytes[SecondaryExitTable::ENTRY_COUNT * 3 + 7] = 0xe9;
        bytes[SecondaryExitTable::ENTRY_COUNT * 4 + 7] = 0xd7;
        bytes[SecondaryExitTable::ENTRY_COUNT * 5 + 7] = 0x66;
        let table = SecondaryExitTable::decode(&bytes).unwrap();
        assert_eq!(
            table.entries[7],
            SecondaryExit {
                destination_level: 0x134,
                position_and_method: 0xa5,
                screen: 0x0b,
                x: 7,
                y: 0x0c,
                destination_flags: 0xe1,
                x_and_overworld_flags: 0xd0,
                additional_flags: 0x66,
            }
        );
        assert_eq!(table.encode().unwrap(), bytes);
    }

    #[test]
    fn mwl_exit_record_retargets_destination_and_preserves_fields() {
        let bytes = [0x34, 0x12, 0xa5, 0xbc, 0xe7, 0xd9, 0x66, 0x55];
        let record = MwlSecondaryExit::decode(&bytes, 0x105).unwrap();
        assert_eq!(record.index, 0x1234);
        assert_eq!(record.exit.destination_level, 0x105);
        assert_eq!(record.exit.position_and_method, 0xa5);
        let encoded = record.encode().unwrap();
        assert_eq!(encoded, [0x34, 0x12, 0xa5, 0xbc, 0xef, 0xd9, 0x66, 0x55]);
    }

    #[test]
    fn mwl_decode_rejects_target_levels_that_would_be_silently_wrapped() {
        let bytes = [0; MwlSecondaryExit::ENCODED_LEN];
        assert_eq!(
            MwlSecondaryExit::decode(&bytes, 0x0200),
            Err(MwlSecondaryExitDecodeError::TargetLevelOutOfRange(0x0200))
        );
        assert_eq!(
            MwlSecondaryExit::decode_all(&[], 0x1205),
            Err(MwlSecondaryExitDecodeError::TargetLevelOutOfRange(0x1205))
        );
        assert_eq!(
            MwlSecondaryExit::decode(&bytes[..7], 0x105),
            Err(MwlSecondaryExitDecodeError::WrongLength(7))
        );
    }

    #[test]
    fn mwl_exit_aggregate_size_is_exact_and_checked() {
        let maximum = usize::MAX / MwlSecondaryExit::ENCODED_LEN;
        assert_eq!(
            mwl_secondary_exit_encoded_len(maximum).unwrap(),
            maximum * MwlSecondaryExit::ENCODED_LEN
        );
        assert_eq!(
            mwl_secondary_exit_encoded_len(maximum + 1),
            Err(SecondaryExitEncodingError::SizeOverflow {
                records: maximum + 1,
            })
        );
    }

    #[test]
    fn native_table_requires_exact_shape() {
        for actual in [
            0,
            SecondaryExitTable::ENTRY_COUNT - 1,
            SecondaryExitTable::ENTRY_COUNT + 1,
        ] {
            let table = SecondaryExitTable {
                entries: vec![SecondaryExit::default(); actual],
            };
            assert_eq!(
                table.encode(),
                Err(SecondaryExitEncodingError::WrongEntryCount {
                    actual,
                    expected: SecondaryExitTable::ENTRY_COUNT,
                })
            );
        }
    }

    #[test]
    fn unrepresentable_exit_fields_are_rejected_instead_of_masked() {
        let cases = [
            SecondaryExit {
                destination_level: 0x200,
                ..SecondaryExit::default()
            },
            SecondaryExit {
                screen: 0x10,
                ..SecondaryExit::default()
            },
            SecondaryExit {
                x: 0x10,
                ..SecondaryExit::default()
            },
            SecondaryExit {
                y: 0x10,
                ..SecondaryExit::default()
            },
            SecondaryExit {
                destination_flags: 8,
                ..SecondaryExit::default()
            },
            SecondaryExit {
                x_and_overworld_flags: 1,
                ..SecondaryExit::default()
            },
        ];
        for exit in cases {
            let record = MwlSecondaryExit {
                exit,
                ..MwlSecondaryExit::default()
            };
            assert!(record.encode().is_err());
            let mut entries = vec![SecondaryExit::default(); SecondaryExitTable::ENTRY_COUNT];
            entries[37] = exit;
            let error = SecondaryExitTable { entries }.encode().unwrap_err();
            assert!(format!("{error:?}").contains("entry: 37"));
        }
    }
}
mod mwl;
mod secondary_file;
mod secondary_table;
mod validation;

pub use secondary_file::SecondaryExitTableFileError;
