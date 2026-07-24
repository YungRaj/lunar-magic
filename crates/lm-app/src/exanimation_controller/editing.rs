use super::{ExAnimationControllerEdit, ExAnimationControllerEditFailure};
use lm_graphics::{
    CompactExAnimation, ExAnimationEditError, ExAnimationFrameEdit, ExAnimationRecord,
    edit_exanimation_frames,
};

pub(crate) fn apply_animation_edits(
    animation: &mut CompactExAnimation,
    edits: &[ExAnimationControllerEdit],
    maximum_records: usize,
    double_size_modes: &[bool; 256],
) -> Result<(), (usize, ExAnimationControllerEditFailure)> {
    for (command, edit) in edits.iter().enumerate() {
        let result: Result<(), ExAnimationControllerEditFailure> = match edit {
            ExAnimationControllerEdit::SetSetting(value) => {
                animation.setting = *value;
                Ok(())
            }
            ExAnimationControllerEdit::SetHeaderValue(value) => {
                animation.header_value = *value;
                Ok(())
            }
            ExAnimationControllerEdit::SetTrigger { trigger, value } => animation
                .set_trigger(*trigger, *value)
                .map_err(ExAnimationControllerEditFailure::Animation),
            ExAnimationControllerEdit::InsertRecord { index, record } => animation
                .insert_record(*index, record.clone(), maximum_records)
                .map_err(ExAnimationControllerEditFailure::Animation),
            ExAnimationControllerEdit::ReplaceRecord { index, record } => {
                replace_record(animation, *index, record.clone())
                    .map_err(ExAnimationControllerEditFailure::Animation)
            }
            ExAnimationControllerEdit::RemoveRecord { index } => animation
                .remove_record(*index)
                .map(drop)
                .map_err(ExAnimationControllerEditFailure::Animation),
            ExAnimationControllerEdit::MoveRecordBefore { from, before } => animation
                .move_record_before(*from, *before)
                .map_err(ExAnimationControllerEditFailure::Animation),
            ExAnimationControllerEdit::EditRecordFrames { record, edits } => {
                edit_record_frames(animation, *record, edits, double_size_modes)
            }
        };
        result.map_err(|error| (command, error))?;
        let encoded = animation
            .encode(double_size_modes)
            .map_err(ExAnimationControllerEditFailure::Encoding)
            .map_err(|error| (command, error))?;
        let (canonical, consumed) =
            CompactExAnimation::decode(&encoded, maximum_records, double_size_modes)
                .map_err(ExAnimationControllerEditFailure::Encoding)
                .map_err(|error| (command, error))?;
        if consumed != encoded.len() || canonical != *animation {
            return Err((
                command,
                ExAnimationControllerEditFailure::NonCanonicalEncoding,
            ));
        }
    }
    Ok(())
}

fn edit_record_frames(
    animation: &mut CompactExAnimation,
    index: usize,
    edits: &[ExAnimationFrameEdit],
    double_size_modes: &[bool; 256],
) -> Result<(), ExAnimationControllerEditFailure> {
    let len = animation.records.len();
    let record = animation.records.get(index).ok_or({
        ExAnimationControllerEditFailure::Animation(ExAnimationEditError::RecordIndexOutOfRange {
            index,
            len,
        })
    })?;
    let double_size = double_size_modes[usize::from(record.size_mode())];
    let edited = edit_exanimation_frames(record, double_size, edits).map_err(|error| {
        ExAnimationControllerEditFailure::Frames {
            record: index,
            error,
        }
    })?;
    animation.records[index] = edited;
    Ok(())
}

fn replace_record(
    animation: &mut CompactExAnimation,
    index: usize,
    record: ExAnimationRecord,
) -> Result<(), ExAnimationEditError> {
    let len = animation.records.len();
    let target = animation
        .records
        .get_mut(index)
        .ok_or(ExAnimationEditError::RecordIndexOutOfRange { index, len })?;
    *target = record;
    Ok(())
}
