use crate::StandardSpritePreviewTile;
use crate::standard_sprite_renderer::preview_definition;
use lm_level::{SscDirective, SscEntry};

/// Materializes one decoded `.ssc` display record through the same preview-definition table used
/// by Lunar Magic's standard sprite renderer.
///
/// Returns `None` for non-display records or when a referenced definition has not been loaded.
#[must_use]
pub fn render_lunar_magic_custom_sprite(
    entry: &SscEntry,
) -> Option<Vec<StandardSpritePreviewTile>> {
    let SscDirective::Display(display) = &entry.directive else {
        return None;
    };
    display
        .iter()
        .map(|tile| {
            Some(StandardSpritePreviewTile {
                definition_index: tile.tile,
                subtiles: preview_definition(tile.tile)?,
                x: tile.x,
                y: tile.y,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::SscSidecar;

    #[test]
    fn renders_explicit_and_text_macro_definitions() {
        let sidecar = SscSidecar::decode(b"12\t2\t-8,4,10;8,4,*A*\n").unwrap();
        let parts = render_lunar_magic_custom_sprite(&sidecar.entries()[0]).unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].definition_index, 0x10);
        assert_eq!((parts[0].x, parts[0].y), (-8, 4));
        assert_eq!(parts[1].definition_index, 0x3c7c);
        assert_eq!(parts[2].definition_index, 0x3c41);
    }

    #[test]
    fn rejects_non_display_and_unknown_definitions() {
        let descriptions = SscSidecar::decode(b"1\t0\tdescription\n").unwrap();
        assert!(render_lunar_magic_custom_sprite(&descriptions.entries()[0]).is_none());
        let unknown = SscSidecar::decode(b"1\t2\t0,0,3BFF\n").unwrap();
        assert!(render_lunar_magic_custom_sprite(&unknown.entries()[0]).is_none());
    }
}
