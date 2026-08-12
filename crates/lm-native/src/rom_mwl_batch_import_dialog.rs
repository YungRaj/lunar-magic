use crate::document_loader::{BoundedRead, DocumentLoader, LoadedDocument};
use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey, LocalizationCatalog};
use lm_level::MwlFile;
use std::collections::VecDeque;
use std::ops::Range;
use std::path::PathBuf;

struct PendingCommit {
    path: PathBuf,
    level: u16,
}

struct PendingRead {
    path: PathBuf,
    revision: u64,
}

#[derive(Default)]
pub(crate) struct RomMwlBatchImportDialog {
    directory: Option<PathBuf>,
    paths: VecDeque<PathBuf>,
    total: usize,
    hidden_skipped: usize,
    inserted: usize,
    failed: usize,
    search_start: String,
    search_end: String,
    active_search: Option<Range<usize>>,
    pending_commit: Option<PendingCommit>,
    loader: DocumentLoader,
    pending_read: Option<PendingRead>,
    last_diagnostic: Option<String>,
    error: Option<String>,
}

impl RomMwlBatchImportDialog {
    pub(crate) const fn is_open(&self) -> bool {
        self.directory.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        let Some(directory) = crate::dialogs::choose_mwl_directory() else {
            return;
        };
        let listing = match lm_app::discover_mwl_directory(&directory) {
            Ok(listing) => listing,
            Err(error) => {
                self.error = Some(error.to_string());
                self.directory = Some(directory);
                return;
            }
        };
        let logical_len = app.project().map_or(0, |project| project.rom.logical_len());
        self.directory = Some(directory);
        self.paths = listing.paths.into();
        self.total = self.paths.len();
        self.hidden_skipped = listing.hidden_skipped;
        self.inserted = 0;
        self.failed = 0;
        self.search_start = "0".into();
        self.search_end = format!("{logical_len:X}");
        self.active_search = None;
        self.pending_commit = None;
        self.last_diagnostic = None;
        self.error = None;
    }

    pub(crate) fn request_close(&mut self, _application: bool) -> bool {
        self.clear();
        true
    }

    pub(crate) fn show(&mut self, context: &egui::Context, app: &AppState) -> Option<Command> {
        let catalog = app.localization();
        let directory = self.directory.clone()?;
        let loaded = self.loader.show(context);
        let mut start = false;
        let mut close = false;
        let mut cancel = false;
        egui::Window::new(text(catalog, ExtendedUiTextKey::MwlBatchImportTitle))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(format_text(
                    catalog,
                    ExtendedUiTextKey::MwlBatchImportDirectoryFormat,
                    &[("{path}", directory.display().to_string())],
                ));
                ui.label(format_text(
                    catalog,
                    ExtendedUiTextKey::MwlBatchImportSummaryFormat,
                    &[
                        ("{inserted}", self.inserted.to_string()),
                        ("{failed}", self.failed.to_string()),
                        ("{hidden}", self.hidden_skipped.to_string()),
                        (
                            "{remaining}",
                            (self.paths.len()
                                + usize::from(self.pending_commit.is_some())
                                + usize::from(self.pending_read.is_some()))
                            .to_string(),
                        ),
                    ],
                ));
                if self.active_search.is_none()
                    && self.pending_commit.is_none()
                    && self.pending_read.is_none()
                {
                    ui.horizontal(|ui| {
                        ui.label(text(
                            catalog,
                            ExtendedUiTextKey::MwlBatchImportAllocationSearch,
                        ));
                        ui.text_edit_singleline(&mut self.search_start);
                        ui.label(text(
                            catalog,
                            ExtendedUiTextKey::MwlBatchImportRangeSeparator,
                        ));
                        ui.text_edit_singleline(&mut self.search_end);
                    });
                    if self.inserted + self.failed == 0
                        && ui
                            .button(text(catalog, ExtendedUiTextKey::MwlBatchImportStart))
                            .clicked()
                    {
                        start = true;
                    }
                } else {
                    ui.label(text(catalog, ExtendedUiTextKey::MwlBatchImportCancelNotice));
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::MwlBatchImportCancel))
                        .clicked()
                    {
                        cancel = true;
                    }
                }
                if let Some(pending) = &self.pending_commit {
                    ui.label(level_path_text(
                        catalog,
                        ExtendedUiTextKey::MwlBatchImportCommittingFormat,
                        pending.level,
                        &pending.path,
                    ));
                }
                if let Some(pending) = &self.pending_read {
                    ui.label(format_text(
                        catalog,
                        ExtendedUiTextKey::MwlBatchImportReadingFormat,
                        &[("{path}", pending.path.display().to_string())],
                    ));
                }
                if let Some(diagnostic) = &self.last_diagnostic {
                    ui.label(diagnostic);
                }
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::RED, error);
                }
                let complete = self.active_search.is_none()
                    && self.pending_commit.is_none()
                    && self.pending_read.is_none()
                    && (self.paths.is_empty() || self.inserted + self.failed != 0);
                if (complete || self.total == 0)
                    && ui
                        .button(text(catalog, ExtendedUiTextKey::MwlBatchImportClose))
                        .clicked()
                {
                    close = true;
                }
            });
        if close {
            self.clear();
            return None;
        }
        if start {
            match crate::rom_allocation::parse_search_range(&self.search_start, &self.search_end) {
                Ok(search) => {
                    self.active_search = Some(search);
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        if cancel || context.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.paths.clear();
            self.active_search = None;
            self.last_diagnostic = Some(text(catalog, ExtendedUiTextKey::MwlBatchImportCancelled));
        }
        if let Some(result) = loaded {
            let command = self.finish_read(app, result);
            context.request_repaint();
            return command;
        }
        if self.active_search.is_some()
            && self.pending_commit.is_none()
            && self.pending_read.is_none()
        {
            self.start_next_read(app, catalog);
            context.request_repaint();
        }
        None
    }

    fn start_next_read(&mut self, app: &AppState, catalog: Option<&LocalizationCatalog>) {
        let Some(path) = self.paths.pop_front() else {
            self.active_search = None;
            self.last_diagnostic = Some(format_text(
                catalog,
                ExtendedUiTextKey::MwlBatchImportCompleteFormat,
                &[
                    ("{inserted}", self.inserted.to_string()),
                    ("{failed}", self.failed.to_string()),
                    ("{hidden}", self.hidden_skipped.to_string()),
                ],
            ));
            return;
        };
        match self.loader.start(vec![BoundedRead::new(
            path.clone(),
            u64::try_from(MwlFile::MAX_FILE_BYTES).unwrap_or(u64::MAX),
            "complete MWL level",
        )]) {
            Ok(()) => {
                self.pending_read = Some(PendingRead {
                    path,
                    revision: app.project_revision(),
                });
            }
            Err(error) => {
                self.failed += 1;
                self.last_diagnostic = Some(format_text(
                    catalog,
                    ExtendedUiTextKey::MwlBatchImportReadFailedFormat,
                    &[("{path}", path.display().to_string()), ("{error}", error)],
                ));
            }
        }
    }

    fn finish_read(
        &mut self,
        app: &AppState,
        result: Result<LoadedDocument, String>,
    ) -> Option<Command> {
        let pending = match self.pending_read.take() {
            Some(pending) => pending,
            None => {
                self.error = Some("MWL loader completed without a pending file".into());
                return None;
            }
        };
        if self.active_search.is_none() {
            self.last_diagnostic = Some(text(
                app.localization(),
                ExtendedUiTextKey::MwlBatchImportDiscardedRead,
            ));
            return None;
        }
        let result = result
            .and_then(|loaded| loaded.into_exact::<1>("complete MWL level"))
            .and_then(|[(_, bytes)]| {
                if app.project_revision() != pending.revision {
                    return Err("the ROM changed while the MWL was loading".into());
                }
                let profiled = app
                    .profiled_controller_snapshot()
                    .map_err(|error| error.to_string())?;
                lm_app::prepare_declared_mwl_import(
                    &profiled,
                    &bytes,
                    self.active_search
                        .clone()
                        .ok_or_else(|| "batch import is not active".to_string())?,
                )
            });
        match result {
            Ok((level, prepared)) => {
                self.pending_commit = Some(PendingCommit {
                    path: pending.path.clone(),
                    level,
                });
                self.last_diagnostic = Some(level_path_text(
                    app.localization(),
                    ExtendedUiTextKey::MwlBatchImportPreparedFormat,
                    level,
                    &pending.path,
                ));
                Some(prepared.into_command())
            }
            Err(error) => {
                self.failed += 1;
                self.last_diagnostic = Some(format_text(
                    app.localization(),
                    ExtendedUiTextKey::MwlBatchImportInsertFailedFormat,
                    &[
                        ("{path}", pending.path.display().to_string()),
                        ("{error}", error),
                    ],
                ));
                None
            }
        }
    }

    pub(crate) fn commit_succeeded(&mut self, catalog: Option<&LocalizationCatalog>) {
        let Some(pending) = self.pending_commit.take() else {
            return;
        };
        self.inserted += 1;
        self.last_diagnostic = Some(level_path_text(
            catalog,
            ExtendedUiTextKey::MwlBatchImportInsertedFormat,
            pending.level,
            &pending.path,
        ));
    }

    pub(crate) fn commit_failed(&mut self, catalog: Option<&LocalizationCatalog>) {
        let Some(pending) = self.pending_commit.take() else {
            return;
        };
        self.failed += 1;
        self.last_diagnostic = Some(level_path_text(
            catalog,
            ExtendedUiTextKey::MwlBatchImportCommitFailedFormat,
            pending.level,
            &pending.path,
        ));
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}
fn format_text(
    catalog: Option<&LocalizationCatalog>,
    key: ExtendedUiTextKey,
    replacements: &[(&str, String)],
) -> String {
    replacements
        .iter()
        .fold(text(catalog, key), |text, (placeholder, value)| {
            text.replace(placeholder, value)
        })
}
fn level_path_text(
    catalog: Option<&LocalizationCatalog>,
    key: ExtendedUiTextKey,
    level: u16,
    path: &std::path::Path,
) -> String {
    format_text(
        catalog,
        key,
        &[
            ("{level}", format!("{level:03X}")),
            ("{path}", path.display().to_string()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn complete_batch_import_form_uses_every_typed_key() {
        let source = include_str!("rom_mwl_batch_import_dialog.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("MwlBatchImport"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for literal in [
            "Window::new(\"Insert Multiple MWL Levels\")",
            "ui.button(\"Start import\")",
            "ui.button(\"Cancel\")",
            "ui.button(\"Close\")",
        ] {
            assert!(!source.contains(literal));
        }
    }

    fn temporary_mwl() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-batch-import-worker-{}-{}.mwl",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, b"bounded worker payload").unwrap();
        path
    }

    fn pending(level: u16) -> PendingCommit {
        PendingCommit {
            path: PathBuf::from(format!("Level {level:03X}.mwl")),
            level,
        }
    }

    #[test]
    fn dispatch_acknowledgements_advance_exactly_one_file() {
        let mut dialog = RomMwlBatchImportDialog {
            directory: Some(PathBuf::from("levels")),
            pending_commit: Some(pending(0x105)),
            ..RomMwlBatchImportDialog::default()
        };
        dialog.commit_succeeded(None);
        assert_eq!(dialog.inserted, 1);
        assert_eq!(dialog.failed, 0);
        assert!(dialog.pending_commit.is_none());
        assert!(dialog.last_diagnostic.as_deref().unwrap().contains("105"));

        dialog.pending_commit = Some(pending(0x106));
        dialog.commit_failed(None);
        assert_eq!(dialog.inserted, 1);
        assert_eq!(dialog.failed, 1);
        assert!(dialog.pending_commit.is_none());
    }

    #[test]
    fn close_discards_only_coordinator_state() {
        let mut dialog = RomMwlBatchImportDialog {
            directory: Some(PathBuf::from("levels")),
            paths: [PathBuf::from("Level 001.mwl")].into(),
            inserted: 4,
            failed: 2,
            active_search: Some(0x80_000..0x10_0000),
            ..RomMwlBatchImportDialog::default()
        };
        assert!(dialog.request_close(false));
        assert!(!dialog.is_open());
        assert!(dialog.paths.is_empty());
        assert_eq!(dialog.inserted, 0);
        assert_eq!(dialog.failed, 0);
    }

    #[test]
    fn next_file_starts_a_revision_bound_nonblocking_read() {
        let path = temporary_mwl();
        let mut dialog = RomMwlBatchImportDialog {
            directory: path.parent().map(PathBuf::from),
            paths: [path.clone()].into(),
            active_search: Some(0x80_000..0x10_0000),
            ..RomMwlBatchImportDialog::default()
        };
        let app = AppState::default();
        dialog.start_next_read(&app, None);
        assert!(dialog.loader.is_running());
        assert_eq!(dialog.pending_read.as_ref().unwrap().path, path);
        assert_eq!(dialog.pending_read.as_ref().unwrap().revision, 0);
        assert!(dialog.paths.is_empty());
        dialog.clear();
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stale_or_cancelled_worker_completion_never_prepares_a_commit() {
        let path = PathBuf::from("Level 105.mwl");
        let loaded = || {
            Ok(LoadedDocument {
                files: vec![(path.clone(), b"not decoded before stale rejection".to_vec())],
            })
        };
        let app = AppState::default();
        let mut stale = RomMwlBatchImportDialog {
            directory: Some(PathBuf::from("levels")),
            active_search: Some(0x80_000..0x10_0000),
            pending_read: Some(PendingRead {
                path: path.clone(),
                revision: 1,
            }),
            ..RomMwlBatchImportDialog::default()
        };
        assert!(stale.finish_read(&app, loaded()).is_none());
        assert_eq!(stale.failed, 1);
        assert!(stale.pending_commit.is_none());
        assert!(
            stale
                .last_diagnostic
                .as_deref()
                .unwrap()
                .contains("ROM changed")
        );

        let mut cancelled = RomMwlBatchImportDialog {
            directory: Some(PathBuf::from("levels")),
            pending_read: Some(PendingRead {
                path: path.clone(),
                revision: 0,
            }),
            ..RomMwlBatchImportDialog::default()
        };
        assert!(cancelled.finish_read(&app, loaded()).is_none());
        assert_eq!(cancelled.failed, 0);
        assert!(cancelled.pending_commit.is_none());
        assert!(
            cancelled
                .last_diagnostic
                .as_deref()
                .unwrap()
                .contains("cancellation")
        );
    }
}
