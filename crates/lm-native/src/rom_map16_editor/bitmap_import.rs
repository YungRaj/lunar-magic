use super::{Command, RomMap16Editor, egui};
use crate::{dialogs, document_loader::BoundedRead, rom_allocation::parse_search_range};
use lm_app::{
    DecodedMap16Bitmap, MAP16_BITMAP_MAX_DIMENSION, MAP16_BITMAP_MAX_PIXELS,
    MAP16_BITMAP_MAX_PNG_BYTES, NativeMap16BitmapImportSession,
    NativeMap16BitmapImportSessionRequest, decode_map16_bitmap_image,
};
use lm_graphics::{
    BitmapPaletteColorOptions, BitmapPaletteEntryState, BitmapPaletteReduction, Palette,
};
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PendingBitmapImport {
    revision: u64,
    level: usize,
    start_map16_tile: usize,
    extra_graphics: [Option<usize>; 2],
    palette_row: u8,
}

#[derive(Default)]
pub(super) struct BitmapClipboardLoader {
    running: Option<Receiver<Result<DecodedMap16Bitmap, String>>>,
}

impl BitmapClipboardLoader {
    pub(super) fn is_running(&self) -> bool {
        self.running.is_some()
    }

    fn start(&mut self) -> Result<(), String> {
        if self.running.is_some() {
            return Err("a clipboard bitmap load is already running".into());
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("lm-clipboard-bitmap-load".into())
            .spawn(move || {
                let result = arboard::Clipboard::new()
                    .map_err(|error| format!("could not open the system clipboard: {error}"))
                    .and_then(|mut clipboard| {
                        clipboard.get_image().map_err(|error| {
                            format!("clipboard does not contain an image: {error}")
                        })
                    })
                    .and_then(|image| {
                        decode_clipboard_rgba(image.width, image.height, image.bytes.as_ref())
                    });
                let _send_result = sender.send(result);
            })
            .map_err(|error| format!("could not create clipboard-image worker: {error}"))?;
        self.running = Some(receiver);
        Ok(())
    }

    fn show(&mut self, context: &egui::Context) -> Option<Result<DecodedMap16Bitmap, String>> {
        let completion = self.poll();
        if self.running.is_some() {
            egui::Window::new("Opening")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label("Reading clipboard bitmap");
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
        completion
    }

    fn poll(&mut self) -> Option<Result<DecodedMap16Bitmap, String>> {
        let receiver = self.running.as_ref()?;
        match receiver.try_recv() {
            Ok(result) => {
                self.running = None;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.running = None;
                Some(Err(
                    "clipboard-image worker stopped without reporting a result".into(),
                ))
            }
        }
    }
}

impl RomMap16Editor {
    pub(super) fn poll_bitmap_loader(&mut self, context: &egui::Context) -> Option<Command> {
        if let Some(completion) = self.bitmap_loader.show(context) {
            let pending = self.pending_bitmap_import.take();
            match completion.and_then(|loaded| {
                let pending = pending.ok_or("Map16 bitmap request is missing")?;
                let [(_, bytes)] = loaded.into_exact::<1>("Map16 bitmap")?;
                self.open_bitmap_session(&bytes, pending)
            }) {
                Ok(()) => {}
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(completion) = self.bitmap_clipboard_loader.show(context) {
            let pending = self.pending_bitmap_import.take();
            match completion.and_then(|bitmap| {
                let pending = pending.ok_or("clipboard Map16 bitmap request is missing")?;
                self.open_decoded_bitmap_session(bitmap, pending)
            }) {
                Ok(()) => {}
                Err(error) => self.error = Some(error),
            }
        }
        None
    }

    fn open_bitmap_session(
        &mut self,
        bytes: &[u8],
        pending: PendingBitmapImport,
    ) -> Result<(), String> {
        let bitmap = decode_map16_bitmap_image(bytes).map_err(|error| error.to_string())?;
        self.open_decoded_bitmap_session(bitmap, pending)
    }

    fn open_decoded_bitmap_session(
        &mut self,
        bitmap: DecodedMap16Bitmap,
        pending: PendingBitmapImport,
    ) -> Result<(), String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        validate_bitmap_revision(workspace.controller.revision(), pending.revision)?;
        let request = NativeMap16BitmapImportSessionRequest {
            level: pending.level,
            start_map16_tile: pending.start_map16_tile,
            extra_graphics: pending.extra_graphics,
            pixels: bitmap.pixels,
            width: bitmap.width,
            height: bitmap.height,
            palette_row: pending.palette_row,
        };
        let mut session = if let Some(profile) = workspace.profile.clone() {
            NativeMap16BitmapImportSession::new(workspace.snapshot.clone(), profile, request)
        } else {
            NativeMap16BitmapImportSession::new_smw_us_v1(workspace.snapshot.clone(), request)
        }
        .map_err(|error| error.to_string())?;
        let options = bitmap_options_for_session(
            self.bitmap_import_options.as_ref(),
            pending.start_map16_tile,
        );
        session
            .set_options(options.clone())
            .map_err(|error| error.to_string())?;
        self.bitmap_import_options = Some(options);
        self.bitmap_session = Some(session);
        self.bitmap_original_texture = None;
        self.bitmap_converted_texture = None;
        self.bitmap_preview_zoom = 1;
        self.bitmap_preview_scroll = egui::Vec2::ZERO;
        Ok(())
    }

    pub(super) fn bitmap_import_window(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> Option<Command> {
        self.bitmap_session.as_ref()?;
        let stale = self
            .workspace
            .as_ref()
            .is_none_or(|workspace| workspace.controller.revision() != project_revision);
        let mut accepted = false;
        let mut cancelled = false;
        egui::Window::new("Import Bitmap as Map16")
            .default_width(580.0)
            .collapsible(false)
            .show(context, |ui| {
                if stale {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "The ROM changed. Reopen the import before committing.",
                    );
                }
                self.bitmap_preview_textures(ui);
                let Some(session) = self.bitmap_session.as_mut() else {
                    return;
                };
                let mut options = session.preview().options();
                let mut use_blank_graphics = options.graphics.blank_tile.is_some();
                let mut changed = ui
                    .horizontal_wrapped(|ui| {
                        ui.checkbox(
                            &mut options.graphics.optimize_new_tiles,
                            "Optimize new 8×8 tiles",
                        )
                        .changed()
                            | ui.checkbox(
                                &mut options.graphics.reuse_existing_tiles,
                                "Reuse existing tiles",
                            )
                            .changed()
                            | ui.checkbox(
                                &mut options.use_reserved_map16_for_blank,
                                "Use reserved Map16 tile for blank blocks",
                            )
                            .changed()
                            | ui.checkbox(
                                &mut options.deduplicate_map16,
                                "Optimize 16×16 tiles",
                            )
                            .changed()
                            | ui.checkbox(&mut options.layer_priority, "Layer priority")
                                .changed()
                    })
                    .inner;
                let blank_toggle_changed = ui
                    .checkbox(
                        &mut use_blank_graphics,
                        "Use configured 8×8 tile for blank source tiles",
                    )
                    .changed();
                if blank_toggle_changed {
                    options.graphics.blank_tile = use_blank_graphics.then_some(0x0f8);
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.label("First 8×8 tile");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut options.graphics.allocation_start)
                                .range(0..=0x2ff)
                                .hexadecimal(3, false, true),
                        )
                        .changed();
                    if let Some(blank_tile) = options.graphics.blank_tile.as_mut() {
                        ui.label("Blank 8×8 tile");
                        changed |= ui
                            .add(
                                egui::DragValue::new(blank_tile)
                                    .range(0..=0x2ff)
                                    .hexadecimal(3, false, true),
                            )
                            .changed();
                    }
                    ui.label("First Map16 tile");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut options.map16_allocation_start)
                                .range(0..=0xffff)
                                .hexadecimal(4, false, true),
                        )
                        .changed();
                    ui.label("Reserved Map16 tile");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut options.reserved_map16_tile)
                                .range(0..=0xffff)
                                .hexadecimal(4, false, true),
                        )
                        .changed();
                });
                if options.color.is_none() {
                    options.color = Some(BitmapPaletteColorOptions::lunar_magic_initial());
                    changed = true;
                }
                changed |= bitmap_multi_row_color_options(
                    ui,
                    options
                        .color
                        .as_mut()
                        .expect("native bitmap imports always have color options"),
                    &session.preview().inputs().palette,
                );
                if changed {
                    match session.set_options(options.clone()) {
                        Ok(()) => {
                            self.bitmap_import_options = Some(options.clone());
                            self.bitmap_converted_texture = None;
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                let plan = session.preview().plan();
                ui.label(format!(
                    "{} generated colors; {} newly occupied 8×8 tiles",
                    plan.generated_colors, plan.newly_occupied_tiles
                ));
                match session.map16_allocation() {
                    Ok(allocation) => {
                        ui.label(format!(
                            "{} source blocks placed using {} new 16×16 tiles",
                            allocation.assignments.len(),
                            allocation.allocated_definitions
                        ));
                        if allocation.exhausted {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "Not enough blank 16×16 tiles; only the reported prefix will be imported.",
                            );
                        }
                    }
                    Err(error) => {
                        ui.colored_label(egui::Color32::RED, error.to_string());
                    }
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!stale, egui::Button::new("Import into ROM"))
                        .clicked()
                    {
                        accepted = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                });
            });
        if cancelled {
            self.clear_bitmap_session();
            return None;
        }
        if !accepted {
            return None;
        }
        let search = match parse_search_range(&self.search_start, &self.search_end) {
            Ok(search) => search,
            Err(error) => {
                self.error = Some(error);
                return None;
            }
        };
        match self
            .bitmap_session
            .as_ref()
            .expect("accepted session remains open")
            .prepare_commit(search)
        {
            Ok(prepared) => Some(prepared.into_command()),
            Err(error) => {
                self.error = Some(error.to_string());
                None
            }
        }
    }

    fn bitmap_preview_textures(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.bitmap_session.as_ref() else {
            return;
        };
        let width = session.preview().inputs().width;
        let height = session.preview().inputs().height;
        if self.bitmap_original_texture.is_none() {
            self.bitmap_original_texture = Some(ui.ctx().load_texture(
                "map16-bitmap-original",
                rgba_image(session.preview().original_pixels(), width, height),
                egui::TextureOptions::NEAREST,
            ));
        }
        if self.bitmap_converted_texture.is_none() {
            self.bitmap_converted_texture = Some(ui.ctx().load_texture(
                "map16-bitmap-converted",
                rgba_image(session.preview().converted_pixels(), width, height),
                egui::TextureOptions::NEAREST,
            ));
        }
        ui.horizontal(|ui| {
            ui.label("Preview zoom");
            ui.add(
                egui::Slider::new(&mut self.bitmap_preview_zoom, 1..=8)
                    .integer()
                    .suffix("×"),
            );
            if ui.button("Reset pan").clicked() {
                self.bitmap_preview_scroll = egui::Vec2::ZERO;
            }
        });
        let image_size = preview_size(width, height, self.bitmap_preview_zoom);
        let mut next_scroll = self.bitmap_preview_scroll;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Original");
                if let Some(texture) = &self.bitmap_original_texture {
                    let output = egui::ScrollArea::both()
                        .id_salt("map16-bitmap-original-preview")
                        .max_width(272.0)
                        .max_height(272.0)
                        .horizontal_scroll_offset(self.bitmap_preview_scroll.x)
                        .vertical_scroll_offset(self.bitmap_preview_scroll.y)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(egui::Image::new(texture).fit_to_exact_size(image_size));
                        });
                    if output.inner_rect.contains(
                        ui.ctx()
                            .pointer_hover_pos()
                            .unwrap_or(egui::Pos2::new(f32::MIN, f32::MIN)),
                    ) {
                        next_scroll = output.state.offset;
                    }
                }
            });
            ui.vertical(|ui| {
                ui.label("Converted");
                if let Some(texture) = &self.bitmap_converted_texture {
                    let output = egui::ScrollArea::both()
                        .id_salt("map16-bitmap-converted-preview")
                        .max_width(272.0)
                        .max_height(272.0)
                        .horizontal_scroll_offset(self.bitmap_preview_scroll.x)
                        .vertical_scroll_offset(self.bitmap_preview_scroll.y)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(egui::Image::new(texture).fit_to_exact_size(image_size));
                        });
                    if output.inner_rect.contains(
                        ui.ctx()
                            .pointer_hover_pos()
                            .unwrap_or(egui::Pos2::new(f32::MIN, f32::MIN)),
                    ) {
                        next_scroll = output.state.offset;
                    }
                }
            });
        });
        self.bitmap_preview_scroll = next_scroll;
    }

    pub(super) fn bitmap_import_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        project_revision: u64,
    ) {
        ui.separator();
        ui.heading("Bitmap to Map16");
        ui.label("The preview level and its real object tileset are used.");
        let busy = self.bitmap_loader.is_running()
            || self.bitmap_clipboard_loader.is_running()
            || self.bitmap_session.is_some();
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                ui.label("Editable GFX slot 4");
                ui.text_edit_singleline(&mut self.bitmap_extra_slot_4);
                ui.label("slot 5");
                ui.text_edit_singleline(&mut self.bitmap_extra_slot_5);
            });
        });
        ui.small("Enter hexadecimal GFX/ExGFX file numbers. Blank slots cannot store new tiles.");
        let supported = self.workspace.is_some();
        if ui
            .add_enabled(
                supported && !stale && !busy,
                egui::Button::new("Choose PNG/BMP…"),
            )
            .clicked()
            && let Some(path) = dialogs::choose_map16_bitmap()
        {
            let result = self
                .capture_bitmap_import(project_revision)
                .and_then(|pending| {
                    self.bitmap_loader.start(vec![BoundedRead::new(
                        path,
                        u64::try_from(MAP16_BITMAP_MAX_PNG_BYTES).unwrap_or(u64::MAX),
                        "Map16 bitmap image",
                    )])?;
                    self.pending_bitmap_import = Some(pending);
                    Ok(())
                });
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
        if ui
            .add_enabled(
                supported && !stale && !busy,
                egui::Button::new("Paste bitmap from clipboard"),
            )
            .clicked()
        {
            let result = self
                .capture_bitmap_import(project_revision)
                .and_then(|pending| {
                    self.bitmap_clipboard_loader.start()?;
                    self.pending_bitmap_import = Some(pending);
                    Ok(())
                });
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
    }

    fn capture_bitmap_import(&self, revision: u64) -> Result<PendingBitmapImport, String> {
        let start_map16_tile = self
            .bitmap_import_options
            .as_ref()
            .map_or_else(lm_app::native_map16_bitmap_import_options, Clone::clone)
            .map16_allocation_start;
        capture_bitmap_import(
            revision,
            &self.preview_level,
            start_map16_tile,
            [&self.bitmap_extra_slot_4, &self.bitmap_extra_slot_5],
        )
    }

    fn clear_bitmap_session(&mut self) {
        self.bitmap_session = None;
        self.bitmap_original_texture = None;
        self.bitmap_converted_texture = None;
        self.bitmap_preview_scroll = egui::Vec2::ZERO;
    }
}

fn validate_bitmap_revision(current: u64, requested: u64) -> Result<(), String> {
    if current != requested {
        return Err("the ROM changed while the Map16 bitmap was loading".into());
    }
    Ok(())
}

/// Reconstructs Lunar Magic's process-global dialog state for a newly opened bitmap preview.
///
/// The request captures the process-global First Map16 value before asynchronous bitmap loading;
/// every other accepted option is retained across cancel, import, Map16-window close, and the next
/// preview as well.
fn bitmap_options_for_session(
    previous: Option<&lm_app::Map16BitmapImportOptions>,
    start_map16_tile: usize,
) -> lm_app::Map16BitmapImportOptions {
    let mut options = previous
        .cloned()
        .unwrap_or_else(lm_app::native_map16_bitmap_import_options);
    options.map16_allocation_start = start_map16_tile;
    options
}

fn capture_bitmap_import(
    revision: u64,
    level: &str,
    start_map16_tile: usize,
    extra_graphics: [&str; 2],
) -> Result<PendingBitmapImport, String> {
    let level = u16::from_str_radix(level.trim(), 16)
        .map_err(|_| "bitmap import level must be hexadecimal")?;
    if level > 0x01ff {
        return Err("bitmap import level must be between 000 and 1FF".into());
    }
    Ok(PendingBitmapImport {
        revision,
        level: usize::from(level),
        start_map16_tile,
        extra_graphics: [
            parse_optional_graphics(extra_graphics[0], "GFX slot 4")?,
            parse_optional_graphics(extra_graphics[1], "GFX slot 5")?,
        ],
        // Native previews always use the eight-row allocator. This compatibility field remains
        // meaningful only to the portable single-row preparation API.
        palette_row: 4,
    })
}

fn bitmap_multi_row_color_options(
    ui: &mut egui::Ui,
    options: &mut BitmapPaletteColorOptions,
    palette: &Palette,
) -> bool {
    let mut changed = false;
    ui.indent("map16-bitmap-multi-row-color-options", |ui| {
        ui.horizontal(|ui| {
            changed |= ui
                .add(egui::Slider::new(&mut options.maximum_colors, 1..=128).text("Maximum colors"))
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut options.priority_level, 1..=4).text("Priority"))
                .changed();
            egui::ComboBox::from_id_salt("map16-bitmap-reduction")
                .selected_text(match options.reduction {
                    BitmapPaletteReduction::MedianCut => "Median Cut",
                    BitmapPaletteReduction::Popularity => "Popularity",
                })
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(
                            &mut options.reduction,
                            BitmapPaletteReduction::MedianCut,
                            "Median Cut",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut options.reduction,
                            BitmapPaletteReduction::Popularity,
                            "Popularity",
                        )
                        .changed();
                });
        });
        changed |= bitmap_popularity_reduction_options(ui, options);
        changed |= ui
            .checkbox(
                &mut options.allow_modifying_unmarked_colors,
                "Allow modifying colors not marked reserved",
            )
            .changed();
        changed |= ui
            .add_enabled(
                false,
                egui::Checkbox::new(
                    &mut options.prioritize_exact_palette_matches,
                    "Prioritize exact existing-palette matches",
                ),
            )
            .on_hover_text(
                "Lunar Magic 3.63 stores this checked preference, but disables its control and has no conversion-path reader",
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut options.reusable_color_hue_tolerance, 0..=240)
                    .text("Reusable-color hue tolerance"),
            )
            .changed();
        ui.label("Palette entries: F = free, U = reusable, X = reserved");
        egui::Grid::new("map16-bitmap-palette-entry-states")
            .spacing([3.0, 3.0])
            .show(ui, |ui| {
                for row in 0..8 {
                    ui.label(format!("{row}:"));
                    for (marker, state, description) in [
                        ("F", BitmapPaletteEntryState::Free, "free"),
                        ("U", BitmapPaletteEntryState::Reusable, "reusable"),
                        ("X", BitmapPaletteEntryState::Reserved, "reserved"),
                    ] {
                        if ui
                            .small_button(marker)
                            .on_hover_text(format!("Mark every color in row {row} {description}"))
                            .clicked()
                        {
                            changed |=
                                set_bitmap_palette_row_state(&mut options.entries, row, state);
                        }
                    }
                    ui.separator();
                    for entry in 0..Palette::COLORS_PER_ROW {
                        let index = row * Palette::COLORS_PER_ROW + entry;
                        let color = palette.colors.get(index).copied().unwrap_or_default();
                        let rgb = color.to_rgb8();
                        let state = &mut options.entries[index];
                        let marker = match state {
                            BitmapPaletteEntryState::Free => "F",
                            BitmapPaletteEntryState::Reusable => "U",
                            BitmapPaletteEntryState::Reserved => "X",
                        };
                        let response = ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(marker)
                                        .background_color(egui::Color32::from_rgb(
                                            rgb.red, rgb.green, rgb.blue,
                                        ))
                                        .color(contrasting_text(rgb)),
                                )
                                .min_size(egui::vec2(20.0, 20.0)),
                            )
                            .on_hover_text(format!("Row {row}, color {entry:X}: {:04X}", color.0));
                        if response.clicked() {
                            *state = match state {
                                BitmapPaletteEntryState::Free => BitmapPaletteEntryState::Reusable,
                                BitmapPaletteEntryState::Reusable => {
                                    BitmapPaletteEntryState::Reserved
                                }
                                BitmapPaletteEntryState::Reserved => BitmapPaletteEntryState::Free,
                            };
                            changed = true;
                        }
                    }
                    ui.end_row();
                }
            });
    });
    changed
}

fn set_bitmap_palette_row_state(
    entries: &mut [BitmapPaletteEntryState],
    row: usize,
    state: BitmapPaletteEntryState,
) -> bool {
    let Some(start) = row.checked_mul(Palette::COLORS_PER_ROW) else {
        return false;
    };
    let Some(end) = start.checked_add(Palette::COLORS_PER_ROW) else {
        return false;
    };
    let Some(entries) = entries.get_mut(start..end) else {
        return false;
    };
    let changed = entries.iter().any(|entry| *entry != state);
    entries.fill(state);
    changed
}

fn bitmap_popularity_reduction_options(
    ui: &mut egui::Ui,
    options: &mut BitmapPaletteColorOptions,
) -> bool {
    let mut changed = false;
    ui.add_enabled_ui(
        options.reduction == BitmapPaletteReduction::Popularity,
        |ui| {
            changed |= ui
                .checkbox(
                    &mut options.prioritize_unique_colors,
                    "Give higher priority to unique colors",
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut options.maintain_detail,
                    "Maintain detail (assign each bitmap color separately)",
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut options.popularity_reduction_method_1,
                    "Reduce colors, method 1 (for high-color images)",
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut options.popularity_reduction_method_2,
                    "Reduce colors, method 2 (for high-color images)",
                )
                .changed();
        },
    );
    changed
}

fn parse_optional_graphics(text: &str, name: &str) -> Result<Option<usize>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    usize::from_str_radix(text, 16)
        .map(Some)
        .map_err(|_| format!("{name} must be hexadecimal or blank"))
}

fn rgba_image(pixels: &[lm_graphics::Rgba8], width: usize, height: usize) -> egui::ColorImage {
    let rgba: Vec<u8> = pixels
        .iter()
        .flat_map(|pixel| [pixel.red, pixel.green, pixel.blue, pixel.alpha])
        .collect();
    egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba)
}

fn preview_size(width: usize, height: usize, zoom: u8) -> egui::Vec2 {
    let zoom = f32::from(zoom.max(1));
    let width = f32::from(u16::try_from(width).unwrap_or(u16::MAX));
    let height = f32::from(u16::try_from(height).unwrap_or(u16::MAX));
    egui::vec2(width * zoom, height * zoom)
}

fn contrasting_text(color: lm_graphics::Rgb8) -> egui::Color32 {
    let luminance =
        u32::from(color.red) * 299 + u32::from(color.green) * 587 + u32::from(color.blue) * 114;
    if luminance >= 128_000 {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    }
}

fn decode_clipboard_rgba(
    width: usize,
    height: usize,
    bytes: &[u8],
) -> Result<DecodedMap16Bitmap, String> {
    if width == 0
        || height == 0
        || width > MAP16_BITMAP_MAX_DIMENSION
        || height > MAP16_BITMAP_MAX_DIMENSION
    {
        return Err(format!(
            "clipboard bitmap dimensions must be 1..={MAP16_BITMAP_MAX_DIMENSION}, got {width}×{height}"
        ));
    }
    let pixels = width
        .checked_mul(height)
        .filter(|pixels| *pixels <= MAP16_BITMAP_MAX_PIXELS)
        .ok_or_else(|| "clipboard bitmap pixel count exceeds the importer bound".to_owned())?;
    let expected = pixels
        .checked_mul(4)
        .ok_or_else(|| "clipboard bitmap byte length overflow".to_owned())?;
    if bytes.len() != expected {
        return Err(format!(
            "clipboard bitmap contains {} RGBA bytes, expected {expected}",
            bytes.len()
        ));
    }
    Ok(DecodedMap16Bitmap {
        width,
        height,
        pixels: bytes
            .chunks_exact(4)
            .map(|pixel| lm_graphics::Rgba8 {
                red: pixel[0],
                green: pixel[1],
                blue: pixel[2],
                alpha: pixel[3],
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::Rgba8;

    #[test]
    fn palette_row_controls_update_exactly_one_complete_row() {
        let mut entries = vec![BitmapPaletteEntryState::Reserved; 128];

        assert!(set_bitmap_palette_row_state(
            &mut entries,
            3,
            BitmapPaletteEntryState::Reusable,
        ));
        assert!(
            entries[..48]
                .iter()
                .all(|entry| *entry == BitmapPaletteEntryState::Reserved)
        );
        assert!(
            entries[48..64]
                .iter()
                .all(|entry| *entry == BitmapPaletteEntryState::Reusable)
        );
        assert!(
            entries[64..]
                .iter()
                .all(|entry| *entry == BitmapPaletteEntryState::Reserved)
        );
        assert!(!set_bitmap_palette_row_state(
            &mut entries,
            3,
            BitmapPaletteEntryState::Reusable,
        ));

        let unchanged = entries.clone();
        assert!(!set_bitmap_palette_row_state(
            &mut entries,
            8,
            BitmapPaletteEntryState::Free,
        ));
        assert!(!set_bitmap_palette_row_state(
            &mut entries,
            usize::MAX,
            BitmapPaletteEntryState::Free,
        ));
        assert_eq!(entries, unchanged);
    }

    #[test]
    fn optional_graphics_assignments_are_explicit_hexadecimal_values() {
        assert_eq!(parse_optional_graphics("", "slot").unwrap(), None);
        assert_eq!(parse_optional_graphics(" 7F ", "slot").unwrap(), Some(0x7f));
        assert_eq!(parse_optional_graphics("100", "slot").unwrap(), Some(0x100));
        assert!(parse_optional_graphics("xyz", "slot").is_err());
    }

    #[test]
    fn bitmap_request_captures_every_typed_target_before_loading() {
        assert_eq!(
            capture_bitmap_import(17, " 105 ", 0x234, [" 7f ", "100"]).unwrap(),
            PendingBitmapImport {
                revision: 17,
                level: 0x105,
                start_map16_tile: 0x234,
                extra_graphics: [Some(0x7f), Some(0x100)],
                palette_row: 4,
            }
        );
        assert!(capture_bitmap_import(17, "200", 0x234, ["", ""]).is_err());
        assert!(capture_bitmap_import(17, "105", 0x234, ["xyz", ""]).is_err());
    }

    #[test]
    fn first_native_preview_uses_8200_and_later_previews_use_the_retained_option() {
        let mut editor = RomMap16Editor {
            preview_level: "105".into(),
            ..RomMap16Editor::default()
        };
        assert_eq!(
            editor.capture_bitmap_import(7).unwrap().start_map16_tile,
            0x8200
        );

        let mut retained = lm_app::native_map16_bitmap_import_options();
        retained.map16_allocation_start = 0x8345;
        editor.bitmap_import_options = Some(retained);
        assert_eq!(
            editor.capture_bitmap_import(8).unwrap().start_map16_tile,
            0x8345
        );
    }

    #[test]
    fn bitmap_completion_is_bound_to_the_revision_that_started_loading() {
        assert!(validate_bitmap_revision(17, 17).is_ok());
        assert_eq!(
            validate_bitmap_revision(18, 17).unwrap_err(),
            "the ROM changed while the Map16 bitmap was loading"
        );
    }

    #[test]
    fn native_bitmap_options_start_with_multi_row_colors_and_persist_between_previews() {
        let first = bitmap_options_for_session(None, 0x2345);
        assert_eq!(first.map16_allocation_start, 0x2345);
        assert_eq!(
            first.color,
            Some(BitmapPaletteColorOptions::lunar_magic_initial())
        );

        let mut edited = first;
        edited.graphics.optimize_new_tiles = false;
        edited.graphics.allocation_start = 0x2a0;
        edited.layer_priority = true;
        edited.color.as_mut().unwrap().maximum_colors = 37;
        let reopened = bitmap_options_for_session(Some(&edited), 0x3456);
        assert!(!reopened.graphics.optimize_new_tiles);
        assert_eq!(reopened.graphics.allocation_start, 0x2a0);
        assert!(reopened.layer_priority);
        assert_eq!(reopened.color.as_ref().unwrap().maximum_colors, 37);
        assert_eq!(reopened.map16_allocation_start, 0x3456);
    }

    #[test]
    fn preview_texture_preserves_rgba_channels() {
        let pixels = vec![
            Rgba8 {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 4,
            };
            lm_app::MAP16_BITMAP_WIDTH * lm_app::MAP16_BITMAP_HEIGHT
        ];
        let image = rgba_image(
            &pixels,
            lm_app::MAP16_BITMAP_WIDTH,
            lm_app::MAP16_BITMAP_HEIGHT,
        );
        assert_eq!(
            image.size,
            [lm_app::MAP16_BITMAP_WIDTH, lm_app::MAP16_BITMAP_HEIGHT]
        );
        assert_eq!(
            image.pixels[0],
            egui::Color32::from_rgba_unmultiplied(1, 2, 3, 4)
        );
    }

    #[test]
    fn alpha_bitfield_bmp_reaches_the_native_preview_without_losing_transparency() {
        let dib_size = 108_usize;
        let pixel_offset = 14 + dib_size;
        let mut bytes = vec![0; pixel_offset + 4];
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&u32::try_from(dib_size).unwrap().to_le_bytes());
        bytes[18..22].copy_from_slice(&1_i32.to_le_bytes());
        bytes[22..26].copy_from_slice(&1_i32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&32_u16.to_le_bytes());
        bytes[30..34].copy_from_slice(&3_u32.to_le_bytes());
        bytes[34..38].copy_from_slice(&4_u32.to_le_bytes());
        for (index, mask) in [0x00ff_0000_u32, 0x0000_ff00, 0x0000_00ff, 0xff00_0000]
            .into_iter()
            .enumerate()
        {
            let at = 54 + index * 4;
            bytes[at..at + 4].copy_from_slice(&mask.to_le_bytes());
        }
        bytes[pixel_offset..pixel_offset + 4].copy_from_slice(&0x4012_3456_u32.to_le_bytes());

        let decoded = decode_map16_bitmap_image(&bytes).unwrap();
        assert_eq!(decoded.pixels[0].alpha, 0x40);
        let preview = rgba_image(&decoded.pixels, decoded.width, decoded.height);
        assert_eq!(
            preview.pixels[0],
            egui::Color32::from_rgba_unmultiplied(0x12, 0x34, 0x56, 0x40)
        );
    }

    #[test]
    fn core_header_bmp_reaches_the_native_preview_with_rgb_channels_intact() {
        let pixel_offset = 26_usize;
        let mut bytes = vec![0; pixel_offset + 4];
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&12_u32.to_le_bytes());
        bytes[18..20].copy_from_slice(&1_u16.to_le_bytes());
        bytes[20..22].copy_from_slice(&1_u16.to_le_bytes());
        bytes[22..24].copy_from_slice(&1_u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&24_u16.to_le_bytes());
        bytes[pixel_offset..pixel_offset + 3].copy_from_slice(&[0x56, 0x34, 0x12]);

        let decoded = decode_map16_bitmap_image(&bytes).unwrap();
        let preview = rgba_image(&decoded.pixels, decoded.width, decoded.height);
        assert_eq!(
            preview.pixels[0],
            egui::Color32::from_rgba_unmultiplied(0x12, 0x34, 0x56, 0xff)
        );
    }

    #[test]
    fn os2_v2_bmp_reaches_the_native_preview_with_rgb_channels_intact() {
        let pixel_offset = 78_usize;
        let mut bytes = vec![0; pixel_offset + 4];
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&64_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&1_u32.to_le_bytes());
        bytes[22..26].copy_from_slice(&1_u32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&24_u16.to_le_bytes());
        bytes[34..38].copy_from_slice(&4_u32.to_le_bytes());
        bytes[pixel_offset..pixel_offset + 3].copy_from_slice(&[0x56, 0x34, 0x12]);

        let decoded = decode_map16_bitmap_image(&bytes).unwrap();
        let preview = rgba_image(&decoded.pixels, decoded.width, decoded.height);
        assert_eq!(
            preview.pixels[0],
            egui::Color32::from_rgba_unmultiplied(0x12, 0x34, 0x56, 0xff)
        );
    }

    #[test]
    fn os2_v2_rle24_bmp_reaches_the_native_preview_with_rgb_channels_intact() {
        let pixel_offset = 78_usize;
        let stream = [1, 0x56, 0x34, 0x12, 0, 1];
        let mut bytes = vec![0; pixel_offset];
        bytes.extend_from_slice(&stream);
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&64_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&1_u32.to_le_bytes());
        bytes[22..26].copy_from_slice(&1_u32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&24_u16.to_le_bytes());
        bytes[30..34].copy_from_slice(&4_u32.to_le_bytes());
        bytes[34..38].copy_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());

        let decoded = decode_map16_bitmap_image(&bytes).unwrap();
        let preview = rgba_image(&decoded.pixels, decoded.width, decoded.height);
        assert_eq!(
            preview.pixels[0],
            egui::Color32::from_rgba_unmultiplied(0x12, 0x34, 0x56, 0xff)
        );
    }

    #[test]
    fn packed_2bpp_bmp_reaches_the_native_preview_with_palette_rgb_intact() {
        let pixel_offset = 70_usize;
        let mut bytes = vec![0; pixel_offset + 4];
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&1_i32.to_le_bytes());
        bytes[22..26].copy_from_slice(&1_i32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&2_u16.to_le_bytes());
        bytes[34..38].copy_from_slice(&4_u32.to_le_bytes());
        bytes[46..50].copy_from_slice(&4_u32.to_le_bytes());
        for (index, color) in [[0, 0, 0], [3, 2, 1], [0x56, 0x34, 0x12], [9, 8, 7]]
            .into_iter()
            .enumerate()
        {
            let at = 54 + index * 4;
            bytes[at..at + 3].copy_from_slice(&color);
        }
        bytes[pixel_offset] = 2 << 6;

        let decoded = decode_map16_bitmap_image(&bytes).unwrap();
        let preview = rgba_image(&decoded.pixels, decoded.width, decoded.height);
        assert_eq!(
            preview.pixels[0],
            egui::Color32::from_rgba_unmultiplied(0x12, 0x34, 0x56, 0xff)
        );
    }

    #[test]
    fn os2_huffman1d_bmp_reaches_native_preview_with_palette_and_orientation_intact() {
        let pixel_offset = 86_usize;
        let stream = [0, 25, 128, 13, 176, 1, 0, 16, 1, 0, 16, 1, 0, 16];
        let mut bytes = vec![0; pixel_offset];
        bytes.extend_from_slice(&stream);
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&64_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&8_u32.to_le_bytes());
        bytes[22..26].copy_from_slice(&2_u32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&1_u16.to_le_bytes());
        bytes[30..34].copy_from_slice(&3_u32.to_le_bytes());
        bytes[34..38].copy_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());
        bytes[46..50].copy_from_slice(&2_u32.to_le_bytes());
        bytes[78..82].copy_from_slice(&[0x56, 0x34, 0x12, 0]);
        bytes[82..86].copy_from_slice(&[0xbc, 0x9a, 0x78, 0]);

        let decoded = decode_map16_bitmap_image(&bytes).unwrap();
        let preview = rgba_image(&decoded.pixels, decoded.width, decoded.height);
        assert_eq!(preview.size, [8, 2]);
        assert_eq!(
            preview.pixels[0],
            egui::Color32::from_rgba_unmultiplied(0x12, 0x34, 0x56, 0xff)
        );
        assert_eq!(
            preview.pixels[4],
            egui::Color32::from_rgba_unmultiplied(0x78, 0x9a, 0xbc, 0xff)
        );
        assert_eq!(preview.pixels[8], preview.pixels[0]);
    }

    #[test]
    fn preview_size_preserves_source_aspect_ratio_and_integer_zoom() {
        assert_eq!(preview_size(32, 16, 1), egui::vec2(32.0, 16.0));
        assert_eq!(preview_size(32, 16, 4), egui::vec2(128.0, 64.0));
        assert_eq!(preview_size(32, 16, 0), egui::vec2(32.0, 16.0));
    }

    #[test]
    fn palette_swatch_text_contrasts_with_light_and_dark_colors() {
        assert_eq!(
            contrasting_text(lm_graphics::Rgb8 {
                red: 255,
                green: 255,
                blue: 255,
            }),
            egui::Color32::BLACK
        );
        assert_eq!(
            contrasting_text(lm_graphics::Rgb8::default()),
            egui::Color32::WHITE
        );
    }

    #[test]
    fn clipboard_rgba_decode_is_bounded_and_preserves_channels() {
        let decoded = decode_clipboard_rgba(2, 1, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(
            decoded.pixels,
            [
                lm_graphics::Rgba8 {
                    red: 1,
                    green: 2,
                    blue: 3,
                    alpha: 4,
                },
                lm_graphics::Rgba8 {
                    red: 5,
                    green: 6,
                    blue: 7,
                    alpha: 8,
                },
            ]
        );
        assert!(decode_clipboard_rgba(0, 1, &[]).is_err());
        assert!(decode_clipboard_rgba(1, 1, &[0; 3]).is_err());
        assert!(decode_clipboard_rgba(MAP16_BITMAP_MAX_DIMENSION + 1, 1, &[]).is_err());
    }
}
