use crate::{document_loader::DocumentLoader, native_clipboard};
use eframe::egui;
use lm_app::{
    AppState, Command, PaletteController, PaletteControllerEdit, ProfiledControllerSnapshot,
    RevisionProfile,
};
use lm_graphics::{Bgr555, PaletteChange, Rgb8};

mod commit;
mod lifecycle;
mod ownership;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    controller: PaletteController,
    profile: RevisionProfile,
    slot: u16,
    image: lm_rom::RomImage,
    internal_header: usize,
}

struct PendingLoad {
    profiled: ProfiledControllerSnapshot,
}

#[derive(Default)]
pub(crate) struct RomPaletteEditor {
    workspace: Option<Workspace>,
    selected: usize,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    loader: DocumentLoader,
    pending_load: Option<PendingLoad>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
}

impl RomPaletteEditor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        revision: u64,
    ) -> (bool, Option<Command>) {
        if let Some(result) = self.loader.show(context) {
            self.finish_ownership_load(result, revision);
        }
        let mut command = match self.manifest_loader.show(context, revision) {
            Some(Ok(manifest)) => match self.prepare_commit_owned(&manifest) {
                Ok(command) => Some(command),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            },
            Some(Err(error)) => {
                self.error = Some(error);
                None
            }
            None => None,
        };
        if self.workspace.is_some() {
            egui::Window::new("ROM Palette Editor")
                .default_size([600.0, 560.0])
                .show(context, |ui| {
                    if let Some(ui_command) = self.contents(ui, revision) {
                        command = Some(ui_command);
                    }
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }
    fn contents(&mut self, ui: &mut egui::Ui, revision: u64) -> Option<Command> {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.controller.revision() != revision;
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed; reopen before editing or committing.",
            );
        }
        self.palette_surface(ui, stale, pasted);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Allocation logical PC hex");
            ui.text_edit_singleline(&mut self.search_start);
            ui.label("..");
            ui.text_edit_singleline(&mut self.search_end);
        });
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|w| w.controller.is_modified());
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new("Commit palette to ROM"),
            )
            .clicked()
        {
            match self.prepare_commit() {
                Ok(command) => {
                    return Some(command);
                }
                Err(error) => self.error = Some(error),
            }
        }
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new("Commit and reclaim"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged palette changes"
        } else {
            "No staged changes"
        });
        None
    }
    fn palette_surface(&mut self, ui: &mut egui::Ui, stale: bool, pasted: Option<String>) {
        let Some(workspace) = self.workspace.as_ref() else {
            self.error = Some("palette workspace is closed".into());
            return;
        };
        let colors = workspace.controller.palette().colors.clone();
        self.selected = self.selected.min(colors.len().saturating_sub(1));
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .show(ui, |ui| {
                egui::Grid::new("rom-palette-grid")
                    .spacing([3.0, 3.0])
                    .show(ui, |ui| {
                        for (index, color) in colors.iter().copied().enumerate() {
                            let rgb = color.to_rgb8();
                            let label = if index == self.selected { "•" } else { "" };
                            if ui
                                .add_sized(
                                    [25.0, 25.0],
                                    egui::Button::new(label).fill(egui::Color32::from_rgb(
                                        rgb.red, rgb.green, rgb.blue,
                                    )),
                                )
                                .clicked()
                            {
                                self.selected = index;
                            }
                            if index % 16 == 15 {
                                ui.end_row();
                            }
                        }
                    });
            });
        if let Some(color) = colors.get(self.selected).copied() {
            let rgb = color.to_rgb8();
            let mut value = [rgb.red, rgb.green, rgb.blue];
            ui.label(format!(
                "Color {:03X} — raw BGR555 {:04X}",
                self.selected, color.0
            ));
            let owner = self
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.controller.ownership().owner(self.selected));
            let editable = ownership::show(ui, owner);
            ui.horizontal(|ui| {
                if ui.button("Copy color").clicked() {
                    match native_clipboard::encode_palette_color(color) {
                        Ok(text) => ui.ctx().copy_text(text),
                        Err(error) => self.error = Some(error),
                    }
                }
                if ui
                    .add_enabled(!stale && editable, egui::Button::new("Paste color"))
                    .clicked()
                {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                }
            });
            if !stale
                && editable
                && let Some(text) = pasted
            {
                match native_clipboard::decode_palette_color(&text) {
                    Ok(color) => {
                        self.apply(PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
                            index: self.selected,
                            color,
                        }]));
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled_ui(!stale && editable, |ui| {
                    ui.color_edit_button_srgb(&mut value)
                })
                .inner
                .changed()
            {
                self.apply(PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
                    index: self.selected,
                    color: Bgr555::from_rgb8(Rgb8 {
                        red: value[0],
                        green: value[1],
                        blue: value[2],
                    }),
                }]));
            }
        }
    }
    fn apply(&mut self, edit: PaletteControllerEdit) {
        let Some(workspace) = self.workspace.as_mut() else {
            self.error = Some("palette workspace is closed".into());
            return;
        };
        if let Err(error) = workspace.controller.apply_edits(&[edit]) {
            self.error = Some(error.to_string());
        }
    }
}
