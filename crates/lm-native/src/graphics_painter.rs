use eframe::egui;
use lm_graphics::{GraphicsColorMapFilters, IndexedTile, PaletteInterchangeFile, TileShift};

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
pub(crate) enum GraphicsDisplayPalette {
    Default,
    Row(usize),
}

impl Default for GraphicsDisplayPalette {
    fn default() -> Self {
        Self::Row(0)
    }
}

impl GraphicsDisplayPalette {
    pub(crate) fn label(self) -> String {
        match self {
            Self::Default => "Default".into(),
            Self::Row(row) => format!("{row:X}"),
        }
    }

    pub(crate) fn status(self) -> String {
        match self {
            Self::Default => "Rendered with default palette.".into(),
            Self::Row(row) => format!("Rendered with palette 0x{row:X}."),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GraphicsEditorStatus {
    text: Option<String>,
    hovered_tile: Option<usize>,
    editor_hovered: bool,
    hovered_color: Option<u8>,
}

impl GraphicsEditorStatus {
    pub(crate) fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    pub(crate) fn set(&mut self, text: impl Into<String>) {
        self.text = Some(text.into());
    }

    pub(crate) fn set_pointer_action(&mut self, text: impl Into<String>) {
        self.hovered_tile = None;
        self.editor_hovered = false;
        self.hovered_color = None;
        self.set(text);
    }

    pub(crate) fn select_tile(&mut self, index: usize) {
        self.set(format!("Tile 0x{index:X} selected for editing."));
    }

    pub(crate) fn select_foreground_color(&mut self, color: u8) {
        self.hovered_tile = None;
        self.editor_hovered = false;
        self.hovered_color = Some(color);
        self.set(format!("Color {color:X} selected for FG."));
    }

    pub(crate) fn update_palette_hover(&mut self, hovered: Option<u8>) {
        if hovered == self.hovered_color {
            return;
        }
        self.hovered_color = hovered;
        if let Some(color) = hovered {
            self.hovered_tile = None;
            self.editor_hovered = false;
            self.text = Some(format!("Color {color:X}."));
        } else {
            self.text = None;
        }
    }

    pub(crate) fn update_tile_hover(
        &mut self,
        responses: &[egui::Response],
        modifiers: egui::Modifiers,
    ) {
        let hovered = responses.iter().position(egui::Response::hovered);
        if hovered == self.hovered_tile {
            return;
        }
        self.hovered_tile = hovered;
        if let Some(index) = hovered {
            self.editor_hovered = false;
            self.hovered_color = None;
            self.text = Some(tile_hover_status(index, modifiers));
        } else {
            self.text = None;
        }
    }

    pub(crate) fn update_pixel_editor_hover(&mut self, hovered: bool, selected: usize) {
        if hovered == self.editor_hovered {
            return;
        }
        self.editor_hovered = hovered;
        if hovered {
            self.hovered_tile = None;
            self.hovered_color = None;
            self.select_tile(selected);
        } else {
            self.text = None;
        }
    }

    pub(crate) fn show(&self, ui: &mut egui::Ui) {
        ui.monospace(self.text().unwrap_or(""));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TilePointerAction {
    Select(usize),
    Copy(usize),
    PasteSelected(usize),
    PasteClipboard(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GraphicsCharacterShortcut {
    ApplyColorMap,
    EditColorMap,
    RotateClockwise,
    FlipHorizontal,
    FlipVertical,
}

#[derive(Clone, Debug)]
struct ColorMapDialog {
    draft: GraphicsColorMapFilters,
    source: u8,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct GraphicsColorMapEditor {
    filters: GraphicsColorMapFilters,
    selected_filter: usize,
    dialog: Option<ColorMapDialog>,
}

impl GraphicsColorMapEditor {
    pub(crate) fn apply(&self, tile: &IndexedTile) -> Option<IndexedTile> {
        self.filters.apply(self.selected_filter, tile)
    }

    pub(crate) fn open_dialog(&mut self) {
        self.begin_dialog();
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &PaletteInterchangeFile,
        display_palette: GraphicsDisplayPalette,
        tile: &IndexedTile,
        apply_enabled: bool,
    ) -> Option<IndexedTile> {
        let mut apply = false;
        ui.horizontal(|ui| {
            if ui.button("Color-map filters…").clicked() {
                self.begin_dialog();
            }
            apply = ui
                .add_enabled(apply_enabled, egui::Button::new("Apply color-map filter"))
                .clicked();
            ui.monospace(format!("Filter {:X}", self.selected_filter));
        });
        self.show_dialog(ui.ctx(), palette, display_palette);
        apply
            .then(|| self.filters.apply(self.selected_filter, tile))
            .flatten()
    }

    fn show_dialog(
        &mut self,
        context: &egui::Context,
        palette: &PaletteInterchangeFile,
        display_palette: GraphicsDisplayPalette,
    ) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let mut window_open = true;
        let mut accepted = false;
        let mut cancelled = false;
        let mut selected_filter = self.selected_filter;
        egui::Window::new("Graphics color-map filters")
            .collapsible(false)
            .resizable(false)
            .open(&mut window_open)
            .show(context, |ui| {
                show_color_map_dialog_contents(
                    ui,
                    &mut dialog,
                    palette,
                    display_palette,
                    &mut selected_filter,
                    &mut accepted,
                    &mut cancelled,
                );
            });
        self.selected_filter = selected_filter;
        if accepted {
            self.finish_dialog(dialog, true);
        } else if window_open && !cancelled {
            self.dialog = Some(dialog);
        } else {
            self.finish_dialog(dialog, false);
        }
    }

    fn begin_dialog(&mut self) {
        self.dialog = Some(ColorMapDialog {
            draft: self.filters.clone(),
            source: 0,
        });
    }

    fn finish_dialog(&mut self, dialog: ColorMapDialog, accepted: bool) {
        if accepted {
            self.filters = dialog.draft;
        }
    }
}

fn show_color_map_dialog_contents(
    ui: &mut egui::Ui,
    dialog: &mut ColorMapDialog,
    palette: &PaletteInterchangeFile,
    display_palette: GraphicsDisplayPalette,
    selected_filter: &mut usize,
    accepted: &mut bool,
    cancelled: &mut bool,
) {
    egui::ComboBox::from_label("Filter")
        .selected_text(format!("{selected_filter:X}"))
        .show_ui(ui, |ui| {
            for filter in 0..GraphicsColorMapFilters::FILTERS {
                ui.selectable_value(
                    selected_filter,
                    filter,
                    format!("Use color-map filter {filter:X}"),
                );
            }
        });
    ui.label("Source colors");
    ui.horizontal(|ui| {
        for source in 0_u8..16 {
            if color_map_button(
                ui,
                palette_color(palette, display_palette, source),
                source == dialog.source,
                source,
            )
            .clicked()
            {
                dialog.source = source;
            }
        }
    });
    let destination = dialog
        .draft
        .destination(*selected_filter, dialog.source)
        .unwrap_or(dialog.source);
    ui.label("Mapped colors");
    ui.horizontal(|ui| {
        for source in 0_u8..16 {
            let mapped = dialog
                .draft
                .destination(*selected_filter, source)
                .unwrap_or(source);
            if color_map_button(
                ui,
                palette_color(palette, display_palette, mapped),
                source == dialog.source,
                source,
            )
            .on_hover_text(format!("Color {source:X} → {mapped:X}"))
            .clicked()
            {
                dialog.source = source;
            }
        }
    });
    ui.label(format!("Destination for color {:X}", dialog.source));
    ui.horizontal(|ui| {
        for color in 0_u8..16 {
            let selected = color == destination;
            if color_map_button(
                ui,
                palette_color(palette, display_palette, color),
                selected,
                color,
            )
            .clicked()
            {
                let _ = dialog
                    .draft
                    .set_destination(*selected_filter, dialog.source, color);
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.button("Reset filter").clicked() {
            let _ = dialog.draft.reset(*selected_filter);
        }
        if ui.button("Cancel").clicked() {
            *cancelled = true;
        }
        if ui.button("OK").clicked() {
            *accepted = true;
        }
    });
}

fn color_map_button(
    ui: &mut egui::Ui,
    fill: egui::Color32,
    selected: bool,
    color: u8,
) -> egui::Response {
    ui.add_sized(
        [24.0, 24.0],
        egui::Button::new(if selected { "•" } else { "" }).fill(fill),
    )
    .on_hover_text(format!("Color {color:X}"))
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
    display_palette: GraphicsDisplayPalette,
    color: u8,
) -> egui::Color32 {
    match display_palette {
        GraphicsDisplayPalette::Default => {
            // RGBQUAD bytes at Lunar Magic 3.63 address 005E7B60, converted from BGR to RGB.
            const COLORS: [(u8, u8, u8); 16] = [
                (0, 0, 0),
                (240, 240, 240),
                (57, 51, 255),
                (89, 140, 242),
                (51, 0, 134),
                (191, 115, 0),
                (0, 207, 255),
                (239, 235, 180),
                (147, 0, 0),
                (81, 255, 0),
                (255, 172, 0),
                (188, 17, 164),
                (99, 207, 99),
                (220, 255, 255),
                (128, 128, 128),
                (240, 0, 0),
            ];
            let (red, green, blue) = COLORS[usize::from(color.min(15))];
            egui::Color32::from_rgb(red, green, blue)
        }
        GraphicsDisplayPalette::Row(row) => {
            let index = row.saturating_mul(16).saturating_add(usize::from(color));
            let rgb = palette.palette.colors[index].to_rgb8();
            egui::Color32::from_rgb(rgb.red, rgb.green, rgb.blue)
        }
    }
}

pub(crate) fn tile_button(
    ui: &mut egui::Ui,
    tile: &IndexedTile,
    palette: &PaletteInterchangeFile,
    display_palette: GraphicsDisplayPalette,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(34.0), egui::Sense::click());
    paint_tile(
        ui.painter(),
        rect.shrink(1.0),
        tile,
        palette,
        display_palette,
    );
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

pub(crate) fn take_graphics_save_shortcut(ui: &mut egui::Ui) -> bool {
    ui.input_mut(|input| {
        !input.modifiers.any() && input.consume_key(egui::Modifiers::NONE, egui::Key::F9)
    })
}

fn classify_tile_pointer_action(
    index: usize,
    primary: bool,
    secondary: bool,
    modifiers: egui::Modifiers,
) -> Option<TilePointerAction> {
    if secondary && modifiers == egui::Modifiers::NONE {
        Some(TilePointerAction::PasteSelected(index))
    } else if secondary && modifiers == egui::Modifiers::CTRL {
        Some(TilePointerAction::PasteClipboard(index))
    } else if primary && modifiers == egui::Modifiers::CTRL {
        Some(TilePointerAction::Copy(index))
    } else if primary && modifiers == egui::Modifiers::NONE {
        Some(TilePointerAction::Select(index))
    } else {
        None
    }
}

fn tile_hover_status(index: usize, modifiers: egui::Modifiers) -> String {
    if modifiers == (egui::Modifiers::CTRL | egui::Modifiers::SHIFT) {
        format!("Tile 0x{index:X}.")
    } else {
        format!(
            "Tile 0x{index:X} (Address 0x{:X})",
            index.saturating_mul(0x20)
        )
    }
}

pub(crate) fn apply_tile_keyboard_navigation(
    ui: &mut egui::Ui,
    selected: &mut usize,
    responses: &[egui::Response],
) -> Option<String> {
    let Some(response) = responses.get(*selected) else {
        return None;
    };
    if !response.has_focus() {
        return None;
    }
    let navigation = ui.input_mut(|input| {
        if input.modifiers.any() {
            return None;
        }
        const KEYS: [(egui::Key, TileNavigation); 2] = [
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
        return None;
    };
    let next = navigated_tile_index(*selected, responses.len(), navigation);
    let page = *selected / GRAPHICS_PAGE_TILES;
    if next == *selected {
        return Some(match navigation {
            TileNavigation::PreviousPage => format!("Already at Start (0x{page:X})."),
            TileNavigation::NextPage => format!("Already at End (0x{page:X})."),
        });
    }
    let Some(response) = responses.get(next) else {
        return None;
    };
    *selected = next;
    response.request_focus();
    response.scroll_to_me(Some(egui::Align::Center));
    Some(format!(
        "Viewing 8x8 page 0x{:X}.",
        next / GRAPHICS_PAGE_TILES
    ))
}

pub(crate) fn apply_tile_palette_keyboard(
    ui: &mut egui::Ui,
    selected: usize,
    responses: &[egui::Response],
    display_palette: &mut GraphicsDisplayPalette,
    row_count: usize,
) -> Option<String> {
    if row_count == 0
        || !responses
            .get(selected)
            .is_some_and(egui::Response::has_focus)
    {
        return None;
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
    let step = step?;
    let next = stepped_display_palette(*display_palette, row_count, step);
    if next == *display_palette {
        return None;
    }
    *display_palette = next;
    Some(next.status())
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

pub(crate) fn take_graphics_character_shortcut(
    ui: &mut egui::Ui,
    selected: usize,
    responses: &[egui::Response],
) -> Option<GraphicsCharacterShortcut> {
    if !responses
        .get(selected)
        .is_some_and(egui::Response::has_focus)
    {
        return None;
    }
    ui.input_mut(|input| {
        if input.modifiers.any() {
            return None;
        }
        [
            (egui::Key::D, GraphicsCharacterShortcut::ApplyColorMap),
            (egui::Key::M, GraphicsCharacterShortcut::EditColorMap),
            (egui::Key::R, GraphicsCharacterShortcut::RotateClockwise),
            (egui::Key::X, GraphicsCharacterShortcut::FlipHorizontal),
            (egui::Key::Y, GraphicsCharacterShortcut::FlipVertical),
        ]
        .into_iter()
        .find_map(|(key, shortcut)| {
            input
                .consume_key(egui::Modifiers::NONE, key)
                .then_some(shortcut)
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

fn stepped_display_palette(
    current: GraphicsDisplayPalette,
    row_count: usize,
    step: PaletteStep,
) -> GraphicsDisplayPalette {
    if row_count == 0 {
        return GraphicsDisplayPalette::Default;
    }
    match (current, step) {
        (
            GraphicsDisplayPalette::Default | GraphicsDisplayPalette::Row(0),
            PaletteStep::Previous,
        ) => GraphicsDisplayPalette::Default,
        (GraphicsDisplayPalette::Default, PaletteStep::Next) => GraphicsDisplayPalette::Row(0),
        (GraphicsDisplayPalette::Row(row), PaletteStep::Previous) => {
            GraphicsDisplayPalette::Row(row.min(row_count - 1) - 1)
        }
        (GraphicsDisplayPalette::Row(row), PaletteStep::Next) => {
            GraphicsDisplayPalette::Row((row.min(row_count - 1) + 1).min(row_count - 1))
        }
    }
}

pub(crate) fn paint_tile(
    painter: &egui::Painter,
    rect: egui::Rect,
    tile: &IndexedTile,
    palette: &PaletteInterchangeFile,
    display_palette: GraphicsDisplayPalette,
) {
    let cell = rect.width().min(rect.height()) / 8.0;
    for (y, y_offset) in CELL_OFFSETS.into_iter().enumerate() {
        for (x, x_offset) in CELL_OFFSETS.into_iter().enumerate() {
            let color = palette_color(palette, display_palette, tile.pixel(x, y).unwrap_or(0));
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

    fn render_navigation_status(
        context: &egui::Context,
        selected: &mut usize,
        request_focus: bool,
    ) -> Option<String> {
        let mut status = None;
        egui::CentralPanel::default().show(context, |ui| {
            let responses = (0..TEST_GRID_TILES)
                .map(|index| ui.button(index.to_string()))
                .collect::<Vec<_>>();
            if request_focus {
                responses[*selected].request_focus();
            }
            status = apply_tile_keyboard_navigation(ui, selected, &responses);
        });
        status
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
    fn palette_shortcuts_include_the_native_default_palette_and_bound_rows() {
        use GraphicsDisplayPalette::{Default, Row};

        assert_eq!(
            stepped_display_palette(Row(7), 0, PaletteStep::Next),
            Default
        );
        assert_eq!(
            stepped_display_palette(Default, 8, PaletteStep::Previous),
            Default
        );
        assert_eq!(
            stepped_display_palette(Default, 8, PaletteStep::Next),
            Row(0)
        );
        assert_eq!(
            stepped_display_palette(Row(0), 8, PaletteStep::Previous),
            Default
        );
        assert_eq!(Default.status(), "Rendered with default palette.");
        assert_eq!(Row(0xa).status(), "Rendered with palette 0xA.");
        assert_eq!(
            stepped_display_palette(Row(0), 8, PaletteStep::Next),
            Row(1)
        );
        assert_eq!(
            stepped_display_palette(Row(7), 8, PaletteStep::Next),
            Row(7)
        );
        assert_eq!(
            stepped_display_palette(Row(usize::MAX), 8, PaletteStep::Previous),
            Row(6)
        );
    }

    #[test]
    fn default_palette_matches_the_recovered_rgbquad_table() {
        let palette = PaletteInterchangeFile {
            source_palette: 0,
            palette: lm_graphics::Palette { colors: Vec::new() },
        };
        let expected = [
            (0, 0, 0),
            (240, 240, 240),
            (57, 51, 255),
            (89, 140, 242),
            (51, 0, 134),
            (191, 115, 0),
            (0, 207, 255),
            (239, 235, 180),
            (147, 0, 0),
            (81, 255, 0),
            (255, 172, 0),
            (188, 17, 164),
            (99, 207, 99),
            (220, 255, 255),
            (128, 128, 128),
            (240, 0, 0),
        ];
        for (color, (red, green, blue)) in expected.into_iter().enumerate() {
            assert_eq!(
                palette_color(
                    &palette,
                    GraphicsDisplayPalette::Default,
                    u8::try_from(color).unwrap()
                ),
                egui::Color32::from_rgb(red, green, blue)
            );
        }
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
            Some(TilePointerAction::PasteSelected(6))
        );
        for modifiers in [egui::Modifiers::SHIFT, egui::Modifiers::COMMAND] {
            assert_eq!(
                classify_tile_pointer_action(7, true, false, modifiers),
                None
            );
        }
        assert_eq!(
            classify_tile_pointer_action(8, false, true, egui::Modifiers::CTRL),
            Some(TilePointerAction::PasteClipboard(8))
        );
    }

    #[test]
    fn graphics_save_shortcut_requires_unmodified_f9() {
        let context = egui::Context::default();
        let mut taken = false;
        let mut modified_taken = false;
        let _ = context.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::F9,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    taken = take_graphics_save_shortcut(ui);
                });
            },
        );
        let _ = context.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::F9,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                }],
                modifiers: egui::Modifiers::CTRL,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    modified_taken = take_graphics_save_shortcut(ui);
                });
            },
        );

        assert!(taken);
        assert!(!modified_taken);
    }

    #[test]
    fn color_map_dialog_commits_only_on_accept() {
        let mut editor = GraphicsColorMapEditor::default();
        editor.begin_dialog();
        let mut cancelled = editor.dialog.take().unwrap();
        cancelled.draft.set_destination(4, 2, 11).unwrap();
        editor.finish_dialog(cancelled, false);
        assert_eq!(editor.filters.destination(4, 2), Some(2));

        editor.begin_dialog();
        let mut accepted = editor.dialog.take().unwrap();
        accepted.draft.set_destination(4, 2, 11).unwrap();
        editor.finish_dialog(accepted, true);
        assert_eq!(editor.filters.destination(4, 2), Some(11));
    }

    #[test]
    fn native_tile_status_formats_address_modifier_and_actions_exactly() {
        assert_eq!(
            tile_hover_status(0x1f, egui::Modifiers::NONE),
            "Tile 0x1F (Address 0x3E0)"
        );
        assert_eq!(
            tile_hover_status(0x1f, egui::Modifiers::CTRL | egui::Modifiers::SHIFT),
            "Tile 0x1F."
        );
        let mut status = GraphicsEditorStatus::default();
        status.select_tile(0x123);
        assert_eq!(
            status.text.as_deref(),
            Some("Tile 0x123 selected for editing.")
        );
        status.select_foreground_color(0xe);
        assert_eq!(status.text.as_deref(), Some("Color E selected for FG."));
    }

    #[test]
    fn transient_status_changes_only_when_the_pointer_region_changes() {
        let mut status = GraphicsEditorStatus::default();
        status.update_palette_hover(Some(3));
        assert_eq!(status.text.as_deref(), Some("Color 3."));
        status.select_foreground_color(3);
        status.update_palette_hover(Some(3));
        assert_eq!(status.text.as_deref(), Some("Color 3 selected for FG."));
        status.update_palette_hover(None);
        assert_eq!(status.text, None);

        status.update_pixel_editor_hover(true, 0x2a);
        assert_eq!(
            status.text.as_deref(),
            Some("Tile 0x2A selected for editing.")
        );
        status.set("Rendered with palette 0x4.");
        status.update_pixel_editor_hover(true, 0x2a);
        assert_eq!(status.text.as_deref(), Some("Rendered with palette 0x4."));
        status.update_pixel_editor_hover(false, 0x2a);
        assert_eq!(status.text, None);
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
    fn focused_grid_reports_exact_page_and_boundary_status() {
        let context = egui::Context::default();
        let mut selected = 0;
        let _ = context.run(egui::RawInput::default(), |context| {
            render_navigation_status(context, &mut selected, true);
        });
        let mut status = None;
        let _ = context.run(
            egui::RawInput {
                events: vec![key_event(egui::Key::ArrowUp, egui::Modifiers::NONE)],
                ..Default::default()
            },
            |context| status = render_navigation_status(context, &mut selected, false),
        );
        assert_eq!(selected, 0);
        assert_eq!(status.as_deref(), Some("Already at Start (0x0)."));

        let _ = context.run(
            egui::RawInput {
                events: vec![key_event(egui::Key::ArrowLeft, egui::Modifiers::NONE)],
                ..Default::default()
            },
            |context| status = render_navigation_status(context, &mut selected, false),
        );
        assert_eq!(status, None);

        let _ = context.run(
            egui::RawInput {
                events: vec![key_event(egui::Key::ArrowDown, egui::Modifiers::NONE)],
                ..Default::default()
            },
            |context| status = render_navigation_status(context, &mut selected, false),
        );
        assert_eq!(selected, 256);
        assert_eq!(status.as_deref(), Some("Viewing 8x8 page 0x1."));

        selected = TEST_GRID_TILES - 1;
        let _ = context.run(egui::RawInput::default(), |context| {
            render_navigation_status(context, &mut selected, true);
        });
        let _ = context.run(
            egui::RawInput {
                events: vec![key_event(egui::Key::ArrowDown, egui::Modifiers::NONE)],
                ..Default::default()
            },
            |context| status = render_navigation_status(context, &mut selected, false),
        );
        assert_eq!(status.as_deref(), Some("Already at End (0x2)."));
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
        let mut display_palette = GraphicsDisplayPalette::Row(6);
        let mut status = None;
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
                status =
                    apply_tile_palette_keyboard(ui, selected, &responses, &mut display_palette, 8);
            });
        });
        assert_eq!(selected, 9);
        assert_eq!(display_palette, GraphicsDisplayPalette::Row(7));
        assert_eq!(status.as_deref(), Some("Rendered with palette 0x7."));

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
                status =
                    apply_tile_palette_keyboard(ui, selected, &responses, &mut display_palette, 8);
            });
        });
        assert_eq!(display_palette, GraphicsDisplayPalette::Row(6));
        assert_eq!(status.as_deref(), Some("Rendered with palette 0x6."));
    }

    #[test]
    fn focused_grid_routes_exact_native_character_shortcuts() {
        let cases = [
            (egui::Key::D, GraphicsCharacterShortcut::ApplyColorMap),
            (egui::Key::M, GraphicsCharacterShortcut::EditColorMap),
            (egui::Key::R, GraphicsCharacterShortcut::RotateClockwise),
            (egui::Key::X, GraphicsCharacterShortcut::FlipHorizontal),
            (egui::Key::Y, GraphicsCharacterShortcut::FlipVertical),
        ];
        for (key, expected) in cases {
            let context = egui::Context::default();
            let mut selected = 0;
            let _ = context.run(egui::RawInput::default(), |context| {
                render_keyboard_grid(context, &mut selected, true);
            });
            let mut actual = None;
            let _ = context.run(
                egui::RawInput {
                    events: vec![key_event(key, egui::Modifiers::NONE)],
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        let responses = (0..TEST_GRID_TILES)
                            .map(|index| ui.button(index.to_string()))
                            .collect::<Vec<_>>();
                        actual = take_graphics_character_shortcut(ui, selected, &responses);
                    });
                },
            );
            assert_eq!(actual, Some(expected));
        }

        let context = egui::Context::default();
        let mut selected = 0;
        let _ = context.run(egui::RawInput::default(), |context| {
            render_keyboard_grid(context, &mut selected, true);
        });
        let mut modified = None;
        let _ = context.run(
            egui::RawInput {
                events: vec![key_event(egui::Key::X, egui::Modifiers::CTRL)],
                modifiers: egui::Modifiers::CTRL,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let responses = (0..TEST_GRID_TILES)
                        .map(|index| ui.button(index.to_string()))
                        .collect::<Vec<_>>();
                    modified = take_graphics_character_shortcut(ui, selected, &responses);
                });
            },
        );
        assert_eq!(modified, None);
    }
}
