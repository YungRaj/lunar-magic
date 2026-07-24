use super::{Map16Controller, Map16ControllerError};
use crate::{ControllerSnapshot, EditorMode};
use lm_level::Map16Set;
use lm_project::{Map16IoError, Map16RomLayout, PayloadReadPolicy, Project};
use lm_rom::RomImage;

impl Map16Controller {
    /// Decodes every Map16 page declared by the explicit parallel table layout.
    ///
    /// # Errors
    ///
    /// Returns [`Map16ControllerError`] for the wrong editor mode, a mapper disagreement, malformed
    /// source image/layout, or any invalid page payload.
    pub fn decode(
        snapshot: &ControllerSnapshot,
        layout: Map16RomLayout,
    ) -> Result<Self, Map16ControllerError> {
        if snapshot.mode != EditorMode::Map16 {
            return Err(Map16ControllerError::WrongMode(snapshot.mode));
        }
        if snapshot.identity.mapper != layout.mapper {
            return Err(Map16ControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: layout.mapper,
            });
        }
        let image =
            RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(Map16ControllerError::Rom)?;
        let project = Project::new(image);
        let set = project
            .load_map16_set(layout)
            .map_err(Map16ControllerError::Io)?;
        let previous_graphics = (0..layout.graphics.entries)
            .map(|page| {
                project
                    .load_payload(
                        layout
                            .graphics
                            .pointer_offset(page)
                            .map_err(Map16IoError::from)?,
                        layout.mapper,
                        &PayloadReadPolicy::TaggedOrFixed {
                            len: Map16Set::GRAPHICS_PAGE_LEN,
                        },
                    )
                    .map(|payload| payload.block)
                    .map_err(Map16IoError::from)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| Map16ControllerError::Io(error.into()))?;
        let previous_acts_like = (0..layout.acts_like.entries)
            .map(|page| {
                project
                    .load_payload(
                        layout
                            .acts_like
                            .pointer_offset(page)
                            .map_err(Map16IoError::from)?,
                        layout.mapper,
                        &PayloadReadPolicy::TaggedOrFixed {
                            len: Map16Set::ACTS_LIKE_PAGE_LEN,
                        },
                    )
                    .map(|payload| payload.block)
                    .map_err(Map16IoError::from)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| Map16ControllerError::Io(error.into()))?;
        Ok(Self {
            revision: snapshot.revision,
            layout,
            checksum_field_offset: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            baseline: set.clone(),
            set,
            previous_graphics,
            previous_acts_like,
        })
    }
}
