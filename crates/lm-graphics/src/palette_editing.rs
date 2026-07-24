use crate::{Bgr555, Palette};
use std::{collections::BTreeSet, fmt};

/// The subsystem that owns one palette entry in the active editor context.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PaletteEntryOwner {
    #[default]
    Editable,
    Fixed,
    ExAnimation {
        record: u16,
    },
}

/// Explicit ownership for every color in one decoded palette.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaletteOwnership {
    owners: Vec<PaletteEntryOwner>,
}

impl PaletteOwnership {
    #[must_use]
    pub fn editable(color_count: usize) -> Self {
        Self {
            owners: vec![PaletteEntryOwner::Editable; color_count],
        }
    }

    #[must_use]
    pub fn from_owners(owners: Vec<PaletteEntryOwner>) -> Self {
        Self { owners }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.owners.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }

    #[must_use]
    pub fn owner(&self, index: usize) -> Option<PaletteEntryOwner> {
        self.owners.get(index).copied()
    }

    /// Marks an existing color with its editor owner.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteBatchEditError::ColorOutOfRange`] for an invalid index.
    pub fn set_owner(
        &mut self,
        index: usize,
        owner: PaletteEntryOwner,
    ) -> Result<(), PaletteBatchEditError> {
        let len = self.owners.len();
        let target = self
            .owners
            .get_mut(index)
            .ok_or(PaletteBatchEditError::ColorOutOfRange { index, len })?;
        *target = owner;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaletteChange {
    pub index: usize,
    pub color: Bgr555,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaletteBatchEditError {
    OwnershipShape {
        colors: usize,
        owners: usize,
    },
    ColorOutOfRange {
        index: usize,
        len: usize,
    },
    DuplicateColor(usize),
    ProtectedColor {
        index: usize,
        owner: PaletteEntryOwner,
    },
    RangeOverflow,
}

impl fmt::Display for PaletteBatchEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid palette batch edit: {self:?}")
    }
}

impl std::error::Error for PaletteBatchEditError {}

impl Palette {
    /// Applies unique editable color changes after validating the complete batch.
    ///
    /// Fixed colors and colors owned by an `ExAnimation` record are rejected so callers can
    /// navigate to the owning editor instead of silently overwriting generated state. An empty
    /// batch still checks that the ownership map describes this palette exactly.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteBatchEditError`] for a mismatched ownership map, invalid or duplicate
    /// indexes, or protected colors. Failure leaves the palette unchanged.
    pub fn apply_changes(
        &mut self,
        changes: &[PaletteChange],
        ownership: &PaletteOwnership,
    ) -> Result<(), PaletteBatchEditError> {
        validate_ownership_shape(self, ownership)?;
        let mut indexes = BTreeSet::new();
        for change in changes {
            let owner =
                ownership
                    .owner(change.index)
                    .ok_or(PaletteBatchEditError::ColorOutOfRange {
                        index: change.index,
                        len: self.colors.len(),
                    })?;
            if !indexes.insert(change.index) {
                return Err(PaletteBatchEditError::DuplicateColor(change.index));
            }
            if owner != PaletteEntryOwner::Editable {
                return Err(PaletteBatchEditError::ProtectedColor {
                    index: change.index,
                    owner,
                });
            }
        }
        for change in changes {
            self.colors[change.index] = change.color;
        }
        Ok(())
    }

    /// Replaces a contiguous destination range through ownership-aware batch validation.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteBatchEditError`] if range arithmetic overflows, any destination is absent,
    /// or any destination is protected. Failure leaves the palette unchanged.
    pub fn replace_range(
        &mut self,
        start: usize,
        colors: &[Bgr555],
        ownership: &PaletteOwnership,
    ) -> Result<(), PaletteBatchEditError> {
        let end = start
            .checked_add(colors.len())
            .ok_or(PaletteBatchEditError::RangeOverflow)?;
        if end > self.colors.len() {
            return Err(PaletteBatchEditError::ColorOutOfRange {
                index: end.saturating_sub(1),
                len: self.colors.len(),
            });
        }
        let changes = colors
            .iter()
            .copied()
            .enumerate()
            .map(|(offset, color)| PaletteChange {
                index: start + offset,
                color,
            })
            .collect::<Vec<_>>();
        self.apply_changes(&changes, ownership)
    }
}

fn validate_ownership_shape(
    palette: &Palette,
    ownership: &PaletteOwnership,
) -> Result<(), PaletteBatchEditError> {
    if palette.colors.len() != ownership.len() {
        return Err(PaletteBatchEditError::OwnershipShape {
            colors: palette.colors.len(),
            owners: ownership.len(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PaletteInterchangeFile;

    fn palette() -> Palette {
        Palette {
            colors: (0_u16..32).map(Bgr555).collect(),
        }
    }

    #[test]
    fn unique_editable_batch_commits_and_round_trips() {
        let mut palette = palette();
        let ownership = PaletteOwnership::editable(32);
        palette
            .apply_changes(
                &[
                    PaletteChange {
                        index: 20,
                        color: Bgr555(0x7fff),
                    },
                    PaletteChange {
                        index: 2,
                        color: Bgr555(0x1234),
                    },
                ],
                &ownership,
            )
            .unwrap();
        assert_eq!(palette.colors[2], Bgr555(0x1234));
        assert_eq!(palette.colors[20], Bgr555(0x7fff));
        let file = PaletteInterchangeFile {
            source_palette: 1,
            palette: palette.clone(),
        };
        assert_eq!(
            PaletteInterchangeFile::decode(&file.encode().unwrap())
                .unwrap()
                .palette,
            palette
        );
    }

    #[test]
    fn fixed_and_animation_owned_colors_reject_the_whole_batch() {
        let mut palette = palette();
        let original = palette.clone();
        let mut ownership = PaletteOwnership::editable(32);
        ownership.set_owner(0, PaletteEntryOwner::Fixed).unwrap();
        ownership
            .set_owner(17, PaletteEntryOwner::ExAnimation { record: 9 })
            .unwrap();
        assert_eq!(
            palette.apply_changes(
                &[
                    PaletteChange {
                        index: 4,
                        color: Bgr555(9),
                    },
                    PaletteChange {
                        index: 17,
                        color: Bgr555(10),
                    },
                ],
                &ownership,
            ),
            Err(PaletteBatchEditError::ProtectedColor {
                index: 17,
                owner: PaletteEntryOwner::ExAnimation { record: 9 },
            })
        );
        assert_eq!(palette, original);
        assert!(matches!(
            palette.replace_range(0, &[Bgr555(8)], &ownership),
            Err(PaletteBatchEditError::ProtectedColor { index: 0, .. })
        ));
        assert_eq!(palette, original);
    }

    #[test]
    fn shape_duplicate_and_range_errors_are_atomic() {
        let mut palette = palette();
        let original = palette.clone();
        assert!(matches!(
            palette.apply_changes(&[], &PaletteOwnership::editable(31)),
            Err(PaletteBatchEditError::OwnershipShape { .. })
        ));
        let ownership = PaletteOwnership::editable(32);
        let duplicate = PaletteChange {
            index: 3,
            color: Bgr555(7),
        };
        assert_eq!(
            palette.apply_changes(&[duplicate, duplicate], &ownership),
            Err(PaletteBatchEditError::DuplicateColor(3))
        );
        assert!(matches!(
            palette.replace_range(31, &[Bgr555(1), Bgr555(2)], &ownership),
            Err(PaletteBatchEditError::ColorOutOfRange { .. })
        ));
        assert_eq!(palette, original);
    }
}
