use crate::EditorMode;
use lm_graphics::{
    CompactExAnimation, ExAnimationEditError, ExAnimationError, ExAnimationFrame,
    ExAnimationFrameEdit, ExAnimationFrameEditError, ExAnimationRecord, exanimation_frames,
};
use lm_project::{ExAnimationIoError, ExAnimationRomLayout, TransactionError};
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomError};
use std::fmt;

mod commit;
mod editing;
mod load;

pub(crate) use editing::apply_animation_edits;

/// One ordered mutation in a compact native `ExAnimation` payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationControllerEdit {
    SetSetting(u8),
    SetHeaderValue(u32),
    SetTrigger {
        trigger: usize,
        value: Option<u8>,
    },
    InsertRecord {
        index: usize,
        record: ExAnimationRecord,
    },
    ReplaceRecord {
        index: usize,
        record: ExAnimationRecord,
    },
    RemoveRecord {
        index: usize,
    },
    MoveRecordBefore {
        from: usize,
        before: usize,
    },
    EditRecordFrames {
        record: usize,
        edits: Vec<ExAnimationFrameEdit>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationControllerEditFailure {
    Animation(ExAnimationEditError),
    Encoding(ExAnimationError),
    NonCanonicalEncoding,
    Frames {
        record: usize,
        error: ExAnimationFrameEditError,
    },
}

#[derive(Debug)]
pub enum ExAnimationControllerError {
    WrongMode(EditorMode),
    MapperMismatch {
        snapshot: Mapper,
        layout: Mapper,
    },
    SizeModeCount(usize),
    Rom(RomError),
    Io(ExAnimationIoError),
    Edit {
        command: usize,
        error: ExAnimationControllerEditFailure,
    },
    Mutation(TransactionError),
}

impl fmt::Display for ExAnimationControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ExAnimation controller failed: {self:?}")
    }
}

impl std::error::Error for ExAnimationControllerError {}

/// One compact native `ExAnimation` slot decoded from an immutable application snapshot.
#[derive(Clone, Debug)]
pub struct ExAnimationController {
    revision: u64,
    slot: usize,
    layout: ExAnimationRomLayout,
    checksum_field_offset: usize,
    source_file_bytes: Vec<u8>,
    double_size_modes: [bool; 256],
    baseline: CompactExAnimation,
    animation: CompactExAnimation,
    previous_block: Option<RatsBlock>,
}

impl ExAnimationController {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn animation(&self) -> &CompactExAnimation {
        &self.animation
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.animation != self.baseline
    }

    /// Returns the ordinary source-word frames for one record using its revision size mode.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationControllerEditFailure`] for an absent record or a transfer kind with no
    /// ordinary frame payload.
    pub fn record_frames(
        &self,
        record: usize,
    ) -> Result<Vec<ExAnimationFrame>, ExAnimationControllerEditFailure> {
        let len = self.animation.records.len();
        let value = self.animation.records.get(record).ok_or({
            ExAnimationControllerEditFailure::Animation(
                ExAnimationEditError::RecordIndexOutOfRange { index: record, len },
            )
        })?;
        exanimation_frames(
            value,
            self.double_size_modes[usize::from(value.size_mode())],
        )
        .map_err(|error| ExAnimationControllerEditFailure::Frames { record, error })
    }

    /// Applies one atomic source-frame edit batch to a selected record.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationControllerError`] without mutation for invalid record/frame indexes,
    /// incompatible frame widths, no-payload kinds, or compact-capacity overflow.
    pub fn apply_frame_edits(
        &mut self,
        record: usize,
        edits: Vec<ExAnimationFrameEdit>,
    ) -> Result<(), ExAnimationControllerError> {
        self.apply_edits(&[ExAnimationControllerEdit::EditRecordFrames { record, edits }])
    }

    /// Applies a mixed ordered edit batch on a staged animation.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationControllerError::Edit`] with the failing command. Record limits use the
    /// explicit native layout, and any failure rolls back all earlier commands in this call.
    pub fn apply_edits(
        &mut self,
        edits: &[ExAnimationControllerEdit],
    ) -> Result<(), ExAnimationControllerError> {
        if edits.is_empty() {
            return Ok(());
        }
        let mut staged = self.animation.clone();
        apply_animation_edits(
            &mut staged,
            edits,
            self.layout.maximum_records,
            &self.double_size_modes,
        )
        .map_err(|(command, error)| ExAnimationControllerError::Edit { command, error })?;
        self.animation = staged;
        Ok(())
    }
}

#[cfg(test)]
#[path = "exanimation_controller_tests.rs"]
mod tests;
