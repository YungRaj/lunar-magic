use crate::exanimation_io::exanimation_save_request;
use crate::{
    ExAnimationIoError, ExAnimationSaveOptions, InstalledExAnimationRomLayout, LevelLoadError,
    LevelPointerTable, PayloadSaveError, PayloadSaveResult, Project, RomWrite,
};
use lm_graphics::{
    CompactExAnimation, ExAnimationRecord, LEGACY_EXANIMATION_MAX_RECORDS,
    LEGACY_EXANIMATION_RECORD_LEN, LegacyExAnimationError, convert_legacy_exanimation_records,
};
use lm_rats::ProtectedRange;
use lm_rom::{Mapper, RomError, SnesPointer24, mapper_supports_image_len};
use std::fmt;
use std::ops::Range;

pub const LEGACY_EXANIMATION_LEVEL_COUNT: usize = 0x200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyExAnimationRomLayout {
    pub mapper: Mapper,
    pub pointers: LevelPointerTable,
}

/// Complete legacy-to-current ExAnimation migration boundary recovered from
/// `MigrateLegacyGlobalExAnimations` (`0045F980`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyExAnimationMigrationLayout {
    pub legacy: LegacyExAnimationRomLayout,
    /// The already-installed current 512-entry pointer table. Migration is allowed only while
    /// every selected pointer-presence bit remains zero.
    pub current: InstalledExAnimationRomLayout,
    /// The adjacent legacy `$140`-byte auxiliary table erased by Lunar Magic after conversion.
    pub legacy_auxiliary: Range<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyExAnimationMigrationResult {
    /// Slots that retained at least one active converted record and received a current payload.
    pub migrated_slots: Vec<usize>,
    /// Allocations in the same order as `migrated_slots`.
    pub allocations: Vec<PayloadSaveResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoadedLegacyExAnimationSlot {
    Empty,
    Present {
        payload_offset: usize,
        raw_count: u8,
        record_count: usize,
        records: Vec<ExAnimationRecord>,
    },
}

#[derive(Debug)]
pub enum LegacyExAnimationIoError {
    WrongPointerEntryCount(usize),
    WrongPointerStride(usize),
    Layout(LevelLoadError),
    Rom(RomError),
    Conversion(LegacyExAnimationError),
    PayloadOffsetOverflow(usize),
    MapperMismatch { legacy: Mapper, current: Mapper },
    InvalidCurrentPointerPresenceMask(u32),
    WrongCurrentPointerEntryCount(usize),
    WrongCurrentPointerStride(usize),
    CurrentRecordLimit(usize),
    WrongLegacyAuxiliaryLength(usize),
    CurrentPointerTableNotEmpty { slot: usize, pointer: u32 },
    MigrationEraseRangeOverlap { first: usize, second: usize },
    MigrationStorageOverlap { first: usize, second: usize },
    ExAnimation(ExAnimationIoError),
    Save(PayloadSaveError),
}

impl fmt::Display for LegacyExAnimationIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "legacy ExAnimation I/O failed: {self:?}")
    }
}

impl std::error::Error for LegacyExAnimationIoError {}

impl From<LevelLoadError> for LegacyExAnimationIoError {
    fn from(value: LevelLoadError) -> Self {
        Self::Layout(value)
    }
}

impl From<RomError> for LegacyExAnimationIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<LegacyExAnimationError> for LegacyExAnimationIoError {
    fn from(value: LegacyExAnimationError) -> Self {
        Self::Conversion(value)
    }
}

impl From<ExAnimationIoError> for LegacyExAnimationIoError {
    fn from(value: ExAnimationIoError) -> Self {
        Self::ExAnimation(value)
    }
}

impl From<PayloadSaveError> for LegacyExAnimationIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl Project {
    /// Loads one slot from the exact legacy 512-entry ExAnimation pointer table consumed by Lunar
    /// Magic's migration routine.
    ///
    /// Pointer presence is the original bank-byte test, not a generic nonzero-pointer test. A
    /// present pointer addresses one raw count byte followed immediately by `$23 * count` bytes.
    /// Lunar Magic masks the count with `$3F` and clamps it to 32 before reading and converting.
    ///
    /// # Errors
    ///
    /// Returns [`LegacyExAnimationIoError`] for a non-512/three-byte table, slot bounds, unmappable
    /// pointers, truncated count/records, arithmetic overflow, or noncanonical conversion.
    pub fn load_legacy_exanimation(
        &self,
        slot: usize,
        layout: LegacyExAnimationRomLayout,
    ) -> Result<LoadedLegacyExAnimationSlot, LegacyExAnimationIoError> {
        validate_layout(layout)?;
        let pointer_offset = layout.pointers.pointer_offset(slot)?;
        let pointer_bytes = self.rom.read(pointer_offset, 3)?;
        if pointer_bytes[2] == 0 {
            return Ok(LoadedLegacyExAnimationSlot::Empty);
        }
        let pointer = SnesPointer24::decode(pointer_bytes)
            .expect("an exact three-byte ROM slice is a valid 24-bit pointer");
        let payload_offset = pointer.to_pc(layout.mapper)?;
        let raw_count = self.rom.read(payload_offset, 1)?[0];
        let record_count = usize::from(raw_count & 0x3f).min(LEGACY_EXANIMATION_MAX_RECORDS);
        let records_len = record_count
            .checked_mul(LEGACY_EXANIMATION_RECORD_LEN)
            .ok_or(LegacyExAnimationIoError::PayloadOffsetOverflow(
                payload_offset,
            ))?;
        let records_offset = payload_offset.checked_add(1).ok_or(
            LegacyExAnimationIoError::PayloadOffsetOverflow(payload_offset),
        )?;
        let records = convert_legacy_exanimation_records(
            self.rom.read(records_offset, records_len)?,
            record_count,
        )?;
        Ok(LoadedLegacyExAnimationSlot::Present {
            payload_offset,
            raw_count,
            record_count,
            records,
        })
    }

    /// Loads all 512 legacy slots in pointer-table order.
    ///
    /// # Errors
    ///
    /// Returns the first typed layout, pointer, bounds, or conversion failure without mutation.
    pub fn load_all_legacy_exanimations(
        &self,
        layout: LegacyExAnimationRomLayout,
    ) -> Result<Vec<LoadedLegacyExAnimationSlot>, LegacyExAnimationIoError> {
        validate_layout(layout)?;
        (0..LEGACY_EXANIMATION_LEVEL_COUNT)
            .map(|slot| self.load_legacy_exanimation(slot, layout))
            .collect()
    }

    /// Converts all legacy slots and publishes the installed current table as one transaction.
    ///
    /// This reproduces the migration ordering rather than offering a lossy compatibility shim:
    /// all 512 old pointers and payloads are authenticated first, the current destination table
    /// must be entirely empty, canonical compact payloads are allocated, both legacy tables and
    /// the recovered old payload extents are erased, every new pointer is written, and the SNES
    /// checksum is repaired in one undoable commit. Trailing inactive legacy records are omitted,
    /// matching the current serializer; an all-inactive slot remains an empty current pointer.
    ///
    /// Lunar Magic's old-payload erase starts at the legacy count byte and spans exactly
    /// `record_count * $23` bytes. Although that leaves the final source-record byte untouched,
    /// retaining that recovered extent is necessary for behavioral parity.
    ///
    /// # Errors
    ///
    /// Returns before mutation for malformed layouts, a nonempty destination, truncated or
    /// overlapping legacy storage, conversion/serialization failure, allocation failure, or an
    /// invalid checksum boundary.
    pub fn migrate_legacy_exanimations(
        &mut self,
        layout: &LegacyExAnimationMigrationLayout,
        double_size_modes: &[bool],
        options: &ExAnimationSaveOptions,
        checksum_field: usize,
    ) -> Result<LegacyExAnimationMigrationResult, LegacyExAnimationIoError> {
        let current = layout
            .current
            .resolve(&self.rom)
            .map_err(ExAnimationIoError::PointerLocator)?;
        validate_migration_layout(self, layout, current)?;
        if double_size_modes.len() != 256 {
            return Err(LegacyExAnimationIoError::ExAnimation(
                ExAnimationIoError::WrongSizeModeCount(double_size_modes.len()),
            ));
        }
        if !mapper_supports_image_len(current.payload.mapper, self.rom.logical_len()) {
            return Err(LegacyExAnimationIoError::Save(
                PayloadSaveError::MapperCannotAddressImage {
                    mapper: current.payload.mapper,
                    image_len: self.rom.logical_len(),
                },
            ));
        }
        options
            .allocation
            .validate(self.rom.logical_len())
            .map_err(PayloadSaveError::Allocation)?;
        let loaded = self.load_all_legacy_exanimations(layout.legacy)?;

        let legacy_table = checked_range(
            layout.legacy.pointers.offset,
            LEGACY_EXANIMATION_LEVEL_COUNT * 3,
        )?;
        let mut erase_ranges = vec![legacy_table, layout.legacy_auxiliary.clone()];
        for slot in &loaded {
            if let LoadedLegacyExAnimationSlot::Present {
                payload_offset,
                record_count,
                ..
            } = slot
            {
                let len = record_count
                    .checked_mul(LEGACY_EXANIMATION_RECORD_LEN)
                    .ok_or(LegacyExAnimationIoError::PayloadOffsetOverflow(
                        *payload_offset,
                    ))?;
                if len != 0 {
                    erase_ranges.push(checked_range(*payload_offset, len)?);
                }
            }
        }
        erase_ranges.sort_by_key(|range| range.start);
        for pair in erase_ranges.windows(2) {
            if pair[1].start < pair[0].end {
                return Err(LegacyExAnimationIoError::MigrationEraseRangeOverlap {
                    first: pair[0].start,
                    second: pair[1].start,
                });
            }
        }
        for range in &erase_ranges {
            self.rom.read(range.start, range.end - range.start)?;
        }

        let current_table = checked_range(
            current.payload.pointers.offset,
            LEGACY_EXANIMATION_LEVEL_COUNT * 3,
        )?;
        let checksum_range = checked_range(checksum_field, 4)?;
        self.rom.read(checksum_range.start, 4)?;
        for range in &erase_ranges {
            if overlaps(range, &current_table) {
                return Err(LegacyExAnimationIoError::MigrationStorageOverlap {
                    first: range.start,
                    second: current_table.start,
                });
            }
            if overlaps(range, &checksum_range) {
                return Err(LegacyExAnimationIoError::MigrationStorageOverlap {
                    first: range.start,
                    second: checksum_range.start,
                });
            }
        }
        if overlaps(&current_table, &checksum_range) {
            return Err(LegacyExAnimationIoError::MigrationStorageOverlap {
                first: current_table.start,
                second: checksum_range.start,
            });
        }

        let mut migration_options = options.clone();
        migration_options.previous_block = None;
        for range in erase_ranges.iter().chain([&current_table, &checksum_range]) {
            if !migration_options
                .allocation
                .protected
                .iter()
                .any(|protected| protected.0.start <= range.start && range.end <= protected.0.end)
            {
                migration_options
                    .allocation
                    .protected
                    .push(ProtectedRange(range.clone()));
            }
        }

        let mut migrated_slots = Vec::new();
        let mut requests = Vec::new();
        for (slot, loaded) in loaded.into_iter().enumerate() {
            let LoadedLegacyExAnimationSlot::Present { mut records, .. } = loaded else {
                continue;
            };
            while records.last().is_some_and(|record| record.kind() == 0) {
                records.pop();
            }
            if records.is_empty() {
                continue;
            }
            let animation = CompactExAnimation {
                setting: 0,
                header_value: 0xffff,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records,
            };
            requests.push(exanimation_save_request(
                slot,
                &animation,
                current.payload,
                double_size_modes,
                &migration_options,
            )?);
            migrated_slots.push(slot);
        }
        let writes = erase_ranges
            .into_iter()
            .map(|range| RomWrite {
                offset: range.start,
                bytes: vec![options.erase_fill; range.end - range.start],
            })
            .collect::<Vec<_>>();
        let allocations = self.save_tagged_payloads_with_checksum_and_writes(
            "migrate legacy ExAnimation runtime",
            &requests,
            &writes,
            checksum_field,
        )?;
        Ok(LegacyExAnimationMigrationResult {
            migrated_slots,
            allocations,
        })
    }
}

fn checked_range(offset: usize, len: usize) -> Result<Range<usize>, LegacyExAnimationIoError> {
    let end = offset
        .checked_add(len)
        .ok_or(LegacyExAnimationIoError::PayloadOffsetOverflow(offset))?;
    Ok(offset..end)
}

fn overlaps(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn validate_migration_layout(
    project: &Project,
    layout: &LegacyExAnimationMigrationLayout,
    current: InstalledExAnimationRomLayout,
) -> Result<(), LegacyExAnimationIoError> {
    validate_layout(layout.legacy)?;
    if layout.legacy.mapper != current.payload.mapper {
        return Err(LegacyExAnimationIoError::MapperMismatch {
            legacy: layout.legacy.mapper,
            current: current.payload.mapper,
        });
    }
    if current.pointer_presence_mask == 0 || current.pointer_presence_mask & !0x00ff_ffff != 0 {
        return Err(LegacyExAnimationIoError::InvalidCurrentPointerPresenceMask(
            current.pointer_presence_mask,
        ));
    }
    if current.payload.pointers.entries != LEGACY_EXANIMATION_LEVEL_COUNT {
        return Err(LegacyExAnimationIoError::WrongCurrentPointerEntryCount(
            current.payload.pointers.entries,
        ));
    }
    if current.payload.pointers.stride != 3 {
        return Err(LegacyExAnimationIoError::WrongCurrentPointerStride(
            current.payload.pointers.stride,
        ));
    }
    if current.payload.maximum_records < LEGACY_EXANIMATION_MAX_RECORDS {
        return Err(LegacyExAnimationIoError::CurrentRecordLimit(
            current.payload.maximum_records,
        ));
    }
    if layout.legacy_auxiliary.len() != 0x140 {
        return Err(LegacyExAnimationIoError::WrongLegacyAuxiliaryLength(
            layout.legacy_auxiliary.len(),
        ));
    }
    for slot in 0..LEGACY_EXANIMATION_LEVEL_COUNT {
        let offset = current.payload.pointers.pointer_offset(slot)?;
        let bytes = project.rom.read(offset, 3)?;
        let raw = u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16;
        if raw & current.pointer_presence_mask != 0 {
            return Err(LegacyExAnimationIoError::CurrentPointerTableNotEmpty {
                slot,
                pointer: raw,
            });
        }
    }
    Ok(())
}

fn validate_layout(layout: LegacyExAnimationRomLayout) -> Result<(), LegacyExAnimationIoError> {
    if layout.pointers.entries != LEGACY_EXANIMATION_LEVEL_COUNT {
        return Err(LegacyExAnimationIoError::WrongPointerEntryCount(
            layout.pointers.entries,
        ));
    }
    if layout.pointers.stride != 3 {
        return Err(LegacyExAnimationIoError::WrongPointerStride(
            layout.pointers.stride,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExAnimationRomLayout, ExAnimationSaveOptions, InstalledExAnimationRomLayout};
    use lm_rats::AllocationPolicy;
    use lm_rom::{CopierHeader, RomImage, compute_snes_checksum, pc_to_snes};

    fn layout(mapper: Mapper) -> LegacyExAnimationRomLayout {
        LegacyExAnimationRomLayout {
            mapper,
            pointers: LevelPointerTable {
                offset: 0x100,
                entries: 0x200,
                stride: 3,
            },
        }
    }

    fn write_pointer(rom: &mut RomImage, offset: usize, mapper: Mapper, target: usize) {
        let pointer = pc_to_snes(mapper, target).unwrap().to_le_bytes();
        rom.write(offset, &pointer[..3]).unwrap();
    }

    fn legacy_record(control: u8, destination: u16, word: u16) -> [u8; 0x23] {
        let mut record = [0; 0x23];
        record[0] = control;
        record[1..3].copy_from_slice(&destination.to_le_bytes());
        for frame in 0..16 {
            record[3 + frame * 2..5 + frame * 2].copy_from_slice(&word.to_le_bytes());
        }
        record
    }

    fn migration_layout(mapper: Mapper) -> LegacyExAnimationMigrationLayout {
        LegacyExAnimationMigrationLayout {
            legacy: layout(mapper),
            current: InstalledExAnimationRomLayout {
                payload: ExAnimationRomLayout {
                    mapper,
                    pointers: LevelPointerTable {
                        offset: 0x800,
                        entries: 0x200,
                        stride: 3,
                    },
                    maximum_records: 0x40,
                    maximum_encoded_len: 0x4000,
                },
                pointer_presence_mask: 0x00ff_0000,
                pointer_locator: None,
            },
            legacy_auxiliary: 0xe00..0xf40,
        }
    }

    fn migration_options() -> ExAnimationSaveOptions {
        ExAnimationSaveOptions {
            allocation: AllocationPolicy::lorom(0x1_0000..0x1_8000),
            previous_block: None,
            reuse_identical: false,
            erase_fill: 0xff,
        }
    }

    fn empty_migration_rom() -> RomImage {
        let mut bytes = vec![0xff; 0x2_0000];
        bytes[0x100..0x700].fill(0);
        bytes[0x800..0xe00].fill(0);
        RomImage::from_bytes(bytes).unwrap()
    }

    #[test]
    fn loads_count_byte_and_records_across_every_supported_mapper() {
        for mapper in [Mapper::LoRom, Mapper::ExLoRom, Mapper::Sa1] {
            let mut rom = RomImage::from_bytes(vec![0xff; 0x20_0000]).unwrap();
            let payload = 0x20_000;
            write_pointer(&mut rom, 0x100 + 7 * 3, mapper, payload);
            rom.write(payload, &[2]).unwrap();
            rom.write(payload + 1, &legacy_record(0x21, 0x1234, 0x4567))
                .unwrap();
            rom.write(payload + 1 + 0x23, &[0; 0x23]).unwrap();
            let project = Project::new(rom);

            let LoadedLegacyExAnimationSlot::Present {
                payload_offset,
                raw_count,
                record_count,
                records,
            } = project.load_legacy_exanimation(7, layout(mapper)).unwrap()
            else {
                panic!("expected present legacy slot");
            };
            assert_eq!(payload_offset, payload);
            assert_eq!(raw_count, 2);
            assert_eq!(record_count, 2);
            assert_eq!(records[0].kind(), 1);
            assert_eq!(records[0].destination(), 0x1234);
            assert_eq!(records[1], ExAnimationRecord::inactive());
        }
    }

    #[test]
    fn bank_zero_is_empty_even_when_the_low_word_is_nonzero() {
        let mut rom = RomImage::from_bytes(vec![0xff; 0x20_000]).unwrap();
        rom.write(0x100, &[0x34, 0x12, 0]).unwrap();
        let project = Project::new(rom);
        assert_eq!(
            project
                .load_legacy_exanimation(0, layout(Mapper::LoRom))
                .unwrap(),
            LoadedLegacyExAnimationSlot::Empty
        );
    }

    #[test]
    fn count_is_masked_to_six_bits_and_clamped_to_thirty_two() {
        let mut rom = RomImage::from_bytes(vec![0xff; 0x20_000]).unwrap();
        let payload = 0x4000;
        write_pointer(&mut rom, 0x100, Mapper::LoRom, payload);
        rom.write(payload, &[0xff]).unwrap();
        rom.write(payload + 1, &vec![0; 32 * 0x23]).unwrap();
        let project = Project::new(rom);
        let LoadedLegacyExAnimationSlot::Present {
            raw_count,
            record_count,
            records,
            ..
        } = project
            .load_legacy_exanimation(0, layout(Mapper::LoRom))
            .unwrap()
        else {
            panic!("expected present legacy slot");
        };
        assert_eq!(raw_count, 0xff);
        assert_eq!(record_count, 32);
        assert_eq!(records.len(), 32);
    }

    #[test]
    fn complete_loader_returns_all_five_hundred_twelve_slots_in_order() {
        let mut bytes = vec![0xff; 0x20_000];
        bytes[0x100..0x700].fill(0);
        let mut rom = RomImage::from_bytes(bytes).unwrap();
        for (slot, payload, word) in [(0, 0x4000, 0x1111), (511, 0x5000, 0x2222)] {
            write_pointer(&mut rom, 0x100 + slot * 3, Mapper::LoRom, payload);
            rom.write(payload, &[1]).unwrap();
            rom.write(payload + 1, &legacy_record(0x21, 0x1234, word))
                .unwrap();
        }
        let project = Project::new(rom);
        let loaded = project
            .load_all_legacy_exanimations(layout(Mapper::LoRom))
            .unwrap();
        assert_eq!(loaded.len(), 512);
        assert!(matches!(
            loaded[0],
            LoadedLegacyExAnimationSlot::Present { .. }
        ));
        assert!(
            loaded[1..511]
                .iter()
                .all(|slot| matches!(slot, LoadedLegacyExAnimationSlot::Empty))
        );
        assert!(matches!(
            loaded[511],
            LoadedLegacyExAnimationSlot::Present { .. }
        ));
    }

    #[test]
    fn table_shape_slot_pointer_and_payload_failures_are_typed() {
        let project = Project::new(RomImage::from_bytes(vec![0xff; 0x20_000]).unwrap());
        let mut wrong_entries = layout(Mapper::LoRom);
        wrong_entries.pointers.entries = 511;
        assert!(matches!(
            project.load_legacy_exanimation(0, wrong_entries),
            Err(LegacyExAnimationIoError::WrongPointerEntryCount(511))
        ));
        let mut wrong_stride = layout(Mapper::LoRom);
        wrong_stride.pointers.stride = 4;
        assert!(matches!(
            project.load_legacy_exanimation(0, wrong_stride),
            Err(LegacyExAnimationIoError::WrongPointerStride(4))
        ));
        assert!(matches!(
            project.load_legacy_exanimation(512, layout(Mapper::LoRom)),
            Err(LegacyExAnimationIoError::Layout(_))
        ));

        let mut truncated = vec![0xff; 0x20_000];
        let end = truncated.len() - 1;
        let pointer = pc_to_snes(Mapper::LoRom, end).unwrap().to_le_bytes();
        truncated[0x100..0x103].copy_from_slice(&pointer[..3]);
        truncated[end] = 1;
        let project = Project::new(RomImage::from_bytes(truncated).unwrap());
        assert!(matches!(
            project.load_legacy_exanimation(0, layout(Mapper::LoRom)),
            Err(LegacyExAnimationIoError::Rom(_))
        ));
    }

    #[test]
    fn migration_converts_erases_reopens_repairs_checksum_and_undoes_once() {
        let mut rom = empty_migration_rom();
        let payload = 0x4000;
        write_pointer(&mut rom, 0x100 + 7 * 3, Mapper::LoRom, payload);
        rom.write(payload, &[1]).unwrap();
        rom.write(payload + 1, &legacy_record(0x21, 0x4321, 0x1234))
            .unwrap();
        let inactive_payload = 0x5000;
        write_pointer(&mut rom, 0x100 + 8 * 3, Mapper::LoRom, inactive_payload);
        rom.write(inactive_payload, &[1]).unwrap();
        rom.write(inactive_payload + 1, &[0; 0x23]).unwrap();
        rom.write(0xe00, &[0x55; 0x140]).unwrap();
        let before = rom.logical_bytes().to_vec();
        let mut project = Project::new(rom);
        let mut size_modes = [false; 256];
        size_modes[1] = true;
        size_modes[2] = true;
        size_modes[3] = true;
        let migration = migration_layout(Mapper::LoRom);

        let result = project
            .migrate_legacy_exanimations(&migration, &size_modes, &migration_options(), 0x7fdc)
            .unwrap();
        assert_eq!(result.migrated_slots, [7]);
        assert_eq!(result.allocations.len(), 1);
        assert!(
            project
                .rom
                .read(0x100, 0x600)
                .unwrap()
                .iter()
                .all(|byte| *byte == 0xff)
        );
        assert!(
            project
                .rom
                .read(0xe00, 0x140)
                .unwrap()
                .iter()
                .all(|byte| *byte == 0xff)
        );
        assert!(
            project
                .rom
                .read(payload, 0x23)
                .unwrap()
                .iter()
                .all(|byte| *byte == 0xff)
        );
        assert_eq!(project.rom.read(payload + 0x23, 1).unwrap(), &[0x12]);
        assert!(
            project
                .rom
                .read(inactive_payload, 0x23)
                .unwrap()
                .iter()
                .all(|byte| *byte == 0xff)
        );
        assert_eq!(project.rom.read(0x800 + 8 * 3, 3).unwrap(), &[0, 0, 0]);

        let reopened = project
            .load_exanimation(7, migration.current.payload, &size_modes)
            .unwrap();
        assert_eq!(reopened.setting, 0);
        assert_eq!(reopened.header_value, 0xffff);
        assert_eq!(reopened.trigger_mask, 0);
        assert_eq!(reopened.records.len(), 1);
        assert_eq!(reopened.records[0].kind(), 0x01);
        assert_eq!(reopened.records[0].destination(), 0x4321);
        assert_eq!(reopened.records[0].frame_bytes(false), &[0x34, 0x12]);
        assert_eq!(
            project.rom.read(0x7fdc, 4).unwrap(),
            compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc)
                .unwrap()
                .encoded()
        );

        assert!(project.undo().unwrap());
        assert_eq!(project.rom.logical_bytes(), before);
        assert!(!project.undo().unwrap());
        assert!(project.redo().unwrap());
        assert_eq!(
            project
                .load_exanimation(7, migration.current.payload, &size_modes)
                .unwrap(),
            reopened
        );
    }

    #[test]
    fn migration_rejects_nonempty_destination_and_overlapping_sources_atomically() {
        let mut rom = empty_migration_rom();
        rom.write(0x800 + 9 * 3, &[0, 0, 0x80]).unwrap();
        let before = rom.logical_bytes().to_vec();
        let mut project = Project::new(rom);
        let modes = [false; 256];
        assert!(matches!(
            project.migrate_legacy_exanimations(
                &migration_layout(Mapper::LoRom),
                &modes,
                &migration_options(),
                0x7fdc,
            ),
            Err(LegacyExAnimationIoError::CurrentPointerTableNotEmpty { slot: 9, .. })
        ));
        assert_eq!(project.rom.logical_bytes(), before);
        assert!(!project.undo().unwrap());

        let mut rom = empty_migration_rom();
        write_pointer(&mut rom, 0x100, Mapper::LoRom, 0xe10);
        rom.write(0xe10, &[1]).unwrap();
        rom.write(0xe11, &legacy_record(0x21, 0x1234, 0x5678))
            .unwrap();
        let before = rom.logical_bytes().to_vec();
        let mut project = Project::new(rom);
        assert!(matches!(
            project.migrate_legacy_exanimations(
                &migration_layout(Mapper::LoRom),
                &modes,
                &migration_options(),
                0x7fdc,
            ),
            Err(LegacyExAnimationIoError::MigrationEraseRangeOverlap { .. })
        ));
        assert_eq!(project.rom.logical_bytes(), before);
        assert!(!project.undo().unwrap());
    }

    #[test]
    fn migration_is_mapper_and_copier_header_invariant() {
        for mapper in [Mapper::LoRom, Mapper::ExLoRom, Mapper::Sa1] {
            for header in [CopierHeader::Absent, CopierHeader::Present] {
                let mut rom = empty_migration_rom();
                rom.set_copier_header(header, 0xa5);
                let header_before = rom.copier_header_bytes().map(<[u8]>::to_vec);
                let payload = 0x1_c000;
                write_pointer(&mut rom, 0x100, mapper, payload);
                rom.write(payload, &[1]).unwrap();
                rom.write(payload + 1, &legacy_record(0x21, 0x2222, 0x3456))
                    .unwrap();
                let mut project = Project::new(rom);
                let layout = migration_layout(mapper);

                let result = project
                    .migrate_legacy_exanimations(
                        &layout,
                        &[false; 256],
                        &migration_options(),
                        0x7fdc,
                    )
                    .unwrap();
                assert_eq!(result.migrated_slots, [0]);
                assert_eq!(project.rom.copier_header_bytes(), header_before.as_deref());
                assert_eq!(
                    project
                        .load_exanimation(0, layout.current.payload, &[false; 256])
                        .unwrap()
                        .records[0]
                        .destination(),
                    0x2222
                );
            }
        }
    }

    #[test]
    fn late_allocation_failure_leaves_every_legacy_byte_and_history_unchanged() {
        let mut rom = empty_migration_rom();
        write_pointer(&mut rom, 0x100, Mapper::LoRom, 0x4000);
        rom.write(0x4000, &[1]).unwrap();
        rom.write(0x4001, &legacy_record(0x21, 0x2222, 0x3456))
            .unwrap();
        let before = rom.logical_bytes().to_vec();
        let mut project = Project::new(rom);
        let mut options = migration_options();
        options.allocation.search = 0x1_0000..0x1_0010;

        assert!(matches!(
            project.migrate_legacy_exanimations(
                &migration_layout(Mapper::LoRom),
                &[false; 256],
                &options,
                0x7fdc,
            ),
            Err(LegacyExAnimationIoError::Save(
                PayloadSaveError::Allocation(_)
            ))
        ));
        assert_eq!(project.rom.logical_bytes(), before);
        assert!(!project.undo().unwrap());
    }

    #[test]
    fn empty_legacy_tables_still_require_complete_modes_and_allocation_authority() {
        let rom = empty_migration_rom();
        let before = rom.logical_bytes().to_vec();
        let mut project = Project::new(rom);
        assert!(matches!(
            project.migrate_legacy_exanimations(
                &migration_layout(Mapper::LoRom),
                &[false; 255],
                &migration_options(),
                0x7fdc,
            ),
            Err(LegacyExAnimationIoError::ExAnimation(
                ExAnimationIoError::WrongSizeModeCount(255)
            ))
        ));
        let mut options = migration_options();
        options.allocation.search = 0x1_0000..0x3_0000;
        assert!(matches!(
            project.migrate_legacy_exanimations(
                &migration_layout(Mapper::LoRom),
                &[false; 256],
                &options,
                0x7fdc,
            ),
            Err(LegacyExAnimationIoError::Save(
                PayloadSaveError::Allocation(_)
            ))
        ));
        assert_eq!(project.rom.logical_bytes(), before);
        assert!(!project.undo().unwrap());
    }
}
