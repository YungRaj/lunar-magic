use crate::{AppError, AppState, EditorMode};

/// Exact dirty-ROM state retained by a native frontend across an abnormal exit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoverySnapshot {
    pub revision: u64,
    pub level: Option<u16>,
    pub saved_baseline: Vec<u8>,
    pub current_rom: Vec<u8>,
}

#[derive(Default)]
pub struct OverworldRecoveryEdits<'a> {
    pub paths: Option<&'a lm_overworld::OverworldPathLinkTable>,
    pub warps: Option<&'a lm_overworld::OverworldWarpLinkTable>,
    pub event_numbers: Option<&'a lm_overworld::EventNumberMap>,
    pub event_reveals: Option<&'a lm_overworld::EventRevealTable>,
    pub special_events: Option<&'a lm_overworld::SpecialEventRevealTable>,
    pub event_tilemaps: Option<&'a lm_overworld::EventTilemapBuffers>,
    pub level_names: Option<&'a lm_overworld::NativeOverworldLevelNameTable>,
    pub player_starts: Option<&'a lm_overworld::NativeOverworldPlayerStarts>,
    pub settings: Option<&'a lm_level::ExpandedOverworldSettings>,
    pub messages: Option<&'a [lm_overworld::OverworldMessage]>,
    pub boss_sequence: Option<&'a lm_overworld::BossSequenceMessageTable>,
}

impl AppState {
    pub fn recovery_snapshot_with_overworld_edits(
        &self,
        edits: OverworldRecoveryEdits<'_>,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let active_families = usize::from(edits.paths.is_some() || edits.warps.is_some())
            + usize::from(
                edits.event_numbers.is_some()
                    || edits.event_reveals.is_some()
                    || edits.special_events.is_some()
                    || edits.event_tilemaps.is_some(),
            )
            + usize::from(
                edits.level_names.is_some()
                    || edits.player_starts.is_some()
                    || edits.settings.is_some(),
            )
            + usize::from(edits.messages.is_some() || edits.boss_sequence.is_some());
        if active_families > 1 {
            return Err(AppError::Recovery(
                "cross-family overworld recovery requires a combined shared-hook runtime".into(),
            ));
        }
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        if let Some(value) = edits.paths {
            crate::overworld_path_link_state::replace_native_path_links_in_project(
                &mut staged,
                value,
            )?;
        }
        if let Some(value) = edits.warps {
            crate::overworld_warp_link_state::replace_native_warp_links_in_project(
                &mut staged,
                value,
            )?;
        }
        if let Some(value) = edits.event_numbers {
            crate::save_native_overworld_event_number_map_to_project(&mut staged, value)?;
        }
        if let Some(value) = edits.event_reveals {
            crate::save_native_overworld_event_reveals_to_project(&mut staged, value)?;
        }
        if let Some(value) = edits.special_events {
            crate::save_native_special_event_reveals_to_project(&mut staged, value)?;
        }
        if let Some(value) = edits.event_tilemaps {
            crate::save_native_overworld_event_tilemaps_to_project(&mut staged, value)?;
        }
        if let Some(value) = edits.level_names {
            crate::save_native_overworld_level_names_to_project(&mut staged, value)?;
        }
        if let Some(value) = edits.player_starts {
            crate::save_native_overworld_player_starts_to_project(&mut staged, value)?;
        }
        if let Some(value) = edits.settings {
            crate::save_native_overworld_settings_to_project(&mut staged, value)?;
        }
        if let Some(value) = edits.messages {
            crate::save_native_overworld_messages_to_project(&mut staged, value)?;
        }
        if let Some(value) = edits.boss_sequence {
            crate::save_native_boss_sequence_to_project(&mut staged, value)?;
        }
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Captures a recovery record only while the open ROM differs from its save baseline.
    #[must_use]
    pub fn recovery_snapshot(&self) -> Option<RecoverySnapshot> {
        let project = self.project.as_ref()?;
        project.is_modified().then(|| RecoverySnapshot {
            revision: self.project_revision,
            level: self.current_level(),
            saved_baseline: project.saved_baseline_snapshot(),
            current_rom: project.save_snapshot(),
        })
    }

    /// Captures recovery state after applying one revision-bound editor mutation to an isolated
    /// clone of the current project.
    ///
    /// Native editor forms use this for changes that are intentionally staged outside
    /// [`AppState`]. The live project, its history, and its save baseline remain untouched.
    ///
    /// # Errors
    ///
    /// Returns an error when no project is open or the staged mutation is incompatible with the
    /// current ROM snapshot.
    pub fn recovery_snapshot_with_mutation(
        &self,
        mutation: &lm_project::RomMutation,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        staged
            .apply_mutation("Stage editor state for crash recovery", mutation)
            .map_err(|error| AppError::Recovery(error.to_string()))?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Validates and captures an editor-composed physical ROM without publishing it to the live
    /// application project.
    pub fn recovery_snapshot_with_current_rom(
        &self,
        current_rom: Vec<u8>,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let saved_baseline = project.saved_baseline_snapshot();
        let staged =
            lm_project::Project::open_recovered(saved_baseline.clone(), current_rom.clone())
                .map_err(|error| AppError::Recovery(error.to_string()))?;
        Ok(staged.is_modified().then(|| RecoverySnapshot {
            revision: self.project_revision,
            level,
            saved_baseline,
            current_rom,
        }))
    }

    /// Composes an optional staged ROM mutation and staged native overworld route links on one
    /// isolated project before validating the recovery image.
    pub fn recovery_snapshot_with_overworld_path_links(
        &self,
        mutation: Option<&lm_project::RomMutation>,
        links: &lm_overworld::OverworldPathLinkTable,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        if let Some(mutation) = mutation {
            staged
                .apply_mutation("Stage overworld terrain for crash recovery", mutation)
                .map_err(|error| AppError::Recovery(error.to_string()))?;
        }
        crate::overworld_path_link_state::replace_native_path_links_in_project(&mut staged, links)?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Applies a complete staged overworld warp-link table to an isolated project and validates the
    /// resulting recovery image without publishing it to the live project.
    pub fn recovery_snapshot_with_overworld_warp_links(
        &self,
        links: &lm_overworld::OverworldWarpLinkTable,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        crate::overworld_warp_link_state::replace_native_warp_links_in_project(&mut staged, links)?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Installs staged path and warp tables sequentially into one isolated project. Sequential
    /// allocation is required because both pristine tables may install relocatable runtimes.
    pub fn recovery_snapshot_with_overworld_navigation_links(
        &self,
        paths: &lm_overworld::OverworldPathLinkTable,
        warps: &lm_overworld::OverworldWarpLinkTable,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        crate::overworld_path_link_state::replace_native_path_links_in_project(&mut staged, paths)?;
        crate::overworld_warp_link_state::replace_native_warp_links_in_project(&mut staged, warps)?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Applies playable main-map terrain, its staged path table, and an independently staged warp
    /// table to one evolving clone. Terrain is a revision-bound mutation; both link installers then
    /// allocate against its resulting image in native persistence order.
    pub fn recovery_snapshot_with_overworld_terrain_navigation(
        &self,
        terrain: Option<&lm_project::RomMutation>,
        paths: Option<&lm_overworld::OverworldPathLinkTable>,
        warps: &lm_overworld::OverworldWarpLinkTable,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        if let Some(mutation) = terrain {
            staged
                .apply_mutation("Stage overworld terrain for crash recovery", mutation)
                .map_err(|error| AppError::Recovery(error.to_string()))?;
        }
        if let Some(value) = paths {
            crate::overworld_path_link_state::replace_native_path_links_in_project(
                &mut staged,
                value,
            )?;
        }
        crate::overworld_warp_link_state::replace_native_warp_links_in_project(&mut staged, warps)?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Persists all four staged overworld-event domains sequentially into one isolated project so
    /// their pristine installers allocate against a shared evolving ROM image.
    pub fn recovery_snapshot_with_overworld_event_family(
        &self,
        numbers: &lm_overworld::EventNumberMap,
        reveals: &lm_overworld::EventRevealTable,
        special: &lm_overworld::SpecialEventRevealTable,
        tilemaps: &lm_overworld::EventTilemapBuffers,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        crate::save_native_overworld_event_number_map_to_project(&mut staged, numbers)?;
        crate::save_native_overworld_event_reveals_to_project(&mut staged, reveals)?;
        crate::save_native_special_event_reveals_to_project(&mut staged, special)?;
        crate::save_native_overworld_event_tilemaps_to_project(&mut staged, tilemaps)?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Persists the staged overworld names, player starts, and seven-map settings into one
    /// isolated project. The ordering lets relocatable pristine installers share one ROM image.
    pub fn recovery_snapshot_with_overworld_configuration(
        &self,
        names: &lm_overworld::NativeOverworldLevelNameTable,
        starts: &lm_overworld::NativeOverworldPlayerStarts,
        settings: &lm_level::ExpandedOverworldSettings,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        crate::save_native_overworld_level_names_to_project(&mut staged, names)?;
        crate::save_native_overworld_player_starts_to_project(&mut staged, starts)?;
        crate::save_native_overworld_settings_to_project(&mut staged, settings)?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Persists ordinary overworld messages and the boss-sequence message table sequentially into
    /// one isolated project so both staged editors share the same evolving allocation image.
    pub fn recovery_snapshot_with_overworld_message_family(
        &self,
        messages: &[lm_overworld::OverworldMessage],
        boss_sequence: &lm_overworld::BossSequenceMessageTable,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        crate::save_native_overworld_messages_to_project(&mut staged, messages)?;
        crate::save_native_boss_sequence_to_project(&mut staged, boss_sequence)?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Persists both complete staged title/credits tilemaps sequentially into one isolated ROM.
    pub fn recovery_snapshot_with_global_tilemaps(
        &self,
        title: &lm_overworld::ExpandedLayerTilemap,
        credits: &lm_overworld::CreditsTilemap,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        crate::save_native_title_tilemap_to_project(&mut staged, title)?;
        crate::save_native_credits_tilemap_to_project(&mut staged, credits)?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Applies an installed-ROM palette mutation and a staged shared-palette file to one isolated
    /// project, preserving allocation order and validating the resulting recovery image.
    pub fn recovery_snapshot_with_palette_family(
        &self,
        mutation: &lm_project::RomMutation,
        shared: &lm_graphics::SmwPaletteFile,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        staged
            .apply_mutation("Stage installed palette for crash recovery", mutation)
            .map_err(|error| AppError::Recovery(error.to_string()))?;
        crate::save_native_shared_palette_to_project(&mut staged, shared)?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Composes two non-growing mutations prepared from the same ROM revision. Differing
    /// overlapping writes fail closed, while the shared checksum field is recomputed after both.
    pub fn recovery_snapshot_with_graphics_family(
        &self,
        graphics: &lm_project::RomMutation,
        exanimation: &lm_project::RomMutation,
        level: Option<u16>,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        const CHECKSUM_FIELD: usize = 0x7fdc;
        if !graphics.appended.is_empty() || !exanimation.appended.is_empty() {
            return Err(AppError::Recovery(
                "simultaneous graphics recovery cannot yet rebase growing allocations".into(),
            ));
        }
        if graphics.expected_len != exanimation.expected_len
            || graphics.mapper != exanimation.mapper
        {
            return Err(AppError::Recovery(
                "simultaneous graphics mutations do not share one ROM baseline".into(),
            ));
        }
        reject_conflicting_mutation_writes(graphics, exanimation, CHECKSUM_FIELD)?;
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        staged
            .apply_mutation("Stage graphics for crash recovery", graphics)
            .map_err(|error| AppError::Recovery(error.to_string()))?;
        staged
            .apply_mutation("Stage ExAnimation for crash recovery", exanimation)
            .map_err(|error| AppError::Recovery(error.to_string()))?;
        staged.rom.update_snes_checksum(CHECKSUM_FIELD)?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), level)
    }

    /// Composes primary-level and aggregate asset-only mutations prepared from one exact ROM.
    /// Growth is rejected because independently planned allocations cannot be safely rebased.
    pub fn recovery_snapshot_with_level_and_assets(
        &self,
        level: &lm_project::RomMutation,
        assets: &lm_project::RomMutation,
        selected_level: u16,
    ) -> Result<Option<RecoverySnapshot>, AppError> {
        const CHECKSUM_FIELD: usize = 0x7fdc;
        if !level.appended.is_empty() || !assets.appended.is_empty() {
            return Err(AppError::Recovery(
                "simultaneous level/assets recovery cannot rebase growing allocations".into(),
            ));
        }
        if level.expected_len != assets.expected_len || level.mapper != assets.mapper {
            return Err(AppError::Recovery(
                "simultaneous level/assets mutations do not share one ROM baseline".into(),
            ));
        }
        reject_conflicting_mutation_writes(level, assets, CHECKSUM_FIELD)?;
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let mut staged = project.clone();
        staged
            .apply_mutation("Stage primary level for crash recovery", level)
            .map_err(|error| AppError::Recovery(error.to_string()))?;
        staged
            .apply_mutation("Stage aggregate level assets for crash recovery", assets)
            .map_err(|error| AppError::Recovery(error.to_string()))?;
        staged.rom.update_snes_checksum(CHECKSUM_FIELD)?;
        self.recovery_snapshot_with_current_rom(staged.save_snapshot(), Some(selected_level))
    }

    /// Restores a recovery record as an unnamed, dirty project.
    ///
    /// The original path is deliberately not restored. The first explicit save therefore uses
    /// Save As and cannot silently replace the source ROM.
    ///
    /// # Errors
    ///
    /// Returns an error when a project is already open, either image is malformed, the current
    /// ROM is unsupported, or the record contains no unsaved change.
    pub fn load_recovery(&mut self, snapshot: RecoverySnapshot) -> Result<(), AppError> {
        if self.project.is_some() {
            return Err(AppError::ProjectAlreadyOpen);
        }
        if snapshot.level.is_some_and(|level| level > 0x1ff) {
            return Err(AppError::Recovery(
                "recovery record contains an invalid level".into(),
            ));
        }
        let project =
            lm_project::Project::open_recovered(snapshot.saved_baseline, snapshot.current_rom)
                .map_err(|error| AppError::Recovery(error.to_string()))?;
        if !project.is_modified() {
            return Err(AppError::Recovery(
                "recovery record does not contain unsaved changes".into(),
            ));
        }
        self.install_project(project, None);
        if let Some(level) = snapshot.level {
            self.mode = EditorMode::Level(level);
            self.level_navigation.visit(level);
        }
        self.status = "Recovered unsaved ROM; use Save As to preserve it".into();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::{OverworldPathLink, OverworldWarpLink};
    use lm_profile::{
        smw_us_v1_overworld_path_patch_locator, smw_us_v1_overworld_warp_patch_locator,
    };

    #[test]
    fn cross_family_overworld_recovery_rejects_before_shared_hook_mutation() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let project = app.project().unwrap();
        let paths = project
            .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
            .unwrap()
            .table;
        let numbers = project
            .load_overworld_event_number_map_detected(
                lm_profile::smw_us_v1_overworld_event_number_map_locator(),
            )
            .unwrap()
            .map;
        let baseline = project.save_snapshot();
        let error = app
            .recovery_snapshot_with_overworld_edits(
                OverworldRecoveryEdits {
                    paths: Some(&paths),
                    event_numbers: Some(&numbers),
                    ..OverworldRecoveryEdits::default()
                },
                Some(0x105),
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("combined shared-hook runtime"), "{error}");
        assert_eq!(app.project().unwrap().save_snapshot(), baseline);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
    }

    #[test]
    fn level_and_asset_mutations_compose_disjoint_writes_and_reject_growth() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let baseline = app.project().unwrap().rom.logical_bytes().to_vec();
        let mut level_bytes = baseline.clone();
        level_bytes[0x1000] ^= 0x11;
        let mut asset_bytes = baseline.clone();
        asset_bytes[0x2000] ^= 0x22;
        let level =
            lm_project::RomMutation::between(lm_rom::Mapper::LoRom, &baseline, &level_bytes)
                .unwrap();
        let assets =
            lm_project::RomMutation::between(lm_rom::Mapper::LoRom, &baseline, &asset_bytes)
                .unwrap();
        let recovery = app
            .recovery_snapshot_with_level_and_assets(&level, &assets, 0x105)
            .unwrap()
            .unwrap();
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        assert_eq!(
            reopened.project().unwrap().rom.logical_bytes()[0x1000],
            level_bytes[0x1000]
        );
        assert_eq!(
            reopened.project().unwrap().rom.logical_bytes()[0x2000],
            asset_bytes[0x2000]
        );

        let mut grown = baseline.clone();
        grown.extend_from_slice(&[0xff; 16]);
        let growth =
            lm_project::RomMutation::between(lm_rom::Mapper::LoRom, &baseline, &grown).unwrap();
        assert!(
            app.recovery_snapshot_with_level_and_assets(&level, &growth, 0x105)
                .unwrap_err()
                .to_string()
                .contains("cannot rebase growing allocations")
        );
    }

    #[test]
    #[ignore = "requires a combined shared-hook runtime for cross-family overworld recovery"]
    fn cross_family_overworld_recovery_reopens_navigation_event_configuration_and_messages() {
        use lm_profile::{
            load_smw_us_v1_overworld_messages, smw_us_v1_overworld_event_number_map_locator,
            smw_us_v1_overworld_level_name_locator, smw_us_v1_overworld_level_name_runtime,
            smw_us_v1_overworld_message_patch_locator,
        };
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let baseline = app.project().unwrap().save_snapshot();
        let project = app.project().unwrap();
        let mut paths = project
            .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
            .unwrap()
            .table;
        paths.links[0].target.x_tile ^= 0x01;
        let mut numbers =
            project
                .load_overworld_event_number_map_detected(
                    smw_us_v1_overworld_event_number_map_locator(),
                )
                .unwrap()
                .map;
        numbers.set(0xff, 0x7e);
        let mut names = project
            .load_overworld_level_names_detected(
                smw_us_v1_overworld_level_name_locator(),
                smw_us_v1_overworld_level_name_runtime(),
            )
            .unwrap()
            .table;
        names.names[0].tiles[0] ^= 0x3f;
        let mut messages = load_smw_us_v1_overworld_messages(project).unwrap().messages;
        messages[0].0[0] ^= 0x01;

        let recovery = app
            .recovery_snapshot_with_overworld_edits(
                OverworldRecoveryEdits {
                    paths: Some(&paths),
                    event_numbers: Some(&numbers),
                    level_names: Some(&names),
                    messages: Some(&messages),
                    ..OverworldRecoveryEdits::default()
                },
                Some(0x105),
            )
            .unwrap()
            .unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), baseline);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let project = reopened.project().unwrap();
        assert_eq!(
            project
                .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
                .unwrap()
                .table,
            paths
        );
        assert_eq!(
            project
                .load_overworld_event_number_map_detected(
                    smw_us_v1_overworld_event_number_map_locator()
                )
                .unwrap()
                .map,
            numbers
        );
        assert_eq!(
            project
                .load_overworld_level_names_detected(
                    smw_us_v1_overworld_level_name_locator(),
                    smw_us_v1_overworld_level_name_runtime()
                )
                .unwrap()
                .table,
            names
        );
        assert_eq!(
            project
                .load_expanded_overworld_messages_detected(
                    smw_us_v1_overworld_message_patch_locator()
                )
                .unwrap()
                .messages,
            messages
        );
        let logical = project.rom.logical_bytes();
        assert_eq!(
            lm_rom::SnesChecksum::decode(logical, 0x7fdc).unwrap(),
            lm_rom::compute_snes_checksum(logical, 0x7fdc).unwrap()
        );
    }

    #[test]
    fn simultaneous_pristine_path_and_warp_growth_allocate_and_recover_together() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let project = app.project().unwrap();
        let mut paths = project
            .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
            .unwrap()
            .table;
        let mut path_tail: OverworldPathLink = *paths.links.last().unwrap();
        path_tail.target.x_tile ^= 0x5a;
        paths.links.push(path_tail);
        let mut warps = project
            .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
            .unwrap()
            .table;
        let mut warp_tail: OverworldWarpLink = *warps.links.last().unwrap();
        warp_tail.destination.horizontal_tile ^= 0x1234;
        warps.links.push(warp_tail);

        let recovery = app
            .recovery_snapshot_with_overworld_navigation_links(&paths, &warps, Some(0x105))
            .unwrap()
            .unwrap();
        assert_eq!(app.capabilities().project, crate::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        assert_eq!(reopened.current_level(), Some(0x105));
        let project = reopened.project().unwrap();
        assert_eq!(
            project
                .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
                .unwrap()
                .table,
            paths
        );
        assert_eq!(
            project
                .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
                .unwrap()
                .table,
            warps
        );
    }

    #[test]
    fn terrain_mutation_path_and_warp_tables_recover_on_one_evolving_project() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let baseline = app.project().unwrap().save_snapshot();
        let project = app.project().unwrap();
        let mut paths = project
            .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
            .unwrap()
            .table;
        paths.links[0].target.x_tile ^= 1;
        let mut warps = project
            .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
            .unwrap()
            .table;
        warps.links[0].destination.horizontal_tile ^= 1;
        let offset = 0x1000;
        let mut evolved = project.rom.logical_bytes().to_vec();
        evolved[offset] ^= 0x5a;
        let terrain = lm_project::RomMutation::between(
            lm_rom::Mapper::LoRom,
            project.rom.logical_bytes(),
            &evolved,
        )
        .unwrap();

        let recovery = app
            .recovery_snapshot_with_overworld_terrain_navigation(
                Some(&terrain),
                Some(&paths),
                &warps,
                Some(0x105),
            )
            .unwrap()
            .unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), baseline);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let project = reopened.project().unwrap();
        assert_eq!(project.rom.logical_bytes()[offset], evolved[offset]);
        assert_eq!(
            project
                .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
                .unwrap()
                .table,
            paths
        );
        assert_eq!(
            project
                .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
                .unwrap()
                .table,
            warps
        );
    }

    #[test]
    fn simultaneous_pristine_event_family_installs_and_recovers_every_domain() {
        use lm_overworld::{EventReveal, EventTilemapBuffers};
        use lm_profile::{
            load_smw_us_v1_event_tilemaps, smw_us_v1_overworld_event_number_map_locator,
            smw_us_v1_overworld_event_reveal_locator, smw_us_v1_special_event_reveal_locator,
        };

        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let project = app.project().unwrap();
        let mut numbers =
            project
                .load_overworld_event_number_map_detected(
                    smw_us_v1_overworld_event_number_map_locator(),
                )
                .unwrap()
                .map;
        numbers.set(0xff, 0x7e);
        let mut reveals = project
            .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())
            .unwrap()
            .table;
        reveals.entries.push(EventReveal {
            source_tile: 0x321,
            destination_tile: 0x654,
        });
        let mut special = project
            .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())
            .unwrap()
            .table;
        special.reveals[23] = EventReveal {
            source_tile: 0x456,
            destination_tile: 0x765,
        };
        special.directions[23] = 3;
        let mut tilemaps = EventTilemapBuffers::default();
        tilemaps.primary_bytes_mut()[0xfff] = 0xa5;
        tilemaps.secondary_high_bytes_mut()[0x7ff] = 0x5a;

        let recovery = app
            .recovery_snapshot_with_overworld_event_family(
                &numbers,
                &reveals,
                &special,
                &tilemaps,
                Some(0x105),
            )
            .unwrap()
            .unwrap();
        assert_eq!(app.capabilities().project, crate::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let project = reopened.project().unwrap();
        assert_eq!(
            project
                .load_overworld_event_number_map_detected(
                    smw_us_v1_overworld_event_number_map_locator(),
                )
                .unwrap()
                .map,
            numbers
        );
        assert_eq!(
            project
                .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator(),)
                .unwrap()
                .table,
            reveals
        );
        assert_eq!(
            project
                .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())
                .unwrap()
                .table,
            special
        );
        assert_eq!(
            load_smw_us_v1_event_tilemaps(project).unwrap().buffers,
            tilemaps
        );
        assert_eq!(reveals.entries.len(), 113);
    }

    #[test]
    fn simultaneous_pristine_overworld_configuration_installs_and_recovers_every_domain() {
        use lm_profile::{
            load_smw_us_v1_overworld_settings, smw_us_v1_overworld_level_name_locator,
            smw_us_v1_overworld_level_name_runtime, smw_us_v1_overworld_player_start_layout,
        };

        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let project = app.project().unwrap();
        let mut names = project
            .load_overworld_level_names_detected(
                smw_us_v1_overworld_level_name_locator(),
                smw_us_v1_overworld_level_name_runtime(),
            )
            .unwrap()
            .table;
        names.names[0].tiles[0] ^= 0x3f;
        let mut starts = project
            .load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())
            .unwrap();
        starts.starts[0].x ^= 0x20;
        let mut settings = load_smw_us_v1_overworld_settings(project).unwrap().settings;
        settings.records[6].set_word(11, 0x4567).unwrap();

        let recovery = app
            .recovery_snapshot_with_overworld_configuration(&names, &starts, &settings, Some(0x105))
            .unwrap()
            .unwrap();
        assert_eq!(app.capabilities().project, crate::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        assert_eq!(reopened.current_level(), Some(0x105));
        let project = reopened.project().unwrap();
        assert_eq!(
            project
                .load_overworld_level_names_detected(
                    smw_us_v1_overworld_level_name_locator(),
                    smw_us_v1_overworld_level_name_runtime(),
                )
                .unwrap()
                .table,
            names
        );
        assert_eq!(
            project
                .load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())
                .unwrap(),
            starts
        );
        assert_eq!(
            load_smw_us_v1_overworld_settings(project).unwrap().settings,
            settings
        );
    }

    #[test]
    fn simultaneous_pristine_message_family_installs_and_recovers_both_tables() {
        use lm_profile::{
            load_smw_us_v1_overworld_messages, smw_us_v1_boss_sequence_locator,
            smw_us_v1_overworld_message_patch_locator,
        };

        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let project = app.project().unwrap();
        let mut messages = load_smw_us_v1_overworld_messages(project).unwrap().messages;
        messages[0].0[0] ^= 0x01;
        let mut boss_sequence = project
            .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())
            .unwrap()
            .table;
        boss_sequence.messages[0].0[0] ^= 0x01;

        let recovery = app
            .recovery_snapshot_with_overworld_message_family(&messages, &boss_sequence, Some(0x105))
            .unwrap()
            .unwrap();
        assert_eq!(app.capabilities().project, crate::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        assert_eq!(reopened.current_level(), Some(0x105));
        let project = reopened.project().unwrap();
        assert_eq!(
            project
                .load_expanded_overworld_messages_detected(
                    smw_us_v1_overworld_message_patch_locator(),
                )
                .unwrap()
                .messages,
            messages
        );
        assert_eq!(
            project
                .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())
                .unwrap()
                .table,
            boss_sequence
        );
    }

    #[test]
    fn simultaneous_pristine_global_tilemaps_install_and_recover_exactly() {
        use lm_profile::{smw_us_v1_credits_tilemap_locator, smw_us_v1_title_tilemap_locator};

        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let project = app.project().unwrap();
        let mut title = project
            .load_title_tilemap_detected(smw_us_v1_title_tilemap_locator())
            .unwrap()
            .tilemap;
        title.primary_bytes_mut()[0] ^= 0x01;
        title.secondary_bytes_mut()[0] ^= 0x80;
        let credits_locator = smw_us_v1_credits_tilemap_locator();
        let mut credits = project
            .load_credits_tilemap_detected(&credits_locator)
            .unwrap()
            .tilemap;
        let last = credits.words().len() - 1;
        credits.words_mut()[last] ^= 0x0001;

        let recovery = app
            .recovery_snapshot_with_global_tilemaps(&title, &credits, Some(0x105))
            .unwrap()
            .unwrap();
        assert_eq!(app.capabilities().project, crate::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        assert_eq!(reopened.current_level(), Some(0x105));
        let project = reopened.project().unwrap();
        assert_eq!(
            project
                .load_title_tilemap_detected(smw_us_v1_title_tilemap_locator())
                .unwrap()
                .tilemap,
            title
        );
        assert_eq!(
            project
                .load_credits_tilemap_detected(&credits_locator)
                .unwrap()
                .tilemap,
            credits
        );
    }

    #[test]
    fn simultaneous_pristine_palette_family_recovers_mutation_and_expanded_shared_palette() {
        use lm_graphics::SmwPaletteFile;
        use lm_profile::smw_us_v1_shared_palette_layout_for_mapper;
        use lm_rom::Mapper;

        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let project = app.project().unwrap();
        let before = project.rom.logical_bytes().to_vec();
        let mut after = before.clone();
        after[0x10000] ^= 0x01;
        let mutation = lm_project::RomMutation::between(Mapper::LoRom, &before, &after).unwrap();
        let layout = smw_us_v1_shared_palette_layout_for_mapper(Mapper::LoRom);
        let expected = project
            .rom
            .read(layout.table_offset, SmwPaletteFile::EXPANDED_FILE_LEN)
            .unwrap();
        let mut palette =
            SmwPaletteFile::expanded(expected[0x10..].to_vec(), expected[..0x10].to_vec()).unwrap();
        let mut encoded = palette.encode();
        encoded[0x234] ^= 0x01;
        palette = SmwPaletteFile::decode(&encoded).unwrap();

        let recovery = app
            .recovery_snapshot_with_palette_family(&mutation, &palette, Some(0x105))
            .unwrap()
            .unwrap();
        assert_eq!(app.capabilities().project, crate::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        assert_eq!(reopened.current_level(), Some(0x105));
        assert_eq!(
            reopened.project().unwrap().rom.logical_bytes()[0x10000],
            after[0x10000]
        );
        assert_eq!(
            reopened
                .project()
                .unwrap()
                .load_shared_palette(layout)
                .unwrap(),
            palette
        );
    }

    #[test]
    fn same_baseline_graphics_mutations_compose_and_repair_the_final_checksum() {
        use lm_rom::{Mapper, detect_identity};

        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let before = app.project().unwrap().rom.logical_bytes().to_vec();
        let mut graphics_image = before.clone();
        graphics_image[0x10000] ^= 0x01;
        let graphics =
            lm_project::RomMutation::between(Mapper::LoRom, &before, &graphics_image).unwrap();
        let mut exanimation_image = before.clone();
        exanimation_image[0x10010] ^= 0x02;
        let exanimation =
            lm_project::RomMutation::between(Mapper::LoRom, &before, &exanimation_image).unwrap();

        let recovery = app
            .recovery_snapshot_with_graphics_family(&graphics, &exanimation, Some(0x105))
            .unwrap()
            .unwrap();
        let image = lm_rom::RomImage::from_bytes(recovery.current_rom.clone()).unwrap();
        assert_eq!(image.logical_bytes()[0x10000], graphics_image[0x10000]);
        assert_eq!(image.logical_bytes()[0x10010], exanimation_image[0x10010]);
        assert!(detect_identity(&image).unwrap().checksum_matches());
        assert_eq!(app.capabilities().project, crate::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
    }

    #[test]
    fn graphics_mutation_composition_rejects_growth_and_conflicts() {
        use lm_project::{RomMutation, RomWrite};
        use lm_rom::Mapper;

        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let len = app.project().unwrap().rom.logical_len();
        let first = RomMutation {
            mapper: Mapper::LoRom,
            expected_len: len,
            appended: Vec::new(),
            writes: vec![RomWrite {
                offset: 0x10000,
                bytes: vec![1],
            }],
        };
        let mut conflict = first.clone();
        conflict.writes[0].bytes[0] = 2;
        assert!(
            app.recovery_snapshot_with_graphics_family(&first, &conflict, None)
                .is_err()
        );
        let mut growth = first.clone();
        growth.appended.push(0xff);
        assert!(
            app.recovery_snapshot_with_graphics_family(&growth, &first, None)
                .is_err()
        );
    }
}

fn reject_conflicting_mutation_writes(
    first: &lm_project::RomMutation,
    second: &lm_project::RomMutation,
    checksum_field: usize,
) -> Result<(), AppError> {
    let checksum_end = checksum_field + 4;
    for left in &first.writes {
        for right in &second.writes {
            let left_end = left.offset.checked_add(left.bytes.len()).ok_or_else(|| {
                AppError::Recovery("first graphics mutation write range overflowed".into())
            })?;
            let right_end = right.offset.checked_add(right.bytes.len()).ok_or_else(|| {
                AppError::Recovery("second graphics mutation write range overflowed".into())
            })?;
            let start = left.offset.max(right.offset);
            let end = left_end.min(right_end);
            for offset in start..end {
                if (checksum_field..checksum_end).contains(&offset) {
                    continue;
                }
                if left.bytes[offset - left.offset] != right.bytes[offset - right.offset] {
                    return Err(AppError::Recovery(format!(
                        "simultaneous graphics mutations conflict at logical ROM offset {offset:06X}"
                    )));
                }
            }
        }
    }
    Ok(())
}
