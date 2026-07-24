use crate::{
    ClipboardError, ClipboardKind, ClipboardPayload, ExAnimationController,
    ExAnimationControllerError,
};
use lm_graphics::ExAnimationFrameEdit;
use std::collections::BTreeSet;
use std::fmt;

#[derive(Debug)]
pub enum ExAnimationClipboardError {
    WrongSelectionKind(ClipboardKind),
    EmptySelection,
    DuplicateFrame(usize),
    FrameOutOfRange { index: usize, len: usize },
    Clipboard(ClipboardError),
    Controller(ExAnimationControllerError),
}

impl fmt::Display for ExAnimationClipboardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "ExAnimation clipboard operation failed: {self:?}"
        )
    }
}

impl std::error::Error for ExAnimationClipboardError {}

/// Copies selected frame indexes in ascending canonical order.
///
/// # Errors
///
/// Returns [`ExAnimationClipboardError`] for the wrong selection domain, empty/duplicate/out-of-
/// range indexes, an invalid record, or an unrepresentable frame payload.
pub fn copy_exanimation_frames(
    controller: &ExAnimationController,
    record: usize,
    kind: ClipboardKind,
    indices: &[usize],
) -> Result<ClipboardPayload, ExAnimationClipboardError> {
    let frames = controller.record_frames(record).map_err(|error| {
        ExAnimationClipboardError::Controller(ExAnimationControllerError::Edit {
            command: 0,
            error,
        })
    })?;
    let indices = validate_selection(kind, indices, frames.len())?;
    let selected = indices
        .into_iter()
        .map(|index| frames[index].clone())
        .collect::<Vec<_>>();
    ClipboardPayload::from_exanimation_frames(&selected)
        .map_err(ExAnimationClipboardError::Clipboard)
}

/// Copies then atomically removes selected frames in descending index order.
///
/// # Errors
///
/// Returns [`ExAnimationClipboardError`] without controller mutation if copying, selection
/// validation, removal, or final record validation fails.
pub fn cut_exanimation_frames(
    controller: &mut ExAnimationController,
    record: usize,
    kind: ClipboardKind,
    indices: &[usize],
) -> Result<ClipboardPayload, ExAnimationClipboardError> {
    let payload = copy_exanimation_frames(controller, record, kind, indices)?;
    let len = controller
        .record_frames(record)
        .map_err(|error| {
            ExAnimationClipboardError::Controller(ExAnimationControllerError::Edit {
                command: 0,
                error,
            })
        })?
        .len();
    let mut indices = validate_selection(kind, indices, len)?;
    indices.reverse();
    controller
        .apply_frame_edits(
            record,
            indices
                .into_iter()
                .map(|index| ExAnimationFrameEdit::Remove { index })
                .collect(),
        )
        .map_err(ExAnimationClipboardError::Controller)?;
    Ok(payload)
}

/// Inserts clipboard frames before one frame index, preserving clipboard order.
///
/// # Errors
///
/// Returns [`ExAnimationClipboardError`] without mutation for malformed/wrong-domain clipboard
/// data, an invalid insertion point, destination width mismatch, or payload-capacity overflow.
pub fn paste_exanimation_frames(
    controller: &mut ExAnimationController,
    record: usize,
    before: usize,
    payload: &ClipboardPayload,
) -> Result<(), ExAnimationClipboardError> {
    let frames = payload
        .to_exanimation_frames()
        .map_err(ExAnimationClipboardError::Clipboard)?;
    let len = controller
        .record_frames(record)
        .map_err(|error| {
            ExAnimationClipboardError::Controller(ExAnimationControllerError::Edit {
                command: 0,
                error,
            })
        })?
        .len();
    if before > len {
        return Err(ExAnimationClipboardError::FrameOutOfRange { index: before, len });
    }
    controller
        .apply_frame_edits(
            record,
            frames
                .into_iter()
                .enumerate()
                .map(|(offset, frame)| ExAnimationFrameEdit::Insert {
                    index: before + offset,
                    frame,
                })
                .collect(),
        )
        .map_err(ExAnimationClipboardError::Controller)
}

fn validate_selection(
    kind: ClipboardKind,
    indices: &[usize],
    len: usize,
) -> Result<Vec<usize>, ExAnimationClipboardError> {
    if kind != ClipboardKind::ExAnimationFrames {
        return Err(ExAnimationClipboardError::WrongSelectionKind(kind));
    }
    if indices.is_empty() {
        return Err(ExAnimationClipboardError::EmptySelection);
    }
    let mut unique = BTreeSet::new();
    for index in indices {
        if *index >= len {
            return Err(ExAnimationClipboardError::FrameOutOfRange { index: *index, len });
        }
        if !unique.insert(*index) {
            return Err(ExAnimationClipboardError::DuplicateFrame(*index));
        }
    }
    Ok(unique.into_iter().collect())
}
