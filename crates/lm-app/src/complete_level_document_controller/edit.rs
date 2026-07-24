use lm_level::{
    CompleteLevelFile, Layer3Edit, Layer3EditError, LevelAuxiliaryEdit, LevelAuxiliaryEditError,
    LevelLayer, LevelPropertyEdit, LevelPropertyEditError, ObjectEdit, ObjectEditError, SpriteEdit,
    SpriteEditError, SpriteEditLimits,
};

/// One cross-domain mutation owned by a complete portable level document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompleteLevelDocumentEdit {
    Property(LevelPropertyEdit),
    LayerObject { layer: LevelLayer, edit: ObjectEdit },
    Sprite(SpriteEdit),
    Layer3(Layer3Edit),
    Auxiliary(LevelAuxiliaryEdit),
}

#[derive(Debug)]
pub enum CompleteLevelDocumentEditError {
    Property(LevelPropertyEditError),
    Object(ObjectEditError),
    Sprite(SpriteEditError),
    Layer3(Layer3EditError),
    Auxiliary(LevelAuxiliaryEditError),
}

pub(super) fn apply_edit(
    staged: &mut CompleteLevelFile,
    edit: &CompleteLevelDocumentEdit,
) -> Result<(), CompleteLevelDocumentEditError> {
    match edit {
        CompleteLevelDocumentEdit::Property(edit) => staged
            .0
            .apply_property_edits(std::slice::from_ref(edit))
            .map_err(CompleteLevelDocumentEditError::Property),
        CompleteLevelDocumentEdit::LayerObject { layer, edit } => {
            let objects = match layer {
                LevelLayer::Layer1 => &mut staged.0.layer1.objects,
                LevelLayer::Layer2 => &mut staged.0.layer2.objects,
            };
            objects
                .apply_edits(std::slice::from_ref(edit))
                .map_err(CompleteLevelDocumentEditError::Object)
        }
        CompleteLevelDocumentEdit::Sprite(edit) => staged
            .0
            .sprites
            .apply_edits(std::slice::from_ref(edit), SpriteEditLimits::PORTABLE)
            .map_err(CompleteLevelDocumentEditError::Sprite),
        CompleteLevelDocumentEdit::Layer3(edit) => staged
            .0
            .apply_layer3_edits(std::slice::from_ref(edit))
            .map_err(CompleteLevelDocumentEditError::Layer3),
        CompleteLevelDocumentEdit::Auxiliary(edit) => staged
            .0
            .apply_auxiliary_edits(std::slice::from_ref(edit))
            .map_err(CompleteLevelDocumentEditError::Auxiliary),
    }
}
