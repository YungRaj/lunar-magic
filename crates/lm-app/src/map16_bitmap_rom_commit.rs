//! Revision-checked application boundary for accepted native bitmap import plans.

use crate::{ControllerSnapshot, PreparedRomCommit};
use lm_project::{
    Map16BitmapRomSave, Map16BitmapRomSaveError, Project, RomMutation, TransactionError,
};
use lm_rom::{RomError, RomImage};
use std::fmt;

#[derive(Debug)]
pub enum Map16BitmapCommitError {
    Rom(RomError),
    Save(Map16BitmapRomSaveError),
    Mutation(TransactionError),
}

impl fmt::Display for Map16BitmapCommitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot prepare native Map16 bitmap import: {self:?}"
        )
    }
}

impl std::error::Error for Map16BitmapCommitError {}

/// Serializes every accepted import domain privately and returns one revision-bound mutation.
///
/// If the document changes while its preview is open, dispatch rejects the returned command
/// instead of overwriting the newer project.
///
/// # Errors
///
/// Returns a ROM parse, grouped native-save, or mutation error without changing application state.
pub fn prepare_map16_bitmap_rom_commit(
    snapshot: &ControllerSnapshot,
    save: &Map16BitmapRomSave<'_>,
) -> Result<PreparedRomCommit, Map16BitmapCommitError> {
    let description = save.description.to_owned();
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(Map16BitmapCommitError::Rom)?;
    let before = image.logical_bytes().to_vec();
    let mut project = Project::new(image);
    project
        .save_map16_bitmap_import(save)
        .map_err(Map16BitmapCommitError::Save)?;
    let mutation = RomMutation::between(
        snapshot.identity.mapper,
        &before,
        project.rom.logical_bytes(),
    )
    .map_err(Map16BitmapCommitError::Mutation)?;
    Ok(PreparedRomCommit {
        expected_revision: snapshot.revision,
        description,
        mutation,
    })
}
