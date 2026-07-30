use flate2::read::DeflateDecoder;
use std::{collections::BTreeSet, error::Error, fmt, io::Read};

const ARCHIVE_PREFIX_LEN: usize = 0x130;
const ARCHIVE_HEADER_LEN: usize = 0x100;
const RECORD_HEADER_LEN: usize = 0x100;
const MAX_RECORDS: usize = 1_000_000;
const MAX_COMMAND_STREAM_LEN: u64 = 0x200_0000;
const MAX_RESTORED_ROM_LEN: usize = 0x100_0000;
const DECODED_CHECKSUM_XOR: u32 = 0xfade_c0de;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackedRestoreDate {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl PackedRestoreDate {
    const fn decode(value: u32) -> Self {
        Self {
            year: (value >> 16) as u16,
            month: ((value >> 8) & 0xff) as u8,
            day: (value & 0xff) as u8,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackedRestoreTime {
    pub day_of_week: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl PackedRestoreTime {
    const fn decode(value: u32) -> Self {
        Self {
            day_of_week: (value >> 24) as u8,
            hour: ((value >> 16) & 0xff) as u8,
            minute: ((value >> 8) & 0xff) as u8,
            second: (value & 0xff) as u8,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LunarRestoreArchiveHeader {
    pub format_version: [u8; 2],
    pub next_record_id: u32,
    pub last_modified: PackedRestoreDate,
    pub first_record_offset: u64,
    pub last_record_offset: u64,
    pub last_rom_timestamp: u64,
    pub latest_rom_hash: u32,
    pub reserved: Vec<u8>,
    pub producer: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LunarRestorePointRecord {
    pub archive_offset: u64,
    pub next_record_offset: u64,
    pub previous_record_offset: u64,
    pub decoded_payload_size: u32,
    pub payload_checksum: u32,
    pub decoded_payload_checksum: u32,
    pub stored_payload_size: u32,
    pub payload_offset: u32,
    pub description: Vec<u8>,
    pub directory_version: u32,
    pub record_id: u32,
    pub created: PackedRestoreDate,
    pub rom_size: u32,
    pub created_time: PackedRestoreTime,
    pub rom_variant: u32,
    pub rom_hash: u32,
    pub raw_header: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LunarRestoreCommand {
    Fill { offset: u32, length: u32, value: u8 },
    Raw { offset: u32, bytes: Vec<u8> },
}

impl LunarRestorePointRecord {
    #[must_use]
    pub fn description_text(&self) -> String {
        String::from_utf8_lossy(&self.description).into_owned()
    }

    #[must_use]
    pub const fn compressed(&self) -> bool {
        self.directory_version & 0x4000 != 0
    }

    /// Returns the exact stored payload bytes from the supplied archive image.
    ///
    /// # Errors
    ///
    /// Returns an error when the record-relative payload range overflows or lies outside `archive`.
    pub fn stored_payload<'a>(
        &self,
        archive: &'a [u8],
    ) -> Result<&'a [u8], LunarRestoreArchiveError> {
        let start = checked_record_address(self.archive_offset, self.payload_offset)?;
        checked_slice(
            archive,
            start,
            self.stored_payload_size as usize,
            "record payload",
        )
    }

    /// Inflates and decodes this record's exact Lunar Magic delta-command stream.
    ///
    /// # Errors
    ///
    /// Returns an error when inflation fails, the decoded checksum differs, a command is malformed,
    /// or the stream lacks its final `0xFF` command.
    pub fn commands(
        &self,
        archive: &[u8],
    ) -> Result<Vec<LunarRestoreCommand>, LunarRestoreArchiveError> {
        let stored = self.stored_payload(archive)?;
        let decoded = if self.compressed() {
            let mut output = Vec::new();
            DeflateDecoder::new(stored)
                .take(MAX_COMMAND_STREAM_LEN + 1)
                .read_to_end(&mut output)
                .map_err(|error| LunarRestoreArchiveError::Inflate(error.to_string()))?;
            if u64::try_from(output.len()).unwrap_or(u64::MAX) > MAX_COMMAND_STREAM_LEN {
                return Err(LunarRestoreArchiveError::CommandStreamTooLarge(
                    output.len(),
                ));
            }
            output
        } else {
            stored.to_vec()
        };
        let checksum = decoded
            .iter()
            .fold(0_u32, |sum, byte| sum.wrapping_add(u32::from(*byte)))
            ^ DECODED_CHECKSUM_XOR;
        if checksum != self.decoded_payload_checksum {
            return Err(LunarRestoreArchiveError::DecodedChecksumMismatch {
                record: self.archive_offset,
                expected: self.decoded_payload_checksum,
                actual: checksum,
            });
        }
        decode_commands(&decoded, self.archive_offset)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LunarRestoreArchive {
    pub header: LunarRestoreArchiveHeader,
    pub records: Vec<LunarRestorePointRecord>,
    bytes: Vec<u8>,
}

impl LunarRestoreArchive {
    /// Decodes and validates the linked directory of a Lunar Magic `.lrp` archive.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid signatures, truncated fields, invalid payload ranges, link
    /// cycles, inconsistent backward links, or a last-record pointer that disagrees with traversal.
    pub fn decode(bytes: &[u8]) -> Result<Self, LunarRestoreArchiveError> {
        if bytes.len() < ARCHIVE_PREFIX_LEN {
            return Err(LunarRestoreArchiveError::TruncatedArchive {
                actual: bytes.len(),
                minimum: ARCHIVE_PREFIX_LEN,
            });
        }
        if bytes[0..2] != *b"LR" {
            return Err(LunarRestoreArchiveError::BadArchiveMagic([
                bytes[0], bytes[1],
            ]));
        }

        let header = LunarRestoreArchiveHeader {
            format_version: [bytes[2], bytes[3]],
            next_record_id: read_u32(bytes, 8, "next record id")?,
            last_modified: PackedRestoreDate::decode(read_u32(bytes, 0x0c, "archive date")?),
            first_record_offset: read_u64(bytes, 0x10, "first record offset")?,
            last_record_offset: read_u64(bytes, 0x18, "last record offset")?,
            last_rom_timestamp: read_u64(bytes, 0x28, "last ROM timestamp")?,
            latest_rom_hash: read_u32(bytes, 0x30, "latest ROM hash")?,
            reserved: bytes[0x34..ARCHIVE_HEADER_LEN].to_vec(),
            producer: bytes[ARCHIVE_HEADER_LEN..ARCHIVE_PREFIX_LEN].to_vec(),
        };

        let mut records = Vec::new();
        let mut visited = BTreeSet::new();
        let mut offset = header.first_record_offset;
        let mut expected_previous = 0;
        while offset != 0 {
            if records.len() == MAX_RECORDS {
                return Err(LunarRestoreArchiveError::TooManyRecords(MAX_RECORDS));
            }
            if !visited.insert(offset) {
                return Err(LunarRestoreArchiveError::RecordCycle(offset));
            }
            let record = decode_record(bytes, offset)?;
            if record.previous_record_offset != expected_previous {
                return Err(LunarRestoreArchiveError::BrokenPreviousLink {
                    record: offset,
                    expected: expected_previous,
                    actual: record.previous_record_offset,
                });
            }
            expected_previous = offset;
            offset = record.next_record_offset;
            records.push(record);
        }

        let observed_last = records.last().map_or(0, |record| record.archive_offset);
        if observed_last != header.last_record_offset {
            return Err(LunarRestoreArchiveError::LastRecordMismatch {
                header: header.last_record_offset,
                observed: observed_last,
            });
        }

        Ok(Self {
            header,
            records,
            bytes: bytes.to_vec(),
        })
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Reconstructs the ROM state at `record_id` by applying each linked delta from the original.
    ///
    /// # Errors
    ///
    /// Returns an error when the record does not exist, a payload is corrupt, a command exceeds
    /// the supported ROM bound, or command arithmetic overflows.
    pub fn restore_through(
        &self,
        record_id: u32,
        original_rom: &[u8],
    ) -> Result<Vec<u8>, LunarRestoreArchiveError> {
        if original_rom.len() > MAX_RESTORED_ROM_LEN {
            return Err(LunarRestoreArchiveError::RestoredRomTooLarge(
                original_rom.len(),
            ));
        }
        let mut restored = original_rom.to_vec();
        let mut found = false;
        for record in &self.records {
            apply_commands(&mut restored, &record.commands(&self.bytes)?)?;
            let target_len = record.rom_size as usize;
            if target_len > MAX_RESTORED_ROM_LEN {
                return Err(LunarRestoreArchiveError::RestoredRomTooLarge(target_len));
            }
            restored.resize(target_len, 0);
            if record.record_id == record_id {
                found = true;
                break;
            }
        }
        if !found {
            return Err(LunarRestoreArchiveError::UnknownRecordId(record_id));
        }
        Ok(restored)
    }
}

fn decode_commands(
    decoded: &[u8],
    record: u64,
) -> Result<Vec<LunarRestoreCommand>, LunarRestoreArchiveError> {
    let mut cursor = 0;
    let mut commands = Vec::new();
    loop {
        let control = *decoded
            .get(cursor)
            .ok_or(LunarRestoreArchiveError::MissingCommandTerminator(record))?;
        cursor += 1;
        if control == 0xff {
            if cursor != decoded.len() {
                return Err(LunarRestoreArchiveError::TrailingCommandData {
                    record,
                    length: decoded.len() - cursor,
                });
            }
            return Ok(commands);
        }
        if control & !0x17 != 0 {
            return Err(LunarRestoreArchiveError::UnknownCommand { record, control });
        }
        let offset_width = if control & 4 == 0 { 3 } else { 4 };
        let length_width = usize::from(control & 3) + 1;
        let offset = read_variable_u32(decoded, &mut cursor, offset_width, record)?;
        let length = read_variable_u32(decoded, &mut cursor, length_width, record)?;
        if control & 0x10 == 0 {
            let value = *decoded
                .get(cursor)
                .ok_or(LunarRestoreArchiveError::TruncatedCommand(record))?;
            cursor += 1;
            commands.push(LunarRestoreCommand::Fill {
                offset,
                length,
                value,
            });
        } else {
            let length_usize = length as usize;
            let bytes = checked_slice(decoded, cursor, length_usize, "raw restore command")
                .map_err(|_| LunarRestoreArchiveError::TruncatedCommand(record))?;
            cursor += length_usize;
            commands.push(LunarRestoreCommand::Raw {
                offset,
                bytes: bytes.to_vec(),
            });
        }
    }
}

fn read_variable_u32(
    bytes: &[u8],
    cursor: &mut usize,
    width: usize,
    record: u64,
) -> Result<u32, LunarRestoreArchiveError> {
    let source = bytes
        .get(*cursor..*cursor + width)
        .ok_or(LunarRestoreArchiveError::TruncatedCommand(record))?;
    *cursor += width;
    let mut encoded = [0; 4];
    encoded[..width].copy_from_slice(source);
    Ok(u32::from_le_bytes(encoded))
}

fn apply_commands(
    restored: &mut Vec<u8>,
    commands: &[LunarRestoreCommand],
) -> Result<(), LunarRestoreArchiveError> {
    for command in commands {
        let (offset, length) = match command {
            LunarRestoreCommand::Fill { offset, length, .. } => (*offset, *length),
            LunarRestoreCommand::Raw { offset, bytes } => (
                *offset,
                u32::try_from(bytes.len())
                    .map_err(|_| LunarRestoreArchiveError::RestoredRomTooLarge(bytes.len()))?,
            ),
        };
        let start = usize::try_from(offset)
            .map_err(|_| LunarRestoreArchiveError::CommandAddressOverflow { offset, length })?;
        let end =
            start
                .checked_add(usize::try_from(length).map_err(|_| {
                    LunarRestoreArchiveError::CommandAddressOverflow { offset, length }
                })?)
                .ok_or(LunarRestoreArchiveError::CommandAddressOverflow { offset, length })?;
        if end > MAX_RESTORED_ROM_LEN {
            return Err(LunarRestoreArchiveError::RestoredRomTooLarge(end));
        }
        if end > restored.len() {
            restored.resize(end, 0);
        }
        match command {
            LunarRestoreCommand::Fill { value, .. } => restored[start..end].fill(*value),
            LunarRestoreCommand::Raw { bytes, .. } => restored[start..end].copy_from_slice(bytes),
        }
    }
    Ok(())
}

fn decode_record(
    bytes: &[u8],
    archive_offset: u64,
) -> Result<LunarRestorePointRecord, LunarRestoreArchiveError> {
    let start = usize::try_from(archive_offset)
        .map_err(|_| LunarRestoreArchiveError::AddressOverflow(archive_offset))?;
    let header = checked_slice(bytes, start, RECORD_HEADER_LEN, "restore-point header")?;
    if header[0x3c..0x40] != *b"DIRL" {
        return Err(LunarRestoreArchiveError::BadDirectoryMagic {
            offset: archive_offset,
            actual: header[0x3c..0x40].try_into().unwrap(),
        });
    }
    let payload_offset = read_u32(header, 0x30, "payload offset")?;
    if payload_offset < 0x100 {
        return Err(LunarRestoreArchiveError::PayloadOverlapsHeader {
            record: archive_offset,
            payload_offset,
        });
    }
    let description_length = read_u32(header, 0x38, "description length")? as usize;
    let description_bytes = checked_slice(
        bytes,
        start + RECORD_HEADER_LEN,
        description_length,
        "restore-point description",
    )?;
    let Some((&0, description)) = description_bytes.split_last() else {
        return Err(LunarRestoreArchiveError::UnterminatedDescription(
            archive_offset,
        ));
    };
    let stored_payload_size = read_u32(header, 0x28, "stored payload size")?;
    let payload_start = checked_record_address(archive_offset, payload_offset)?;
    checked_slice(
        bytes,
        payload_start,
        stored_payload_size as usize,
        "record payload",
    )?;

    Ok(LunarRestorePointRecord {
        archive_offset,
        next_record_offset: read_u64(header, 0, "next record offset")?,
        previous_record_offset: read_u64(header, 8, "previous record offset")?,
        decoded_payload_size: read_u32(header, 0x18, "decoded payload size")?,
        payload_checksum: read_u32(header, 0x20, "payload checksum")?,
        decoded_payload_checksum: read_u32(header, 0x24, "decoded payload checksum")?,
        stored_payload_size,
        payload_offset,
        description: description.to_vec(),
        directory_version: read_u32(header, 0x40, "directory version")?,
        record_id: read_u32(header, 0x48, "record id")?,
        created: PackedRestoreDate::decode(read_u32(header, 0x4c, "record date")?),
        rom_size: read_u32(header, 0x50, "ROM size")?,
        created_time: PackedRestoreTime::decode(read_u32(header, 0x58, "record time")?),
        rom_variant: read_u32(header, 0x5c, "ROM variant")?,
        rom_hash: read_u32(header, 0x60, "ROM hash")?,
        raw_header: header.to_vec(),
    })
}

fn checked_record_address(record: u64, relative: u32) -> Result<usize, LunarRestoreArchiveError> {
    let address = record
        .checked_add(u64::from(relative))
        .ok_or(LunarRestoreArchiveError::AddressOverflow(record))?;
    usize::try_from(address).map_err(|_| LunarRestoreArchiveError::AddressOverflow(address))
}

fn checked_slice<'a>(
    bytes: &'a [u8],
    offset: usize,
    length: usize,
    field: &'static str,
) -> Result<&'a [u8], LunarRestoreArchiveError> {
    let end = offset
        .checked_add(length)
        .ok_or(LunarRestoreArchiveError::FieldOutOfBounds {
            field,
            offset,
            length,
            archive_len: bytes.len(),
        })?;
    bytes
        .get(offset..end)
        .ok_or(LunarRestoreArchiveError::FieldOutOfBounds {
            field,
            offset,
            length,
            archive_len: bytes.len(),
        })
}

fn read_u32(
    bytes: &[u8],
    offset: usize,
    field: &'static str,
) -> Result<u32, LunarRestoreArchiveError> {
    Ok(u32::from_le_bytes(
        checked_slice(bytes, offset, 4, field)?.try_into().unwrap(),
    ))
}

fn read_u64(
    bytes: &[u8],
    offset: usize,
    field: &'static str,
) -> Result<u64, LunarRestoreArchiveError> {
    Ok(u64::from_le_bytes(
        checked_slice(bytes, offset, 8, field)?.try_into().unwrap(),
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LunarRestoreArchiveError {
    TruncatedArchive {
        actual: usize,
        minimum: usize,
    },
    BadArchiveMagic([u8; 2]),
    FieldOutOfBounds {
        field: &'static str,
        offset: usize,
        length: usize,
        archive_len: usize,
    },
    AddressOverflow(u64),
    BadDirectoryMagic {
        offset: u64,
        actual: [u8; 4],
    },
    PayloadOverlapsHeader {
        record: u64,
        payload_offset: u32,
    },
    UnterminatedDescription(u64),
    RecordCycle(u64),
    BrokenPreviousLink {
        record: u64,
        expected: u64,
        actual: u64,
    },
    LastRecordMismatch {
        header: u64,
        observed: u64,
    },
    TooManyRecords(usize),
    Inflate(String),
    CommandStreamTooLarge(usize),
    DecodedChecksumMismatch {
        record: u64,
        expected: u32,
        actual: u32,
    },
    MissingCommandTerminator(u64),
    TrailingCommandData {
        record: u64,
        length: usize,
    },
    UnknownCommand {
        record: u64,
        control: u8,
    },
    TruncatedCommand(u64),
    UnknownRecordId(u32),
    CommandAddressOverflow {
        offset: u32,
        length: u32,
    },
    RestoredRomTooLarge(usize),
}

impl fmt::Display for LunarRestoreArchiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedArchive { actual, minimum } => {
                write!(
                    formatter,
                    "restore archive has {actual} bytes; need at least {minimum}"
                )
            }
            Self::BadArchiveMagic(actual) => {
                write!(formatter, "invalid restore archive magic {actual:02X?}")
            }
            Self::FieldOutOfBounds {
                field,
                offset,
                length,
                archive_len,
            } => write!(
                formatter,
                "{field} at {offset:#x}+{length:#x} exceeds {archive_len:#x}-byte archive"
            ),
            Self::AddressOverflow(address) => {
                write!(
                    formatter,
                    "restore archive address {address:#x} is not representable"
                )
            }
            Self::BadDirectoryMagic { offset, actual } => write!(
                formatter,
                "restore point at {offset:#x} has invalid directory magic {actual:02X?}"
            ),
            Self::PayloadOverlapsHeader {
                record,
                payload_offset,
            } => write!(
                formatter,
                "restore point at {record:#x} has payload offset {payload_offset:#x} inside its header"
            ),
            Self::UnterminatedDescription(offset) => {
                write!(
                    formatter,
                    "restore point at {offset:#x} has no terminated description"
                )
            }
            Self::RecordCycle(offset) => {
                write!(formatter, "restore-point links form a cycle at {offset:#x}")
            }
            Self::BrokenPreviousLink {
                record,
                expected,
                actual,
            } => write!(
                formatter,
                "restore point at {record:#x} links backward to {actual:#x}, expected {expected:#x}"
            ),
            Self::LastRecordMismatch { header, observed } => write!(
                formatter,
                "archive names {header:#x} as its last restore point, but traversal ended at {observed:#x}"
            ),
            Self::TooManyRecords(limit) => {
                write!(
                    formatter,
                    "restore archive exceeds the {limit} record limit"
                )
            }
            command_error => fmt_command_error(command_error, formatter),
        }
    }
}

fn fmt_command_error(
    error: &LunarRestoreArchiveError,
    formatter: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    use LunarRestoreArchiveError as E;
    match error {
        E::Inflate(error) => write!(formatter, "cannot inflate restore payload: {error}"),
        E::CommandStreamTooLarge(length) => write!(
            formatter,
            "decoded restore command stream is {length} bytes, above the supported limit"
        ),
        E::DecodedChecksumMismatch {
            record,
            expected,
            actual,
        } => write!(
            formatter,
            "restore point at {record:#x} has decoded checksum {actual:#010x}, expected {expected:#010x}"
        ),
        E::MissingCommandTerminator(record) => {
            write!(
                formatter,
                "restore point at {record:#x} has no command terminator"
            )
        }
        E::TrailingCommandData { record, length } => write!(
            formatter,
            "restore point at {record:#x} has {length} bytes after its command terminator"
        ),
        E::UnknownCommand { record, control } => write!(
            formatter,
            "restore point at {record:#x} uses unknown command control {control:#04x}"
        ),
        E::TruncatedCommand(record) => {
            write!(
                formatter,
                "restore point at {record:#x} ends inside a command"
            )
        }
        E::UnknownRecordId(id) => write!(formatter, "restore point id {id} does not exist"),
        E::CommandAddressOverflow { offset, length } => write!(
            formatter,
            "restore command range {offset:#x}+{length:#x} overflows"
        ),
        E::RestoredRomTooLarge(length) => write!(
            formatter,
            "restored ROM length {length:#x} exceeds the supported limit"
        ),
        _ => unreachable!("non-command errors are formatted by the outer match"),
    }
}

impl Error for LunarRestoreArchiveError {}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::DeflateEncoder};
    use std::io::Write;

    fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn archive() -> Vec<u8> {
        let mut bytes = vec![0; 0x380];
        bytes[0..2].copy_from_slice(b"LR");
        bytes[2..4].copy_from_slice(&[0, 2]);
        put_u32(&mut bytes, 8, 3);
        put_u32(&mut bytes, 0x0c, 0x07ea_071b);
        put_u64(&mut bytes, 0x10, 0x130);
        put_u64(&mut bytes, 0x18, 0x250);
        bytes[0x100..0x105].copy_from_slice(b"LM363");

        for (offset, next, previous, id, description) in [
            (0x130, 0x250, 0, 1, &b"Original\0"[..]),
            (0x250, 0, 0x130, 2, &b"Edited\0"[..]),
        ] {
            put_u64(&mut bytes, offset, next);
            put_u64(&mut bytes, offset + 8, previous);
            put_u32(&mut bytes, offset + 0x18, 4);
            put_u32(&mut bytes, offset + 0x28, 4);
            put_u32(&mut bytes, offset + 0x30, 0x110);
            put_u32(
                &mut bytes,
                offset + 0x38,
                u32::try_from(description.len()).unwrap(),
            );
            bytes[offset + 0x3c..offset + 0x40].copy_from_slice(b"DIRL");
            put_u32(&mut bytes, offset + 0x40, 0x0363_c001);
            put_u32(&mut bytes, offset + 0x48, id);
            put_u32(&mut bytes, offset + 0x4c, 0x07ea_071b);
            put_u32(&mut bytes, offset + 0x50, 0x08_0200);
            put_u32(&mut bytes, offset + 0x58, 0x0113_2804);
            bytes[offset + 0x100..offset + 0x100 + description.len()].copy_from_slice(description);
            bytes[offset + 0x110..offset + 0x114].copy_from_slice(&[u8::try_from(id).unwrap(); 4]);
        }
        bytes
    }

    #[test]
    fn decodes_linked_lunar_magic_records_and_payloads() {
        let bytes = archive();
        let decoded = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_eq!(decoded.header.format_version, [0, 2]);
        assert_eq!(decoded.header.next_record_id, 3);
        assert_eq!(
            decoded.header.last_modified,
            PackedRestoreDate {
                year: 2026,
                month: 7,
                day: 27
            }
        );
        assert_eq!(decoded.records.len(), 2);
        assert_eq!(decoded.records[0].description_text(), "Original");
        assert_eq!(decoded.records[1].record_id, 2);
        assert_eq!(decoded.records[1].rom_size, 0x08_0200);
        assert_eq!(
            decoded.records[1].created_time,
            PackedRestoreTime {
                day_of_week: 1,
                hour: 19,
                minute: 40,
                second: 4
            }
        );
        assert_eq!(decoded.records[1].stored_payload(&bytes).unwrap(), &[2; 4]);
    }

    #[test]
    fn rejects_cycles_and_broken_backward_links() {
        let mut bytes = archive();
        put_u64(&mut bytes, 0x250, 0x130);
        assert_eq!(
            LunarRestoreArchive::decode(&bytes),
            Err(LunarRestoreArchiveError::RecordCycle(0x130))
        );

        let mut bytes = archive();
        put_u64(&mut bytes, 0x250 + 8, 0);
        assert_eq!(
            LunarRestoreArchive::decode(&bytes),
            Err(LunarRestoreArchiveError::BrokenPreviousLink {
                record: 0x250,
                expected: 0x130,
                actual: 0
            })
        );
    }

    #[test]
    fn rejects_payloads_outside_the_archive_before_exposing_records() {
        let mut bytes = archive();
        put_u32(&mut bytes, 0x130 + 0x28, u32::MAX);
        assert!(matches!(
            LunarRestoreArchive::decode(&bytes),
            Err(LunarRestoreArchiveError::FieldOutOfBounds {
                field: "record payload",
                ..
            })
        ));
    }

    #[test]
    fn inflates_commands_and_reconstructs_the_selected_rom() {
        let commands = [
            0x00, 4, 0, 0, 2, 9, // Fill two bytes at $000004.
            0x10, 1, 0, 0, 2, 7, 8, // Copy two bytes at $000001.
            0xff,
        ];
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&commands).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut bytes = archive();
        put_u64(&mut bytes, 0x130, 0);
        put_u64(&mut bytes, 0x18, 0x130);
        put_u32(
            &mut bytes,
            0x130 + 0x28,
            u32::try_from(compressed.len()).unwrap(),
        );
        put_u32(&mut bytes, 0x130 + 0x30, 0x110);
        put_u32(&mut bytes, 0x130 + 0x50, 8);
        let checksum = commands
            .iter()
            .fold(0_u32, |sum, byte| sum.wrapping_add(u32::from(*byte)))
            ^ DECODED_CHECKSUM_XOR;
        put_u32(&mut bytes, 0x130 + 0x24, checksum);
        bytes[0x240..0x240 + compressed.len()].copy_from_slice(&compressed);

        let archive = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_eq!(
            archive.records[0].commands(archive.bytes()).unwrap(),
            [
                LunarRestoreCommand::Fill {
                    offset: 4,
                    length: 2,
                    value: 9
                },
                LunarRestoreCommand::Raw {
                    offset: 1,
                    bytes: vec![7, 8]
                }
            ]
        );
        assert_eq!(
            archive.restore_through(1, &[1, 2, 3, 4]).unwrap(),
            [1, 7, 8, 4, 9, 9, 0, 0]
        );
    }
}
