use super::{Command, RomMap16Editor, egui};
use crate::{dialogs, document_loader::BoundedRead, rom_allocation::parse_search_range};
use lm_app::{
    MAP16_BITMAP_MAX_PNG_BYTES, NativeMap16BitmapImportSession,
    NativeMap16BitmapImportSessionRequest, decode_map16_bitmap_png_image,
};

impl RomMap16Editor {
    pub(super) fn poll_bitmap_loader(&mut self, context: &egui::Context) -> Option<Command> {
        let completion = self.bitmap_loader.show(context)?;
        match completion.and_then(|loaded| {
            let [(_, bytes)] = loaded.into_exact::<1>("Map16 bitmap")?;
            self.open_bitmap_session(&bytes)
        }) {
            Ok(()) => {}
            Err(error) => self.error = Some(error),
        }
        None
    }

    fn open_bitmap_session(&mut self, bytes: &[u8]) -> Result<(), String> {
        let bitmap = decode_map16_bitmap_png_image(bytes).map_err(|error| error.to_string())?;
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let level = u16::from_str_radix(self.preview_level.trim(), 16)
            .map_err(|_| "bitmap import level must be hexadecimal")?;
        if level > 0x01ff {
            return Err("bitmap import level must be between 000 and 1FF".into());
        }
        let acts_like = u16::from_str_radix(self.bitmap_acts_like.trim(), 16)
            .map_err(|_| "bitmap import Acts Like must be hexadecimal")?;
        let extra_graphics = [
            parse_optional_graphics(&self.bitmap_extra_slot_4, "GFX slot 4")?,
            parse_optional_graphics(&self.bitmap_extra_slot_5, "GFX slot 5")?,
        ];
        let request = NativeMap16BitmapImportSessionRequest {
            level: usize::from(level),
            page: self.page,
            extra_graphics,
            pixels: bitmap.pixels,
            width: bitmap.width,
            height: bitmap.height,
            palette_row: self.bitmap_palette_row,
            acts_like,
        };
        self.bitmap_session = Some(
            if let Some(profile) = workspace.profile.clone() {
                NativeMap16BitmapImportSession::new(workspace.snapshot.clone(), profile, request)
            } else {
                NativeMap16BitmapImportSession::new_smw_us_v1(workspace.snapshot.clone(), request)
            }
            .map_err(|error| error.to_string())?,
        );
        self.bitmap_original_texture = None;
        self.bitmap_converted_texture = None;
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
                let changed = ui
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
                                &mut options.graphics.allow_flipped_matches,
                                "Reuse flipped matches",
                            )
                            .changed()
                            | ui.checkbox(&mut options.layer_priority, "Layer priority")
                                .changed()
                    })
                    .inner;
                if changed {
                    match session.set_options(options) {
                        Ok(()) => {
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
            ui.vertical(|ui| {
                ui.label("Original");
                if let Some(texture) = &self.bitmap_original_texture {
                    ui.add(egui::Image::new(texture).fit_to_exact_size(egui::Vec2::splat(256.0)));
                }
            });
            ui.vertical(|ui| {
                ui.label("Converted");
                if let Some(texture) = &self.bitmap_converted_texture {
                    ui.add(egui::Image::new(texture).fit_to_exact_size(egui::Vec2::splat(256.0)));
                }
            });
        });
    }

    pub(super) fn bitmap_import_controls(&mut self, ui: &mut egui::Ui, stale: bool) {
        ui.separator();
        ui.heading("Bitmap to Map16");
        ui.label("The selected page, preview level, and its real object tileset are used.");
        ui.horizontal(|ui| {
            ui.label("Editable GFX slot 4");
            ui.text_edit_singleline(&mut self.bitmap_extra_slot_4);
            ui.label("slot 5");
            ui.text_edit_singleline(&mut self.bitmap_extra_slot_5);
        });
        ui.small("Enter hexadecimal GFX/ExGFX file numbers. Blank slots cannot store new tiles.");
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.bitmap_palette_row, 0..=7).text("Palette row"));
            ui.label("Acts Like");
            ui.text_edit_singleline(&mut self.bitmap_acts_like);
        });
        let supported = self.workspace.is_some();
        if ui
            .add_enabled(
                supported && !stale && !self.bitmap_loader.is_running(),
                egui::Button::new("Choose PNG…"),
            )
            .clicked()
            && let Some(path) = dialogs::choose_map16_bitmap_png()
            && let Err(error) = self.bitmap_loader.start(vec![BoundedRead::new(
                path,
                u64::try_from(MAP16_BITMAP_MAX_PNG_BYTES).unwrap_or(u64::MAX),
                "Map16 bitmap PNG",
            )])
        {
            self.error = Some(error);
        }
    }

    fn clear_bitmap_session(&mut self) {
        self.bitmap_session = None;
        self.bitmap_original_texture = None;
        self.bitmap_converted_texture = None;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::Rgba8;

    #[test]
    fn optional_graphics_assignments_are_explicit_hexadecimal_values() {
        assert_eq!(parse_optional_graphics("", "slot").unwrap(), None);
        assert_eq!(parse_optional_graphics(" 7F ", "slot").unwrap(), Some(0x7f));
        assert_eq!(parse_optional_graphics("100", "slot").unwrap(), Some(0x100));
        assert!(parse_optional_graphics("xyz", "slot").is_err());
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
}
