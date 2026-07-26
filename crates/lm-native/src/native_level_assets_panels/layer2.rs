use super::{AggregatePanels, PasteTarget, index, pasted_text};
use crate::{level_editor_forms, native_clipboard};
use eframe::egui;
use lm_app::NativeLevelAssetsControllerEdit;
use lm_level::{NativeLayer2Data, ObjectEdit};

fn layer2_tilemap_word(bytes: &[u8], x: usize, y: usize) -> Option<(usize, u16)> {
    let index = lm_level::native_layer2_tilemap_index(x, y)?;
    let offset = index.checked_mul(2)?;
    let pair = bytes.get(offset..offset + 2)?;
    Some((index, u16::from_le_bytes([pair[0], pair[1]])))
}

fn layer2_selection_indices(
    anchor: Option<(usize, usize)>,
    cursor: Option<(usize, usize)>,
) -> Vec<usize> {
    let (Some((anchor_x, anchor_y)), Some((cursor_x, cursor_y))) = (anchor, cursor) else {
        return Vec::new();
    };
    let (minimum_x, maximum_x) = (anchor_x.min(cursor_x), anchor_x.max(cursor_x));
    let (minimum_y, maximum_y) = (anchor_y.min(cursor_y), anchor_y.max(cursor_y));
    (minimum_y..=maximum_y)
        .flat_map(|y| {
            (minimum_x..=maximum_x).filter_map(move |x| lm_level::native_layer2_tilemap_index(x, y))
        })
        .collect()
}

fn layer2_selection_contains(
    anchor: Option<(usize, usize)>,
    cursor: Option<(usize, usize)>,
    x: usize,
    y: usize,
) -> bool {
    let (Some((anchor_x, anchor_y)), Some((cursor_x, cursor_y))) = (anchor, cursor) else {
        return false;
    };
    (anchor_x.min(cursor_x)..=anchor_x.max(cursor_x)).contains(&x)
        && (anchor_y.min(cursor_y)..=anchor_y.max(cursor_y)).contains(&y)
}

fn layer2_word_edits(
    fallback_index: usize,
    anchor: Option<(usize, usize)>,
    cursor: Option<(usize, usize)>,
    word: u16,
) -> Vec<(usize, u16)> {
    let indexes = layer2_selection_indices(anchor, cursor);
    if indexes.is_empty() {
        vec![(fallback_index, word)]
    } else {
        indexes.into_iter().map(|index| (index, word)).collect()
    }
}

fn layer2_selection_words(
    bytes: &[u8],
    anchor: Option<(usize, usize)>,
    cursor: Option<(usize, usize)>,
) -> Result<(u8, u8, Vec<u16>), String> {
    let (Some((anchor_x, anchor_y)), Some((cursor_x, cursor_y))) = (anchor, cursor) else {
        return Err("select a Layer 2 canvas rectangle before copying".into());
    };
    let (minimum_x, maximum_x) = (anchor_x.min(cursor_x), anchor_x.max(cursor_x));
    let (minimum_y, maximum_y) = (anchor_y.min(cursor_y), anchor_y.max(cursor_y));
    let width = u8::try_from(maximum_x - minimum_x + 1)
        .map_err(|_| "Layer 2 selection width is out of range".to_string())?;
    let height = u8::try_from(maximum_y - minimum_y + 1)
        .map_err(|_| "Layer 2 selection height is out of range".to_string())?;
    let mut words = Vec::with_capacity(usize::from(width) * usize::from(height));
    for y in minimum_y..=maximum_y {
        for x in minimum_x..=maximum_x {
            let (_, word) = layer2_tilemap_word(bytes, x, y)
                .ok_or_else(|| "Layer 2 selection exceeds the tilemap".to_string())?;
            words.push(word);
        }
    }
    Ok((width, height, words))
}

fn layer2_paste_edits(
    origin: Option<(usize, usize)>,
    width: u8,
    height: u8,
    words: &[u16],
) -> Result<Vec<(usize, u16)>, String> {
    let (origin_x, origin_y) =
        origin.ok_or_else(|| "select a Layer 2 canvas destination before pasting".to_string())?;
    let end_x = origin_x
        .checked_add(usize::from(width))
        .ok_or_else(|| "Layer 2 paste width overflow".to_string())?;
    let end_y = origin_y
        .checked_add(usize::from(height))
        .ok_or_else(|| "Layer 2 paste height overflow".to_string())?;
    let expected = usize::from(width)
        .checked_mul(usize::from(height))
        .ok_or_else(|| "Layer 2 paste size overflow".to_string())?;
    if width == 0
        || height == 0
        || end_x > lm_level::NATIVE_LAYER2_TILEMAP_WIDTH
        || end_y > lm_level::NATIVE_LAYER2_TILEMAP_HEIGHT
        || words.len() != expected
    {
        return Err("Layer 2 paste rectangle does not fit the 32×32 canvas".into());
    }
    let mut edits = Vec::with_capacity(expected);
    for (offset, word) in words.iter().copied().enumerate() {
        let x = origin_x + offset % usize::from(width);
        let y = origin_y + offset / usize::from(width);
        let index = lm_level::native_layer2_tilemap_index(x, y)
            .ok_or_else(|| "Layer 2 paste coordinate is out of range".to_string())?;
        edits.push((index, word));
    }
    Ok(edits)
}

fn layer2_cut_edits(
    anchor: Option<(usize, usize)>,
    cursor: Option<(usize, usize)>,
) -> Result<Vec<(usize, u16)>, String> {
    let indexes = layer2_selection_indices(anchor, cursor);
    if indexes.is_empty() {
        return Err("select a Layer 2 canvas rectangle before cutting".into());
    }
    Ok(indexes.into_iter().map(|index| (index, 0)).collect())
}

fn layer2_flood_edits(
    bytes: &[u8],
    start: Option<(usize, usize)>,
    replacement: u16,
) -> Result<Vec<(usize, u16)>, String> {
    let (x, y) =
        start.ok_or_else(|| "select a Layer 2 canvas cell before flood filling".to_string())?;
    let indexes =
        lm_level::native_layer2_flood_region(bytes, x, y).map_err(|error| error.to_string())?;
    let map16_index = replacement & 0x0fff;
    Ok(indexes
        .into_iter()
        .map(|index| (index, map16_index))
        .collect())
}

impl AggregatePanels {
    pub(super) fn layer2_panel(
        &mut self,
        ui: &mut egui::Ui,
        layer2: &NativeLayer2Data,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        match layer2 {
            NativeLayer2Data::Objects(objects) => self.layer2_objects_panel(ui, objects),
            NativeLayer2Data::Tilemap(bytes) => self.layer2_tilemap_panel(ui, bytes),
        }
    }

    fn layer2_objects_panel(
        &mut self,
        ui: &mut egui::Ui,
        objects: &lm_level::LevelObjectData,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        ui.heading(format!(
            "Layer 2 objects ({})",
            objects.objects.records.len()
        ));
        index(
            ui,
            &mut self.layer2_object_index,
            objects.objects.records.len(),
        );
        ui.text_edit_singleline(&mut self.layer2_object);
        let mut action = None;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Load").clicked() {
                self.layer2_object = objects
                    .objects
                    .records
                    .get(self.layer2_object_index)
                    .map_or_else(String::new, |record| {
                        level_editor_forms::format_bytes(record.encoded())
                    });
            }
            for (label, value) in [("Insert", 0), ("Replace", 1), ("Remove", 2)] {
                if ui.button(label).clicked() {
                    action = Some(value);
                }
            }
            if ui
                .add_enabled(
                    self.layer2_object_index < objects.objects.records.len(),
                    egui::Button::new("Copy"),
                )
                .clicked()
            {
                let record = &objects.objects.records[self.layer2_object_index];
                match native_clipboard::encode_level_object(record) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => copy_error = Some(error),
                }
            }
            if ui.button("Paste").clicked() {
                self.paste_target = Some(PasteTarget::Layer2Object);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(error) = copy_error {
            return Some(Err(error));
        }
        if self.paste_target == Some(PasteTarget::Layer2Object)
            && let Some(text) = pasted_text(ui)
        {
            self.paste_target = None;
            return Some(native_clipboard::decode_level_object(&text).map(|record| {
                NativeLevelAssetsControllerEdit::Layer2Objects(vec![ObjectEdit::Replace {
                    index: self.layer2_object_index,
                    record,
                }])
            }));
        }
        action.map(|action| {
            let edit = match action {
                2 => Ok(ObjectEdit::Remove {
                    index: self.layer2_object_index,
                }),
                _ => level_editor_forms::parse_object(&self.layer2_object).map(|record| {
                    if action == 0 {
                        ObjectEdit::Insert {
                            index: self.layer2_object_index,
                            record,
                        }
                    } else {
                        ObjectEdit::Replace {
                            index: self.layer2_object_index,
                            record,
                        }
                    }
                }),
            };
            edit.map(|edit| NativeLevelAssetsControllerEdit::Layer2Objects(vec![edit]))
        })
    }

    fn layer2_tilemap_panel(
        &mut self,
        ui: &mut egui::Ui,
        bytes: &[u8],
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let words = bytes.len() / 2;
        ui.heading(format!("Layer 2 tilemap ({words} words)"));
        ui.label(
            "Click a Map16 cell, or Shift-click a second cell to select a rectangle. Applying fills \
             every selected cell with the complete 16-bit tile word.",
        );
        self.layer2_tilemap_grid(ui, bytes);
        let selected_cells =
            layer2_selection_indices(self.layer2_tile_anchor, self.layer2_tile_cursor);
        if let (Some(anchor), Some(cursor)) = (self.layer2_tile_anchor, self.layer2_tile_cursor) {
            ui.label(format!(
                "Canvas selection: ({}, {}) to ({}, {}) · {} cell{}",
                anchor.0,
                anchor.1,
                cursor.0,
                cursor.1,
                selected_cells.len(),
                if selected_cells.len() == 1 { "" } else { "s" }
            ));
        }
        ui.horizontal(|ui| {
            ui.label("Storage index");
            if ui
                .add(
                    egui::DragValue::new(&mut self.layer2_tile_index)
                        .range(0..=words.saturating_sub(1)),
                )
                .changed()
            {
                self.layer2_tile_anchor = None;
                self.layer2_tile_cursor = None;
            }
            if ui
                .add_enabled(
                    self.layer2_tile_anchor.is_some(),
                    egui::Button::new("Clear canvas selection"),
                )
                .clicked()
            {
                self.layer2_tile_anchor = None;
                self.layer2_tile_cursor = None;
            }
        });
        if let Some(edit) = self.layer2_clipboard_controls(ui, bytes) {
            return Some(edit);
        }
        ui.horizontal(|ui| {
            ui.label("16-bit tile word");
            ui.text_edit_singleline(&mut self.layer2_tile);
            if ui.button("Load").clicked()
                && let Some(bytes) =
                    bytes.get(self.layer2_tile_index * 2..self.layer2_tile_index * 2 + 2)
            {
                self.layer2_tile = format!("{:04X}", u16::from_le_bytes([bytes[0], bytes[1]]));
            }
        });
        let apply_label = if selected_cells.len() > 1 {
            format!("Fill {} selected cells", selected_cells.len())
        } else {
            "Apply tile".into()
        };
        let mut action = None;
        ui.horizontal(|ui| {
            if ui.button(apply_label).clicked() {
                action = Some(false);
            }
            if ui
                .add_enabled(
                    self.layer2_tile_cursor.is_some(),
                    egui::Button::new("Flood fill from cursor"),
                )
                .on_hover_text(
                    "Replace the four-connected region matching the cursor's complete 16-bit word. \
                     Lunar Magic normalizes the replacement to a 12-bit Map16 index.",
                )
                .clicked()
            {
                action = Some(true);
            }
        });
        action.map(|flood| {
            level_editor_forms::parse_hex_u16(&self.layer2_tile, "Layer 2 tile").and_then(|word| {
                let edits = if flood {
                    layer2_flood_edits(bytes, self.layer2_tile_cursor, word)
                } else {
                    Ok(layer2_word_edits(
                        self.layer2_tile_index,
                        self.layer2_tile_anchor,
                        self.layer2_tile_cursor,
                        word,
                    ))
                }?;
                Ok(NativeLevelAssetsControllerEdit::Layer2TilemapWords(edits))
            })
        })
    }

    fn layer2_clipboard_controls(
        &mut self,
        ui: &mut egui::Ui,
        bytes: &[u8],
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let mut copy_error = None;
        let mut cut_requested = false;
        ui.horizontal(|ui| {
            let can_copy = self.layer2_tile_anchor.is_some() && self.layer2_tile_cursor.is_some();
            for (label, cut) in [("Copy selection", false), ("Cut selection", true)] {
                if ui.add_enabled(can_copy, egui::Button::new(label)).clicked() {
                    let encoded = layer2_selection_words(
                        bytes,
                        self.layer2_tile_anchor,
                        self.layer2_tile_cursor,
                    )
                    .and_then(|(width, height, words)| {
                        native_clipboard::encode_layer2_tilemap_selection(width, height, &words)
                    });
                    match encoded {
                        Ok(text) => {
                            ui.ctx().copy_text(text);
                            cut_requested = cut;
                        }
                        Err(error) => copy_error = Some(error),
                    }
                }
            }
            if ui
                .add_enabled(
                    self.layer2_tile_anchor.is_some(),
                    egui::Button::new("Paste at anchor"),
                )
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Layer2Tilemap);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(error) = copy_error {
            return Some(Err(error));
        }
        if cut_requested {
            return Some(
                layer2_cut_edits(self.layer2_tile_anchor, self.layer2_tile_cursor)
                    .map(NativeLevelAssetsControllerEdit::Layer2TilemapWords),
            );
        }
        if self.paste_target != Some(PasteTarget::Layer2Tilemap) {
            return None;
        }
        let text = pasted_text(ui)?;
        self.paste_target = None;
        let result = (|| {
            let (width, height, words) = native_clipboard::decode_layer2_tilemap_selection(&text)?;
            let edits = layer2_paste_edits(self.layer2_tile_anchor, width, height, &words)?;
            let (anchor_x, anchor_y) = self
                .layer2_tile_anchor
                .ok_or_else(|| "Layer 2 paste destination disappeared".to_string())?;
            self.layer2_tile_cursor = Some((
                anchor_x + usize::from(width) - 1,
                anchor_y + usize::from(height) - 1,
            ));
            Ok(NativeLevelAssetsControllerEdit::Layer2TilemapWords(edits))
        })();
        Some(result)
    }

    fn layer2_tilemap_grid(&mut self, ui: &mut egui::Ui, bytes: &[u8]) {
        egui::ScrollArea::both()
            .id_salt("native-layer2-tilemap-grid")
            .max_height(360.0)
            .show(ui, |ui| {
                egui::Grid::new("native-layer2-tilemap-grid-cells")
                    .spacing([2.0, 2.0])
                    .show(ui, |ui| {
                        for y in 0..lm_level::NATIVE_LAYER2_TILEMAP_HEIGHT {
                            for x in 0..lm_level::NATIVE_LAYER2_TILEMAP_WIDTH {
                                let Some((index, word)) = layer2_tilemap_word(bytes, x, y) else {
                                    continue;
                                };
                                let response = ui.selectable_label(
                                    layer2_selection_contains(
                                        self.layer2_tile_anchor,
                                        self.layer2_tile_cursor,
                                        x,
                                        y,
                                    ) || (self.layer2_tile_anchor.is_none()
                                        && index == self.layer2_tile_index),
                                    format!("{:03X}", word & 0x0fff),
                                );
                                if response.clicked() {
                                    let extend = ui.input(|input| input.modifiers.shift);
                                    if !extend || self.layer2_tile_anchor.is_none() {
                                        self.layer2_tile_anchor = Some((x, y));
                                    }
                                    self.layer2_tile_cursor = Some((x, y));
                                    self.layer2_tile_index = index;
                                    self.layer2_tile = format!("{word:04X}");
                                }
                                response.on_hover_text(format!(
                                    "Canvas ({x}, {y}) · storage index ${index:03X} · word ${word:04X}"
                                ));
                            }
                            ui.end_row();
                        }
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilemap_grid_reads_lunar_magic_canvas_order() {
        let bytes = (0_u16..1024).flat_map(u16::to_le_bytes).collect::<Vec<_>>();
        assert_eq!(layer2_tilemap_word(&bytes, 0, 0), Some((0, 0)));
        assert_eq!(layer2_tilemap_word(&bytes, 1, 0), Some((16, 16)));
        assert_eq!(layer2_tilemap_word(&bytes, 31, 15), Some((511, 511)));
        assert_eq!(layer2_tilemap_word(&bytes, 0, 16), Some((512, 512)));
        assert_eq!(layer2_tilemap_word(&bytes, 31, 31), Some((1023, 1023)));
        assert_eq!(layer2_tilemap_word(&bytes, 32, 0), None);
        assert_eq!(layer2_tilemap_word(&bytes, 0, 32), None);
    }

    #[test]
    fn tilemap_grid_rejects_truncated_storage() {
        assert_eq!(layer2_tilemap_word(&[0x34], 0, 0), None);
        assert_eq!(layer2_tilemap_word(&[0x34, 0x12], 1, 0), None);
    }

    #[test]
    fn rectangle_selection_uses_canvas_coordinates_and_native_storage_indexes() {
        assert_eq!(
            layer2_selection_indices(Some((1, 15)), Some((2, 16))),
            vec![31, 47, 528, 544]
        );
        assert_eq!(
            layer2_selection_indices(Some((2, 16)), Some((1, 15))),
            vec![31, 47, 528, 544]
        );
        assert!(layer2_selection_contains(
            Some((2, 16)),
            Some((1, 15)),
            1,
            16
        ));
        assert!(!layer2_selection_contains(
            Some((2, 16)),
            Some((1, 15)),
            0,
            16
        ));
    }

    #[test]
    fn rectangle_fill_routes_unique_atomic_word_edits() {
        assert_eq!(
            layer2_word_edits(99, Some((0, 0)), Some((1, 1)), 0xbeef),
            vec![(0, 0xbeef), (16, 0xbeef), (1, 0xbeef), (17, 0xbeef)]
        );
        assert_eq!(
            layer2_word_edits(99, None, None, 0xbeef),
            vec![(99, 0xbeef)]
        );
    }

    #[test]
    fn rectangle_copy_is_visual_row_major_across_native_planes() {
        let bytes = (0_u16..1024).flat_map(u16::to_le_bytes).collect::<Vec<_>>();
        assert_eq!(
            layer2_selection_words(&bytes, Some((1, 15)), Some((2, 16))).unwrap(),
            (2, 2, vec![31, 47, 528, 544])
        );
        assert_eq!(
            layer2_selection_words(&bytes, Some((2, 16)), Some((1, 15))).unwrap(),
            (2, 2, vec![31, 47, 528, 544])
        );
        assert!(layer2_selection_words(&bytes, None, None).is_err());
    }

    #[test]
    fn rectangle_paste_translates_visual_words_to_native_indexes() {
        assert_eq!(
            layer2_paste_edits(Some((1, 15)), 2, 2, &[10, 11, 12, 13]).unwrap(),
            vec![(31, 10), (47, 11), (528, 12), (544, 13)]
        );
        assert!(layer2_paste_edits(Some((31, 31)), 2, 1, &[1, 2]).is_err());
        assert!(layer2_paste_edits(Some((0, 0)), 2, 2, &[1, 2, 3]).is_err());
        assert!(layer2_paste_edits(None, 1, 1, &[1]).is_err());
    }

    #[test]
    fn cut_uses_lunar_magics_proven_zero_word_for_every_selected_cell() {
        assert_eq!(
            layer2_cut_edits(Some((1, 15)), Some((2, 16))).unwrap(),
            vec![(31, 0), (47, 0), (528, 0), (544, 0)]
        );
        assert!(layer2_cut_edits(None, None).is_err());
    }

    #[test]
    fn flood_fill_matches_complete_words_and_normalizes_replacement_to_map16() {
        let mut words = vec![0_u16; 1024];
        for (x, y) in [(0, 0), (1, 0), (1, 1)] {
            words[lm_level::native_layer2_tilemap_index(x, y).unwrap()] = 0x8123;
        }
        words[lm_level::native_layer2_tilemap_index(2, 0).unwrap()] = 0x0123;
        let bytes = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        assert_eq!(
            layer2_flood_edits(&bytes, Some((0, 0)), 0xf456).unwrap(),
            vec![(0, 0x0456), (16, 0x0456), (17, 0x0456)]
        );
        assert!(layer2_flood_edits(&bytes, None, 1).is_err());
    }
}
