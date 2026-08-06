use crate::{AppError, AppState, FrontendEffect};
use lm_project::{GraphicsCompression, GraphicsMigrationOptions, GraphicsRomLayout};

impl AppState {
    /// Recompresses the complete native graphics table through the project transaction boundary.
    ///
    /// The revision token prevents a frontend request decoded from an old snapshot from changing a
    /// newer project. A successful migration advances the application revision exactly once and is
    /// exposed as one undoable change; an equal-codec request is a semantic no-op.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] for a missing project, stale revision, revision overflow, or any source
    /// decode, allocation, checksum, or semantic-reopen failure. Application and project state are
    /// unchanged on failure.
    pub(crate) fn migrate_graphics_compression(
        &mut self,
        expected_revision: u64,
        source: GraphicsRomLayout,
        target: GraphicsCompression,
        options: &GraphicsMigrationOptions,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.project.as_ref().ok_or(AppError::NoProject)?;
        if self
            .revision_profile
            .as_ref()
            .is_none_or(|profile| profile.graphics != source)
        {
            return Err(AppError::GraphicsMigrationProfileMismatch);
        }
        if source.compression == target {
            return Ok(Vec::new());
        }
        self.ensure_project_revision_capacity()?;
        let installed_smw = lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(
            &self.project.as_ref().ok_or(AppError::NoProject)?.rom,
        );
        let changed = if installed_smw {
            if !matches!(
                (source.compression, target),
                (GraphicsCompression::Lz2, GraphicsCompression::Lz3)
                    | (GraphicsCompression::Lz3, GraphicsCompression::Lz2)
            ) {
                return Err(AppError::GraphicsRuntimeMigrationRequired);
            }
            let project = self.project.as_mut().ok_or(AppError::NoProject)?;
            let current_len = project.rom.logical_len();
            let target_len = match current_len {
                0..0x20_0000 => 0x20_0000,
                0x20_0000..0x40_0000 => 0x40_0000,
                _ => return Err(AppError::GraphicsRuntimeMigrationRequired),
            };
            let allocation = lm_rats::AllocationPolicy::lorom(current_len..target_len);
            let plan = match target {
                GraphicsCompression::Lz2 => lm_profile::smw_us_v1_lz2_original_installation_plan(
                    &project.rom,
                    allocation,
                    options.checksum_field,
                )?,
                GraphicsCompression::Lz3 => lm_profile::smw_us_v1_lz3_installation_plan(
                    &project.rom,
                    allocation,
                    options.checksum_field,
                )?,
            };
            project.install_relocatable_patch_with_kind(
                &plan,
                lm_project::EditKind::GraphicsCompressionMigration {
                    source: source.compression,
                    target,
                },
            )?;
            true
        } else {
            self.project
                .as_mut()
                .ok_or(AppError::NoProject)?
                .migrate_graphics_compression(source, target, options)?
        };
        debug_assert!(changed);
        self.revision_profile
            .as_mut()
            .expect("profile validated before migration")
            .graphics
            .compression = target;
        self.advance_project_revision()?;
        let description = format!(
            "Change graphics compression from {} to {}",
            compression_name(source.compression),
            compression_name(target)
        );
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }
}

const fn compression_name(compression: GraphicsCompression) -> &'static str {
    match compression {
        GraphicsCompression::Lz2 => "LZ2",
        GraphicsCompression::Lz3 => "LZ3",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, EditorMode, GraphicsController};
    use lm_graphics::{GraphicsFile4bpp, GraphicsOwnership, IndexedTile};
    use lm_project::{GraphicsSaveOptions, LevelPointerTable, Project};
    use lm_rats::{AllocationPolicy, ProtectedRange};
    use lm_rom::{Mapper, RomImage, SnesChecksum, compute_snes_checksum};
    use std::fs;

    fn layout(compression: GraphicsCompression) -> GraphicsRomLayout {
        GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x200,
                entries: 2,
                stride: 3,
            },
            split_pointer_planes: None,
            compression,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x10000,
        }
    }

    fn allocation(search: std::ops::Range<usize>) -> AllocationPolicy {
        AllocationPolicy {
            search,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(0x200..0x206), ProtectedRange(0x7fdc..0x7fe0)],
        }
    }

    fn fixture() -> Vec<u8> {
        let source = layout(GraphicsCompression::Lz2);
        let mut bytes = vec![0xff; 0x10000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes[0x7fd9] = 1;
        bytes[0x7fdb] = 0;
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        for (slot, pixel) in [1, 9].into_iter().enumerate() {
            project
                .save_graphics_file(
                    slot,
                    &GraphicsFile4bpp {
                        tiles: vec![IndexedTile::new([pixel; 64])],
                    },
                    source,
                    &GraphicsSaveOptions {
                        allocation: allocation(0x1000..0x5000),
                        previous_block: None,
                        reuse_identical: true,
                        erase_fill: 0xff,
                    },
                )
                .unwrap();
        }
        project.refresh_checksum(0x7fdc).unwrap();
        project.save_snapshot()
    }

    fn command(expected_revision: u64) -> Command {
        Command::MigrateGraphicsCompression {
            expected_revision,
            source: layout(GraphicsCompression::Lz2),
            target: GraphicsCompression::Lz3,
            options: GraphicsMigrationOptions {
                allocation: allocation(0x8000..0x10000),
                reuse_identical: true,
                erase_fill: 0xff,
                checksum_field: 0x7fdc,
            },
        }
    }

    fn install_test_profile(app: &mut AppState) {
        let mut profile = lm_profile::test_support::profile();
        profile.graphics = layout(GraphicsCompression::Lz2);
        app.revision_profile = Some(profile);
    }

    #[test]
    fn application_migration_is_one_revision_and_one_undo_step() {
        let mut app = AppState::default();
        let original = fixture();
        app.load_rom(original.clone()).unwrap();
        install_test_profile(&mut app);
        app.dispatch(Command::ShowGraphics(0)).unwrap();

        assert_eq!(
            app.dispatch(command(0)).unwrap(),
            [FrontendEffect::ProjectChanged {
                description: "Change graphics compression from LZ2 to LZ3".into(),
                mode: EditorMode::Graphics(0),
                revision: 1,
            }]
        );
        assert_eq!(
            app.revision_profile().unwrap().graphics.compression,
            GraphicsCompression::Lz3
        );
        let snapshot = app.controller_snapshot().unwrap();
        assert_eq!(
            GraphicsController::decode(
                &snapshot,
                app.revision_profile().unwrap().graphics,
                GraphicsOwnership::editable(1),
            )
            .unwrap()
            .graphics()
            .tiles[0]
                .pixels()[0],
            1
        );
        let target = layout(GraphicsCompression::Lz3);
        for slot in 0..2 {
            assert_eq!(
                app.project()
                    .unwrap()
                    .load_graphics_file(slot, target)
                    .unwrap()
                    .tiles[0]
                    .pixels()[0],
                [1, 9][slot]
            );
        }
        let logical = app.project().unwrap().rom.logical_bytes();
        assert_eq!(
            SnesChecksum::decode(logical, 0x7fdc).unwrap(),
            compute_snes_checksum(logical, 0x7fdc).unwrap()
        );

        app.dispatch(Command::CommitRomWrites {
            expected_revision: 1,
            description: "Independent edit".into(),
            writes: vec![lm_project::RomWrite {
                offset: 0x40,
                bytes: vec![0x5a],
            }],
        })
        .unwrap();
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.revision_profile().unwrap().graphics.compression,
            GraphicsCompression::Lz3
        );
        // A profile replacement between history steps must not turn restoration into a toggle.
        app.revision_profile.as_mut().unwrap().graphics.compression = GraphicsCompression::Lz2;
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.as_file_bytes(), original);
        assert_eq!(
            app.revision_profile().unwrap().graphics.compression,
            GraphicsCompression::Lz2
        );
        let snapshot = app.controller_snapshot().unwrap();
        GraphicsController::decode(
            &snapshot,
            app.revision_profile().unwrap().graphics,
            GraphicsOwnership::editable(1),
        )
        .unwrap();
        assert!(!app.project().unwrap().history.can_undo());
        app.revision_profile.as_mut().unwrap().graphics.compression = GraphicsCompression::Lz3;
        app.dispatch(Command::Redo).unwrap();
        assert_eq!(
            app.revision_profile().unwrap().graphics.compression,
            GraphicsCompression::Lz3
        );
        for slot in 0..2 {
            app.project()
                .unwrap()
                .load_graphics_file(slot, target)
                .unwrap();
        }
        app.dispatch(Command::Redo).unwrap();
        assert_eq!(app.project().unwrap().rom.logical_bytes()[0x40], 0x5a);
    }

    #[test]
    fn stale_noop_and_late_failure_preserve_application_state() {
        let mut app = AppState::default();
        app.load_rom(fixture()).unwrap();
        let before = app.project().unwrap().rom.as_file_bytes().to_vec();

        assert!(matches!(
            app.dispatch(command(0)),
            Err(AppError::GraphicsMigrationProfileMismatch)
        ));
        install_test_profile(&mut app);

        assert!(matches!(
            app.dispatch(command(1)),
            Err(AppError::StaleProjectRevision { .. })
        ));
        assert_eq!(app.project_revision(), 0);
        assert_eq!(app.project().unwrap().rom.as_file_bytes(), before);

        let mut no_op = command(0);
        let Command::MigrateGraphicsCompression { target, .. } = &mut no_op else {
            unreachable!();
        };
        *target = GraphicsCompression::Lz2;
        assert!(app.dispatch(no_op).unwrap().is_empty());
        assert_eq!(app.project_revision(), 0);

        let mut impossible = command(0);
        let Command::MigrateGraphicsCompression { options, .. } = &mut impossible else {
            unreachable!();
        };
        options.allocation.search = 0x7fe0..0x7fe1;
        assert!(matches!(
            app.dispatch(impossible),
            Err(AppError::GraphicsMigration(_))
        ));
        assert_eq!(app.project_revision(), 0);
        assert_eq!(app.project().unwrap().rom.as_file_bytes(), before);
        assert!(!app.project().unwrap().history.can_undo());
    }

    #[test]
    fn installed_smw_graphics_reject_unauthenticated_runtime_before_mutation() {
        let mut app = AppState::default();
        app.load_rom(fixture()).unwrap();
        install_test_profile(&mut app);
        for offset in lm_profile::SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS {
            app.project
                .as_mut()
                .unwrap()
                .rom
                .write(offset, &[lm_profile::SMW_US_V1_4BPP_GRAPHICS_MARKER])
                .unwrap();
        }
        let before = app.project().unwrap().rom.as_file_bytes().to_vec();

        assert!(matches!(
            app.dispatch(command(0)),
            Err(AppError::GraphicsCompressionMigration(_))
        ));
        assert_eq!(app.project().unwrap().rom.as_file_bytes(), before);
        assert_eq!(app.project_revision(), 0);
        assert!(!app.project().unwrap().history.can_undo());
    }

    #[test]
    #[ignore = "requires retained Lunar Magic 3.63 LZ2-Orig installed-graphics ROM"]
    fn installed_smw_lz3_command_reopens_and_undo_redo_tracks_effective_codec() {
        let original = fs::read(std::env::var_os("LM_LZ2_ORIGINAL_ROM").unwrap()).unwrap();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let source = lm_profile::smw_us_v1_vanilla_graphics_layout();
        let mut profile = lm_profile::test_support::profile();
        profile.graphics = source;
        app.revision_profile = Some(profile);
        app.dispatch(Command::MigrateGraphicsCompression {
            expected_revision: 0,
            source,
            target: GraphicsCompression::Lz3,
            options: GraphicsMigrationOptions {
                allocation: allocation(0x8000..0x10000),
                reuse_identical: true,
                erase_fill: 0xff,
                checksum_field: 0x7fdc,
            },
        })
        .unwrap();
        assert_eq!(app.project_revision(), 1);
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        assert_eq!(
            lm_profile::detect_smw_us_v1_graphics_compression_mode(&app.project().unwrap().rom)
                .unwrap(),
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz3
        );
        assert_eq!(
            app.revision_profile().unwrap().graphics.compression,
            GraphicsCompression::Lz3
        );
        if let Some(path) = std::env::var_os("LM_LZ3_APP_RUST_OUTPUT") {
            fs::write(path, app.project().unwrap().rom.as_file_bytes()).unwrap();
        }
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.as_file_bytes(), original);
        assert_eq!(
            app.revision_profile().unwrap().graphics.compression,
            GraphicsCompression::Lz2
        );
        app.dispatch(Command::Redo).unwrap();
        assert_eq!(
            app.revision_profile().unwrap().graphics.compression,
            GraphicsCompression::Lz3
        );
        assert_eq!(
            lm_profile::detect_smw_us_v1_graphics_compression_mode(&app.project().unwrap().rom)
                .unwrap(),
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz3
        );
        let lz3 = app.project().unwrap().rom.as_file_bytes().to_vec();
        let source = app.revision_profile().unwrap().graphics;
        app.dispatch(Command::MigrateGraphicsCompression {
            expected_revision: app.project_revision(),
            source,
            target: GraphicsCompression::Lz2,
            options: GraphicsMigrationOptions {
                allocation: allocation(0x8000..0x10000),
                reuse_identical: true,
                erase_fill: 0xff,
                checksum_field: 0x7fdc,
            },
        })
        .unwrap();
        assert_eq!(app.project_revision(), 4);
        assert_eq!(
            lm_profile::detect_smw_us_v1_graphics_compression_mode(&app.project().unwrap().rom)
                .unwrap(),
            lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Original
        );
        assert_eq!(
            app.revision_profile().unwrap().graphics.compression,
            GraphicsCompression::Lz2
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.as_file_bytes(), lz3);
        assert_eq!(
            app.revision_profile().unwrap().graphics.compression,
            GraphicsCompression::Lz3
        );
        app.dispatch(Command::Redo).unwrap();
        assert_eq!(
            app.revision_profile().unwrap().graphics.compression,
            GraphicsCompression::Lz2
        );
    }
}
