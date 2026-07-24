use crate::LevelPointerTable;
use lm_rom::{Mapper, RomError, RomImage, snes_to_pc};
use std::fmt;

/// Resolves an allocator-dependent table through two embedded 24-bit SNES operands.
///
/// Lunar Magic runtime installers commonly patch a stable hook with a long target address, then
/// place the actual table operand at a fixed signed displacement within that allocated runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChainedSnesPointerLocator {
    pub mapper: Mapper,
    /// Logical ROM offset of the first three-byte SNES operand (immediately after its opcode).
    pub first_operand_offset: usize,
    /// Signed displacement from the first operand's resolved PC target to the final operand.
    pub final_operand_displacement: isize,
}

impl ChainedSnesPointerLocator {
    /// Resolves the final operand to a logical ROM offset.
    ///
    /// # Errors
    ///
    /// Returns a bounds, mapping, or signed-displacement error for malformed installed code.
    pub fn resolve(self, rom: &RomImage) -> Result<usize, PointerLocatorError> {
        let final_operand_offset = self.final_operand_offset(rom)?;
        Ok(snes_to_pc(
            self.mapper,
            read_u24(rom, final_operand_offset)?,
        )?)
    }

    /// Resolves the logical ROM location of the second embedded operand.
    ///
    /// # Errors
    ///
    /// Returns a bounds, mapping, or signed-displacement error for malformed installed code.
    pub fn final_operand_offset(self, rom: &RomImage) -> Result<usize, PointerLocatorError> {
        let first_target = snes_to_pc(self.mapper, read_u24(rom, self.first_operand_offset)?)?;
        first_target
            .checked_add_signed(self.final_operand_displacement)
            .ok_or(PointerLocatorError::DisplacementOverflow {
                target: first_target,
                displacement: self.final_operand_displacement,
            })
    }

    /// Resolves a concrete three-byte level-pointer table layout.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::resolve`].
    pub fn resolve_level_table(
        self,
        rom: &RomImage,
        entries: usize,
    ) -> Result<LevelPointerTable, PointerLocatorError> {
        Ok(LevelPointerTable {
            offset: self.resolve(rom)?,
            entries,
            stride: 3,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PointerLocatorError {
    Rom(RomError),
    DisplacementOverflow { target: usize, displacement: isize },
}

impl fmt::Display for PointerLocatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "installed pointer locator failed: {self:?}")
    }
}

impl std::error::Error for PointerLocatorError {}

impl From<RomError> for PointerLocatorError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

fn read_u24(rom: &RomImage, offset: usize) -> Result<u32, RomError> {
    let bytes = rom.read(offset, 3)?;
    Ok(u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::pc_to_snes;

    fn write_u24(rom: &mut RomImage, offset: usize, value: u32) {
        rom.write(
            offset,
            &[
                value.to_le_bytes()[0],
                value.to_le_bytes()[1],
                value.to_le_bytes()[2],
            ],
        )
        .unwrap();
    }

    #[test]
    fn follows_hook_target_to_allocator_dependent_table_operand() {
        let mut rom = RomImage::from_bytes(vec![0xff; 0x8000]).unwrap();
        write_u24(&mut rom, 0x101, pc_to_snes(Mapper::LoRom, 0x300).unwrap());
        write_u24(&mut rom, 0x2e0, pc_to_snes(Mapper::LoRom, 0x600).unwrap());
        let locator = ChainedSnesPointerLocator {
            mapper: Mapper::LoRom,
            first_operand_offset: 0x101,
            final_operand_displacement: -0x20,
        };
        assert_eq!(locator.resolve(&rom), Ok(0x600));
        assert_eq!(
            locator.resolve_level_table(&rom, 0x200).unwrap(),
            LevelPointerTable {
                offset: 0x600,
                entries: 0x200,
                stride: 3,
            }
        );
    }

    #[test]
    fn malformed_operands_and_signed_offsets_are_rejected() {
        let rom = RomImage::from_bytes(vec![0xff; 0x8000]).unwrap();
        assert!(matches!(
            ChainedSnesPointerLocator {
                mapper: Mapper::LoRom,
                first_operand_offset: 0x8000,
                final_operand_displacement: 0,
            }
            .resolve(&rom),
            Err(PointerLocatorError::Rom(RomError::RangeOutOfBounds { .. }))
        ));
        assert!(matches!(
            ChainedSnesPointerLocator {
                mapper: Mapper::LoRom,
                first_operand_offset: 0,
                final_operand_displacement: isize::MIN,
            }
            .resolve(&rom),
            Err(PointerLocatorError::Rom(_) | PointerLocatorError::DisplacementOverflow { .. })
        ));
    }
}
