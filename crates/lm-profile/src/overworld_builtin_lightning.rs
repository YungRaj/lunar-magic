//! Descriptor-backed original overworld-lightning sources.

use lm_rom::{Mapper, Region, RomError, RomImage, SupportedGame};

pub const LUNAR_MAGIC_OVERWORLD_LIGHTNING_MASK_DESCRIPTOR_FIELD: usize = 0x0904;
pub const LUNAR_MAGIC_OVERWORLD_LIGHTNING_DELAYS_DESCRIPTOR_FIELD: usize = 0x090c;
pub const BUILT_IN_OVERWORLD_LIGHTNING_SELECTOR_LEN: usize = 128;

const SMW_NA_MASK_OPERAND_PHYSICAL: usize = 0x02_7909;
const SMW_NA_DELAYS_PHYSICAL: usize = 0x02_78f8;
const SMW_J_MASK_OPERAND_PHYSICAL: usize = 0x02_7901;
const SMW_J_DELAYS_PHYSICAL: usize = 0x02_78f0;
const ALL_STARS_MASK_OPERAND_PHYSICAL: usize = 0x1a_78fd;
const ALL_STARS_DELAYS_PHYSICAL: usize = 0x1a_78ec;
const SELECTOR_PROLOGUE: [u8; 8] = [0xa9, 0xf7, 0x20, 0x82, 0xf8, 0xd0, 0x5f, 0xac];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuiltInOverworldLightningLayout {
    pub mask_operand_offset: usize,
    pub delays_offset: usize,
    pub initial_colors_offset: usize,
    pub selectors_offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuiltInOverworldLightningSources {
    pub layout: BuiltInOverworldLightningLayout,
    pub delays: [u8; 8],
    pub initial_colors: [u8; 8],
    pub selectors: [u8; BUILT_IN_OVERWORLD_LIGHTNING_SELECTOR_LEN],
}

/// Resolves descriptor fields `+$904` and `+$90C` to logical project offsets.
#[must_use]
pub const fn builtin_overworld_lightning_layout(
    game: SupportedGame,
    region: Region,
    mapper: Mapper,
) -> BuiltInOverworldLightningLayout {
    let (mask_physical, delays_physical) = match (game, region) {
        (SupportedGame::SuperMarioWorld, Region::Japan) => {
            (SMW_J_MASK_OPERAND_PHYSICAL, SMW_J_DELAYS_PHYSICAL)
        }
        (SupportedGame::SuperMarioWorld, Region::NorthAmerica) => {
            (SMW_NA_MASK_OPERAND_PHYSICAL, SMW_NA_DELAYS_PHYSICAL)
        }
        (SupportedGame::AllStarsAndWorld, _) => {
            (ALL_STARS_MASK_OPERAND_PHYSICAL, ALL_STARS_DELAYS_PHYSICAL)
        }
    };
    let active_body = if matches!(mapper, Mapper::ExLoRom) {
        0x40_0000
    } else {
        0
    };
    let mask_operand_offset = active_body + mask_physical - 0x200;
    let delays_offset = active_body + delays_physical - 0x200;
    BuiltInOverworldLightningLayout {
        mask_operand_offset,
        delays_offset,
        initial_colors_offset: delays_offset + 8,
        selectors_offset: mask_operand_offset - 1,
    }
}

/// Loads an authenticated selected lightning family.
///
/// Truncation is an error. A complete but foreign/corrupt routine returns `None`, preventing an
/// unrelated 128-byte region from being interpreted as the native selector schedule.
pub fn probe_builtin_overworld_lightning_sources(
    image: &RomImage,
    game: SupportedGame,
    region: Region,
    mapper: Mapper,
) -> Result<Option<BuiltInOverworldLightningSources>, RomError> {
    let layout = builtin_overworld_lightning_layout(game, region, mapper);
    let delays: [u8; 8] = image.read(layout.delays_offset, 8)?.try_into().unwrap();
    let initial_colors: [u8; 8] = image
        .read(layout.initial_colors_offset, 8)?
        .try_into()
        .unwrap();
    let selectors: [u8; BUILT_IN_OVERWORLD_LIGHTNING_SELECTOR_LEN] = image
        .read(
            layout.selectors_offset,
            BUILT_IN_OVERWORLD_LIGHTNING_SELECTOR_LEN,
        )?
        .try_into()
        .unwrap();
    if delays.contains(&0)
        || initial_colors
            .iter()
            .any(|&color| !(1..=7).contains(&color))
        || selectors[..SELECTOR_PROLOGUE.len()] != SELECTOR_PROLOGUE
    {
        return Ok(None);
    }
    Ok(Some(BuiltInOverworldLightningSources {
        layout,
        delays,
        initial_colors,
        selectors,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install_family(
        bytes: &mut [u8],
        game: SupportedGame,
        region: Region,
        mapper: Mapper,
        marker: u8,
    ) {
        let layout = builtin_overworld_lightning_layout(game, region, mapper);
        bytes[layout.delays_offset..layout.delays_offset + 8]
            .copy_from_slice(&[marker, 2, 3, 4, 5, 6, 7, 8]);
        bytes[layout.initial_colors_offset..layout.initial_colors_offset + 8]
            .copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 1]);
        bytes[layout.selectors_offset..layout.selectors_offset + 8]
            .copy_from_slice(&SELECTOR_PROLOGUE);
        bytes[layout.selectors_offset + 8..layout.selectors_offset + 128].fill(marker);
    }

    #[test]
    fn descriptor_fields_route_na_japan_all_stars_sa1_and_exlorom() {
        let na = builtin_overworld_lightning_layout(
            SupportedGame::SuperMarioWorld,
            Region::NorthAmerica,
            Mapper::LoRom,
        );
        assert_eq!((na.delays_offset, na.selectors_offset), (0x276f8, 0x27708));
        let japan = builtin_overworld_lightning_layout(
            SupportedGame::SuperMarioWorld,
            Region::Japan,
            Mapper::LoRom,
        );
        assert_eq!(
            (japan.delays_offset, japan.selectors_offset),
            (0x276f0, 0x27700)
        );
        let sa1 = builtin_overworld_lightning_layout(
            SupportedGame::SuperMarioWorld,
            Region::NorthAmerica,
            Mapper::Sa1,
        );
        assert_eq!(sa1, na);
        let ex = builtin_overworld_lightning_layout(
            SupportedGame::SuperMarioWorld,
            Region::NorthAmerica,
            Mapper::ExLoRom,
        );
        assert_eq!(
            (ex.delays_offset, ex.selectors_offset),
            (0x4276f8, 0x427708)
        );
        let all_stars = builtin_overworld_lightning_layout(
            SupportedGame::AllStarsAndWorld,
            Region::NorthAmerica,
            Mapper::LoRom,
        );
        assert_eq!(
            (all_stars.delays_offset, all_stars.selectors_offset),
            (0x1a76ec, 0x1a76fc)
        );
    }

    #[test]
    fn selected_family_is_authenticated_without_cross_identity_fallback() {
        let mut bytes = vec![0xff; 0x80_0000];
        install_family(
            &mut bytes,
            SupportedGame::SuperMarioWorld,
            Region::NorthAmerica,
            Mapper::LoRom,
            0x20,
        );
        install_family(
            &mut bytes,
            SupportedGame::AllStarsAndWorld,
            Region::NorthAmerica,
            Mapper::LoRom,
            0x30,
        );
        let image = RomImage::from_bytes(bytes).unwrap();
        let ordinary = probe_builtin_overworld_lightning_sources(
            &image,
            SupportedGame::SuperMarioWorld,
            Region::NorthAmerica,
            Mapper::LoRom,
        )
        .unwrap()
        .unwrap();
        assert_eq!(ordinary.delays[0], 0x20);
        let all_stars = probe_builtin_overworld_lightning_sources(
            &image,
            SupportedGame::AllStarsAndWorld,
            Region::NorthAmerica,
            Mapper::LoRom,
        )
        .unwrap()
        .unwrap();
        assert_eq!(all_stars.delays[0], 0x30);

        let mut corrupt = image;
        corrupt
            .write(all_stars.layout.selectors_offset, &[0x00])
            .unwrap();
        assert!(
            probe_builtin_overworld_lightning_sources(
                &corrupt,
                SupportedGame::AllStarsAndWorld,
                Region::NorthAmerica,
                Mapper::LoRom,
            )
            .unwrap()
            .is_none()
        );
    }
}
