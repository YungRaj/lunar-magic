//! Shared asynchronous persistence lifecycle for single-file document controllers.

use crate::persistence_worker::{PersistenceTarget, PersistenceWorker};
use eframe::egui;
use std::path::PathBuf;

pub(crate) struct DocumentSaveSnapshot {
    request_id: u64,
    path: PathBuf,
    bytes: Vec<u8>,
}

pub(crate) struct PairedDocumentSaveSnapshot {
    request_id: u64,
    first_path: PathBuf,
    first_bytes: Vec<u8>,
    second_path: PathBuf,
    second_bytes: Vec<u8>,
}

pub(crate) trait DocumentSaveController {
    fn begin_document_save(&mut self) -> Result<DocumentSaveSnapshot, String>;
    fn acknowledge_document_save(&mut self, request_id: u64) -> Result<(), String>;
    fn cancel_document_save(&mut self, request_id: u64) -> Result<(), String>;
}

pub(crate) trait PairedDocumentSaveController {
    fn begin_paired_save(&mut self) -> Result<PairedDocumentSaveSnapshot, String>;
    fn acknowledge_paired_save(&mut self, request_id: u64) -> Result<(), String>;
    fn cancel_paired_save(&mut self, request_id: u64) -> Result<(), String>;
}

macro_rules! controller {
    ($controller:ty) => {
        impl DocumentSaveController for $controller {
            fn begin_document_save(&mut self) -> Result<DocumentSaveSnapshot, String> {
                self.begin_save()
                    .map(|snapshot| DocumentSaveSnapshot {
                        request_id: snapshot.request_id,
                        path: snapshot.path,
                        bytes: snapshot.bytes.into(),
                    })
                    .map_err(|error| error.to_string())
            }

            fn acknowledge_document_save(&mut self, request_id: u64) -> Result<(), String> {
                self.acknowledge_save(request_id)
                    .map_err(|error| error.to_string())
            }

            fn cancel_document_save(&mut self, request_id: u64) -> Result<(), String> {
                self.cancel_save(request_id)
                    .map_err(|error| error.to_string())
            }
        }
    };
}

controller!(lm_app::ExAnimationDocumentController);
controller!(lm_app::DscSidecarController);
controller!(lm_app::SscSidecarController);
controller!(lm_app::OscSidecarController);
controller!(lm_app::CompleteLevelDocumentController);
controller!(lm_app::EntityAppearanceDocumentController);
controller!(lm_app::ExpandedSettingsDocumentController);
controller!(lm_app::GraphicsDocumentController);
controller!(lm_app::Layer3DocumentController);
controller!(lm_app::Map16DocumentController);
controller!(lm_app::Map16PageDocumentController);
controller!(lm_app::MwlDocumentController);
controller!(lm_app::NativeLevelDocumentController);
controller!(lm_app::NativeLevelAssetsDocumentController);
controller!(lm_app::NativeMap16SidecarController);
controller!(lm_app::OverworldAppearanceDocumentController);
controller!(lm_app::OverworldDocumentController);
controller!(lm_app::OverworldMetadataController);
controller!(lm_app::OverworldPathController);
controller!(lm_app::PaletteDocumentController);

macro_rules! paired_controller {
    ($controller:ty) => {
        impl PairedDocumentSaveController for $controller {
            fn begin_paired_save(&mut self) -> Result<PairedDocumentSaveSnapshot, String> {
                self.begin_save()
                    .map(|snapshot| PairedDocumentSaveSnapshot {
                        request_id: snapshot.request_id,
                        first_path: snapshot.data_path,
                        first_bytes: snapshot.data,
                        second_path: snapshot.descriptions_path,
                        second_bytes: snapshot.descriptions,
                    })
                    .map_err(|error| error.to_string())
            }

            fn acknowledge_paired_save(&mut self, request_id: u64) -> Result<(), String> {
                self.acknowledge_save(request_id)
                    .map_err(|error| error.to_string())
            }

            fn cancel_paired_save(&mut self, request_id: u64) -> Result<(), String> {
                self.cancel_save(request_id)
                    .map_err(|error| error.to_string())
            }
        }
    };
}

paired_controller!(lm_app::CustomObjectLibraryController);
paired_controller!(lm_app::CustomSpriteLibraryController);

#[derive(Default)]
pub(crate) struct DocumentPersistence {
    worker: PersistenceWorker,
}

impl DocumentPersistence {
    pub(crate) const fn is_running(&self) -> bool {
        self.worker.is_running()
    }

    pub(crate) fn begin<C: DocumentSaveController>(
        &mut self,
        controller: &mut C,
    ) -> Result<(), String> {
        let snapshot = controller.begin_document_save()?;
        if let Err(error) = self.worker.start(
            snapshot.request_id,
            PersistenceTarget::Replace(snapshot.path),
            snapshot.bytes,
        ) {
            controller.cancel_document_save(snapshot.request_id)?;
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn show<C: DocumentSaveController>(
        &mut self,
        context: &egui::Context,
        controller: &mut C,
    ) -> Option<Result<(), String>> {
        let completion = self.worker.show(context)?;
        Some(Self::complete(controller, completion))
    }

    fn complete<C: DocumentSaveController>(
        controller: &mut C,
        completion: crate::persistence_worker::PersistenceCompletion,
    ) -> Result<(), String> {
        match completion.result {
            Ok(()) => controller.acknowledge_document_save(completion.request_id),
            Err(write_error) => match controller.cancel_document_save(completion.request_id) {
                Ok(()) => Err(write_error),
                Err(cancel_error) => Err(format!("{write_error}; additionally, {cancel_error}")),
            },
        }
    }

    pub(crate) fn begin_pair<C: PairedDocumentSaveController>(
        &mut self,
        controller: &mut C,
    ) -> Result<(), String> {
        let snapshot = controller.begin_paired_save()?;
        if let Err(error) = self.worker.start_pair(
            snapshot.request_id,
            snapshot.first_path,
            snapshot.first_bytes,
            snapshot.second_path,
            snapshot.second_bytes,
        ) {
            controller.cancel_paired_save(snapshot.request_id)?;
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn show_pair<C: PairedDocumentSaveController>(
        &mut self,
        context: &egui::Context,
        controller: &mut C,
    ) -> Option<Result<(), String>> {
        let completion = self.worker.show(context)?;
        Some(Self::complete_pair(controller, completion))
    }

    fn complete_pair<C: PairedDocumentSaveController>(
        controller: &mut C,
        completion: crate::persistence_worker::PersistenceCompletion,
    ) -> Result<(), String> {
        match completion.result {
            Ok(()) => controller.acknowledge_paired_save(completion.request_id),
            Err(write_error) => match controller.cancel_paired_save(completion.request_id) {
                Ok(()) => Err(write_error),
                Err(cancel_error) => Err(format!("{write_error}; additionally, {cancel_error}")),
            },
        }
    }

    #[cfg(test)]
    fn wait_for_test<C: DocumentSaveController>(
        &mut self,
        controller: &mut C,
    ) -> Result<(), String> {
        Self::complete(controller, self.worker.wait_for_test())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Controller {
        path: PathBuf,
        bytes: Vec<u8>,
        pending: bool,
        acknowledged: Vec<u64>,
        cancelled: Vec<u64>,
    }

    impl DocumentSaveController for Controller {
        fn begin_document_save(&mut self) -> Result<DocumentSaveSnapshot, String> {
            self.pending = true;
            Ok(DocumentSaveSnapshot {
                request_id: 41,
                path: self.path.clone(),
                bytes: self.bytes.clone(),
            })
        }

        fn acknowledge_document_save(&mut self, request_id: u64) -> Result<(), String> {
            self.pending = false;
            self.acknowledged.push(request_id);
            Ok(())
        }

        fn cancel_document_save(&mut self, request_id: u64) -> Result<(), String> {
            self.pending = false;
            self.cancelled.push(request_id);
            Ok(())
        }
    }

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm-document-persistence-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn controller(path: PathBuf) -> Controller {
        Controller {
            path,
            bytes: vec![0, 1, 0, 2],
            pending: false,
            acknowledged: Vec::new(),
            cancelled: Vec::new(),
        }
    }

    #[test]
    fn successful_write_acknowledges_the_exact_snapshot() {
        let path = path("success");
        fs::write(&path, [9]).unwrap();
        let mut controller = controller(path.clone());
        let mut persistence = DocumentPersistence::default();
        persistence.begin(&mut controller).unwrap();
        persistence.wait_for_test(&mut controller).unwrap();
        assert_eq!(controller.acknowledged, [41]);
        assert!(controller.cancelled.is_empty());
        assert_eq!(fs::read(&path).unwrap(), [0, 1, 0, 2]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_write_cancels_without_acknowledging() {
        let path = path("missing");
        let mut controller = controller(path);
        let mut persistence = DocumentPersistence::default();
        persistence.begin(&mut controller).unwrap();
        assert!(persistence.wait_for_test(&mut controller).is_err());
        assert!(controller.acknowledged.is_empty());
        assert_eq!(controller.cancelled, [41]);
        assert!(!controller.pending);
    }
}
