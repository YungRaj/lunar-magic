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
    RomMutation, TransactionError,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomError, RomImage};
use std::{fmt, ops::Range};

/// One open native import dialog backed by immutable ROM and bitmap inputs.
pub struct NativeMap16BitmapImportSession {
    snapshot: ControllerSnapshot,
    profile: Option<RevisionProfile>,
    workspace: NativeMap16BitmapGraphicsWorkspace,
    preview: Map16BitmapImportPreviewState,
    level: usize,
    page: usize,
    smw_map16: Option<lm_profile::LoadedSmwUsV1TransferredMap16>,
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
            profile: Some(profile),
            workspace,
            preview,
            level: request.level,
            page: request.page,
            smw_map16: None,
        })
    }

    /// Opens the same import workflow directly on an authenticated original/transferred SMW-US
    /// ROM, without requiring an externally installed revision profile.
    ///
    /// # Errors
    ///
    /// Returns a typed load or import error without changing the ROM.
    pub fn new_smw_us_v1(
        snapshot: ControllerSnapshot,
        request: NativeMap16BitmapImportSessionRequest,
    ) -> Result<Self, NativeMap16BitmapImportSessionError> {
        if snapshot.identity.mapper != Mapper::LoRom {
            return Err(NativeMap16BitmapImportSessionError::Profile(
                "native SMW bitmap import requires LoROM".into(),
            ));
        }
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(NativeMap16BitmapImportSessionError::Rom)?;
        let project = Project::new(image);
        let mut level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        level_layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&project.rom)
            .map_err(NativeMap16BitmapImportSessionError::Rom)?;
        let loaded = project
            .load_level_slot(
                request.level,
                level_layout,
                &lm_level::SpriteLengthTable::standard(),
            )
            .map_err(|error| NativeMap16BitmapImportSessionError::Level(error.to_string()))?;
        let workspace = NativeMap16BitmapGraphicsWorkspace::load_smw_us_v1(
            &project,
            loaded.layer1.header.object_tileset(),
            request.extra_graphics,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        )
        .map_err(NativeMap16BitmapImportSessionError::WorkspaceLoad)?;
        let composed = lm_profile::compose_smw_us_v1_level_palette(
            &project,
            u16::try_from(request.level).map_err(|_| {
                NativeMap16BitmapImportSessionError::Level("level exceeds 16 bits".into())
            })?,
            loaded.layer1.header,
            0,
        )
        .map_err(|error| NativeMap16BitmapImportSessionError::Palette(error.to_string()))?;
        let mut palette = composed.palette;
        palette.colors.push(composed.backdrop);
        let preview = Map16BitmapImportPreviewState::new(
            Map16BitmapImportInputs {
                pixels: request.pixels,
                palette_row: request.palette_row,
                acts_like: request.acts_like,
                palette: palette.clone(),
                palette_ownership: PaletteOwnership::editable(palette.colors.len()),
                graphics: workspace.graphics.clone(),
                graphics_ownership: workspace.ownership.clone(),
                occupied: workspace.occupied.clone(),
            },
            native_map16_bitmap_import_options(),
        )
        .map_err(NativeMap16BitmapImportSessionError::Import)?;
        let smw_map16 = lm_profile::load_smw_us_v1_transferred_map16(&project)
            .map_err(|error| NativeMap16BitmapImportSessionError::Map16(error.to_string()))?;
        Ok(Self {
            snapshot,
            profile: None,
            workspace,
            preview,
            level: request.level,
            page: request.page,
            smw_map16: Some(smw_map16),
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
        if self.profile.is_none() {
            return self.prepare_smw_us_v1_commit(search);
        }
        let image = RomImage::from_bytes(self.snapshot.rom_bytes.clone())
            .map_err(NativeMap16BitmapImportSessionError::Rom)?;
        let Some(profile) = self.profile.as_ref() else {
            return Err(NativeMap16BitmapImportSessionError::Profile(
                "profile-backed import lost its revision profile".into(),
            ));
        };
        let allocation = profile
            .allocation_policy_for_rom(
                search,
                &image,
                self.snapshot.identity.internal_header_offset,
            )
            .map_err(|error| NativeMap16BitmapImportSessionError::Allocation(error.to_string()))?;
        let palette_layout = profile
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
                layout: profile.graphics,
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
                layout: profile.map16,
                options: &map16_options,
            },
            checksum_field: lm_profile::SMW_US_V1_CHECKSUM_FIELD,
        };
        prepare_map16_bitmap_rom_commit(&self.snapshot, &save)
            .map_err(|error| NativeMap16BitmapImportSessionError::Commit(error.to_string()))
    }

    #[allow(clippy::too_many_lines)]
    fn prepare_smw_us_v1_commit(
        &self,
        search: Range<usize>,
    ) -> Result<PreparedRomCommit, NativeMap16BitmapImportSessionError> {
        let image = RomImage::from_bytes(self.snapshot.rom_bytes.clone())
            .map_err(NativeMap16BitmapImportSessionError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let mut project = Project::new(image);
        let checksum_field = self.snapshot.identity.internal_header_offset + 0x1c;
        if search.end > project.rom.logical_len() {
            project
                .expand_rom(Mapper::LoRom, search.end, 0xff, checksum_field)
                .map_err(NativeMap16BitmapImportSessionError::Mutation)?;
        }
        let allocation = smw_bitmap_allocation_policy(
            search,
            project.rom.logical_len(),
            self.snapshot.identity.internal_header_offset,
        )?;
        let graphics_options = GraphicsSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let changed = self
            .workspace
            .changed_assigned_slots(&self.preview.plan().graphics)
            .map_err(NativeMap16BitmapImportSessionError::Workspace)?;
        let graphics_layout = lm_profile::smw_us_v1_vanilla_graphics_layout();
        for (_, file_number, graphics) in &changed {
            project
                .save_graphics_file(*file_number, graphics, graphics_layout, &graphics_options)
                .map_err(|error| {
                    NativeMap16BitmapImportSessionError::Graphics(error.to_string())
                })?;
        }

        let installation = lm_profile::smw_us_v1_custom_palette_installation();
        if installation
            .resolve(&project.rom)
            .map_err(NativeMap16BitmapImportSessionError::Installation)?
            .is_none()
        {
            let shared_layout = lm_profile::smw_us_v1_shared_palette_layout();
            let expected = project
                .rom
                .read(
                    shared_layout.table_offset,
                    lm_graphics::SmwPaletteFile::EXPANDED_FILE_LEN,
                )
                .map_err(NativeMap16BitmapImportSessionError::Rom)?
                .to_vec();
            let shared = lm_graphics::SmwPaletteFile::expanded(
                expected[0x10..].to_vec(),
                expected[..0x10].to_vec(),
            )
            .map_err(|error| NativeMap16BitmapImportSessionError::Palette(error.to_string()))?;
            let plan =
                lm_profile::smw_us_v1_expanded_shared_palette_installation_plan(&shared, &expected)
                    .map_err(|error| {
                        NativeMap16BitmapImportSessionError::Palette(error.to_string())
                    })?;
            project
                .install_relocatable_patch(&plan)
                .map_err(|error| NativeMap16BitmapImportSessionError::Commit(error.to_string()))?;
        }
        let palette_layout = installation
            .resolve(&project.rom)
            .map_err(NativeMap16BitmapImportSessionError::Installation)?
            .ok_or(NativeMap16BitmapImportSessionError::InstalledPaletteRequired)?;
        let native_palette = native_installed_palette(self.preview.plan().palette.clone());
        project
            .save_palette(
                self.level,
                &native_palette,
                palette_layout,
                &PaletteSaveOptions {
                    allocation: allocation.clone(),
                    previous_block: None,
                    reuse_identical: true,
                    erase_fill: 0xff,
                },
            )
            .map_err(|error| NativeMap16BitmapImportSessionError::Palette(error.to_string()))?;

        let mut map16 = self
            .smw_map16
            .clone()
            .ok_or_else(|| NativeMap16BitmapImportSessionError::Map16("missing baseline".into()))?;
        replace_transferred_page(&mut map16, self.page, &self.preview.plan().page)?;
        let map16_options = lm_profile::SmwUsV1TransferredMap16SaveOptions {
            allocation,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        lm_profile::save_smw_us_v1_transferred_map16(
            &mut project,
            &map16.definitions,
            &map16.acts_like,
            checksum_field,
            &map16_options,
        )
        .map_err(|error| NativeMap16BitmapImportSessionError::Map16(error.to_string()))?;

        for (_, file_number, graphics) in changed {
            if project
                .load_graphics_file(file_number, graphics_layout)
                .map_err(|error| NativeMap16BitmapImportSessionError::Graphics(error.to_string()))?
                != graphics
            {
                return Err(NativeMap16BitmapImportSessionError::Graphics(
                    "saved graphics did not reopen".into(),
                ));
            }
        }
        if project
            .load_palette(self.level, palette_layout)
            .map_err(|error| NativeMap16BitmapImportSessionError::Palette(error.to_string()))?
            != native_palette
        {
            return Err(NativeMap16BitmapImportSessionError::Palette(
                "saved palette did not reopen".into(),
            ));
        }
        let reopened_map16 = lm_profile::load_smw_us_v1_transferred_map16(&project)
            .map_err(|error| NativeMap16BitmapImportSessionError::Map16(error.to_string()))?;
        if reopened_map16.definitions != map16.definitions
            || reopened_map16.acts_like != map16.acts_like
        {
            return Err(NativeMap16BitmapImportSessionError::Map16(
                "saved Map16 did not reopen".into(),
            ));
        }
        let mutation = RomMutation::between(Mapper::LoRom, &before, project.rom.logical_bytes())
            .map_err(NativeMap16BitmapImportSessionError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.snapshot.revision,
            description: "Import bitmap as Map16".into(),
            mutation,
        })
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
    Graphics(String),
    Map16(String),
    Import(Map16BitmapImportError),
    Allocation(String),
    Commit(String),
    Mutation(TransactionError),
}

impl fmt::Display for NativeMap16BitmapImportSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot run native Map16 bitmap import: {self:?}")
    }
}

impl std::error::Error for NativeMap16BitmapImportSessionError {}

fn replace_transferred_page(
    map16: &mut lm_profile::LoadedSmwUsV1TransferredMap16,
    page_number: usize,
    page: &lm_level::Map16Page,
) -> Result<(), NativeMap16BitmapImportSessionError> {
    if page.tiles.len() != lm_level::Map16Page::TILE_COUNT {
        return Err(NativeMap16BitmapImportSessionError::Map16(format!(
            "imported page contains {} tiles",
            page.tiles.len()
        )));
    }
    let first_tile = page_number
        .checked_mul(lm_level::Map16Page::TILE_COUNT)
        .ok_or_else(|| NativeMap16BitmapImportSessionError::Map16("page index overflow".into()))?;
    let first_word = first_tile
        .checked_mul(4)
        .ok_or_else(|| NativeMap16BitmapImportSessionError::Map16("word index overflow".into()))?;
    let final_word = first_word
        .checked_add(lm_level::Map16Page::TILE_COUNT * 4)
        .ok_or_else(|| NativeMap16BitmapImportSessionError::Map16("word range overflow".into()))?;
    if final_word > map16.definitions.len()
        || first_tile + lm_level::Map16Page::TILE_COUNT > map16.acts_like.len()
    {
        return Err(NativeMap16BitmapImportSessionError::Map16(format!(
            "page {page_number:X} is outside native transferred Map16 storage"
        )));
    }
    for (index, tile) in page.tiles.iter().enumerate() {
        let word = first_word + index * 4;
        map16.definitions[word..word + 4].copy_from_slice(&[
            tile.top_left.0,
            tile.top_right.0,
            tile.bottom_left.0,
            tile.bottom_right.0,
        ]);
        map16.acts_like[first_tile + index] = tile.acts_like;
    }
    Ok(())
}

fn smw_bitmap_allocation_policy(
    search: Range<usize>,
    image_len: usize,
    internal_header: usize,
) -> Result<AllocationPolicy, NativeMap16BitmapImportSessionError> {
    if search.start >= search.end || search.end > image_len {
        return Err(NativeMap16BitmapImportSessionError::Allocation(format!(
            "allocation range {:X}..{:X} is outside the {:X}-byte ROM",
            search.start, search.end, image_len
        )));
    }
    let graphics = lm_profile::smw_us_v1_vanilla_graphics_layout();
    let mut protected = vec![
        ProtectedRange(internal_header..internal_header + 0x40),
        ProtectedRange(0x77550..0x77bd0),
    ];
    if let Some(planes) = graphics.split_pointer_planes {
        for offset in [planes.low_offset, planes.high_offset, planes.bank_offset] {
            protected.push(ProtectedRange(
                offset..offset + planes.entries * planes.stride,
            ));
        }
    }
    for (offset, len) in [
        (lm_profile::SMW_US_V1_MAP16_DEFINITION_WORD_OFFSET, 2),
        (lm_profile::SMW_US_V1_MAP16_DEFINITION_BANK_OFFSET, 1),
        (lm_profile::SMW_US_V1_MAP16_DEFINITION_ODD_WORD_OFFSET, 2),
        (lm_profile::SMW_US_V1_MAP16_ACTS_LOW_WORD_OFFSET, 2),
        (lm_profile::SMW_US_V1_MAP16_ACTS_LOW_BANK_OFFSET, 1),
        (lm_profile::SMW_US_V1_MAP16_ACTS_HIGH_WORD_OFFSET, 2),
        (lm_profile::SMW_US_V1_MAP16_ACTS_HIGH_BANK_OFFSET, 1),
    ] {
        protected.push(ProtectedRange(offset..offset + len));
    }
    Ok(AllocationPolicy {
        search,
        bank_size: Some(0x8000),
        fill_bytes: vec![0x00, 0xff],
        protected,
    })
}

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
    use crate::{AppState, Command};
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

    #[test]
    fn pristine_smw_import_installs_reopens_and_undoes_every_domain() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::ShowMap16).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let pixels = (0..crate::MAP16_BITMAP_PIXELS)
            .map(|index| {
                if (index / 8 + index % 8) & 1 == 0 {
                    Rgba8 {
                        red: 220,
                        green: 30,
                        blue: 40,
                        alpha: 255,
                    }
                } else {
                    Rgba8 {
                        red: 20,
                        green: 80,
                        blue: 230,
                        alpha: 255,
                    }
                }
            })
            .collect();
        let session = NativeMap16BitmapImportSession::new_smw_us_v1(
            snapshot,
            NativeMap16BitmapImportSessionRequest {
                level: 0x105,
                page: 0,
                extra_graphics: [Some(0x20), Some(0x21)],
                pixels,
                palette_row: 4,
                acts_like: 0x130,
            },
        )
        .unwrap();
        let expected_page = session.preview().plan().page.clone();
        let expected_palette = native_installed_palette(session.preview().plan().palette.clone());
        let prepared = session.prepare_commit(0x80_000..0x10_0000).unwrap();
        app.dispatch(prepared.into_command()).unwrap();

        let project = app.project().unwrap();
        let palette_layout = lm_profile::smw_us_v1_custom_palette_installation()
            .resolve(&project.rom)
            .unwrap()
            .unwrap();
        assert_eq!(
            project.load_palette(0x105, palette_layout).unwrap(),
            expected_palette
        );
        let reopened = lm_profile::load_smw_us_v1_transferred_map16(project).unwrap();
        let first = &reopened.definitions[..lm_level::Map16Page::TILE_COUNT * 4];
        let expected_words = expected_page
            .tiles
            .iter()
            .flat_map(|tile| {
                [
                    tile.top_left.0,
                    tile.top_right.0,
                    tile.bottom_left.0,
                    tile.bottom_right.0,
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(first, expected_words);
        assert_eq!(
            &reopened.acts_like[..lm_level::Map16Page::TILE_COUNT],
            expected_page
                .tiles
                .iter()
                .map(|tile| tile.acts_like)
                .collect::<Vec<_>>()
        );
        assert!(project.identity.as_ref().unwrap().checksum_matches());
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
