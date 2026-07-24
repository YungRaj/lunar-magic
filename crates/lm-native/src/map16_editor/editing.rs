use super::{Map16Editor, map16_subtile_form, native_clipboard};
use lm_app::Map16PageDocumentEdit;

impl Map16Editor {
    pub(super) fn apply_subtile(&mut self) {
        let result = self.subtile.parse().and_then(|value| {
            let document = self.document.as_mut().ok_or("no Map16 document")?;
            document
                .controller
                .apply_edits(
                    document.controller.revision(),
                    &[Map16PageDocumentEdit::SetSubtile {
                        tile: self.selected_tile,
                        quadrant: map16_subtile_form::quadrant(self.quadrant),
                        value,
                    }],
                )
                .map_err(|error| error.to_string())
        });
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    pub(super) fn apply_acts_like(&mut self) {
        let result = u16::from_str_radix(self.acts_like.trim(), 16)
            .map_err(|error| format!("invalid Acts Like value: {error}"))
            .and_then(|value| {
                let document = self.document.as_mut().ok_or("no Map16 document")?;
                document
                    .controller
                    .apply_edits(
                        document.controller.revision(),
                        &[Map16PageDocumentEdit::SetActsLike {
                            tile: self.selected_tile,
                            value,
                        }],
                    )
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    pub(super) fn paste_tile(&mut self, text: &str) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        match native_clipboard::decode_map16_tile(text) {
            Ok(value) => {
                let edit = Map16PageDocumentEdit::ReplaceTile {
                    tile: self.selected_tile,
                    value,
                };
                match document
                    .controller
                    .apply_edits(document.controller.revision(), &[edit])
                {
                    Ok(()) => {
                        self.loaded_selection = None;
                        self.rendered_revision = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            Err(error) => self.error = Some(error),
        }
    }
}
