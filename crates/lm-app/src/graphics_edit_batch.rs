use crate::GraphicsControllerEdit;
use lm_graphics::{GraphicsEditError, GraphicsFile4bpp, GraphicsOwnership};

pub(crate) fn apply_graphics_edit_batch(
    graphics: &mut GraphicsFile4bpp,
    ownership: &GraphicsOwnership,
    edits: &[GraphicsControllerEdit],
) -> Result<(), (usize, GraphicsEditError)> {
    let mut staged = graphics.clone();
    staged
        .apply_tile_changes(&[], ownership)
        .map_err(|error| (0, error))?;
    for (command, edit) in edits.iter().enumerate() {
        let result = match edit {
            GraphicsControllerEdit::ApplyChanges(changes) => {
                staged.apply_tile_changes(changes, ownership)
            }
            GraphicsControllerEdit::ReplaceRange { start, tiles } => {
                staged.replace_tile_range(*start, tiles, ownership)
            }
        };
        result.map_err(|error| (command, error))?;
    }
    *graphics = staged;
    Ok(())
}
