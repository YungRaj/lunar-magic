use super::{PathEditor, PendingOpen};
use crate::{
    dialogs,
    document_loader::{BoundedRead, LoadedDocument},
};
use lm_app::OverworldPathController;
use lm_overworld::OverworldPathGraph;

impl PathEditor {
    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_overworld_path_document() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(OverworldPathGraph::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "overworld path document",
        )]) {
            self.error = Some(error);
        }
    }

    pub(super) fn finish_open(&mut self) {
        let Some(pending) = self.pending_open.take() else {
            return;
        };
        let result = OverworldPathController::decode(
            pending.path.clone(),
            &pending.bytes,
            pending.require_reciprocal,
        )
        .and_then(|mut controller| {
            controller.apply_edits(controller.revision(), &[])?;
            Ok(controller)
        });
        match result {
            Ok(controller) => {
                self.controller = Some(controller);
                self.invalidate();
            }
            Err(error) => {
                self.error = Some(error.to_string());
                self.pending_open = Some(pending);
            }
        }
    }
}

pub(super) fn pending_open(mut loaded: LoadedDocument) -> Result<PendingOpen, String> {
    let (path, bytes) = loaded
        .files
        .pop()
        .ok_or_else(|| "path loader returned no file".to_string())?;
    Ok(PendingOpen {
        path,
        bytes,
        require_reciprocal: false,
    })
}
