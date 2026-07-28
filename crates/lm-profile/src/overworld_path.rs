//! SMW US revision-0 native overworld path-link layout.

use lm_project::OverworldPathLinkRomLayout;
use lm_rom::Mapper;

pub const SMW_US_V1_OVERWORLD_PATH_SOURCE_OFFSET: usize = 0x02_1964;
pub const SMW_US_V1_OVERWORLD_PATH_DESTINATION_OFFSET: usize = 0x02_19aa;
pub const SMW_US_V1_OVERWORLD_PATH_TARGET_OFFSET: usize = 0x02_19f0;
pub const SMW_US_V1_OVERWORLD_PATH_LINK_COUNT: usize = 14;

#[must_use]
pub const fn smw_us_v1_overworld_path_link_layout() -> OverworldPathLinkRomLayout {
    OverworldPathLinkRomLayout {
        mapper: Mapper::LoRom,
        source_offset: SMW_US_V1_OVERWORLD_PATH_SOURCE_OFFSET,
        destination_offset: SMW_US_V1_OVERWORLD_PATH_DESTINATION_OFFSET,
        target_offset: SMW_US_V1_OVERWORLD_PATH_TARGET_OFFSET,
        entries: SMW_US_V1_OVERWORLD_PATH_LINK_COUNT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;
    use lm_rom::RomImage;
    use std::path::PathBuf;

    #[test]
    fn pristine_rom_decodes_and_reencodes_all_three_exact_native_planes() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let project = Project::open_supported(RomImage::from_bytes(bytes).unwrap()).unwrap();
        let table = project
            .load_overworld_path_links(smw_us_v1_overworld_path_link_layout())
            .unwrap();
        assert_eq!(table.links.len(), SMW_US_V1_OVERWORLD_PATH_LINK_COUNT);
        assert_eq!(table.links[0].source.x, 0x0140);
        assert_eq!(table.links[0].source.y, 0x0028);
        assert_eq!(table.links[0].destination.x, 0);
        assert_eq!(table.links[0].destination.y, 0x0048);
        assert_eq!(table.links[0].target.y_tile, 0);
        assert_eq!(table.links[0].target.x_tile, 4);

        let encoded = table.encode_planes().unwrap();
        assert_eq!(
            encoded.sources,
            project
                .rom
                .read(
                    SMW_US_V1_OVERWORLD_PATH_SOURCE_OFFSET,
                    SMW_US_V1_OVERWORLD_PATH_LINK_COUNT * 5
                )
                .unwrap()
        );
        assert_eq!(
            encoded.destinations,
            project
                .rom
                .read(
                    SMW_US_V1_OVERWORLD_PATH_DESTINATION_OFFSET,
                    SMW_US_V1_OVERWORLD_PATH_LINK_COUNT * 5
                )
                .unwrap()
        );
        assert_eq!(
            encoded.targets,
            project
                .rom
                .read(
                    SMW_US_V1_OVERWORLD_PATH_TARGET_OFFSET,
                    SMW_US_V1_OVERWORLD_PATH_LINK_COUNT * 2
                )
                .unwrap()
        );
    }
}
