use crate::PaletteControllerEdit;
use lm_graphics::{Palette, PaletteBatchEditError, PaletteOwnership};

pub(crate) fn apply_palette_edit_batch(
    palette: &mut Palette,
    ownership: &PaletteOwnership,
    edits: &[PaletteControllerEdit],
) -> Result<(), (usize, PaletteBatchEditError)> {
    let mut staged = palette.clone();
    staged
        .apply_changes(&[], ownership)
        .map_err(|error| (0, error))?;
    for (command, edit) in edits.iter().enumerate() {
        let result = match edit {
            PaletteControllerEdit::ApplyChanges(changes) => {
                staged.apply_changes(changes, ownership)
            }
            PaletteControllerEdit::ReplaceRange { start, colors } => {
                staged.replace_range(*start, colors, ownership)
            }
        };
        result.map_err(|error| (command, error))?;
    }
    *palette = staged;
    Ok(())
}
