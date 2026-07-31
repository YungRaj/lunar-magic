//! Per-level animation feature switches used by Lunar Magic's Super GFX Bypass dialog.

/// Lossless semantic view of Lunar Magic's per-level animation feature byte.
///
/// The four high bits use inverted polarity in storage. The low nibble is unrelated to these
/// switches and is retained verbatim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExAnimationFeatureOptions {
    pub preserved_low_nibble: u8,
    enabled: [bool; 4],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExAnimationFeature {
    LevelExAnimation = 0,
    GlobalExAnimation = 1,
    VanillaAnimation = 2,
    PaletteAnimation = 3,
}

impl ExAnimationFeatureOptions {
    /// Decodes the four inverted feature-disable bits recovered from
    /// `EncodeExAnimationFeatureDisableFlags` at `00460340`.
    #[must_use]
    pub const fn decode(packed: u8) -> Self {
        Self {
            preserved_low_nibble: packed & 0x0f,
            enabled: [
                packed & 0x10 == 0,
                packed & 0x20 == 0,
                packed & 0x40 == 0,
                packed & 0x80 == 0,
            ],
        }
    }

    #[must_use]
    pub const fn enabled(self, feature: ExAnimationFeature) -> bool {
        self.enabled[feature as usize]
    }

    pub const fn set_enabled(&mut self, feature: ExAnimationFeature, enabled: bool) {
        self.enabled[feature as usize] = enabled;
    }

    /// Rebuilds the exact feature byte while preserving the unrelated low nibble.
    #[must_use]
    pub fn encode(self) -> u8 {
        (self.preserved_low_nibble & 0x0f)
            | (u8::from(!self.enabled[3]) << 7)
            | (u8::from(!self.enabled[2]) << 6)
            | (u8::from(!self.enabled[1]) << 5)
            | (u8::from(!self.enabled[0]) << 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_feature_byte_round_trips_losslessly() {
        for packed in 0_u8..=u8::MAX {
            assert_eq!(ExAnimationFeatureOptions::decode(packed).encode(), packed);
        }
    }

    #[test]
    fn recovered_inverted_bits_have_named_semantics() {
        let options = ExAnimationFeatureOptions::decode(0xa5);
        assert!(!options.enabled(ExAnimationFeature::PaletteAnimation));
        assert!(options.enabled(ExAnimationFeature::VanillaAnimation));
        assert!(!options.enabled(ExAnimationFeature::GlobalExAnimation));
        assert!(options.enabled(ExAnimationFeature::LevelExAnimation));
        assert_eq!(options.preserved_low_nibble, 5);
    }
}
