use super::{ExAnimationEditor, PendingOpen, animation_modes, decode_document, dialogs, egui};
use crate::document_loader::BoundedRead;
use crate::document_loader::LoadedDocument;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog};
use lm_graphics::CompactExAnimationFile;

impl ExAnimationEditor {
    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(animation) = dialogs::choose_exanimation_document() else {
            return;
        };
        let Some(mode_path) = dialogs::choose_exanimation_size_modes() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![
            BoundedRead::new(
                animation,
                u64::try_from(CompactExAnimationFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
                "ExAnimation document",
            ),
            BoundedRead::new(mode_path, 256, "ExAnimation size-mode table"),
        ]) {
            self.error = Some(error);
        }
    }

    pub(super) fn poll_open_load(&mut self, context: &egui::Context) {
        let Some(result) = self.loader.show(context) else {
            return;
        };
        match result.and_then(pending_from_loaded) {
            Ok(pending) => self.pending_open = Some(pending),
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn show_open_configuration(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) {
        if self.pending_open.is_none() {
            return;
        }
        egui::Window::new(super::text(catalog, Key::ExAnimationDocumentOpenTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(super::text(catalog, Key::ExAnimationDocumentMaximumRecords));
                if let Some(pending) = self.pending_open.as_mut() {
                    ui.text_edit_singleline(&mut pending.maximum_records);
                }
                ui.horizontal(|ui| {
                    if ui
                        .button(super::text(catalog, Key::ExAnimationDocumentCancel))
                        .clicked()
                    {
                        self.pending_open = None;
                    }
                    if ui
                        .button(super::text(catalog, Key::ExAnimationDocumentOpen))
                        .clicked()
                    {
                        self.finish_open();
                    }
                });
            });
    }

    fn finish_open(&mut self) {
        let Some(pending) = self.pending_open.take() else {
            return;
        };
        let result = pending
            .maximum_records
            .trim()
            .parse::<usize>()
            .map_err(|error| format!("invalid maximum record count: {error}"))
            .and_then(|maximum| {
                decode_document(pending.animation, &pending.bytes, pending.modes, maximum)
            });
        match result {
            Ok(document) => {
                self.document = Some(document);
                self.selected_record = 0;
                self.loaded_revision = None;
                self.loaded_record = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
}

fn pending_from_loaded(loaded: LoadedDocument) -> Result<PendingOpen, String> {
    let [(animation, bytes), (_, mode_bytes)] = loaded.into_exact::<2>("ExAnimation")?;
    Ok(PendingOpen {
        animation,
        bytes,
        modes: animation_modes::decode(&mode_bytes)?,
        maximum_records: "32".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn loaded_group_binds_document_bytes_to_exact_modes() {
        let mut modes = vec![0; 256];
        modes[17] = 9;
        let pending = pending_from_loaded(LoadedDocument {
            files: vec![
                (PathBuf::from("animation.lmexan"), vec![1, 2, 3]),
                (PathBuf::from("modes.bin"), modes),
            ],
        })
        .unwrap();
        assert_eq!(pending.animation, PathBuf::from("animation.lmexan"));
        assert_eq!(pending.bytes, [1, 2, 3]);
        assert!(pending.modes[17]);
        assert_eq!(pending.maximum_records, "32");
    }

    #[test]
    fn loaded_group_rejects_missing_or_malformed_mode_table() {
        assert!(
            pending_from_loaded(LoadedDocument {
                files: vec![(PathBuf::from("animation.lmexan"), Vec::new())],
            })
            .is_err()
        );
        assert!(
            pending_from_loaded(LoadedDocument {
                files: vec![
                    (PathBuf::from("animation.lmexan"), Vec::new()),
                    (PathBuf::from("modes.bin"), vec![0; 255]),
                ],
            })
            .is_err()
        );
    }
}
