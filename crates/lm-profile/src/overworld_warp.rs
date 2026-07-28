//! SMW US revision-0 native overworld warp/exit-link layout.

use lm_project::OverworldWarpLinkRomLayout;
use lm_rom::Mapper;

pub const SMW_US_V1_OVERWORLD_WARP_SOURCE_VERTICAL_OFFSET: usize = 0x02_0431;
pub const SMW_US_V1_OVERWORLD_WARP_SOURCE_HORIZONTAL_OFFSET: usize = 0x02_0467;
pub const SMW_US_V1_OVERWORLD_WARP_DESTINATION_VERTICAL_OFFSET: usize = 0x02_049d;
pub const SMW_US_V1_OVERWORLD_WARP_DESTINATION_HORIZONTAL_OFFSET: usize = 0x02_04d3;
pub const SMW_US_V1_OVERWORLD_WARP_LINK_COUNT: usize = 27;

#[must_use]
pub const fn smw_us_v1_overworld_warp_link_layout() -> OverworldWarpLinkRomLayout {
    OverworldWarpLinkRomLayout {
        mapper: Mapper::LoRom,
        source_vertical_offset: SMW_US_V1_OVERWORLD_WARP_SOURCE_VERTICAL_OFFSET,
        source_horizontal_offset: SMW_US_V1_OVERWORLD_WARP_SOURCE_HORIZONTAL_OFFSET,
        destination_vertical_offset: SMW_US_V1_OVERWORLD_WARP_DESTINATION_VERTICAL_OFFSET,
        destination_horizontal_offset: SMW_US_V1_OVERWORLD_WARP_DESTINATION_HORIZONTAL_OFFSET,
        entries: SMW_US_V1_OVERWORLD_WARP_LINK_COUNT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;
    use lm_rom::RomImage;
    use std::path::PathBuf;

    #[test]
    fn pristine_rom_round_trips_all_four_exact_native_planes() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let project = Project::open_supported(RomImage::from_bytes(bytes).unwrap()).unwrap();
        let table = project
            .load_overworld_warp_links(smw_us_v1_overworld_warp_link_layout())
            .unwrap();
        assert_eq!(table.links.len(), SMW_US_V1_OVERWORLD_WARP_LINK_COUNT);
        assert_eq!(table.links[0].source.packed_vertical, 0x0011);
        assert_eq!(table.links[0].source.horizontal_tile, 0x0007);
        assert_eq!(table.links[0].destination.packed_vertical, 0x04a8);
        assert_eq!(table.links[0].destination.horizontal_tile, 0x0148);

        let planes = table.encode_planes().unwrap();
        for (offset, actual) in [
            (
                SMW_US_V1_OVERWORLD_WARP_SOURCE_VERTICAL_OFFSET,
                planes.source_vertical,
            ),
            (
                SMW_US_V1_OVERWORLD_WARP_SOURCE_HORIZONTAL_OFFSET,
                planes.source_horizontal,
            ),
            (
                SMW_US_V1_OVERWORLD_WARP_DESTINATION_VERTICAL_OFFSET,
                planes.destination_vertical,
            ),
            (
                SMW_US_V1_OVERWORLD_WARP_DESTINATION_HORIZONTAL_OFFSET,
                planes.destination_horizontal,
            ),
        ] {
            assert_eq!(
                actual,
                project
                    .rom
                    .read(offset, SMW_US_V1_OVERWORLD_WARP_LINK_COUNT * 2)
                    .unwrap()
            );
        }
    }
}
