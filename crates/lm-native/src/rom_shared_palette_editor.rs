use crate::level_editor_forms::{format_bytes, parse_bytes};
use eframe::egui;
use lm_app::{AppState, Command};
use lm_graphics::{Bgr555, SmwPaletteBackend};
use lm_profile::smw_us_v1_shared_palette_layout;

mod form;
mod transfer;
mod workspace;

use form::ColorForm;
use workspace::Workspace;

const COLORS_PER_PAGE: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

#[derive(Default)]
pub(crate) struct RomSharedPaletteEditor {
    workspace: Option<Workspace>,
    selected: usize,
    page: usize,
    loaded: Option<usize>,
    form: ColorForm,
    auxiliary: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    transfer_loader: crate::document_loader::DocumentLoader,
    transfer_persistence: crate::persistence_worker::PersistenceWorker,
}

impl RomSharedPaletteEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        let result = app
            .project()
            .ok_or_else(|| "open a supported ROM first".to_owned())
            .and_then(|project| {
                project
                    .load_shared_palette(smw_us_v1_shared_palette_layout())
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(palette) => {
                self.form = match ColorForm::load(&palette, 0) {
                    Ok(form) => form,
                    Err(error) => {
                        self.error = Some(error);
                        return;
                    }
                };
                self.auxiliary = format_bytes(palette.auxiliary_bytes());
                self.workspace = Some(Workspace {
                    revision: app.project_revision(),
                    original: palette.clone(),
                    current: palette,
                });
                self.selected = 0;
                self.page = 0;
                self.loaded = Some(0);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.transfer_loader.is_running() || self.transfer_persistence.is_running() {
            self.error = Some("wait for shared-palette file work to finish before closing".into());
            return false;
        }
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if !workspace.dirty() {
            self.clear();
            return true;
        }
        self.pending_close = Some(if application {
            PendingClose::Application
        } else {
            PendingClose::Editor
        });
        false
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> (bool, Option<Command>) {
        self.poll_transfer_file_io(context, project_revision);
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new("Native Shared/Custom SMW Palettes")
                .default_size([690.0, 650.0])
                .show(context, |ui| command = self.contents(ui, project_revision));
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.revision != project_revision;
        let dirty = workspace.dirty();
        let transfer_busy =
            self.transfer_loader.is_running() || self.transfer_persistence.is_running();
        let backend = workspace.current.backend();
        let palette = match workspace.current.palette() {
            Ok(palette) => palette,
            Err(error) => {
                self.error = Some(error.to_string());
                return None;
            }
        };
        let colors = palette.colors;
        let pages = colors.len().div_ceil(COLORS_PER_PAGE);
        self.page = self.page.min(pages.saturating_sub(1));
        ui.label(format!(
            "{backend:?} backend · {} colors · exact native .smwpal ordering",
            colors.len()
        ));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after this palette was opened. Reopen before committing.",
            );
        }
        self.complete_file_controls(ui, stale, project_revision);
        ui.add_enabled_ui(!self.transfer_loader.is_running(), |ui| {
            self.show_palette_grid(ui, &colors, pages);
            self.show_color_form(ui, stale);
        });
        if backend == SmwPaletteBackend::Expanded {
            ui.add_enabled_ui(!self.transfer_loader.is_running(), |ui| {
                self.show_auxiliary_form(ui, stale);
            });
        }
        ui.separator();
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    dirty && !stale && !transfer_busy,
                    egui::Button::new("Commit palette to ROM"),
                )
                .clicked()
            {
                match self.prepare_commit(project_revision) {
                    Ok(prepared) => command = prepared,
                    Err(error) => self.error = Some(error),
                }
            }
            ui.label(if dirty { "Staged" } else { "Unchanged" });
        });
        command
    }

    fn show_palette_grid(&mut self, ui: &mut egui::Ui, colors: &[Bgr555], pages: usize) {
        ui.horizontal(|ui| {
            ui.label("Page");
            ui.add(
                egui::DragValue::new(&mut self.page)
                    .range(0..=pages.saturating_sub(1))
                    .hexadecimal(1, false, true),
            );
            ui.label(format!("of {}", pages.saturating_sub(1)));
        });
        let start = self.page * COLORS_PER_PAGE;
        let end = (start + COLORS_PER_PAGE).min(colors.len());
        egui::Grid::new("native-shared-palette-grid")
            .spacing([2.0, 2.0])
            .show(ui, |ui| {
                for (index, color) in colors.iter().enumerate().take(end).skip(start) {
                    let rgb = color.to_rgb8();
                    let response = ui.add(
                        egui::Button::new(format!("{:X}", index & 0x0f))
                            .min_size(egui::vec2(28.0, 20.0))
                            .fill(egui::Color32::from_rgb(rgb.red, rgb.green, rgb.blue))
                            .stroke(if index == self.selected {
                                egui::Stroke::new(2.0_f32, egui::Color32::YELLOW)
                            } else {
                                egui::Stroke::NONE
                            }),
                    );
                    if response.clicked() {
                        self.selected = index;
                        if let Err(error) = self.load_selected() {
                            self.error = Some(error);
                        }
                    }
                    if (index - start) % 16 == 15 {
                        ui.end_row();
                    }
                }
            });
    }

    fn show_color_form(&mut self, ui: &mut egui::Ui, stale: bool) {
        ui.label(format!("Selected color ${:03X}", self.selected));
        egui::Grid::new("native-shared-palette-color-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label("SNES BGR555");
                ui.add(
                    egui::DragValue::new(&mut self.form.word)
                        .range(0..=0x7fff)
                        .hexadecimal(4, false, true),
                );
                if ui.button("Decode raw").clicked()
                    && let Err(error) = self.form.use_word()
                {
                    self.error = Some(error);
                }
                ui.end_row();
                for (label, channel) in [
                    ("Red", &mut self.form.red),
                    ("Green", &mut self.form.green),
                    ("Blue", &mut self.form.blue),
                ] {
                    ui.label(label);
                    ui.add(egui::Slider::new(channel, 0..=255));
                    ui.end_row();
                }
            });
        let preview = self.form.rgb_color().to_rgb8();
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(preview.red, preview.green, preview.blue),
                "████ Preview",
            );
            if ui
                .add_enabled(!stale, egui::Button::new("Apply RGB color"))
                .clicked()
                && let Err(error) = self.apply_rgb()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(!stale, egui::Button::new("Apply raw word"))
                .clicked()
                && let Err(error) = self.apply_raw()
            {
                self.error = Some(error);
            }
        });
    }

    fn show_auxiliary_form(&mut self, ui: &mut egui::Ui, stale: bool) {
        ui.separator();
        ui.label("Expanded auxiliary bytes");
        ui.text_edit_singleline(&mut self.auxiliary);
        if ui
            .add_enabled(!stale, egui::Button::new("Stage auxiliary bytes"))
            .clicked()
            && let Err(error) = self.apply_auxiliary()
        {
            self.error = Some(error);
        }
    }

    fn load_selected(&mut self) -> Result<(), String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "shared-palette workspace is closed".to_owned())?;
        self.form = ColorForm::load(&workspace.current, self.selected)?;
        self.loaded = Some(self.selected);
        Ok(())
    }

    fn apply_rgb(&mut self) -> Result<(), String> {
        self.require_loaded_selection()?;
        let color = self.form.rgb_color();
        self.workspace
            .as_mut()
            .ok_or_else(|| "shared-palette workspace is closed".to_owned())?
            .replace_color(self.selected, color)?;
        self.form = ColorForm::from_color(color);
        Ok(())
    }

    fn apply_raw(&mut self) -> Result<(), String> {
        self.require_loaded_selection()?;
        self.form.use_word()?;
        let color = Bgr555(self.form.word);
        self.workspace
            .as_mut()
            .ok_or_else(|| "shared-palette workspace is closed".to_owned())?
            .replace_color(self.selected, color)
    }

    fn require_loaded_selection(&self) -> Result<(), String> {
        if self.loaded != Some(self.selected) {
            return Err("load the selected shared-palette color before applying it".into());
        }
        Ok(())
    }

    fn apply_auxiliary(&mut self) -> Result<(), String> {
        let auxiliary = parse_bytes(&self.auxiliary, "shared-palette auxiliary bytes")?;
        self.workspace
            .as_mut()
            .ok_or_else(|| "shared-palette workspace is closed".to_owned())?
            .replace_auxiliary(auxiliary)
    }

    fn prepare_commit(&self, project_revision: u64) -> Result<Option<Command>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "shared-palette workspace is closed".to_owned())?;
        if workspace.revision != project_revision {
            return Err("stale shared-palette workspace cannot be committed".into());
        }
        if !workspace.dirty() {
            return Ok(None);
        }
        Ok(Some(Command::ReplaceNativeSharedPalette {
            rev: workspace.revision,
            palette: Box::new(workspace.current.clone()),
        }))
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard shared-palette changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged shared/custom palette has not been committed to the ROM.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_close = None;
                    }
                    if ui.button("Discard").clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("Shared-palette editor error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.loaded = None;
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pristine_app() -> (AppState, Vec<u8>) {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        (app, original)
    }

    #[test]
    fn pristine_gui_color_edit_dispatches_reopens_and_undoes_exactly() {
        let (mut app, original) = pristine_app();
        let mut editor = RomSharedPaletteEditor::default();
        editor.open(&app);
        editor.selected = 0x123;
        editor.load_selected().unwrap();
        editor.form.red ^= 0xff;
        editor.apply_rgb().unwrap();
        let expected = editor.workspace.as_ref().unwrap().current.clone();
        let command = editor.prepare_commit(0).unwrap().unwrap();
        app.dispatch(command).unwrap();
        assert_eq!(
            app.project()
                .unwrap()
                .load_shared_palette(smw_us_v1_shared_palette_layout())
                .unwrap(),
            expected
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn expanded_auxiliary_stale_and_dirty_close_are_guarded() {
        let (mut app, _) = pristine_app();
        app.dispatch(Command::InstallExpandedSharedPalettes { rev: 0 })
            .unwrap();
        let mut editor = RomSharedPaletteEditor::default();
        editor.open(&app);
        assert_eq!(
            editor
                .workspace
                .as_ref()
                .unwrap()
                .current
                .auxiliary_bytes()
                .len(),
            16
        );
        editor.auxiliary = "00 01".into();
        assert!(editor.apply_auxiliary().is_err());
        editor.auxiliary = format_bytes(&[0xaa; 16]);
        editor.apply_auxiliary().unwrap();
        assert!(editor.prepare_commit(app.project_revision() + 1).is_err());
        assert!(!editor.request_close(false));
        assert!(editor.is_open());
    }

    #[test]
    fn complete_file_upgrade_dispatches_and_reopens_exact_expanded_backend() {
        let (mut app, _) = pristine_app();
        let mut editor = RomSharedPaletteEditor::default();
        editor.open(&app);
        let imported = lm_graphics::SmwPaletteFile::expanded(
            vec![0x24; lm_graphics::SmwPaletteFile::EXPANDED_PALETTE_LEN],
            (0_u8..16).collect(),
        )
        .unwrap();
        editor
            .workspace
            .as_mut()
            .unwrap()
            .replace_file(imported.clone())
            .unwrap();
        let command = editor.prepare_commit(0).unwrap().unwrap();
        app.dispatch(command).unwrap();
        assert_eq!(
            app.project()
                .unwrap()
                .load_shared_palette(smw_us_v1_shared_palette_layout())
                .unwrap(),
            imported
        );
    }
}
