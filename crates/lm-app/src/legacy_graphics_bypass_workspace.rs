use crate::{ControllerSnapshot, EditorMode, PreparedRomCommit};
use lm_level::{
    LegacyGraphicsBypassSelectors, LegacyGraphicsBypassTable, LegacyGraphicsBypassTableError,
    ObjectStreamError, SpriteLengthTable,
};
use lm_project::{LevelSaveOptions, Project, RomMutation};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, Region, RomError, RomImage, SupportedGame, compute_snes_checksum};
use std::{fmt, ops::Range};

#[derive(Debug)]
pub enum LegacyGraphicsBypassWorkspaceError {
    LevelNotSelected,
    UnsupportedIdentity,
    Rom(RomError),
    Table(LegacyGraphicsBypassTableError),
    Level(String),
    ObjectStream(ObjectStreamError),
    Allocation(String),
    Mutation(String),
    Verification(String),
}

impl fmt::Display for LegacyGraphicsBypassWorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "legacy graphics-bypass workspace failed: {self:?}"
        )
    }
}

impl std::error::Error for LegacyGraphicsBypassWorkspaceError {}

/// Revision-bound model shared by the two legacy standard-GFX bypass dialogs.
#[derive(Clone, Debug)]
pub struct LegacyGraphicsBypassWorkspace {
    revision: u64,
    level: u16,
    source_file_bytes: Vec<u8>,
    layout: lm_project::LevelRomLayout,
    sprite_lengths: SpriteLengthTable,
    checksum_field: usize,
    baseline_table: LegacyGraphicsBypassTable,
    table: LegacyGraphicsBypassTable,
    baseline_selectors: LegacyGraphicsBypassSelectors,
    selectors: LegacyGraphicsBypassSelectors,
}

impl LegacyGraphicsBypassWorkspace {
    /// Loads the descriptor-authenticated `$400` table and command-`$24` selectors from one exact
    /// application snapshot. Only the SMW-US revision-0 family has recovered storage evidence.
    pub fn load(snapshot: &ControllerSnapshot) -> Result<Self, LegacyGraphicsBypassWorkspaceError> {
        let EditorMode::Level(level) = snapshot.mode else {
            return Err(LegacyGraphicsBypassWorkspaceError::LevelNotSelected);
        };
        if snapshot.identity.game != SupportedGame::SuperMarioWorld
            || snapshot.identity.region != Region::NorthAmerica
            || snapshot.identity.revision != 0
            || snapshot.identity.mapper != Mapper::LoRom
        {
            return Err(LegacyGraphicsBypassWorkspaceError::UnsupportedIdentity);
        }
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(LegacyGraphicsBypassWorkspaceError::Rom)?;
        let table =
            lm_profile::load_smw_us_v1_legacy_graphics_bypass_table(&image).map_err(|error| {
                LegacyGraphicsBypassWorkspaceError::Table(match error {
                    lm_profile::SmwUsV1LegacyGraphicsBypassError::Table(error) => error,
                    lm_profile::SmwUsV1LegacyGraphicsBypassError::Rom(error) => {
                        return LegacyGraphicsBypassWorkspaceError::Rom(error);
                    }
                })
            })?;
        let mut layout = lm_profile::smw_us_v1_vanilla_level_layout();
        layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image)
            .map_err(LegacyGraphicsBypassWorkspaceError::Rom)?;
        let sprite_lengths = SpriteLengthTable::standard();
        let project = Project::new(image);
        let loaded = project
            .load_level_slot(usize::from(level), layout, &sprite_lengths)
            .map_err(|error| LegacyGraphicsBypassWorkspaceError::Level(error.to_string()))?;
        let selectors = loaded
            .layer1
            .objects
            .legacy_graphics_bypass_selectors(loaded.layer1.header.is_vertical());
        Ok(Self {
            revision: snapshot.revision,
            level,
            source_file_bytes: snapshot.rom_bytes.clone(),
            layout,
            sprite_lengths,
            checksum_field: snapshot.identity.internal_header_offset + 0x1c,
            baseline_table: table.clone(),
            table,
            baseline_selectors: selectors,
            selectors,
        })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn level(&self) -> u16 {
        self.level
    }

    #[must_use]
    pub const fn selectors(&self) -> LegacyGraphicsBypassSelectors {
        self.selectors
    }

    pub fn set_selectors(&mut self, selectors: LegacyGraphicsBypassSelectors) {
        self.selectors = selectors;
    }

    pub fn table(&self) -> &LegacyGraphicsBypassTable {
        &self.table
    }

    pub fn table_mut(&mut self) -> &mut LegacyGraphicsBypassTable {
        &mut self.table
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.table != self.baseline_table || self.selectors != self.baseline_selectors
    }

    /// Produces one revision-checked mutation containing the level selector, list row changes,
    /// allocation/repointing when the three-byte control grows Layer 1, and the repaired checksum.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
    ) -> Result<PreparedRomCommit, LegacyGraphicsBypassWorkspaceError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(LegacyGraphicsBypassWorkspaceError::Rom)?;
        let before = image.logical_bytes().to_vec();
        if !self.is_modified() {
            return Ok(PreparedRomCommit {
                expected_revision: self.revision,
                description: description.into(),
                mutation: RomMutation::unchanged(Mapper::LoRom, before.len()),
            });
        }
        let mut project = Project::new(image);
        let mut level = project
            .load_level_slot(usize::from(self.level), self.layout, &self.sprite_lengths)
            .map_err(|error| LegacyGraphicsBypassWorkspaceError::Level(error.to_string()))?;
        level
            .layer1
            .objects
            .set_legacy_graphics_bypass_selectors(level.layer1.header.is_vertical(), self.selectors)
            .map_err(LegacyGraphicsBypassWorkspaceError::ObjectStream)?;
        let policy = allocation_policy(
            &project.rom,
            self.layout,
            self.checksum_field.saturating_sub(0x1c),
        )?;
        project
            .save_level_layer1_with_checksum(
                self.layout,
                &level,
                self.checksum_field,
                &LevelSaveOptions {
                    layer1_allocation: policy.clone(),
                    sprite_allocation: policy,
                    previous_layer1: None,
                    previous_sprites: None,
                    reuse_identical: true,
                    erase_fill: 0xff,
                },
            )
            .map_err(|error| LegacyGraphicsBypassWorkspaceError::Level(error.to_string()))?;
        project
            .rom
            .write(
                lm_profile::SMW_US_V1_LEGACY_GRAPHICS_BYPASS_TABLE_OFFSET,
                &self.table.encode(),
            )
            .map_err(LegacyGraphicsBypassWorkspaceError::Rom)?;
        let checksum = compute_snes_checksum(project.rom.logical_bytes(), self.checksum_field)
            .map_err(LegacyGraphicsBypassWorkspaceError::Rom)?;
        project
            .rom
            .write(self.checksum_field, &checksum.encoded())
            .map_err(LegacyGraphicsBypassWorkspaceError::Rom)?;

        verify_reopen(
            &project,
            self.layout,
            &self.sprite_lengths,
            self.level,
            self.selectors,
            &self.table,
            self.checksum_field,
        )?;
        let mutation = RomMutation::between(Mapper::LoRom, &before, project.rom.logical_bytes())
            .map_err(|error| LegacyGraphicsBypassWorkspaceError::Mutation(error.to_string()))?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description: description.into(),
            mutation,
        })
    }
}

fn allocation_policy(
    image: &RomImage,
    layout: lm_project::LevelRomLayout,
    internal_header: usize,
) -> Result<AllocationPolicy, LegacyGraphicsBypassWorkspaceError> {
    let len = image.logical_len();
    if len <= 0x80_000 {
        return Err(LegacyGraphicsBypassWorkspaceError::Allocation(
            "install Lunar Magic's expanded-settings prerequisite before editing legacy GFX bypass lists"
                .into(),
        ));
    }
    let search: Range<usize> = 0x80_000..len;
    Ok(AllocationPolicy {
        search,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff, 0x00],
        protected: vec![
            ProtectedRange(
                layout.layer1.offset
                    ..layout.layer1.offset + layout.layer1.entries * layout.layer1.stride,
            ),
            ProtectedRange(internal_header..internal_header + 0x40),
            ProtectedRange(
                lm_profile::SMW_US_V1_LEGACY_GRAPHICS_BYPASS_TABLE_OFFSET
                    ..lm_profile::SMW_US_V1_LEGACY_GRAPHICS_BYPASS_TABLE_OFFSET
                        + LegacyGraphicsBypassTable::ENCODED_LEN,
            ),
        ],
    })
}

fn verify_reopen(
    project: &Project,
    layout: lm_project::LevelRomLayout,
    sprite_lengths: &SpriteLengthTable,
    level: u16,
    selectors: LegacyGraphicsBypassSelectors,
    table: &LegacyGraphicsBypassTable,
    checksum_field: usize,
) -> Result<(), LegacyGraphicsBypassWorkspaceError> {
    let reopened = project
        .load_level_slot(usize::from(level), layout, sprite_lengths)
        .map_err(|error| LegacyGraphicsBypassWorkspaceError::Verification(error.to_string()))?;
    let actual = reopened
        .layer1
        .objects
        .legacy_graphics_bypass_selectors(reopened.layer1.header.is_vertical());
    if actual != selectors {
        return Err(LegacyGraphicsBypassWorkspaceError::Verification(format!(
            "selector reopen mismatch: expected {selectors:?}, got {actual:?}"
        )));
    }
    let actual_table = lm_profile::load_smw_us_v1_legacy_graphics_bypass_table(&project.rom)
        .map_err(|error| LegacyGraphicsBypassWorkspaceError::Verification(error.to_string()))?;
    if &actual_table != table {
        return Err(LegacyGraphicsBypassWorkspaceError::Verification(
            "assignment-table reopen mismatch".into(),
        ));
    }
    let stored = lm_rom::SnesChecksum::decode(project.rom.logical_bytes(), checksum_field)
        .map_err(LegacyGraphicsBypassWorkspaceError::Rom)?;
    let computed = compute_snes_checksum(project.rom.logical_bytes(), checksum_field)
        .map_err(LegacyGraphicsBypassWorkspaceError::Rom)?;
    if stored != computed {
        return Err(LegacyGraphicsBypassWorkspaceError::Verification(
            "checksum reopen mismatch".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppState, Command};
    use lm_level::LegacyGraphicsAssignment;
    use lm_rom::CopierHeader;

    #[test]
    fn installed_header_variants_commit_table_selector_reopen_and_one_undo() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut installed = AppState::default();
        installed.load_rom(original).unwrap();
        installed
            .dispatch(Command::InstallSettings { rev: 0 })
            .unwrap();
        let logical = installed.project().unwrap().rom.logical_bytes().to_vec();

        let mut logical_results = Vec::new();
        for copier in [CopierHeader::Absent, CopierHeader::Present] {
            let mut image = RomImage::from_bytes(logical.clone()).unwrap();
            if copier == CopierHeader::Present {
                image
                    .replace_copier_header_exact(None, Some(&[0x5a; 512]))
                    .unwrap();
            }
            let original = image.as_file_bytes().to_vec();
            let mut app = AppState::default();
            app.load_rom(original.clone()).unwrap();
            app.dispatch(Command::SelectLevel(0x105)).unwrap();
            let mut workspace =
                LegacyGraphicsBypassWorkspace::load(&app.controller_snapshot().unwrap()).unwrap();
            workspace.set_selectors(LegacyGraphicsBypassSelectors {
                foreground_background: Some(5),
                sprites: Some(7),
            });
            workspace
                .table_mut()
                .set_entry(5, LegacyGraphicsAssignment([1, 2, 4, 3]))
                .unwrap();
            workspace
                .table_mut()
                .set_entry(7, LegacyGraphicsAssignment([0x12, 0x13, 0x14, 0x15]))
                .unwrap();
            app.dispatch(
                workspace
                    .prepare_commit("Edit legacy standard GFX bypass")
                    .unwrap()
                    .into_command(),
            )
            .unwrap();
            let after = app.project().unwrap().save_snapshot();
            let reopened =
                LegacyGraphicsBypassWorkspace::load(&app.controller_snapshot().unwrap()).unwrap();
            assert_eq!(reopened.selectors(), workspace.selectors());
            assert_eq!(reopened.table(), workspace.table());
            assert_eq!(app.project().unwrap().history.undo_len(), 1);
            app.dispatch(Command::Undo).unwrap();
            assert_eq!(app.project().unwrap().save_snapshot(), original);
            let logical_after = RomImage::from_bytes(after)
                .unwrap()
                .logical_bytes()
                .to_vec();
            if logical_results.is_empty() {
                logical_results = logical_after;
            } else {
                assert_eq!(logical_results, logical_after);
            }
        }
    }
}
