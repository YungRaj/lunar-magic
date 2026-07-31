use crate::{document_loader::DocumentLoader, native_level_assets_panels::AggregatePanels};
use eframe::egui;
use lm_app::{
    AppState, Command, NativeLevelAssetsController, ProfiledControllerSnapshot, RevisionProfile,
};
use lm_graphics::PaletteOwnership;
use lm_level::{Map16Set, NativeLayer2Data, ObjectStream};
use lm_project::NativeLevelAssetsFile;
use lm_render::{
    NativeLevelMap16Layout, NativeLevelRasterRequest, NativeMap16Placement, Rgba,
    StandardLevelOrientation, StandardObjectDefinitionSet, StandardSpritePreviewMode,
    StandardSpritePreviewSource, draw_native_sprite_preview_definition_pages,
    install_lunar_magic_shared_extended_objects, install_lunar_magic_shared_standard_objects,
    install_lunar_magic_tileset_extended_objects, lunar_magic_standard_sprite_preview_source,
    render_lunar_magic_standard_sprite_with_mode, render_mapped_standard_object_stream,
};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeSpritePreviewPlacement {
    subtiles: [u16; 4],
    x: i32,
    y: i32,
}

#[derive(Default)]
struct LivePreviewState {
    enabled: bool,
    dirty: bool,
}

impl LivePreviewState {
    fn toggle(&mut self) {
        self.enabled = !self.enabled;
        self.dirty = self.enabled;
    }

    fn invalidate(&mut self) {
        if self.enabled {
            self.dirty = true;
        }
    }

    fn take_refresh(&mut self) -> bool {
        let refresh = self.enabled && self.dirty;
        self.dirty = false;
        refresh
    }
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
    bypass_preview: LivePreviewState,
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
            workspace.controller.exanimation_features(),
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
                            self.bypass_preview.invalidate();
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
        let preview_button = if self.bypass_preview.enabled {
            "Stop live bypass-aware preview"
        } else {
            "Start live bypass-aware preview"
        };
        if ui.button(preview_button).clicked() {
            self.bypass_preview.toggle();
            if self.bypass_preview.enabled {
                self.bypass_layer2_texture = None;
            }
        }
        if self.bypass_preview.take_refresh() {
            match self
                .workspace
                .as_ref()
                .ok_or_else(|| "level-assets workspace is closed".to_owned())
                .and_then(render_super_graphics_level_preview)
            {
                Ok((image, diagnostics)) => {
                    self.bypass_layer2_texture = Some(ui.ctx().load_texture(
                        "installed-super-gfx-level-preview",
                        image,
                        egui::TextureOptions::NEAREST,
                    ));
                    self.bypass_validation = Some(if diagnostics.is_empty() {
                        "Rendered installed Layer 2 and Layer 1 object streams with the selected Super GFX files, installed Map16 definitions, and staged level palette.".into()
                    } else {
                        format!(
                            "Rendered the installed object layers with unresolved definitions: {}",
                            diagnostics.join("; ")
                        )
                    });
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

fn render_super_graphics_level_preview(
    workspace: &Workspace,
) -> Result<(egui::ColorImage, Vec<String>), String> {
    let settings = workspace
        .controller
        .assets()
        .expanded_settings
        .as_ref()
        .ok_or_else(|| "No installed expanded-settings record is available.".to_owned())?;
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
    let header = workspace.controller.assets().level.layer1.header;
    let (layer1, layout, mut diagnostics) = render_object_placements(
        &workspace.image,
        &workspace.controller.assets().level.layer1.objects,
        header.level_mode(),
        header.object_tileset(),
    )?;
    let layer2 = match workspace.controller.layer2() {
        Some(NativeLayer2Data::Tilemap(tilemap)) => layer2_placements(tilemap)?,
        Some(NativeLayer2Data::Objects(objects)) => {
            let (placements, _, layer2_diagnostics) = render_object_placements(
                &workspace.image,
                &objects.objects,
                header.level_mode(),
                header.object_tileset(),
            )?;
            diagnostics.extend(
                layer2_diagnostics
                    .into_iter()
                    .map(|diagnostic| format!("Layer 2 {diagnostic}")),
            );
            placements
        }
        None => Vec::new(),
    };
    let (sprites, sprite_diagnostics) = render_sprite_placements(
        &workspace.controller.assets().level.sprites,
        header.level_mode(),
        header.sprite_tileset(),
    );
    diagnostics.extend(sprite_diagnostics);
    let animated_sprite_tiles =
        crate::vanilla_map16_preview::load_vanilla_sprite_display_tiles(&project)?;
    render_level_image(
        &[&layer2, &layer1],
        &sprites,
        layout,
        &map16,
        &vram.foreground_background,
        &vram.sprites,
        &animated_sprite_tiles,
        &workspace.controller.assets().palette,
    )
    .map(|image| (image, diagnostics))
}

fn render_level_image(
    layers: &[&[NativeMap16Placement]],
    sprites: &[NativeSpritePreviewPlacement],
    layout: NativeLevelMap16Layout,
    map16: &Map16Set,
    tiles: &[lm_graphics::IndexedTile],
    sprite_tiles: &[lm_graphics::IndexedTile],
    animated_sprite_tiles: &[lm_graphics::IndexedTile],
    palette: &lm_graphics::Palette,
) -> Result<egui::ColorImage, String> {
    let definitions = map16
        .pages
        .iter()
        .flat_map(|page| page.tiles.iter().copied())
        .collect::<Vec<_>>();
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
    let mut canvas = lm_render::render_native_level_framebuffer(NativeLevelRasterRequest {
        width: layout.width * 16,
        height: layout.height * 16,
        camera_x: 0,
        camera_y: 0,
        backdrop,
        layers,
        definitions: &definitions,
        tiles,
        palette,
    })
    .map_err(|error| error.to_string())?;
    for sprite in sprites {
        draw_native_sprite_preview_definition_pages(
            &mut canvas,
            sprite.subtiles,
            sprite_tiles,
            animated_sprite_tiles,
            palette,
            sprite.x,
            sprite.y,
        );
    }
    let mut rgba = Vec::with_capacity(canvas.pixels().len() * 4);
    for pixel in canvas.pixels() {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [canvas.width(), canvas.height()],
        &rgba,
    ))
}

fn render_object_placements(
    image: &lm_rom::RomImage,
    objects: &ObjectStream,
    level_mode: u8,
    object_tileset: u8,
) -> Result<
    (
        Vec<NativeMap16Placement>,
        NativeLevelMap16Layout,
        Vec<String>,
    ),
    String,
> {
    let mode = lm_profile::smw_us_v1_level_mode(level_mode);
    if mode.editor_major_screens == 0 {
        return Err(format!("level mode {level_mode:02X} has no editor canvas"));
    }
    let layout = NativeLevelMap16Layout {
        width: if mode.vertical {
            32
        } else {
            usize::from(mode.editor_major_screens) * 16
        },
        height: if mode.vertical {
            usize::from(mode.editor_major_screens) * 16
        } else {
            27
        },
        page_stride: 0x1b0,
        base_cell: 0,
        vertical: mode.vertical,
    };
    let object_map = lm_profile::load_smw_us_v1_standard_object_definition_map(image)
        .map_err(|error| error.to_string())?;
    let family = match lm_profile::smw_us_v1_object_family(object_tileset) {
        lm_profile::VanillaObjectFamily::Normal => 0,
        lm_profile::VanillaObjectFamily::Castle => 1,
        lm_profile::VanillaObjectFamily::Rope => 2,
        lm_profile::VanillaObjectFamily::Underground => 3,
        lm_profile::VanillaObjectFamily::GhostHouse => 4,
    };
    let handler_map = object_map
        .family(family)
        .ok_or_else(|| format!("object-definition family {family} is unavailable"))?;
    let mut definitions = StandardObjectDefinitionSet::empty();
    install_lunar_magic_shared_extended_objects(&mut definitions)
        .and_then(|()| install_lunar_magic_shared_standard_objects(&mut definitions))
        .and_then(|()| {
            install_lunar_magic_tileset_extended_objects(&mut definitions, object_tileset)
        })
        .map_err(|error| error.to_string())?;
    let rendered =
        render_mapped_standard_object_stream(objects, &definitions, handler_map, layout, 0x25)
            .map_err(|error| error.to_string())?;
    let mut placements = Vec::with_capacity(layout.width * layout.height);
    for y in 0..layout.height {
        for x in 0..layout.width {
            let index = lm_render::NativeLevelMap16Cache::cell_index(layout, x, y);
            placements.push(NativeMap16Placement {
                x: i32::try_from(x).map_err(|_| "object-layer X overflow".to_owned())?,
                y: i32::try_from(y).map_err(|_| "object-layer Y overflow".to_owned())?,
                word: rendered.cache.cells()[index],
            });
        }
    }
    let mut diagnostics = Vec::new();
    if !rendered.missing_commands.is_empty() {
        diagnostics.push(format!("commands {:?}", rendered.missing_commands));
    }
    if !rendered.missing_extended_objects.is_empty() {
        diagnostics.push(format!(
            "extended objects {:?}",
            rendered.missing_extended_objects
        ));
    }
    Ok((placements, layout, diagnostics))
}

fn render_sprite_placements(
    sprites: &lm_level::NativeSpriteStream,
    level_mode: u8,
    sprite_tileset: u8,
) -> (Vec<NativeSpritePreviewPlacement>, Vec<String>) {
    let mode = lm_profile::smw_us_v1_level_mode(level_mode);
    let orientation = if mode.vertical {
        StandardLevelOrientation::Vertical
    } else {
        StandardLevelOrientation::Horizontal
    };
    let mut rendered = Vec::new();
    let mut diagnostics = Vec::new();
    let mut sprite_8a_sequence_index = 0_u8;
    for placement in sprites.native_placements() {
        let preview = render_lunar_magic_standard_sprite_with_mode(
            placement.sprite_number,
            StandardSpritePreviewMode {
                placement_first: placement.packed_display_position(),
                placement_major: placement.major,
                placement_minor: placement.minor,
                level_mode,
                level_orientation: orientation,
                sprite_graphics_mode: sprite_tileset,
                sprite_8a_sequence_index,
                ..StandardSpritePreviewMode::default()
            },
        );
        if placement.sprite_number == 0x8a {
            sprite_8a_sequence_index = sprite_8a_sequence_index.saturating_add(1);
        }
        let Some(parts) = preview else {
            if lunar_magic_standard_sprite_preview_source(placement.sprite_number)
                == StandardSpritePreviewSource::BuiltIn
            {
                diagnostics.push(format!(
                    "sprite ${:02X} has no materialized preview",
                    placement.sprite_number
                ));
            }
            continue;
        };
        let (tile_x, tile_y) = placement.tile_coordinates(mode.vertical);
        let origin_x = i32::from(tile_x).saturating_mul(16);
        let origin_y = i32::from(tile_y).saturating_mul(16);
        for part in parts {
            rendered.push(NativeSpritePreviewPlacement {
                subtiles: part.subtiles,
                x: origin_x.saturating_add(i32::from(part.x)),
                y: origin_y.saturating_add(i32::from(part.y)),
            });
        }
    }
    diagnostics.sort();
    diagnostics.dedup();
    (rendered, diagnostics)
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
    fn live_preview_refreshes_once_per_enable_or_accepted_edit() {
        let mut preview = LivePreviewState::default();
        assert!(!preview.take_refresh());
        preview.invalidate();
        assert!(!preview.take_refresh());

        preview.toggle();
        assert!(preview.take_refresh());
        assert!(!preview.take_refresh());

        preview.invalidate();
        assert!(preview.take_refresh());
        assert!(!preview.take_refresh());

        preview.toggle();
        preview.invalidate();
        assert!(!preview.take_refresh());
    }

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
        let placements = layer2_placements(&tilemap).unwrap();
        let layers: [&[NativeMap16Placement]; 1] = [&placements];
        let image = render_level_image(
            &layers,
            &[],
            NativeLevelMap16Layout {
                width: 32,
                height: 32,
                page_stride: 0x1b0,
                base_cell: 0,
                vertical: false,
            },
            &map16,
            &tiles,
            &[],
            &[],
            &Palette { colors },
        )
        .unwrap();
        assert_eq!(image.size, [512, 512]);
        assert_eq!(image[(16, 32)], egui::Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn object_stream_preview_uses_recovered_vertical_mode_dimensions() {
        let image = lm_rom::RomImage::from_bytes(vec![0; 0x80000]).unwrap();
        let (placements, layout, diagnostics) =
            render_object_placements(&image, &ObjectStream::default(), 3, 0).unwrap();
        assert_eq!(layout.width, 32);
        assert_eq!(layout.height, 13 * 16);
        assert!(layout.vertical);
        assert_eq!(placements.len(), 32 * 13 * 16);
        assert!(placements.iter().all(|placement| placement.word == 0x25));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn standard_sprite_stream_uses_native_placement_and_preview_dispatch() {
        let sprites = lm_level::NativeSpriteStream {
            header: 0,
            expanded: false,
            tokens: vec![lm_level::SpriteToken::Record(lm_level::SpriteRecord {
                encoded: vec![0x20, 0x10, 0x00],
            })],
        };
        let (rendered, diagnostics) = render_sprite_placements(&sprites, 0, 0);
        assert!(diagnostics.is_empty());
        assert_eq!(rendered.len(), 1);
        assert_eq!(rendered[0].x, 16);
        assert_eq!(rendered[0].y, 33);
        assert_eq!(
            rendered[0].subtiles,
            lm_render::render_lunar_magic_standard_sprite(0, false).unwrap()[0].subtiles
        );
    }

    #[test]
    #[ignore = "requires the retained Lunar Magic-created SMW-US ROM fixture"]
    fn retained_level_zero_object_stream_materializes_nonblank_cells() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = std::fs::read(
            root.join("oracle-work/lm363/pristine-us/exanimation-install-positive/after.smc"),
        )
        .unwrap();
        let image = lm_rom::RomImage::from_bytes(bytes).unwrap();
        let mut level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        level_layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
        let level = lm_project::Project::new(image.clone())
            .load_level_slot(0, level_layout, &lm_level::SpriteLengthTable::standard())
            .unwrap();
        let (placements, _, diagnostics) = render_object_placements(
            &image,
            &level.layer1.objects,
            level.layer1.header.level_mode(),
            level.layer1.header.object_tileset(),
        )
        .unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(placements.iter().any(|placement| placement.word != 0x25));
        let (sprites, sprite_diagnostics) = render_sprite_placements(
            &level.sprites,
            level.layer1.header.level_mode(),
            level.layer1.header.sprite_tileset(),
        );
        assert!(!sprites.is_empty(), "{sprite_diagnostics:?}");
    }
}
