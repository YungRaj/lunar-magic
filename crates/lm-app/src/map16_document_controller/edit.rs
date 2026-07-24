use super::{Map16DocumentController, Map16DocumentControllerError, Map16DocumentEdit};
use lm_level::Map16SetFile;

impl Map16DocumentController {
    /// Applies an ordered edit batch against one exact revision and canonically reopens it.
    ///
    /// # Errors
    ///
    /// Any stale revision, invalid command, encoding failure, or overflow preserves all state.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[Map16DocumentEdit],
    ) -> Result<(), Map16DocumentControllerError> {
        if expected_revision != self.revision {
            return Err(Map16DocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        for (command, edit) in edits.iter().enumerate() {
            let result = match edit {
                Map16DocumentEdit::ReplaceTiles {
                    replacements,
                    resolution_limit,
                } => staged.set.replace_tiles(replacements, *resolution_limit),
                Map16DocumentEdit::SetSubtile {
                    address,
                    quadrant,
                    subtile,
                    resolution_limit,
                } => staged
                    .set
                    .set_subtile(*address, *quadrant, *subtile, *resolution_limit),
                Map16DocumentEdit::SetActsLike {
                    address,
                    acts_like,
                    resolution_limit,
                } => staged
                    .set
                    .set_acts_like(*address, *acts_like, *resolution_limit),
                Map16DocumentEdit::AppendPage {
                    page,
                    resolution_limit,
                } => staged.set.push_page(page.clone(), *resolution_limit),
                Map16DocumentEdit::RemoveLastPage { resolution_limit } => {
                    staged.set.pop_page(*resolution_limit).map(|_| ())
                }
            };
            result.map_err(|error| Map16DocumentControllerError::Edit { command, error })?;
        }
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(Map16DocumentControllerError::RevisionOverflow)?;
        let bytes = staged
            .encode()
            .map_err(Map16DocumentControllerError::File)?;
        let reopened = Map16SetFile::decode(&bytes).map_err(Map16DocumentControllerError::File)?;
        if reopened != staged {
            return Err(Map16DocumentControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }
}
