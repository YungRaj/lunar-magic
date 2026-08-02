use super::{
    NativeLevelAssetsController, NativeLevelAssetsControllerError, empty_compact_exanimation,
};
use lm_level::{
    Layer2Storage, MwlFile, MwlLevelHeaderSection, MwlMainEntranceSettings,
    MwlMidwayEntranceSettings, MwlSecondaryExit, level_mode_layer2_storage,
};
use lm_project::{MwlNativeLevel, Project};
use lm_rom::RomImage;

impl NativeLevelAssetsController {
    /// Builds a complete semantic MWL export from the selected installed SMW-US level.
    ///
    /// All allocator-dependent source pointers are read from the active ROM tables. Global
    /// entrance and secondary-exit state is reopened from the same immutable controller snapshot,
    /// while staged per-level assets are exported from the controller value.
    ///
    /// # Errors
    ///
    /// Rejects a missing Layer 2 domain or installed runtime, malformed pointers/tables, and ROM
    /// bounds failures instead of emitting an incomplete MWL.
    pub fn export_smw_us_v1_installed_mwl(
        &self,
    ) -> Result<MwlNativeLevel, NativeLevelAssetsControllerError> {
        let layer2 = self
            .layer2
            .clone()
            .ok_or(NativeLevelAssetsControllerError::MwlLayer2Unavailable)?;
        let layer2_layout = self
            .layer2_layout
            .ok_or(NativeLevelAssetsControllerError::MwlLayer2Unavailable)?;
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(NativeLevelAssetsControllerError::Rom)?;
        let project = Project::new(image);
        let level = self.assets.level.number;
        let mut header = export_header(&project, level)?;
        let layer2_pointer = layer2_layout
            .pointers
            .read_snes_pointer(&project.rom, level)
            .map_err(NativeLevelAssetsControllerError::Layout)?
            .get();
        header.0[16] |= derived_level_mode_high_bit(
            self.assets.level.layer1.header.level_mode(),
            header.0[3],
            layer2_pointer,
        );
        let secondary_exits = export_secondary_exits(&project, level)?;
        let exanimation = (self.assets.exanimation != empty_compact_exanimation())
            .then(|| self.assets.exanimation.clone());
        let exanimation_source = if exanimation.is_some() {
            self.layout
                .exanimation
                .pointers
                .read_snes_pointer(&project.rom, level)
                .map_err(NativeLevelAssetsControllerError::Layout)?
                .get()
        } else {
            0
        };
        let palette_source = self
            .layout
            .palette
            .pointers
            .read_snes_pointer(&project.rom, level)
            .map_err(NativeLevelAssetsControllerError::Layout)?
            .get();
        let mut palette = self.assets.palette.clone();
        // Lunar Magic rotates the installed 257-word payload left by one word when populating its
        // 0x202-byte MWL working buffer.
        palette.colors.rotate_left(1);
        let default = MwlFile::default();
        Ok(MwlNativeLevel {
            version: MwlFile::CURRENT_VERSION,
            flags: default.flags,
            attribution: default.attribution,
            header,
            layer1_metadata: [
                u32::from(palette_source > 0xff),
                self.layout
                    .level
                    .layer1
                    .read_snes_pointer(&project.rom, level)
                    .map_err(NativeLevelAssetsControllerError::Layout)?
                    .get(),
            ],
            layer1: self.assets.level.layer1.clone(),
            layer2_descriptor: self
                .layer2_descriptor
                .unwrap_or_else(|| lm_level::MwlLayer2Descriptor::from_raw(0)),
            layer2_source_address: layer2_pointer,
            layer2,
            sprite_metadata: [
                0,
                lm_profile::smw_us_v1_sprite_pointer_table(&project.rom)
                    .map_err(NativeLevelAssetsControllerError::Rom)?
                    .read_snes_pointer(&project.rom, level)
                    .map_err(NativeLevelAssetsControllerError::Layout)?
                    .get(),
            ],
            sprites: self.assets.level.sprites.clone(),
            palette_metadata: [0, palette_source],
            palette,
            secondary_exit_metadata: [0; 2],
            secondary_exits,
            exanimation_metadata: [0, exanimation_source],
            exanimation,
            expanded_settings: self.assets.expanded_settings.clone(),
        })
    }
}

fn export_header(
    project: &Project,
    level: usize,
) -> Result<MwlLevelHeaderSection, NativeLevelAssetsControllerError> {
    let vanilla = project
        .load_vanilla_main_entrance(level, lm_profile::smw_us_v1_vanilla_entrance_layout())
        .map_err(NativeLevelAssetsControllerError::MwlVanillaEntrance)?;
    let lfix3 = project
        .load_lfix3_level_fields(level, lm_profile::smw_us_v1_lfix3_level_fields_layout())
        .map_err(NativeLevelAssetsControllerError::MwlLfix3Fields)?;
    let mut header = MwlLevelHeaderSection([0; MwlLevelHeaderSection::ENCODED_LEN]);
    header.set_level_number(u16::try_from(level).map_err(|_| {
        NativeLevelAssetsControllerError::MwlTargetMismatch {
            expected: level,
            actual: u16::MAX,
        }
    })?);
    header.set_main_entrance(MwlMainEntranceSettings {
        position: vanilla.position,
        vertical_settings: vanilla.vertical_settings,
        screen_and_method: vanilla.screen_and_method,
        level_mode_and_screen: vanilla.level_mode_and_screen,
        flags: lfix3.flags,
        high_position: lfix3.high_position,
        additional_flags: lfix3.additional_flags,
    });
    if lfix3.flags & 0x20 != 0 {
        let midway = project
            .load_separate_midway_table(lm_profile::smw_us_v1_separate_midway_locator())
            .map_err(NativeLevelAssetsControllerError::MwlSeparateMidway)?
            .table
            .entries[level];
        header.set_midway_entrance(MwlMidwayEntranceSettings {
            position: midway.position,
            flags: midway.flags,
            high_position: midway.high_position,
            additional_flags: midway.additional_flags,
        });
    }
    header.0[16] = project
        .load_expanded_level_mode(level, lm_profile::smw_us_v1_expanded_level_mode_locator())
        .map_err(NativeLevelAssetsControllerError::MwlExpandedLevelMode)?;
    header.0[17] = lfix3.runtime_flags;
    Ok(header)
}

fn derived_level_mode_high_bit(level_mode: u8, vertical_settings: u8, layer2_pointer: u32) -> u8 {
    if level_mode_layer2_storage(level_mode) == Layer2Storage::Objects
        || vertical_settings >> 6 == 1
        || (vertical_settings >> 6 == 2 && layer2_pointer != 0xff_e103)
    {
        0x80
    } else {
        0
    }
}

fn export_secondary_exits(
    project: &Project,
    level: usize,
) -> Result<Vec<MwlSecondaryExit>, NativeLevelAssetsControllerError> {
    let table = project
        .load_secondary_exit_table_detected(lm_profile::smw_us_v1_secondary_exit_locator())
        .map_err(NativeLevelAssetsControllerError::MwlSecondaryExits)?
        .table;
    Ok(table
        .entries
        .into_iter()
        .enumerate()
        .filter(|(index, exit)| secondary_exit_is_exported(*index, exit, level))
        .map(|(index, exit)| MwlSecondaryExit {
            index: u16::try_from(index).expect("secondary-exit table has exactly 0x2000 entries"),
            exit,
            reserved: 0,
        })
        .collect())
}

fn secondary_exit_is_exported(index: usize, exit: &lm_level::SecondaryExit, level: usize) -> bool {
    if usize::from(exit.destination_level) != level {
        return false;
    }
    if !matches!(level, 0 | 0x100) || index >= 0x2000 {
        return true;
    }
    exit.destination_level != 0
        || exit.x_and_overworld_flags & 0x80 != 0
        || exit.destination_flags & 0x40 != 0
        || exit.position_and_method & 0x0f != 0
        || exit.screen != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ControllerSnapshot, EditorMode};
    use lm_graphics::PaletteOwnership;
    use lm_project::{ExAnimationRomLayout, LevelPointerTable, NativeLevelAssetsLayout};
    use lm_rom::{Mapper, detect_identity};

    #[test]
    fn derived_mode_bit_matches_all_retained_lunar_magic_headers() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for slot in 0..0x200 {
            let bytes = std::fs::read(root.join(format!(
                "oracle-work/lm363/pristine-us/levels/Level {slot:03X}.mwl"
            )))
            .unwrap();
            let file = MwlFile::decode(&bytes).unwrap();
            let semantic = MwlNativeLevel::decode(
                &file,
                &lm_level::SpriteLengthTable::standard(),
                32,
                &[false; 256],
            )
            .unwrap();
            assert_eq!(
                semantic.header.0[16] & 0x80,
                derived_level_mode_high_bit(
                    semantic.layer1.header.level_mode(),
                    semantic.header.0[3],
                    semantic.layer2_source_address,
                ),
                "slot {slot:03X}"
            );
        }
    }

    #[test]
    fn installed_level_zero_export_matches_lunar_magic_semantics_and_provenance() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rom_bytes = std::fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let image = RomImage::from_bytes(rom_bytes.clone()).unwrap();
        let snapshot = ControllerSnapshot {
            revision: 11,
            mode: EditorMode::Level(0),
            identity: detect_identity(&image).unwrap(),
            document_path: None,
            rom_bytes,
        };
        let mut level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        level_layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
        let layout = NativeLevelAssetsLayout {
            level: level_layout,
            palette: lm_profile::smw_us_v1_custom_palette_layout(),
            exanimation: ExAnimationRomLayout {
                mapper: Mapper::LoRom,
                pointers: LevelPointerTable {
                    offset: 0x8138b,
                    entries: 0x200,
                    stride: 3,
                },
                maximum_records: 32,
                maximum_encoded_len: 0x8000,
            },
            expanded_settings: Some(lm_profile::smw_us_v1_expanded_settings_layout()),
        };
        let controller = NativeLevelAssetsController::decode_with_layer2(
            &snapshot,
            layout,
            Some(lm_profile::smw_us_v1_layer2_layout(&image).unwrap()),
            &lm_level::SpriteLengthTable::standard(),
            &[false; 256],
            PaletteOwnership::editable(257),
        )
        .unwrap();
        let actual = controller.export_smw_us_v1_installed_mwl().unwrap();
        let expected_file = MwlFile::decode(
            &std::fs::read(root.join(
                "oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/exported/Level 000.mwl",
            ))
            .unwrap(),
        )
        .unwrap();
        let expected = MwlNativeLevel::decode(
            &expected_file,
            &lm_level::SpriteLengthTable::standard(),
            32,
            &[false; 256],
        )
        .unwrap();
        assert_eq!(actual.header, expected.header);
        assert_eq!(actual.layer1_metadata, expected.layer1_metadata);
        assert_eq!(actual.layer1, expected.layer1);
        assert_eq!(actual.layer2_descriptor, expected.layer2_descriptor);
        assert_eq!(actual.layer2_source_address, expected.layer2_source_address);
        assert_eq!(actual.layer2, expected.layer2);
        assert_eq!(actual.sprite_metadata, expected.sprite_metadata);
        assert_eq!(actual.sprites, expected.sprites);
        assert_eq!(actual.palette_metadata, expected.palette_metadata);
        assert_eq!(actual.palette, expected.palette);
        assert_eq!(actual.secondary_exits, expected.secondary_exits);
        assert_eq!(actual.exanimation_metadata, expected.exanimation_metadata);
        assert_eq!(actual.exanimation, expected.exanimation);
        assert_eq!(actual.expanded_settings, expected.expanded_settings);
    }
}
