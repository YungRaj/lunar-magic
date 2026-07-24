use crate::MwlOptionalLevelAssets;
use lm_graphics::{
    Bgr555, CompactExAnimation, ExAnimationFrame, ExAnimationFrameEdit, ExAnimationFrameEditError,
    ExAnimationRecord, edit_exanimation_frames,
};
use std::fmt;

/// One semantic mutation of the typed palette or `ExAnimation` sections in an MWL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MwlOptionalAssetsEdit {
    SetPaletteMetadata([u32; 2]),
    SetPaletteColor {
        index: usize,
        color: Bgr555,
    },
    SetExAnimationMetadata([u32; 2]),
    CreateExAnimation,
    SetExAnimationGlobals {
        setting: u8,
        header_value: u32,
    },
    SetTrigger {
        index: usize,
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
    InsertFrame {
        record: usize,
        index: usize,
        frame: ExAnimationFrame,
    },
    ReplaceFrame {
        record: usize,
        index: usize,
        frame: ExAnimationFrame,
    },
    RemoveFrame {
        record: usize,
        index: usize,
    },
    MoveFrameBefore {
        record: usize,
        from: usize,
        before: usize,
    },
}

/// Applies one semantic mutation without encoding or committing its containing MWL.
///
/// # Errors
///
/// Rejects out-of-range palette, trigger, or record indexes and animation operations that do not
/// match the aggregate's present/absent state.
pub fn apply_mwl_optional_assets_edit(
    assets: &mut MwlOptionalLevelAssets,
    double_size_modes: &[bool],
    edit: &MwlOptionalAssetsEdit,
) -> Result<(), MwlOptionalAssetsEditError> {
    match edit {
        MwlOptionalAssetsEdit::SetPaletteMetadata(metadata) => {
            assets.palette_metadata = *metadata;
        }
        MwlOptionalAssetsEdit::SetPaletteColor { index, color } => {
            let actual = assets.palette.colors.len();
            let target = assets.palette.colors.get_mut(*index).ok_or(
                MwlOptionalAssetsEditError::PaletteIndex {
                    index: *index,
                    actual,
                },
            )?;
            *target = *color;
        }
        MwlOptionalAssetsEdit::SetExAnimationMetadata(metadata) => {
            assets.exanimation_metadata = *metadata;
        }
        MwlOptionalAssetsEdit::CreateExAnimation => {
            if assets.exanimation.is_some() {
                return Err(MwlOptionalAssetsEditError::ExAnimationAlreadyPresent);
            }
            assets.exanimation = Some(empty_animation());
        }
        MwlOptionalAssetsEdit::SetExAnimationGlobals {
            setting,
            header_value,
        } => {
            let animation = animation_mut(assets)?;
            animation.setting = *setting;
            animation.header_value = *header_value;
        }
        MwlOptionalAssetsEdit::SetTrigger { index, value } => {
            if *index >= 16 {
                return Err(MwlOptionalAssetsEditError::TriggerIndex(*index));
            }
            let animation = animation_mut(assets)?;
            let bit = 1_u16 << index;
            if let Some(value) = value {
                animation.trigger_mask |= bit;
                animation.trigger_values[*index] = *value;
            } else {
                animation.trigger_mask &= !bit;
            }
        }
        MwlOptionalAssetsEdit::InsertRecord { index, record } => {
            let animation = animation_mut(assets)?;
            if *index > animation.records.len() {
                return Err(record_index(*index, animation.records.len(), true));
            }
            animation.records.insert(*index, record.clone());
        }
        MwlOptionalAssetsEdit::ReplaceRecord { index, record } => {
            let animation = animation_mut(assets)?;
            let actual = animation.records.len();
            let target = animation
                .records
                .get_mut(*index)
                .ok_or_else(|| record_index(*index, actual, false))?;
            *target = record.clone();
        }
        MwlOptionalAssetsEdit::RemoveRecord { index } => {
            let animation = animation_mut(assets)?;
            if *index >= animation.records.len() {
                return Err(record_index(*index, animation.records.len(), false));
            }
            animation.records.remove(*index);
        }
        MwlOptionalAssetsEdit::InsertFrame { .. }
        | MwlOptionalAssetsEdit::ReplaceFrame { .. }
        | MwlOptionalAssetsEdit::RemoveFrame { .. }
        | MwlOptionalAssetsEdit::MoveFrameBefore { .. } => {
            apply_frame_edit(assets, double_size_modes, edit)?;
        }
    }
    Ok(())
}

fn apply_frame_edit(
    assets: &mut MwlOptionalLevelAssets,
    double_size_modes: &[bool],
    edit: &MwlOptionalAssetsEdit,
) -> Result<(), MwlOptionalAssetsEditError> {
    let (record, frame_edit) = match edit {
        MwlOptionalAssetsEdit::InsertFrame {
            record,
            index,
            frame,
        } => (
            *record,
            ExAnimationFrameEdit::Insert {
                index: *index,
                frame: frame.clone(),
            },
        ),
        MwlOptionalAssetsEdit::ReplaceFrame {
            record,
            index,
            frame,
        } => (
            *record,
            ExAnimationFrameEdit::Replace {
                index: *index,
                frame: frame.clone(),
            },
        ),
        MwlOptionalAssetsEdit::RemoveFrame { record, index } => {
            (*record, ExAnimationFrameEdit::Remove { index: *index })
        }
        MwlOptionalAssetsEdit::MoveFrameBefore {
            record,
            from,
            before,
        } => (
            *record,
            ExAnimationFrameEdit::MoveBefore {
                from: *from,
                before: *before,
            },
        ),
        _ => unreachable!("caller selects only frame edits"),
    };
    edit_record_frames(assets, double_size_modes, record, frame_edit)
}

fn edit_record_frames(
    assets: &mut MwlOptionalLevelAssets,
    double_size_modes: &[bool],
    target_record: usize,
    edit: ExAnimationFrameEdit,
) -> Result<(), MwlOptionalAssetsEditError> {
    let animation = animation_mut(assets)?;
    let actual = animation.records.len();
    let record = animation
        .records
        .get_mut(target_record)
        .ok_or_else(|| record_index(target_record, actual, false))?;
    let mode_index = usize::from(record.size_mode());
    let double_size =
        *double_size_modes
            .get(mode_index)
            .ok_or(MwlOptionalAssetsEditError::SizeModeIndex {
                record: target_record,
                index: mode_index,
                actual: double_size_modes.len(),
            })?;
    *record = edit_exanimation_frames(record, double_size, &[edit])
        .map_err(MwlOptionalAssetsEditError::Frame)?;
    Ok(())
}

fn empty_animation() -> CompactExAnimation {
    CompactExAnimation {
        setting: 0,
        header_value: 0,
        trigger_mask: 0,
        trigger_values: [0; 16],
        records: Vec::new(),
    }
}

fn animation_mut(
    assets: &mut MwlOptionalLevelAssets,
) -> Result<&mut CompactExAnimation, MwlOptionalAssetsEditError> {
    assets
        .exanimation
        .as_mut()
        .ok_or(MwlOptionalAssetsEditError::ExAnimationAbsent)
}

fn record_index(index: usize, actual: usize, insertion: bool) -> MwlOptionalAssetsEditError {
    MwlOptionalAssetsEditError::RecordIndex {
        index,
        actual,
        insertion,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MwlOptionalAssetsEditError {
    PaletteIndex {
        index: usize,
        actual: usize,
    },
    ExAnimationAbsent,
    ExAnimationAlreadyPresent,
    TriggerIndex(usize),
    RecordIndex {
        index: usize,
        actual: usize,
        insertion: bool,
    },
    SizeModeIndex {
        record: usize,
        index: usize,
        actual: usize,
    },
    Frame(ExAnimationFrameEditError),
}

impl fmt::Display for MwlOptionalAssetsEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "MWL optional-assets edit failed: {self:?}")
    }
}

impl std::error::Error for MwlOptionalAssetsEditError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::Palette;

    fn assets() -> MwlOptionalLevelAssets {
        MwlOptionalLevelAssets {
            palette_metadata: [0; 2],
            palette: Palette {
                colors: vec![Bgr555(0); 257],
            },
            exanimation_metadata: [0; 2],
            exanimation: None,
        }
    }

    #[test]
    fn edits_palette_and_complete_animation_collection() {
        let mut value = assets();
        apply_mwl_optional_assets_edit(
            &mut value,
            &[false; 256],
            &MwlOptionalAssetsEdit::SetPaletteColor {
                index: 256,
                color: Bgr555(0x1234),
            },
        )
        .unwrap();
        apply_mwl_optional_assets_edit(
            &mut value,
            &[false; 256],
            &MwlOptionalAssetsEdit::CreateExAnimation,
        )
        .unwrap();
        apply_mwl_optional_assets_edit(
            &mut value,
            &[false; 256],
            &MwlOptionalAssetsEdit::SetTrigger {
                index: 3,
                value: Some(7),
            },
        )
        .unwrap();
        assert_eq!(value.palette.colors[256], Bgr555(0x1234));
        let animation = value.exanimation.unwrap();
        assert_eq!(animation.trigger_mask, 1 << 3);
        assert_eq!(animation.trigger_values[3], 7);
    }

    #[test]
    fn invalid_late_targets_are_structured() {
        let mut value = assets();
        assert!(matches!(
            apply_mwl_optional_assets_edit(
                &mut value,
                &[false; 256],
                &MwlOptionalAssetsEdit::SetPaletteColor {
                    index: 257,
                    color: Bgr555(0),
                }
            ),
            Err(MwlOptionalAssetsEditError::PaletteIndex { .. })
        ));
        assert_eq!(
            apply_mwl_optional_assets_edit(
                &mut value,
                &[false; 256],
                &MwlOptionalAssetsEdit::CreateExAnimation
            ),
            Ok(())
        );
        assert_eq!(
            apply_mwl_optional_assets_edit(
                &mut value,
                &[false; 256],
                &MwlOptionalAssetsEdit::CreateExAnimation
            ),
            Err(MwlOptionalAssetsEditError::ExAnimationAlreadyPresent)
        );
    }

    #[test]
    fn frame_edits_follow_the_revision_size_mode() {
        let mut value = assets();
        value.exanimation = Some(CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![
                ExAnimationRecord::new(1, 0, 7, 0x100, false, &[0, 6, 1, 6], true).unwrap(),
            ],
        });
        let mut modes = [false; 256];
        modes[7] = true;
        apply_mwl_optional_assets_edit(
            &mut value,
            &modes,
            &MwlOptionalAssetsEdit::ReplaceFrame {
                record: 0,
                index: 0,
                frame: ExAnimationFrame {
                    source_words: vec![0x1234, 0x5678],
                },
            },
        )
        .unwrap();
        assert_eq!(
            value.exanimation.unwrap().records[0].frame_bytes(true),
            [0x34, 0x12, 0x78, 0x56]
        );
    }

    #[test]
    fn missing_modes_and_wrong_frame_width_preserve_the_record() {
        let mut value = assets();
        let record = ExAnimationRecord::new(1, 0, 7, 0x100, false, &[0, 6, 1, 6], true).unwrap();
        value.exanimation = Some(CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![record.clone()],
        });
        let edit = MwlOptionalAssetsEdit::ReplaceFrame {
            record: 0,
            index: 0,
            frame: ExAnimationFrame {
                source_words: vec![0x1234],
            },
        };
        assert!(matches!(
            apply_mwl_optional_assets_edit(&mut value, &[false; 7], &edit),
            Err(MwlOptionalAssetsEditError::SizeModeIndex { .. })
        ));
        let mut modes = [false; 256];
        modes[7] = true;
        assert!(matches!(
            apply_mwl_optional_assets_edit(&mut value, &modes, &edit),
            Err(MwlOptionalAssetsEditError::Frame(
                ExAnimationFrameEditError::WrongWordCount { .. }
            ))
        ));
        assert_eq!(value.exanimation.as_ref().unwrap().records[0], record);
    }
}
