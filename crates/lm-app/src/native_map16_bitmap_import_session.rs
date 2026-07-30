//! Toolkit-independent native bitmap-to-Map16 import session.

use crate::{
    ControllerSnapshot, Map16BitmapImportError, Map16BitmapImportInputs, Map16BitmapImportOptions,
    Map16BitmapImportPreviewState, NativeMap16BitmapGraphicsWorkspace,
    NativeMap16BitmapWorkspaceError, NativeMap16BitmapWorkspaceLoadError, PreparedRomCommit,
    RevisionProfile, native_map16_bitmap_import_options, prepare_map16_bitmap_rom_commit,
};
use lm_graphics::{PaletteOwnership, Rgba8};
use lm_project::{
    GraphicsSaveOptions, InstalledLayoutError, Map16BitmapGraphicsSave, Map16BitmapPageSave,
    Map16BitmapPaletteSave, Map16BitmapRomSave, Map16SaveOptions, PaletteSaveOptions, Project,
};
use lm_rom::{RomError, RomImage};
use std::{fmt, ops::Range};

/// One open native import dialog backed by immutable ROM and bitmap inputs.
pub struct NativeMap16BitmapImportSession {
    snapshot: ControllerSnapshot,
    profile: RevisionProfile,
    workspace: NativeMap16BitmapGraphicsWorkspace,
    preview: Map16BitmapImportPreviewState,
    level: usize,
    page: usize,
}

/// User-selected inputs needed to open a native bitmap import session.
pub struct NativeMap16BitmapImportSessionRequest {
    pub level: usize,
    pub page: usize,
    pub extra_graphics: [Option<usize>; 2],
    pub pixels: Vec<Rgba8>,
    pub palette_row: u8,
    pub acts_like: u16,
}

impl NativeMap16BitmapImportSession {
    /// Loads the selected level's real graphics and installed palette, then builds the first
    /// synchronized preview.
    ///
    /// The current native persistence path intentionally requires Lunar Magic's installed
    /// per-level palette storage. Pristine shared-palette ROMs require a separate grouped
    /// direct-transfer transaction and are rejected rather than written through a false layout.
    ///
    /// # Errors
    ///
    /// Returns a ROM, level, graphics, palette-installation, palette-load, or import-plan error.
    pub fn new(
        snapshot: ControllerSnapshot,
        profile: RevisionProfile,
        request: NativeMap16BitmapImportSessionRequest,
    ) -> Result<Self, NativeMap16BitmapImportSessionError> {
        profile
            .ensure_identity(&snapshot.identity)
            .map_err(|error| NativeMap16BitmapImportSessionError::Profile(error.to_string()))?;
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(NativeMap16BitmapImportSessionError::Rom)?;
        let project = Project::new(image);
        let level_layout = profile
            .level_layout_for_rom(&project.rom)
            .map_err(NativeMap16BitmapImportSessionError::Rom)?;
        let loaded = project
            .load_level_slot(request.level, level_layout, &profile.sprite_lengths)
            .map_err(|error| NativeMap16BitmapImportSessionError::Level(error.to_string()))?;
        let workspace = NativeMap16BitmapGraphicsWorkspace::load_smw_us_v1(
            &project,
            loaded.layer1.header.object_tileset(),
            request.extra_graphics,
            profile.graphics,
        )
        .map_err(NativeMap16BitmapImportSessionError::WorkspaceLoad)?;
        let palette_layout = profile
            .palette_installation
            .resolve(&project.rom)
            .map_err(NativeMap16BitmapImportSessionError::Installation)?
            .ok_or(NativeMap16BitmapImportSessionError::InstalledPaletteRequired)?;
        let palette = project
            .load_palette(request.level, palette_layout)
            .map_err(|error| NativeMap16BitmapImportSessionError::Palette(error.to_string()))?;
        let palette = bitmap_working_palette(palette);
        let palette_ownership = PaletteOwnership::editable(palette.colors.len());
        let preview = Map16BitmapImportPreviewState::new(
            Map16BitmapImportInputs {
                pixels: request.pixels,
                palette_row: request.palette_row,
                acts_like: request.acts_like,
                palette,
                palette_ownership,
                graphics: workspace.graphics.clone(),
                graphics_ownership: workspace.ownership.clone(),
                occupied: workspace.occupied.clone(),
            },
            native_map16_bitmap_import_options(),
        )
        .map_err(NativeMap16BitmapImportSessionError::Import)?;
        Ok(Self {
            snapshot,
            profile,
            workspace,
            preview,
            level: request.level,
            page: request.page,
        })
    }

    #[must_use]
    pub const fn preview(&self) -> &Map16BitmapImportPreviewState {
        &self.preview
    }

    /// Recomputes every preview domain from the immutable source bitmap.
    ///
    /// # Errors
    ///
    /// Leaves the preceding valid preview unchanged when the new options cannot be materialized.
    pub fn set_options(
        &mut self,
        options: Map16BitmapImportOptions,
    ) -> Result<(), Map16BitmapImportError> {
        self.preview.set_options(options)
    }

    /// Serializes the exact previewed palette, graphics, and Map16 page into one revision-bound
    /// command.
    ///
    /// # Errors
    ///
    /// Returns an allocation, workspace, installed-layout, or grouped-save error without changing
    /// either the session or application state.
    pub fn prepare_commit(
        &self,
        search: Range<usize>,
    ) -> Result<PreparedRomCommit, NativeMap16BitmapImportSessionError> {
        let image = RomImage::from_bytes(self.snapshot.rom_bytes.clone())
            .map_err(NativeMap16BitmapImportSessionError::Rom)?;
        let allocation = self
            .profile
            .allocation_policy_for_rom(
                search,
                &image,
                self.snapshot.identity.internal_header_offset,
            )
            .map_err(|error| NativeMap16BitmapImportSessionError::Allocation(error.to_string()))?;
        let palette_layout = self
            .profile
            .palette_installation
            .resolve(&image)
            .map_err(NativeMap16BitmapImportSessionError::Installation)?
            .ok_or(NativeMap16BitmapImportSessionError::InstalledPaletteRequired)?;
        let changed = self
            .workspace
            .changed_assigned_slots(&self.preview.plan().graphics)
            .map_err(NativeMap16BitmapImportSessionError::Workspace)?;
        let graphics_options = GraphicsSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let palette_options = PaletteSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let map16_options = Map16SaveOptions {
            graphics_allocation: allocation.clone(),
            acts_like_allocation: allocation,
            previous_graphics: None,
            previous_acts_like: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let graphics_saves = changed
            .iter()
            .map(|(_, file_number, graphics)| Map16BitmapGraphicsSave {
                file_number: *file_number,
                graphics,
                layout: self.profile.graphics,
                options: &graphics_options,
            })
            .collect::<Vec<_>>();
        let native_palette = native_installed_palette(self.preview.plan().palette.clone());
        let save = Map16BitmapRomSave {
            description: "Import bitmap as Map16",
            graphics: &graphics_saves,
            palette: Map16BitmapPaletteSave {
                palette_number: self.level,
                palette: &native_palette,
                layout: palette_layout,
                options: &palette_options,
            },
            map16: Map16BitmapPageSave {
                page_number: self.page,
                page: &self.preview.plan().page,
                layout: self.profile.map16,
                options: &map16_options,
            },
            checksum_field: lm_profile::SMW_US_V1_CHECKSUM_FIELD,
        };
        prepare_map16_bitmap_rom_commit(&self.snapshot, &save)
            .map_err(|error| NativeMap16BitmapImportSessionError::Commit(error.to_string()))
    }
}

#[derive(Debug)]
pub enum NativeMap16BitmapImportSessionError {
    Rom(RomError),
    Profile(String),
    Level(String),
    WorkspaceLoad(NativeMap16BitmapWorkspaceLoadError),
    Workspace(NativeMap16BitmapWorkspaceError),
    Installation(InstalledLayoutError),
    InstalledPaletteRequired,
    Palette(String),
    Import(Map16BitmapImportError),
    Allocation(String),
    Commit(String),
}

impl fmt::Display for NativeMap16BitmapImportSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot run native Map16 bitmap import: {self:?}")
    }
}

impl std::error::Error for NativeMap16BitmapImportSessionError {}

fn bitmap_working_palette(mut native: lm_graphics::Palette) -> lm_graphics::Palette {
    if native.colors.len() == lm_profile::SMW_US_V1_CUSTOM_PALETTE_COLORS {
        native.colors.rotate_left(1);
    }
    native
}

fn native_installed_palette(mut working: lm_graphics::Palette) -> lm_graphics::Palette {
    if working.colors.len() == lm_profile::SMW_US_V1_CUSTOM_PALETTE_COLORS {
        working.colors.rotate_right(1);
    }
    working
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, Palette};

    #[test]
    fn installed_palette_rotation_matches_lunar_magic_working_buffer_order() {
        let native = Palette {
            colors: (0..lm_profile::SMW_US_V1_CUSTOM_PALETTE_COLORS)
                .map(|value| Bgr555(u16::try_from(value).unwrap()))
                .collect(),
        };
        let working = bitmap_working_palette(native.clone());
        assert_eq!(working.colors[0], Bgr555(1));
        assert_eq!(working.colors[255], Bgr555(256));
        assert_eq!(working.colors[256], Bgr555(0));
        assert_eq!(native_installed_palette(working), native);
    }

    #[test]
    fn ordinary_256_color_working_palettes_are_not_rotated() {
        let palette = Palette {
            colors: (0_u16..256).map(Bgr555).collect(),
        };
        assert_eq!(bitmap_working_palette(palette.clone()), palette);
        assert_eq!(native_installed_palette(palette.clone()), palette);
    }
}
