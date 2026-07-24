use super::{CustomSpriteEditor, CustomSpriteLibraryEdit, native_clipboard};
use lm_level::{CustomSpriteEntry, SpriteRecord};

impl CustomSpriteEditor {
    pub(super) fn apply_edit(&mut self, edit: &CustomSpriteLibraryEdit) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if let Err(error) =
            controller.apply_edits(controller.revision(), std::slice::from_ref(edit))
        {
            self.error = Some(error.to_string());
        } else {
            self.invalidate();
        }
    }

    pub(super) fn current_sprites(&self) -> Option<&[SpriteRecord]> {
        Some(
            &self
                .controller
                .as_ref()?
                .library()
                .entries()
                .get(self.index)?
                .sprites,
        )
    }

    pub(super) fn paste_placement(&mut self, text: &str) {
        match native_clipboard::decode_level_sprites(text).and_then(|sprites| {
            CustomSpriteEntry::new(sprites, self.form.description.clone())
                .map_err(|error| error.to_string())
        }) {
            Ok(entry) => self.apply_edit(&CustomSpriteLibraryEdit::Replace {
                index: self.index,
                entry,
            }),
            Err(error) => self.error = Some(error),
        }
    }
}
