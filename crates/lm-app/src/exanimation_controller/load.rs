use super::{ExAnimationController, ExAnimationControllerError};
use crate::{ControllerSnapshot, EditorMode};
use lm_project::{ExAnimationRomLayout, PayloadReadPolicy, Project};
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
            slot: usize::from(slot),
            layout,
            checksum_field_offset: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            double_size_modes: modes,
            baseline: animation.clone(),
            animation,
            previous_block,
        })
    }
}
