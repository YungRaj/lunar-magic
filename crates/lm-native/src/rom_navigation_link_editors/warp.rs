use crate::level_editor_forms::{parse_hex_u8, parse_hex_u16};
use eframe::egui;
use lm_app::{AppState, Command};
use lm_overworld::{OverworldWarpEndpoint, OverworldWarpLink, OverworldWarpLinkTable};
use lm_profile::smw_us_v1_overworld_warp_patch_locator;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    revision: u64,
    original: OverworldWarpLinkTable,
    current: OverworldWarpLinkTable,
}

#[derive(Default)]
pub(crate) struct RomOverworldWarpLinkEditor {
    workspace: Option<Workspace>,
    form: WarpLinkForm,
    count: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

#[derive(Default)]
struct WarpLinkForm {
    index: String,
    source_vertical: String,
    source_horizontal: String,
    destination_vertical: String,
    destination_horizontal: String,
    loaded: Option<usize>,
}

impl RomOverworldWarpLinkEditor {
    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        if workspace.current == workspace.original {
            return None;
        }
        let content_revision = workspace.current.links.iter().fold(
            0x4f57_5741_5250_4c4e_u64 ^ workspace.current.links.len() as u64,
            |revision, link| {
                revision.rotate_left(7)
                    ^ u64::from(link.source.packed_vertical)
                    ^ u64::from(link.source.horizontal_tile).rotate_left(17)
                    ^ u64::from(link.destination.packed_vertical).rotate_left(33)
                    ^ u64::from(link.destination.horizontal_tile).rotate_left(49)
            },
        );
        Some(
            app.project_revision().wrapping_mul(0xa24b_aed4_963e_e407)
                ^ workspace.revision.rotate_left(31)
                ^ content_revision,
        )
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "warp-link workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale warp-link workspace cannot be recovered".into());
        }
        if workspace.current == workspace.original {
            return Ok(app.recovery_snapshot());
        }
        app.recovery_snapshot_with_overworld_warp_links(&workspace.current, app.current_level())
            .map_err(|error| error.to_string())
    }

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
                    .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
                    .map_err(|error| error.to_string())
            });
        match loaded {
            Ok(loaded) => {
                self.count = format!("{:03X}", loaded.table.links.len());
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
            egui::Window::new("ROM Overworld Warp Links")
                .default_size([560.0, 390.0])
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
        ui.label("Four lossless coordinate words per warp. Packed vertical fields remain opaque.");
        ui.label(format!(
            "Staged warp links: {}",
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
            ui.label("Table count (000–100)");
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
        egui::Grid::new("rom-overworld-warp-link-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Index");
                if ui.text_edit_singleline(&mut self.form.index).changed() {
                    self.form.loaded = None;
                }
                ui.end_row();
                word_row(ui, "Source packed vertical", &mut self.form.source_vertical);
                word_row(
                    ui,
                    "Source horizontal tile",
                    &mut self.form.source_horizontal,
                );
                word_row(
                    ui,
                    "Destination packed vertical",
                    &mut self.form.destination_vertical,
                );
                word_row(
                    ui,
                    "Destination horizontal tile",
                    &mut self.form.destination_horizontal,
                );
            });
    }

    fn selected_index(&self) -> Result<usize, String> {
        let index = usize::from(parse_hex_u8(&self.form.index, "warp-link index")?);
        let len = self
            .workspace
            .as_ref()
            .ok_or_else(|| "warp-link workspace is closed".to_owned())?
            .current
            .links
            .len();
        if index >= len {
            return Err(format!("warp-link index must be below {len:03X}"));
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
            return Err("load the selected warp link before applying it".into());
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
        let count = usize::from(parse_hex_u16(&self.count, "warp-link count")?);
        if count > OverworldWarpLinkTable::MAX_LINKS {
            return Err("warp-link count must be at most 100".into());
        }
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "warp-link workspace is closed".to_owned())?;
        workspace.current.links.resize(count, blank_warp_link());
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
            .ok_or_else(|| "warp-link workspace is closed".to_owned())?;
        if workspace.revision != revision {
            return Err("stale warp-link workspace cannot be committed".into());
        }
        if workspace.current == workspace.original {
            return Ok(None);
        }
        workspace
            .current
            .encode_planes()
            .map_err(|error| error.to_string())?;
        Ok(Some(Command::ReplaceNativeOverworldWarpLinks {
            rev: workspace.revision,
            table: Box::new(workspace.current.clone()),
        }))
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard warp-link changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged warp-link table has not been committed.");
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
            egui::Window::new("Warp-link editor error").show(context, |ui| {
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

impl WarpLinkForm {
    fn set(&mut self, index: usize, link: OverworldWarpLink) {
        self.source_vertical = format!("{:04X}", link.source.packed_vertical);
        self.source_horizontal = format!("{:04X}", link.source.horizontal_tile);
        self.destination_vertical = format!("{:04X}", link.destination.packed_vertical);
        self.destination_horizontal = format!("{:04X}", link.destination.horizontal_tile);
        self.loaded = Some(index);
    }

    fn parse(&self) -> Result<OverworldWarpLink, String> {
        Ok(OverworldWarpLink {
            source: OverworldWarpEndpoint {
                packed_vertical: parse_hex_u16(&self.source_vertical, "source packed vertical")?,
                horizontal_tile: parse_hex_u16(&self.source_horizontal, "source horizontal")?,
            },
            destination: OverworldWarpEndpoint {
                packed_vertical: parse_hex_u16(
                    &self.destination_vertical,
                    "destination packed vertical",
                )?,
                horizontal_tile: parse_hex_u16(
                    &self.destination_horizontal,
                    "destination horizontal",
                )?,
            },
        })
    }
}

fn word_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.text_edit_singleline(value);
    ui.end_row();
}

fn blank_warp_link() -> OverworldWarpLink {
    OverworldWarpLink {
        source: OverworldWarpEndpoint {
            packed_vertical: 0xffff,
            horizontal_tile: 0xffff,
        },
        destination: OverworldWarpEndpoint {
            packed_vertical: 0xffff,
            horizontal_tile: 0xffff,
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
        let mut editor = RomOverworldWarpLinkEditor::default();
        editor.open(&app);
        assert_eq!(editor.workspace.as_ref().unwrap().current.links.len(), 27);
        editor.count = "01C".into();
        editor.resize().unwrap();
        editor.form.index = "1B".into();
        editor.load_selected().unwrap();
        editor.form.destination_horizontal = "1234".into();
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
            .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
            .unwrap();
        assert_eq!(reopened.table.links.len(), 28);
        assert_eq!(reopened.table.links[27].destination.horizontal_tile, 0x1234);
    }

    #[test]
    fn selection_identity_and_stale_revision_are_enforced() {
        let table = OverworldWarpLinkTable {
            links: vec![blank_warp_link()],
        };
        let mut editor = RomOverworldWarpLinkEditor {
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
        editor.form.source_vertical = "1234".into();
        editor.apply_selected().unwrap();
        assert!(!editor.request_close(true));
        assert!(editor.is_open());
    }

    #[test]
    fn staged_pristine_warp_growth_recovers_complete_installed_table() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut editor = RomOverworldWarpLinkEditor::default();
        editor.open(&app);
        editor.count = "01C".into();
        editor.resize().unwrap();
        editor.form.index = "1B".into();
        editor.load_selected().unwrap();
        editor.form.source_vertical = "1234".into();
        editor.form.source_horizontal = "2345".into();
        editor.form.destination_vertical = "3456".into();
        editor.form.destination_horizontal = "4567".into();
        editor.apply_selected().unwrap();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let table = &reopened
            .project()
            .unwrap()
            .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
            .unwrap()
            .table;
        assert_eq!(table.links.len(), 28);
        assert_eq!(table.links[27].source.packed_vertical, 0x1234);
        assert_eq!(table.links[27].source.horizontal_tile, 0x2345);
        assert_eq!(table.links[27].destination.packed_vertical, 0x3456);
        assert_eq!(table.links[27].destination.horizontal_tile, 0x4567);
    }

    #[test]
    fn staged_installed_warp_update_preserves_prior_tail_link() {
        let mut installer = AppState::default();
        installer
            .load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut first = RomOverworldWarpLinkEditor::default();
        first.open(&installer);
        first.count = "01C".into();
        first.resize().unwrap();
        first.form.index = "1B".into();
        first.load_selected().unwrap();
        first.form.destination_horizontal = "6789".into();
        first.apply_selected().unwrap();
        installer
            .dispatch(first.prepare_commit(0).unwrap().unwrap())
            .unwrap();

        let mut app = AppState::default();
        app.load_rom(installer.project().unwrap().save_snapshot())
            .unwrap();
        let mut editor = RomOverworldWarpLinkEditor::default();
        editor.open(&app);
        editor.form.index = "00".into();
        editor.load_selected().unwrap();
        editor.form.source_vertical = "789A".into();
        editor.apply_selected().unwrap();

        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let table = &reopened
            .project()
            .unwrap()
            .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
            .unwrap()
            .table;
        assert_eq!(table.links.len(), 28);
        assert_eq!(table.links[0].source.packed_vertical, 0x789a);
        assert_eq!(table.links[27].destination.horizontal_tile, 0x6789);
    }
}
