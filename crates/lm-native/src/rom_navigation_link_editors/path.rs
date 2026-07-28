use crate::level_editor_forms::{parse_hex_u8, parse_hex_u16};
use eframe::egui;
use lm_app::{AppState, Command};
use lm_overworld::{
    OverworldEndpoint, OverworldPathLink, OverworldPathLinkTable, OverworldPathTarget,
};
use lm_profile::smw_us_v1_overworld_path_patch_locator;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    revision: u64,
    original: OverworldPathLinkTable,
    current: OverworldPathLinkTable,
}

#[derive(Default)]
pub(crate) struct RomOverworldPathLinkEditor {
    workspace: Option<Workspace>,
    form: PathLinkForm,
    count: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

#[derive(Default)]
struct PathLinkForm {
    index: String,
    source_x: String,
    source_y: String,
    source_submap: String,
    destination_x: String,
    destination_y: String,
    destination_submap: String,
    target_x: String,
    target_y: String,
    loaded: Option<usize>,
}

impl RomOverworldPathLinkEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        let loaded = app
            .project()
            .ok_or_else(|| "open a supported ROM first".to_owned())
            .and_then(|project| {
                project
                    .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
                    .map_err(|error| error.to_string())
            });
        match loaded {
            Ok(loaded) => {
                self.count = format!("{:02X}", loaded.table.links.len());
                self.workspace = Some(Workspace {
                    revision: app.project_revision(),
                    original: loaded.table.clone(),
                    current: loaded.table,
                });
                self.form.index = "00".into();
                self.load_selected().ok();
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if workspace.current == workspace.original {
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
        revision: u64,
    ) -> (bool, Option<Command>) {
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new("ROM Overworld Path Links")
                .default_size([600.0, 470.0])
                .show(context, |ui| command = self.contents(ui, revision));
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, revision: u64) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.revision != revision;
        let dirty = workspace.current != workspace.original;
        ui.label("Lossless source/destination endpoints and engine target bytes. Hexadecimal.");
        ui.label(format!(
            "Staged path links: {}",
            workspace.current.links.len()
        ));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after this table was opened. Reopen before committing.",
            );
        }
        self.form_ui(ui);
        ui.horizontal(|ui| {
            ui.label("Table count (00–80)");
            ui.text_edit_singleline(&mut self.count);
            if ui
                .add_enabled(!stale, egui::Button::new("Resize table"))
                .clicked()
                && let Err(error) = self.resize()
            {
                self.error = Some(error);
            }
        });
        let mut command = None;
        ui.horizontal(|ui| {
            if ui.button("Load link").clicked()
                && let Err(error) = self.load_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(!stale, egui::Button::new("Apply link"))
                .clicked()
                && let Err(error) = self.apply_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(dirty && !stale, egui::Button::new("Commit links to ROM"))
                .clicked()
            {
                match self.prepare_commit(revision) {
                    Ok(prepared) => command = prepared,
                    Err(error) => self.error = Some(error),
                }
            }
            ui.label(if dirty { "Staged" } else { "Unchanged" });
        });
        command
    }

    fn form_ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("rom-overworld-path-link-form")
            .striped(true)
            .show(ui, |ui| {
                form_row(ui, "Index", &mut self.form.index, &mut self.form.loaded);
                form_row(ui, "Source X", &mut self.form.source_x, &mut None::<usize>);
                form_row(ui, "Source Y", &mut self.form.source_y, &mut None::<usize>);
                form_row(
                    ui,
                    "Source submap",
                    &mut self.form.source_submap,
                    &mut None::<usize>,
                );
                form_row(
                    ui,
                    "Destination X",
                    &mut self.form.destination_x,
                    &mut None::<usize>,
                );
                form_row(
                    ui,
                    "Destination Y",
                    &mut self.form.destination_y,
                    &mut None::<usize>,
                );
                form_row(
                    ui,
                    "Destination submap",
                    &mut self.form.destination_submap,
                    &mut None::<usize>,
                );
                form_row(
                    ui,
                    "Target X tile",
                    &mut self.form.target_x,
                    &mut None::<usize>,
                );
                form_row(
                    ui,
                    "Target Y tile",
                    &mut self.form.target_y,
                    &mut None::<usize>,
                );
            });
    }

    fn selected_index(&self) -> Result<usize, String> {
        let index = usize::from(parse_hex_u8(&self.form.index, "link index")?);
        let len = self
            .workspace
            .as_ref()
            .ok_or_else(|| "path-link workspace is closed".to_owned())?
            .current
            .links
            .len();
        if index >= len {
            return Err(format!("link index must be below {len:02X}"));
        }
        Ok(index)
    }

    fn load_selected(&mut self) -> Result<(), String> {
        let index = self.selected_index()?;
        let link = self.workspace.as_ref().unwrap().current.links[index];
        self.form.set(index, link);
        Ok(())
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        let index = self.selected_index()?;
        if self.form.loaded != Some(index) {
            return Err("load the selected link before applying it".into());
        }
        let link = self.form.parse()?;
        let workspace = self.workspace.as_mut().unwrap();
        let mut staged = workspace.current.clone();
        staged.links[index] = link;
        staged.encode_planes().map_err(|error| error.to_string())?;
        workspace.current = staged;
        Ok(())
    }

    fn resize(&mut self) -> Result<(), String> {
        let count = usize::from(parse_hex_u8(&self.count, "path-link count")?);
        if count > OverworldPathLinkTable::MAX_LINKS {
            return Err("path-link count must be at most 80".into());
        }
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "path-link workspace is closed".to_owned())?;
        workspace.current.links.resize(count, blank_path_link());
        workspace
            .current
            .encode_planes()
            .map_err(|error| error.to_string())?;
        self.form.loaded = None;
        Ok(())
    }

    fn prepare_commit(&self, revision: u64) -> Result<Option<Command>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "path-link workspace is closed".to_owned())?;
        if workspace.revision != revision {
            return Err("stale path-link workspace cannot be committed".into());
        }
        if workspace.current == workspace.original {
            return Ok(None);
        }
        workspace
            .current
            .encode_planes()
            .map_err(|error| error.to_string())?;
        Ok(Some(Command::ReplaceNativeOverworldPathLinks {
            rev: workspace.revision,
            table: Box::new(workspace.current.clone()),
        }))
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard path-link changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged path-link table has not been committed.");
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
            egui::Window::new("Path-link editor error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.form.loaded = None;
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

impl PathLinkForm {
    fn set(&mut self, index: usize, link: OverworldPathLink) {
        self.source_x = format!("{:04X}", link.source.x);
        self.source_y = format!("{:04X}", link.source.y);
        self.source_submap = format!("{:02X}", link.source.submap);
        self.destination_x = format!("{:04X}", link.destination.x);
        self.destination_y = format!("{:04X}", link.destination.y);
        self.destination_submap = format!("{:02X}", link.destination.submap);
        self.target_x = format!("{:02X}", link.target.x_tile);
        self.target_y = format!("{:02X}", link.target.y_tile);
        self.loaded = Some(index);
    }

    fn parse(&self) -> Result<OverworldPathLink, String> {
        Ok(OverworldPathLink {
            source: OverworldEndpoint {
                x: parse_hex_u16(&self.source_x, "source X")?,
                y: parse_hex_u16(&self.source_y, "source Y")?,
                submap: parse_hex_u8(&self.source_submap, "source submap")?,
            },
            destination: OverworldEndpoint {
                x: parse_hex_u16(&self.destination_x, "destination X")?,
                y: parse_hex_u16(&self.destination_y, "destination Y")?,
                submap: parse_hex_u8(&self.destination_submap, "destination submap")?,
            },
            target: OverworldPathTarget {
                x_tile: parse_hex_u8(&self.target_x, "target X")?,
                y_tile: parse_hex_u8(&self.target_y, "target Y")?,
            },
        })
    }
}

fn form_row(ui: &mut egui::Ui, label: &str, value: &mut String, loaded: &mut Option<usize>) {
    ui.label(label);
    if ui.text_edit_singleline(value).changed() && label == "Index" {
        *loaded = None;
    }
    ui.end_row();
}

fn blank_path_link() -> OverworldPathLink {
    OverworldPathLink {
        source: OverworldEndpoint {
            x: 0xffff,
            y: 0xffff,
            submap: 0xff,
        },
        destination: OverworldEndpoint {
            x: 0xffff,
            y: 0xffff,
            submap: 0xff,
        },
        target: OverworldPathTarget {
            x_tile: 0xff,
            y_tile: 0xff,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn pristine_table_grows_installs_and_reopens_exact_link() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original).unwrap();
        let mut editor = RomOverworldPathLinkEditor::default();
        editor.open(&app);
        assert_eq!(editor.workspace.as_ref().unwrap().current.links.len(), 14);
        editor.count = "0F".into();
        editor.resize().unwrap();
        editor.form.index = "0E".into();
        editor.load_selected().unwrap();
        editor.form.target_x = "2A".into();
        editor.apply_selected().unwrap();
        app.dispatch(
            editor
                .prepare_commit(app.project_revision())
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
            .unwrap();
        assert_eq!(reopened.table.links.len(), 15);
        assert_eq!(reopened.table.links[14].target.x_tile, 0x2a);
    }

    #[test]
    fn selection_identity_and_stale_revision_are_enforced() {
        let table = OverworldPathLinkTable {
            links: vec![blank_path_link()],
        };
        let mut editor = RomOverworldPathLinkEditor {
            workspace: Some(Workspace {
                revision: 3,
                original: table.clone(),
                current: table,
            }),
            ..Default::default()
        };
        editor.form.index = "00".into();
        assert!(editor.apply_selected().is_err());
        editor.load_selected().unwrap();
        assert!(editor.prepare_commit(4).is_err());
        editor.form.target_x = "7A".into();
        editor.apply_selected().unwrap();
        assert!(!editor.request_close(true));
        assert!(editor.is_open());
    }
}
