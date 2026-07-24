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

    pub(super) fn paste_tile(&mut self, text: &str) {
        match native_clipboard::decode_map16_tile(text) {
            Ok(tile) => self.apply_edit(&Map16DocumentEdit::ReplaceTiles {
                replacements: vec![(
                    Map16Address {
                        page: self.page,
                        tile: self.tile,
                    },
                    tile,
                )],
                resolution_limit: self.resolution_limit(),
            }),
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
}
