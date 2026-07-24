use lm_graphics::{CompactExAnimation, ExAnimationError};
use lm_level::{MwlError, MwlPayloadSection};
use std::fmt;

/// Typed bridge between an MWL common-prefix section and Lunar Magic's compact ROM `ExAnimation`
/// representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MwlExAnimationSection {
    pub metadata: [u32; 2],
    pub animation: Option<CompactExAnimation>,
}

impl MwlExAnimationSection {
    /// Decodes an empty or populated MWL `ExAnimation` section.
    ///
    /// # Errors
    ///
    /// Returns a typed error for malformed MWL framing, invalid compact records, or trailing
    /// compact bytes.
    pub fn decode(
        bytes: &[u8],
        maximum_records: usize,
        double_size_modes: &[bool],
    ) -> Result<Self, MwlExAnimationSectionError> {
        let section = MwlPayloadSection::decode(bytes)?;
        if section.payload.is_empty() {
            return Ok(Self {
                metadata: section.metadata,
                animation: None,
            });
        }
        let (animation, consumed) =
            CompactExAnimation::decode(&section.payload, maximum_records, double_size_modes)?;
        if consumed != section.payload.len() {
            return Err(MwlExAnimationSectionError::TrailingPayload {
                consumed,
                actual: section.payload.len(),
            });
        }
        Ok(Self {
            metadata: section.metadata,
            animation: Some(animation),
        })
    }

    /// Encodes the common MWL provenance prefix followed by the canonical compact payload.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the animation cannot be represented compactly or aggregate
    /// length overflows.
    pub fn encode(
        &self,
        double_size_modes: &[bool],
    ) -> Result<Vec<u8>, MwlExAnimationSectionError> {
        let payload = self
            .animation
            .as_ref()
            .map(|animation| animation.encode(double_size_modes))
            .transpose()?
            .unwrap_or_default();
        Ok(MwlPayloadSection {
            metadata: self.metadata,
            payload,
        }
        .encode()?)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MwlExAnimationSectionError {
    Mwl(MwlError),
    Animation(ExAnimationError),
    TrailingPayload { consumed: usize, actual: usize },
}

impl fmt::Display for MwlExAnimationSectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid MWL ExAnimation section: {self:?}")
    }
}

impl std::error::Error for MwlExAnimationSectionError {}

impl From<MwlError> for MwlExAnimationSectionError {
    fn from(value: MwlError) -> Self {
        Self::Mwl(value)
    }
}

impl From<ExAnimationError> for MwlExAnimationSectionError {
    fn from(value: ExAnimationError) -> Self {
        Self::Animation(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::ExAnimationRecord;

    fn active() -> CompactExAnimation {
        CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![ExAnimationRecord::new(1, 0, 0, 0x100, false, &[0, 6], false).unwrap()],
        }
    }

    #[test]
    fn empty_and_active_sections_round_trip_exactly() {
        let modes = [false; 256];
        for section in [
            MwlExAnimationSection {
                metadata: [0, 0],
                animation: None,
            },
            MwlExAnimationSection {
                metadata: [0, 0x10_97e9],
                animation: Some(active()),
            },
        ] {
            assert_eq!(
                MwlExAnimationSection::decode(&section.encode(&modes).unwrap(), 32, &modes)
                    .unwrap(),
                section
            );
        }
    }

    #[test]
    fn compact_payload_must_be_consumed_exactly() {
        let modes = [false; 256];
        let mut bytes = MwlExAnimationSection {
            metadata: [0; 2],
            animation: Some(active()),
        }
        .encode(&modes)
        .unwrap();
        bytes.push(0xaa);
        assert!(matches!(
            MwlExAnimationSection::decode(&bytes, 32, &modes),
            Err(MwlExAnimationSectionError::TrailingPayload { .. })
        ));
    }
}
