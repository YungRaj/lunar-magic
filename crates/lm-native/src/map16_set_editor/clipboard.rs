use super::Map16SetEditor;
use crate::native_clipboard;
use lm_app::Map16DocumentEdit;
use lm_level::{Map16Address, Map16Tile};

impl Map16SetEditor {
    pub(super) fn current_tile(&self) -> Option<Map16Tile> {
        self.document
            .as_ref()?
            .controller
            .value()
            .set
            .pages
            .get(self.page)?
            .tiles
            .get(self.tile)
            .copied()
    }

    #[cfg(test)]
    pub(super) fn paste_tile(&mut self, text: &str) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        self.paste_tile_at(
            text,
            document.controller.revision(),
            Map16Address {
                page: self.page,
                tile: self.tile,
            },
        );
    }

    pub(super) fn paste_tile_at(&mut self, text: &str, revision: u64, address: Map16Address) {
        match native_clipboard::decode_map16_tile(text) {
            Ok(tile) => {
                let resolution_limit = self.resolution_limit();
                let result = self
                    .document
                    .as_mut()
                    .ok_or_else(|| "no complete Map16 document".to_owned())
                    .and_then(|document| {
                        document
                            .controller
                            .apply_edits(
                                revision,
                                &[Map16DocumentEdit::ReplaceTiles {
                                    replacements: vec![(address, tile)],
                                    resolution_limit,
                                }],
                            )
                            .map_err(|error| error.to_string())
                    });
                match result {
                    Ok(()) => self.invalidate(),
                    Err(error) => self.error = Some(error),
                }
            }
            Err(error) => self.error = Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map16_set_editor::Document;
    use lm_app::Map16DocumentController;
    use lm_graphics::{GraphicsFile4bpp, GraphicsInterchangeFile, Palette, PaletteInterchangeFile};
    use lm_level::{Map16Page, Map16Set, Map16SetFile, Subtile};

    fn editor() -> Map16SetEditor {
        let file = Map16SetFile {
            set: Map16Set {
                pages: vec![
                    Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap(),
                ],
            },
        };
        Map16SetEditor {
            document: Some(Document {
                controller: Map16DocumentController::decode(
                    "set.lm16set".into(),
                    &file.encode().unwrap(),
                )
                .unwrap(),
                graphics: GraphicsInterchangeFile {
                    source_slot: 0,
                    graphics: GraphicsFile4bpp { tiles: Vec::new() },
                },
                palette: PaletteInterchangeFile {
                    source_palette: 0,
                    palette: Palette { colors: Vec::new() },
                },
            }),
            ..Map16SetEditor::default()
        }
    }

    #[test]
    fn typed_map16_paste_replaces_complete_set_tile_in_one_revision() {
        let mut editor = editor();
        editor.tile = 1;
        let replacement = Map16Tile {
            top_left: Subtile(1),
            top_right: Subtile(2),
            bottom_left: Subtile(3),
            bottom_right: Subtile(4),
            acts_like: 0x00cd,
        };

        editor.paste_tile(&native_clipboard::encode_map16_tile(replacement).unwrap());

        let document = editor.document.as_ref().unwrap();
        assert_eq!(document.controller.revision(), 1);
        assert_eq!(
            document.controller.value().set.pages[0].tiles[1],
            replacement
        );
        assert!(editor.loaded_key.is_none());
        assert!(editor.rendered_key.is_none());
    }

    #[test]
    fn wrong_clipboard_domain_preserves_complete_set_and_revision() {
        let mut editor = editor();
        let before = editor.document.as_ref().unwrap().controller.value().clone();
        let text = native_clipboard::encode_palette_color(lm_graphics::Bgr555(0x1234)).unwrap();

        editor.paste_tile(&text);

        let document = editor.document.as_ref().unwrap();
        assert_eq!(document.controller.revision(), 0);
        assert_eq!(document.controller.value(), &before);
        assert!(editor.error.is_some());
    }

    #[test]
    fn clipboard_delivery_uses_requested_address_and_rejects_a_stale_revision() {
        let mut editor = editor();
        let replacement = Map16Tile {
            top_left: Subtile(5),
            top_right: Subtile(6),
            bottom_left: Subtile(7),
            bottom_right: Subtile(8),
            acts_like: 0x0034,
        };
        let text = native_clipboard::encode_map16_tile(replacement).unwrap();
        editor.tile = 9;
        editor.paste_tile_at(&text, 0, Map16Address { page: 0, tile: 3 });
        assert_eq!(
            editor
                .document
                .as_ref()
                .unwrap()
                .controller
                .value()
                .set
                .pages[0]
                .tiles[3],
            replacement
        );
        assert_eq!(
            editor
                .document
                .as_ref()
                .unwrap()
                .controller
                .value()
                .set
                .pages[0]
                .tiles[9],
            Map16Tile::default()
        );

        editor.paste_tile_at(&text, 0, Map16Address { page: 0, tile: 4 });
        assert!(editor.error.is_some());
        assert_eq!(
            editor
                .document
                .as_ref()
                .unwrap()
                .controller
                .value()
                .set
                .pages[0]
                .tiles[4],
            Map16Tile::default()
        );
    }
}
