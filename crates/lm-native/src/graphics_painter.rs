use eframe::egui;
use lm_app::{ExtendedUiTextKey, LocalizationCatalog};
use lm_graphics::{
    GraphicsColorMapFilters, GraphicsTileOwner, IndexedTile, PaletteInterchangeFile, TileShift,
};

const ORIGINAL_COLOR_MAP_DIALOG_ID: u16 = 0x0401;

pub(crate) const TILE_GRID_COLUMNS: usize = 16;
const TILE_SHEET_CELL_SIDE: f32 = 16.0;
pub(crate) const TILE_EDITOR_SIDE: f32 = 256.0;
const GRAPHICS_PAGE_TILES: usize = 0x100;
const CELL_OFFSETS: [f32; 8] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
const CELL_ENDS: [f32; 8] = [0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TileNavigation {
    PreviousPage,
    NextPage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaletteStep {
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
        self.set(format!("Color {color:X} selected for FG."));
    }

    pub(crate) fn select_background_color(&mut self, color: u8) {
        self.set(format!("Color {color:X} selected for BG."));
    }

    pub(crate) fn update_palette_hover(&mut self, hovered: Option<u8>, pointer_moved: bool) {
        if hovered == self.hovered_color && (hovered.is_none() || !pointer_moved) {
            return;
        }
        let text = hovered.map(|color| format!("Color {color:X}."));
        if hovered == self.hovered_color && text == self.text {
            return;
        }
        self.hovered_color = hovered;
        if hovered.is_some() {
            self.hovered_tile = None;
            self.editor_hovered = false;
            self.text = text;
        } else {
            self.text = None;
        }
    }

    pub(crate) fn update_tile_hover(
        &mut self,
        responses: &[egui::Response],
        first_index: usize,
        modifiers: egui::Modifiers,
        owner: Option<GraphicsTileOwner>,
        pointer_moved: bool,
    ) {
        let hovered = responses
            .iter()
            .position(egui::Response::hovered)
            .and_then(|index| first_index.checked_add(index));
        if hovered == self.hovered_tile && (hovered.is_none() || !pointer_moved) {
            return;
        }
        let text = hovered.map(|index| tile_hover_status(index, modifiers, owner));
        if hovered == self.hovered_tile && text == self.text {
            return;
        }
        self.hovered_tile = hovered;
        if hovered.is_some() {
            self.editor_hovered = false;
            self.hovered_color = None;
            self.text = text;
        } else {
            self.text = None;
        }
    }

    pub(crate) fn update_pixel_editor_hover(
        &mut self,
        hovered: bool,
        selected: usize,
        pointer_moved: bool,
    ) {
        if hovered == self.editor_hovered && (!hovered || !pointer_moved) {
            return;
        }
        let text = hovered.then(|| format!("Tile 0x{selected:X} selected for editing."));
        if hovered == self.editor_hovered && text == self.text {
            return;
        }
        self.editor_hovered = hovered;
        if hovered {
            self.hovered_tile = None;
            self.hovered_color = None;
            self.text = text;
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
pub(crate) enum TilePixelPointerAction {
    PaintForeground,
    PaintBackground,
    PickForeground,
    PickBackground,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PalettePointerAction {
    SelectForeground,
    SelectBackground,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TilePixelPointerCapture {
    #[default]
    None,
    SampleForeground,
    SampleBackground,
    PaintForeground,
    PaintBackground,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GraphicsTileGridColor {
    White,
    Black,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GraphicsTileGrid {
    visible: bool,
    color: GraphicsTileGridColor,
}

impl Default for GraphicsTileGrid {
    fn default() -> Self {
        Self {
            visible: false,
            color: GraphicsTileGridColor::White,
        }
    }
}

impl GraphicsTileGrid {
    fn color(self) -> Option<egui::Color32> {
        self.visible.then_some(match self.color {
            GraphicsTileGridColor::White => egui::Color32::WHITE,
            GraphicsTileGridColor::Black => egui::Color32::BLACK,
        })
    }

    pub(crate) fn apply_f8(&mut self, modifiers: egui::Modifiers) -> Option<&'static str> {
        if modifiers.ctrl && modifiers.alt {
            self.color = match self.color {
                GraphicsTileGridColor::White => GraphicsTileGridColor::Black,
                GraphicsTileGridColor::Black => GraphicsTileGridColor::White,
            };
            Some(match self.color {
                GraphicsTileGridColor::White => "Tile grid color 1.",
                GraphicsTileGridColor::Black => "Tile grid color 2.",
            })
        } else {
            self.visible = !self.visible;
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GraphicsCharacterShortcut {
    ApplyColorMap,
    EditColorMap,
    RotateClockwise,
    FlipHorizontal,
    FlipVertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GraphicsTileTransform {
    RotateClockwise,
    FlipHorizontal,
    FlipVertical,
}

pub(crate) fn graphics_transform_controls(
    ui: &mut egui::Ui,
    enabled: bool,
    catalog: Option<&LocalizationCatalog>,
) -> Option<GraphicsTileTransform> {
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(graphics_text(
                    catalog,
                    ExtendedUiTextKey::GraphicsRotateClockwise,
                )),
            )
            .clicked()
        {
            Some(GraphicsTileTransform::RotateClockwise)
        } else if ui
            .add_enabled(
                enabled,
                egui::Button::new(graphics_text(
                    catalog,
                    ExtendedUiTextKey::GraphicsFlipHorizontal,
                )),
            )
            .clicked()
        {
            Some(GraphicsTileTransform::FlipHorizontal)
        } else if ui
            .add_enabled(
                enabled,
                egui::Button::new(graphics_text(
                    catalog,
                    ExtendedUiTextKey::GraphicsFlipVertical,
                )),
            )
            .clicked()
        {
            Some(GraphicsTileTransform::FlipVertical)
        } else {
            None
        }
    })
    .inner
}

pub(crate) fn graphics_navigation_controls(
    ui: &mut egui::Ui,
    pages_enabled: bool,
    palettes_enabled: bool,
    catalog: Option<&LocalizationCatalog>,
) -> (Option<TileNavigation>, Option<PaletteStep>) {
    ui.horizontal(|ui| {
        let page = if ui
            .add_enabled(
                pages_enabled,
                egui::Button::new(graphics_text(
                    catalog,
                    ExtendedUiTextKey::GraphicsPreviousPage,
                )),
            )
            .clicked()
        {
            Some(TileNavigation::PreviousPage)
        } else if ui
            .add_enabled(
                pages_enabled,
                egui::Button::new(graphics_text(catalog, ExtendedUiTextKey::GraphicsNextPage)),
            )
            .clicked()
        {
            Some(TileNavigation::NextPage)
        } else {
            None
        };
        ui.separator();
        let palette = if ui
            .add_enabled(
                palettes_enabled,
                egui::Button::new(graphics_text(
                    catalog,
                    ExtendedUiTextKey::GraphicsPreviousPalette,
                )),
            )
            .clicked()
        {
            Some(PaletteStep::Previous)
        } else if ui
            .add_enabled(
                palettes_enabled,
                egui::Button::new(graphics_text(
                    catalog,
                    ExtendedUiTextKey::GraphicsNextPalette,
                )),
            )
            .clicked()
        {
            Some(PaletteStep::Next)
        } else {
            None
        };
        (page, palette)
    })
    .inner
}

pub(crate) const fn shortcut_transform(
    shortcut: Option<GraphicsCharacterShortcut>,
) -> Option<GraphicsTileTransform> {
    match shortcut {
        Some(GraphicsCharacterShortcut::RotateClockwise) => {
            Some(GraphicsTileTransform::RotateClockwise)
        }
        Some(GraphicsCharacterShortcut::FlipHorizontal) => {
            Some(GraphicsTileTransform::FlipHorizontal)
        }
        Some(GraphicsCharacterShortcut::FlipVertical) => Some(GraphicsTileTransform::FlipVertical),
        _ => None,
    }
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
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<IndexedTile> {
        let mut apply = false;
        ui.horizontal(|ui| {
            if ui
                .button(graphics_text(
                    catalog,
                    ExtendedUiTextKey::GraphicsColorMapFilters,
                ))
                .clicked()
            {
                self.begin_dialog();
            }
            apply = ui
                .add_enabled(
                    apply_enabled,
                    egui::Button::new(graphics_text(
                        catalog,
                        ExtendedUiTextKey::GraphicsApplyColorMapFilter,
                    )),
                )
                .clicked();
            ui.monospace(
                graphics_text(catalog, ExtendedUiTextKey::GraphicsFilterFormat)
                    .replace("{filter}", &format!("{:X}", self.selected_filter)),
            );
        });
        self.show_dialog(ui.ctx(), palette, display_palette, catalog);
        apply
            .then(|| self.filters.apply(self.selected_filter, tile))
            .flatten()
    }

    fn show_dialog(
        &mut self,
        context: &egui::Context,
        palette: &PaletteInterchangeFile,
        display_palette: GraphicsDisplayPalette,
        catalog: Option<&LocalizationCatalog>,
    ) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let mut window_open = true;
        let mut accepted = false;
        let mut cancelled = false;
        let mut selected_filter = self.selected_filter;
        egui::Window::new(color_map_dialog_title(catalog))
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
                    catalog,
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

fn graphics_text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

fn show_color_map_dialog_contents(
    ui: &mut egui::Ui,
    dialog: &mut ColorMapDialog,
    palette: &PaletteInterchangeFile,
    display_palette: GraphicsDisplayPalette,
    selected_filter: &mut usize,
    accepted: &mut bool,
    cancelled: &mut bool,
    catalog: Option<&LocalizationCatalog>,
) {
    egui::ComboBox::from_label(color_map_dialog_text(
        catalog,
        0x19c,
        "Color Map Number to use",
    ))
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
    ui.label(color_map_dialog_text(
        catalog,
        0x1df,
        "Original to Mapped Color",
    ));
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
    ui.label(color_map_dialog_text(
        catalog,
        0x67,
        "Original Colors to Mapped Colors",
    ));
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
    ui.label(
        color_map_dialog_text(catalog, 0x6a, "Color: 0")
            .replace('0', &format!("{:X}", dialog.source)),
    );
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
        if ui
            .button(color_map_dialog_text(catalog, 6, "Reset"))
            .clicked()
        {
            let _ = dialog.draft.reset(*selected_filter);
        }
        if ui
            .button(color_map_dialog_text(catalog, 2, "Cancel"))
            .clicked()
        {
            *cancelled = true;
        }
        if ui.button(color_map_dialog_text(catalog, 1, "OK")).clicked() {
            *accepted = true;
        }
    });
}

fn color_map_dialog_title(catalog: Option<&LocalizationCatalog>) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_title(ORIGINAL_COLOR_MAP_DIALOG_ID))
        .unwrap_or("Create Filter to Remap Colors in Tile")
        .to_owned()
}

fn color_map_dialog_text(
    catalog: Option<&LocalizationCatalog>,
    control_id: u32,
    fallback: &str,
) -> String {
    catalog
        .and_then(|catalog| {
            catalog.original_dialog_control_text(ORIGINAL_COLOR_MAP_DIALOG_ID, control_id)
        })
        .unwrap_or(fallback)
        .to_owned()
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
    grid: GraphicsTileGrid,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::splat(TILE_SHEET_CELL_SIDE),
        egui::Sense::click(),
    );
    paint_tile(ui.painter(), rect, tile, palette, display_palette);
    if let Some(color) = grid.color() {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.right() - 1.0, rect.top()),
                rect.right_bottom(),
            ),
            0.0,
            color,
        );
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - 1.0),
                rect.right_bottom(),
            ),
            0.0,
            color,
        );
    }
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

pub(crate) fn take_tile_grid_shortcut(
    ui: &mut egui::Ui,
    selected: usize,
    responses: &[egui::Response],
    grid: &mut GraphicsTileGrid,
) -> Option<&'static str> {
    if !responses
        .get(selected % GRAPHICS_PAGE_TILES)
        .is_some_and(egui::Response::has_focus)
    {
        return None;
    }
    ui.input_mut(|input| {
        let modifiers = input.modifiers;
        input
            .consume_key(modifiers, egui::Key::F8)
            .then(|| grid.apply_f8(modifiers))
            .flatten()
    })
}

pub(crate) fn tile_pointer_action(
    response: &egui::Response,
    index: usize,
) -> Option<TilePointerAction> {
    let contains_pointer = response.contains_pointer();
    response.ctx.input(|input| {
        classify_tile_pointer_action(
            index,
            contains_pointer && input.pointer.button_pressed(egui::PointerButton::Primary),
            contains_pointer && input.pointer.button_pressed(egui::PointerButton::Secondary),
            input.modifiers,
        )
    })
}

pub(crate) fn tile_pixel_pointer_action(
    response: &egui::Response,
    modifiers: egui::Modifiers,
    capture: &mut TilePixelPointerCapture,
) -> Option<TilePixelPointerAction> {
    let contains_pointer = response.contains_pointer();
    let pointer = response.ctx.input(|input| TilePixelPointerInput {
        contains_pointer,
        primary: TilePixelButtonState::read(input, egui::PointerButton::Primary),
        secondary: TilePixelButtonState::read(input, egui::PointerButton::Secondary),
        control: modifiers.ctrl,
    });
    classify_tile_pixel_pointer_transition(capture, pointer)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TilePixelButtonState {
    pressed: bool,
    down: bool,
    released: bool,
}

impl TilePixelButtonState {
    fn read(input: &egui::InputState, button: egui::PointerButton) -> Self {
        Self {
            pressed: input.pointer.button_pressed(button),
            down: input.pointer.button_down(button),
            released: input.pointer.button_released(button),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TilePixelPointerInput {
    contains_pointer: bool,
    primary: TilePixelButtonState,
    secondary: TilePixelButtonState,
    control: bool,
}

fn classify_tile_pixel_pointer_transition(
    capture: &mut TilePixelPointerCapture,
    input: TilePixelPointerInput,
) -> Option<TilePixelPointerAction> {
    if input.contains_pointer && input.primary.pressed {
        return if input.control {
            *capture = TilePixelPointerCapture::SampleForeground;
            Some(TilePixelPointerAction::PickForeground)
        } else {
            *capture = TilePixelPointerCapture::PaintForeground;
            Some(TilePixelPointerAction::PaintForeground)
        };
    }
    if input.contains_pointer && input.secondary.pressed {
        return if input.control {
            *capture = TilePixelPointerCapture::SampleBackground;
            Some(TilePixelPointerAction::PickBackground)
        } else {
            *capture = TilePixelPointerCapture::PaintBackground;
            Some(TilePixelPointerAction::PaintBackground)
        };
    }
    let captured_button_released =
        match *capture {
            TilePixelPointerCapture::SampleForeground
            | TilePixelPointerCapture::PaintForeground => input.primary.released,
            TilePixelPointerCapture::SampleBackground
            | TilePixelPointerCapture::PaintBackground => input.secondary.released,
            TilePixelPointerCapture::None => false,
        };
    if captured_button_released {
        *capture = TilePixelPointerCapture::None;
        return None;
    }
    match *capture {
        TilePixelPointerCapture::PaintForeground if input.primary.down => {
            Some(TilePixelPointerAction::PaintForeground)
        }
        TilePixelPointerCapture::PaintBackground if input.secondary.down => {
            Some(TilePixelPointerAction::PaintBackground)
        }
        TilePixelPointerCapture::SampleForeground if input.primary.down => None,
        TilePixelPointerCapture::SampleBackground if input.secondary.down => None,
        TilePixelPointerCapture::None => None,
        _ => {
            *capture = TilePixelPointerCapture::None;
            None
        }
    }
}

pub(crate) fn color_selection_marker(color: u8, foreground: u8, background: u8) -> &'static str {
    match (color == foreground, color == background) {
        (true, true) => "F/B",
        (true, false) => "F",
        (false, true) => "B",
        (false, false) => "",
    }
}

pub(crate) fn palette_pointer_action(response: &egui::Response) -> Option<PalettePointerAction> {
    let contains_pointer = response.contains_pointer();
    response.ctx.input(|input| {
        classify_palette_pointer_action(
            contains_pointer,
            input.pointer.button_pressed(egui::PointerButton::Primary),
            input.pointer.button_pressed(egui::PointerButton::Secondary),
        )
    })
}

fn classify_palette_pointer_action(
    contains_pointer: bool,
    primary_pressed: bool,
    secondary_pressed: bool,
) -> Option<PalettePointerAction> {
    if !contains_pointer {
        None
    } else if primary_pressed {
        Some(PalettePointerAction::SelectForeground)
    } else if secondary_pressed {
        Some(PalettePointerAction::SelectBackground)
    } else {
        None
    }
}

pub(crate) fn take_graphics_save_shortcut(ui: &mut egui::Ui) -> bool {
    ui.input_mut(|input| {
        !input.modifiers.any() && input.consume_key(egui::Modifiers::NONE, egui::Key::F9)
    })
}

pub(crate) fn take_graphics_refresh_shortcut(ui: &mut egui::Ui) -> bool {
    let taken = ui.input_mut(|input| {
        let modifiers = input.modifiers;
        input.consume_key(modifiers, egui::Key::F1)
    });
    if taken {
        ui.ctx().request_repaint();
    }
    taken
}

fn classify_tile_pointer_action(
    index: usize,
    primary: bool,
    secondary: bool,
    modifiers: egui::Modifiers,
) -> Option<TilePointerAction> {
    if secondary {
        if modifiers.ctrl {
            Some(TilePointerAction::PasteClipboard(index))
        } else {
            Some(TilePointerAction::PasteSelected(index))
        }
    } else if primary {
        if modifiers.ctrl && !modifiers.shift && !modifiers.alt {
            Some(TilePointerAction::Copy(index))
        } else {
            Some(TilePointerAction::Select(index))
        }
    } else {
        None
    }
}

fn tile_hover_status(
    index: usize,
    modifiers: egui::Modifiers,
    owner: Option<GraphicsTileOwner>,
) -> String {
    if modifiers.ctrl && modifiers.shift {
        match owner {
            Some(GraphicsTileOwner::OriginalAnimation { slot }) => {
                format!("Tile 0x{index:X}, OrigAnim slot 0x{slot:X}.")
            }
            Some(GraphicsTileOwner::LevelExAnimation { slot }) => {
                format!("Tile 0x{index:X}, ExAnim Level slot 0x{slot:X}.")
            }
            Some(GraphicsTileOwner::GlobalExAnimation { slot }) => {
                format!("Tile 0x{index:X}, ExAnim Global slot 0x{slot:X}.")
            }
            _ => format!("Tile 0x{index:X}."),
        }
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
    tile_count: usize,
    tile_shift_enabled: bool,
) -> Option<String> {
    let Some(response) = responses.get(*selected % GRAPHICS_PAGE_TILES) else {
        return None;
    };
    if !response.has_focus() {
        return None;
    }
    let navigation = ui.input_mut(|input| {
        const KEYS: [(egui::Key, TileNavigation); 2] = [
            (egui::Key::ArrowUp, TileNavigation::PreviousPage),
            (egui::Key::ArrowDown, TileNavigation::NextPage),
        ];
        KEYS.into_iter().find_map(|(key, navigation)| {
            let modifiers = input.modifiers;
            (!native_vertical_tile_shift(modifiers, tile_shift_enabled)
                && input.consume_key(modifiers, key))
            .then_some(navigation)
        })
    });
    let Some(navigation) = navigation else {
        return None;
    };
    apply_tile_navigation(selected, responses, tile_count, navigation)
}

pub(crate) fn apply_tile_navigation(
    selected: &mut usize,
    responses: &[egui::Response],
    tile_count: usize,
    navigation: TileNavigation,
) -> Option<String> {
    let next = navigated_tile_index(*selected, tile_count, navigation);
    let page = *selected / GRAPHICS_PAGE_TILES;
    if next == *selected {
        return Some(match navigation {
            TileNavigation::PreviousPage => format!("Already at Start (0x{page:X})."),
            TileNavigation::NextPage => format!("Already at End (0x{page:X})."),
        });
    }
    let Some(response) = responses.get(next % GRAPHICS_PAGE_TILES) else {
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
            .get(selected % GRAPHICS_PAGE_TILES)
            .is_some_and(egui::Response::has_focus)
    {
        return None;
    }
    let step = ui.input_mut(|input| {
        let modifiers = input.modifiers;
        if input.consume_key(modifiers, egui::Key::PageUp) {
            Some(PaletteStep::Next)
        } else if !(modifiers.ctrl && modifiers.shift)
            && input.consume_key(modifiers, egui::Key::PageDown)
        {
            Some(PaletteStep::Previous)
        } else {
            None
        }
    });
    apply_tile_palette_step(display_palette, row_count, step?)
}

pub(crate) fn take_internal_graphics_cache_unlock(
    ui: &mut egui::Ui,
    selected: usize,
    responses: &[egui::Response],
) -> bool {
    if !responses
        .get(selected % GRAPHICS_PAGE_TILES)
        .is_some_and(egui::Response::has_focus)
    {
        return false;
    }
    ui.input_mut(|input| {
        let modifiers = input.modifiers;
        modifiers.ctrl && modifiers.shift && input.consume_key(modifiers, egui::Key::PageDown)
    })
}

pub(crate) fn apply_tile_palette_step(
    display_palette: &mut GraphicsDisplayPalette,
    row_count: usize,
    step: PaletteStep,
) -> Option<String> {
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
            .get(selected % GRAPHICS_PAGE_TILES)
            .is_some_and(egui::Response::has_focus)
    {
        return None;
    }
    ui.input_mut(|input| {
        let modifiers = input.modifiers;
        if !modifiers.shift {
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
            let allowed = matches!(shift, TileShift::Left | TileShift::Right)
                || native_vertical_tile_shift(modifiers, enabled);
            (allowed && input.consume_key(modifiers, key)).then_some(shift)
        })
    })
}

fn native_vertical_tile_shift(modifiers: egui::Modifiers, enabled: bool) -> bool {
    enabled && modifiers.shift && !modifiers.ctrl && !modifiers.alt
}

pub(crate) fn take_graphics_character_shortcut(
    ui: &mut egui::Ui,
    selected: usize,
    responses: &[egui::Response],
) -> Option<GraphicsCharacterShortcut> {
    if !responses
        .get(selected % GRAPHICS_PAGE_TILES)
        .is_some_and(egui::Response::has_focus)
    {
        return None;
    }
    ui.input_mut(|input| {
        let modifiers = input.modifiers;
        if modifiers.ctrl || modifiers.alt || modifiers.command || modifiers.mac_cmd {
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
        .find_map(|(key, shortcut)| input.consume_key(modifiers, key).then_some(shortcut))
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

pub(crate) fn tile_page_range(selected: usize, tile_count: usize) -> std::ops::Range<usize> {
    let selected = selected.min(tile_count.saturating_sub(1));
    let start = selected / GRAPHICS_PAGE_TILES * GRAPHICS_PAGE_TILES;
    start..start.saturating_add(GRAPHICS_PAGE_TILES).min(tile_count)
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
                responses[*selected % GRAPHICS_PAGE_TILES].request_focus();
            }
            apply_tile_keyboard_navigation(ui, selected, &responses, TEST_GRID_TILES, true);
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
                responses[*selected % GRAPHICS_PAGE_TILES].request_focus();
            }
            status =
                apply_tile_keyboard_navigation(ui, selected, &responses, TEST_GRID_TILES, true);
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

    fn pixel_pointer_input(
        primary: (bool, bool, bool),
        secondary: (bool, bool, bool),
        control: bool,
    ) -> TilePixelPointerInput {
        TilePixelPointerInput {
            contains_pointer: true,
            primary: TilePixelButtonState {
                pressed: primary.0,
                down: primary.1,
                released: primary.2,
            },
            secondary: TilePixelButtonState {
                pressed: secondary.0,
                down: secondary.1,
                released: secondary.2,
            },
            control,
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
        assert_eq!(tile_page_range(0, 0), 0..0);
        assert_eq!(tile_page_range(9, 600), 0..256);
        assert_eq!(tile_page_range(265, 600), 256..512);
        assert_eq!(tile_page_range(599, 600), 512..600);
        assert_eq!(tile_page_range(usize::MAX, 600), 512..600);
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
    fn visible_page_commands_share_native_keyboard_status_and_bounds() {
        let context = egui::Context::default();
        let mut selected = 9;
        let mut statuses = Vec::new();
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let responses = (0..10)
                    .map(|index| ui.button(index.to_string()))
                    .collect::<Vec<_>>();
                statuses.push(apply_tile_navigation(
                    &mut selected,
                    &responses,
                    600,
                    TileNavigation::NextPage,
                ));
                statuses.push(apply_tile_navigation(
                    &mut selected,
                    &responses,
                    600,
                    TileNavigation::PreviousPage,
                ));
                statuses.push(apply_tile_navigation(
                    &mut selected,
                    &responses,
                    600,
                    TileNavigation::PreviousPage,
                ));
            });
        });
        assert_eq!(selected, 9);
        assert_eq!(
            statuses,
            [
                Some("Viewing 8x8 page 0x1.".into()),
                Some("Viewing 8x8 page 0x0.".into()),
                Some("Already at Start (0x0).".into()),
            ]
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

        let mut selected = Default;
        assert_eq!(
            apply_tile_palette_step(&mut selected, 8, PaletteStep::Next),
            Some("Rendered with palette 0x0.".into())
        );
        assert_eq!(selected, Row(0));
        assert_eq!(
            apply_tile_palette_step(&mut selected, 8, PaletteStep::Previous),
            Some("Rendered with default palette.".into())
        );
        assert_eq!(selected, Default);
        assert_eq!(
            apply_tile_palette_step(&mut selected, 8, PaletteStep::Previous),
            None
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
    fn pointer_gestures_follow_native_control_fallback_routing() {
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
        for modifiers in [
            egui::Modifiers::SHIFT,
            egui::Modifiers::ALT,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Modifiers::CTRL | egui::Modifiers::ALT,
        ] {
            assert_eq!(
                classify_tile_pointer_action(7, true, false, modifiers),
                Some(TilePointerAction::Select(7))
            );
        }
        for modifiers in [egui::Modifiers::SHIFT, egui::Modifiers::ALT] {
            assert_eq!(
                classify_tile_pointer_action(8, false, true, modifiers),
                Some(TilePointerAction::PasteSelected(8))
            );
        }
        for modifiers in [
            egui::Modifiers::CTRL,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Modifiers::CTRL | egui::Modifiers::ALT,
        ] {
            assert_eq!(
                classify_tile_pointer_action(9, false, true, modifiers),
                Some(TilePointerAction::PasteClipboard(9))
            );
        }
        assert_eq!(
            classify_tile_pointer_action(10, false, false, egui::Modifiers::NONE),
            None
        );
        assert_eq!(
            classify_tile_pointer_action(11, true, true, egui::Modifiers::NONE),
            Some(TilePointerAction::PasteSelected(11))
        );
    }

    #[test]
    fn tile_pointer_adapter_dispatches_on_press_before_release() {
        let context = egui::Context::default();
        let mut rect = egui::Rect::NOTHING;
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                rect = ui.button("tile").rect;
            });
        });

        let mut action = None;
        let modifiers = egui::Modifiers::CTRL;
        let _ = context.run(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers,
                    },
                ],
                modifiers,
                ..egui::RawInput::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    action = tile_pointer_action(&ui.button("tile"), 12);
                });
            },
        );
        assert_eq!(action, Some(TilePointerAction::Copy(12)));
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
    fn graphics_refresh_shortcut_accepts_every_native_modifier_form() {
        for modifiers in [
            egui::Modifiers::NONE,
            egui::Modifiers::SHIFT,
            egui::Modifiers::CTRL,
            egui::Modifiers::ALT,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        ] {
            let context = egui::Context::default();
            let mut taken = false;
            let _ = context.run(
                egui::RawInput {
                    events: vec![key_event(egui::Key::F1, modifiers)],
                    modifiers,
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        taken = take_graphics_refresh_shortcut(ui);
                    });
                },
            );
            assert!(taken, "F1 was not consumed for {modifiers:?}");
        }
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
    fn original_color_map_template_localizes_matching_controls_and_round_trips() {
        use lm_app::{OriginalDialogTextKey, UiTextKey};

        let catalog = LocalizationCatalog::new(
            "fr-test",
            UiTextKey::ALL.map(|key| (key, key.english().to_owned())),
        )
        .unwrap()
        .with_original_dialog_texts([
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_COLOR_MAP_DIALOG_ID,
                    item_index: u16::MAX,
                    control_id: u32::MAX,
                },
                "Créer un filtre de couleurs".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_COLOR_MAP_DIALOG_ID,
                    item_index: 1,
                    control_id: 0x19c,
                },
                "Numéro de palette à utiliser".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_COLOR_MAP_DIALOG_ID,
                    item_index: 2,
                    control_id: 6,
                },
                "Réinitialiser".into(),
            ),
        ])
        .unwrap();

        assert_eq!(
            color_map_dialog_title(Some(&catalog)),
            "Créer un filtre de couleurs"
        );
        assert_eq!(
            color_map_dialog_text(Some(&catalog), 0x19c, "fallback"),
            "Numéro de palette à utiliser"
        );
        assert_eq!(
            color_map_dialog_text(Some(&catalog), 6, "Reset"),
            "Réinitialiser"
        );
        assert_eq!(color_map_dialog_text(Some(&catalog), 2, "Cancel"), "Cancel");
        assert_eq!(
            color_map_dialog_title(None),
            "Create Filter to Remap Colors in Tile"
        );

        let reopened = LocalizationCatalog::decode(&catalog.encode().unwrap()).unwrap();
        assert_eq!(
            color_map_dialog_title(Some(&reopened)),
            "Créer un filtre de couleurs"
        );
    }

    #[test]
    fn native_tile_status_formats_address_modifier_and_actions_exactly() {
        assert_eq!(
            tile_hover_status(0x1f, egui::Modifiers::NONE, None),
            "Tile 0x1F (Address 0x3E0)"
        );
        assert_eq!(
            tile_hover_status(0x1f, egui::Modifiers::CTRL | egui::Modifiers::SHIFT, None),
            "Tile 0x1F."
        );
        assert_eq!(
            tile_hover_status(
                0x1f,
                egui::Modifiers::CTRL | egui::Modifiers::SHIFT | egui::Modifiers::ALT,
                None
            ),
            "Tile 0x1F."
        );
        for (owner, expected) in [
            (
                GraphicsTileOwner::OriginalAnimation { slot: 0x12 },
                "Tile 0x1F, OrigAnim slot 0x12.",
            ),
            (
                GraphicsTileOwner::LevelExAnimation { slot: 0x23 },
                "Tile 0x1F, ExAnim Level slot 0x23.",
            ),
            (
                GraphicsTileOwner::GlobalExAnimation { slot: 0x34 },
                "Tile 0x1F, ExAnim Global slot 0x34.",
            ),
        ] {
            for modifiers in [
                egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                egui::Modifiers::CTRL | egui::Modifiers::SHIFT | egui::Modifiers::ALT,
            ] {
                assert_eq!(tile_hover_status(0x1f, modifiers, Some(owner)), expected);
            }
        }
        let mut status = GraphicsEditorStatus::default();
        status.select_tile(0x123);
        assert_eq!(
            status.text.as_deref(),
            Some("Tile 0x123 selected for editing.")
        );
        status.select_foreground_color(0xe);
        assert_eq!(status.text.as_deref(), Some("Color E selected for FG."));
        status.select_background_color(0xa);
        assert_eq!(status.text.as_deref(), Some("Color A selected for BG."));
    }

    #[test]
    fn pixel_paint_capture_preserves_native_mouse_down_mode() {
        use TilePixelPointerAction::{PaintBackground, PaintForeground};

        let mut capture = TilePixelPointerCapture::None;
        assert_eq!(
            classify_tile_pixel_pointer_transition(
                &mut capture,
                pixel_pointer_input((true, true, false), (false, false, false), false),
            ),
            Some(PaintForeground)
        );
        assert_eq!(capture, TilePixelPointerCapture::PaintForeground);
        // Ctrl pressed after capture does not change the paint operation.
        assert_eq!(
            classify_tile_pixel_pointer_transition(
                &mut capture,
                pixel_pointer_input((false, true, false), (false, false, false), true),
            ),
            Some(PaintForeground)
        );
        // Releasing the unrelated button does not clear primary capture.
        assert_eq!(
            classify_tile_pixel_pointer_transition(
                &mut capture,
                pixel_pointer_input((false, true, false), (false, false, true), true),
            ),
            Some(PaintForeground)
        );
        assert_eq!(
            classify_tile_pixel_pointer_transition(
                &mut capture,
                pixel_pointer_input((false, false, true), (false, false, false), true),
            ),
            None
        );
        assert_eq!(capture, TilePixelPointerCapture::None);

        assert_eq!(
            classify_tile_pixel_pointer_transition(
                &mut capture,
                pixel_pointer_input((false, false, false), (true, true, false), false),
            ),
            Some(PaintBackground)
        );
        assert_eq!(capture, TilePixelPointerCapture::PaintBackground);
    }

    #[test]
    fn pixel_sample_capture_never_becomes_paint() {
        use TilePixelPointerAction::{PickBackground, PickForeground};

        let mut capture = TilePixelPointerCapture::None;
        assert_eq!(
            classify_tile_pixel_pointer_transition(
                &mut capture,
                pixel_pointer_input((true, true, false), (false, false, false), true),
            ),
            Some(PickForeground)
        );
        // Releasing Ctrl while the button remains down cannot start painting.
        assert_eq!(
            classify_tile_pixel_pointer_transition(
                &mut capture,
                pixel_pointer_input((false, true, false), (false, false, false), false),
            ),
            None
        );
        assert_eq!(
            classify_tile_pixel_pointer_transition(
                &mut capture,
                pixel_pointer_input((false, false, true), (false, false, false), false),
            ),
            None
        );
        assert_eq!(capture, TilePixelPointerCapture::None);

        assert_eq!(
            classify_tile_pixel_pointer_transition(
                &mut capture,
                pixel_pointer_input((false, false, false), (true, true, false), true),
            ),
            Some(PickBackground)
        );
    }

    #[test]
    fn palette_marks_foreground_background_and_shared_colors() {
        assert_eq!(color_selection_marker(1, 1, 0), "F");
        assert_eq!(color_selection_marker(0, 1, 0), "B");
        assert_eq!(color_selection_marker(4, 4, 4), "F/B");
        assert_eq!(color_selection_marker(7, 1, 0), "");
    }

    #[test]
    fn palette_pointer_actions_begin_on_native_button_down() {
        assert_eq!(
            classify_palette_pointer_action(true, true, false),
            Some(PalettePointerAction::SelectForeground)
        );
        assert_eq!(
            classify_palette_pointer_action(true, false, true),
            Some(PalettePointerAction::SelectBackground)
        );
        assert_eq!(classify_palette_pointer_action(true, false, false), None);
        assert_eq!(classify_palette_pointer_action(false, true, false), None);
        assert_eq!(
            classify_palette_pointer_action(true, true, true),
            Some(PalettePointerAction::SelectForeground)
        );
    }

    #[test]
    fn transient_status_follows_native_pointer_movement_boundaries() {
        let mut status = GraphicsEditorStatus::default();
        status.update_palette_hover(Some(3), true);
        assert_eq!(status.text.as_deref(), Some("Color 3."));
        status.update_tile_hover(&[], 0, egui::Modifiers::NONE, None, true);
        status.update_pixel_editor_hover(false, 0, true);
        assert_eq!(status.text.as_deref(), Some("Color 3."));
        status.select_foreground_color(3);
        status.update_palette_hover(Some(3), false);
        assert_eq!(status.text.as_deref(), Some("Color 3 selected for FG."));
        status.update_palette_hover(Some(3), true);
        assert_eq!(status.text.as_deref(), Some("Color 3."));
        status.update_palette_hover(None, true);
        assert_eq!(status.text, None);

        status.update_pixel_editor_hover(true, 0x2a, true);
        assert_eq!(
            status.text.as_deref(),
            Some("Tile 0x2A selected for editing.")
        );
        status.update_palette_hover(None, true);
        status.update_tile_hover(&[], 0, egui::Modifiers::NONE, None, true);
        assert_eq!(
            status.text.as_deref(),
            Some("Tile 0x2A selected for editing.")
        );
        status.select_foreground_color(5);
        status.update_pixel_editor_hover(true, 0x2a, false);
        assert_eq!(status.text.as_deref(), Some("Color 5 selected for FG."));
        status.update_pixel_editor_hover(true, 0x2a, true);
        assert_eq!(
            status.text.as_deref(),
            Some("Tile 0x2A selected for editing.")
        );
        status.update_pixel_editor_hover(false, 0x2a, true);
        assert_eq!(status.text, None);
    }

    #[test]
    fn same_tile_mouse_move_refreshes_attribution_but_stationary_pointer_does_not() {
        let context = egui::Context::default();
        let mut rect = egui::Rect::NOTHING;
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                rect = ui.button("tile").rect;
            });
        });
        let mut response = None;
        let _ = context.run(
            egui::RawInput {
                events: vec![egui::Event::PointerMoved(rect.center())],
                ..egui::RawInput::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    response = Some(ui.button("tile"));
                });
            },
        );
        let response = response.unwrap();
        let mut status = GraphicsEditorStatus::default();
        status.update_tile_hover(
            std::slice::from_ref(&response),
            0x20,
            egui::Modifiers::NONE,
            None,
            true,
        );
        assert_eq!(status.text(), Some("Tile 0x20 (Address 0x400)"));
        status.update_palette_hover(None, true);
        status.update_pixel_editor_hover(false, 0, true);
        assert_eq!(status.text(), Some("Tile 0x20 (Address 0x400)"));

        status.set("Copied tile to clipboard.");
        let attribution_modifiers = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
        let owner = Some(GraphicsTileOwner::OriginalAnimation { slot: 3 });
        status.update_tile_hover(
            std::slice::from_ref(&response),
            0x20,
            attribution_modifiers,
            owner,
            false,
        );
        assert_eq!(status.text(), Some("Copied tile to clipboard."));

        status.update_tile_hover(
            std::slice::from_ref(&response),
            0x20,
            attribution_modifiers,
            owner,
            true,
        );
        assert_eq!(status.text(), Some("Tile 0x20, OrigAnim slot 0x3."));
    }

    #[test]
    fn tile_editor_uses_the_native_fixed_logical_canvas() {
        assert_eq!(TILE_EDITOR_SIDE, 256.0);
        let rect = egui::Rect::from_min_size(
            egui::Pos2::new(5.0, 9.0),
            egui::Vec2::splat(TILE_EDITOR_SIDE),
        );
        assert_eq!(tile_coordinate(rect, rect.min), Some((0, 0)));
        assert_eq!(
            tile_coordinate(rect, rect.max - egui::Vec2::splat(0.01)),
            Some((7, 7))
        );
    }

    #[test]
    fn tile_sheet_geometry_and_f8_grid_state_match_native_defaults() {
        assert_eq!(TILE_GRID_COLUMNS, 16);
        assert_eq!(TILE_SHEET_CELL_SIDE, 16.0);
        assert_eq!(TILE_SHEET_CELL_SIDE * TILE_GRID_COLUMNS as f32, 256.0);

        let mut grid = GraphicsTileGrid::default();
        assert!(!grid.visible);
        assert_eq!(grid.color, GraphicsTileGridColor::White);
        assert_eq!(grid.apply_f8(egui::Modifiers::NONE), None);
        assert!(grid.visible);
        assert_eq!(grid.color(), Some(egui::Color32::WHITE));

        assert_eq!(
            grid.apply_f8(egui::Modifiers::CTRL | egui::Modifiers::ALT),
            Some("Tile grid color 2.")
        );
        assert!(grid.visible);
        assert_eq!(grid.color(), Some(egui::Color32::BLACK));
        assert_eq!(
            grid.apply_f8(egui::Modifiers::CTRL | egui::Modifiers::ALT | egui::Modifiers::SHIFT),
            Some("Tile grid color 1.")
        );
        assert_eq!(grid.color(), Some(egui::Color32::WHITE));

        assert_eq!(grid.apply_f8(egui::Modifiers::CTRL), None);
        assert!(!grid.visible);
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
    fn focused_grid_routes_native_asymmetric_shift_arrows() {
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
                apply_tile_keyboard_navigation(
                    ui,
                    &mut selected,
                    &responses,
                    TEST_GRID_TILES,
                    true,
                );
                shift = take_tile_shift(ui, selected, &responses, true);
            });
        });
        assert_eq!(selected, 9);
        assert_eq!(shift, Some(TileShift::Left));

        let control_shift = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
        let input = egui::RawInput {
            events: vec![key_event(egui::Key::ArrowRight, control_shift)],
            modifiers: control_shift,
            ..Default::default()
        };
        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let responses = (0..TEST_GRID_TILES)
                    .map(|index| ui.button(index.to_string()))
                    .collect::<Vec<_>>();
                shift = take_tile_shift(ui, selected, &responses, true);
            });
        });
        assert_eq!(shift, Some(TileShift::Right));

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
    fn modified_vertical_arrows_fall_back_to_native_page_navigation() {
        let context = egui::Context::default();
        let mut selected = 9;
        let _ = context.run(egui::RawInput::default(), |context| {
            render_keyboard_grid(context, &mut selected, true);
        });
        let control_shift = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
        let input = egui::RawInput {
            events: vec![key_event(egui::Key::ArrowDown, control_shift)],
            modifiers: control_shift,
            ..Default::default()
        };
        let mut status = None;
        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let responses = (0..TEST_GRID_TILES)
                    .map(|index| ui.button(index.to_string()))
                    .collect::<Vec<_>>();
                status = apply_tile_keyboard_navigation(
                    ui,
                    &mut selected,
                    &responses,
                    TEST_GRID_TILES,
                    true,
                );
            });
        });
        assert_eq!(selected, 0x109);
        assert_eq!(status.as_deref(), Some("Viewing 8x8 page 0x1."));

        let _ = context.run(egui::RawInput::default(), |context| {
            render_keyboard_grid(context, &mut selected, true);
        });
        let input = egui::RawInput {
            events: vec![key_event(egui::Key::ArrowUp, egui::Modifiers::SHIFT)],
            modifiers: egui::Modifiers::SHIFT,
            ..Default::default()
        };
        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let responses = (0..TEST_GRID_TILES)
                    .map(|index| ui.button(index.to_string()))
                    .collect::<Vec<_>>();
                status = apply_tile_keyboard_navigation(
                    ui,
                    &mut selected,
                    &responses,
                    TEST_GRID_TILES,
                    false,
                );
            });
        });
        assert_eq!(selected, 9);
        assert_eq!(status.as_deref(), Some("Viewing 8x8 page 0x0."));
    }

    #[test]
    fn focused_grid_routes_native_modified_page_keys_to_palette_rows() {
        let context = egui::Context::default();
        let mut selected = 9;
        let mut display_palette = GraphicsDisplayPalette::Row(6);
        let mut status = None;
        let _ = context.run(egui::RawInput::default(), |context| {
            render_keyboard_grid(context, &mut selected, true);
        });
        let input = egui::RawInput {
            events: vec![key_event(egui::Key::PageUp, egui::Modifiers::CTRL)],
            modifiers: egui::Modifiers::CTRL,
            ..Default::default()
        };
        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let responses = (0..TEST_GRID_TILES)
                    .map(|index| ui.button(index.to_string()))
                    .collect::<Vec<_>>();
                apply_tile_keyboard_navigation(
                    ui,
                    &mut selected,
                    &responses,
                    TEST_GRID_TILES,
                    true,
                );
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

        let _ = context.run(egui::RawInput::default(), |context| {
            render_keyboard_grid(context, &mut selected, true);
        });
        let control_shift = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
        let input = egui::RawInput {
            events: vec![key_event(egui::Key::PageDown, control_shift)],
            modifiers: control_shift,
            ..Default::default()
        };
        let mut unlocked = false;
        let _ = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let responses = (0..TEST_GRID_TILES)
                    .map(|index| ui.button(index.to_string()))
                    .collect::<Vec<_>>();
                status =
                    apply_tile_palette_keyboard(ui, selected, &responses, &mut display_palette, 8);
                unlocked = take_internal_graphics_cache_unlock(ui, selected, &responses);
            });
        });
        assert_eq!(display_palette, GraphicsDisplayPalette::Row(6));
        assert_eq!(status, None);
        assert!(unlocked);
    }

    #[test]
    fn focused_grid_routes_native_lowercase_and_uppercase_character_shortcuts() {
        let cases = [
            (egui::Key::D, GraphicsCharacterShortcut::ApplyColorMap),
            (egui::Key::M, GraphicsCharacterShortcut::EditColorMap),
            (egui::Key::R, GraphicsCharacterShortcut::RotateClockwise),
            (egui::Key::X, GraphicsCharacterShortcut::FlipHorizontal),
            (egui::Key::Y, GraphicsCharacterShortcut::FlipVertical),
        ];
        for (key, expected) in cases {
            for modifiers in [egui::Modifiers::NONE, egui::Modifiers::SHIFT] {
                let context = egui::Context::default();
                let mut selected = 0;
                let _ = context.run(egui::RawInput::default(), |context| {
                    render_keyboard_grid(context, &mut selected, true);
                });
                let mut actual = None;
                let _ = context.run(
                    egui::RawInput {
                        events: vec![key_event(key, modifiers)],
                        modifiers,
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
                assert_eq!(
                    actual,
                    Some(expected),
                    "key {key:?}, modifiers {modifiers:?}"
                );
            }
        }

        for modifiers in [
            egui::Modifiers::CTRL,
            egui::Modifiers::ALT,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        ] {
            let context = egui::Context::default();
            let mut selected = 0;
            let _ = context.run(egui::RawInput::default(), |context| {
                render_keyboard_grid(context, &mut selected, true);
            });
            let mut modified = None;
            let _ = context.run(
                egui::RawInput {
                    events: vec![key_event(egui::Key::X, modifiers)],
                    modifiers,
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
            assert_eq!(modified, None, "modifiers {modifiers:?}");
        }
    }

    #[test]
    fn native_transform_shortcuts_share_the_visible_transform_actions() {
        assert_eq!(
            shortcut_transform(Some(GraphicsCharacterShortcut::RotateClockwise)),
            Some(GraphicsTileTransform::RotateClockwise)
        );
        assert_eq!(
            shortcut_transform(Some(GraphicsCharacterShortcut::FlipHorizontal)),
            Some(GraphicsTileTransform::FlipHorizontal)
        );
        assert_eq!(
            shortcut_transform(Some(GraphicsCharacterShortcut::FlipVertical)),
            Some(GraphicsTileTransform::FlipVertical)
        );
        assert_eq!(
            shortcut_transform(Some(GraphicsCharacterShortcut::ApplyColorMap)),
            None
        );
        assert_eq!(
            shortcut_transform(Some(GraphicsCharacterShortcut::EditColorMap)),
            None
        );
        assert_eq!(shortcut_transform(None), None);
    }
}
