//! Detection and owned updates for Lunar Magic's separate-midway runtime.

use crate::{Project, RomWrite, TransactionError};
use lm_level::{SeparateMidwayEntranceTable, SeparateMidwayEntranceTableError};
use lm_rats::{RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, compute_snes_checksum, snes_to_pc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SeparateMidwayPatchLocator {
    pub mapper: Mapper,
    pub hook_offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSeparateMidwayTable {
    pub table: SeparateMidwayEntranceTable,
    pub helper_block: RatsBlock,
    pub table_block: RatsBlock,
}

#[derive(Debug)]
pub enum SeparateMidwayPatchError {
    HookSignature,
    HelperOwnership,
    HelperSignature,
    TablePointerMismatch,
    TableOwnership,
    TableLength(usize),
    Rom(RomError),
    Table(SeparateMidwayEntranceTableError),
    Transaction(TransactionError),
}

impl std::fmt::Display for SeparateMidwayPatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "separate-midway patch failed: {self:?}")
    }
}

impl std::error::Error for SeparateMidwayPatchError {}

impl From<RomError> for SeparateMidwayPatchError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<SeparateMidwayEntranceTableError> for SeparateMidwayPatchError {
    fn from(value: SeparateMidwayEntranceTableError) -> Self {
        Self::Table(value)
    }
}

impl From<TransactionError> for SeparateMidwayPatchError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads a structurally validated, RATS-owned separate-midway table.
    ///
    /// # Errors
    ///
    /// Rejects absent or altered hooks, unowned helper/table payloads, wrong table pointers,
    /// malformed helper markers, and any table length other than `$800`.
    pub fn load_separate_midway_table(
        &self,
        locator: SeparateMidwayPatchLocator,
    ) -> Result<LoadedSeparateMidwayTable, SeparateMidwayPatchError> {
        let hook = self.rom.read(locator.hook_offset, 4)?;
        if hook[0] != 0x22 {
            return Err(SeparateMidwayPatchError::HookSignature);
        }
        let helper = pointer_pc(&hook[1..4], locator.mapper)?;
        let helper_block = owned_block(self.rom.logical_bytes(), helper)
            .ok_or(SeparateMidwayPatchError::HelperOwnership)?;
        if helper_block.payload.len() != 0xd0 {
            return Err(SeparateMidwayPatchError::HelperSignature);
        }
        let bytes = self.rom.read(helper, 0xd0)?;
        if bytes[0] != 0x4a
            || bytes[9] != 0xbf
            || bytes[0x26] != 0xbf
            || bytes[0x47] != 0xbf
            || bytes[0x56] != 0xbf
            || &bytes[0xcc..0xd0] != b"LM\x10\x01"
        {
            return Err(SeparateMidwayPatchError::HelperSignature);
        }
        let pointers = [
            pointer_pc(&bytes[0x0a..0x0d], locator.mapper)?,
            pointer_pc(&bytes[0x27..0x2a], locator.mapper)?,
            pointer_pc(&bytes[0x57..0x5a], locator.mapper)?,
            pointer_pc(&bytes[0x48..0x4b], locator.mapper)?,
        ];
        let base = pointers[0];
        if pointers != [base, base + 0x200, base + 0x400, base + 0x600] {
            return Err(SeparateMidwayPatchError::TablePointerMismatch);
        }
        let table_block = owned_block(self.rom.logical_bytes(), base)
            .ok_or(SeparateMidwayPatchError::TableOwnership)?;
        if table_block.payload.len() != SeparateMidwayEntranceTable::ENCODED_LEN {
            return Err(SeparateMidwayPatchError::TableLength(
                table_block.payload.len(),
            ));
        }
        Ok(LoadedSeparateMidwayTable {
            table: SeparateMidwayEntranceTable::decode(
                self.rom
                    .read(base, SeparateMidwayEntranceTable::ENCODED_LEN)?,
            )?,
            helper_block,
            table_block,
        })
    }

    /// Replaces an installed table in place and repairs the checksum atomically.
    ///
    /// # Errors
    ///
    /// Revalidates exact runtime/table ownership and rejects malformed table shapes, ROM bounds,
    /// checksum failures, or transaction failures without mutation.
    pub fn save_separate_midway_table(
        &mut self,
        table: &SeparateMidwayEntranceTable,
        locator: SeparateMidwayPatchLocator,
        checksum_field: usize,
    ) -> Result<bool, SeparateMidwayPatchError> {
        let loaded = self.load_separate_midway_table(locator)?;
        if &loaded.table == table {
            return Ok(false);
        }
        let table_write = RomWrite {
            offset: loaded.table_block.payload.start,
            bytes: table.encode()?,
        };
        let mut staged = self.rom.clone();
        staged.write(table_write.offset, &table_write.bytes)?;
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        Ok(self.apply_writes(
            "save separate midway entrances",
            &[
                table_write,
                RomWrite {
                    offset: checksum_field,
                    bytes: checksum.encoded().to_vec(),
                },
            ],
        )?)
    }
}

fn pointer_pc(bytes: &[u8], mapper: Mapper) -> Result<usize, RomError> {
    let address = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
    // LoROM's installed code commonly publishes the low-bank mirror. ExLoROM uses bit 23 to
    // distinguish its two 4 MiB halves, while SA-1 uses the full canonical bank range; stripping
    // that bit for either mapper silently redirects an otherwise valid owned runtime pointer.
    let address = if mapper == Mapper::LoRom {
        address & 0x7f_ffff
    } else {
        address
    };
    snes_to_pc(mapper, address)
}

fn owned_block(bytes: &[u8], payload: usize) -> Option<RatsBlock> {
    let header = payload.checked_sub(lm_rats::HEADER_LEN)?;
    let block = parse_at(bytes, header).ok()?;
    (block.payload.start == payload).then_some(block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{RomImage, pc_to_snes};

    #[test]
    fn owned_runtime_loads_updates_and_undoes() {
        let helper = 0x10008;
        let table = 0x11008;
        let hook = 0x200;
        let mut bytes = vec![0xff; 0x1_00000];
        write_rats_header(&mut bytes, helper - 8, 0xd0);
        write_rats_header(
            &mut bytes,
            table - 8,
            SeparateMidwayEntranceTable::ENCODED_LEN,
        );
        bytes[helper] = 0x4a;
        bytes[helper + 9] = 0xbf;
        bytes[helper + 0x26] = 0xbf;
        bytes[helper + 0x47] = 0xbf;
        bytes[helper + 0x56] = 0xbf;
        bytes[helper + 0xcc..helper + 0xd0].copy_from_slice(b"LM\x10\x01");
        for (pointer_offset, addend) in [(0x0a, 0), (0x27, 0x200), (0x57, 0x400), (0x48, 0x600)] {
            let address = pc_to_snes(Mapper::LoRom, table + addend).unwrap() & 0x7f_ffff;
            bytes[helper + pointer_offset..helper + pointer_offset + 3]
                .copy_from_slice(&address.to_le_bytes()[..3]);
        }
        let helper_address = pc_to_snes(Mapper::LoRom, helper).unwrap();
        bytes[hook] = 0x22;
        bytes[hook + 1..hook + 4].copy_from_slice(&helper_address.to_le_bytes()[..3]);
        bytes[table + 0x105] = 0xa5;
        let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        let mut project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let locator = SeparateMidwayPatchLocator {
            mapper: Mapper::LoRom,
            hook_offset: hook,
        };
        let mut loaded = project.load_separate_midway_table(locator).unwrap();
        assert_eq!(loaded.table.entries[0x105].flags, 0xa5);
        loaded.table.entries[0x105].position ^= 1;
        assert!(
            project
                .save_separate_midway_table(&loaded.table, locator, 0x7fdc)
                .unwrap()
        );
        assert_eq!(
            project.load_separate_midway_table(locator).unwrap().table,
            loaded.table
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), bytes);
    }

    fn write_rats_header(bytes: &mut [u8], offset: usize, payload_len: usize) {
        let length = u16::try_from(payload_len - 1).unwrap();
        bytes[offset..offset + 4].copy_from_slice(b"STAR");
        bytes[offset + 4..offset + 6].copy_from_slice(&length.to_le_bytes());
        bytes[offset + 6..offset + 8].copy_from_slice(&(!length).to_le_bytes());
    }

    #[test]
    fn pointer_mapping_helper_matches_lorom() {
        let pc = 0x80718;
        let address = pc_to_snes(Mapper::LoRom, pc).unwrap() & 0x7f_ffff;
        assert_eq!(
            pointer_pc(&address.to_le_bytes()[..3], Mapper::LoRom).unwrap(),
            pc
        );
    }

    #[test]
    fn pointer_mapping_helper_preserves_mapper_significant_high_bank_bit() {
        for (mapper, pc) in [
            (Mapper::ExLoRom, 0x10008),
            (Mapper::ExLoRom, 0x410008),
            (Mapper::Sa1, 0x210008),
            (Mapper::Sa1, 0x410008),
        ] {
            let address = pc_to_snes(mapper, pc).unwrap();
            assert_eq!(pointer_pc(&address.to_le_bytes()[..3], mapper).unwrap(), pc);
        }
    }
}
