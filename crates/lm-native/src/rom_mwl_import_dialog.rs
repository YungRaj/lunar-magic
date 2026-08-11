use crate::document_loader::{BoundedRead, DocumentLoader, LoadedDocument};
use eframe::egui;
use lm_app::{AppState, Command};
use lm_level::{LegacyMwlManifest, MwlFile};
use lm_project::LegacyMwlBundle;
use std::ops::Range;
use std::path::PathBuf;

enum PendingRead {
    Primary {
        path: PathBuf,
        revision: u64,
    },
    LegacySidecars {
        path: PathBuf,
        revision: u64,
        manifest: LegacyMwlManifest,
        diagnostic: Option<String>,
    },
}

struct PendingCommit {
    path: PathBuf,
    level: u16,
}

#[derive(Default)]
pub(crate) struct RomMwlImportDialog {
    loader: DocumentLoader,
    pending_read: Option<PendingRead>,
    pending_commit: Option<PendingCommit>,
    active_search: Option<Range<usize>>,
    status: Option<String>,
    error: Option<String>,
}

impl RomMwlImportDialog {
    pub(crate) const fn is_open(&self) -> bool {
        self.pending_read.is_some() || self.pending_commit.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) -> Result<(), String> {
        if self.is_open() {
            return Ok(());
        }
        app.profiled_controller_snapshot()
            .map_err(|error| error.to_string())?;
        let Some(path) = crate::dialogs::choose_mwl_document() else {
            return Ok(());
        };
        self.start_path(app, path)
    }

    fn start_path(&mut self, app: &AppState, path: PathBuf) -> Result<(), String> {
        if self.is_open() || self.loader.is_running() {
            return Err("a level-file import is already active".into());
        }
        let logical_len = app
            .project()
            .ok_or("open a supported ROM before importing a level file")?
            .rom
            .logical_len();
        self.loader.start(vec![BoundedRead::new(
            path.clone(),
            u64::try_from(MwlFile::MAX_FILE_BYTES.max(LegacyMwlManifest::MAX_FILE_BYTES))
                .unwrap_or(u64::MAX),
            "MWL level file",
        )])?;
        self.pending_read = Some(PendingRead::Primary {
            path: path.clone(),
            revision: app.project_revision(),
        });
        self.pending_commit = None;
        self.active_search = Some(0..logical_len);
        self.status = Some(format!("Reading {}", path.display()));
        self.error = None;
        Ok(())
    }

    pub(crate) fn show(&mut self, context: &egui::Context, app: &AppState) -> Option<Command> {
        let loaded = self.loader.show(context);
        if self.is_open() || self.error.is_some() {
            let mut dismiss = false;
            egui::Window::new("Insert Level From File")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    if let Some(status) = &self.status {
                        ui.label(status);
                    }
                    if let Some(error) = &self.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                        dismiss = ui.button("Close").clicked();
                    } else {
                        ui.spinner();
                    }
                });
            if dismiss {
                self.clear();
                return None;
            }
        }
        let result = loaded?;
        let command = match self.finish_read(app, result) {
            Ok(command) => command,
            Err(error) => {
                self.error = Some(error);
                None
            }
        };
        context.request_repaint();
        command
    }

    fn finish_read(
        &mut self,
        app: &AppState,
        result: Result<LoadedDocument, String>,
    ) -> Result<Option<Command>, String> {
        let pending = self
            .pending_read
            .take()
            .ok_or("MWL loader completed without a pending level file")?;
        match pending {
            PendingRead::Primary { path, revision } => {
                let [(loaded_path, bytes)] = result?.into_exact::<1>("MWL level file")?;
                if loaded_path != path {
                    return Err("MWL loader returned a different source path".into());
                }
                self.require_revision(app, revision)?;
                if bytes.starts_with(&MwlFile::SIGNATURE) {
                    return self.prepare_modern(app, path, revision, &bytes).map(Some);
                }
                let report =
                    LegacyMwlManifest::decode_with_diagnostics(&bytes).map_err(|error| {
                        format!("level file is neither binary nor legacy MWL: {error}")
                    })?;
                let manifest = report.manifest;
                let sidecar_paths = lm_app::legacy_mwl_sidecar_paths(&path, &manifest);
                let mut requests = sidecar_paths
                    .iter()
                    .take(3)
                    .enumerate()
                    .map(|(index, path)| {
                        BoundedRead::new(
                            path.clone(),
                            u64::try_from(LegacyMwlBundle::MAX_SIDECAR_BYTES).unwrap_or(u64::MAX),
                            ["legacy Layer 1", "legacy Layer 2", "legacy sprites"][index],
                        )
                    })
                    .collect::<Vec<_>>();
                if manifest.layer1.flags & 1 != 0 {
                    let palette = sidecar_paths
                        .get(3)
                        .ok_or("legacy MWL palette filename is unavailable")?;
                    requests.push(BoundedRead::optional(
                        palette.clone(),
                        u64::try_from(LegacyMwlBundle::MAX_SIDECAR_BYTES).unwrap_or(u64::MAX),
                        "legacy palette",
                    ));
                }
                self.loader.start(requests)?;
                let diagnostic = (!report.diagnostics.is_empty()).then(|| {
                    report
                        .diagnostics
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("; ")
                });
                self.status = Some(format!("Reading legacy sidecars for {}", path.display()));
                self.pending_read = Some(PendingRead::LegacySidecars {
                    path,
                    revision,
                    manifest,
                    diagnostic,
                });
                Ok(None)
            }
            PendingRead::LegacySidecars {
                path,
                revision,
                manifest,
                diagnostic,
            } => {
                self.require_revision(app, revision)?;
                let loaded = result?;
                let palette_declared = manifest.layer1.flags & 1 != 0;
                let expected_maximum = if palette_declared { 4 } else { 3 };
                if !(3..=expected_maximum).contains(&loaded.files.len()) {
                    return Err(format!(
                        "legacy MWL requires three sidecars and at most one palette, got {} files",
                        loaded.files.len()
                    ));
                }
                let mut payloads = loaded.files.into_iter().map(|(_, bytes)| bytes);
                let layer1 = payloads.next().ok_or("legacy MWL omitted Layer 1")?;
                let layer2 = payloads.next().ok_or("legacy MWL omitted Layer 2")?;
                let sprites = payloads.next().ok_or("legacy MWL omitted sprites")?;
                let palette = payloads.next();
                let missing_palette = palette_declared && palette.is_none();
                let bundle = LegacyMwlBundle {
                    manifest,
                    layer1,
                    layer2,
                    sprites,
                    palette,
                };
                let profiled = app
                    .profiled_controller_snapshot()
                    .map_err(|error| error.to_string())?;
                let (level, prepared) = lm_app::prepare_declared_legacy_mwl_import(
                    &profiled,
                    &bundle,
                    self.active_search
                        .clone()
                        .ok_or("level-file import allocation range is unavailable")?,
                )?;
                let mut notes = diagnostic.into_iter().collect::<Vec<_>>();
                if missing_palette {
                    notes.push(
                        "Couldn't locate the palette file! Switching to non-custom shared palette."
                            .into(),
                    );
                }
                self.pending_commit = Some(PendingCommit {
                    path: path.clone(),
                    level,
                });
                self.status = Some(if notes.is_empty() {
                    format!("Committing level {level:03X} from {}", path.display())
                } else {
                    format!(
                        "Committing level {level:03X} from {} ({})",
                        path.display(),
                        notes.join("; ")
                    )
                });
                Ok(Some(prepared.into_command()))
            }
        }
    }

    fn prepare_modern(
        &mut self,
        app: &AppState,
        path: PathBuf,
        revision: u64,
        bytes: &[u8],
    ) -> Result<Command, String> {
        self.require_revision(app, revision)?;
        let profiled = app
            .profiled_controller_snapshot()
            .map_err(|error| error.to_string())?;
        let (level, prepared) = lm_app::prepare_declared_mwl_import(
            &profiled,
            bytes,
            self.active_search
                .clone()
                .ok_or("level-file import allocation range is unavailable")?,
        )?;
        self.pending_commit = Some(PendingCommit {
            path: path.clone(),
            level,
        });
        self.status = Some(format!(
            "Committing level {level:03X} from {}",
            path.display()
        ));
        Ok(prepared.into_command())
    }

    fn require_revision(&self, app: &AppState, revision: u64) -> Result<(), String> {
        (app.project_revision() == revision)
            .then_some(())
            .ok_or_else(|| "the ROM changed while the MWL was loading".into())
    }

    pub(crate) fn commit_succeeded(&mut self) -> Option<u16> {
        let Some(pending) = self.pending_commit.take() else {
            return None;
        };
        self.status = Some(format!(
            "Inserted level {:03X} from {}",
            pending.level,
            pending.path.display()
        ));
        self.active_search = None;
        Some(pending.level)
    }

    pub(crate) fn commit_failed(&mut self) {
        let Some(pending) = self.pending_commit.take() else {
            return;
        };
        self.active_search = None;
        self.error = Some(format!(
            "Failed to commit level {:03X} from {}",
            pending.level,
            pending.path.display()
        ));
    }

    pub(crate) fn request_close(&mut self, _application: bool) -> bool {
        if self.loader.is_running() || self.pending_commit.is_some() {
            return false;
        }
        self.clear();
        true
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn closed_dialog_has_no_pending_import() {
        assert!(!RomMwlImportDialog::default().is_open());
    }

    #[test]
    fn revision_guard_rejects_stale_async_work() {
        let dialog = RomMwlImportDialog::default();
        let app = AppState::default();
        assert!(dialog.require_revision(&app, 1).is_err());
    }

    #[test]
    fn sidecar_request_order_follows_manifest_paths() {
        let manifest = LegacyMwlManifest {
            version: LegacyMwlManifest::CURRENT_VERSION,
            attribution: String::new(),
            level_number: 0,
            header: [0; 5],
            layer1: lm_level::LegacyMwlSidecar {
                flags: 1,
                source_address: 0,
                file_name: "A.mw0".into(),
            },
            layer2: lm_level::LegacyMwlSidecar {
                flags: 0,
                source_address: 0,
                file_name: "B.mw1".into(),
            },
            sprites: lm_level::LegacyMwlSidecar {
                flags: 0,
                source_address: 0,
                file_name: "C.mw2".into(),
            },
            secondary_exits: Vec::new(),
        };
        assert_eq!(
            lm_app::legacy_mwl_sidecar_paths(Path::new("levels/Level.mwl"), &manifest),
            [
                PathBuf::from("levels/A.mw0"),
                PathBuf::from("levels/B.mw1"),
                PathBuf::from("levels/C.mw2"),
                PathBuf::from("levels/A.mw3"),
            ]
        );
    }
}
