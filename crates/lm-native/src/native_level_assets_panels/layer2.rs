use super::level::object_semantic_fields;
use super::{
    AggregatePanels, Layer2FillPattern, PasteTarget, PendingSelectionMove, index,
    move_before_indexes, pasted_text,
};
use crate::{level_editor_forms, native_clipboard};
use eframe::egui;
use lm_app::{LocalizationCatalog, NativeLevelAssetsControllerEdit};
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

fn select_layer2_tile_cell(
    panels: &mut AggregatePanels,
    x: usize,
    y: usize,
    index: usize,
    word: u16,
    extend: bool,
) {
    if !extend || panels.layer2_tile_anchor.is_none() {
        panels.layer2_tile_anchor = Some((x, y));
    }
    panels.layer2_tile_cursor = Some((x, y));
    panels.layer2_tile_index = index;
    panels.layer2_tile = format!("{word:04X}");
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

fn layer2_pattern_flood_edits(
    bytes: &[u8],
    start: Option<(usize, usize)>,
    pattern: &Layer2FillPattern,
) -> Result<Vec<(usize, u16)>, String> {
    let (x, y) =
        start.ok_or_else(|| "select a Layer 2 canvas cell before flood filling".to_string())?;
    lm_level::native_layer2_flood_pattern(
        bytes,
        x,
        y,
        usize::from(pattern.width),
        usize::from(pattern.height),
        &pattern.words,
    )
    .map_err(|error| error.to_string())
}

type Layer2SelectionEdit = (Vec<(usize, u16)>, (usize, usize), (usize, usize));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Layer2ResizeEdge {
    Left,
    Right,
    Top,
    Bottom,
}

fn layer2_move_edits(
    bytes: &[u8],
    anchor: Option<(usize, usize)>,
    cursor: Option<(usize, usize)>,
    delta_x: i32,
    delta_y: i32,
) -> Result<Layer2SelectionEdit, String> {
    let (Some((anchor_x, anchor_y)), Some((cursor_x, cursor_y))) = (anchor, cursor) else {
        return Err("select a Layer 2 canvas rectangle before moving".into());
    };
    let minimum_x = anchor_x.min(cursor_x);
    let minimum_y = anchor_y.min(cursor_y);
    let width = anchor_x.max(cursor_x) - minimum_x + 1;
    let height = anchor_y.max(cursor_y) - minimum_y + 1;
    let edits = lm_level::native_layer2_move_rectangle(
        bytes, minimum_x, minimum_y, width, height, delta_x, delta_y,
    )
    .map_err(|error| error.to_string())?;
    let shifted = |coordinate: (usize, usize)| -> Result<(usize, usize), String> {
        let x = i64::try_from(coordinate.0)
            .ok()
            .and_then(|value| value.checked_add(i64::from(delta_x)))
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| "Layer 2 horizontal move exceeds the canvas".to_string())?;
        let y = i64::try_from(coordinate.1)
            .ok()
            .and_then(|value| value.checked_add(i64::from(delta_y)))
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| "Layer 2 vertical move exceeds the canvas".to_string())?;
        Ok((x, y))
    };
    Ok((
        edits,
        shifted((anchor_x, anchor_y))?,
        shifted((cursor_x, cursor_y))?,
    ))
}

fn layer2_resize_edits(
    bytes: &[u8],
    anchor: Option<(usize, usize)>,
    cursor: Option<(usize, usize)>,
    edge: Layer2ResizeEdge,
    grow: bool,
) -> Result<Layer2SelectionEdit, String> {
    let (Some((anchor_x, anchor_y)), Some((cursor_x, cursor_y))) = (anchor, cursor) else {
        return Err("select a Layer 2 canvas rectangle before resizing".into());
    };
    let minimum_x = anchor_x.min(cursor_x);
    let maximum_x = anchor_x.max(cursor_x);
    let minimum_y = anchor_y.min(cursor_y);
    let maximum_y = anchor_y.max(cursor_y);
    let source = lm_level::NativeLayer2Rectangle {
        x: minimum_x,
        y: minimum_y,
        width: maximum_x - minimum_x + 1,
        height: maximum_y - minimum_y + 1,
    };
    let mut resized = source;
    match (edge, grow) {
        (Layer2ResizeEdge::Left, true) if resized.x > 0 => {
            resized.x -= 1;
            resized.width += 1;
        }
        (Layer2ResizeEdge::Left, false) if resized.width > 1 => {
            resized.x += 1;
            resized.width -= 1;
        }
        (Layer2ResizeEdge::Right, true)
            if resized.x + resized.width < lm_level::NATIVE_LAYER2_TILEMAP_WIDTH =>
        {
            resized.width += 1;
        }
        (Layer2ResizeEdge::Right, false) if resized.width > 1 => resized.width -= 1,
        (Layer2ResizeEdge::Top, true) if resized.y > 0 => {
            resized.y -= 1;
            resized.height += 1;
        }
        (Layer2ResizeEdge::Top, false) if resized.height > 1 => {
            resized.y += 1;
            resized.height -= 1;
        }
        (Layer2ResizeEdge::Bottom, true)
            if resized.y + resized.height < lm_level::NATIVE_LAYER2_TILEMAP_HEIGHT =>
        {
            resized.height += 1;
        }
        (Layer2ResizeEdge::Bottom, false) if resized.height > 1 => resized.height -= 1,
        _ => return Err("Layer 2 selection cannot resize past the canvas or below 1×1".into()),
    }
    let edits = lm_level::native_layer2_resize_rectangle(bytes, source, resized)
        .map_err(|error| error.to_string())?;
    let horizontal_forward = anchor_x <= cursor_x;
    let vertical_forward = anchor_y <= cursor_y;
    let new_minimum = (resized.x, resized.y);
    let new_maximum = (
        resized.x + resized.width - 1,
        resized.y + resized.height - 1,
    );
    let anchor = (
        if horizontal_forward {
            new_minimum.0
        } else {
            new_maximum.0
        },
        if vertical_forward {
            new_minimum.1
        } else {
            new_maximum.1
        },
    );
    let cursor = (
        if horizontal_forward {
            new_maximum.0
        } else {
            new_minimum.0
        },
        if vertical_forward {
            new_maximum.1
        } else {
            new_minimum.1
        },
    );
    Ok((edits, anchor, cursor))
}

impl AggregatePanels {
    pub(super) fn layer2_panel(
        &mut self,
        ui: &mut egui::Ui,
        layer2: &NativeLayer2Data,
        descriptor: Option<lm_level::MwlLayer2Descriptor>,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        match layer2 {
            NativeLayer2Data::Objects(objects) => self.layer2_objects_panel(ui, objects, catalog),
            NativeLayer2Data::Tilemap(bytes) => self.layer2_tilemap_panel(ui, bytes, descriptor),
        }
    }

    fn layer2_objects_panel(
        &mut self,
        ui: &mut egui::Ui,
        objects: &lm_level::LevelObjectData,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        ui.heading(format!(
            "Layer 2 objects ({})",
            objects.objects.records.len()
        ));
        index(
            ui,
            &mut self.layer2_object_index,
            objects.objects.records.len(),
            catalog,
        );
        self.sync_layer2_object_form(objects, false);
        ui.text_edit_singleline(&mut self.layer2_record.object);
        object_semantic_fields(ui, &mut self.layer2_record, catalog);
        let mut action = None;
        let mut apply_object_fields = false;
        let mut move_object = None;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Load").clicked() {
                self.sync_layer2_object_form(objects, true);
            }
            for (label, value) in [("Insert", 0), ("Replace", 1), ("Remove", 2)] {
                if ui.button(label).clicked() {
                    action = Some(value);
                }
            }
            if ui
                .add_enabled(
                    self.layer2_record.object_fields_loaded
                        && self.layer2_object_index < objects.objects.records.len(),
                    egui::Button::new("Apply object fields"),
                )
                .clicked()
            {
                apply_object_fields = true;
            }
            if ui
                .add_enabled(self.layer2_object_index > 0, egui::Button::new("Move up"))
                .clicked()
            {
                move_object = move_before_indexes(
                    self.layer2_object_index,
                    objects.objects.records.len(),
                    false,
                );
            }
            if ui
                .add_enabled(
                    self.layer2_object_index.saturating_add(1) < objects.objects.records.len(),
                    egui::Button::new("Move down"),
                )
                .clicked()
            {
                move_object = move_before_indexes(
                    self.layer2_object_index,
                    objects.objects.records.len(),
                    true,
                );
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
        if apply_object_fields {
            let edit = self
                .layer2_record
                .object_field_edit(self.layer2_object_index);
            if let Ok(lm_app::NativeLevelEdit::Objects(edits)) = &edit
                && let [ObjectEdit::SetOrdinaryFields { index, fields }] = edits.as_slice()
            {
                let mut predicted = objects.objects.clone();
                match predicted.set_ordinary_fields(*index, *fields) {
                    Ok(selected) => {
                        self.pending_selection_move =
                            Some(PendingSelectionMove::Layer2Object(selected));
                    }
                    Err(error) => return Some(Err(error.to_string())),
                }
            }
            return Some(edit.and_then(|edit| match edit {
                lm_app::NativeLevelEdit::Objects(edits) => {
                    Ok(NativeLevelAssetsControllerEdit::Layer2Objects(edits))
                }
                _ => Err("Layer 2 semantic form produced a non-object edit".into()),
            }));
        }
        if let Some((before, selected)) = move_object {
            self.pending_selection_move = Some(PendingSelectionMove::Layer2Object(selected));
            return Some(Ok(NativeLevelAssetsControllerEdit::Layer2Objects(vec![
                ObjectEdit::MoveBefore {
                    from: self.layer2_object_index,
                    before,
                },
            ])));
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
                _ => level_editor_forms::parse_object(&self.layer2_record.object).map(|record| {
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
        descriptor: Option<lm_level::MwlLayer2Descriptor>,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let words = bytes.len() / 2;
        ui.heading(format!("Layer 2 tilemap ({words} words)"));
        if let Some(descriptor) = descriptor {
            ui.label(format!(
                "Installed descriptor ${:02X} · active Map16 bank ${:X}",
                descriptor.raw(),
                descriptor.active_bank()
            ));
        } else {
            ui.label("Pristine/legacy descriptor · active Map16 bank $0");
        }
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
        if let Some(edit) = self.layer2_move_controls(ui, bytes) {
            return Some(edit);
        }
        if let Some(edit) = self.layer2_resize_controls(ui, bytes) {
            return Some(edit);
        }
        if let Some(edit) = self.layer2_word_controls(ui, bytes, selected_cells.len()) {
            return Some(edit);
        }
        if let Some(edit) = self.layer2_remap_controls(ui) {
            return Some(edit);
        }
        self.layer2_pattern_flood_controls(ui, bytes)
    }

    fn layer2_remap_controls(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        ui.separator();
        ui.collapsing("Remap Map16 tiles", |ui| {
            ui.label(
                "Enter Lunar Magic source,destination pairs using displayed $8000–$FFFF values. \
                 Ranges and the +, −, M, and R prefixes are supported.",
            );
            ui.add(
                egui::TextEdit::multiline(&mut self.layer2_remap_script)
                    .desired_rows(4)
                    .code_editor(),
            );
            ui.horizontal(|ui| {
                ui.label("Global offset");
                ui.add(
                    egui::DragValue::new(&mut self.layer2_remap_offset)
                        .range(-0x7fff..=0x7fff)
                        .hexadecimal(4, true, true),
                );
                ui.checkbox(
                    &mut self.layer2_remap_selection_only,
                    "Selected rectangle only",
                );
            });
            let has_selection =
                self.layer2_tile_anchor.is_some() && self.layer2_tile_cursor.is_some();
            let enabled = !self.layer2_remap_selection_only || has_selection;
            ui.add_enabled(enabled, egui::Button::new("Apply remap"))
                .on_hover_text(
                    "Apply the complete program as one undoable edit. Cross-bank mappings persist \
                     when this ROM profile supplies Lunar Magic's installed descriptor table; \
                     pristine/legacy layouts reject them before mutation.",
                )
                .clicked()
        })
        .body_returned
        .filter(|clicked| *clicked)
        .map(|_| {
            let selection = self.layer2_remap_selection_only.then(|| {
                layer2_selection_indices(self.layer2_tile_anchor, self.layer2_tile_cursor)
            });
            if selection.as_ref().is_some_and(Vec::is_empty) {
                return Err("select a Layer 2 rectangle before applying a scoped remap".into());
            }
            Ok(NativeLevelAssetsControllerEdit::Layer2TilemapRemap {
                script: self.layer2_remap_script.clone(),
                global_offset: self.layer2_remap_offset,
                selection,
            })
        })
    }

    fn layer2_word_controls(
        &mut self,
        ui: &mut egui::Ui,
        bytes: &[u8],
        selected_cells: usize,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
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
        let apply_label = if selected_cells > 1 {
            format!("Fill {selected_cells} selected cells")
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

    fn layer2_move_controls(
        &mut self,
        ui: &mut egui::Ui,
        bytes: &[u8],
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let enabled = self.layer2_tile_anchor.is_some() && self.layer2_tile_cursor.is_some();
        let mut requested = None;
        ui.horizontal(|ui| {
            ui.label("Move selection");
            for (label, delta_x, delta_y) in [("←", -1, 0), ("↑", 0, -1), ("↓", 0, 1), ("→", 1, 0)]
            {
                if ui
                    .add_enabled(enabled, egui::Button::new(label))
                    .on_hover_text(
                        "Move the complete rectangle by one Map16 cell as one undoable edit.",
                    )
                    .clicked()
                {
                    requested = Some((delta_x, delta_y));
                }
            }
        });
        requested.map(|(delta_x, delta_y)| {
            let (edits, anchor, cursor) = layer2_move_edits(
                bytes,
                self.layer2_tile_anchor,
                self.layer2_tile_cursor,
                delta_x,
                delta_y,
            )?;
            self.layer2_tile_anchor = Some(anchor);
            self.layer2_tile_cursor = Some(cursor);
            self.layer2_tile_index = lm_level::native_layer2_tilemap_index(cursor.0, cursor.1)
                .ok_or_else(|| "moved Layer 2 cursor exceeds the canvas".to_string())?;
            Ok(NativeLevelAssetsControllerEdit::Layer2TilemapWords(edits))
        })
    }

    fn layer2_resize_controls(
        &mut self,
        ui: &mut egui::Ui,
        bytes: &[u8],
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let enabled = self.layer2_tile_anchor.is_some() && self.layer2_tile_cursor.is_some();
        let mut requested = None;
        ui.horizontal(|ui| {
            ui.label("Resize selection");
            for (label, edge, grow) in [
                ("L+", Layer2ResizeEdge::Left, true),
                ("L−", Layer2ResizeEdge::Left, false),
                ("R−", Layer2ResizeEdge::Right, false),
                ("R+", Layer2ResizeEdge::Right, true),
                ("T+", Layer2ResizeEdge::Top, true),
                ("T−", Layer2ResizeEdge::Top, false),
                ("B−", Layer2ResizeEdge::Bottom, false),
                ("B+", Layer2ResizeEdge::Bottom, true),
            ] {
                if ui
                    .add_enabled(enabled, egui::Button::new(label))
                    .on_hover_text(
                        "Grow (+) or shrink (−) this edge by one cell, repeating the original \
                         selection pattern from the resized top-left corner.",
                    )
                    .clicked()
                {
                    requested = Some((edge, grow));
                }
            }
        });
        requested.map(|(edge, grow)| {
            let (edits, anchor, cursor) = layer2_resize_edits(
                bytes,
                self.layer2_tile_anchor,
                self.layer2_tile_cursor,
                edge,
                grow,
            )?;
            self.layer2_tile_anchor = Some(anchor);
            self.layer2_tile_cursor = Some(cursor);
            self.layer2_tile_index = lm_level::native_layer2_tilemap_index(cursor.0, cursor.1)
                .ok_or_else(|| "resized Layer 2 cursor exceeds the canvas".to_string())?;
            Ok(NativeLevelAssetsControllerEdit::Layer2TilemapWords(edits))
        })
    }

    fn layer2_pattern_flood_controls(
        &mut self,
        ui: &mut egui::Ui,
        bytes: &[u8],
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let can_capture = self.layer2_tile_anchor.is_some() && self.layer2_tile_cursor.is_some();
        let mut capture_error = None;
        let mut apply = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_capture, egui::Button::new("Capture fill pattern"))
                .on_hover_text(
                    "Retain the selected rectangle as a visual row-major Map16 pattern. Then click \
                     any destination cell and apply it to that connected region.",
                )
                .clicked()
            {
                match layer2_selection_words(
                    bytes,
                    self.layer2_tile_anchor,
                    self.layer2_tile_cursor,
                ) {
                    Ok((width, height, words)) => {
                        self.layer2_fill_pattern = Some(Layer2FillPattern {
                            width,
                            height,
                            words,
                        });
                    }
                    Err(error) => capture_error = Some(error),
                }
            }
            let label = self.layer2_fill_pattern.as_ref().map_or_else(
                || "Flood fill with captured pattern".into(),
                |pattern| {
                    format!(
                        "Flood fill with {}×{} pattern",
                        pattern.width, pattern.height
                    )
                },
            );
            if ui
                .add_enabled(
                    self.layer2_fill_pattern.is_some() && self.layer2_tile_cursor.is_some(),
                    egui::Button::new(label),
                )
                .on_hover_text(
                    "Repeat the captured rectangle from the connected region's minimum X/Y corner, \
                     matching Lunar Magic's pattern anchoring.",
                )
                .clicked()
            {
                apply = true;
            }
        });
        if let Some(error) = capture_error {
            return Some(Err(error));
        }
        apply.then(|| {
            layer2_pattern_flood_edits(
                bytes,
                self.layer2_tile_cursor,
                self.layer2_fill_pattern
                    .as_ref()
                    .expect("button requires a captured pattern"),
            )
            .map(NativeLevelAssetsControllerEdit::Layer2TilemapWords)
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
                                    select_layer2_tile_cell(self, x, y, index, word, extend);
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
        assert_eq!(layer2_tilemap_word(&bytes, 1, 0), Some((1, 1)));
        assert_eq!(layer2_tilemap_word(&bytes, 31, 15), Some((767, 767)));
        assert_eq!(layer2_tilemap_word(&bytes, 0, 16), Some((256, 256)));
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
            vec![241, 242, 257, 258]
        );
        assert_eq!(
            layer2_selection_indices(Some((2, 16)), Some((1, 15))),
            vec![241, 242, 257, 258]
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
            vec![(0, 0xbeef), (1, 0xbeef), (16, 0xbeef), (17, 0xbeef)]
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
            (2, 2, vec![241, 242, 257, 258])
        );
        assert_eq!(
            layer2_selection_words(&bytes, Some((2, 16)), Some((1, 15))).unwrap(),
            (2, 2, vec![241, 242, 257, 258])
        );
        assert!(layer2_selection_words(&bytes, None, None).is_err());
    }

    #[test]
    fn rectangle_paste_translates_visual_words_to_native_indexes() {
        assert_eq!(
            layer2_paste_edits(Some((1, 15)), 2, 2, &[10, 11, 12, 13]).unwrap(),
            vec![(241, 10), (242, 11), (257, 12), (258, 13)]
        );
        assert!(layer2_paste_edits(Some((31, 31)), 2, 1, &[1, 2]).is_err());
        assert!(layer2_paste_edits(Some((0, 0)), 2, 2, &[1, 2, 3]).is_err());
        assert!(layer2_paste_edits(None, 1, 1, &[1]).is_err());
    }

    #[test]
    fn cut_uses_lunar_magics_proven_zero_word_for_every_selected_cell() {
        assert_eq!(
            layer2_cut_edits(Some((1, 15)), Some((2, 16))).unwrap(),
            vec![(241, 0), (242, 0), (257, 0), (258, 0)]
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
            vec![(0, 0x0456), (1, 0x0456), (17, 0x0456)]
        );
        assert!(layer2_flood_edits(&bytes, None, 1).is_err());
    }

    #[test]
    fn captured_pattern_flood_uses_visual_shape_and_one_edit_batch() {
        let mut words = vec![0_u16; 1024];
        for (x, y) in [(2, 1), (3, 1), (1, 2), (2, 2), (3, 2)] {
            words[lm_level::native_layer2_tilemap_index(x, y).unwrap()] = 0x8123;
        }
        let bytes = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        let pattern = Layer2FillPattern {
            width: 2,
            height: 2,
            words: vec![0xf001, 0xf002, 0xf003, 0xf004],
        };
        assert_eq!(
            layer2_pattern_flood_edits(&bytes, Some((3, 1)), &pattern).unwrap(),
            [
                ((2, 1), 0x0002),
                ((3, 1), 0x0001),
                ((1, 2), 0x0003),
                ((2, 2), 0x0004),
                ((3, 2), 0x0003),
            ]
            .map(|((x, y), word)| { (lm_level::native_layer2_tilemap_index(x, y).unwrap(), word) })
        );
    }

    #[test]
    fn move_selection_preserves_reversed_endpoints_and_rejects_edges() {
        let mut words = vec![0_u16; 1024];
        words[lm_level::native_layer2_tilemap_index(2, 2).unwrap()] = 0x1111;
        words[lm_level::native_layer2_tilemap_index(3, 2).unwrap()] = 0x2222;
        let bytes = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        let (edits, anchor, cursor) =
            layer2_move_edits(&bytes, Some((3, 2)), Some((2, 2)), 0, 1).unwrap();
        assert_eq!(anchor, (3, 3));
        assert_eq!(cursor, (2, 3));
        assert_eq!(
            edits,
            [((2, 2), 0), ((3, 2), 0), ((2, 3), 0x1111), ((3, 3), 0x2222),].map(
                |((x, y), word)| { (lm_level::native_layer2_tilemap_index(x, y).unwrap(), word,) }
            )
        );
        assert!(layer2_move_edits(&bytes, Some((0, 0)), Some((1, 1)), -1, 0).is_err());
        assert!(layer2_move_edits(&bytes, None, None, 1, 0).is_err());
    }

    #[test]
    fn resize_selection_preserves_orientation_and_enforces_edge_limits() {
        let mut words = vec![0_u16; 1024];
        for (offset, word) in [1_u16, 2, 3, 4].into_iter().enumerate() {
            let x = 2 + offset % 2;
            let y = 2 + offset / 2;
            words[lm_level::native_layer2_tilemap_index(x, y).unwrap()] = word;
        }
        let bytes = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        let (_, anchor, cursor) = layer2_resize_edits(
            &bytes,
            Some((3, 3)),
            Some((2, 2)),
            Layer2ResizeEdge::Left,
            true,
        )
        .unwrap();
        assert_eq!(anchor, (3, 3));
        assert_eq!(cursor, (1, 2));

        for (anchor, cursor, edge, grow) in [
            ((0, 0), (1, 1), Layer2ResizeEdge::Left, true),
            ((30, 0), (31, 1), Layer2ResizeEdge::Right, true),
            ((0, 0), (1, 1), Layer2ResizeEdge::Top, true),
            ((0, 30), (1, 31), Layer2ResizeEdge::Bottom, true),
            ((0, 0), (0, 1), Layer2ResizeEdge::Left, false),
            ((0, 0), (0, 1), Layer2ResizeEdge::Right, false),
            ((0, 0), (1, 0), Layer2ResizeEdge::Top, false),
            ((0, 0), (1, 0), Layer2ResizeEdge::Bottom, false),
        ] {
            assert!(
                layer2_resize_edits(&bytes, Some(anchor), Some(cursor), edge, grow).is_err(),
                "{edge:?} grow={grow}"
            );
        }
        assert!(layer2_resize_edits(&bytes, None, None, Layer2ResizeEdge::Right, true).is_err());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn complete_canvas_gesture_sequence_composes_every_rectangle_tool() {
        fn encoded(words: &[u16]) -> Vec<u8> {
            words.iter().flat_map(|word| word.to_le_bytes()).collect()
        }

        fn apply(words: &mut [u16], edits: &[(usize, u16)]) {
            for &(index, word) in edits {
                words[index] = word;
            }
        }

        let mut panels = AggregatePanels::default();
        let mut words = vec![0_u16; 1024];
        for (offset, word) in [0x1001_u16, 0x2002, 0x3003, 0x4004].into_iter().enumerate() {
            let x = 2 + offset % 2;
            let y = 2 + offset / 2;
            words[lm_level::native_layer2_tilemap_index(x, y).unwrap()] = word;
        }

        // A click anchors the rectangle; Shift-click extends it and loads that complete word.
        let first = lm_level::native_layer2_tilemap_index(2, 2).unwrap();
        select_layer2_tile_cell(&mut panels, 2, 2, first, words[first], false);
        let last = lm_level::native_layer2_tilemap_index(3, 3).unwrap();
        select_layer2_tile_cell(&mut panels, 3, 3, last, words[last], true);
        assert_eq!(panels.layer2_tile_anchor, Some((2, 2)));
        assert_eq!(panels.layer2_tile_cursor, Some((3, 3)));
        assert_eq!(panels.layer2_tile_index, last);
        assert_eq!(panels.layer2_tile, "4004");

        let copied = layer2_selection_words(
            &encoded(&words),
            panels.layer2_tile_anchor,
            panels.layer2_tile_cursor,
        )
        .unwrap();
        assert_eq!(copied, (2, 2, vec![0x1001, 0x2002, 0x3003, 0x4004]));

        // Fill preserves the complete 16-bit word, then paste restores visual row-major data.
        let fill = layer2_word_edits(
            panels.layer2_tile_index,
            panels.layer2_tile_anchor,
            panels.layer2_tile_cursor,
            0x8abc,
        );
        apply(&mut words, &fill);
        assert!(
            layer2_selection_words(
                &encoded(&words),
                panels.layer2_tile_anchor,
                panels.layer2_tile_cursor,
            )
            .unwrap()
            .2
            .iter()
            .all(|word| *word == 0x8abc)
        );
        apply(
            &mut words,
            &layer2_paste_edits(Some((2, 2)), copied.0, copied.1, &copied.2).unwrap(),
        );

        // Move retains complete words. Resize updates the gesture's live endpoints and repeats
        // Lunar Magic's normalized 12-bit Map16 pattern.
        let (edits, anchor, cursor) = layer2_move_edits(
            &encoded(&words),
            panels.layer2_tile_anchor,
            panels.layer2_tile_cursor,
            1,
            1,
        )
        .unwrap();
        apply(&mut words, &edits);
        panels.layer2_tile_anchor = Some(anchor);
        panels.layer2_tile_cursor = Some(cursor);
        assert_eq!((anchor, cursor), ((3, 3), (4, 4)));
        let (edits, anchor, cursor) = layer2_resize_edits(
            &encoded(&words),
            panels.layer2_tile_anchor,
            panels.layer2_tile_cursor,
            Layer2ResizeEdge::Right,
            true,
        )
        .unwrap();
        apply(&mut words, &edits);
        panels.layer2_tile_anchor = Some(anchor);
        panels.layer2_tile_cursor = Some(cursor);
        assert_eq!((anchor, cursor), ((3, 3), (5, 4)));
        assert_eq!(
            layer2_selection_words(
                &encoded(&words),
                panels.layer2_tile_anchor,
                panels.layer2_tile_cursor,
            )
            .unwrap()
            .2,
            vec![0x0001, 0x0002, 0x0001, 0x0003, 0x0004, 0x0003]
        );

        // Cut clears the full resized selection. Pattern flood then consumes the captured copy
        // through the same recovered 12-bit normalization boundary as resize.
        apply(
            &mut words,
            &layer2_cut_edits(panels.layer2_tile_anchor, panels.layer2_tile_cursor).unwrap(),
        );
        assert!(
            layer2_selection_words(
                &encoded(&words),
                panels.layer2_tile_anchor,
                panels.layer2_tile_cursor,
            )
            .unwrap()
            .2
            .iter()
            .all(|word| *word == 0)
        );
        let pattern = Layer2FillPattern {
            width: copied.0,
            height: copied.1,
            words: copied.2,
        };
        let edits = layer2_pattern_flood_edits(&encoded(&words), Some((3, 3)), &pattern).unwrap();
        apply(&mut words, &edits);
        // The zero region reaches the canvas origin, so the captured 2×2 pattern is anchored
        // there rather than at the clicked cell.
        assert_eq!(
            words[lm_level::native_layer2_tilemap_index(3, 3).unwrap()],
            4
        );
        assert_eq!(
            words[lm_level::native_layer2_tilemap_index(4, 3).unwrap()],
            3
        );

        // Ordinary flood uses the cursor's four-connected complete-word region and masks the
        // chosen replacement to Lunar Magic's 12-bit Map16 namespace.
        let edits = layer2_flood_edits(&encoded(&words), Some((3, 3)), 0xf321).unwrap();
        assert!(!edits.is_empty());
        assert!(edits.iter().all(|(_, word)| *word == 0x0321));

        // A new unmodified click starts a new 1×1 rectangle instead of retaining Shift state.
        let replacement = lm_level::native_layer2_tilemap_index(7, 8).unwrap();
        select_layer2_tile_cell(&mut panels, 7, 8, replacement, 0xbeef, false);
        assert_eq!(panels.layer2_tile_anchor, Some((7, 8)));
        assert_eq!(panels.layer2_tile_cursor, Some((7, 8)));
        assert_eq!(panels.layer2_tile, "BEEF");
    }
}
