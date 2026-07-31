use crate::{document_loader::DocumentLoader, native_level_assets_panels::AggregatePanels};
use eframe::egui;
use lm_app::{
    AppState, Command, NativeLevelAssetsController, ProfiledControllerSnapshot, RevisionProfile,
};
use lm_graphics::PaletteOwnership;
use lm_level::{Map16Set, NativeLayer2Data};
use lm_project::NativeLevelAssetsFile;
use lm_render::{NativeLevelRasterRequest, NativeMap16Placement, Rgba};

mod commit;
mod lifecycle;
mod mwl;
mod mwl_batch;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    controller: NativeLevelAssetsController,
    snapshot: lm_app::ControllerSnapshot,
    profile: RevisionProfile,
    source_slot: u16,
    image: lm_rom::RomImage,
    internal_header: usize,
    ownership: PaletteOwnership,
}

struct PendingLoad {
    profiled: ProfiledControllerSnapshot,
}

#[derive(Default)]
pub(crate) struct RomLevelAssetsEditor {
    workspace: Option<Workspace>,
    panels: AggregatePanels,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    loader: DocumentLoader,
    mwl_loader: DocumentLoader,
    legacy_mwl_loader: DocumentLoader,
    pending_legacy_mwl_load: Option<mwl::PendingLegacyMwlLoad>,
    mwl_batch_worker: mwl_batch::MwlBatchExportWorker,
    mwl_batch_status: Option<String>,
    pending_load: Option<PendingLoad>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
    bypass_validation: Option<String>,
    bypass_layer2_texture: Option<egui::TextureHandle>,
}

impl RomLevelAssetsEditor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> (bool, Option<Command>) {
        if let Some(result) = self.mwl_batch_worker.show(context) {
            match result {
                Ok(count) => {
                    self.mwl_batch_status =
                        Some(format!("{count} levels were exported successfully."));
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(result) = self.loader.show(context) {
            self.finish_ownership_load(result, project_revision);
        }
        let mut command = self.mwl_loader.show(context).and_then(|result| {
            match self.finish_mwl_import(result, project_revision) {
                Ok(command) => Some(command),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            }
        });
        if let Some(result) = self.legacy_mwl_loader.show(context) {
            match self.finish_legacy_mwl_load(result, project_revision) {
                Ok(Some(legacy_command)) => command = Some(legacy_command),
                Ok(None) => {}
                Err(error) => self.error = Some(error),
            }
        }
        let reclamation_command = match self.manifest_loader.show(context, project_revision) {
            Some(Ok(manifest)) => match self.prepare_commit_with_reclamation(&manifest) {
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
        if reclamation_command.is_some() {
            command = reclamation_command;
        }
        if self.workspace.is_some() {
            egui::Window::new("ROM Native Level Assets")
                .default_size([900.0, 720.0])
                .vscroll(true)
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
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.controller.revision() != project_revision;
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed. Close and reopen this workspace before committing.",
            );
        }
        ui.horizontal(|ui| {
            ui.label("Allocation search (logical PC hex, end-exclusive)");
            ui.text_edit_singleline(&mut self.search_start);
            ui.label("..");
            ui.text_edit_singleline(&mut self.search_end);
        });
        let file = NativeLevelAssetsFile {
            source_slot: workspace.source_slot,
            assets: workspace.controller.assets().clone(),
        };
        let edit = self.panels.show(
            ui,
            workspace.controller.revision(),
            &file,
            (
                workspace.controller.layer2(),
                workspace.controller.layer2_descriptor(),
            ),
            &workspace.profile.exanimation_double_size_modes,
            &workspace.ownership,
        );
        if let Some(edit) = edit {
            match edit {
                Ok(edit) if !stale => {
                    if let Some(workspace) = self.workspace.as_mut() {
                        if let Err(error) = workspace.controller.apply_edits(&[edit]) {
                            self.error = Some(error.to_string());
                        } else {
                            self.bypass_validation = None;
                            self.bypass_layer2_texture = None;
                            self.panels.invalidate();
                        }
                    } else {
                        self.error = Some("level-assets workspace is closed".into());
                    }
                }
                Ok(_) => self.error = Some("stale ROM workspace cannot accept more edits".into()),
                Err(error) => self.error = Some(error),
            }
        }
        if ui.button("Validate selected Super GFX files").clicked() {
            self.bypass_validation = self.workspace.as_ref().map(validate_super_graphics);
        }
        if ui.button("Render bypass-aware Layer 2 preview").clicked() {
            match self
                .workspace
                .as_ref()
                .ok_or_else(|| "level-assets workspace is closed".to_owned())
                .and_then(render_super_graphics_layer2_preview)
            {
                Ok(image) => {
                    self.bypass_layer2_texture = Some(ui.ctx().load_texture(
                        "installed-super-gfx-layer2-preview",
                        image,
                        egui::TextureOptions::NEAREST,
                    ));
                    self.bypass_validation =
                        Some("Rendered the installed 32×32 Layer 2 tilemap with the selected Super GFX files, installed Map16 definitions, and staged level palette.".into());
                }
                Err(error) => {
                    self.bypass_layer2_texture = None;
                    self.bypass_validation = Some(error);
                }
            }
        }
        if let Some(validation) = &self.bypass_validation {
            ui.label(validation);
        }
        if let Some(texture) = &self.bypass_layer2_texture {
            egui::ScrollArea::horizontal()
                .id_salt("installed-super-gfx-layer2-preview")
                .show(ui, |ui| {
                    ui.image(texture);
                });
        }
        ui.separator();
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|w| w.controller.is_modified());
        self.show_mwl_actions(ui, stale, modified);
        if let Some(status) = &self.mwl_batch_status {
            ui.label(status);
        }
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new("Commit all domains to ROM"),
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
                egui::Button::new("Commit and reclaim with LMRATS01 evidence"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(project_revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged aggregate changes"
        } else {
            "No staged changes"
        });
        None
    }
}

fn validate_super_graphics(workspace: &Workspace) -> String {
    let Some(settings) = workspace.controller.assets().expanded_settings.as_ref() else {
        return "No installed expanded-settings record is available.".to_owned();
    };
    let project = lm_project::Project::new(workspace.image.clone());
    match project.load_super_graphics_bypass(settings, workspace.profile.graphics) {
        Ok(None) => {
            "Super GFX bypass is disabled; legacy tileset assignments remain active.".into()
        }
        Ok(Some(loaded)) => {
            let vram = lm_render::materialize_super_graphics_vram(&loaded);
            let foreground_tiles = vram.foreground_background.len();
            let sprite_tiles = vram.sprites.len();
            format!(
                "Validated and materialized 6 FG/BG files ({foreground_tiles} VRAM tiles) and 4 sprite files ({sprite_tiles} VRAM tiles)."
            )
        }
        Err(error) => error.to_string(),
    }
}

fn render_super_graphics_layer2_preview(workspace: &Workspace) -> Result<egui::ColorImage, String> {
    let settings = workspace
        .controller
        .assets()
        .expanded_settings
        .as_ref()
        .ok_or_else(|| "No installed expanded-settings record is available.".to_owned())?;
    let NativeLayer2Data::Tilemap(tilemap) = workspace
        .controller
        .layer2()
        .ok_or_else(|| "This level has no decoded Layer 2 data.".to_owned())?
    else {
        return Err(
            "This level uses an object-stream Layer 2; its bypass-aware preview is not yet routed."
                .into(),
        );
    };
    let project = lm_project::Project::new(workspace.image.clone());
    let loaded = project
        .load_super_graphics_bypass(settings, workspace.profile.graphics)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            "Super GFX bypass is disabled; legacy tileset assignments remain active.".to_owned()
        })?;
    let vram = lm_render::materialize_super_graphics_vram(&loaded);
    let map16 = project
        .load_map16_set(workspace.profile.map16)
        .map_err(|error| error.to_string())?;
    render_layer2_image(
        tilemap,
        &map16,
        &vram.foreground_background,
        &workspace.controller.assets().palette,
    )
}

fn render_layer2_image(
    tilemap: &[u8],
    map16: &Map16Set,
    tiles: &[lm_graphics::IndexedTile],
    palette: &lm_graphics::Palette,
) -> Result<egui::ColorImage, String> {
    let placements = layer2_placements(tilemap)?;
    let definitions = map16
        .pages
        .iter()
        .flat_map(|page| page.tiles.iter().copied())
        .collect::<Vec<_>>();
    let layers: [&[NativeMap16Placement]; 1] = [&placements];
    let backdrop = palette
        .colors
        .first()
        .copied()
        .map(lm_graphics::Bgr555::to_rgb8)
        .map_or_else(Rgba::default, |color| Rgba {
            red: color.red,
            green: color.green,
            blue: color.blue,
            alpha: 255,
        });
    let canvas = lm_render::render_native_level_framebuffer(NativeLevelRasterRequest {
        width: lm_level::NATIVE_LAYER2_TILEMAP_WIDTH * 16,
        height: lm_level::NATIVE_LAYER2_TILEMAP_HEIGHT * 16,
        camera_x: 0,
        camera_y: 0,
        backdrop,
        layers: &layers,
        definitions: &definitions,
        tiles,
        palette,
    })
    .map_err(|error| error.to_string())?;
    let mut rgba = Vec::with_capacity(canvas.pixels().len() * 4);
    for pixel in canvas.pixels() {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [canvas.width(), canvas.height()],
        &rgba,
    ))
}

fn layer2_placements(tilemap: &[u8]) -> Result<Vec<NativeMap16Placement>, String> {
    if tilemap.len() != lm_level::NATIVE_LAYER2_TILEMAP_LEN {
        return Err(format!(
            "native Layer 2 tilemap has {} bytes instead of {}",
            tilemap.len(),
            lm_level::NATIVE_LAYER2_TILEMAP_LEN
        ));
    }
    let mut placements = Vec::with_capacity(
        lm_level::NATIVE_LAYER2_TILEMAP_WIDTH * lm_level::NATIVE_LAYER2_TILEMAP_HEIGHT,
    );
    for y in 0..lm_level::NATIVE_LAYER2_TILEMAP_HEIGHT {
        for x in 0..lm_level::NATIVE_LAYER2_TILEMAP_WIDTH {
            let index = lm_level::native_layer2_tilemap_index(x, y)
                .ok_or_else(|| "bounded Layer 2 coordinate did not map to storage".to_owned())?;
            let offset = index * 2;
            placements.push(NativeMap16Placement {
                x: i32::try_from(x).map_err(|_| "Layer 2 X coordinate overflow".to_owned())?,
                y: i32::try_from(y).map_err(|_| "Layer 2 Y coordinate overflow".to_owned())?,
                word: u16::from_le_bytes([tilemap[offset], tilemap[offset + 1]]),
            });
        }
    }
    Ok(placements)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, IndexedTile, Palette};
    use lm_level::{Map16Page, Map16Tile, Subtile};

    #[test]
    fn layer2_preview_uses_visual_coordinates_and_native_storage_planes() {
        let mut bytes = vec![0; lm_level::NATIVE_LAYER2_TILEMAP_LEN];
        for (x, y, word) in [(0, 0, 0x0123_u16), (31, 0, 0x4567_u16), (0, 31, 0x89ab_u16)] {
            let index = lm_level::native_layer2_tilemap_index(x, y).unwrap();
            bytes[index * 2..index * 2 + 2].copy_from_slice(&word.to_le_bytes());
        }
        let placements = layer2_placements(&bytes).unwrap();
        assert_eq!(
            placements[0],
            NativeMap16Placement {
                x: 0,
                y: 0,
                word: 0x0123
            }
        );
        assert_eq!(
            placements[31],
            NativeMap16Placement {
                x: 31,
                y: 0,
                word: 0x4567
            }
        );
        assert_eq!(
            placements[31 * 32],
            NativeMap16Placement {
                x: 0,
                y: 31,
                word: 0x89ab
            }
        );
    }

    #[test]
    fn layer2_preview_rejects_noncanonical_tilemap_lengths() {
        assert!(layer2_placements(&[0; 0x7ff]).is_err());
    }

    #[test]
    fn layer2_preview_rasterizes_installed_map16_vram_and_palette() {
        let mut tilemap = vec![0; lm_level::NATIVE_LAYER2_TILEMAP_LEN];
        let index = lm_level::native_layer2_tilemap_index(1, 2).unwrap();
        tilemap[index * 2..index * 2 + 2].copy_from_slice(&1_u16.to_le_bytes());
        let mut definitions = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        definitions[1] = Map16Tile {
            top_left: Subtile(0),
            top_right: Subtile(0),
            bottom_left: Subtile(0),
            bottom_right: Subtile(0),
            acts_like: 1,
        };
        let map16 = Map16Set {
            pages: vec![Map16Page::new(definitions).unwrap()],
        };
        let tiles = [IndexedTile::new([1; IndexedTile::PIXEL_COUNT])];
        let mut colors = vec![Bgr555(0); 128];
        colors[1] = Bgr555(0x001f);
        let image = render_layer2_image(&tilemap, &map16, &tiles, &Palette { colors }).unwrap();
        assert_eq!(image.size, [512, 512]);
        assert_eq!(image[(16, 32)], egui::Color32::from_rgb(255, 0, 0));
    }
}
