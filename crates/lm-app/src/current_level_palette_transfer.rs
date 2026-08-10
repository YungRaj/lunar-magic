use crate::{ControllerSnapshot, EditorMode, PreparedRomCommit, RevisionProfile};
use lm_graphics::{Palette, SmwPaletteFile};
use lm_project::{PaletteSaveOptions, Project, RomMutation};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, SupportedGame};
use std::fmt;

#[derive(Debug)]
pub enum CurrentLevelPaletteTransferError {
    LevelNotSelected,
    UnsupportedRevision,
    UnsupportedPristineMapper(Mapper),
    Rom(String),
    Level(String),
    Palette(String),
    Installation(String),
    Allocation(String),
    Mutation(String),
    Verification(String),
}

impl fmt::Display for CurrentLevelPaletteTransferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "current-level palette transfer failed: {self:?}")
    }
}

impl std::error::Error for CurrentLevelPaletteTransferError {}

fn selected_level(snapshot: &ControllerSnapshot) -> Result<u16, CurrentLevelPaletteTransferError> {
    match snapshot.mode {
        EditorMode::Level(level) => Ok(level),
        _ => Err(CurrentLevelPaletteTransferError::LevelNotSelected),
    }
}

fn validate_profile(profile: &RevisionProfile) -> Result<(), CurrentLevelPaletteTransferError> {
    if profile.game != SupportedGame::SuperMarioWorld
        || profile.region != lm_rom::Region::NorthAmerica
        || profile.revision != 0
    {
        return Err(CurrentLevelPaletteTransferError::UnsupportedRevision);
    }
    Ok(())
}

fn pointer_slot_is_empty(
    project: &Project,
    level: u16,
    layout: lm_project::PaletteRomLayout,
) -> Result<bool, CurrentLevelPaletteTransferError> {
    let offset = layout
        .pointers
        .pointer_offset(usize::from(level))
        .map_err(|error| CurrentLevelPaletteTransferError::Palette(error.to_string()))?;
    project
        .rom
        .read(offset, layout.pointers.stride)
        .map(|bytes| bytes.iter().all(|byte| *byte == 0))
        .map_err(|error| CurrentLevelPaletteTransferError::Rom(error.to_string()))
}

fn composed_native_palette(
    project: &Project,
    profile: &RevisionProfile,
    level: u16,
) -> Result<Palette, CurrentLevelPaletteTransferError> {
    if profile.mapper != Mapper::LoRom {
        return Err(CurrentLevelPaletteTransferError::UnsupportedPristineMapper(
            profile.mapper,
        ));
    }
    let loaded = project
        .load_level_slot(usize::from(level), profile.level, &profile.sprite_lengths)
        .map_err(|error| CurrentLevelPaletteTransferError::Level(error.to_string()))?;
    let composed =
        lm_profile::compose_smw_us_v1_level_palette(project, level, loaded.layer1.header, 0)
            .map_err(|error| CurrentLevelPaletteTransferError::Palette(error.to_string()))?;
    let mut native = composed.palette;
    native.colors.insert(1, lm_graphics::Bgr555(0));
    native.colors.rotate_left(1);
    Ok(native)
}

/// Loads the exact 257-color native current-level palette. Untouched slots are composed from
/// vanilla shared tables and the active level header.
pub fn load_current_level_native_palette(
    snapshot: &ControllerSnapshot,
    profile: &RevisionProfile,
) -> Result<Palette, CurrentLevelPaletteTransferError> {
    validate_profile(profile)?;
    let level = selected_level(snapshot)?;
    let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
        .map_err(|error| CurrentLevelPaletteTransferError::Rom(error.to_string()))?;
    let project = Project::new(image);
    let installation = lm_profile::smw_us_v1_custom_palette_installation_for_mapper(profile.mapper);
    if let Some(layout) = installation
        .resolve(&project.rom)
        .map_err(|error| CurrentLevelPaletteTransferError::Installation(error.to_string()))?
        && !pointer_slot_is_empty(&project, level, layout)?
    {
        return project
            .load_palette(usize::from(level), layout)
            .map_err(|error| CurrentLevelPaletteTransferError::Palette(error.to_string()));
    }
    composed_native_palette(&project, profile, level)
}

/// Builds one revision-checked mutation which installs the runtime if necessary, saves the
/// imported palette, refreshes the checksum, and verifies an exact save/reopen round trip.
pub fn prepare_current_level_palette_import(
    snapshot: &ControllerSnapshot,
    profile: &RevisionProfile,
    palette: &Palette,
) -> Result<PreparedRomCommit, CurrentLevelPaletteTransferError> {
    validate_profile(profile)?;
    let level = selected_level(snapshot)?;
    if palette.colors.len() != lm_profile::SMW_US_V1_CUSTOM_PALETTE_COLORS {
        return Err(CurrentLevelPaletteTransferError::Palette(format!(
            "expected {} native colors, got {}",
            lm_profile::SMW_US_V1_CUSTOM_PALETTE_COLORS,
            palette.colors.len()
        )));
    }
    let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
        .map_err(|error| CurrentLevelPaletteTransferError::Rom(error.to_string()))?;
    let before = image.logical_bytes().to_vec();
    let mut project = Project::new(image);
    let installation = lm_profile::smw_us_v1_custom_palette_installation_for_mapper(profile.mapper);
    if installation
        .resolve(&project.rom)
        .map_err(|error| CurrentLevelPaletteTransferError::Installation(error.to_string()))?
        .is_none()
    {
        let shared_layout = lm_profile::smw_us_v1_shared_palette_layout_for_mapper(profile.mapper);
        let expected = project
            .rom
            .read(
                shared_layout.table_offset,
                SmwPaletteFile::EXPANDED_FILE_LEN,
            )
            .map_err(|error| CurrentLevelPaletteTransferError::Rom(error.to_string()))?
            .to_vec();
        let shared = SmwPaletteFile::expanded(expected[0x10..].to_vec(), expected[..0x10].to_vec())
            .map_err(|error| CurrentLevelPaletteTransferError::Palette(error.to_string()))?;
        let plan = lm_profile::smw_us_v1_expanded_shared_palette_installation_plan_for_mapper(
            &shared,
            &expected,
            profile.mapper,
        )
        .map_err(|error| CurrentLevelPaletteTransferError::Installation(error.to_string()))?;
        project
            .install_relocatable_patch(&plan)
            .map_err(|error| CurrentLevelPaletteTransferError::Installation(error.to_string()))?;
    }
    let layout = installation
        .resolve(&project.rom)
        .map_err(|error| CurrentLevelPaletteTransferError::Installation(error.to_string()))?
        .ok_or_else(|| {
            CurrentLevelPaletteTransferError::Installation(
                "custom-palette marker did not resolve after installation".into(),
            )
        })?;
    let search_start = if profile.mapper == Mapper::ExLoRom {
        0x40_0000
    } else {
        0x4_0000
    };
    let search_end = project.rom.logical_len();
    if search_start >= search_end {
        return Err(CurrentLevelPaletteTransferError::Allocation(format!(
            "ROM has no palette payload allocation window at {search_start:X}..{search_end:X}"
        )));
    }
    let mut allocation = AllocationPolicy {
        search: search_start..search_end,
        bank_size: Some(0x8000),
        fill_bytes: vec![0x00, 0xff],
        protected: vec![ProtectedRange(
            snapshot.identity.internal_header_offset
                ..snapshot.identity.internal_header_offset + 0x40,
        )],
    };
    let pointer_end = layout
        .pointers
        .offset
        .checked_add(layout.pointers.entries * layout.pointers.stride)
        .ok_or_else(|| {
            CurrentLevelPaletteTransferError::Allocation("pointer range overflow".into())
        })?;
    let protected = ProtectedRange(layout.pointers.offset..pointer_end);
    if !allocation.protected.contains(&protected) {
        allocation.protected.push(protected);
    }
    project
        .save_palette(
            usize::from(level),
            palette,
            layout,
            &PaletteSaveOptions {
                allocation,
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .map_err(|error| CurrentLevelPaletteTransferError::Palette(error.to_string()))?;
    project
        .refresh_checksum(snapshot.identity.internal_header_offset + 0x1c)
        .map_err(|error| CurrentLevelPaletteTransferError::Mutation(error.to_string()))?;
    let after = project.rom.logical_bytes().to_vec();
    let reopened = Project::new(
        RomImage::from_bytes(after.clone())
            .map_err(|error| CurrentLevelPaletteTransferError::Verification(error.to_string()))?,
    );
    let reopened_layout = installation
        .resolve(&reopened.rom)
        .map_err(|error| CurrentLevelPaletteTransferError::Verification(error.to_string()))?
        .ok_or_else(|| {
            CurrentLevelPaletteTransferError::Verification(
                "custom-palette runtime disappeared after reopen".into(),
            )
        })?;
    let actual = reopened
        .load_palette(usize::from(level), reopened_layout)
        .map_err(|error| CurrentLevelPaletteTransferError::Verification(error.to_string()))?;
    if actual != *palette {
        return Err(CurrentLevelPaletteTransferError::Verification(
            "reopened palette differs from the staged import".into(),
        ));
    }
    let mutation = RomMutation::between(profile.mapper, &before, &after)
        .map_err(|error| CurrentLevelPaletteTransferError::Mutation(error.to_string()))?;
    Ok(PreparedRomCommit {
        expected_revision: snapshot.revision,
        description: format!("Import native palette for level {level:03X}"),
        mutation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::pristine_smw_us_rom_bytes;
    use lm_graphics::Bgr555;
    use lm_level::SpriteLengthTable;
    use lm_rom::detect_identity;

    fn fixture() -> (ControllerSnapshot, RevisionProfile) {
        let bytes = pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
        let identity = detect_identity(&image).unwrap();
        let mut profile = lm_profile::test_support::profile();
        profile.game = identity.game;
        profile.region = identity.region;
        profile.revision = identity.revision;
        profile.mapper = identity.mapper;
        profile.level = lm_profile::smw_us_v1_vanilla_level_layout();
        profile.sprite_lengths = SpriteLengthTable::standard();
        (
            ControllerSnapshot {
                revision: 41,
                mode: EditorMode::Level(0x105),
                identity,
                document_path: None,
                rom_bytes: bytes,
            },
            profile,
        )
    }

    #[test]
    fn pristine_export_composes_exact_native_shape_without_mutating_rom() {
        let (snapshot, profile) = fixture();
        let before = snapshot.rom_bytes.clone();
        let palette = load_current_level_native_palette(&snapshot, &profile).unwrap();
        assert_eq!(palette.colors.len(), 257);
        let raw = lm_graphics::RawSnesPaletteFile {
            palette: palette.clone(),
        }
        .encode()
        .unwrap();
        assert_eq!(
            lm_oracle::sha256_hex(&raw),
            "8a50127cc38c0f39120687e3b4c2fa3067ded7dfbddf49c88a1d431003640c8f"
        );
        let supported = palette.colors[..256]
            .iter()
            .copied()
            .enumerate()
            .map(|(index, color)| {
                if index % 16 == 0 {
                    palette.colors[256]
                } else {
                    color
                }
            })
            .collect();
        let supported = Palette { colors: supported };
        let tpl = lm_graphics::TplPaletteFile {
            palette: supported.clone(),
        }
        .encode()
        .unwrap();
        let rgb = lm_graphics::RgbPaletteFile::from_snes_palette(
            &supported,
            lm_graphics::RgbChannelExpansion::HighBits,
        )
        .unwrap()
        .encode()
        .unwrap();
        assert_eq!(
            lm_oracle::sha256_hex(&tpl),
            "d4da32140cc2994b332e2bfd86579a7002868d692a4c6779ae99adedc6182201"
        );
        assert_eq!(
            lm_oracle::sha256_hex(&rgb),
            "88586ad377c5501476d93a820387c58312df9d05a64dd68af8f3131d71d10afa"
        );
        assert_eq!(snapshot.rom_bytes, before);
    }

    #[test]
    fn pristine_import_installs_saves_and_reopens_exactly() {
        let (snapshot, profile) = fixture();
        let mut expected = load_current_level_native_palette(&snapshot, &profile).unwrap();
        expected.colors[42] = Bgr555(0x1234);
        let prepared =
            prepare_current_level_palette_import(&snapshot, &profile, &expected).unwrap();
        assert_eq!(prepared.expected_revision, 41);
        assert!(!prepared.mutation.is_empty());

        let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap());
        assert!(
            project
                .apply_mutation("test current-level palette import", &prepared.mutation)
                .unwrap()
        );
        let mut reopened_snapshot = snapshot;
        reopened_snapshot.rom_bytes = project.rom.as_file_bytes().to_vec();
        assert_eq!(
            load_current_level_native_palette(&reopened_snapshot, &profile).unwrap(),
            expected
        );
    }

    #[test]
    fn malformed_palette_is_rejected_before_any_mutation() {
        let (snapshot, profile) = fixture();
        let malformed = Palette {
            colors: vec![Bgr555(7); 256],
        };
        assert!(matches!(
            prepare_current_level_palette_import(&snapshot, &profile, &malformed),
            Err(CurrentLevelPaletteTransferError::Palette(_))
        ));
    }
}
