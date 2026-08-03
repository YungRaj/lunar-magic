use super::{
    AppState, MainLayer2Workspace, PendingClose, PendingLoad, PendingOpen, RomOverworldEditor,
    Workspace, egui,
};
use crate::{
    document_loader::{BoundedRead, LoadedDocument},
    level_editor_forms,
};
use lm_app::{PaletteOwnershipFile, RevisionProfileControllers};
use lm_graphics::{GraphicsFile4bpp, GraphicsInterchangeFile, IndexedTile};
use lm_level::Map16SetFile;
use lm_project::Project;
use lm_rom::RomImage;

const OVERWORLD_GRAPHICS_FILES: [usize; 4] = [0x1c, 0x1d, 0x1e, 0x1f];
const TILES_PER_NATIVE_GRAPHICS_SLOT: usize = 0x80;

impl RomOverworldEditor {
    pub(crate) fn handles(app: &AppState) -> bool {
        let Some(identity) = app.project().and_then(|project| project.identity.as_ref()) else {
            return false;
        };
        matches!(app.mode, lm_app::EditorMode::Overworld)
            && identity.game == lm_rom::SupportedGame::SuperMarioWorld
            && identity.region == lm_rom::Region::NorthAmerica
            && identity.revision == 0
            && identity.mapper == lm_rom::Mapper::LoRom
    }

    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
            || self.main_layer2_workspace.is_some()
            || self.pending_open.is_some()
            || self.loader.is_running()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        match app.profiled_controller_snapshot() {
            Ok(profiled) if profiled.snapshot.mode == lm_app::EditorMode::Overworld => {
                self.pending_open = Some(PendingOpen {
                    profiled,
                    slot: "0".into(),
                });
            }
            Ok(_) => self.error = Some("switch to overworld mode before opening the editor".into()),
            Err(profile_error) => match app
                .controller_snapshot()
                .map_err(|error| error.to_string())
                .and_then(decode_main_layer2_workspace)
            {
                Ok(workspace) => {
                    self.main_layer2_workspace = Some(workspace);
                    self.search_start.clear();
                    self.search_end.clear();
                    self.layer = 1;
                    self.x = 0;
                    self.y = 0;
                    self.paint_anchor = None;
                    self.map16_page = 0;
                    self.invalidate();
                }
                Err(native_error) => {
                    self.error = Some(format!(
                        "{profile_error}; built-in playable Layer 2 open also failed: {native_error}"
                    ));
                }
            },
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.manifest_loader.is_running()
            || self.loader.is_running()
            || self.transfer_loader.is_running()
            || self.transfer_persistence.is_running()
        {
            self.error = Some("wait for overworld file work to finish before closing".into());
            return false;
        }
        if self.pending_open.is_some() {
            self.pending_open = None;
            return true;
        }
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.controller.is_modified())
            || self
                .main_layer2_workspace
                .as_ref()
                .is_some_and(|workspace| workspace.controller.is_modified());
        if !modified {
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

    pub(super) fn open_dialog(&mut self, context: &egui::Context) {
        if self.pending_open.is_none() {
            return;
        }
        egui::Window::new("Open native overworld")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Profile overworld slot (hex)");
                if let Some(pending) = self.pending_open.as_mut() {
                    ui.text_edit_singleline(&mut pending.slot);
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_open = None;
                    }
                    if ui.button("Open").clicked() {
                        self.finish_open();
                    }
                });
            });
    }

    fn finish_open(&mut self) {
        let Some(pending) = self.pending_open.take() else {
            return;
        };
        let slot = match parse_slot(&pending.slot) {
            Ok(slot) => slot,
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending);
                return;
            }
        };
        let Some(path) = crate::dialogs::choose_palette_ownership() else {
            self.pending_open = Some(pending);
            return;
        };
        let request = BoundedRead::new(
            path,
            u64::try_from(PaletteOwnershipFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "palette ownership evidence",
        );
        match self.loader.start(vec![request]) {
            Ok(()) => {
                self.pending_load = Some(PendingLoad {
                    open: pending,
                    slot,
                });
            }
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending);
            }
        }
    }

    pub(super) fn finish_ownership_load(
        &mut self,
        result: Result<LoadedDocument, String>,
        current_revision: u64,
    ) {
        let Some(pending) = self.pending_load.take() else {
            self.error = Some("overworld ownership load lost its open request".into());
            return;
        };
        match result.and_then(|loaded| decode_loaded(&pending, loaded, current_revision)) {
            Ok(workspace) => {
                self.workspace = Some(workspace);
                self.search_start.clear();
                self.search_end.clear();
                self.x = 0;
                self.y = 0;
                self.paint_anchor = None;
                self.map16_page = 0;
                self.invalidate();
            }
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending.open);
            }
        }
    }

    pub(super) fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard staged overworld changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(if self.main_layer2_workspace.is_some() {
                    "Playable Layer 2 map changes have not been committed."
                } else {
                    "Changes across the nine payloads have not been committed."
                });
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

    pub(super) fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("ROM overworld error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.main_layer2_workspace = None;
        self.pending_open = None;
        self.pending_load = None;
        self.pending_close = None;
        self.paint_anchor = None;
        self.texture = None;
        self.map16_texture = None;
        self.invalidate();
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

fn decode_main_layer2_workspace(
    snapshot: lm_app::ControllerSnapshot,
) -> Result<MainLayer2Workspace, String> {
    if snapshot.mode != lm_app::EditorMode::Overworld {
        return Err("switch to overworld mode before opening the editor".into());
    }
    let controller = lm_app::SmwMainOverworldLayer2Controller::decode(&snapshot)
        .map_err(|error| error.to_string())?;
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let project = Project::new(image.clone());
    let mut map16_snapshot = snapshot.clone();
    map16_snapshot.mode = lm_app::EditorMode::Map16;
    let map16 =
        lm_app::SmwMap16Controller::decode(&map16_snapshot).map_err(|error| error.to_string())?;
    let mut tiles =
        Vec::with_capacity(OVERWORLD_GRAPHICS_FILES.len() * TILES_PER_NATIVE_GRAPHICS_SLOT);
    for file_number in OVERWORLD_GRAPHICS_FILES {
        let slot = project
            .load_graphics_file(file_number, lm_profile::smw_us_v1_vanilla_graphics_layout())
            .map_err(|error| format!("could not load overworld GFX{file_number:02X}: {error}"))?
            .tiles;
        append_overworld_graphics_slot(&mut tiles, file_number, slot)?;
    }
    let mut palette = project
        .load_shared_palette(lm_profile::smw_us_v1_shared_palette_layout())
        .map_err(|error| error.to_string())?
        .palette()
        .map_err(|error| error.to_string())?;
    // The legacy `.smwpal` backend retains one non-row tail color. Rendering consumes complete
    // 16-color SNES CGRAM rows, so keep every complete row and leave that auxiliary tail intact in
    // the ROM rather than inventing a palette entry.
    let complete_colors = palette.colors.len() / 16 * 16;
    palette.colors.truncate(complete_colors);
    Ok(MainLayer2Workspace {
        controller,
        palette,
        assets: crate::overworld_editor_render::OverworldAssets {
            map16: Map16SetFile {
                set: map16.set().clone(),
            },
            graphics: GraphicsInterchangeFile {
                source_slot: u16::try_from(OVERWORLD_GRAPHICS_FILES[0]).unwrap_or_default(),
                graphics: GraphicsFile4bpp { tiles },
            },
        },
    })
}

fn parse_slot(value: &str) -> Result<u16, String> {
    level_editor_forms::parse_hex_u16(value, "overworld slot")
}

fn decode_loaded(
    pending: &PendingLoad,
    loaded: LoadedDocument,
    current_revision: u64,
) -> Result<Workspace, String> {
    crate::rom_load::ensure_current_revision(
        pending.open.profiled.snapshot.revision,
        current_revision,
        "overworld ownership evidence",
    )?;
    let [(_, bytes)] = loaded.into_exact::<1>("overworld ownership")?;
    let ownership = PaletteOwnershipFile::decode(&bytes)
        .map_err(|error| error.to_string())?
        .ownership;
    let profiled = pending.open.profiled.clone();
    let controller = profiled
        .profile
        .decode_overworld(
            &profiled.snapshot,
            usize::from(pending.slot),
            ownership.clone(),
        )
        .map_err(|error| error.to_string())?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    let assets = decode_overworld_assets(&profiled)?;
    Ok(Workspace {
        controller,
        profiled,
        slot: pending.slot,
        image,
        ownership,
        assets,
    })
}

fn decode_overworld_assets(
    profiled: &lm_app::ProfiledControllerSnapshot,
) -> Result<crate::overworld_editor_render::OverworldAssets, String> {
    let map16 = profiled
        .profile
        .decode_map16(&profiled.snapshot)
        .map_err(|error| error.to_string())?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    let project = Project::new(image);
    let mut tiles =
        Vec::with_capacity(OVERWORLD_GRAPHICS_FILES.len() * TILES_PER_NATIVE_GRAPHICS_SLOT);
    for file_number in OVERWORLD_GRAPHICS_FILES {
        let slot = project
            .load_graphics_file(file_number, profiled.profile.graphics)
            .map_err(|error| format!("could not load overworld GFX{file_number:02X}: {error}"))?
            .tiles;
        append_overworld_graphics_slot(&mut tiles, file_number, slot)?;
    }
    Ok(crate::overworld_editor_render::OverworldAssets {
        map16: Map16SetFile {
            set: map16.set().clone(),
        },
        graphics: GraphicsInterchangeFile {
            source_slot: u16::try_from(OVERWORLD_GRAPHICS_FILES[0]).unwrap_or_default(),
            graphics: GraphicsFile4bpp { tiles },
        },
    })
}

fn append_overworld_graphics_slot(
    destination: &mut Vec<IndexedTile>,
    file_number: usize,
    mut slot: Vec<IndexedTile>,
) -> Result<(), String> {
    if slot.len() > TILES_PER_NATIVE_GRAPHICS_SLOT {
        return Err(format!(
            "overworld GFX{file_number:02X} has {} tiles; expected at most {TILES_PER_NATIVE_GRAPHICS_SLOT}",
            slot.len()
        ));
    }
    slot.resize_with(TILES_PER_NATIVE_GRAPHICS_SLOT, || {
        IndexedTile::new([0; IndexedTile::PIXEL_COUNT])
    });
    destination.extend(slot);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        TILES_PER_NATIVE_GRAPHICS_SLOT, append_overworld_graphics_slot,
        decode_main_layer2_workspace, parse_slot,
    };
    use lm_graphics::IndexedTile;
    use std::{fs, path::Path};

    #[test]
    fn configured_overworld_slot_is_bound_as_hexadecimal() {
        assert_eq!(parse_slot("0").unwrap(), 0);
        assert_eq!(parse_slot("01ff").unwrap(), 0x01ff);
        assert!(parse_slot("").is_err());
        assert!(parse_slot("10000").is_err());
        assert!(parse_slot("not-a-slot").is_err());
    }

    #[test]
    fn native_overworld_graphics_slots_keep_vram_boundaries() {
        let marked = IndexedTile::new([7; IndexedTile::PIXEL_COUNT]);
        let mut tiles = Vec::new();
        append_overworld_graphics_slot(&mut tiles, 0x1c, vec![marked.clone()]).unwrap();
        append_overworld_graphics_slot(&mut tiles, 0x1d, vec![marked.clone(); 2]).unwrap();
        assert_eq!(tiles.len(), TILES_PER_NATIVE_GRAPHICS_SLOT * 2);
        assert_eq!(tiles[0], marked);
        assert_eq!(
            tiles[TILES_PER_NATIVE_GRAPHICS_SLOT],
            IndexedTile::new([7; IndexedTile::PIXEL_COUNT])
        );
        assert!(tiles[1].pixels().iter().all(|pixel| *pixel == 0));
        assert!(
            append_overworld_graphics_slot(
                &mut tiles,
                0x1e,
                vec![
                    IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
                    TILES_PER_NATIVE_GRAPHICS_SLOT + 1
                ],
            )
            .is_err()
        );
    }

    #[test]
    fn authentic_lunar_magic_rom_opens_and_renders_profile_free_playable_layer2_workspace() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/overworld-transfer-positive/after.smc");
        let mut app = lm_app::AppState::default();
        app.load_rom(fs::read(fixture).unwrap()).unwrap();
        app.dispatch(lm_app::Command::ShowOverworld).unwrap();
        let mut workspace =
            decode_main_layer2_workspace(app.controller_snapshot().unwrap()).unwrap();
        assert_eq!(
            (
                workspace.controller.layer().width,
                workspace.controller.layer().height,
            ),
            (128, 64)
        );
        assert_eq!(workspace.assets.map16.set.pages.len(), 0x100);
        assert_eq!(workspace.assets.graphics.graphics.tiles.len(), 0x200);
        let canvas = lm_render::render_portable_overworld_layer(
            2,
            workspace.controller.layer(),
            &workspace.assets.map16,
            &workspace.assets.graphics,
            &workspace.palette,
        )
        .unwrap();
        assert_eq!((canvas.width(), canvas.height()), (2048, 1024));
        assert!(
            canvas
                .pixels()
                .iter()
                .any(|pixel| { pixel.red != 0 || pixel.green != 0 || pixel.blue != 0 })
        );

        let original = workspace.controller.layer().tile(12, 9).unwrap();
        let replacement = original ^ 1;
        let cells = [(12, 9), (13, 9), (12, 10), (13, 10)];
        let edits = cells
            .into_iter()
            .map(|(x, y)| lm_app::OverworldControllerEdit::SetLayerTile {
                layer: lm_app::OverworldLayerId::Layer2,
                x,
                y,
                tile: replacement,
            })
            .collect::<Vec<_>>();
        workspace.controller.apply_edits(&edits).unwrap();
        let command = workspace
            .controller
            .prepare_commit(
                "visual Layer 2 paint regression",
                lm_rats::AllocationPolicy {
                    search: 0x0e_0000..0x0f_0000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff, 0],
                    protected: vec![
                        lm_rats::ProtectedRange(
                            lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD
                                ..lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD + 2,
                        ),
                        lm_rats::ProtectedRange(
                            lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK
                                ..lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK + 1,
                        ),
                        lm_rats::ProtectedRange(
                            lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD
                                ..lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD + 2,
                        ),
                        lm_rats::ProtectedRange(0x7fdc..0x7fe0),
                    ],
                },
            )
            .unwrap()
            .into_command();
        app.dispatch(command).unwrap();
        let reopened =
            lm_profile::load_smw_us_v1_main_overworld_layer2(app.project().unwrap()).unwrap();
        for (x, y) in cells {
            assert_eq!(reopened.layer.tile(x, y).unwrap(), replacement);
        }
        app.dispatch(lm_app::Command::Undo).unwrap();
        let restored =
            lm_profile::load_smw_us_v1_main_overworld_layer2(app.project().unwrap()).unwrap();
        assert_eq!(restored.layer.tile(12, 9).unwrap(), original);
    }
}
