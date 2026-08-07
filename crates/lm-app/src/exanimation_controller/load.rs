use super::{ExAnimationController, ExAnimationControllerError, ExAnimationControllerTarget};
use crate::{ControllerSnapshot, EditorMode};
use lm_graphics::CompactExAnimation;
use lm_project::{
    ExAnimationRomLayout, InstalledAsset, InstalledExAnimationRomLayout, InstalledLayout,
    PayloadReadPolicy, Project,
};
use lm_rom::RomImage;

impl ExAnimationController {
    /// Loads the selected slot using the exact recovered 256-entry transfer-size table.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationControllerError`] for wrong mode/mapper, a non-256-entry size table,
    /// or any native payload/layout/record failure.
    pub fn decode(
        snapshot: &ControllerSnapshot,
        layout: ExAnimationRomLayout,
        double_size_modes: &[bool],
    ) -> Result<Self, ExAnimationControllerError> {
        let EditorMode::ExAnimation(slot) = snapshot.mode else {
            return Err(ExAnimationControllerError::WrongMode(snapshot.mode));
        };
        if snapshot.identity.mapper != layout.mapper {
            return Err(ExAnimationControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: layout.mapper,
            });
        }
        let modes: [bool; 256] = double_size_modes
            .try_into()
            .map_err(|_| ExAnimationControllerError::SizeModeCount(double_size_modes.len()))?;
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(ExAnimationControllerError::Rom)?;
        let project = Project::new(image);
        let pointer = layout
            .pointers
            .pointer_offset(usize::from(slot))
            .map_err(|error| ExAnimationControllerError::Io(error.into()))?;
        let previous_block = project
            .load_payload(
                pointer,
                layout.mapper,
                &PayloadReadPolicy::TaggedOrBounded {
                    maximum_len: layout.maximum_encoded_len,
                    bank_size: Some(0x8000),
                },
            )
            .map_err(|error| ExAnimationControllerError::Io(error.into()))?
            .block;
        let animation = project
            .load_exanimation(usize::from(slot), layout, &modes)
            .map_err(ExAnimationControllerError::Io)?;
        Ok(Self {
            revision: snapshot.revision,
            target: ExAnimationControllerTarget::Level(usize::from(slot)),
            layout,
            checksum_field_offset: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            double_size_modes: modes,
            baseline: animation.clone(),
            animation,
            previous_block,
        })
    }

    /// Loads Lunar Magic's ROM-global 32-record ExAnimation domain through the authenticated
    /// installed runtime selected by the active revision profile.
    ///
    /// An installed runtime with a zero global pointer opens as an empty editable set. The current
    /// ExAnimation level-slot selection remains intact so the native editor can switch between the
    /// two domains without changing application navigation state.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationControllerError`] for a wrong editor mode, mapper/profile disagreement,
    /// malformed size modes, absent runtime, or invalid global payload.
    pub fn decode_global(
        snapshot: &ControllerSnapshot,
        installation: InstalledLayout<InstalledExAnimationRomLayout>,
        double_size_modes: &[bool],
    ) -> Result<Self, ExAnimationControllerError> {
        if !matches!(snapshot.mode, EditorMode::ExAnimation(_)) {
            return Err(ExAnimationControllerError::WrongMode(snapshot.mode));
        }
        let modes: [bool; 256] = double_size_modes
            .try_into()
            .map_err(|_| ExAnimationControllerError::SizeModeCount(double_size_modes.len()))?;
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(ExAnimationControllerError::Rom)?;
        let project = Project::new(image);
        let loaded = project
            .load_installed_global_exanimation_with_ownership(installation, &modes)
            .map_err(ExAnimationControllerError::Io)?;
        let animation = match loaded.asset {
            InstalledAsset::Present(animation) => animation,
            InstalledAsset::SlotEmpty => CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: Vec::new(),
            },
            InstalledAsset::SubsystemAbsent => {
                return Err(ExAnimationControllerError::Io(
                    lm_project::ExAnimationIoError::Installation(
                        lm_project::InstalledLayoutError::NotInstalled("global ExAnimation"),
                    ),
                ));
            }
        };
        let layout = installation
            .resolve(&project.rom)
            .map_err(|error| ExAnimationControllerError::Io(error.into()))?
            .ok_or_else(|| {
                ExAnimationControllerError::Io(lm_project::ExAnimationIoError::Installation(
                    lm_project::InstalledLayoutError::NotInstalled("global ExAnimation"),
                ))
            })?
            .payload;
        if snapshot.identity.mapper != layout.mapper {
            return Err(ExAnimationControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: layout.mapper,
            });
        }
        Ok(Self {
            revision: snapshot.revision,
            target: ExAnimationControllerTarget::Global(installation),
            layout,
            checksum_field_offset: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            double_size_modes: modes,
            baseline: animation.clone(),
            animation,
            previous_block: loaded.block,
        })
    }
}
