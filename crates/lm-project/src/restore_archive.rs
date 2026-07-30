use flate2::{Compression, read::DeflateDecoder, write::DeflateEncoder};
use std::{
    collections::BTreeSet,
    error::Error,
    fmt,
    io::{Read, Write},
};

const ARCHIVE_PREFIX_LEN: usize = 0x130;
const ARCHIVE_HEADER_LEN: usize = 0x100;
const RECORD_HEADER_LEN: usize = 0x100;
const MAX_RECORDS: usize = 1_000_000;
const MAX_COMMAND_STREAM_LEN: u64 = 0x200_0000;
const MAX_RESTORED_ROM_LEN: usize = 0x100_0000;
const MAX_ASSOCIATED_FILE_LEN: u64 = 0x1000_0000;
const DECODED_CHECKSUM_XOR: u32 = 0xfade_c0de;
pub const LUNAR_RESTORE_ASSOCIATED_FILE_COUNT: usize = 13;

/// The thirteen ROM-adjacent files tracked by Lunar Magic restore points, in on-disk slot order.
pub const LUNAR_RESTORE_ASSOCIATED_EXTENSIONS: [&str; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT] = [
    "msc", "dsc", "ssc", "m16", "s16", "mwt", "mw2", "sscov", "s16ov", "lmtbl", "mw0t", "mw0",
    "osc",
];

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
    pub latest_record_id: u32,
    pub latest_record_sequence: u32,
    pub last_modified: PackedRestoreDate,
    pub first_record_offset: u64,
    pub last_record_offset: u64,
    pub last_rom_timestamp: u64,
    pub latest_rom_hash: u32,
    pub associated_file_timestamps: [u64; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT],
    pub reserved: Vec<u8>,
    pub producer: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LunarRestorePointRecord {
    pub archive_offset: u64,
    pub next_record_offset: u64,
    pub previous_record_offset: u64,
    pub reversion_target_offset: u64,
    pub record_size: u32,
    pub payload_checksum: u32,
    pub decoded_payload_checksum: u32,
    pub stored_payload_size: u32,
    pub payload_offset: u32,
    pub description: Vec<u8>,
    pub directory_version: u32,
    pub record_sequence: u32,
    pub record_id: u32,
    pub created: PackedRestoreDate,
    pub rom_size: u32,
    pub created_time: PackedRestoreTime,
    pub rom_variant: u32,
    pub rom_hash: u32,
    pub associated_files: [LunarRestoreAssociatedFileEntry; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT],
    pub raw_header: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LunarRestoreAssociatedFileEntry {
    /// Record-relative archive offset. Zero means that this record inherits the previous value.
    pub relative_offset: u32,
    /// Stored byte count. The record compression flag also applies to this sidecar.
    pub stored_size: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LunarRestoredAssociatedFile {
    pub extension: &'static str,
    pub bytes: Vec<u8>,
}

pub struct LunarRestoreArchiveCreateRequest<'a> {
    pub original_rom: &'a [u8],
    pub current_rom: &'a [u8],
    pub description: &'a str,
    pub created: PackedRestoreDate,
    pub created_time: PackedRestoreTime,
    pub rom_variant: u32,
    pub last_rom_timestamp: u64,
    pub compress: bool,
    pub associated_files: [Option<&'a [u8]>; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT],
    pub associated_file_timestamps: [u64; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT],
    pub producer: [u8; 0x30],
}

pub struct LunarRestoreReversionRequest<'a> {
    pub target_record_id: u32,
    pub restored_rom: &'a [u8],
    pub created: PackedRestoreDate,
    pub created_time: PackedRestoreTime,
    pub last_rom_timestamp: u64,
    pub associated_file_timestamps: [u64; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT],
}

impl<'a> LunarRestoreArchiveCreateRequest<'a> {
    #[must_use]
    pub fn new(
        original_rom: &'a [u8],
        current_rom: &'a [u8],
        description: &'a str,
        created: PackedRestoreDate,
        created_time: PackedRestoreTime,
    ) -> Self {
        let mut producer = [b' '; 0x30];
        producer[..24].copy_from_slice(b"Lunar Magic Rust restore");
        Self {
            original_rom,
            current_rom,
            description,
            created,
            created_time,
            rom_variant: 0,
            last_rom_timestamp: 0,
            compress: true,
            associated_files: [None; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT],
            associated_file_timestamps: [0; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT],
            producer,
        }
    }
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
    /// Creates a one-record full Lunar Restore archive.
    ///
    /// The record uses Lunar Magic's native command, checksum, compression, directory, and
    /// associated-file layout and can be appended with later delta records by a future writer.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized ROMs, embedded-NUL descriptions, arithmetic overflow, or
    /// compression failure.
    #[allow(clippy::too_many_lines)] // The linear assignments mirror one fixed binary record.
    pub fn create_full(
        request: &LunarRestoreArchiveCreateRequest<'_>,
    ) -> Result<Vec<u8>, LunarRestoreArchiveError> {
        if request.original_rom.len() > MAX_RESTORED_ROM_LEN {
            return Err(LunarRestoreArchiveError::RestoredRomTooLarge(
                request.original_rom.len(),
            ));
        }
        if request.current_rom.len() > MAX_RESTORED_ROM_LEN {
            return Err(LunarRestoreArchiveError::RestoredRomTooLarge(
                request.current_rom.len(),
            ));
        }
        if request.description.as_bytes().contains(&0) {
            return Err(LunarRestoreArchiveError::DescriptionContainsNul);
        }
        let description = if request.description.is_empty() {
            b"(unspecified)".as_slice()
        } else {
            request.description.as_bytes()
        };
        let mut description_bytes = description.to_vec();
        description_bytes.push(0);

        let commands = encode_rom_delta(request.original_rom, request.current_rom)?;
        let stored_payload = maybe_deflate(&commands, request.compress)?;
        let mut stored_sidecars: [Option<Vec<u8>>; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT] =
            std::array::from_fn(|_| None);
        for (slot, source) in request.associated_files.iter().enumerate() {
            if let Some(bytes) = source {
                stored_sidecars[slot] = Some(if request.compress && !bytes.is_empty() {
                    deflate(bytes)?
                } else {
                    bytes.to_vec()
                });
            }
        }

        let record_offset = ARCHIVE_PREFIX_LEN;
        let payload_offset = RECORD_HEADER_LEN
            .checked_add(description_bytes.len())
            .ok_or(LunarRestoreArchiveError::AddressOverflow(
                record_offset as u64,
            ))?;
        let mut record = vec![0; payload_offset];
        record[RECORD_HEADER_LEN..].copy_from_slice(&description_bytes);
        record.extend_from_slice(&stored_payload);
        for (slot, sidecar) in stored_sidecars.iter().enumerate() {
            let Some(sidecar) = sidecar else {
                continue;
            };
            let relative_offset = u32::try_from(record.len())
                .map_err(|_| LunarRestoreArchiveError::AddressOverflow(record.len() as u64))?;
            put_u32_at(&mut record, 0x80 + slot * 8, relative_offset);
            put_u32_at(
                &mut record,
                0x84 + slot * 8,
                u32::try_from(sidecar.len()).map_err(|_| {
                    LunarRestoreArchiveError::AssociatedFileTooLarge {
                        extension: LUNAR_RESTORE_ASSOCIATED_EXTENSIONS[slot],
                        length: sidecar.len(),
                    }
                })?,
            );
            record.extend_from_slice(sidecar);
        }

        let record_size = u32::try_from(record.len())
            .map_err(|_| LunarRestoreArchiveError::AddressOverflow(record.len() as u64))?;
        put_u32_at(&mut record, 0x18, record_size);
        put_u32_at(
            &mut record,
            0x24,
            byte_sum(&commands) ^ DECODED_CHECKSUM_XOR,
        );
        put_u32_at(
            &mut record,
            0x28,
            u32::try_from(stored_payload.len()).map_err(|_| {
                LunarRestoreArchiveError::CommandStreamTooLarge(stored_payload.len())
            })?,
        );
        put_u32_at(
            &mut record,
            0x30,
            u32::try_from(payload_offset)
                .map_err(|_| LunarRestoreArchiveError::AddressOverflow(payload_offset as u64))?,
        );
        put_u32_at(&mut record, 0x34, 0x100);
        let description_length = u32::try_from(description_bytes.len()).map_err(|_| {
            LunarRestoreArchiveError::AddressOverflow(description_bytes.len() as u64)
        })?;
        put_u32_at(&mut record, 0x38, description_length);
        record[0x3c..0x40].copy_from_slice(b"DIRL");
        put_u32_at(
            &mut record,
            0x40,
            0x0363_8001 | if request.compress { 0x4000 } else { 0 },
        );
        put_u32_at(&mut record, 0x48, 1);
        put_u32_at(&mut record, 0x4c, encode_date(request.created));
        put_u32_at(
            &mut record,
            0x50,
            u32::try_from(request.current_rom.len()).map_err(|_| {
                LunarRestoreArchiveError::RestoredRomTooLarge(request.current_rom.len())
            })?,
        );
        put_u32_at(&mut record, 0x58, encode_time(request.created_time));
        put_u32_at(&mut record, 0x5c, request.rom_variant);
        put_u32_at(
            &mut record,
            0x60,
            logical_restore_crc32(request.current_rom),
        );
        let stored_checksum = byte_sum(&record[0x30..])
            ^ if request.compress {
                DECODED_CHECKSUM_XOR
            } else {
                0xc001_c0de
            };
        put_u32_at(&mut record, 0x20, stored_checksum);

        let mut archive = vec![0; ARCHIVE_PREFIX_LEN];
        archive[0..4].copy_from_slice(b"LR\0\x02");
        put_u32_at(&mut archive, 8, 1);
        put_u32_at(&mut archive, 0x0c, encode_date(request.created));
        put_u64_at(&mut archive, 0x10, record_offset as u64);
        put_u64_at(&mut archive, 0x18, record_offset as u64);
        put_u32_at(&mut archive, 0x20, 0);
        put_u64_at(&mut archive, 0x28, request.last_rom_timestamp);
        put_u32_at(
            &mut archive,
            0x30,
            logical_restore_crc32(request.current_rom),
        );
        for (slot, timestamp) in request.associated_file_timestamps.iter().enumerate() {
            put_u64_at(&mut archive, 0x40 + slot * 8, *timestamp);
        }
        archive[ARCHIVE_HEADER_LEN..ARCHIVE_PREFIX_LEN].copy_from_slice(&request.producer);
        archive.extend_from_slice(&record);
        Ok(archive)
    }

    /// Appends a native delta record to this archive.
    ///
    /// `request.original_rom` must be the exact ROM state represented by the current last record;
    /// `request.current_rom` is the new state. Associated-file slots should contain only files
    /// changed for this record, while the timestamp array represents the complete latest snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the base ROM does not match the archive tip, metadata counters
    /// overflow, or record construction/validation fails.
    pub fn append_delta(
        &self,
        request: &LunarRestoreArchiveCreateRequest<'_>,
    ) -> Result<Vec<u8>, LunarRestoreArchiveError> {
        let last = self
            .records
            .last()
            .ok_or(LunarRestoreArchiveError::CannotAppendToEmptyArchive)?;
        let base_hash = logical_restore_crc32(request.original_rom);
        if request.original_rom.len() != last.rom_size as usize || base_hash != last.rom_hash {
            return Err(LunarRestoreArchiveError::AppendBaseMismatch {
                expected_size: last.rom_size,
                actual_size: request.original_rom.len(),
                expected_hash: last.rom_hash,
                actual_hash: base_hash,
            });
        }
        self.append_record(request, false)
    }

    /// Appends a full checkpoint encoded against the original ROM supplied in `request`.
    ///
    /// Full checkpoints restart later reconstruction from the original image and must include the
    /// complete desired associated-file snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when metadata counters overflow or record construction/validation fails.
    pub fn append_full(
        &self,
        request: &LunarRestoreArchiveCreateRequest<'_>,
    ) -> Result<Vec<u8>, LunarRestoreArchiveError> {
        if self.records.is_empty() {
            return Err(LunarRestoreArchiveError::CannotAppendToEmptyArchive);
        }
        self.append_record(request, true)
    }

    fn append_record(
        &self,
        request: &LunarRestoreArchiveCreateRequest<'_>,
        full: bool,
    ) -> Result<Vec<u8>, LunarRestoreArchiveError> {
        let last = self
            .records
            .last()
            .ok_or(LunarRestoreArchiveError::CannotAppendToEmptyArchive)?;
        let new_id = self
            .header
            .latest_record_id
            .checked_add(1)
            .ok_or(LunarRestoreArchiveError::RestoreMetadataOverflow)?;
        let new_sequence = self
            .header
            .latest_record_sequence
            .checked_add(1)
            .ok_or(LunarRestoreArchiveError::RestoreMetadataOverflow)?;
        let new_offset = u64::try_from(self.bytes.len())
            .map_err(|_| LunarRestoreArchiveError::AddressOverflow(self.bytes.len() as u64))?;

        let single = Self::create_full(request)?;
        let mut record = single[ARCHIVE_PREFIX_LEN..].to_vec();
        put_u64_at(&mut record, 8, last.archive_offset);
        let mut directory_version = read_u32(&record, 0x40, "directory version")?;
        if !full {
            directory_version &= !3;
        }
        put_u32_at(&mut record, 0x40, directory_version);
        put_u32_at(&mut record, 0x44, new_sequence);
        put_u32_at(&mut record, 0x48, new_id);
        reseal_stored_record_checksum(&mut record)?;

        let mut archive = self.bytes.clone();
        let last_offset = usize::try_from(last.archive_offset)
            .map_err(|_| LunarRestoreArchiveError::AddressOverflow(last.archive_offset))?;
        put_u64_at(&mut archive, last_offset, new_offset);
        put_u32_at(&mut archive, 8, new_id);
        put_u32_at(&mut archive, 0x0c, encode_date(request.created));
        put_u64_at(&mut archive, 0x18, new_offset);
        put_u32_at(&mut archive, 0x20, new_sequence);
        put_u64_at(&mut archive, 0x28, request.last_rom_timestamp);
        put_u32_at(
            &mut archive,
            0x30,
            logical_restore_crc32(request.current_rom),
        );
        for (slot, timestamp) in request.associated_file_timestamps.iter().enumerate() {
            put_u64_at(&mut archive, 0x40 + slot * 8, *timestamp);
        }
        archive.extend_from_slice(&record);
        Self::decode(&archive)?;
        Ok(archive)
    }

    /// Appends a successful reversion marker targeting an existing restore point.
    ///
    /// Consecutive reversion markers reuse the prior marker's ID and file extent, matching Lunar
    /// Magic's replacement behavior.
    ///
    /// # Errors
    ///
    /// Returns an error when the target does not exist, the supplied restored ROM differs from
    /// that target, metadata overflows, or the resulting archive fails validation.
    pub fn append_reversion(
        &self,
        request: &LunarRestoreReversionRequest<'_>,
    ) -> Result<Vec<u8>, LunarRestoreArchiveError> {
        if request.restored_rom.len() > MAX_RESTORED_ROM_LEN {
            return Err(LunarRestoreArchiveError::RestoredRomTooLarge(
                request.restored_rom.len(),
            ));
        }
        let target_chain = self.restore_record_indices(request.target_record_id)?;
        let target = &self.records[*target_chain.last().ok_or(
            LunarRestoreArchiveError::MissingFullRestorePoint(request.target_record_id),
        )?];
        let restored_hash = logical_restore_crc32(request.restored_rom);
        if request.restored_rom.len() != target.rom_size as usize
            || restored_hash != target.rom_hash
        {
            return Err(LunarRestoreArchiveError::AppendBaseMismatch {
                expected_size: target.rom_size,
                actual_size: request.restored_rom.len(),
                expected_hash: target.rom_hash,
                actual_hash: restored_hash,
            });
        }
        let old_last = self
            .records
            .last()
            .ok_or(LunarRestoreArchiveError::CannotAppendToEmptyArchive)?;
        let replacing = old_last.directory_version & 4 != 0;
        let (previous_offset, new_offset, new_id, mut archive) = if replacing {
            (
                old_last.previous_record_offset,
                old_last.archive_offset,
                old_last.record_id,
                self.bytes[..usize::try_from(old_last.archive_offset).map_err(|_| {
                    LunarRestoreArchiveError::AddressOverflow(old_last.archive_offset)
                })?]
                    .to_vec(),
            )
        } else {
            (
                old_last.archive_offset,
                u64::try_from(self.bytes.len()).map_err(|_| {
                    LunarRestoreArchiveError::AddressOverflow(self.bytes.len() as u64)
                })?,
                self.header
                    .latest_record_id
                    .checked_add(1)
                    .ok_or(LunarRestoreArchiveError::RestoreMetadataOverflow)?,
                self.bytes.clone(),
            )
        };

        let description = format!("Reverted to save point #{}.", request.target_record_id);
        let mut record = build_reversion_record(
            previous_offset,
            target,
            new_id,
            &description,
            request,
            restored_hash,
        )?;
        reseal_stored_record_checksum(&mut record)?;
        let previous = self
            .records
            .iter()
            .find(|record| record.archive_offset == previous_offset)
            .ok_or(LunarRestoreArchiveError::BrokenRestoreChain {
                record: new_offset,
                target: previous_offset,
            })?;
        put_u64_at(
            &mut archive,
            usize::try_from(previous.archive_offset)
                .map_err(|_| LunarRestoreArchiveError::AddressOverflow(previous.archive_offset))?,
            new_offset,
        );
        put_u32_at(&mut archive, 8, new_id);
        put_u32_at(&mut archive, 0x0c, encode_date(request.created));
        put_u64_at(&mut archive, 0x18, new_offset);
        put_u64_at(&mut archive, 0x28, request.last_rom_timestamp);
        put_u32_at(&mut archive, 0x30, restored_hash);
        for (slot, timestamp) in request.associated_file_timestamps.iter().enumerate() {
            put_u64_at(&mut archive, 0x40 + slot * 8, *timestamp);
        }
        archive.extend_from_slice(&record);
        Self::decode(&archive)?;
        Ok(archive)
    }

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
            latest_record_id: read_u32(bytes, 8, "latest record id")?,
            latest_record_sequence: read_u32(bytes, 0x20, "latest record sequence")?,
            last_modified: PackedRestoreDate::decode(read_u32(bytes, 0x0c, "archive date")?),
            first_record_offset: read_u64(bytes, 0x10, "first record offset")?,
            last_record_offset: read_u64(bytes, 0x18, "last record offset")?,
            last_rom_timestamp: read_u64(bytes, 0x28, "last ROM timestamp")?,
            latest_rom_hash: read_u32(bytes, 0x30, "latest ROM hash")?,
            associated_file_timestamps: read_associated_timestamps(bytes)?,
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
            validate_record_checksum(bytes, &record)?;
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
        for record in &records {
            let observed_end = record
                .archive_offset
                .checked_add(u64::from(record.record_size))
                .ok_or(LunarRestoreArchiveError::AddressOverflow(
                    record.archive_offset,
                ))?;
            let expected_end = if record.next_record_offset == 0 {
                bytes.len() as u64
            } else {
                record.next_record_offset
            };
            let expected_size = expected_end.saturating_sub(record.archive_offset);
            if expected_end < record.archive_offset || observed_end != expected_end {
                return Err(LunarRestoreArchiveError::RecordSizeMismatch {
                    record: record.archive_offset,
                    expected: expected_size,
                    actual: record.record_size,
                });
            }
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
        let record_indices = self.restore_record_indices(record_id)?;
        let mut restored = original_rom.to_vec();
        for index in record_indices {
            let record = &self.records[index];
            apply_commands(&mut restored, &record.commands(&self.bytes)?)?;
            let target_len = record.rom_size as usize;
            if target_len > MAX_RESTORED_ROM_LEN {
                return Err(LunarRestoreArchiveError::RestoredRomTooLarge(target_len));
            }
            restored.resize(target_len, 0);
            let header_len = match lm_rom::detect_copier_header(restored.len()) {
                lm_rom::CopierHeader::Present => lm_rom::COPIER_HEADER_LEN,
                lm_rom::CopierHeader::Absent => 0,
            };
            let actual_hash = restore_crc32(&restored[header_len..]);
            if actual_hash != record.rom_hash {
                return Err(LunarRestoreArchiveError::RomHashMismatch {
                    record: record.archive_offset,
                    expected: record.rom_hash,
                    actual: actual_hash,
                });
            }
        }
        Ok(restored)
    }

    /// Reconstructs the latest value of every associated file captured through `record_id`.
    ///
    /// Lunar Magic stores only changed slots in each directory record. A nonzero slot therefore
    /// replaces the value inherited from earlier records; a zero slot leaves it unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error when the record does not exist, an inherited range is outside the archive,
    /// or a compressed associated file is invalid or exceeds the bounded output limit.
    pub fn restore_associated_files_through(
        &self,
        record_id: u32,
    ) -> Result<Vec<LunarRestoredAssociatedFile>, LunarRestoreArchiveError> {
        let mut resolved: [Option<(u64, u32, bool)>; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT] =
            [None; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT];
        for index in self.restore_record_indices(record_id)? {
            let record = &self.records[index];
            for (slot, entry) in record.associated_files.iter().enumerate() {
                if entry.relative_offset != 0 {
                    resolved[slot] = Some((
                        associated_file_address(record, *entry)?,
                        entry.stored_size,
                        record.compressed(),
                    ));
                }
            }
        }

        resolved
            .into_iter()
            .enumerate()
            .filter_map(|(slot, entry)| entry.map(|entry| (slot, entry)))
            .map(|(slot, (offset, stored_size, compressed))| {
                let stored = checked_slice(
                    &self.bytes,
                    usize::try_from(offset)
                        .map_err(|_| LunarRestoreArchiveError::AddressOverflow(offset))?,
                    stored_size as usize,
                    "associated restore file",
                )?;
                let bytes = if compressed && !stored.is_empty() {
                    let mut output = Vec::new();
                    DeflateDecoder::new(stored)
                        .take(MAX_ASSOCIATED_FILE_LEN + 1)
                        .read_to_end(&mut output)
                        .map_err(|error| LunarRestoreArchiveError::AssociatedFileInflate {
                            extension: LUNAR_RESTORE_ASSOCIATED_EXTENSIONS[slot],
                            error: error.to_string(),
                        })?;
                    if u64::try_from(output.len()).unwrap_or(u64::MAX) > MAX_ASSOCIATED_FILE_LEN {
                        return Err(LunarRestoreArchiveError::AssociatedFileTooLarge {
                            extension: LUNAR_RESTORE_ASSOCIATED_EXTENSIONS[slot],
                            length: output.len(),
                        });
                    }
                    output
                } else {
                    stored.to_vec()
                };
                Ok(LunarRestoredAssociatedFile {
                    extension: LUNAR_RESTORE_ASSOCIATED_EXTENSIONS[slot],
                    bytes,
                })
            })
            .collect()
    }

    fn restore_record_indices(
        &self,
        record_id: u32,
    ) -> Result<Vec<usize>, LunarRestoreArchiveError> {
        let mut current = self
            .records
            .iter()
            .position(|record| record.record_id == record_id)
            .ok_or(LunarRestoreArchiveError::UnknownRecordId(record_id))?;
        let mut chain = Vec::new();
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(current) {
                return Err(LunarRestoreArchiveError::RestoreChainCycle(
                    self.records[current].archive_offset,
                ));
            }
            let record = &self.records[current];
            if record.directory_version & 4 == 0 {
                chain.push(current);
                if record.directory_version & 3 != 0 {
                    chain.reverse();
                    return Ok(chain);
                }
            }
            let prior_offset = if record.directory_version & 4 != 0 {
                record.reversion_target_offset
            } else {
                record.previous_record_offset
            };
            current = self
                .records
                .iter()
                .position(|candidate| candidate.archive_offset == prior_offset)
                .ok_or(LunarRestoreArchiveError::BrokenRestoreChain {
                    record: record.archive_offset,
                    target: prior_offset,
                })?;
        }
    }
}

fn encode_rom_delta(original: &[u8], current: &[u8]) -> Result<Vec<u8>, LunarRestoreArchiveError> {
    let mut encoded = Vec::new();
    let mut cursor = 0;
    while cursor < current.len() {
        if original.get(cursor) == current.get(cursor) {
            cursor += 1;
            continue;
        }
        let start = cursor;
        cursor += 1;
        while cursor < current.len() && original.get(cursor) != current.get(cursor) {
            cursor += 1;
        }
        let bytes = &current[start..cursor];
        let length = u32::try_from(bytes.len())
            .map_err(|_| LunarRestoreArchiveError::CommandStreamTooLarge(bytes.len()))?;
        let length_width = variable_width(length);
        let length_control = u8::try_from(length_width - 1)
            .map_err(|_| LunarRestoreArchiveError::CommandStreamTooLarge(bytes.len()))?;
        encoded.push(0x10 | length_control);
        let encoded_start = u32::try_from(start)
            .map_err(|_| LunarRestoreArchiveError::CommandAddressOverflow { offset: 0, length })?;
        encoded.extend_from_slice(&encoded_start.to_le_bytes()[..3]);
        encoded.extend_from_slice(&length.to_le_bytes()[..length_width]);
        encoded.extend_from_slice(bytes);
    }
    encoded.push(0xff);
    Ok(encoded)
}

fn build_reversion_record(
    previous_offset: u64,
    target: &LunarRestorePointRecord,
    record_id: u32,
    description: &str,
    request: &LunarRestoreReversionRequest<'_>,
    restored_hash: u32,
) -> Result<Vec<u8>, LunarRestoreArchiveError> {
    let mut description_bytes = description.as_bytes().to_vec();
    description_bytes.push(0);
    let record_size = RECORD_HEADER_LEN
        .checked_add(description_bytes.len())
        .ok_or(LunarRestoreArchiveError::RestoreMetadataOverflow)?;
    let mut record = vec![0; record_size];
    record[RECORD_HEADER_LEN..].copy_from_slice(&description_bytes);
    put_u64_at(&mut record, 8, previous_offset);
    put_u64_at(&mut record, 0x10, target.archive_offset);
    put_u32_at(
        &mut record,
        0x18,
        u32::try_from(record_size)
            .map_err(|_| LunarRestoreArchiveError::RestoreMetadataOverflow)?,
    );
    put_u32_at(
        &mut record,
        0x30,
        u32::try_from(record_size)
            .map_err(|_| LunarRestoreArchiveError::RestoreMetadataOverflow)?,
    );
    put_u32_at(&mut record, 0x34, 0x100);
    put_u32_at(
        &mut record,
        0x38,
        u32::try_from(description_bytes.len())
            .map_err(|_| LunarRestoreArchiveError::RestoreMetadataOverflow)?,
    );
    record[0x3c..0x40].copy_from_slice(b"DIRL");
    put_u32_at(&mut record, 0x40, 0x0363_8004);
    put_u32_at(&mut record, 0x44, target.record_sequence);
    put_u32_at(&mut record, 0x48, record_id);
    put_u32_at(&mut record, 0x4c, encode_date(request.created));
    put_u32_at(
        &mut record,
        0x50,
        u32::try_from(request.restored_rom.len()).map_err(|_| {
            LunarRestoreArchiveError::RestoredRomTooLarge(request.restored_rom.len())
        })?,
    );
    put_u32_at(&mut record, 0x58, encode_time(request.created_time));
    put_u32_at(&mut record, 0x5c, target.rom_variant);
    put_u32_at(&mut record, 0x60, restored_hash);
    Ok(record)
}

const fn variable_width(value: u32) -> usize {
    if value <= 0xff {
        1
    } else if value <= 0xffff {
        2
    } else if value <= 0xff_ffff {
        3
    } else {
        4
    }
}

fn maybe_deflate(bytes: &[u8], compress: bool) -> Result<Vec<u8>, LunarRestoreArchiveError> {
    if compress {
        deflate(bytes)
    } else {
        Ok(bytes.to_vec())
    }
}

fn deflate(bytes: &[u8]) -> Result<Vec<u8>, LunarRestoreArchiveError> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(bytes)
        .and_then(|()| encoder.finish())
        .map_err(|error| LunarRestoreArchiveError::Deflate(error.to_string()))
}

fn byte_sum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0_u32, |sum, byte| sum.wrapping_add(u32::from(*byte)))
}

fn reseal_stored_record_checksum(record: &mut [u8]) -> Result<(), LunarRestoreArchiveError> {
    let compressed = read_u32(record, 0x40, "directory version")? & 0x4000 != 0;
    let checksum = byte_sum(checked_slice(
        record,
        0x30,
        record.len().saturating_sub(0x30),
        "stored record checksum range",
    )?) ^ if compressed {
        DECODED_CHECKSUM_XOR
    } else {
        0xc001_c0de
    };
    put_u32_at(record, 0x20, checksum);
    Ok(())
}

fn logical_restore_crc32(bytes: &[u8]) -> u32 {
    let header_len = match lm_rom::detect_copier_header(bytes.len()) {
        lm_rom::CopierHeader::Present => lm_rom::COPIER_HEADER_LEN,
        lm_rom::CopierHeader::Absent => 0,
    };
    restore_crc32(&bytes[header_len..])
}

const fn encode_date(date: PackedRestoreDate) -> u32 {
    (date.year as u32) << 16 | (date.month as u32) << 8 | date.day as u32
}

const fn encode_time(time: PackedRestoreTime) -> u32 {
    (time.day_of_week as u32) << 24
        | (time.hour as u32) << 16
        | (time.minute as u32) << 8
        | time.second as u32
}

fn put_u32_at(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64_at(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_associated_timestamps(
    bytes: &[u8],
) -> Result<[u64; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT], LunarRestoreArchiveError> {
    let mut timestamps = [0; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT];
    for (slot, timestamp) in timestamps.iter_mut().enumerate() {
        *timestamp = read_u64(bytes, 0x40 + slot * 8, "associated file timestamp")?;
    }
    Ok(timestamps)
}

fn associated_file_address(
    record: &LunarRestorePointRecord,
    entry: LunarRestoreAssociatedFileEntry,
) -> Result<u64, LunarRestoreArchiveError> {
    let mut address = record
        .archive_offset
        .checked_add(u64::from(entry.relative_offset))
        .ok_or(LunarRestoreArchiveError::AddressOverflow(
            record.archive_offset,
        ))?;
    // LM versions before 3.21 recorded the end rather than the start for large sidecars.
    if entry.stored_size > 0x1_0000 && (record.directory_version & 0xffff_0000) < 0x0321_0000 {
        address = address.checked_sub(u64::from(entry.stored_size)).ok_or(
            LunarRestoreArchiveError::AssociatedFileAddressUnderflow {
                record: record.archive_offset,
                relative_offset: entry.relative_offset,
                stored_size: entry.stored_size,
            },
        )?;
    }
    Ok(address)
}

fn validate_record_checksum(
    archive: &[u8],
    record: &LunarRestorePointRecord,
) -> Result<(), LunarRestoreArchiveError> {
    let start = usize::try_from(record.archive_offset)
        .map_err(|_| LunarRestoreArchiveError::AddressOverflow(record.archive_offset))?;
    let description_length = read_u32(&record.raw_header, 0x38, "description length")? as usize;
    let mut sum = record.raw_header[0x30..]
        .iter()
        .fold(0_u32, |sum, byte| sum.wrapping_add(u32::from(*byte)));
    for byte in checked_slice(
        archive,
        start + RECORD_HEADER_LEN,
        description_length,
        "restore-point description",
    )? {
        sum = sum.wrapping_add(u32::from(*byte));
    }
    for byte in record.stored_payload(archive)? {
        sum = sum.wrapping_add(u32::from(*byte));
    }
    for entry in record.associated_files {
        if entry.relative_offset == 0 {
            continue;
        }
        let offset = associated_file_address(record, entry)?;
        for byte in checked_slice(
            archive,
            usize::try_from(offset)
                .map_err(|_| LunarRestoreArchiveError::AddressOverflow(offset))?,
            entry.stored_size as usize,
            "associated restore file",
        )? {
            sum = sum.wrapping_add(u32::from(*byte));
        }
    }
    let checksum_xor = if record.compressed() {
        DECODED_CHECKSUM_XOR
    } else {
        0xc001_c0de
    };
    let actual = sum ^ checksum_xor;
    if actual != record.payload_checksum {
        return Err(LunarRestoreArchiveError::StoredChecksumMismatch {
            record: record.archive_offset,
            expected: record.payload_checksum,
            actual,
        });
    }
    Ok(())
}

fn restore_crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
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
    let associated_files = std::array::from_fn(|slot| {
        let offset = 0x80 + slot * 8;
        LunarRestoreAssociatedFileEntry {
            relative_offset: u32::from_le_bytes(header[offset..offset + 4].try_into().unwrap()),
            stored_size: u32::from_le_bytes(header[offset + 4..offset + 8].try_into().unwrap()),
        }
    });

    Ok(LunarRestorePointRecord {
        archive_offset,
        next_record_offset: read_u64(header, 0, "next record offset")?,
        previous_record_offset: read_u64(header, 8, "previous record offset")?,
        reversion_target_offset: read_u64(header, 0x10, "reversion target offset")?,
        record_size: read_u32(header, 0x18, "record size")?,
        payload_checksum: read_u32(header, 0x20, "payload checksum")?,
        decoded_payload_checksum: read_u32(header, 0x24, "decoded payload checksum")?,
        stored_payload_size,
        payload_offset,
        description: description.to_vec(),
        directory_version: read_u32(header, 0x40, "directory version")?,
        record_sequence: read_u32(header, 0x44, "record sequence")?,
        record_id: read_u32(header, 0x48, "record id")?,
        created: PackedRestoreDate::decode(read_u32(header, 0x4c, "record date")?),
        rom_size: read_u32(header, 0x50, "ROM size")?,
        created_time: PackedRestoreTime::decode(read_u32(header, 0x58, "record time")?),
        rom_variant: read_u32(header, 0x5c, "ROM variant")?,
        rom_hash: read_u32(header, 0x60, "ROM hash")?,
        associated_files,
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
    DescriptionContainsNul,
    CannotAppendToEmptyArchive,
    MissingFullRestorePoint(u32),
    RestoreChainCycle(u64),
    BrokenRestoreChain {
        record: u64,
        target: u64,
    },
    RestoreMetadataOverflow,
    AppendBaseMismatch {
        expected_size: u32,
        actual_size: usize,
        expected_hash: u32,
        actual_hash: u32,
    },
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
    RecordSizeMismatch {
        record: u64,
        expected: u64,
        actual: u32,
    },
    TooManyRecords(usize),
    Inflate(String),
    Deflate(String),
    CommandStreamTooLarge(usize),
    DecodedChecksumMismatch {
        record: u64,
        expected: u32,
        actual: u32,
    },
    StoredChecksumMismatch {
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
    RomHashMismatch {
        record: u64,
        expected: u32,
        actual: u32,
    },
    AssociatedFileAddressUnderflow {
        record: u64,
        relative_offset: u32,
        stored_size: u32,
    },
    AssociatedFileInflate {
        extension: &'static str,
        error: String,
    },
    AssociatedFileTooLarge {
        extension: &'static str,
        length: usize,
    },
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
            Self::DescriptionContainsNul => {
                write!(
                    formatter,
                    "restore-point description contains an embedded NUL"
                )
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
            Self::RecordSizeMismatch {
                record,
                expected,
                actual,
            } => write!(
                formatter,
                "restore point at {record:#x} declares size {actual:#x}, expected {expected:#x}"
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
        E::Deflate(error) => write!(formatter, "cannot deflate restore payload: {error}"),
        append_error @ (E::CannotAppendToEmptyArchive
        | E::MissingFullRestorePoint(_)
        | E::RestoreChainCycle(_)
        | E::BrokenRestoreChain { .. }
        | E::RestoreMetadataOverflow
        | E::AppendBaseMismatch { .. }) => fmt_append_error(append_error, formatter),
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
        E::StoredChecksumMismatch {
            record,
            expected,
            actual,
        } => write!(
            formatter,
            "restore point at {record:#x} has stored checksum {actual:#010x}, expected {expected:#010x}"
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
        E::RomHashMismatch {
            record,
            expected,
            actual,
        } => write!(
            formatter,
            "restore point at {record:#x} produced ROM hash {actual:#010x}, expected {expected:#010x}"
        ),
        E::AssociatedFileAddressUnderflow {
            record,
            relative_offset,
            stored_size,
        } => write!(
            formatter,
            "restore point at {record:#x} has legacy associated-file range {relative_offset:#x}-{stored_size:#x} below the archive start"
        ),
        E::AssociatedFileInflate { extension, error } => {
            write!(
                formatter,
                "cannot inflate associated .{extension} file: {error}"
            )
        }
        E::AssociatedFileTooLarge { extension, length } => write!(
            formatter,
            "inflated associated .{extension} file is {length} bytes, above the supported limit"
        ),
        _ => unreachable!("non-command errors are formatted by the outer match"),
    }
}

fn fmt_append_error(
    error: &LunarRestoreArchiveError,
    formatter: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    use LunarRestoreArchiveError as E;
    match error {
        E::CannotAppendToEmptyArchive => {
            write!(formatter, "cannot append to an empty restore archive")
        }
        E::MissingFullRestorePoint(record_id) => write!(
            formatter,
            "restore point {record_id} has no preceding full checkpoint"
        ),
        E::RestoreChainCycle(record) => {
            write!(formatter, "restore chain cycles at record {record:#x}")
        }
        E::BrokenRestoreChain { record, target } => write!(
            formatter,
            "restore record {record:#x} points to missing chain target {target:#x}"
        ),
        E::RestoreMetadataOverflow => {
            write!(
                formatter,
                "restore archive ID or sequence counter overflowed"
            )
        }
        E::AppendBaseMismatch {
            expected_size,
            actual_size,
            expected_hash,
            actual_hash,
        } => write!(
            formatter,
            "append base is {actual_size:#x} bytes with hash {actual_hash:#010x}; archive tip expects {expected_size:#x} bytes and {expected_hash:#010x}"
        ),
        _ => unreachable!("non-append error passed to append formatter"),
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

    fn seal_record_checksum(bytes: &mut [u8], offset: usize) {
        let description_length = read_u32(bytes, offset + 0x38, "description").unwrap() as usize;
        let payload_size = read_u32(bytes, offset + 0x28, "payload size").unwrap() as usize;
        let payload_offset = read_u32(bytes, offset + 0x30, "payload offset").unwrap() as usize;
        let mut sum = bytes[offset + 0x30..offset + 0x100]
            .iter()
            .chain(&bytes[offset + 0x100..offset + 0x100 + description_length])
            .chain(&bytes[offset + payload_offset..offset + payload_offset + payload_size])
            .fold(0_u32, |sum, byte| sum.wrapping_add(u32::from(*byte)));
        for slot in 0..LUNAR_RESTORE_ASSOCIATED_FILE_COUNT {
            let entry_offset = offset + 0x80 + slot * 8;
            let relative = read_u32(bytes, entry_offset, "sidecar offset").unwrap() as usize;
            let size = read_u32(bytes, entry_offset + 4, "sidecar size").unwrap() as usize;
            if relative != 0 {
                sum = bytes[offset + relative..offset + relative + size]
                    .iter()
                    .fold(sum, |sum, byte| sum.wrapping_add(u32::from(*byte)));
            }
        }
        let version = read_u32(bytes, offset + 0x40, "version").unwrap();
        put_u32(
            bytes,
            offset + 0x20,
            sum ^ if version & 0x4000 != 0 {
                DECODED_CHECKSUM_XOR
            } else {
                0xc001_c0de
            },
        );
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
            put_u32(
                &mut bytes,
                offset + 0x18,
                if offset == 0x130 { 0x120 } else { 0x130 },
            );
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
            seal_record_checksum(&mut bytes, offset);
        }
        bytes
    }

    #[test]
    fn decodes_linked_lunar_magic_records_and_payloads() {
        let bytes = archive();
        let decoded = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_eq!(decoded.header.format_version, [0, 2]);
        assert_eq!(decoded.header.latest_record_id, 3);
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
    fn restores_changed_associated_files_and_inherits_unchanged_slots() {
        let mut bytes = archive();
        put_u32(&mut bytes, 0x130 + 0x40, 0x0363_8001);
        put_u32(&mut bytes, 0x250 + 0x40, 0x0363_8000);
        put_u32(&mut bytes, 0x130 + 0x80, 0x114);
        put_u32(&mut bytes, 0x130 + 0x84, 3);
        bytes[0x244..0x247].copy_from_slice(b"one");
        put_u32(&mut bytes, 0x250 + 0x88, 0x114);
        put_u32(&mut bytes, 0x250 + 0x8c, 3);
        bytes[0x364..0x367].copy_from_slice(b"two");
        seal_record_checksum(&mut bytes, 0x130);
        seal_record_checksum(&mut bytes, 0x250);

        let archive = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_eq!(
            archive.restore_associated_files_through(1).unwrap(),
            [LunarRestoredAssociatedFile {
                extension: "msc",
                bytes: b"one".to_vec(),
            }]
        );
        assert_eq!(
            archive.restore_associated_files_through(2).unwrap(),
            [
                LunarRestoredAssociatedFile {
                    extension: "msc",
                    bytes: b"one".to_vec(),
                },
                LunarRestoredAssociatedFile {
                    extension: "dsc",
                    bytes: b"two".to_vec(),
                },
            ]
        );
    }

    #[test]
    fn inflates_associated_files_with_the_owning_record_flags() {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"sidecar contents").unwrap();
        let compressed = encoder.finish().unwrap();
        let mut bytes = archive();
        put_u32(&mut bytes, 0x250 + 0x40, 0x0363_c001 | 0x4000);
        put_u32(&mut bytes, 0x250 + 0x90, 0x114);
        put_u32(
            &mut bytes,
            0x250 + 0x94,
            u32::try_from(compressed.len()).unwrap(),
        );
        bytes[0x364..0x364 + compressed.len()].copy_from_slice(&compressed);
        seal_record_checksum(&mut bytes, 0x250);

        let archive = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_eq!(
            archive.restore_associated_files_through(2).unwrap(),
            [LunarRestoredAssociatedFile {
                extension: "ssc",
                bytes: b"sidecar contents".to_vec(),
            }]
        );
    }

    #[test]
    fn applies_the_pre_321_large_sidecar_end_offset_compatibility_rule() {
        let record = LunarRestorePointRecord {
            archive_offset: 0x20_000,
            next_record_offset: 0,
            previous_record_offset: 0,
            reversion_target_offset: 0,
            record_size: 0,
            payload_checksum: 0,
            decoded_payload_checksum: 0,
            stored_payload_size: 0,
            payload_offset: 0x100,
            description: Vec::new(),
            directory_version: 0x0320_0000,
            record_sequence: 0,
            record_id: 1,
            created: PackedRestoreDate::decode(0),
            rom_size: 0,
            created_time: PackedRestoreTime::decode(0),
            rom_variant: 0,
            rom_hash: 0,
            associated_files: [LunarRestoreAssociatedFileEntry::default();
                LUNAR_RESTORE_ASSOCIATED_FILE_COUNT],
            raw_header: Vec::new(),
        };
        assert_eq!(
            associated_file_address(
                &record,
                LunarRestoreAssociatedFileEntry {
                    relative_offset: 0x2_0000,
                    stored_size: 0x1_0001,
                },
            )
            .unwrap(),
            0x2_ffff
        );
    }

    #[test]
    fn creates_native_full_archives_that_round_trip_rom_and_sidecars() {
        let original = [1, 2, 3, 4];
        let current = [1, 9, 9, 4, 5];
        for compress in [false, true] {
            let mut request = LunarRestoreArchiveCreateRequest::new(
                &original,
                &current,
                "Created in Rust",
                PackedRestoreDate {
                    year: 2026,
                    month: 7,
                    day: 30,
                },
                PackedRestoreTime {
                    day_of_week: 4,
                    hour: 21,
                    minute: 5,
                    second: 7,
                },
            );
            request.compress = compress;
            request.last_rom_timestamp = 0x1234_5678_9abc_def0;
            request.associated_files[0] = Some(b"sprite metadata");
            request.associated_files[4] = Some(b"");

            let bytes = LunarRestoreArchive::create_full(&request).unwrap();
            let archive = LunarRestoreArchive::decode(&bytes).unwrap();
            assert_eq!(archive.header.first_record_offset, 0x130);
            assert_eq!(archive.header.last_record_offset, 0x130);
            assert_eq!(archive.header.last_rom_timestamp, 0x1234_5678_9abc_def0);
            assert_eq!(archive.records[0].compressed(), compress);
            assert_eq!(archive.records[0].description_text(), "Created in Rust");
            assert_eq!(archive.restore_through(1, &original).unwrap(), current);
            assert_eq!(
                archive.restore_associated_files_through(1).unwrap(),
                [
                    LunarRestoredAssociatedFile {
                        extension: "msc",
                        bytes: b"sprite metadata".to_vec(),
                    },
                    LunarRestoredAssociatedFile {
                        extension: "s16",
                        bytes: Vec::new(),
                    },
                ]
            );
        }
    }

    #[test]
    fn appends_linked_delta_records_and_inherits_sidecars() {
        let original = [1, 2, 3, 4];
        let first = [1, 8, 3, 4];
        let second = [1, 8, 3, 9, 5];
        let date = PackedRestoreDate {
            year: 2026,
            month: 7,
            day: 30,
        };
        let time = PackedRestoreTime {
            day_of_week: 4,
            hour: 22,
            minute: 10,
            second: 0,
        };
        let mut initial =
            LunarRestoreArchiveCreateRequest::new(&original, &first, "First", date, time);
        initial.associated_files[0] = Some(b"first msc");
        initial.associated_file_timestamps[0] = 10;
        let bytes = LunarRestoreArchive::create_full(&initial).unwrap();
        let archive = LunarRestoreArchive::decode(&bytes).unwrap();

        let mut delta =
            LunarRestoreArchiveCreateRequest::new(&first, &second, "Second", date, time);
        delta.associated_files[1] = Some(b"second dsc");
        delta.associated_file_timestamps[0] = 10;
        delta.associated_file_timestamps[1] = 20;
        delta.last_rom_timestamp = 30;
        let bytes = archive.append_delta(&delta).unwrap();
        let archive = LunarRestoreArchive::decode(&bytes).unwrap();

        assert_eq!(archive.header.latest_record_id, 2);
        assert_eq!(archive.header.latest_record_sequence, 1);
        assert_eq!(archive.header.associated_file_timestamps[0..2], [10, 20]);
        assert_eq!(archive.records.len(), 2);
        assert_eq!(
            archive.records[0].next_record_offset,
            archive.records[1].archive_offset
        );
        assert_eq!(
            archive.records[1].previous_record_offset,
            archive.records[0].archive_offset
        );
        assert_eq!(archive.records[1].directory_version & 3, 0);
        assert_eq!(archive.records[1].record_id, 2);
        assert_eq!(archive.restore_through(1, &original).unwrap(), first);
        assert_eq!(archive.restore_through(2, &original).unwrap(), second);
        assert_eq!(
            archive.restore_associated_files_through(2).unwrap(),
            [
                LunarRestoredAssociatedFile {
                    extension: "msc",
                    bytes: b"first msc".to_vec(),
                },
                LunarRestoredAssociatedFile {
                    extension: "dsc",
                    bytes: b"second dsc".to_vec(),
                },
            ]
        );
        assert!(matches!(
            archive.append_delta(&initial),
            Err(LunarRestoreArchiveError::AppendBaseMismatch { .. })
        ));

        let third = [7, 7, 7, 7];
        let mut full = LunarRestoreArchiveCreateRequest::new(&original, &third, "Full", date, time);
        full.associated_files[0] = Some(b"full msc");
        let bytes = archive.append_full(&full).unwrap();
        let archive = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_eq!(archive.records[2].directory_version & 3, 1);
        assert_eq!(archive.restore_through(3, &original).unwrap(), third);
        assert_eq!(
            archive.restore_associated_files_through(3).unwrap(),
            [LunarRestoredAssociatedFile {
                extension: "msc",
                bytes: b"full msc".to_vec(),
            }]
        );

        let reversion = LunarRestoreReversionRequest {
            target_record_id: 1,
            restored_rom: &first,
            created: date,
            created_time: time,
            last_rom_timestamp: 40,
            associated_file_timestamps: [0; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT],
        };
        let bytes = archive.append_reversion(&reversion).unwrap();
        let archive = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_eq!(archive.records[3].directory_version & 4, 4);
        assert_eq!(
            archive.records[3].reversion_target_offset,
            archive.records[0].archive_offset
        );
        assert_eq!(archive.restore_through(4, &original).unwrap(), first);

        let replacement = LunarRestoreReversionRequest {
            target_record_id: 2,
            restored_rom: &second,
            ..reversion
        };
        let replacement_offset = archive.records[3].archive_offset;
        let bytes = archive.append_reversion(&replacement).unwrap();
        let archive = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_eq!(archive.records.len(), 4);
        assert_eq!(archive.records[3].record_id, 4);
        assert_eq!(archive.records[3].archive_offset, replacement_offset);
        assert_eq!(archive.restore_through(4, &original).unwrap(), second);

        let fourth = [6, 8, 3, 9, 5];
        let delta =
            LunarRestoreArchiveCreateRequest::new(&second, &fourth, "After reversion", date, time);
        let bytes = archive.append_delta(&delta).unwrap();
        let archive = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_eq!(archive.restore_through(5, &original).unwrap(), fourth);
    }

    #[test]
    fn rejects_stored_checksum_mismatches() {
        let mut bytes = archive();
        bytes[0x130 + 0x20] ^= 1;
        assert!(matches!(
            LunarRestoreArchive::decode(&bytes),
            Err(LunarRestoreArchiveError::StoredChecksumMismatch { .. })
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
        put_u32(&mut bytes, 0x130 + 0x18, 0x250);
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
        seal_record_checksum(&mut bytes, 0x130);

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
        let expected = [1, 7, 8, 4, 9, 9, 0, 0];
        put_u32(&mut bytes, 0x130 + 0x60, restore_crc32(&expected));
        seal_record_checksum(&mut bytes, 0x130);
        let archive = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_eq!(archive.restore_through(1, &[1, 2, 3, 4]).unwrap(), expected);
        assert!(matches!(
            archive.restore_through(1, &[0, 2, 3, 4]),
            Err(LunarRestoreArchiveError::RomHashMismatch { .. })
        ));
    }
}
