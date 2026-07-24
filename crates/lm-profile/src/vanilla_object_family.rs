/// The five standard-object definition families selected by Lunar Magic for vanilla SMW.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VanillaObjectFamily {
    Normal,
    Castle,
    Rope,
    Underground,
    GhostHouse,
}

impl VanillaObjectFamily {
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Castle => "Castle",
            Self::Rope => "Rope",
            Self::Underground => "Underground",
            Self::GhostHouse => "Ghost House",
        }
    }
}

/// Resolves the object-definition family recovered from
/// `ConfigureStandardObjectDefinitionsForTileset`.
///
/// Invalid selectors retain Lunar Magic's normal-family fallback.
#[must_use]
pub const fn smw_us_v1_object_family(object_tileset: u8) -> VanillaObjectFamily {
    match object_tileset {
        1 => VanillaObjectFamily::Castle,
        2 | 6 | 8 => VanillaObjectFamily::Rope,
        3 | 9 | 0x0a | 0x0b | 0x0e => VanillaObjectFamily::Underground,
        4 | 5 | 0x0d => VanillaObjectFamily::GhostHouse,
        _ => VanillaObjectFamily::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_vanilla_tileset_uses_the_recovered_definition_family() {
        let expected = [
            VanillaObjectFamily::Normal,
            VanillaObjectFamily::Castle,
            VanillaObjectFamily::Rope,
            VanillaObjectFamily::Underground,
            VanillaObjectFamily::GhostHouse,
            VanillaObjectFamily::GhostHouse,
            VanillaObjectFamily::Rope,
            VanillaObjectFamily::Normal,
            VanillaObjectFamily::Rope,
            VanillaObjectFamily::Underground,
            VanillaObjectFamily::Underground,
            VanillaObjectFamily::Underground,
            VanillaObjectFamily::Normal,
            VanillaObjectFamily::GhostHouse,
            VanillaObjectFamily::Underground,
            VanillaObjectFamily::Normal,
        ];
        for (tileset, expected) in expected.into_iter().enumerate() {
            assert_eq!(
                smw_us_v1_object_family(u8::try_from(tileset).unwrap()),
                expected
            );
        }
        assert_eq!(smw_us_v1_object_family(0xff), VanillaObjectFamily::Normal);
    }
}
