use super::{Command, RomLevelAssetsEditor};
use crate::document_loader::{BoundedRead, LoadedDocument};
use lm_level::{LegacyMwlManifest, MwlFile};
use lm_project::{LegacyMwlBundle, MwlNativeLevel};

pub(super) enum PendingLegacyMwlLoad {
    Manifest {
        revision: u64,
    },
    Sidecars {
        revision: u64,
        manifest: LegacyMwlManifest,
    },
}

impl RomLevelAssetsEditor {
    pub(super) fn show_mwl_actions(
        &mut self,
        ui: &mut eframe::egui::Ui,
        stale: bool,
        modified: bool,
    ) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !stale && !self.mwl_loader.is_running(),
                    eframe::egui::Button::new("Export complete MWL…"),
                )
                .clicked()
                && let Err(error) = self.export_mwl()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    !modified
                        && !stale
                        && !self.mwl_loader.is_running()
                        && !self.legacy_mwl_loader.is_running()
                        && !self.manifest_loader.is_running(),
                    eframe::egui::Button::new("Import complete MWL…"),
                )
                .clicked()
                && let Err(error) = self.choose_mwl_import()
            {
                self.error = Some(error);
            }
        });
        ui.horizontal(|ui| {
            let legacy_enabled = !stale
                && !self.mwl_loader.is_running()
                && !self.legacy_mwl_loader.is_running()
                && !self.manifest_loader.is_running();
            if ui
                .add_enabled(
                    legacy_enabled,
                    eframe::egui::Button::new("Export legacy multi-file level…"),
                )
                .clicked()
                && let Err(error) = self.export_legacy_mwl()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    legacy_enabled && !modified,
                    eframe::egui::Button::new("Import legacy multi-file level…"),
                )
                .clicked()
                && let Err(error) = self.choose_legacy_mwl_import()
            {
                self.error = Some(error);
            }
        });
        ui.horizontal(|ui| {
            let batch_enabled = !modified
                && !stale
                && !self.mwl_loader.is_running()
                && !self.legacy_mwl_loader.is_running()
                && !self.manifest_loader.is_running()
                && !self.mwl_batch_worker.is_running();
            if ui
                .add_enabled(batch_enabled, eframe::egui::Button::new("Export all MWLs…"))
                .clicked()
                && let Err(error) = self.choose_mwl_batch_export(lm_app::MwlBatchExportMode::All)
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    batch_enabled,
                    eframe::egui::Button::new("Export modified MWLs…"),
                )
                .clicked()
                && let Err(error) =
                    self.choose_mwl_batch_export(lm_app::MwlBatchExportMode::Modified)
            {
                self.error = Some(error);
            }
        });
    }

    pub(super) fn export_mwl(&mut self) -> Result<(), String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let Some(destination) = crate::dialogs::choose_mwl_save_path(workspace.source_slot) else {
            return Ok(());
        };
        let semantic = workspace
            .controller
            .export_smw_us_v1_installed_mwl()
            .map_err(|error| error.to_string())?;
        let bytes = semantic
            .encode(
                &workspace.profile.sprite_lengths,
                &workspace.profile.exanimation_double_size_modes,
            )
            .and_then(|file| file.encode().map_err(Into::into))
            .map_err(|error: lm_project::MwlNativeLevelError| error.to_string())?;
        lm_app::file_persistence::write_new(&destination, &bytes).map_err(|error| error.to_string())
    }

    pub(super) fn choose_mwl_import(&mut self) -> Result<(), String> {
        let Some(path) = crate::dialogs::choose_mwl_document() else {
            return Ok(());
        };
        self.mwl_loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(MwlFile::MAX_FILE_BYTES).unwrap_or(u64::MAX),
            "complete MWL level",
        )])
    }

    fn export_legacy_mwl(&mut self) -> Result<(), String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let Some(destination) = crate::dialogs::choose_mwl_save_path(workspace.source_slot) else {
            return Ok(());
        };
        let base_name = destination
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or("legacy MWL output name must be valid Unicode")?;
        let source = workspace
            .controller
            .export_smw_us_v1_installed_mwl()
            .map_err(|error| error.to_string())?;
        let bundle =
            LegacyMwlBundle::from_native(&source, base_name, &workspace.profile.sprite_lengths)
                .map_err(|error| error.to_string())?;
        lm_app::publish_legacy_mwl_bundle_new(&destination, &bundle)
    }

    fn choose_legacy_mwl_import(&mut self) -> Result<(), String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let Some(path) = crate::dialogs::choose_mwl_document() else {
            return Ok(());
        };
        self.legacy_mwl_loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(LegacyMwlManifest::MAX_FILE_BYTES).unwrap_or(u64::MAX),
            "legacy MWL manifest",
        )])?;
        self.pending_legacy_mwl_load = Some(PendingLegacyMwlLoad::Manifest {
            revision: workspace.controller.revision(),
        });
        Ok(())
    }

    pub(super) fn finish_legacy_mwl_load(
        &mut self,
        result: Result<LoadedDocument, String>,
        project_revision: u64,
    ) -> Result<Option<Command>, String> {
        let pending = self
            .pending_legacy_mwl_load
            .take()
            .ok_or("legacy MWL loader completed without a pending operation")?;
        match pending {
            PendingLegacyMwlLoad::Manifest { revision } => {
                let [(path, bytes)] = result?.into_exact::<1>("legacy MWL manifest")?;
                if revision != project_revision {
                    return Err("the ROM changed while the legacy MWL was loading".into());
                }
                let manifest =
                    LegacyMwlManifest::decode(&bytes).map_err(|error| error.to_string())?;
                let paths = lm_app::legacy_mwl_sidecar_paths(&path, &manifest);
                let mut requests = paths
                    .into_iter()
                    .take(3)
                    .enumerate()
                    .map(|(index, path)| {
                        BoundedRead::new(
                            path,
                            u64::try_from(LegacyMwlBundle::MAX_SIDECAR_BYTES).unwrap_or(u64::MAX),
                            ["legacy Layer 1", "legacy Layer 2", "legacy sprites"][index],
                        )
                    })
                    .collect::<Vec<_>>();
                if manifest.layer1.flags & 1 != 0 {
                    let palette_path = lm_app::legacy_mwl_sidecar_paths(&path, &manifest)
                        .into_iter()
                        .nth(3)
                        .ok_or("legacy MWL palette filename is unavailable")?;
                    requests.push(BoundedRead::new(
                        palette_path,
                        u64::try_from(LegacyMwlBundle::PALETTE_BYTES).unwrap_or(u64::MAX),
                        "legacy palette",
                    ));
                }
                self.legacy_mwl_loader.start(requests)?;
                self.pending_legacy_mwl_load =
                    Some(PendingLegacyMwlLoad::Sidecars { revision, manifest });
                Ok(None)
            }
            PendingLegacyMwlLoad::Sidecars { revision, manifest } => {
                if revision != project_revision {
                    return Err("the ROM changed while the legacy MWL was loading".into());
                }
                let loaded = result?;
                let expected = if manifest.layer1.flags & 1 != 0 { 4 } else { 3 };
                if loaded.files.len() != expected {
                    return Err(format!(
                        "legacy MWL loader returned an invalid file group: expected {expected}, got {}",
                        loaded.files.len()
                    ));
                }
                let mut payloads = loaded.files.into_iter().map(|(_, bytes)| bytes);
                let layer1 = payloads.next().ok_or("legacy MWL loader omitted Layer 1")?;
                let layer2 = payloads.next().ok_or("legacy MWL loader omitted Layer 2")?;
                let sprites = payloads.next().ok_or("legacy MWL loader omitted sprites")?;
                let bundle = LegacyMwlBundle {
                    manifest,
                    layer1,
                    layer2,
                    sprites,
                    palette: payloads.next(),
                };
                let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
                if workspace.controller.revision() != revision {
                    return Err("the ROM changed while the legacy MWL was loading".into());
                }
                let mut source = bundle
                    .decode_native(
                        &workspace.profile.sprite_lengths,
                        &workspace.controller.assets().palette,
                        workspace.controller.assets().expanded_settings.is_some(),
                    )
                    .map_err(|error| error.to_string())?;
                source
                    .retarget(workspace.source_slot)
                    .map_err(|error| error.to_string())?;
                let (options, layer2) = self.save_options(workspace)?;
                let layer2 = layer2.ok_or_else(|| {
                    "active revision profile has no native Layer 2 layout".to_string()
                })?;
                workspace
                    .controller
                    .prepare_smw_us_v1_installed_mwl_import(&source, &options, &layer2)
                    .map(lm_app::PreparedRomCommit::into_command)
                    .map(Some)
                    .map_err(|error| error.to_string())
            }
        }
    }

    fn choose_mwl_batch_export(&mut self, mode: lm_app::MwlBatchExportMode) -> Result<(), String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let Some(template) = crate::dialogs::choose_mwl_batch_template() else {
            return Ok(());
        };
        self.mwl_batch_status = None;
        self.mwl_batch_worker.start(
            lm_app::ProfiledControllerSnapshot {
                snapshot: workspace.snapshot.clone(),
                profile: workspace.profile.clone(),
            },
            template,
            mode,
        )
    }

    pub(super) fn finish_mwl_import(
        &self,
        result: Result<LoadedDocument, String>,
        project_revision: u64,
    ) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        if workspace.controller.revision() != project_revision {
            return Err("the ROM changed while the MWL was loading".into());
        }
        let [(_, bytes)] = result?.into_exact::<1>("complete MWL import")?;
        let file = MwlFile::decode(&bytes).map_err(|error| error.to_string())?;
        let mut source = MwlNativeLevel::decode(
            &file,
            &workspace.profile.sprite_lengths,
            workspace.profile.exanimation.maximum_records,
            &workspace.profile.exanimation_double_size_modes,
        )
        .map_err(|error| error.to_string())?;
        source
            .retarget(workspace.source_slot)
            .map_err(|error| error.to_string())?;
        let (options, layer2) = self.save_options(workspace)?;
        let layer2 = layer2
            .ok_or_else(|| "active revision profile has no native Layer 2 layout".to_string())?;
        workspace
            .controller
            .prepare_smw_us_v1_installed_mwl_import(&source, &options, &layer2)
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }
}
