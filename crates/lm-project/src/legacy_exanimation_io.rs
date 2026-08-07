use crate::{LevelLoadError, LevelPointerTable, Project};
use lm_graphics::{
    ExAnimationRecord, LEGACY_EXANIMATION_MAX_RECORDS, LEGACY_EXANIMATION_RECORD_LEN,
    LegacyExAnimationError, convert_legacy_exanimation_records,
};
use lm_rom::{Mapper, RomError, SnesPointer24};
use std::fmt;

pub const LEGACY_EXANIMATION_LEVEL_COUNT: usize = 0x200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyExAnimationRomLayout {
    pub mapper: Mapper,
    pub pointers: LevelPointerTable,
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
    use lm_rom::{RomImage, pc_to_snes};

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
}
