use lm_level::{OscEntry, OscResolvedObject};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CustomObjectPreviewTile {
    pub tile: u16,
    pub x: i16,
    pub y: i16,
}

#[must_use]
pub fn render_lunar_magic_custom_object(entry: &OscEntry) -> Option<Vec<CustomObjectPreviewTile>> {
    let lm_level::OscDirective::Display(display) = &entry.directive else {
        return None;
    };
    Some(
        display
            .iter()
            .map(|tile| CustomObjectPreviewTile {
                tile: tile.tile,
                x: tile.x,
                y: tile.y,
            })
            .collect(),
    )
}

#[must_use]
pub fn render_resolved_lunar_magic_custom_object(
    object: &OscResolvedObject,
) -> Option<Vec<CustomObjectPreviewTile>> {
    Some(
        object
            .display
            .as_ref()?
            .iter()
            .map(|tile| CustomObjectPreviewTile {
                tile: tile.tile,
                x: tile.x,
                y: tile.y,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{OscResolvedTable, OscSidecar};

    #[test]
    fn renders_source_and_resolved_display_records() {
        let source = OscSidecar::decode(b"10\t2\t13\t-8,4,123;8,4,124\n").unwrap();
        assert_eq!(
            render_lunar_magic_custom_object(&source.entries()[0]).unwrap(),
            [
                CustomObjectPreviewTile {
                    tile: 0x123,
                    x: -8,
                    y: 4,
                },
                CustomObjectPreviewTile {
                    tile: 0x124,
                    x: 8,
                    y: 4,
                },
            ]
        );
        let resolved = OscResolvedTable::from_sidecar(&source);
        assert_eq!(
            render_resolved_lunar_magic_custom_object(&resolved.objects()[0])
                .unwrap()
                .len(),
            2
        );
    }
}
