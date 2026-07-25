use crate::{SscDirective, SscDisplayTile, SscSidecar, SscSpriteSelector};

pub const SSC_REMAP_ENTRY_COUNT: usize = 0x3c00;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SscResolvedSprite {
    pub selector: SscSpriteSelector,
    pub description: Option<String>,
    pub display: Option<Vec<SscDisplayTile>>,
    pub palette: Option<Vec<[u16; 4]>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SscResolvedTable {
    sprites: Vec<SscResolvedSprite>,
    tile_remaps: Vec<Option<u16>>,
    palette_remaps: Vec<Option<u16>>,
}

impl SscResolvedTable {
    #[must_use]
    pub fn from_sidecar(sidecar: &SscSidecar) -> Self {
        let mut resolved = Self {
            sprites: Vec::new(),
            tile_remaps: vec![None; SSC_REMAP_ENTRY_COUNT],
            palette_remaps: vec![None; SSC_REMAP_ENTRY_COUNT],
        };
        for entry in sidecar.entries() {
            match (&entry.selector, &entry.directive) {
                (Some(selector), directive) => {
                    let target = resolved.sprite_mut(*selector);
                    match directive {
                        SscDirective::Description(value) => {
                            target.description = Some(value.clone());
                        }
                        SscDirective::Display(value) => target.display = Some(value.clone()),
                        SscDirective::Palette(value) => target.palette = Some(value.clone()),
                        SscDirective::TileRemap { .. } | SscDirective::PaletteRemap(_) => {}
                    }
                }
                (None, SscDirective::TileRemap { ranges, .. }) => {
                    apply_ranges(&mut resolved.tile_remaps, ranges);
                }
                (None, SscDirective::PaletteRemap(ranges)) => {
                    apply_ranges(&mut resolved.palette_remaps, ranges);
                }
                (None, _) => {}
            }
        }
        resolved
    }

    #[must_use]
    pub fn sprites(&self) -> &[SscResolvedSprite] {
        &self.sprites
    }

    #[must_use]
    pub fn get(&self, selector: SscSpriteSelector) -> Option<&SscResolvedSprite> {
        self.sprites.iter().find(|entry| entry.selector == selector)
    }

    /// Resolves the default display variant for a native placement.
    ///
    /// Dimension/extra-byte-specific variants retain source order; callers with the complete
    /// extended record can use [`Self::get`] for an exact selector.
    #[must_use]
    pub fn default_display(&self, sprite_number: u8, extra_bits: u8) -> Option<&SscResolvedSprite> {
        self.sprites.iter().find(|entry| {
            entry.selector.sprite_number == sprite_number
                && entry.selector.extra_bits == extra_bits
                && !entry.selector.alternate
                && entry.display.is_some()
        })
    }

    #[must_use]
    pub fn tile_remap(&self, source: u16) -> Option<u16> {
        self.tile_remaps.get(usize::from(source)).copied().flatten()
    }

    #[must_use]
    pub fn palette_remap(&self, source: u16) -> Option<u16> {
        self.palette_remaps
            .get(usize::from(source))
            .copied()
            .flatten()
    }

    fn sprite_mut(&mut self, selector: SscSpriteSelector) -> &mut SscResolvedSprite {
        if let Some(index) = self
            .sprites
            .iter()
            .position(|entry| entry.selector == selector)
        {
            return &mut self.sprites[index];
        }
        self.sprites.push(SscResolvedSprite {
            selector,
            description: None,
            display: None,
            palette: None,
        });
        self.sprites.last_mut().expect("just inserted")
    }
}

fn apply_ranges(target: &mut [Option<u16>], ranges: &[crate::SscRemapRange]) {
    for range in ranges {
        for value in range.first..=range.last {
            target[usize::from(value)] = Some(range.target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SscSidecar;

    #[test]
    fn later_records_replace_only_their_metadata_domain() {
        let source = SscSidecar::decode(
            b"10\t0\told\n10\t2\t0,0,10\n10\t8\t1,2,3,4\n10\t0\tnew\n10\t2\t8,9,11\n",
        )
        .unwrap();
        let table = SscResolvedTable::from_sidecar(&source);
        let sprite = &table.sprites()[0];
        assert_eq!(sprite.description.as_deref(), Some("new"));
        assert_eq!(sprite.display.as_ref().unwrap()[0].tile, 0x11);
        assert_eq!(sprite.palette.as_deref(), Some(&[[1, 2, 3, 4]][..]));
    }

    #[test]
    fn later_global_ranges_replace_overlapping_cells() {
        let source =
            SscSidecar::decode(b"10000\t1\t10-12,20\n10000\t1\t11-11,30\n20000\t0\t1-2,7\n")
                .unwrap();
        let table = SscResolvedTable::from_sidecar(&source);
        assert_eq!(table.tile_remap(0x10), Some(0x20));
        assert_eq!(table.tile_remap(0x11), Some(0x30));
        assert_eq!(table.tile_remap(0x12), Some(0x20));
        assert_eq!(table.palette_remap(1), Some(7));
        assert_eq!(table.palette_remap(3), None);
    }

    #[test]
    fn default_display_matches_native_number_and_extra_bits() {
        let source =
            SscSidecar::decode(b"10\t12\t0,0,10\n10\t22\t0,0,11\n10\t13\t0,0,12\n").unwrap();
        let table = SscResolvedTable::from_sidecar(&source);
        assert_eq!(
            table
                .default_display(0x10, 1)
                .unwrap()
                .display
                .as_ref()
                .unwrap()[0]
                .tile,
            0x10
        );
        assert_eq!(
            table
                .default_display(0x10, 2)
                .unwrap()
                .display
                .as_ref()
                .unwrap()[0]
                .tile,
            0x11
        );
    }
}
