use super::{Command, Controller, RomMap16Editor};
use crate::{dialogs, document_loader::BoundedRead};
use eframe::egui;
use lm_app::{
    MaterializedSnesMap16Tileset, SNES_TILESET_GRAPHICS_LEN, SNES_TILESET_MAP_LEN,
    SNES_TILESET_PALETTE_ROW_LEN, SnesMap16DefinitionPlacement, SnesMap16TilesetImport,
};
use lm_level::Map16Page;
use lm_project::{GraphicsSaveOptions, PaletteSaveOptions, Project, RomMutation};
use lm_rom::RomImage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PendingSnesTileset {
    revision: u64,
    page: usize,
    placement: SnesMap16DefinitionPlacement,
    includes_palette: bool,
    palette_row: u8,
    remap_offset: u16,
    color_map: Option<[u8; 16]>,
    level: u16,
}

pub(super) struct SnesTilesetPreview {
    pending: PendingSnesTileset,
    materialized: MaterializedSnesMap16Tileset,
    candidate_page: Map16Page,
    assignments: Vec<u16>,
    written_definitions: usize,
    palette: Option<[lm_graphics::Bgr555; 16]>,
}

impl RomMap16Editor {
    pub(super) fn poll_snes_tileset_io(&mut self, context: &egui::Context) {
        let Some(result) = self.snes_tileset_loader.show(context) else {
            return;
        };
        let pending = self.pending_snes_tileset.take();
        let result = result.and_then(|loaded| {
            let pending = pending.ok_or("SNES tileset request is missing")?;
            let workspace = self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
            if workspace.controller.revision() != pending.revision {
                return Err("the ROM changed while the SNES tileset was loading".into());
            }
            let target = workspace
                .controller
                .set()
                .pages
                .get(pending.page)
                .ok_or_else(|| format!("Map16 page {:02X} is unavailable", pending.page))?;
            let files = loaded.files;
            let expected = if pending.includes_palette { 3 } else { 2 };
            if files.len() != expected {
                return Err(format!(
                    "SNES tileset loader returned {} files, expected {expected}",
                    files.len()
                ));
            }
            let palette = pending.includes_palette.then(|| files[2].1.as_slice());
            self.snes_tileset_preview = Some(prepare_preview(
                pending,
                &files[0].1,
                &files[1].1,
                palette,
                target,
            )?);
            Ok(())
        });
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    pub(super) fn snes_tileset_controls(
        &mut self,
        ui: &mut egui::Ui,
        blocked: bool,
        project_revision: u64,
    ) {
        ui.separator();
        ui.label("SNES graphics set + screen tile map");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.snes_tileset_include_palette, "Import palette row");
            ui.add_enabled(
                self.snes_tileset_include_palette,
                egui::DragValue::new(&mut self.snes_tileset_palette_row)
                    .range(0..=15)
                    .prefix("row ")
                    .hexadecimal(1, false, true),
            );
            ui.checkbox(
                &mut self.snes_tileset_deduplicate,
                "Optimize Map16 definitions",
            );
            if ui
                .add_enabled(
                    !blocked && self.page < 0x100,
                    egui::Button::new("Load SNES tileset…"),
                )
                .clicked()
            {
                self.start_snes_tileset_load(project_revision);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Graphics offset");
            ui.add(
                egui::DragValue::new(&mut self.snes_tileset_graphics_offset)
                    .range(0..=0x3ff)
                    .hexadecimal(3, false, true),
            );
            ui.label("Map offset");
            ui.add(
                egui::DragValue::new(&mut self.snes_tileset_map_offset)
                    .range(0..=0x3ff)
                    .hexadecimal(3, false, true),
            );
            ui.checkbox(&mut self.snes_tileset_color_filter, "Color-map filter");
            ui.add_enabled(
                self.snes_tileset_color_filter,
                egui::DragValue::new(&mut self.snes_tileset_color_filter_index).range(0..=15),
            );
        });
        if self.snes_tileset_color_filter {
            let map = &mut self.snes_tileset_color_maps
                [usize::from(self.snes_tileset_color_filter_index)];
            ui.horizontal_wrapped(|ui| {
                ui.label("Color map");
                for value in map {
                    ui.add(
                        egui::DragValue::new(value)
                            .range(0..=15)
                            .hexadecimal(1, false, true),
                    );
                }
            });
        }
        ui.small("Loads the original .set/.bin plus 32×32 .map workflow with an optional 16-color .col/.pal row. Preview is revision-bound and blocks conflicting Map16 work.");
    }

    fn start_snes_tileset_load(&mut self, project_revision: u64) {
        let Ok(level) = u16::from_str_radix(self.preview_level.trim(), 16) else {
            self.error = Some("preview level must be hexadecimal".into());
            return;
        };
        if level > 0x1ff {
            self.error = Some("preview level must be in 000..1FF".into());
            return;
        }
        let Some(graphics) = dialogs::choose_snes_graphics_set() else {
            return;
        };
        let Some(tilemap) = dialogs::choose_snes_screen_tile_map() else {
            return;
        };
        let palette = if self.snes_tileset_include_palette {
            let Some(path) = dialogs::choose_snes_palette_row() else {
                return;
            };
            Some(path)
        } else {
            None
        };
        let mut reads = vec![
            BoundedRead::new(graphics, SNES_TILESET_GRAPHICS_LEN as u64, "SNES GFX set"),
            BoundedRead::new(tilemap, SNES_TILESET_MAP_LEN as u64, "SNES screen tile map"),
        ];
        if let Some(path) = palette {
            reads.push(BoundedRead::new(
                path,
                SNES_TILESET_PALETTE_ROW_LEN as u64,
                "SNES palette row",
            ));
        }
        match self.snes_tileset_loader.start(reads) {
            Ok(()) => {
                self.pending_snes_tileset = Some(PendingSnesTileset {
                    revision: project_revision,
                    page: self.page,
                    placement: if self.snes_tileset_deduplicate {
                        SnesMap16DefinitionPlacement::DeduplicatedIntoBlankDefinitions
                    } else {
                        SnesMap16DefinitionPlacement::Direct
                    },
                    includes_palette: self.snes_tileset_include_palette,
                    palette_row: self.snes_tileset_palette_row,
                    remap_offset: self
                        .snes_tileset_graphics_offset
                        .wrapping_add(self.snes_tileset_map_offset)
                        & 0x03ff,
                    color_map: self.snes_tileset_color_filter.then(|| {
                        self.snes_tileset_color_maps
                            [usize::from(self.snes_tileset_color_filter_index)]
                    }),
                    level,
                });
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn snes_tileset_preview_window(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> Option<Command> {
        let Some(preview) = self.snes_tileset_preview.as_ref() else {
            return None;
        };
        let stale = preview.pending.revision != project_revision;
        let page = preview.pending.page;
        let placement = preview.pending.placement;
        let written = preview.written_definitions;
        let palette_row = preview.palette.map(|_| preview.pending.palette_row);
        let assignment_span = preview
            .assignments
            .first()
            .zip(preview.assignments.last())
            .map(|(first, last)| format!("${first:04X}–${last:04X}"))
            .unwrap_or_else(|| "none".into());
        // Keep the complete staged products alive until discard or the atomic ROM route consumes
        // them. Reading these shapes here also guards accidental partial preview construction.
        let graphics_tiles = preview.materialized.graphics.tiles.len();
        let candidate_tiles = preview.candidate_page.tiles.len();
        let mut discard = false;
        let mut apply = false;
        egui::Window::new("SNES tileset import preview")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(format!("Target Map16 page: ${page:02X}"));
                ui.label(format!("Placement: {placement:?}"));
                ui.label(format!("Graphics tiles: {graphics_tiles}"));
                ui.label(format!("Candidate definitions: {candidate_tiles}"));
                ui.label(format!("Definitions written: {written}"));
                ui.label(format!("Index-grid span: {assignment_span}"));
                ui.label(match palette_row {
                    Some(row) => format!("Palette row loaded: ${row:X}"),
                    None => "Palette row loaded: no".into(),
                });
                if stale {
                    ui.colored_label(egui::Color32::YELLOW, "The ROM changed; discard this preview.");
                }
                ui.small("The decoded graphics, optional palette, candidate page, and background index grid are retained together for the atomic ROM-application milestone.");
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !stale,
                            egui::Button::new("Apply graphics + palette + Map16"),
                        )
                        .clicked()
                    {
                        apply = true;
                    }
                    if ui.button("Discard preview").clicked() {
                        discard = true;
                    }
                });
            });
        if discard {
            self.snes_tileset_preview = None;
        }
        if apply {
            match self.prepare_snes_tileset_graphics_map16_command() {
                Ok(command) => {
                    self.snes_tileset_preview = None;
                    return Some(command);
                }
                Err(error) => self.error = Some(error),
            }
        }
        None
    }

    fn prepare_snes_tileset_graphics_map16_command(&self) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
        let preview = self
            .snes_tileset_preview
            .as_ref()
            .ok_or("SNES tileset preview is closed")?;
        if workspace.controller.revision() != preview.pending.revision {
            return Err("the ROM changed after the SNES tileset preview was created".into());
        }

        let mut controller = workspace.controller.clone();
        let replacements = preview
            .candidate_page
            .tiles
            .iter()
            .copied()
            .enumerate()
            .map(|(tile, value)| {
                (
                    lm_level::Map16Address {
                        page: preview.pending.page,
                        tile,
                    },
                    value,
                )
            })
            .collect();
        controller.apply_edits(&[lm_app::Map16ControllerEdit::ReplaceTiles {
            replacements,
            resolution_limit: controller.set().pages.len() * Map16Page::TILE_COUNT,
        }])?;
        let map16_commit = match &controller {
            Controller::Profile(controller) => controller
                .prepare_commit(
                    "Import SNES tileset graphics, palette, and Map16",
                    &self.profile_save_options(workspace)?,
                )
                .map_err(|error| error.to_string())?,
            Controller::Smw(controller) => controller
                .prepare_commit(
                    "Import SNES tileset graphics, palette, and Map16",
                    &self.smw_save_options(workspace)?,
                )
                .map_err(|error| error.to_string())?,
        };

        let image = RomImage::from_bytes(workspace.snapshot.rom_bytes.clone())
            .map_err(|error| error.to_string())?;
        let before = image.logical_bytes().to_vec();
        let mut project = Project::new(image);
        project
            .apply_mutation("Stage imported Map16 page", &map16_commit.mutation)
            .map_err(|error| error.to_string())?;
        let assignments =
            active_foreground_graphics_files(workspace, &project, preview.pending.level)?;
        let layout = match &workspace.profile {
            Some(profile) => profile.graphics,
            None => lm_profile::smw_us_v1_vanilla_graphics_layout(),
        };
        let mut baselines = Vec::with_capacity(assignments.len());
        for file in assignments.iter().copied() {
            let mut graphics = project
                .load_graphics_file(file, layout)
                .map_err(|error| error.to_string())?;
            if graphics.tiles.len() > 0x80 {
                return Err(format!(
                    "active graphics file {file:03X} has {} tiles; expected at most 128",
                    graphics.tiles.len()
                ));
            }
            graphics.tiles.resize_with(0x80, || {
                lm_graphics::IndexedTile::new([0; lm_graphics::IndexedTile::PIXEL_COUNT])
            });
            baselines.push(graphics);
        }
        let staged = lm_app::stage_snes_tileset_graphics_files(
            &preview.materialized,
            &assignments,
            &baselines,
        )
        .map_err(|error| error.to_string())?;
        let allocation = if let Some(profile) = &workspace.profile {
            profile
                .allocation_policy_for_rom(
                    crate::rom_allocation::parse_search_range(
                        &self.search_start,
                        &self.search_end,
                    )?,
                    &project.rom,
                    workspace.internal_header,
                )
                .map_err(|error| error.to_string())?
        } else {
            self.smw_save_options(workspace)?.allocation
        };
        let options = GraphicsSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        for (file, graphics) in &staged {
            project
                .save_graphics_file(*file, graphics, layout, &options)
                .map_err(|error| error.to_string())?;
        }
        let expected_palette = if let Some(row) = preview.palette {
            Some(stage_and_save_palette_row(
                &mut project,
                preview.pending.level,
                preview.pending.palette_row,
                row,
                allocation.clone(),
            )?)
        } else {
            None
        };
        let expected_layer2 = stage_background_index_grid(
            &mut project,
            preview,
            allocation,
            workspace.internal_header + 0x1c,
        )?;
        project
            .refresh_checksum(workspace.internal_header + 0x1c)
            .map_err(|error| error.to_string())?;
        for (file, expected) in staged {
            let mut reopened = project
                .load_graphics_file(file, layout)
                .map_err(|error| error.to_string())?;
            reopened.tiles.resize_with(0x80, || {
                lm_graphics::IndexedTile::new([0; lm_graphics::IndexedTile::PIXEL_COUNT])
            });
            if reopened != expected {
                return Err(format!("saved graphics file {file:03X} did not reopen"));
            }
        }
        if let Some((palette_layout, expected)) = expected_palette {
            let reopened = project
                .load_palette(usize::from(preview.pending.level), palette_layout)
                .map_err(|error| error.to_string())?;
            if reopened != expected {
                return Err("saved SNES tileset palette did not reopen".into());
            }
        }
        if let Some((layer2_layout, level_mode, expected)) = expected_layer2 {
            let reopened = project
                .load_level_layer2_with_descriptor(
                    usize::from(preview.pending.level),
                    level_mode,
                    layer2_layout,
                )
                .map_err(|error| error.to_string())?;
            if reopened != expected {
                return Err("saved SNES tileset background index grid did not reopen".into());
            }
        }
        let mutation = RomMutation::between(
            workspace.snapshot.identity.mapper,
            &before,
            project.rom.logical_bytes(),
        )
        .map_err(|error| error.to_string())?;
        Ok(lm_app::PreparedRomCommit {
            expected_revision: preview.pending.revision,
            description: "Import SNES tileset graphics, palette, and Map16".into(),
            mutation,
        }
        .into_command())
    }
}

fn stage_background_index_grid(
    project: &mut Project,
    preview: &SnesTilesetPreview,
    mut allocation: lm_rats::AllocationPolicy,
    checksum_field: usize,
) -> Result<
    Option<(
        lm_project::LevelLayer2RomLayout,
        u8,
        lm_project::LoadedLevelLayer2,
    )>,
    String,
> {
    if !(0x80..0x100).contains(&preview.pending.page)
        || preview.pending.placement
            != SnesMap16DefinitionPlacement::DeduplicatedIntoBlankDefinitions
    {
        return Ok(None);
    }
    if preview.assignments.len() != 256 {
        return Err(format!(
            "background SNES tileset requires 256 index assignments, got {}",
            preview.assignments.len()
        ));
    }
    let bank = (preview.pending.page - 0x80) >> 4;
    let first = (bank + 8) * 0x1000;
    if !preview.assignments.iter().any(|assignment| {
        first <= usize::from(*assignment) && usize::from(*assignment) < first + 0x1000
    }) {
        return Err(format!(
            "background index grid has no tile in active bank ${first:04X}–${:04X}",
            first + 0x0fff
        ));
    }
    let level = usize::from(preview.pending.level);
    let level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let loaded_level = project
        .load_level_slot(
            level,
            level_layout,
            &lm_level::SpriteLengthTable::standard(),
        )
        .map_err(|error| error.to_string())?;
    let level_mode = loaded_level.layer1.header.level_mode();
    let Some(layout) = lm_profile::smw_us_v1_level_layer2_layout(&project.rom, level)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    if let Some(table) = layout.descriptor_table {
        let end = table
            .entries
            .checked_mul(table.stride)
            .and_then(|len| table.offset.checked_add(len))
            .ok_or("Layer 2 descriptor-table range overflow")?;
        let range = lm_rats::ProtectedRange(table.offset..end);
        if !allocation
            .protected
            .iter()
            .any(|protected| protected.0.start <= range.0.start && range.0.end <= protected.0.end)
        {
            allocation.protected.push(range);
        }
    }
    let mut loaded = project
        .load_level_layer2_with_descriptor(level, level_mode, layout)
        .map_err(|error| error.to_string())?;
    let lm_level::NativeLayer2Data::Tilemap(bytes) = &mut loaded.data else {
        // Lunar Magic reports "Cannot Modify" here but retains the graphics/Map16 import.
        return Ok(None);
    };
    for y in 0..16 {
        for x in 0..16 {
            let storage = lm_level::native_layer2_tilemap_index(x, y)
                .ok_or("background index-grid coordinate is outside Layer 2")?;
            let offset = storage * 2;
            bytes[offset..offset + 2]
                .copy_from_slice(&(preview.assignments[y * 16 + x] & 0x0fff).to_le_bytes());
        }
    }
    project
        .save_level_layer2_with_descriptor_and_checksum(
            level,
            level_mode,
            &loaded,
            layout,
            &lm_project::LevelLayer2SaveOptions {
                allocation,
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
            checksum_field,
        )
        .map_err(|error| error.to_string())?;
    Ok(Some((layout, level_mode, loaded)))
}

fn stage_and_save_palette_row(
    project: &mut Project,
    level: u16,
    row: u8,
    colors: [lm_graphics::Bgr555; 16],
    allocation: lm_rats::AllocationPolicy,
) -> Result<(lm_project::PaletteRomLayout, lm_graphics::Palette), String> {
    if row > 15 {
        return Err(format!("palette row must be in 0..F, got {row:X}"));
    }
    let installation = lm_profile::smw_us_v1_custom_palette_installation();
    let existing_layout = installation
        .resolve(&project.rom)
        .map_err(|error| error.to_string())?;
    let mut palette = if let Some(layout) = existing_layout {
        project
            .load_palette(usize::from(level), layout)
            .map_err(|error| error.to_string())?
    } else {
        let loaded = project
            .load_level_slot(
                usize::from(level),
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .map_err(|error| error.to_string())?;
        let composed =
            lm_profile::compose_smw_us_v1_level_palette(project, level, loaded.layer1.header, 0)
                .map_err(|error| error.to_string())?;
        let mut palette = composed.palette;
        palette.colors.push(composed.backdrop);
        palette
    };
    if existing_layout.is_none() {
        let shared_layout = lm_profile::smw_us_v1_shared_palette_layout();
        let expected = project
            .rom
            .read(
                shared_layout.table_offset,
                lm_graphics::SmwPaletteFile::EXPANDED_FILE_LEN,
            )
            .map_err(|error| error.to_string())?
            .to_vec();
        let shared = lm_graphics::SmwPaletteFile::expanded(
            expected[0x10..].to_vec(),
            expected[..0x10].to_vec(),
        )
        .map_err(|error| error.to_string())?;
        let plan =
            lm_profile::smw_us_v1_expanded_shared_palette_installation_plan(&shared, &expected)
                .map_err(|error| error.to_string())?;
        project
            .install_relocatable_patch(&plan)
            .map_err(|error| error.to_string())?;
    }
    let layout = installation
        .resolve(&project.rom)
        .map_err(|error| error.to_string())?
        .ok_or("custom palette installation did not resolve")?;
    if palette.colors.len() == lm_profile::SMW_US_V1_CUSTOM_PALETTE_COLORS {
        palette.colors.rotate_left(1);
    }
    let start = usize::from(row) * 16;
    palette
        .colors
        .get_mut(start..start + 16)
        .ok_or_else(|| format!("palette row {row:X} is outside the working palette"))?
        .copy_from_slice(&colors);
    if palette.colors.len() == lm_profile::SMW_US_V1_CUSTOM_PALETTE_COLORS {
        palette.colors.rotate_right(1);
    }
    project
        .save_palette(
            usize::from(level),
            &palette,
            layout,
            &PaletteSaveOptions {
                allocation,
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .map_err(|error| error.to_string())?;
    Ok((layout, palette))
}

fn active_foreground_graphics_files(
    workspace: &super::Workspace,
    project: &Project,
    level: u16,
) -> Result<Vec<usize>, String> {
    if let Some(profile) = &workspace.profile
        && (profile.game != lm_rom::SupportedGame::SuperMarioWorld
            || profile.region != lm_rom::Region::NorthAmerica
            || profile.revision != 0)
    {
        return Err(format!(
            "SNES tileset graphics ownership is not recovered for profile {}",
            profile.name
        ));
    }
    let (layout, lengths) = match &workspace.profile {
        Some(profile) => (
            profile
                .level_layout_for_rom(&project.rom)
                .map_err(|error| error.to_string())?,
            profile.sprite_lengths.clone(),
        ),
        None => (
            lm_profile::smw_us_v1_vanilla_level_layout(),
            lm_level::SpriteLengthTable::standard(),
        ),
    };
    let loaded = project
        .load_level_slot(usize::from(level), layout, &lengths)
        .map_err(|error| error.to_string())?;
    let settings = if let Some(settings_layout) = workspace
        .profile
        .as_ref()
        .and_then(|profile| profile.expanded_settings)
    {
        project
            .load_expanded_level_settings(usize::from(level), settings_layout)
            .map_err(|error| error.to_string())?
    } else {
        lm_profile::load_smw_us_v1_expanded_level_settings(project, usize::from(level))
            .map_err(|error| error.to_string())?
            .settings
    };
    let bypass = lm_level::ExpandedLevelHeader::from(&settings).super_graphics_bypass();
    if bypass.enabled {
        // The expanded record follows the dialog's FG1, FG2, FG3, BG1, BG2, BG3 order;
        // the 8x8 editor and `.set` workspace follow native VRAM order.
        return Ok([0, 1, 3, 2, 4, 5]
            .map(|slot| usize::from(bypass.foreground_background[slot]))
            .into());
    }
    let foreground = lm_profile::smw_us_v1_object_tileset_graphics_files(
        &project.rom,
        usize::from(loaded.layer1.header.object_tileset()),
    )
    .map_err(|error| error.to_string())?;
    Ok(foreground.into())
}

fn prepare_preview(
    pending: PendingSnesTileset,
    graphics: &[u8],
    tilemap: &[u8],
    palette: Option<&[u8]>,
    target: &Map16Page,
) -> Result<SnesTilesetPreview, String> {
    let decoded = SnesMap16TilesetImport::decode(graphics, tilemap, palette)
        .map_err(|error| error.to_string())?;
    // The installed editor's current 1,024-tile workspace is canonical, so no additional external
    // graphics-remap stream is active here.
    let remap = std::array::from_fn(|index| u16::try_from(index).unwrap());
    let imported_palette = decoded.palette_row;
    let materialized = decoded
        .materialize_with_options(&remap, pending.remap_offset, pending.color_map.as_ref())
        .map_err(|error| error.to_string())?;
    let mut candidate_page = target.clone();
    let applied = materialized
        .apply_to_page(
            &mut candidate_page,
            u8::try_from(pending.page).map_err(|_| "SNES tileset target page exceeds FF")?,
            pending.placement,
        )
        .map_err(|error| error.to_string())?;
    Ok(SnesTilesetPreview {
        pending,
        palette: imported_palette,
        materialized,
        candidate_page,
        assignments: applied.assignments,
        written_definitions: applied.written_definitions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::{AppState, Command, LUNAR_MAGIC_BLANK_MAP16_WORD};
    use lm_graphics::{GraphicsFile4bpp, IndexedTile};
    use lm_level::{Map16Tile, Subtile};
    use lm_rom::{RomImage, detect_identity};

    fn blank_page() -> Map16Page {
        Map16Page::new(vec![
            Map16Tile {
                top_left: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
                top_right: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
                bottom_left: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
                bottom_right: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
                acts_like: 0x130,
            };
            Map16Page::TILE_COUNT
        ])
        .unwrap()
    }

    fn pending(placement: SnesMap16DefinitionPlacement) -> PendingSnesTileset {
        PendingSnesTileset {
            revision: 7,
            page: 0x82,
            placement,
            includes_palette: false,
            palette_row: 0,
            remap_offset: 0,
            color_map: None,
            level: 0x105,
        }
    }

    #[test]
    fn native_preview_retains_all_atomic_products_and_target_behavior() {
        let preview = prepare_preview(
            pending(SnesMap16DefinitionPlacement::Direct),
            &[0; 32],
            &[0; SNES_TILESET_MAP_LEN],
            None,
            &blank_page(),
        )
        .unwrap();
        assert_eq!(preview.materialized.graphics.tiles.len(), 1024);
        assert_eq!(preview.candidate_page.tiles.len(), 256);
        assert_eq!(preview.candidate_page.tiles[0].acts_like, 0x130);
        assert_eq!(preview.assignments[0], 0x8200);
        assert_eq!(preview.assignments[255], 0x82ff);
        assert_eq!(preview.written_definitions, 256);
        assert!(preview.palette.is_none());
    }

    #[test]
    fn native_preview_rejects_staged_shape_or_optimized_space_before_mutation() {
        assert!(
            prepare_preview(
                pending(SnesMap16DefinitionPlacement::Direct),
                &[],
                &[0; SNES_TILESET_MAP_LEN - 1],
                None,
                &blank_page(),
            )
            .is_err()
        );

        let mut occupied = blank_page();
        for tile in &mut occupied.tiles {
            tile.top_left = Subtile(0x2222);
        }
        let before = occupied.clone();
        assert!(
            prepare_preview(
                pending(SnesMap16DefinitionPlacement::DeduplicatedIntoBlankDefinitions),
                &[],
                &[0; SNES_TILESET_MAP_LEN],
                None,
                &occupied,
            )
            .is_err()
        );
        assert_eq!(occupied, before);
    }

    #[test]
    fn native_preview_applies_captured_combined_offset_and_color_map() {
        let mut request = pending(SnesMap16DefinitionPlacement::Direct);
        request.remap_offset = 1;
        let mut colors = std::array::from_fn(|index| index as u8);
        colors[1] = 7;
        request.color_map = Some(colors);
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([1; 64])],
        }
        .encode()
        .unwrap();
        let preview = prepare_preview(
            request,
            &graphics,
            &[0; SNES_TILESET_MAP_LEN],
            None,
            &blank_page(),
        )
        .unwrap();
        assert_eq!(preview.candidate_page.tiles[0].top_left.0, 1);
        assert_eq!(preview.materialized.graphics.tiles[1].pixels(), &[7; 64]);
    }

    #[test]
    fn native_apply_commits_graphics_and_map16_atomically_reopens_and_undoes() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::ShowMap16).unwrap();
        let mut editor = RomMap16Editor::default();
        editor.open(&app);
        let workspace = editor.workspace.as_ref().unwrap();
        let revision = workspace.controller.revision();
        let target = workspace.controller.set().pages[2].clone();
        let mut request = pending(SnesMap16DefinitionPlacement::Direct);
        request.revision = revision;
        request.page = 2;
        request.level = 0x105;
        request.includes_palette = true;
        request.palette_row = 3;
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([6; 64])],
        }
        .encode()
        .unwrap();
        let map = (0..0x400)
            .flat_map(|_| 0x0400_u16.to_le_bytes())
            .collect::<Vec<_>>();
        let palette_bytes = (0_u16..16)
            .flat_map(|value| (0x1200 | value).to_le_bytes())
            .collect::<Vec<_>>();
        editor.snes_tileset_preview =
            Some(prepare_preview(request, &graphics, &map, Some(&palette_bytes), &target).unwrap());

        let command = editor
            .prepare_snes_tileset_graphics_map16_command()
            .unwrap();
        app.dispatch(command).unwrap();
        let project = app.project().unwrap();
        let reopened_map16 = lm_profile::load_smw_us_v1_complete_map16(project).unwrap();
        assert_eq!(reopened_map16.foreground.definitions[2 * 256 * 4], 0x0400);
        let reopened_graphics = project
            .load_graphics_file(0x14, lm_profile::smw_us_v1_vanilla_graphics_layout())
            .unwrap();
        assert_eq!(reopened_graphics.tiles[0].pixels(), &[6; 64]);
        let palette_layout = lm_profile::smw_us_v1_custom_palette_installation()
            .resolve(&project.rom)
            .unwrap()
            .unwrap();
        let mut reopened_palette = project.load_palette(0x105, palette_layout).unwrap();
        reopened_palette.colors.rotate_left(1);
        assert_eq!(
            &reopened_palette.colors[3 * 16..4 * 16],
            &(0_u16..16)
                .map(|value| lm_graphics::Bgr555(0x1200 | value))
                .collect::<Vec<_>>()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.as_file_bytes(), original);
    }

    #[test]
    fn native_apply_uses_super_gfx_bypass_in_vram_slot_order() {
        let pristine = crate::test_support::pristine_smw_us_rom_bytes();
        let image = lm_rom::RomImage::from_bytes(pristine).unwrap();
        let mut installed = lm_project::Project::new(image);
        let map16_plan = lm_profile::smw_us_v1_map16_runtime_installation_plan(
            installed.rom.logical_bytes(),
            lm_rats::AllocationPolicy {
                search: 0x90_000..0x10_0000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![lm_rats::ProtectedRange(0x7fc0..0x8000)],
            },
            0x7fdc,
        )
        .unwrap();
        installed
            .rom
            .expand(lm_rom::Mapper::LoRom, 0x10_0000, 0xff)
            .unwrap();
        installed
            .install_relocatable_patch(
                &lm_profile::smw_us_v1_expanded_settings_installation_plan().unwrap(),
            )
            .unwrap();
        installed.install_relocatable_patch(&map16_plan).unwrap();
        let settings_layout = lm_profile::smw_us_v1_expanded_settings_layout();
        let mut header = lm_level::ExpandedLevelHeader::from(
            installed
                .load_expanded_level_settings(0x105, settings_layout)
                .unwrap(),
        );
        header
            .set_super_graphics_bypass(lm_level::SuperGraphicsBypass {
                enabled: true,
                // Dialog order: FG1, FG2, FG3, BG1, BG2, BG3.
                foreground_background: [0x14, 0x17, 0x15, 0x19, 0x14, 0x17],
                sprites: [0, 1, 0x13, 0x20],
            })
            .unwrap();
        installed
            .save_expanded_level_settings(
                0x105,
                &lm_level::ExpandedLevelSettingsRecord::from(header),
                settings_layout,
                0x7fdc,
            )
            .unwrap();
        let original = installed.rom.as_file_bytes().to_vec();
        let layout = lm_profile::smw_us_v1_vanilla_graphics_layout();
        let before_fg3 = installed.load_graphics_file(0x15, layout).unwrap();

        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::ShowMap16).unwrap();
        let mut editor = RomMap16Editor::default();
        editor.open(&app);
        editor.search_start = "80000".into();
        editor.search_end = format!("{:X}", app.project().unwrap().rom.logical_len());
        let workspace = editor.workspace.as_ref().unwrap();
        let mut request = pending(SnesMap16DefinitionPlacement::Direct);
        request.revision = workspace.controller.revision();
        request.page = 2;
        request.level = 0x105;
        let target = workspace.controller.set().pages[2].clone();
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([0x0e; 64]); 0x101],
        }
        .encode()
        .unwrap();
        let map = (0..0x400)
            .flat_map(|_| 0x0100_u16.to_le_bytes())
            .collect::<Vec<_>>();
        editor.snes_tileset_preview =
            Some(prepare_preview(request, &graphics, &map, None, &target).unwrap());
        let command = editor
            .prepare_snes_tileset_graphics_map16_command()
            .unwrap();
        app.dispatch(command).unwrap();

        let project = app.project().unwrap();
        assert_eq!(
            project.load_graphics_file(0x19, layout).unwrap().tiles[0].pixels(),
            &[0x0e; 64]
        );
        assert_eq!(
            project.load_graphics_file(0x15, layout).unwrap(),
            before_fg3
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.as_file_bytes(), original);
    }

    #[test]
    fn optimized_background_apply_pastes_the_native_index_grid_into_layer2() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::ShowMap16).unwrap();
        let mut editor = RomMap16Editor::default();
        editor.open(&app);
        let workspace = editor.workspace.as_ref().unwrap();
        let revision = workspace.controller.revision();
        let page = (0x80..0x100)
            .find(|page| {
                workspace.controller.set().pages[*page]
                    .tiles
                    .iter()
                    .copied()
                    .any(lm_app::is_lunar_magic_blank_map16_tile)
            })
            .unwrap();
        let target = workspace.controller.set().pages[page].clone();
        let mut request = pending(SnesMap16DefinitionPlacement::DeduplicatedIntoBlankDefinitions);
        request.revision = revision;
        request.page = page;
        request.level = 0x105;
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([9; 64])],
        }
        .encode()
        .unwrap();
        let map = (0..0x400)
            .flat_map(|_| 0x0400_u16.to_le_bytes())
            .collect::<Vec<_>>();
        let preview = prepare_preview(request, &graphics, &map, None, &target).unwrap();
        let assignment = preview.assignments[0];
        assert!(preview.assignments.iter().all(|value| *value == assignment));
        editor.snes_tileset_preview = Some(preview);
        let command = editor
            .prepare_snes_tileset_graphics_map16_command()
            .unwrap();
        app.dispatch(command).unwrap();

        let project = app.project().unwrap();
        let loaded_level = project
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let layout = lm_profile::smw_us_v1_level_layer2_layout(&project.rom, 0x105)
            .unwrap()
            .unwrap();
        let layer2 = project
            .load_level_layer2(0x105, loaded_level.layer1.header.level_mode(), layout)
            .unwrap();
        let lm_level::NativeLayer2Data::Tilemap(bytes) = layer2 else {
            panic!("level 105 background must be tilemap-backed");
        };
        for y in 0..16 {
            for x in 0..16 {
                let index = lm_level::native_layer2_tilemap_index(x, y).unwrap() * 2;
                assert_eq!(
                    u16::from_le_bytes([bytes[index], bytes[index + 1]]),
                    assignment & 0x0fff
                );
            }
        }
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.as_file_bytes(), original);
    }

    #[test]
    fn optimized_palette_apply_matches_across_copier_header_variants() {
        let physical = crate::test_support::pristine_smw_us_rom_bytes();
        let physical_image = RomImage::from_bytes(physical.clone()).unwrap();
        let variants = [physical, physical_image.logical_bytes().to_vec()];
        let mut logical_results = Vec::new();

        for original in variants {
            let original_image = RomImage::from_bytes(original.clone()).unwrap();
            let original_header = original_image.copier_header_bytes().map(<[u8]>::to_vec);
            let mut app = AppState::default();
            app.load_rom(original.clone()).unwrap();
            app.dispatch(Command::ShowMap16).unwrap();
            let mut editor = RomMap16Editor::default();
            editor.open(&app);
            let workspace = editor.workspace.as_ref().unwrap();
            let page = (0x80..0x100)
                .find(|page| {
                    workspace.controller.set().pages[*page]
                        .tiles
                        .iter()
                        .copied()
                        .any(lm_app::is_lunar_magic_blank_map16_tile)
                })
                .unwrap();
            let target = workspace.controller.set().pages[page].clone();
            let mut request =
                pending(SnesMap16DefinitionPlacement::DeduplicatedIntoBlankDefinitions);
            request.revision = workspace.controller.revision();
            request.page = page;
            request.level = 0x105;
            request.includes_palette = true;
            request.palette_row = 5;
            let graphics = GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([0x0b; 64])],
            }
            .encode()
            .unwrap();
            let map = (0..0x400)
                .flat_map(|_| 0x0400_u16.to_le_bytes())
                .collect::<Vec<_>>();
            let palette_bytes = (0_u16..16)
                .flat_map(|value| (0x2200 | value).to_le_bytes())
                .collect::<Vec<_>>();
            let preview =
                prepare_preview(request, &graphics, &map, Some(&palette_bytes), &target).unwrap();
            let assignment = preview.assignments[0];
            assert!(preview.assignments.iter().all(|value| *value == assignment));
            editor.snes_tileset_preview = Some(preview);
            let command = editor
                .prepare_snes_tileset_graphics_map16_command()
                .unwrap();
            app.dispatch(command).unwrap();

            let project = app.project().unwrap();
            let result = RomImage::from_bytes(project.save_snapshot()).unwrap();
            assert_eq!(
                result.copier_header_bytes().map(<[u8]>::to_vec),
                original_header
            );
            assert!(detect_identity(&result).unwrap().checksum_matches());
            let map16 = lm_profile::load_smw_us_v1_complete_map16(project).unwrap();
            let definition =
                ((usize::from(assignment >> 8) - 0x80) * 256 + usize::from(assignment & 0xff)) * 4;
            assert_eq!(map16.background.definitions[definition], 0x0400);
            assert_eq!(
                project
                    .load_graphics_file(0x14, lm_profile::smw_us_v1_vanilla_graphics_layout())
                    .unwrap()
                    .tiles[0]
                    .pixels(),
                &[0x0b; 64]
            );
            let palette_layout = lm_profile::smw_us_v1_custom_palette_installation()
                .resolve(&project.rom)
                .unwrap()
                .unwrap();
            let mut palette = project.load_palette(0x105, palette_layout).unwrap();
            palette.colors.rotate_left(1);
            assert_eq!(
                &palette.colors[5 * 16..6 * 16],
                &(0_u16..16)
                    .map(|value| lm_graphics::Bgr555(0x2200 | value))
                    .collect::<Vec<_>>()
            );
            let loaded_level = project
                .load_level_slot(
                    0x105,
                    lm_profile::smw_us_v1_vanilla_level_layout(),
                    &lm_level::SpriteLengthTable::standard(),
                )
                .unwrap();
            let layer2_layout = lm_profile::smw_us_v1_level_layer2_layout(&project.rom, 0x105)
                .unwrap()
                .unwrap();
            let layer2 = project
                .load_level_layer2(
                    0x105,
                    loaded_level.layer1.header.level_mode(),
                    layer2_layout,
                )
                .unwrap();
            let lm_level::NativeLayer2Data::Tilemap(bytes) = layer2 else {
                panic!("level 105 background must be tilemap-backed");
            };
            for y in 0..16 {
                for x in 0..16 {
                    let index = lm_level::native_layer2_tilemap_index(x, y).unwrap() * 2;
                    assert_eq!(
                        u16::from_le_bytes([bytes[index], bytes[index + 1]]),
                        assignment & 0x0fff
                    );
                }
            }
            logical_results.push(result.logical_bytes().to_vec());
            app.dispatch(Command::Undo).unwrap();
            assert_eq!(app.project().unwrap().rom.as_file_bytes(), original);
        }
        assert_eq!(logical_results[0], logical_results[1]);
    }
}
