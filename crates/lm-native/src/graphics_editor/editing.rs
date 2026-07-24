use crate::native_clipboard;
use lm_app::{GraphicsControllerEdit, GraphicsDocumentController};
use lm_graphics::{GraphicsOwnership, GraphicsTileChange, IndexedTile};

pub(super) fn apply_pixel(
    controller: &mut GraphicsDocumentController,
    index: usize,
    x: usize,
    y: usize,
    color: u8,
    mut tile: IndexedTile,
    error_slot: &mut Option<String>,
) {
    if let Err(error) = tile.set_pixel(x, y, color) {
        *error_slot = Some(error.to_string());
        return;
    }
    apply_tile(controller, index, tile, error_slot);
}

pub(super) fn paste_tile(
    controller: &mut GraphicsDocumentController,
    selected_tile: &mut usize,
    text: &str,
    error_slot: &mut Option<String>,
) {
    let count = controller.value().graphics.tiles.len();
    *selected_tile = (*selected_tile).min(count.saturating_sub(1));
    match native_clipboard::decode_graphics_tile(text) {
        Ok(tile) if *selected_tile < count => {
            apply_tile(controller, *selected_tile, tile, error_slot);
        }
        Ok(_) => *error_slot = Some("graphics document has no destination tile".into()),
        Err(error) => *error_slot = Some(error),
    }
}

fn apply_tile(
    controller: &mut GraphicsDocumentController,
    index: usize,
    tile: IndexedTile,
    error_slot: &mut Option<String>,
) {
    let count = controller.value().graphics.tiles.len();
    let edit = GraphicsControllerEdit::ApplyChanges(vec![GraphicsTileChange { index, tile }]);
    if let Err(error) = controller.apply_edits(
        controller.revision(),
        &GraphicsOwnership::editable(count),
        &[edit],
    ) {
        *error_slot = Some(error.to_string());
    }
}
