use crate::{
    ChainedSnesPointerLocator, InstalledLayout, InstalledLayoutError, PointerLocatorError, Project,
    RomWrite, TransactionError,
};
use lm_rom::{RomError, compute_snes_checksum};
use std::fmt;

pub const OVERWORLD_ANIMATION_MAP_COUNT: usize = 7;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldAnimationOptionsRomLayout {
    pub feature_installation: InstalledLayout<ChainedSnesPointerLocator>,
    pub lightning_disable_mask_offset: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadedOverworldAnimationOptions {
    pub feature_bytes: [u8; OVERWORLD_ANIMATION_MAP_COUNT],
    pub lightning_disable_mask: u8,
    pub runtime_installed: bool,
}

#[derive(Debug)]
pub enum OverworldAnimationOptionsIoError {
    RuntimeInstallationRequired,
    ChecksumOverlap,
    OffsetOverflow,
    Installation(InstalledLayoutError),
    PointerLocator(PointerLocatorError),
    Rom(RomError),
    Transaction(TransactionError),
}

impl fmt::Display for OverworldAnimationOptionsIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "overworld animation option I/O failed: {self:?}")
    }
}

impl std::error::Error for OverworldAnimationOptionsIoError {}

impl From<InstalledLayoutError> for OverworldAnimationOptionsIoError {
    fn from(value: InstalledLayoutError) -> Self {
        Self::Installation(value)
    }
}

impl From<PointerLocatorError> for OverworldAnimationOptionsIoError {
    fn from(value: PointerLocatorError) -> Self {
        Self::PointerLocator(value)
    }
}

impl From<RomError> for OverworldAnimationOptionsIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for OverworldAnimationOptionsIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads the seven inverted feature bytes and the independent high-bit-first lightning mask.
    ///
    /// An absent installed runtime means Lunar Magic's exact all-enabled feature default rather
    /// than an error. The lightning mask is always read from its descriptor-selected operand.
    pub fn load_installed_overworld_animation_options(
        &self,
        layout: OverworldAnimationOptionsRomLayout,
    ) -> Result<LoadedOverworldAnimationOptions, OverworldAnimationOptionsIoError> {
        let Some(feature_table_locator) = layout.feature_installation.resolve(&self.rom)? else {
            return Ok(LoadedOverworldAnimationOptions {
                feature_bytes: [0; OVERWORLD_ANIMATION_MAP_COUNT],
                lightning_disable_mask: self.rom.read(layout.lightning_disable_mask_offset, 1)?[0],
                runtime_installed: false,
            });
        };
        let table = feature_table_locator.resolve(&self.rom)?;
        let feature_bytes = self
            .rom
            .read(table, OVERWORLD_ANIMATION_MAP_COUNT)?
            .try_into()
            .expect("fixed seven-byte overworld option slice");
        let lightning_disable_mask = self.rom.read(layout.lightning_disable_mask_offset, 1)?[0];
        Ok(LoadedOverworldAnimationOptions {
            feature_bytes,
            lightning_disable_mask,
            runtime_installed: true,
        })
    }

    /// Saves both original option sources and the SNES checksum as one undoable transaction.
    ///
    /// Lunar Magic must first install its overworld runtime before any nonzero feature byte can be
    /// represented. Lightning remains independently writable in pristine ROMs.
    pub fn save_installed_overworld_animation_options(
        &mut self,
        feature_bytes: [u8; OVERWORLD_ANIMATION_MAP_COUNT],
        lightning_disable_mask: u8,
        layout: OverworldAnimationOptionsRomLayout,
        checksum_field: usize,
    ) -> Result<bool, OverworldAnimationOptionsIoError> {
        let installed = layout.feature_installation.resolve(&self.rom)?;
        if installed.is_none() && feature_bytes != [0; OVERWORLD_ANIMATION_MAP_COUNT] {
            return Err(OverworldAnimationOptionsIoError::RuntimeInstallationRequired);
        }
        let mut writes = vec![RomWrite {
            offset: layout.lightning_disable_mask_offset,
            bytes: vec![lightning_disable_mask],
        }];
        if let Some(feature_table_locator) = installed {
            writes.push(RomWrite {
                offset: feature_table_locator.resolve(&self.rom)?,
                bytes: feature_bytes.to_vec(),
            });
        }
        let checksum_end = checksum_field
            .checked_add(4)
            .ok_or(OverworldAnimationOptionsIoError::OffsetOverflow)?;
        if writes.iter().any(|write| {
            write
                .offset
                .checked_add(write.bytes.len())
                .is_none_or(|end| write.offset < checksum_end && checksum_field < end)
        }) {
            return Err(OverworldAnimationOptionsIoError::ChecksumOverlap);
        }
        let mut staged = self.rom.clone();
        for write in &writes {
            staged.write(write.offset, &write.bytes)?;
        }
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        writes.push(RomWrite {
            offset: checksum_field,
            bytes: checksum.encoded().to_vec(),
        });
        Ok(self.apply_writes("save overworld animation options", &writes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GatedLayout, InstallationMarker};
    use lm_rom::{Mapper, RomImage, pc_to_snes};

    const CHECKSUM: usize = 0x7fdc;
    const HOOK: usize = 0x100;
    const RUNTIME: usize = 0x300;
    const TABLE: usize = 0x600;
    const LIGHTNING: usize = 0x700;

    fn layout() -> OverworldAnimationOptionsRomLayout {
        OverworldAnimationOptionsRomLayout {
            feature_installation: InstalledLayout::Alternatives {
                primary: GatedLayout {
                    marker: InstallationMarker {
                        offset: HOOK,
                        expected: 0x22,
                    },
                    layout: ChainedSnesPointerLocator {
                        mapper: Mapper::LoRom,
                        first_operand_offset: HOOK + 1,
                        final_operand_displacement: 0x4a,
                    },
                },
                fallback: None,
            },
            lightning_disable_mask_offset: LIGHTNING,
        }
    }

    fn write_u24(rom: &mut RomImage, offset: usize, target: usize) {
        let address = pc_to_snes(Mapper::LoRom, target).unwrap();
        rom.write(offset, &address.to_le_bytes()[..3]).unwrap();
    }

    fn installed_project() -> Project {
        let mut rom = RomImage::from_bytes(vec![0xff; 0x8000]).unwrap();
        rom.write(HOOK, &[0x22]).unwrap();
        write_u24(&mut rom, HOOK + 1, RUNTIME);
        write_u24(&mut rom, RUNTIME + 0x4a, TABLE);
        rom.write(TABLE, &[0x10, 0x20, 0x40, 0x80, 0xf0, 0x5a, 0xa5])
            .unwrap();
        rom.write(LIGHTNING, &[0x57]).unwrap();
        Project::new(rom)
    }

    #[test]
    fn installed_chain_loads_all_seven_bytes_and_independent_lightning_mask() {
        let loaded = installed_project()
            .load_installed_overworld_animation_options(layout())
            .unwrap();
        assert_eq!(
            loaded.feature_bytes,
            [0x10, 0x20, 0x40, 0x80, 0xf0, 0x5a, 0xa5]
        );
        assert_eq!(loaded.lightning_disable_mask, 0x57);
        assert!(loaded.runtime_installed);
    }

    #[test]
    fn installed_options_save_reopens_checksums_and_undoes_atomically() {
        let mut project = installed_project();
        let original = project.save_snapshot();
        let features = [0x81, 0x42, 0x24, 0x18, 0xaa, 0x55, 0xf0];
        project
            .save_installed_overworld_animation_options(features, 0xa6, layout(), CHECKSUM)
            .unwrap();
        let loaded = project
            .load_installed_overworld_animation_options(layout())
            .unwrap();
        assert_eq!(loaded.feature_bytes, features);
        assert_eq!(loaded.lightning_disable_mask, 0xa6);
        let checksum = compute_snes_checksum(project.rom.logical_bytes(), CHECKSUM).unwrap();
        assert_eq!(project.rom.read(CHECKSUM, 4).unwrap(), checksum.encoded());
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), original);
        assert!(project.redo().unwrap());
        assert_eq!(
            project
                .load_installed_overworld_animation_options(layout())
                .unwrap(),
            loaded
        );
    }

    #[test]
    fn absent_runtime_uses_vanilla_features_and_refuses_unrepresentable_write() {
        let mut bytes = vec![0xff; 0x40000];
        bytes[LIGHTNING] = 0xf7;
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let loaded = project
            .load_installed_overworld_animation_options(layout())
            .unwrap();
        assert_eq!(loaded.feature_bytes, [0; 7]);
        assert_eq!(loaded.lightning_disable_mask, 0xf7);
        assert!(!loaded.runtime_installed);
        assert!(matches!(
            project.save_installed_overworld_animation_options(
                [0x10, 0, 0, 0, 0, 0, 0],
                0xf7,
                layout(),
                CHECKSUM,
            ),
            Err(OverworldAnimationOptionsIoError::RuntimeInstallationRequired)
        ));
    }
}
