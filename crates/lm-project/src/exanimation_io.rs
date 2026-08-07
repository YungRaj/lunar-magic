use crate::{
    ChainedSnesPointerLocator, InstalledAsset, InstalledLayout, InstalledLayoutError,
    LevelLoadError, LevelPointerTable, LoadedPayload, PayloadLoadError, PayloadReadPolicy,
    PayloadReclamation, PayloadSaveError, PayloadSaveRequest, PayloadSaveResult,
    PointerLocatorError, Project,
};
use lm_graphics::{CompactExAnimation, ExAnimationError};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExAnimationRomLayout {
    pub mapper: Mapper,
    pub pointers: LevelPointerTable,
    pub maximum_records: usize,
    pub maximum_encoded_len: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstalledExAnimationRomLayout {
    pub payload: ExAnimationRomLayout,
    /// A raw 24-bit pointer denotes an empty slot when all selected bits are zero.
    pub pointer_presence_mask: u32,
    /// Optional installed-code lookup for an allocator-dependent pointer table.
    pub pointer_locator: Option<ChainedSnesPointerLocator>,
}

impl InstalledExAnimationRomLayout {
    /// Resolves an allocator-dependent pointer table while retaining every other payload limit.
    ///
    /// # Errors
    ///
    /// Returns a bounds, mapping, or signed-displacement error for malformed installed code.
    pub fn resolve(self, rom: &lm_rom::RomImage) -> Result<Self, PointerLocatorError> {
        let Some(locator) = self.pointer_locator else {
            return Ok(self);
        };
        let mut resolved = self;
        resolved.payload.pointers =
            locator.resolve_level_table(rom, self.payload.pointers.entries)?;
        Ok(resolved)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExAnimationSaveOptions {
    pub allocation: AllocationPolicy,
    pub previous_block: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Debug)]
pub enum ExAnimationIoError {
    Layout(LevelLoadError),
    WrongSizeModeCount(usize),
    Load(PayloadLoadError),
    Animation(ExAnimationError),
    UnconsumedTaggedPayload { consumed: usize, actual: usize },
    NonCanonicalAnimation,
    EncodedLimit { actual: usize, maximum: usize },
    Save(PayloadSaveError),
    Installation(InstalledLayoutError),
    PointerLocator(PointerLocatorError),
    InvalidPointerPresenceMask(u32),
    GlobalPointerLocatorUnavailable,
}

impl fmt::Display for ExAnimationIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ExAnimation I/O failed: {self:?}")
    }
}

impl std::error::Error for ExAnimationIoError {}

impl From<LevelLoadError> for ExAnimationIoError {
    fn from(value: LevelLoadError) -> Self {
        Self::Layout(value)
    }
}

impl From<PayloadLoadError> for ExAnimationIoError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}

impl From<ExAnimationError> for ExAnimationIoError {
    fn from(value: ExAnimationError) -> Self {
        Self::Animation(value)
    }
}

impl From<PayloadSaveError> for ExAnimationIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl From<InstalledLayoutError> for ExAnimationIoError {
    fn from(value: InstalledLayoutError) -> Self {
        Self::Installation(value)
    }
}

impl From<PointerLocatorError> for ExAnimationIoError {
    fn from(value: PointerLocatorError) -> Self {
        Self::PointerLocator(value)
    }
}

impl Project {
    /// Loads Lunar Magic's lazily allocated global ExAnimation set through the installed runtime
    /// hook shared with the per-level pointer-table locator.
    ///
    /// Ghidra's `LoadGlobalExAnimationData` (`0045FDF0`) follows that hook and reads the global
    /// payload pointer at runtime offset `$5C`. An all-zero selected pointer is an intentional
    /// empty global set.
    ///
    /// # Errors
    ///
    /// Returns a marker, locator, mapping, payload, or compact-animation decoding error.
    pub fn load_installed_global_exanimation(
        &self,
        installation: InstalledLayout<InstalledExAnimationRomLayout>,
        double_size_modes: &[bool],
    ) -> Result<InstalledAsset<CompactExAnimation>, ExAnimationIoError> {
        const GLOBAL_POINTER_DISPLACEMENT: isize = 0x5c;
        let Some(installed) = installation.resolve(&self.rom)? else {
            return Ok(InstalledAsset::SubsystemAbsent);
        };
        if installed.pointer_presence_mask == 0
            || installed.pointer_presence_mask & !0x00ff_ffff != 0
        {
            return Err(ExAnimationIoError::InvalidPointerPresenceMask(
                installed.pointer_presence_mask,
            ));
        }
        let mut locator = installed
            .pointer_locator
            .ok_or(ExAnimationIoError::GlobalPointerLocatorUnavailable)?;
        locator.final_operand_displacement = GLOBAL_POINTER_DISPLACEMENT;
        let operand = locator.final_operand_offset(&self.rom)?;
        let bytes = self.rom.read(operand, 3).map_err(PayloadLoadError::Rom)?;
        let raw = u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16;
        if raw & installed.pointer_presence_mask == 0 {
            return Ok(InstalledAsset::SlotEmpty);
        }
        validate_size_modes(double_size_modes)?;
        let pointer = lm_rom::SnesPointer24::new(raw)
            .expect("three decoded pointer bytes always fit 24 bits");
        let payload = self.load_payload_from_pointer(
            pointer,
            installed.payload.mapper,
            &PayloadReadPolicy::TaggedOrBounded {
                maximum_len: installed.payload.maximum_encoded_len,
                bank_size: Some(0x8000),
            },
        )?;
        Ok(InstalledAsset::Present(decode_exanimation_payload(
            payload,
            installed.payload,
            double_size_modes,
        )?))
    }

    /// Saves Lunar Magic's ROM-global compact `ExAnimation` set through the installed runtime's
    /// `$5C` payload-pointer field.
    ///
    /// This refuses to invent the expanded runtime and uses the same checked, copy-on-write RATS
    /// transaction as per-level animation saves.
    ///
    /// # Errors
    ///
    /// Returns a marker, locator, validation, allocation, mapping, or transaction error without
    /// mutating the project.
    pub fn save_installed_global_exanimation(
        &mut self,
        animation: &CompactExAnimation,
        installation: InstalledLayout<InstalledExAnimationRomLayout>,
        double_size_modes: &[bool],
        options: &ExAnimationSaveOptions,
    ) -> Result<PayloadSaveResult, ExAnimationIoError> {
        let request = installed_global_exanimation_save_request(
            &self.rom,
            animation,
            installation,
            double_size_modes,
            options,
        )?;
        Ok(self.save_tagged_payload(&request)?)
    }

    /// Saves ROM-global `ExAnimation` and repairs the SNES checksum in the same transaction.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationIoError`] without mutation when the installed runtime, compact payload,
    /// allocation, pointer publication, or checksum field is invalid.
    pub fn save_installed_global_exanimation_with_checksum(
        &mut self,
        animation: &CompactExAnimation,
        installation: InstalledLayout<InstalledExAnimationRomLayout>,
        double_size_modes: &[bool],
        checksum_field: usize,
        options: &ExAnimationSaveOptions,
    ) -> Result<PayloadSaveResult, ExAnimationIoError> {
        let request = installed_global_exanimation_save_request(
            &self.rom,
            animation,
            installation,
            double_size_modes,
            options,
        )?;
        Ok(self
            .save_tagged_payloads_with_checksum(
                &request.description,
                std::slice::from_ref(&request),
                checksum_field,
            )?
            .remove(0))
    }

    /// Saves and reclaims an exactly owned prior ROM-global animation allocation while repairing
    /// the checksum atomically.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationIoError`] without mutation for invalid installed ownership, encoding,
    /// allocation, reclamation, pointer, or checksum state.
    pub fn save_installed_global_exanimation_with_checksum_and_reclamation(
        &mut self,
        animation: &CompactExAnimation,
        installation: InstalledLayout<InstalledExAnimationRomLayout>,
        double_size_modes: &[bool],
        options: &ExAnimationSaveOptions,
        reclamation: PayloadReclamation<'_>,
    ) -> Result<PayloadSaveResult, ExAnimationIoError> {
        let request = installed_global_exanimation_save_request(
            &self.rom,
            animation,
            installation,
            double_size_modes,
            options,
        )?;
        Ok(self
            .save_tagged_payloads_with_checksum_and_reclamation(
                &request.description,
                std::slice::from_ref(&request),
                reclamation.checksum_field,
                reclamation.manifest,
            )?
            .remove(0))
    }

    /// Resolves the installed hook variant and distinguishes an absent subsystem, empty slot, and
    /// present compact `ExAnimation` payload.
    ///
    /// # Errors
    ///
    /// Returns a marker/pointer bounds error, invalid presence mask, or ordinary decoding error.
    pub fn load_installed_exanimation(
        &self,
        slot: usize,
        installation: InstalledLayout<InstalledExAnimationRomLayout>,
        double_size_modes: &[bool],
    ) -> Result<InstalledAsset<CompactExAnimation>, ExAnimationIoError> {
        let Some(installed) = installation.resolve(&self.rom)? else {
            return Ok(InstalledAsset::SubsystemAbsent);
        };
        let installed = installed.resolve(&self.rom)?;
        if installed.pointer_presence_mask == 0
            || installed.pointer_presence_mask & !0x00ff_ffff != 0
        {
            return Err(ExAnimationIoError::InvalidPointerPresenceMask(
                installed.pointer_presence_mask,
            ));
        }
        let pointer_offset = installed.payload.pointers.pointer_offset(slot)?;
        let bytes = self
            .rom
            .read(pointer_offset, 3)
            .map_err(PayloadLoadError::Rom)?;
        let raw = u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16);
        if raw & installed.pointer_presence_mask == 0 {
            return Ok(InstalledAsset::SlotEmpty);
        }
        Ok(InstalledAsset::Present(self.load_exanimation(
            slot,
            installed.payload,
            double_size_modes,
        )?))
    }

    /// Saves through the marker-selected `ExAnimation` table and refuses to invent an installation.
    ///
    /// # Errors
    ///
    /// Returns [`InstalledLayoutError::NotInstalled`] when neither hook marker matches.
    pub fn save_installed_exanimation(
        &mut self,
        slot: usize,
        animation: &CompactExAnimation,
        installation: InstalledLayout<InstalledExAnimationRomLayout>,
        double_size_modes: &[bool],
        options: &ExAnimationSaveOptions,
    ) -> Result<PayloadSaveResult, ExAnimationIoError> {
        let installed = installation
            .resolve(&self.rom)?
            .ok_or(InstalledLayoutError::NotInstalled("per-level ExAnimation"))?
            .resolve(&self.rom)?;
        if installed.pointer_presence_mask == 0
            || installed.pointer_presence_mask & !0x00ff_ffff != 0
        {
            return Err(ExAnimationIoError::InvalidPointerPresenceMask(
                installed.pointer_presence_mask,
            ));
        }
        self.save_exanimation(
            slot,
            animation,
            installed.payload,
            double_size_modes,
            options,
        )
    }

    /// Loads one compact global or per-level `ExAnimation` payload.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationIoError`] for invalid layouts, payloads, offsets, records, or modes.
    pub fn load_exanimation(
        &self,
        slot: usize,
        layout: ExAnimationRomLayout,
        double_size_modes: &[bool],
    ) -> Result<CompactExAnimation, ExAnimationIoError> {
        validate_size_modes(double_size_modes)?;
        let payload = self.load_payload(
            layout.pointers.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrBounded {
                maximum_len: layout.maximum_encoded_len,
                bank_size: Some(0x8000),
            },
        )?;
        decode_exanimation_payload(payload, layout, double_size_modes)
    }

    /// Encodes and transactionally relocates one compact `ExAnimation` payload.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationIoError`] for invalid records/modes, limits, allocation, or mapping.
    pub fn save_exanimation(
        &mut self,
        slot: usize,
        animation: &CompactExAnimation,
        layout: ExAnimationRomLayout,
        double_size_modes: &[bool],
        options: &ExAnimationSaveOptions,
    ) -> Result<PayloadSaveResult, ExAnimationIoError> {
        Ok(self.save_tagged_payload(&exanimation_save_request(
            slot,
            animation,
            layout,
            double_size_modes,
            options,
        )?)?)
    }

    /// Saves compact `ExAnimation` data and repairs the SNES checksum in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationIoError`] when validation, allocation, mapping, or checksum repair fails.
    pub fn save_exanimation_with_checksum(
        &mut self,
        slot: usize,
        animation: &CompactExAnimation,
        layout: ExAnimationRomLayout,
        double_size_modes: &[bool],
        checksum_field: usize,
        options: &ExAnimationSaveOptions,
    ) -> Result<PayloadSaveResult, ExAnimationIoError> {
        let request =
            exanimation_save_request(slot, animation, layout, double_size_modes, options)?;
        Ok(self
            .save_tagged_payloads_with_checksum(
                &request.description,
                std::slice::from_ref(&request),
                checksum_field,
            )?
            .remove(0))
    }

    /// Saves, reclaims the exactly owned previous animation block, and repairs checksum atomically.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationIoError`] for validation, ownership, allocation, overlap, mapping, or
    /// checksum failure without mutation.
    pub fn save_exanimation_with_checksum_and_reclamation(
        &mut self,
        slot: usize,
        animation: &CompactExAnimation,
        layout: ExAnimationRomLayout,
        double_size_modes: &[bool],
        options: &ExAnimationSaveOptions,
        reclamation: PayloadReclamation<'_>,
    ) -> Result<PayloadSaveResult, ExAnimationIoError> {
        let request =
            exanimation_save_request(slot, animation, layout, double_size_modes, options)?;
        Ok(self
            .save_tagged_payloads_with_checksum_and_reclamation(
                &request.description,
                std::slice::from_ref(&request),
                reclamation.checksum_field,
                reclamation.manifest,
            )?
            .remove(0))
    }
}

fn decode_exanimation_payload(
    payload: LoadedPayload,
    layout: ExAnimationRomLayout,
    double_size_modes: &[bool],
) -> Result<CompactExAnimation, ExAnimationIoError> {
    let (animation, consumed) =
        CompactExAnimation::decode(&payload.bytes, layout.maximum_records, double_size_modes)?;
    if payload.block.is_some() && consumed != payload.bytes.len() {
        return Err(ExAnimationIoError::UnconsumedTaggedPayload {
            consumed,
            actual: payload.bytes.len(),
        });
    }
    Ok(animation)
}

pub(crate) fn exanimation_save_request(
    slot: usize,
    animation: &CompactExAnimation,
    layout: ExAnimationRomLayout,
    double_size_modes: &[bool],
    options: &ExAnimationSaveOptions,
) -> Result<PayloadSaveRequest, ExAnimationIoError> {
    exanimation_save_request_at_pointer(
        &format!("save ExAnimation slot {slot:03x}"),
        layout.pointers.pointer_offset(slot)?,
        animation,
        layout,
        double_size_modes,
        options,
    )
}

fn exanimation_save_request_at_pointer(
    description: &str,
    pointer_offset: usize,
    animation: &CompactExAnimation,
    layout: ExAnimationRomLayout,
    double_size_modes: &[bool],
    options: &ExAnimationSaveOptions,
) -> Result<PayloadSaveRequest, ExAnimationIoError> {
    validate_size_modes(double_size_modes)?;
    let payload = animation.encode(double_size_modes)?;
    if payload.len() > layout.maximum_encoded_len {
        return Err(ExAnimationIoError::EncodedLimit {
            actual: payload.len(),
            maximum: layout.maximum_encoded_len,
        });
    }
    let (canonical, consumed) =
        CompactExAnimation::decode(&payload, layout.maximum_records, double_size_modes)?;
    if consumed != payload.len() || &canonical != animation {
        return Err(ExAnimationIoError::NonCanonicalAnimation);
    }
    Ok(PayloadSaveRequest {
        description: description.into(),
        payload,
        pointer: pointer_offset.into(),
        mapper: layout.mapper,
        allocation_policy: options.allocation.clone(),
        previous_block: options.previous_block.clone(),
        reuse_identical: options.reuse_identical,
        maximum_payload_len: layout.maximum_encoded_len,
        erase_fill: options.erase_fill,
    })
}

fn installed_global_exanimation_save_request(
    rom: &lm_rom::RomImage,
    animation: &CompactExAnimation,
    installation: InstalledLayout<InstalledExAnimationRomLayout>,
    double_size_modes: &[bool],
    options: &ExAnimationSaveOptions,
) -> Result<PayloadSaveRequest, ExAnimationIoError> {
    const GLOBAL_POINTER_DISPLACEMENT: isize = 0x5c;
    let installed = installation
        .resolve(rom)?
        .ok_or(InstalledLayoutError::NotInstalled("global ExAnimation"))?;
    if installed.pointer_presence_mask == 0 || installed.pointer_presence_mask & !0x00ff_ffff != 0 {
        return Err(ExAnimationIoError::InvalidPointerPresenceMask(
            installed.pointer_presence_mask,
        ));
    }
    let mut locator = installed
        .pointer_locator
        .ok_or(ExAnimationIoError::GlobalPointerLocatorUnavailable)?;
    locator.final_operand_displacement = GLOBAL_POINTER_DISPLACEMENT;
    let pointer_offset = locator.final_operand_offset(rom)?;
    exanimation_save_request_at_pointer(
        "save global ExAnimation",
        pointer_offset,
        animation,
        installed.payload,
        double_size_modes,
        options,
    )
}

fn validate_size_modes(double_size_modes: &[bool]) -> Result<(), ExAnimationIoError> {
    if double_size_modes.len() == 256 {
        Ok(())
    } else {
        Err(ExAnimationIoError::WrongSizeModeCount(
            double_size_modes.len(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::ProtectedRange;
    use lm_rom::{RomImage, pc_to_snes};

    fn write_u24(rom: &mut RomImage, offset: usize, value: u32) {
        let bytes = value.to_le_bytes();
        rom.write(offset, &bytes[..3]).unwrap();
    }

    fn layout() -> ExAnimationRomLayout {
        ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x20,
                entries: 2,
                stride: 3,
            },
            maximum_records: 32,
            maximum_encoded_len: 0x4000,
        }
    }

    fn options() -> ExAnimationSaveOptions {
        ExAnimationSaveOptions {
            allocation: AllocationPolicy {
                search: 0x100..0x8000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![ProtectedRange(0x20..0x26)],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    fn animation() -> CompactExAnimation {
        let mut trigger_values = [0; 16];
        trigger_values[2] = 9;
        CompactExAnimation {
            setting: 3,
            header_value: 0x1234_5678,
            trigger_mask: 4,
            trigger_values,
            records: Vec::new(),
        }
    }

    #[test]
    fn tagged_animation_save_load_and_undo_round_trip() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        project
            .save_exanimation(1, &animation(), layout(), &[false; 256], &options())
            .unwrap();
        assert_eq!(
            project
                .load_exanimation(1, layout(), &[false; 256])
                .unwrap(),
            animation()
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    fn installed_layout() -> InstalledLayout<InstalledExAnimationRomLayout> {
        InstalledLayout::Alternatives {
            primary: crate::GatedLayout {
                marker: crate::InstallationMarker {
                    offset: 0x10,
                    expected: 0x22,
                },
                layout: InstalledExAnimationRomLayout {
                    payload: layout(),
                    pointer_presence_mask: 0x00ff_ff00,
                    pointer_locator: None,
                },
            },
            fallback: Some(crate::GatedLayout {
                marker: crate::InstallationMarker {
                    offset: 0x11,
                    expected: 0x22,
                },
                layout: InstalledExAnimationRomLayout {
                    payload: layout(),
                    pointer_presence_mask: 0x00ff_0000,
                    pointer_locator: None,
                },
            }),
        }
    }

    #[test]
    fn installed_exanimation_distinguishes_absent_empty_and_present() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        assert_eq!(
            project
                .load_installed_exanimation(0, installed_layout(), &[false; 256])
                .unwrap(),
            InstalledAsset::SubsystemAbsent
        );
        project.rom.write(0x11, &[0x22]).unwrap();
        project.rom.write(0x20, &[0, 0, 0]).unwrap();
        assert_eq!(
            project
                .load_installed_exanimation(0, installed_layout(), &[false; 256])
                .unwrap(),
            InstalledAsset::SlotEmpty
        );
        project
            .save_installed_exanimation(
                0,
                &animation(),
                installed_layout(),
                &[false; 256],
                &options(),
            )
            .unwrap();
        assert_eq!(
            project
                .load_installed_exanimation(0, installed_layout(), &[false; 256])
                .unwrap(),
            InstalledAsset::Present(animation())
        );
    }

    #[test]
    fn absent_exanimation_hook_rejects_writes_without_mutation() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        assert!(matches!(
            project.save_installed_exanimation(
                0,
                &animation(),
                installed_layout(),
                &[false; 256],
                &options(),
            ),
            Err(ExAnimationIoError::Installation(
                InstalledLayoutError::NotInstalled("per-level ExAnimation")
            ))
        ));
        assert_eq!(project.save_snapshot(), original);
        assert_eq!(project.history.undo_len(), 0);
    }

    #[test]
    fn installed_exanimation_follows_allocator_dependent_pointer_table() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let mut concrete = layout();
        concrete.pointers.offset = 0x1000;
        let mut concrete_options = options();
        concrete_options.allocation.protected = vec![ProtectedRange(0x1000..0x1006)];
        project
            .save_exanimation(0, &animation(), concrete, &[false; 256], &concrete_options)
            .unwrap();
        let runtime_target = 0x400;
        let final_operand = 0x3e0;
        project.rom.write(0x10, &[0x22]).unwrap();
        write_u24(
            &mut project.rom,
            0x11,
            pc_to_snes(Mapper::LoRom, runtime_target).unwrap(),
        );
        write_u24(
            &mut project.rom,
            final_operand,
            pc_to_snes(Mapper::LoRom, concrete.pointers.offset).unwrap(),
        );
        let installed = InstalledLayout::Alternatives {
            primary: crate::GatedLayout {
                marker: crate::InstallationMarker {
                    offset: 0x10,
                    expected: 0x22,
                },
                layout: InstalledExAnimationRomLayout {
                    payload: layout(),
                    pointer_presence_mask: 0x00ff_ff00,
                    pointer_locator: Some(ChainedSnesPointerLocator {
                        mapper: Mapper::LoRom,
                        first_operand_offset: 0x11,
                        final_operand_displacement: -0x20,
                    }),
                },
            },
            fallback: None,
        };
        assert_eq!(
            project
                .load_installed_exanimation(0, installed, &[false; 256])
                .unwrap(),
            InstalledAsset::Present(animation())
        );
    }

    #[test]
    fn installed_global_exanimation_follows_runtime_offset_5c_and_distinguishes_empty() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let mut concrete = layout();
        concrete.pointers.offset = 0x1000;
        let mut concrete_options = options();
        concrete_options.allocation.protected = vec![ProtectedRange(0x1000..0x1003)];
        project
            .save_exanimation(0, &animation(), concrete, &[false; 256], &concrete_options)
            .unwrap();
        let payload_pointer = project.rom.read(0x1000, 3).unwrap().to_vec();
        let runtime_target = 0x7000;
        project.rom.write(0x10, &[0x22]).unwrap();
        write_u24(
            &mut project.rom,
            0x11,
            pc_to_snes(Mapper::LoRom, runtime_target).unwrap(),
        );
        project
            .rom
            .write(runtime_target + 0x5c, &[0, 0, 0])
            .unwrap();
        let installed = InstalledLayout::Alternatives {
            primary: crate::GatedLayout {
                marker: crate::InstallationMarker {
                    offset: 0x10,
                    expected: 0x22,
                },
                layout: InstalledExAnimationRomLayout {
                    payload: layout(),
                    pointer_presence_mask: 0x00ff_0000,
                    pointer_locator: Some(ChainedSnesPointerLocator {
                        mapper: Mapper::LoRom,
                        first_operand_offset: 0x11,
                        final_operand_displacement: -0x20,
                    }),
                },
            },
            fallback: None,
        };
        assert_eq!(
            project
                .load_installed_global_exanimation(installed, &[false; 256])
                .unwrap(),
            InstalledAsset::SlotEmpty
        );
        project
            .rom
            .write(runtime_target + 0x5c, &payload_pointer)
            .unwrap();
        assert_eq!(
            project
                .load_installed_global_exanimation(installed, &[false; 256])
                .unwrap(),
            InstalledAsset::Present(animation())
        );
    }

    #[test]
    fn installed_global_exanimation_save_reopens_and_undoes_through_runtime_offset_5c() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let runtime_target = 0x7000;
        project.rom.write(0x10, &[0x22]).unwrap();
        write_u24(
            &mut project.rom,
            0x11,
            pc_to_snes(Mapper::LoRom, runtime_target).unwrap(),
        );
        project
            .rom
            .write(runtime_target + 0x5c, &[0, 0, 0])
            .unwrap();
        let installed = InstalledLayout::Alternatives {
            primary: crate::GatedLayout {
                marker: crate::InstallationMarker {
                    offset: 0x10,
                    expected: 0x22,
                },
                layout: InstalledExAnimationRomLayout {
                    payload: layout(),
                    pointer_presence_mask: 0x00ff_0000,
                    pointer_locator: Some(ChainedSnesPointerLocator {
                        mapper: Mapper::LoRom,
                        first_operand_offset: 0x11,
                        final_operand_displacement: -0x20,
                    }),
                },
            },
            fallback: None,
        };
        let original = project.save_snapshot();
        let mut save_options = options();
        save_options.allocation.protected = vec![
            ProtectedRange(0x10..0x14),
            ProtectedRange(runtime_target + 0x5c..runtime_target + 0x5f),
            ProtectedRange(0x7fdc..0x7fe0),
        ];

        project
            .save_installed_global_exanimation(
                &animation(),
                installed,
                &[false; 256],
                &save_options,
            )
            .unwrap();
        assert_eq!(
            project
                .load_installed_global_exanimation(installed, &[false; 256])
                .unwrap(),
            InstalledAsset::Present(animation())
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);

        project
            .save_installed_global_exanimation_with_checksum(
                &animation(),
                installed,
                &[false; 256],
                0x7fdc,
                &save_options,
            )
            .unwrap();

        assert_eq!(
            project
                .load_installed_global_exanimation(installed, &[false; 256])
                .unwrap(),
            InstalledAsset::Present(animation())
        );
        let expected = lm_rom::compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap();
        assert_eq!(
            project.rom.read(0x7fdc, 4).unwrap(),
            [
                (expected.complement & 0xff) as u8,
                (expected.complement >> 8) as u8,
                (expected.checksum & 0xff) as u8,
                (expected.checksum >> 8) as u8,
            ]
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn pristine_untagged_animation_loads_from_a_bounded_bank() {
        let encoded = animation().encode(&[false; 256]).unwrap();
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x81, 0x80]);
        bytes[0x100..0x100 + encoded.len()].copy_from_slice(&encoded);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert_eq!(
            project
                .load_exanimation(0, layout(), &[false; 256])
                .unwrap(),
            animation()
        );
    }

    #[test]
    fn encoded_limit_failure_preserves_project() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let mut too_small = layout();
        too_small.maximum_encoded_len = 8;
        assert!(matches!(
            project.save_exanimation(0, &animation(), too_small, &[false; 256], &options()),
            Err(ExAnimationIoError::EncodedLimit { .. })
        ));
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn disabled_trigger_value_cannot_silently_disappear_on_reopen() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let mut invalid = animation();
        invalid.trigger_values[7] = 0xaa;
        assert!(matches!(
            project.save_exanimation(0, &invalid, layout(), &[false; 256], &options()),
            Err(ExAnimationIoError::Animation(
                ExAnimationError::DisabledTriggerValue {
                    trigger: 7,
                    value: 0xaa
                }
            ))
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }

    #[test]
    fn size_mode_table_must_have_the_revision_shape() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        assert!(matches!(
            project.load_exanimation(0, layout(), &[false; 255]),
            Err(ExAnimationIoError::WrongSizeModeCount(255))
        ));
        assert!(matches!(
            project.save_exanimation(0, &animation(), layout(), &[false; 257], &options()),
            Err(ExAnimationIoError::WrongSizeModeCount(257))
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }

    #[test]
    fn tagged_payload_cannot_hide_trailing_bytes() {
        let mut payload = animation().encode(&[false; 256]).unwrap();
        let canonical_len = payload.len();
        payload.extend_from_slice(&[0xaa, 0xbb]);
        let mut bytes = vec![0xff; 0x8000];
        let block = lm_rats::FreeSpaceAllocator::new(
            &mut bytes,
            AllocationPolicy {
                search: 0x100..0x8000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![ProtectedRange(0x20..0x26)],
            },
        )
        .allocate(&payload)
        .unwrap();
        let pointer = lm_rom::pc_to_snes(Mapper::LoRom, block.payload.start)
            .unwrap()
            .to_le_bytes();
        bytes[0x20..0x23].copy_from_slice(&pointer[..3]);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert!(matches!(
            project.load_exanimation(0, layout(), &[false; 256]),
            Err(ExAnimationIoError::UnconsumedTaggedPayload {
                consumed,
                actual,
            }) if consumed == canonical_len && actual == payload.len()
        ));
    }

    #[test]
    fn models_trimmed_or_rejected_by_revision_decode_never_allocate() {
        let mut trailing_inactive = animation();
        trailing_inactive
            .records
            .push(lm_graphics::ExAnimationRecord::inactive());
        let mut too_many = animation();
        too_many.records = (0..33)
            .map(|_| {
                lm_graphics::ExAnimationRecord::new(1, 0, 0, 0, false, &[0, 0], false).unwrap()
            })
            .collect();
        for invalid in [trailing_inactive, too_many] {
            let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
            let original = project.save_snapshot();
            assert!(
                project
                    .save_exanimation(0, &invalid, layout(), &[false; 256], &options())
                    .is_err()
            );
            assert_eq!(project.save_snapshot(), original);
            assert!(!project.history.can_undo());
        }
    }
}
