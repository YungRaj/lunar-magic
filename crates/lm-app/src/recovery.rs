use crate::{AppError, AppState, EditorMode};

/// Exact dirty-ROM state retained by a native frontend across an abnormal exit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoverySnapshot {
    pub revision: u64,
    pub level: Option<u16>,
    pub saved_baseline: Vec<u8>,
    pub current_rom: Vec<u8>,
}

impl AppState {
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
}
