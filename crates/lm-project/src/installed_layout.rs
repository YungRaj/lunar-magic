//! ROM-marker-gated layout selection for optional Lunar Magic-installed subsystems.

use lm_rom::{RomError, RomImage};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstallationMarker {
    pub offset: usize,
    pub expected: u8,
}

impl InstallationMarker {
    /// Tests one marker byte in the logical ROM image.
    ///
    /// # Errors
    ///
    /// Returns the underlying bounds error when the marker is outside the image.
    pub fn matches(self, rom: &RomImage) -> Result<bool, RomError> {
        Ok(rom.read(self.offset, 1)?[0] == self.expected)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GatedLayout<T> {
    pub marker: InstallationMarker,
    pub layout: T,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstalledLayout<T> {
    Absent,
    Unconditional(T),
    Alternatives {
        primary: GatedLayout<T>,
        fallback: Option<GatedLayout<T>>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstalledAsset<T> {
    SubsystemAbsent,
    SlotEmpty,
    Present(T),
}

impl<T: Copy> InstalledLayout<T> {
    /// Resolves the first installed layout, preserving primary-before-fallback precedence.
    ///
    /// # Errors
    ///
    /// Returns a bounds error if a marker location is outside the logical ROM.
    pub fn resolve(self, rom: &RomImage) -> Result<Option<T>, InstalledLayoutError> {
        match self {
            Self::Absent => Ok(None),
            Self::Unconditional(layout) => Ok(Some(layout)),
            Self::Alternatives { primary, fallback } => {
                if primary.marker.matches(rom)? {
                    return Ok(Some(primary.layout));
                }
                if let Some(fallback) = fallback
                    && fallback.marker.matches(rom)?
                {
                    return Ok(Some(fallback.layout));
                }
                Ok(None)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstalledLayoutError {
    Rom(RomError),
    NotInstalled(&'static str),
}

impl fmt::Display for InstalledLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "optional installed layout failed: {self:?}")
    }
}

impl std::error::Error for InstalledLayoutError {}

impl From<RomError> for InstalledLayoutError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternatives_use_primary_precedence_and_report_absence() {
        let mut rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        let layouts = InstalledLayout::Alternatives {
            primary: GatedLayout {
                marker: InstallationMarker {
                    offset: 0x20,
                    expected: 0x22,
                },
                layout: 1_u8,
            },
            fallback: Some(GatedLayout {
                marker: InstallationMarker {
                    offset: 0x30,
                    expected: 0x22,
                },
                layout: 2,
            }),
        };
        assert_eq!(layouts.resolve(&rom).unwrap(), None);
        rom.write(0x30, &[0x22]).unwrap();
        assert_eq!(layouts.resolve(&rom).unwrap(), Some(2));
        rom.write(0x20, &[0x22]).unwrap();
        assert_eq!(layouts.resolve(&rom).unwrap(), Some(1));
    }

    #[test]
    fn an_out_of_image_marker_is_not_silently_treated_as_absent() {
        let rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        let layout = InstalledLayout::Alternatives {
            primary: GatedLayout {
                marker: InstallationMarker {
                    offset: 0x8000,
                    expected: 0xc2,
                },
                layout: (),
            },
            fallback: None,
        };
        assert!(matches!(
            layout.resolve(&rom),
            Err(InstalledLayoutError::Rom(_))
        ));
    }
}
