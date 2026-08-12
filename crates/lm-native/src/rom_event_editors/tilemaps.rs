use crate::level_editor_forms::{parse_hex_u8, parse_hex_u16};
use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey, LocalizationCatalog, UiTextKey};
use lm_overworld::EventTilemapBuffers;
use lm_profile::{SmwUsV1EventTilemapStorage, load_smw_us_v1_event_tilemaps};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Plane {
    #[default]
    PrimaryLow,
    PrimaryHigh,
    SecondaryHigh,
}

struct Workspace {
    revision: u64,
    original: EventTilemapBuffers,
    current: EventTilemapBuffers,
    storage: SmwUsV1EventTilemapStorage,
}

#[derive(Default)]
pub(crate) struct RomOverworldEventTilemapEditor {
    workspace: Option<Workspace>,
    tile: String,
    plane: Plane,
    value: String,
    loaded: Option<(usize, Plane)>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomOverworldEventTilemapEditor {
    pub(crate) fn staged_recovery_buffers<'a>(
        &'a self,
        app: &AppState,
    ) -> Result<Option<&'a EventTilemapBuffers>, String> {
        let Some(workspace) = self.workspace.as_ref() else {
            return Ok(None);
        };
        if workspace.revision != app.project_revision() {
            return Err("stale event-tilemap workspace cannot be recovered".into());
        }
        Ok((workspace.current != workspace.original).then_some(&workspace.current))
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        if workspace.current == workspace.original {
            return None;
        }
        let content_revision = workspace
            .current
            .primary_bytes()
            .iter()
            .chain(workspace.current.secondary_high_bytes())
            .fold(0x4556_5449_4c45_4d50_u64, |revision, byte| {
                revision.rotate_left(5) ^ u64::from(*byte)
            });
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
            .ok_or_else(|| "event-tilemap workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale event-tilemap workspace cannot be recovered".into());
        }
        if workspace.current == workspace.original {
            return Ok(app.recovery_snapshot());
        }
        let mut staged = app.project().ok_or("open a supported ROM first")?.clone();
        lm_app::save_native_overworld_event_tilemaps_to_project(&mut staged, &workspace.current)
            .map_err(|error| error.to_string())?;
        app.recovery_snapshot_with_current_rom(staged.save_snapshot(), app.current_level())
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
                load_smw_us_v1_event_tilemaps(project).map_err(|error| error.to_string())
            });
        match loaded {
            Ok(loaded) => {
                self.tile = "000".into();
                self.plane = Plane::PrimaryLow;
                self.workspace = Some(Workspace {
                    revision: app.project_revision(),
                    original: loaded.buffers.clone(),
                    current: loaded.buffers,
                    storage: loaded.storage,
                });
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
        catalog: Option<&LocalizationCatalog>,
    ) -> (bool, Option<Command>) {
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::EventTilemapEditorTitle))
                .default_size([540.0, 320.0])
                .show(context, |ui| command = self.contents(ui, revision, catalog));
        }
        let approved = self.close_confirmation(context, catalog);
        self.show_error(context, catalog);
        (approved, command)
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.revision != revision;
        let dirty = workspace.current != workspace.original;
        let storage = match workspace.storage {
            SmwUsV1EventTilemapStorage::Pristine => {
                text(catalog, ExtendedUiTextKey::EventTilemapPristineStorage)
            }
            SmwUsV1EventTilemapStorage::Installed(_) => {
                text(catalog, ExtendedUiTextKey::EventTilemapInstalledStorage)
            }
        };
        ui.label(text(catalog, ExtendedUiTextKey::EventTilemapDescription));
        ui.label(
            text(catalog, ExtendedUiTextKey::EventTilemapLoadedStorageFormat)
                .replace("{storage}", &storage),
        );
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                text(catalog, ExtendedUiTextKey::EventTilemapStaleNotice),
            );
        }
        egui::Grid::new("rom-overworld-event-tilemap-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label(text(catalog, ExtendedUiTextKey::EventTilemapTileIndex));
                if ui.text_edit_singleline(&mut self.tile).changed() {
                    self.loaded = None;
                }
                ui.end_row();
                ui.label(text(catalog, ExtendedUiTextKey::EventTilemapPlane));
                egui::ComboBox::from_id_salt("event-tilemap-plane")
                    .selected_text(self.plane.label(catalog))
                    .show_ui(ui, |ui| {
                        for plane in Plane::ALL {
                            if ui
                                .selectable_value(&mut self.plane, plane, plane.label(catalog))
                                .changed()
                            {
                                self.loaded = None;
                            }
                        }
                    });
                ui.end_row();
                ui.label(text(catalog, ExtendedUiTextKey::EventTilemapByteValue));
                ui.text_edit_singleline(&mut self.value);
                ui.end_row();
            });
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .button(text(catalog, ExtendedUiTextKey::EventTilemapLoadByte))
                .clicked()
                && let Err(error) = self.load_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    !stale,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::EventTilemapApplyByte)),
                )
                .clicked()
                && let Err(error) = self.apply_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    dirty && !stale,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::EventTilemapCommit)),
                )
                .clicked()
            {
                match self.prepare_commit(revision) {
                    Ok(prepared) => command = prepared,
                    Err(error) => self.error = Some(error),
                }
            }
            ui.label(text(
                catalog,
                if dirty {
                    ExtendedUiTextKey::EventTilemapStaged
                } else {
                    ExtendedUiTextKey::EventTilemapUnchanged
                },
            ));
        });
        command
    }

    fn selection(&self) -> Result<(usize, Plane), String> {
        let tile = usize::from(parse_hex_u16(&self.tile, "tile index")?);
        if tile >= EventTilemapBuffers::WORD_COUNT {
            return Err("tile index must be between 000 and 7FF".into());
        }
        Ok((tile, self.plane))
    }

    fn load_selected(&mut self) -> Result<(), String> {
        let selection = self.selection()?;
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "event-tilemap workspace is closed".to_owned())?;
        self.value = format!("{:02X}", byte(&workspace.current, selection));
        self.loaded = Some(selection);
        Ok(())
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        let selection = self.selection()?;
        if self.loaded != Some(selection) {
            return Err("load the selected tilemap byte before applying it".into());
        }
        let value = parse_hex_u8(&self.value, "tilemap byte")?;
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "event-tilemap workspace is closed".to_owned())?;
        set_byte(&mut workspace.current, selection, value);
        EventTilemapBuffers::decode_streams(
            workspace.current.primary_bytes(),
            workspace.current.secondary_high_bytes(),
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn prepare_commit(&self, revision: u64) -> Result<Option<Command>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "event-tilemap workspace is closed".to_owned())?;
        if workspace.revision != revision {
            return Err("stale event-tilemap workspace cannot be committed".into());
        }
        if workspace.current == workspace.original {
            return Ok(None);
        }
        Ok(Some(Command::ReplaceNativeOverworldEventTilemaps {
            rev: workspace.revision,
            buffers: Box::new(workspace.current.clone()),
        }))
    }

    fn close_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(text(catalog, ExtendedUiTextKey::EventTilemapDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(text(catalog, ExtendedUiTextKey::EventTilemapUnsavedNotice));
                ui.horizontal(|ui| {
                    if ui
                        .button(crate::frontend_ui::localized_text(
                            catalog,
                            UiTextKey::CommonCancel,
                        ))
                        .clicked()
                    {
                        self.pending_close = None;
                    }
                    if ui
                        .button(crate::frontend_ui::localized_text(
                            catalog,
                            UiTextKey::UnsavedDiscard,
                        ))
                        .clicked()
                    {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::EventTilemapErrorTitle)).show(
                context,
                |ui| {
                    ui.label(error);
                    if ui
                        .button(crate::frontend_ui::localized_text(
                            catalog,
                            UiTextKey::CommonOk,
                        ))
                        .clicked()
                    {
                        self.error = None;
                    }
                },
            );
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

impl Plane {
    const ALL: [Self; 3] = [Self::PrimaryLow, Self::PrimaryHigh, Self::SecondaryHigh];

    fn label(self, catalog: Option<&LocalizationCatalog>) -> String {
        match self {
            Self::PrimaryLow => text(catalog, ExtendedUiTextKey::EventTilemapPrimaryLow),
            Self::PrimaryHigh => text(catalog, ExtendedUiTextKey::EventTilemapPrimaryHigh),
            Self::SecondaryHigh => text(catalog, ExtendedUiTextKey::EventTilemapSecondaryHigh),
        }
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

fn byte(buffers: &EventTilemapBuffers, (tile, plane): (usize, Plane)) -> u8 {
    match plane {
        Plane::PrimaryLow => buffers.primary_bytes()[tile * 2],
        Plane::PrimaryHigh => buffers.primary_bytes()[tile * 2 + 1],
        Plane::SecondaryHigh => buffers.secondary_high_bytes()[tile],
    }
}

fn set_byte(buffers: &mut EventTilemapBuffers, (tile, plane): (usize, Plane), value: u8) {
    match plane {
        Plane::PrimaryLow => buffers.primary_bytes_mut()[tile * 2] = value,
        Plane::PrimaryHigh => buffers.primary_bytes_mut()[tile * 2 + 1] = value,
        Plane::SecondaryHigh => buffers.secondary_high_bytes_mut()[tile] = value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn event_tilemap_editor_surface_has_no_literal_widget_text() {
        let source = include_str!("tilemaps.rs");
        for literal_widget in [
            "egui::Window::new(\"",
            "ui.button(\"",
            "egui::Button::new(\"",
            "ui.label(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "event-tilemap editor bypasses typed localization with {literal_widget}"
            );
        }
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("EventTilemap"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
    }

    #[test]
    fn pristine_planes_install_and_semantically_reopen_exact_bytes() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original).unwrap();
        let mut editor = RomOverworldEventTilemapEditor::default();
        editor.open(&app);
        editor.tile = "7FF".into();
        editor.plane = Plane::PrimaryHigh;
        editor.load_selected().unwrap();
        editor.value = "A5".into();
        editor.apply_selected().unwrap();
        editor.plane = Plane::SecondaryHigh;
        editor.load_selected().unwrap();
        editor.value = "5A".into();
        editor.apply_selected().unwrap();
        app.dispatch(
            editor
                .prepare_commit(app.project_revision())
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let reopened = load_smw_us_v1_event_tilemaps(app.project().unwrap()).unwrap();
        assert_eq!(reopened.buffers.primary_bytes()[0xfff], 0xa5);
        assert_eq!(reopened.buffers.secondary_high_bytes()[0x7ff], 0x5a);
    }

    #[test]
    fn bounds_selection_stale_and_dirty_close_are_safe() {
        let buffers = EventTilemapBuffers::default();
        let mut editor = RomOverworldEventTilemapEditor {
            workspace: Some(Workspace {
                revision: 4,
                original: buffers.clone(),
                current: buffers,
                storage: SmwUsV1EventTilemapStorage::Pristine,
            }),
            tile: "800".into(),
            ..Default::default()
        };
        assert!(editor.load_selected().is_err());
        editor.tile = "000".into();
        assert!(editor.apply_selected().is_err());
        editor.load_selected().unwrap();
        editor.value = "12".into();
        editor.apply_selected().unwrap();
        assert!(editor.prepare_commit(5).is_err());
        assert!(!editor.request_close(true));
        assert!(editor.is_open());
    }

    #[test]
    fn staged_pristine_event_tilemaps_recover_all_three_complete_planes() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut editor = RomOverworldEventTilemapEditor::default();
        editor.open(&app);
        for (tile, plane, value) in [
            ("000", Plane::PrimaryLow, "12"),
            ("7FF", Plane::PrimaryHigh, "A5"),
            ("7FF", Plane::SecondaryHigh, "5A"),
        ] {
            editor.tile = tile.into();
            editor.plane = plane;
            editor.loaded = None;
            editor.load_selected().unwrap();
            editor.value = value.into();
            editor.apply_selected().unwrap();
        }

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let buffers = load_smw_us_v1_event_tilemaps(reopened.project().unwrap())
            .unwrap()
            .buffers;
        assert_eq!(buffers.primary_bytes()[0], 0x12);
        assert_eq!(buffers.primary_bytes()[0xfff], 0xa5);
        assert_eq!(buffers.secondary_high_bytes()[0x7ff], 0x5a);
    }

    #[test]
    fn staged_installed_event_tilemap_update_preserves_prior_planes() {
        let mut installer = AppState::default();
        installer
            .load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut first = RomOverworldEventTilemapEditor::default();
        first.open(&installer);
        first.tile = "000".into();
        first.plane = Plane::PrimaryLow;
        first.load_selected().unwrap();
        first.value = "34".into();
        first.apply_selected().unwrap();
        installer
            .dispatch(first.prepare_commit(0).unwrap().unwrap())
            .unwrap();

        let mut app = AppState::default();
        app.load_rom(installer.project().unwrap().save_snapshot())
            .unwrap();
        let mut editor = RomOverworldEventTilemapEditor::default();
        editor.open(&app);
        editor.tile = "7FF".into();
        editor.plane = Plane::SecondaryHigh;
        editor.loaded = None;
        editor.load_selected().unwrap();
        editor.value = "C7".into();
        editor.apply_selected().unwrap();

        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let buffers = load_smw_us_v1_event_tilemaps(reopened.project().unwrap())
            .unwrap()
            .buffers;
        assert_eq!(buffers.primary_bytes()[0], 0x34);
        assert_eq!(buffers.secondary_high_bytes()[0x7ff], 0xc7);
    }
}
