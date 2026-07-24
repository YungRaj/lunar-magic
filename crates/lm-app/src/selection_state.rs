use crate::{AppError, AppState, ClipboardKind, ClipboardPayload, EditorMode};

impl AppState {
    pub(crate) fn require_kind_for_mode(&self, kind: ClipboardKind) -> Result<(), AppError> {
        let accepted = matches!(
            (self.mode, kind),
            (
                EditorMode::Level(_),
                ClipboardKind::LevelObjects | ClipboardKind::LevelSprites
            ) | (
                EditorMode::Overworld,
                ClipboardKind::OverworldMessages | ClipboardKind::OverworldSprites
            ) | (EditorMode::Map16, ClipboardKind::Map16Tiles)
                | (EditorMode::Graphics(_), ClipboardKind::GraphicsTiles)
                | (EditorMode::Palette(_), ClipboardKind::PaletteColors)
                | (
                    EditorMode::ExAnimation(_),
                    ClipboardKind::ExAnimationRecords | ClipboardKind::ExAnimationFrames
                )
                | (
                    EditorMode::Layer3(_),
                    ClipboardKind::Layer3TilemapBytes | ClipboardKind::Layer3RemapBytes
                )
        );
        if accepted {
            Ok(())
        } else {
            Err(AppError::SelectionWrongMode {
                mode: self.mode,
                kind,
            })
        }
    }

    pub(crate) fn validate_selection_payload(
        &self,
        payload: &ClipboardPayload,
    ) -> Result<(), AppError> {
        self.project.as_ref().ok_or(AppError::NoProject)?;
        self.require_kind_for_mode(payload.kind)?;
        let selection = self
            .selection
            .as_ref()
            .ok_or(AppError::ClipboardSelectionMismatch {
                selected: 0,
                records: payload.records().len(),
            })?;
        if selection.kind != payload.kind || selection.indices().len() != payload.records().len() {
            return Err(AppError::ClipboardSelectionMismatch {
                selected: selection.indices().len(),
                records: payload.records().len(),
            });
        }
        Ok(())
    }
}
