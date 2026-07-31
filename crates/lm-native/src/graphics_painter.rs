use eframe::egui;
use lm_graphics::{IndexedTile, PaletteInterchangeFile, TileShift};

pub(crate) const TILE_GRID_COLUMNS: usize = 8;
const TILE_EDITOR_ZOOMS: [(u16, f32); 5] = [
    (800, 64.0),
    (1_600, 128.0),
    (2_400, 192.0),
    (3_200, 256.0),
    (4_000, 320.0),
];
const GRAPHICS_PAGE_TILES: usize = 0x100;
const CELL_OFFSETS: [f32; 8] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
const CELL_ENDS: [f32; 8] = [0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TileNavigation {
    PreviousPage,
    NextPage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PaletteStep {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TilePointerAction {
    Select(usize),
    Copy(usize),
    Paste(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TileEditorZoom(usize);

impl Default for TileEditorZoom {
    fn default() -> Self {
        Self(TILE_EDITOR_ZOOMS.len() - 1)
    }
}

impl TileEditorZoom {
    pub(crate) fn side(self) -> f32 {
        TILE_EDITOR_ZOOMS
            .get(self.0)
            .unwrap_or_else(|| TILE_EDITOR_ZOOMS.last().expect("tile zooms are nonempty"))
            .1
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui) {
        if self.0 >= TILE_EDITOR_ZOOMS.len() {
            *self = Self::default();
        }
        let percentage = TILE_EDITOR_ZOOMS[self.0].0;
        egui::ComboBox::from_label("Zoom")
            .selected_text(format!("{percentage}%"))
            .show_ui(ui, |ui| {
                for (index, (percentage, _)) in TILE_EDITOR_ZOOMS.into_iter().enumerate() {
                    ui.selectable_value(&mut self.0, index, format!("{percentage}%"));
                }
            });
    }
}

pub(crate) fn palette_color(
    palette: &PaletteInterchangeFile,
    row: usize,
    color: u8,
) -> egui::Color32 {
    let index = row.saturating_mul(16).saturating_add(usize::from(color));
    let rgb = palette.palette.colors[index].to_rgb8();
    egui::Color32::from_rgb(rgb.red, rgb.green, rgb.blue)
}

pub(crate) fn tile_button(
    ui: &mut egui::Ui,
    tile: &IndexedTile,
    palette: &PaletteInterchangeFile,
    row: usize,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(34.0), egui::Sense::click());
    paint_tile(ui.painter(), rect.shrink(1.0), tile, palette, row);
    if selected {
        ui.painter().rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(2.0_f32, egui::Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    } else if response.hovered() {
        ui.painter().rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0_f32, egui::Color32::LIGHT_BLUE),
            egui::StrokeKind::Inside,
        );
    }
    response
}

pub(crate) fn tile_pointer_action(
    ui: &egui::Ui,
    response: &egui::Response,
    index: usize,
) -> Option<TilePointerAction> {
    classify_tile_pointer_action(
        index,
        response.clicked_by(egui::PointerButton::Primary),
        response.clicked_by(egui::PointerButton::Secondary),
        ui.input(|input| input.modifiers),
    )
}

fn classify_tile_pointer_action(
    index: usize,
    primary: bool,
    secondary: bool,
    modifiers: egui::Modifiers,
) -> Option<TilePointerAction> {
    if secondary && modifiers == egui::Modifiers::NONE {
        Some(TilePointerAction::Paste(index))
    } else if primary && modifiers == egui::Modifiers::CTRL {
        Some(TilePointerAction::Copy(index))
    } else if primary && modifiers == egui::Modifiers::NONE {
        Some(TilePointerAction::Select(index))
    } else {
        None
    }
}

pub(crate) fn show_tile_grid_status(
    ui: &mut egui::Ui,
    selected: usize,
    responses: &[egui::Response],
) {
    let hovered = responses.iter().position(egui::Response::hovered);
    if let Some(status) = tile_grid_status(selected, responses.len(), hovered) {
        ui.monospace(status);
    } else {
        ui.monospace("No graphics tiles");
    }
}

fn tile_grid_status(selected: usize, tile_count: usize, hovered: Option<usize>) -> Option<String> {
    let last = tile_count.checked_sub(1)?;
    let selected = selected.min(last);
    match hovered.filter(|index| *index <= last) {
        Some(hovered) => Some(format!(
            "Selected tile {selected:03X}  •  Hover tile {hovered:03X}"
        )),
        None => Some(format!("Selected tile {selected:03X}")),
    }
}

pub(crate) fn apply_tile_keyboard_navigation(
    ui: &mut egui::Ui,
    selected: &mut usize,
    responses: &[egui::Response],
) {
    let Some(response) = responses.get(*selected) else {
        return;
    };
    if !response.has_focus() {
        return;
    }
    let navigation = ui.input_mut(|input| {
        if input.modifiers.any() {
            return None;
        }
        const KEYS: [(egui::Key, TileNavigation); 4] = [
            (egui::Key::ArrowLeft, TileNavigation::PreviousPage),
            (egui::Key::ArrowRight, TileNavigation::NextPage),
            (egui::Key::ArrowUp, TileNavigation::PreviousPage),
            (egui::Key::ArrowDown, TileNavigation::NextPage),
        ];
        KEYS.into_iter().find_map(|(key, navigation)| {
            input
                .consume_key(egui::Modifiers::NONE, key)
                .then_some(navigation)
        })
    });
    let Some(navigation) = navigation else {
        return;
    };
    let next = navigated_tile_index(*selected, responses.len(), navigation);
    let Some(response) = responses.get(next) else {
        return;
    };
    *selected = next;
    response.request_focus();
    response.scroll_to_me(Some(egui::Align::Center));
}

pub(crate) fn apply_tile_palette_keyboard(
    ui: &mut egui::Ui,
    selected: usize,
    responses: &[egui::Response],
    palette_row: &mut usize,
    row_count: usize,
) {
    if row_count == 0
        || !responses
            .get(selected)
            .is_some_and(egui::Response::has_focus)
    {
        return;
    }
    let step = ui.input_mut(|input| {
        if input.modifiers.any() {
            return None;
        }
        if input.consume_key(egui::Modifiers::NONE, egui::Key::PageUp) {
            Some(PaletteStep::Next)
        } else if input.consume_key(egui::Modifiers::NONE, egui::Key::PageDown) {
            Some(PaletteStep::Previous)
        } else {
            None
        }
    });
    if let Some(step) = step {
        *palette_row = cycled_palette_row(*palette_row, row_count, step);
    }
}

pub(crate) fn take_tile_shift(
    ui: &mut egui::Ui,
    selected: usize,
    responses: &[egui::Response],
    enabled: bool,
) -> Option<TileShift> {
    if !enabled
        || !responses
            .get(selected)
            .is_some_and(egui::Response::has_focus)
    {
        return None;
    }
    ui.input_mut(|input| {
        if input.modifiers != egui::Modifiers::SHIFT {
            return None;
        }
        [
            (egui::Key::ArrowLeft, TileShift::Left),
            (egui::Key::ArrowRight, TileShift::Right),
            (egui::Key::ArrowUp, TileShift::Up),
            (egui::Key::ArrowDown, TileShift::Down),
        ]
        .into_iter()
        .find_map(|(key, shift)| {
            input
                .consume_key(egui::Modifiers::SHIFT, key)
                .then_some(shift)
        })
    })
}

fn navigated_tile_index(selected: usize, tile_count: usize, navigation: TileNavigation) -> usize {
    let Some(last) = tile_count.checked_sub(1) else {
        return 0;
    };
    let selected = selected.min(last);
    match navigation {
        TileNavigation::PreviousPage => selected
            .checked_sub(GRAPHICS_PAGE_TILES)
            .unwrap_or(selected),
        TileNavigation::NextPage => {
            let next = selected.saturating_add(GRAPHICS_PAGE_TILES);
            if next <= last {
                next
            } else if selected / GRAPHICS_PAGE_TILES < last / GRAPHICS_PAGE_TILES {
                last
            } else {
                selected
            }
        }
    }
}

fn cycled_palette_row(current: usize, row_count: usize, step: PaletteStep) -> usize {
    if row_count == 0 {
        return 0;
    }
    let current = current.min(row_count - 1);
    match step {
        PaletteStep::Previous => (current + row_count - 1) % row_count,
        PaletteStep::Next => (current + 1) % row_count,
    }
}

pub(crate) fn paint_tile(
    painter: &egui::Painter,
    rect: egui::Rect,
    tile: &IndexedTile,
    palette: &PaletteInterchangeFile,
    row: usize,
) {
    let cell = rect.width().min(rect.height()) / 8.0;
    for (y, y_offset) in CELL_OFFSETS.into_iter().enumerate() {
        for (x, x_offset) in CELL_OFFSETS.into_iter().enumerate() {
            let color = palette_color(palette, row, tile.pixel(x, y).unwrap_or(0));
            let minimum = rect.min + egui::vec2(x_offset * cell, y_offset * cell);
            painter.rect_filled(
                egui::Rect::from_min_size(minimum, egui::Vec2::splat(cell + 0.25)),
                0.0,
                color,
            );
        }
    }
}

pub(crate) fn tile_coordinate(rect: egui::Rect, position: egui::Pos2) -> Option<(usize, usize)> {
    if !rect.contains(position) || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }
    let relative_x = (position.x - rect.min.x) / rect.width();
    let relative_y = (position.y - rect.min.y) / rect.height();
    let x = CELL_ENDS.into_iter().position(|end| relative_x < end)?;
    let y = CELL_ENDS.into_iter().position(|end| relative_y < end)?;
    Some((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_GRID_TILES: usize = 600;

    fn render_keyboard_grid(context: &egui::Context, selected: &mut usize, request_focus: bool) {
        egui::CentralPanel::default().show(context, |ui| {
            let responses = (0..TEST_GRID_TILES)
                .map(|index| ui.button(index.to_string()))
                .collect::<Vec<_>>();
            if request_focus {
                responses[*selected].request_focus();
            }
            apply_tile_keyboard_navigation(ui, selected, &responses);
        });
    }

    fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn tile_hit_testing_is_bounded_and_exact() {
        let rect = egui::Rect::from_min_size(egui::Pos2::new(10.0, 20.0), egui::Vec2::splat(80.0));
        assert_eq!(
            tile_coordinate(rect, egui::Pos2::new(10.0, 20.0)),
            Some((0, 0))
        );
        assert_eq!(
            tile_coordinate(rect, egui::Pos2::new(89.9, 99.9)),
            Some((7, 7))
        );
        assert_eq!(tile_coordinate(rect, egui::Pos2::new(90.1, 50.0)), None);
    }

    #[test]
    fn tile_page_navigation_is_bounded_and_preserves_offset() {
        assert_eq!(navigated_tile_index(0, 0, TileNavigation::NextPage), 0);
        assert_eq!(
            navigated_tile_index(9, 600, TileNavigation::PreviousPage),
            9
        );
        assert_eq!(navigated_tile_index(9, 600, TileNavigation::NextPage), 265);
        assert_eq!(
            navigated_tile_index(265, 600, TileNavigation::PreviousPage),
            9
        );
        assert_eq!(
            navigated_tile_index(500, 600, TileNavigation::NextPage),
            599
        );
        assert_eq!(
            navigated_tile_index(599, 600, TileNavigation::NextPage),
            599
        );
        assert_eq!(
            navigated_tile_index(usize::MAX, 600, TileNavigation::PreviousPage),
            343
        );
    }

    #[test]
    fn palette_row_shortcuts_are_bounded_and_wrap_both_directions() {
        assert_eq!(cycled_palette_row(7, 0, PaletteStep::Next), 0);
        assert_eq!(cycled_palette_row(7, 1, PaletteStep::Previous), 0);
        assert_eq!(cycled_palette_row(0, 8, PaletteStep::Next), 1);
        assert_eq!(cycled_palette_row(7, 8, PaletteStep::Next), 0);
        assert_eq!(cycled_palette_row(0, 8, PaletteStep::Previous), 7);
        assert_eq!(cycled_palette_row(usize::MAX, 8, PaletteStep::Previous), 6);
    }

    #[test]
    fn pointer_gestures_require_exact_buttons_and_modifiers() {
        assert_eq!(
            classify_tile_pointer_action(4, true, false, egui::Modifiers::NONE),
            Some(TilePointerAction::Select(4))
        );
        assert_eq!(
            classify_tile_pointer_action(5, true, false, egui::Modifiers::CTRL),
            Some(TilePointerAction::Copy(5))
        );
        assert_eq!(
            classify_tile_pointer_action(6, false, true, egui::Modifiers::NONE),
            Some(TilePointerAction::Paste(6))
        );
        for modifiers in [egui::Modifiers::SHIFT, egui::Modifiers::COMMAND] {
            assert_eq!(
                classify_tile_pointer_action(7, true, false, modifiers),
                None
            );
        }
        assert_eq!(
            classify_tile_pointer_action(8, false, true, egui::Modifiers::CTRL),
            None
        );
    }

    #[test]
    fn tile_grid_status_is_bounded_and_reports_hover_separately() {
        assert_eq!(tile_grid_status(0, 0, Some(0)), None);
        assert_eq!(
            tile_grid_status(usize::MAX, 0x22, None).as_deref(),
            Some("Selected tile 021")
        );
        assert_eq!(
            tile_grid_status(3, 0x22, Some(0x1f)).as_deref(),
            Some("Selected tile 003  •  Hover tile 01F")
        );
        assert_eq!(
            tile_grid_status(3, 0x22, Some(0x22)).as_deref(),
            Some("Selected tile 003")
        );
    }

    #[test]
    fn tile_editor_zoom_defaults_to_existing_size_and_recovers_invalid_state() {
        let zoom = TileEditorZoom::default();
        assert_eq!(zoom.side(), 320.0);
        assert_eq!(TileEditorZoom(0).side(), 64.0);
        assert_eq!(TileEditorZoom(3).side(), 256.0);
        assert_eq!(TileEditorZoom(usize::MAX).side(), 320.0);
        assert!(
            TILE_EDITOR_ZOOMS
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0 && pair[0].1 < pair[1].1)
        );
        for (_, side) in TILE_EDITOR_ZOOMS {
            let rect =
                egui::Rect::from_min_size(egui::Pos2::new(5.0, 9.0), egui::Vec2::splat(side));
            assert_eq!(tile_coordinate(rect, rect.min), Some((0, 0)));
            assert_eq!(
                tile_coordinate(rect, rect.max - egui::Vec2::splat(0.01)),
                Some((7, 7))
            );
        }
    }

    #[test]
    fn focused_grid_consumes_unmodified_navigation_and_transfers_focus() {
        let context = egui::Context::default();
        let mut selected = 9;
        let _ = context.run(egui::RawInput::default(), |context| {
            render_keyboard_grid(context, &mut selected, true);
        });
        let input = egui::RawInput {
            events: vec![key_event(egui::Key::ArrowDown, egui::Modifiers::NONE)],
            ..Default::default()
        };
        let _ = context.run(input, |context| {
            render_keyboard_grid(context, &mut selected, false);
        });
        assert_eq!(selected, 265);
        let _ = context.run(egui::RawInput::default(), |context| {
            render_keyboard_grid(context, &mut selected, true);
        });

        let input = egui::RawInput {
            events: vec![key_event(egui::Key::ArrowUp, egui::Modifiers::NONE)],
            ..Default::default()
        };
        let _ = context.run(input, |context| {
            render_keyboard_grid(context, &mut selected, false);
        });
        assert_eq!(selected, 9);
    }

    #[test]
    fn grid_ignores_modified_navigation() {
        let context = egui::Context::default();
        let mut selected = 9;
        let _ = context.run(egui::RawInput::default(), |context| {
            render_keyboard_grid(context, &mut selected, true);
        });
        let input = egui::RawInput {
            events: vec![key_event(egui::Key::ArrowDown, egui::Modifiers::SHIFT)],
            modifiers: egui::Modifiers::SHIFT,
            ..Default::default()
        };
        let _ = context.run(input, |context| {
            render_keyboard_grid(context, &mut selected, false);
        });
        assert_eq!(selected, 9);
    }

    #[test]
    fn focused_grid_routes_exact_shift_arrow_only_when_enabled() {
        let context = egui::Context::default();
        let mut selected = 9;
        let _ = context.run(egui::RawInput::default(), |context| {
            render_keyboard_grid(context, &mut selected, true);
        });
        let input = egui::RawInput {
            events: vec![key_event(egui::Key::ArrowLeft, egui::Modifiers::SHIFT)],
            modifiers: egui::Modifiers::SHIFT,
            ..Default::default()
        };
        let mut shift = None;
        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let responses = (0..TEST_GRID_TILES)
                    .map(|index| ui.button(index.to_string()))
                    .collect::<Vec<_>>();
                apply_tile_keyboard_navigation(ui, &mut selected, &responses);
                shift = take_tile_shift(ui, selected, &responses, true);
            });
        });
        assert_eq!(selected, 9);
        assert_eq!(shift, Some(TileShift::Left));

        let input = egui::RawInput {
            events: vec![key_event(egui::Key::ArrowRight, egui::Modifiers::SHIFT)],
            modifiers: egui::Modifiers::SHIFT,
            ..Default::default()
        };
        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let responses = (0..TEST_GRID_TILES)
                    .map(|index| ui.button(index.to_string()))
                    .collect::<Vec<_>>();
                shift = take_tile_shift(ui, selected, &responses, false);
            });
        });
        assert_eq!(shift, None);
    }

    #[test]
    fn focused_grid_routes_unmodified_page_keys_to_palette_rows() {
        let context = egui::Context::default();
        let mut selected = 9;
        let mut palette_row = 7;
        let _ = context.run(egui::RawInput::default(), |context| {
            render_keyboard_grid(context, &mut selected, true);
        });
        let input = egui::RawInput {
            events: vec![key_event(egui::Key::PageUp, egui::Modifiers::NONE)],
            ..Default::default()
        };
        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let responses = (0..TEST_GRID_TILES)
                    .map(|index| ui.button(index.to_string()))
                    .collect::<Vec<_>>();
                apply_tile_keyboard_navigation(ui, &mut selected, &responses);
                apply_tile_palette_keyboard(ui, selected, &responses, &mut palette_row, 8);
            });
        });
        assert_eq!(selected, 9);
        assert_eq!(palette_row, 0);

        let _ = context.run(egui::RawInput::default(), |context| {
            render_keyboard_grid(context, &mut selected, true);
        });
        let input = egui::RawInput {
            events: vec![key_event(egui::Key::PageDown, egui::Modifiers::NONE)],
            ..Default::default()
        };
        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let responses = (0..TEST_GRID_TILES)
                    .map(|index| ui.button(index.to_string()))
                    .collect::<Vec<_>>();
                apply_tile_palette_keyboard(ui, selected, &responses, &mut palette_row, 8);
            });
        });
        assert_eq!(palette_row, 7);
    }
}
