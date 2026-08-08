use super::{CustomObjectEditor, CustomObjectLibraryEdit, native_clipboard};
use lm_level::ObjectRecord;

impl CustomObjectEditor {
    pub(super) fn apply_edit(&mut self, edit: &CustomObjectLibraryEdit) {
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

    pub(super) fn current_object(&self) -> Option<&ObjectRecord> {
        Some(
            &self
                .controller
                .as_ref()?
                .library()
                .entries()
                .get(self.index)?
                .object,
        )
    }

    pub(super) fn paste_object(&mut self, text: &str) {
        match native_clipboard::decode_level_object(text).and_then(|object| {
            let mut entry = self
                .controller
                .as_ref()
                .and_then(|controller| controller.library().entries().get(self.index))
                .cloned()
                .ok_or_else(|| "no custom-object entry is selected".to_string())?;
            entry.object = object;
            Ok(entry)
        }) {
            Ok(entry) => self.apply_edit(&CustomObjectLibraryEdit::Replace {
                index: self.index,
                entry,
            }),
            Err(error) => self.error = Some(error),
        }
    }
}
