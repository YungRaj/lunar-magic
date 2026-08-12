use crate::{
    level_editor_forms::{format_bytes, parse_bytes},
    native_clipboard,
};
use eframe::egui;
use lm_app::{AppState, Command};
use lm_graphics::{Bgr555, SmwPaletteBackend};
use lm_profile::smw_us_v1_shared_palette_layout_for_mapper;

mod form;
mod transfer;
mod workspace;

use form::ColorForm;
use workspace::Workspace;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SharedPaletteTransferAction {
    Export,
    Import,
}

const COLORS_PER_PAGE: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasteTarget {
    Color,
    Row,
}

enum NativePaste {
    Color(Bgr555),
    Row([Bgr555; 16]),
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
    paste_target: Option<PasteTarget>,
}

impl RomSharedPaletteEditor {
    pub(crate) fn stage_recovery_on_project(
        &self,
        app: &AppState,
        staged: &mut lm_project::Project,
    ) -> Result<(), String> {
        self.workspace
            .as_ref()
            .ok_or_else(|| "shared-palette workspace is closed".to_owned())?
            .stage_recovery_on_project(app, staged)
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        self.workspace.as_ref()?.staged_recovery_generation(app)
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        self.workspace
            .as_ref()
            .ok_or_else(|| "shared-palette workspace is closed".to_owned())?
            .staged_recovery_snapshot(app)
    }

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
                let mapper = project
                    .identity
                    .as_ref()
                    .ok_or_else(|| "open a supported ROM first".to_owned())?
                    .mapper;
                if !matches!(mapper, lm_rom::Mapper::LoRom | lm_rom::Mapper::ExLoRom) {
                    return Err(
                        "shared palettes are supported only for SMW LoROM and ExLoROM".into(),
                    );
                }
                project
                    .load_shared_palette(smw_us_v1_shared_palette_layout_for_mapper(mapper))
                    .map_err(|error| error.to_string())
                    .map(|palette| (mapper, palette))
            });
        match result {
            Ok((mapper, palette)) => {
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
                    mapper,
                    original: palette.clone(),
                    current: palette,
                });
                self.selected = 0;
                self.page = 0;
                self.loaded = Some(0);
                self.paste_target = None;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn open_and_start_transfer(
        &mut self,
        app: &AppState,
        action: SharedPaletteTransferAction,
    ) {
        self.open(app);
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        if workspace.revision != app.project_revision() {
            self.error = Some(
                "the ROM changed after the shared palette was opened; reopen it before transfer"
                    .into(),
            );
            return;
        }
        if self.transfer_loader.is_running() || self.transfer_persistence.is_running() {
            self.error = Some("wait for the active shared-palette file transfer to finish".into());
            return;
        }
        match action {
            SharedPaletteTransferAction::Export => {
                self.start_complete_export(app.project_revision())
            }
            SharedPaletteTransferAction::Import => self.start_complete_import(),
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
        let pasted = pasted_text(ui);
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
        let palette_locked = stale || self.transfer_loader.is_running();
        ui.add_enabled_ui(!palette_locked, |ui| {
            if let Some(result) = self.show_palette_grid(ui, &colors, pages, palette_locked) {
                let result = result.and_then(|paste| match paste {
                    NativePaste::Color(color) => self.paste_color(color),
                    NativePaste::Row(colors) => self.paste_row(colors),
                });
                if let Err(error) = result {
                    self.error = Some(error);
                }
            }
            if let Some(text) = pasted {
                if palette_locked {
                    if self.paste_target.take().is_some() {
                        self.error = Some(
                            "shared-palette paste arrived while editing was unavailable".into(),
                        );
                    }
                } else if let Err(error) = self.apply_clipboard_paste(&text) {
                    self.error = Some(error);
                }
            }
            self.show_color_form(ui, palette_locked);
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

    fn show_palette_grid(
        &mut self,
        ui: &mut egui::Ui,
        colors: &[Bgr555],
        pages: usize,
        stale: bool,
    ) -> Option<Result<NativePaste, String>> {
        let mut native_paste = None;
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
                        let modifiers = ui.input(|input| input.modifiers);
                        if modifiers.ctrl {
                            let encoded = if modifiers.alt {
                                palette_row(colors, index).and_then(|row| {
                                    native_clipboard::copy_palette_row_to_system(ui.ctx(), row)
                                })
                            } else {
                                native_clipboard::copy_palette_color_to_system(ui.ctx(), *color)
                            };
                            match encoded {
                                Ok(()) => {}
                                Err(error) => self.error = Some(error),
                            }
                        }
                    }
                    if !stale
                        && response.secondary_clicked()
                        && ui.input(|input| input.modifiers.ctrl)
                    {
                        self.selected = index;
                        if let Err(error) = self.load_selected() {
                            self.error = Some(error);
                        } else {
                            let row = ui.input(|input| input.modifiers.alt);
                            let result = if row {
                                native_clipboard::request_palette_row_paste(ui.ctx())
                                    .map(|value| value.map(NativePaste::Row))
                            } else {
                                native_clipboard::request_palette_color_paste(ui.ctx())
                                    .map(|value| value.map(NativePaste::Color))
                            };
                            match result {
                                Ok(Some(paste)) => native_paste = Some(Ok(paste)),
                                Ok(None) => {
                                    self.paste_target = Some(if row {
                                        PasteTarget::Row
                                    } else {
                                        PasteTarget::Color
                                    });
                                }
                                Err(error) => native_paste = Some(Err(error)),
                            }
                        }
                    }
                    if (index - start) % 16 == 15 {
                        ui.end_row();
                    }
                }
            });
        native_paste
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
            let row = self
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.current.palette().ok())
                .and_then(|palette| {
                    palette_row(&palette.colors, self.selected)
                        .ok()
                        .and_then(|row| <[Bgr555; 16]>::try_from(row).ok())
                });
            if ui
                .add_enabled(row.is_some(), egui::Button::new("Copy row"))
                .clicked()
            {
                if let Err(error) = native_clipboard::copy_palette_row_to_system(
                    ui.ctx(),
                    row.as_ref().expect("enabled row is complete"),
                ) {
                    self.error = Some(error);
                }
            }
            if ui
                .add_enabled(!stale && row.is_some(), egui::Button::new("Paste row"))
                .clicked()
            {
                match native_clipboard::request_palette_row_paste(ui.ctx()) {
                    Ok(Some(colors)) => {
                        if let Err(error) = self.paste_row(colors) {
                            self.error = Some(error);
                        }
                    }
                    Ok(None) => self.paste_target = Some(PasteTarget::Row),
                    Err(error) => self.error = Some(error),
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Copy color").clicked() {
                if let Err(error) =
                    native_clipboard::copy_palette_color_to_system(ui.ctx(), Bgr555(self.form.word))
                {
                    self.error = Some(error);
                }
            }
            if ui
                .add_enabled(!stale, egui::Button::new("Paste color"))
                .clicked()
            {
                match native_clipboard::request_palette_color_paste(ui.ctx()) {
                    Ok(Some(color)) => {
                        if let Err(error) = self.paste_color(color) {
                            self.error = Some(error);
                        }
                    }
                    Ok(None) => self.paste_target = Some(PasteTarget::Color),
                    Err(error) => self.error = Some(error),
                }
            }
            ui.small("Ctrl+left/right uses the swatches; add Alt for a complete row.");
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

    fn apply_clipboard_paste(&mut self, text: &str) -> Result<(), String> {
        let Some(target) = self.paste_target.take() else {
            return Ok(());
        };
        match target {
            PasteTarget::Color => {
                self.paste_color(native_clipboard::decode_palette_color(text)?)?
            }
            PasteTarget::Row => self.paste_row(native_clipboard::decode_palette_row(text)?)?,
        }
        Ok(())
    }

    fn paste_color(&mut self, color: Bgr555) -> Result<(), String> {
        self.workspace
            .as_mut()
            .ok_or_else(|| "shared-palette workspace is closed".to_owned())?
            .replace_color(self.selected, color)?;
        self.load_selected()
    }

    fn paste_row(&mut self, colors: [Bgr555; 16]) -> Result<(), String> {
        let start = self.selected / 16 * 16;
        self.workspace
            .as_mut()
            .ok_or_else(|| "shared-palette workspace is closed".to_owned())?
            .replace_row(start, colors)?;
        self.load_selected()
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
        self.paste_target = None;
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

fn palette_row(colors: &[Bgr555], selected: usize) -> Result<&[Bgr555], String> {
    let start = selected / 16 * 16;
    colors
        .get(start..start.saturating_add(16))
        .ok_or_else(|| "selected color does not belong to a complete palette row".to_string())
}

fn pasted_text(ui: &egui::Ui) -> Option<String> {
    ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) => Some(text.clone()),
            _ => None,
        })
    })
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
    fn staged_legacy_shared_palette_is_recovered_without_committing_live_project() {
        let (app, _) = pristine_app();
        let mut editor = RomSharedPaletteEditor::default();
        editor.open(&app);
        assert_eq!(
            editor.workspace.as_ref().unwrap().mapper,
            lm_rom::Mapper::LoRom
        );
        editor
            .workspace
            .as_mut()
            .unwrap()
            .replace_color(0x123, Bgr555(0x4567))
            .unwrap();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);

        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let palette = reopened
            .project()
            .unwrap()
            .load_shared_palette(smw_us_v1_shared_palette_layout_for_mapper(
                lm_rom::Mapper::LoRom,
            ))
            .unwrap();
        assert_eq!(palette.backend(), SmwPaletteBackend::Legacy);
        assert_eq!(palette.palette().unwrap().colors[0x123], Bgr555(0x4567));
    }

    #[test]
    fn shared_palette_stages_after_fixed_metadata_and_reopens_both_without_live_mutation() {
        use lm_profile::smw_us_v1_lunar_magic_metadata_layout;
        use lm_rom::LunarMagicRomMetadata;

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture =
            std::fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc"))
                .unwrap();
        let mut app = AppState::default();
        app.load_rom(fixture).unwrap();
        let baseline = app.project().unwrap().save_snapshot();
        let mut editor = RomSharedPaletteEditor::default();
        editor.open(&app);
        editor
            .workspace
            .as_mut()
            .unwrap()
            .replace_color(0x123, Bgr555(0x4567))
            .unwrap();

        let layout = smw_us_v1_lunar_magic_metadata_layout();
        let source = app
            .project()
            .unwrap()
            .load_lunar_magic_rom_metadata(layout)
            .unwrap()
            .unwrap();
        let mut attribution = *source.attribution();
        attribution[0x9f] ^= 0x5a;
        let metadata = LunarMagicRomMetadata::from_parts(
            &attribution,
            source.vram_version(),
            source.feature_record(),
        )
        .unwrap();
        let mut staged = app.project().unwrap().clone();
        lm_app::save_lunar_magic_rom_metadata_to_project(&mut staged, &metadata).unwrap();
        editor.stage_recovery_on_project(&app, &mut staged).unwrap();
        let recovery = app
            .recovery_snapshot_with_current_rom(staged.save_snapshot(), Some(0x105))
            .unwrap()
            .unwrap();

        assert_eq!(app.project().unwrap().save_snapshot(), baseline);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        assert_eq!(reopened.current_level(), Some(0x105));
        let project = reopened.project().unwrap();
        assert_eq!(
            project
                .load_lunar_magic_rom_metadata(layout)
                .unwrap()
                .unwrap(),
            metadata
        );
        assert_eq!(
            project
                .load_shared_palette(smw_us_v1_shared_palette_layout_for_mapper(
                    lm_rom::Mapper::LoRom,
                ))
                .unwrap()
                .palette()
                .unwrap()
                .colors[0x123],
            Bgr555(0x4567)
        );
        let logical = project.rom.logical_bytes();
        assert_eq!(
            lm_rom::SnesChecksum::decode(logical, 0x7fdc).unwrap(),
            lm_rom::compute_snes_checksum(logical, 0x7fdc).unwrap()
        );
    }

    #[test]
    fn exlorom_shared_palette_open_and_recovery_use_the_detected_mapper() {
        let (mut installer, _) = pristine_app();
        installer
            .dispatch(Command::ConvertRomTo64MbitExLoRom {
                expected_revision: installer.project_revision(),
            })
            .unwrap();
        installer
            .dispatch(Command::InstallExpandedSharedPalettes {
                rev: installer.project_revision(),
            })
            .unwrap();
        let installed = installer.project().unwrap().save_snapshot();
        let mut app = AppState::default();
        app.load_rom(installed).unwrap();

        let mut editor = RomSharedPaletteEditor::default();
        editor.open(&app);
        let workspace = editor.workspace.as_mut().unwrap();
        assert_eq!(workspace.mapper, lm_rom::Mapper::ExLoRom);
        assert_eq!(workspace.current.backend(), SmwPaletteBackend::Expanded);
        workspace.replace_color(0x234, Bgr555(0x3210)).unwrap();

        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let palette = reopened
            .project()
            .unwrap()
            .load_shared_palette(smw_us_v1_shared_palette_layout_for_mapper(
                lm_rom::Mapper::ExLoRom,
            ))
            .unwrap();
        assert_eq!(palette.backend(), SmwPaletteBackend::Expanded);
        assert_eq!(palette.palette().unwrap().colors[0x234], Bgr555(0x3210));
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
                .load_shared_palette(smw_us_v1_shared_palette_layout_for_mapper(
                    lm_rom::Mapper::LoRom,
                ))
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
                .load_shared_palette(smw_us_v1_shared_palette_layout_for_mapper(
                    lm_rom::Mapper::LoRom,
                ))
                .unwrap(),
            imported
        );
    }

    #[test]
    fn typed_row_paste_replaces_one_aligned_backend_row_atomically() {
        let (app, _) = pristine_app();
        let mut editor = RomSharedPaletteEditor::default();
        editor.open(&app);
        editor.selected = 0x27;
        editor.load_selected().unwrap();
        editor.paste_target = Some(PasteTarget::Row);
        let row = [Bgr555(0x3456); 16];
        let text = native_clipboard::encode_palette_row(&row).unwrap();
        editor.apply_clipboard_paste(&text).unwrap();
        let palette = editor
            .workspace
            .as_ref()
            .unwrap()
            .current
            .palette()
            .unwrap();
        assert_eq!(&palette.colors[0x20..0x30], &row);
        assert_eq!(editor.loaded, Some(0x27));

        let before = editor.workspace.as_ref().unwrap().current.clone();
        editor.paste_target = Some(PasteTarget::Row);
        let invalid = native_clipboard::encode_palette_row(&[Bgr555(0xffff); 16]).unwrap();
        assert!(editor.apply_clipboard_paste(&invalid).is_err());
        assert_eq!(editor.workspace.as_ref().unwrap().current, before);
    }
}
