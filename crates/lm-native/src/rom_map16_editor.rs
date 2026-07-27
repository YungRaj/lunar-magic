use crate::{level_editor_forms, map16_subtile_form, native_clipboard};
use eframe::egui;
use lm_app::{
    AppState, Command, Map16Controller, Map16ControllerEdit, RevisionProfile, SmwMap16Controller,
};
use lm_level::{Map16Address, Map16Page};

mod commit;
mod lifecycle;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    controller: Controller,
    profile: Option<RevisionProfile>,
    image: lm_rom::RomImage,
    internal_header: usize,
}

enum Controller {
    Profile(Map16Controller),
    Smw(SmwMap16Controller),
}

impl Controller {
    fn revision(&self) -> u64 {
        match self {
            Self::Profile(controller) => controller.revision(),
            Self::Smw(controller) => controller.revision(),
        }
    }

    fn set(&self) -> &lm_level::Map16Set {
        match self {
            Self::Profile(controller) => controller.set(),
            Self::Smw(controller) => controller.set(),
        }
    }

    fn is_modified(&self) -> bool {
        match self {
            Self::Profile(controller) => controller.is_modified(),
            Self::Smw(controller) => controller.is_modified(),
        }
    }

    fn apply_edits(&mut self, edits: &[Map16ControllerEdit]) -> Result<(), String> {
        match self {
            Self::Profile(controller) => controller.apply_edits(edits).map_err(|e| e.to_string()),
            Self::Smw(controller) => controller.apply_edits(edits).map_err(|e| e.to_string()),
        }
    }

    const fn supports_reclamation(&self) -> bool {
        matches!(self, Self::Profile(_))
    }
}

#[derive(Default)]
pub(crate) struct RomMap16Editor {
    workspace: Option<Workspace>,
    page: usize,
    tile: usize,
    quadrant: usize,
    subtile: map16_subtile_form::SubtileForm,
    acts_like: String,
    loaded: Option<(u64, usize, usize, usize)>,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
}

impl RomMap16Editor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> (bool, Option<Command>) {
        let mut command = match self.manifest_loader.show(context, project_revision) {
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
            self.clamp();
            self.load();
            egui::Window::new("ROM Complete Map16 Editor")
                .default_size([560.0, 650.0])
                .show(context, |ui| {
                    if let Some(ui_command) = self.contents(ui, project_revision) {
                        command = Some(ui_command);
                    }
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }
    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        let (stale, pages) = {
            let workspace = self.workspace.as_ref()?;
            (
                workspace.controller.revision() != project_revision,
                workspace.controller.set().pages.len(),
            )
        };
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed; reopen before editing or committing.",
            );
        }
        self.selection_and_clipboard(ui, stale, pages, pasted.as_deref());
        self.tile_fields(ui, stale, pages);
        self.commit_controls(ui, stale, project_revision)
    }

    fn selection_and_clipboard(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        pages: usize,
        pasted: Option<&str>,
    ) {
        let old = (self.page, self.tile, self.quadrant);
        ui.add(egui::Slider::new(&mut self.page, 0..=pages.saturating_sub(1)).text("Page"));
        ui.add(egui::Slider::new(&mut self.tile, 0..=Map16Page::TILE_COUNT - 1).text("Tile"));
        egui::ComboBox::from_label("Quadrant")
            .selected_text(map16_subtile_form::quadrant_name(self.quadrant))
            .show_ui(ui, |ui| {
                for index in 0..4 {
                    ui.selectable_value(
                        &mut self.quadrant,
                        index,
                        map16_subtile_form::quadrant_name(index),
                    );
                }
            });
        if old != (self.page, self.tile, self.quadrant) {
            self.loaded = None;
            self.load();
        }
        ui.heading(format!("Map16 {:02X}:{:02X}", self.page, self.tile));
        ui.horizontal(|ui| {
            if ui.button("Copy tile").clicked()
                && let Some(tile) = self.current_tile()
            {
                match native_clipboard::encode_map16_tile(tile) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(!stale, egui::Button::new("Paste tile"))
                .clicked()
            {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if !stale && let Some(text) = pasted {
            match native_clipboard::decode_map16_tile(text) {
                Ok(tile) => self.apply(Map16ControllerEdit::ReplaceTiles {
                    replacements: vec![(self.address(), tile)],
                    resolution_limit: pages * Map16Page::TILE_COUNT,
                }),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn tile_fields(&mut self, ui: &mut egui::Ui, stale: bool, pages: usize) {
        ui.horizontal(|ui| {
            ui.label("8×8 tile");
            ui.text_edit_singleline(&mut self.subtile.tile);
        });
        ui.add(egui::Slider::new(&mut self.subtile.palette, 0..=7).text("Palette"));
        ui.checkbox(&mut self.subtile.priority, "Priority");
        ui.checkbox(&mut self.subtile.x_flip, "X flip");
        ui.checkbox(&mut self.subtile.y_flip, "Y flip");
        let mut edit = None;
        if ui
            .add_enabled(!stale, egui::Button::new("Apply subtile"))
            .clicked()
        {
            edit = Some(
                self.subtile
                    .parse()
                    .map(|subtile| Map16ControllerEdit::SetSubtile {
                        address: self.address(),
                        quadrant: map16_subtile_form::quadrant(self.quadrant),
                        subtile,
                        resolution_limit: pages * Map16Page::TILE_COUNT,
                    }),
            );
        }
        ui.horizontal(|ui| {
            ui.label("Acts Like");
            ui.text_edit_singleline(&mut self.acts_like);
        });
        if ui
            .add_enabled(!stale, egui::Button::new("Apply Acts Like"))
            .clicked()
        {
            edit = Some(
                level_editor_forms::parse_hex_u16(&self.acts_like, "Acts Like").map(|acts_like| {
                    Map16ControllerEdit::SetActsLike {
                        address: self.address(),
                        acts_like,
                        resolution_limit: pages * Map16Page::TILE_COUNT,
                    }
                }),
            );
        }
        if let Some(edit) = edit {
            match edit {
                Ok(edit) => self.apply(edit),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn commit_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        project_revision: u64,
    ) -> Option<Command> {
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
                egui::Button::new("Commit complete Map16 set to ROM"),
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
                modified
                    && !stale
                    && !self.manifest_loader.is_running()
                    && self
                        .workspace
                        .as_ref()
                        .is_some_and(|workspace| workspace.controller.supports_reclamation()),
                egui::Button::new("Commit and reclaim"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(project_revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged Map16 changes"
        } else {
            "No staged changes"
        });
        None
    }
    fn apply(&mut self, edit: Map16ControllerEdit) {
        let Some(workspace) = self.workspace.as_mut() else {
            self.error = Some("Map16 workspace is closed".into());
            return;
        };
        if let Err(error) = workspace.controller.apply_edits(&[edit]) {
            self.error = Some(error);
        } else {
            self.invalidate();
        }
    }
    fn load(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let key = (
            workspace.controller.revision(),
            self.page,
            self.tile,
            self.quadrant,
        );
        if self.loaded == Some(key) {
            return;
        }
        if let Some(tile) = workspace
            .controller
            .set()
            .pages
            .get(self.page)
            .and_then(|p| p.tiles.get(self.tile))
        {
            self.subtile = map16_subtile_form::SubtileForm::from_subtile(
                map16_subtile_form::quadrant_value(*tile, self.quadrant),
            );
            self.acts_like = format!("{:04X}", tile.acts_like);
            self.loaded = Some(key);
        }
    }
    fn address(&self) -> Map16Address {
        Map16Address {
            page: self.page,
            tile: self.tile,
        }
    }
    fn current_tile(&self) -> Option<lm_level::Map16Tile> {
        self.workspace
            .as_ref()?
            .controller
            .set()
            .pages
            .get(self.page)?
            .tiles
            .get(self.tile)
            .copied()
    }
    fn clamp(&mut self) {
        let pages = self
            .workspace
            .as_ref()
            .map_or(0, |w| w.controller.set().pages.len());
        self.page = self.page.min(pages.saturating_sub(1));
        self.tile = self.tile.min(Map16Page::TILE_COUNT - 1);
    }
    fn invalidate(&mut self) {
        self.loaded = None;
    }
}
