use crate::{
    document_loader::DocumentLoader, level_editor_forms,
    overworld_editor_animation::OverworldAnimationPanel,
    overworld_editor_palette::OverworldPalettePanel,
    overworld_editor_records::OverworldRecordPanels,
};
use eframe::egui;
use lm_app::{
    AppState, Command, OverworldController, OverworldControllerEdit, OverworldLayerId,
    ProfiledControllerSnapshot,
};
use lm_graphics::PaletteOwnership;
use lm_project::{CompleteOverworldFile, CompleteOverworldShape};

mod commit;
mod lifecycle;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Panel {
    #[default]
    Records,
    Palette,
    Animation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct PendingOpen {
    profiled: ProfiledControllerSnapshot,
    slot: String,
}

struct PendingLoad {
    open: PendingOpen,
    slot: u16,
}

struct Workspace {
    controller: OverworldController,
    profiled: ProfiledControllerSnapshot,
    slot: u16,
    image: lm_rom::RomImage,
    ownership: PaletteOwnership,
}

#[derive(Default)]
pub(crate) struct RomOverworldEditor {
    workspace: Option<Workspace>,
    pending_open: Option<PendingOpen>,
    panel: Panel,
    records: OverworldRecordPanels,
    palette: OverworldPalettePanel,
    animation: OverworldAnimationPanel,
    layer: usize,
    x: usize,
    y: usize,
    tile: String,
    loaded: Option<(u64, usize, usize, usize)>,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    loader: DocumentLoader,
    pending_load: Option<PendingLoad>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
}

impl RomOverworldEditor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        revision: u64,
    ) -> (bool, Option<Command>) {
        if let Some(result) = self.loader.show(context) {
            self.finish_ownership_load(result, revision);
        }
        self.open_dialog(context);
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
            self.load_tile();
            egui::Window::new("ROM Complete Overworld Editor")
                .default_size([820.0, 720.0])
                .vscroll(true)
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
}

impl RomOverworldEditor {
    fn contents(&mut self, ui: &mut egui::Ui, revision: u64) -> Option<Command> {
        let (stale, shape, slot, controller_revision, data, modes, ownership) = {
            let workspace = self.workspace.as_ref()?;
            (
                workspace.controller.revision() != revision,
                workspace.profiled.profile.overworld_shape,
                workspace.slot,
                workspace.controller.revision(),
                workspace.controller.data().clone(),
                workspace.profiled.profile.exanimation_double_size_modes,
                workspace.ownership.clone(),
            )
        };
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed; reopen before editing or committing.",
            );
        }
        self.layer_tile_controls(ui, shape, stale);
        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.panel, Panel::Records, "Records");
            ui.selectable_value(&mut self.panel, Panel::Palette, "Palette");
            ui.selectable_value(&mut self.panel, Panel::Animation, "Animation");
        });
        let file = CompleteOverworldFile {
            source_slot: slot,
            shape,
            data,
        };
        let edit = match self.panel {
            Panel::Records => self.records.show(ui, &file, controller_revision),
            Panel::Palette => self.palette.show(ui, &file.data.palette, &ownership),
            Panel::Animation => {
                self.animation
                    .show(ui, &file.data.animation, &modes, controller_revision)
            }
        };
        if let Some(edit) = edit {
            match edit {
                Ok(edit) if !stale => self.apply(edit),
                Ok(_) => self.error = Some("stale overworld workspace cannot accept edits".into()),
                Err(error) => self.error = Some(error),
            }
        }
        self.commit_controls(ui, stale, revision)
    }

    fn layer_tile_controls(
        &mut self,
        ui: &mut egui::Ui,
        shape: CompleteOverworldShape,
        stale: bool,
    ) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.layer, 0, "Layer 1");
            ui.selectable_value(&mut self.layer, 1, "Layer 2");
        });
        let old_selection = (self.layer, self.x, self.y);
        ui.add(egui::Slider::new(&mut self.x, 0..=shape.width.saturating_sub(1)).text("X"));
        ui.add(egui::Slider::new(&mut self.y, 0..=shape.height.saturating_sub(1)).text("Y"));
        if old_selection != (self.layer, self.x, self.y) {
            self.loaded = None;
            self.load_tile();
        }
        ui.horizontal(|ui| {
            ui.label("Map16 tile");
            ui.text_edit_singleline(&mut self.tile);
        });
        if ui
            .add_enabled(!stale, egui::Button::new("Apply layer tile"))
            .clicked()
        {
            match level_editor_forms::parse_hex_u16(&self.tile, "overworld tile") {
                Ok(tile) => self.apply(OverworldControllerEdit::SetLayerTile {
                    layer: self.layer_id(),
                    x: self.x,
                    y: self.y,
                    tile,
                }),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn commit_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        revision: u64,
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
            .is_some_and(|value| value.controller.is_modified());
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new("Commit all nine overworld payloads"),
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
                egui::Button::new("Commit and reclaim all nine"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged overworld changes"
        } else {
            "No staged changes"
        });
        None
    }

    fn apply(&mut self, edit: OverworldControllerEdit) {
        let Some(workspace) = self.workspace.as_mut() else {
            self.error = Some("overworld workspace is closed".into());
            return;
        };
        if let Err(error) = workspace.controller.apply_edits(&[edit]) {
            self.error = Some(error.to_string());
        } else {
            self.invalidate();
        }
    }

    fn load_tile(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let key = (workspace.controller.revision(), self.layer, self.x, self.y);
        if self.loaded == Some(key) {
            return;
        }
        let shape = workspace.profiled.profile.overworld_shape;
        self.x = self.x.min(shape.width.saturating_sub(1));
        self.y = self.y.min(shape.height.saturating_sub(1));
        let tiles = if self.layer == 0 {
            &workspace.controller.data().layers.layer1.tiles
        } else {
            &workspace.controller.data().layers.layer2.tiles
        };
        if let Some(tile) = tiles.get(self.y * shape.width + self.x) {
            self.tile = format!("{tile:04X}");
        }
        self.loaded = Some((workspace.controller.revision(), self.layer, self.x, self.y));
    }

    fn layer_id(&self) -> OverworldLayerId {
        if self.layer == 0 {
            OverworldLayerId::Layer1
        } else {
            OverworldLayerId::Layer2
        }
    }

    fn invalidate(&mut self) {
        self.loaded = None;
        self.records.invalidate();
        self.animation.invalidate();
    }
}
