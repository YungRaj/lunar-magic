use lm_app::{Command, ControllerSnapshot};
use lm_project::{Project, RomMutation, RomWrite};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};

const SMW_NA_PATCHES: [usize; 2] = [0x1cd3, 0x1b1c];
const SMW_J_PATCHES: [usize; 2] = [0x1c6d, 0x1ab1];
const ALL_STARS_WORLD_PATCHES: [usize; 2] = [0x181cec, 0x181afa];

fn patch_offsets(snapshot: &ControllerSnapshot) -> [usize; 2] {
    let base = match (snapshot.identity.game, snapshot.identity.region) {
        (SupportedGame::SuperMarioWorld, Region::NorthAmerica) => SMW_NA_PATCHES,
        (SupportedGame::SuperMarioWorld, Region::Japan) => SMW_J_PATCHES,
        (SupportedGame::AllStarsAndWorld, _) => ALL_STARS_WORLD_PATCHES,
    };
    if snapshot.identity.mapper == Mapper::ExLoRom {
        base.map(|offset| offset + 0x40_0000)
    } else {
        base
    }
}

const fn patch_bytes(mapper: Mapper) -> [u8; 3] {
    [
        0xad,
        0x01,
        if matches!(mapper, Mapper::Sa1) {
            0x67
        } else {
            0x07
        },
    ]
}

/// Composes Lunar Magic's session-only Level `$0C7` menu-color patch option with one level save.
///
/// The original option never removes an existing patch. When enabled, saving level `$0C7` writes
/// `LDA $0701` at both descriptor-selected sites (`$6701` under SA-1 RAM remapping), then repairs
/// the checksum as part of the same revision-bound mutation.
pub(crate) fn prepare_level_save_command(
    snapshot: &ControllerSnapshot,
    enabled: bool,
    command: Command,
) -> Result<Command, String> {
    if !enabled || snapshot.mode != lm_app::EditorMode::Level(0x0c7) {
        return Ok(command);
    }
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let (expected_revision, description, mutation) = match command {
        Command::CommitRomMutation {
            expected_revision,
            description,
            mutation,
        } => (expected_revision, description, mutation),
        Command::CommitRomWrites {
            expected_revision,
            description,
            writes,
        } => (
            expected_revision,
            description,
            RomMutation {
                mapper: snapshot.identity.mapper,
                expected_len: image.logical_len(),
                appended: Vec::new(),
                writes,
            },
        ),
        _ => return Err("menu-color fix can only be applied by a level ROM commit".into()),
    };
    if expected_revision != snapshot.revision {
        return Err(format!(
            "stale level save revision: expected {expected_revision}, snapshot is {}",
            snapshot.revision
        ));
    }

    let original = image.logical_bytes().to_vec();
    let mut staged = Project::new(image);
    staged
        .apply_mutation("stage level save with menu-color fix", &mutation)
        .map_err(|error| error.to_string())?;
    let bytes = patch_bytes(snapshot.identity.mapper);
    let writes = patch_offsets(snapshot).map(|offset| RomWrite {
        offset,
        bytes: bytes.to_vec(),
    });
    staged
        .apply_writes("Apply Level C7 menu-color fix", &writes)
        .map_err(|error| error.to_string())?;
    staged
        .refresh_checksum(snapshot.identity.internal_header_offset + 0x1c)
        .map_err(|error| error.to_string())?;
    let combined = RomMutation::between(
        snapshot.identity.mapper,
        &original,
        staged.rom.logical_bytes(),
    )
    .map_err(|error| error.to_string())?;
    Ok(Command::CommitRomMutation {
        expected_revision,
        description: format!("{description} and apply Level C7 menu-color fix"),
        mutation: combined,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(level: u16) -> ControllerSnapshot {
        let rom_bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(rom_bytes.clone()).unwrap();
        ControllerSnapshot {
            revision: 7,
            mode: lm_app::EditorMode::Level(level),
            identity: lm_rom::detect_identity(&image).unwrap(),
            document_path: None,
            rom_bytes,
        }
    }

    fn unchanged(snapshot: &ControllerSnapshot) -> Command {
        Command::CommitRomMutation {
            expected_revision: snapshot.revision,
            description: "save test level".into(),
            mutation: RomMutation::unchanged(
                snapshot.identity.mapper,
                RomImage::from_bytes(snapshot.rom_bytes.clone())
                    .unwrap()
                    .logical_len(),
            ),
        }
    }

    #[test]
    fn enabled_level_c7_save_patches_both_sites_and_repairs_checksum() {
        let source = snapshot(0x0c7);
        let command = prepare_level_save_command(&source, true, unchanged(&source)).unwrap();
        let Command::CommitRomMutation { mutation, .. } = command else {
            panic!("expected one combined mutation");
        };
        let original = source.rom_bytes.clone();
        let mut project = Project::new(RomImage::from_bytes(source.rom_bytes).unwrap());
        project.apply_mutation("apply", &mutation).unwrap();
        for offset in SMW_NA_PATCHES {
            assert_eq!(project.rom.read(offset, 3).unwrap(), [0xad, 0x01, 0x07]);
        }
        assert!(
            lm_rom::detect_identity(&project.rom)
                .unwrap()
                .checksum_matches()
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.logical_bytes(), original);
    }

    #[test]
    fn disabled_or_non_c7_save_is_byte_for_byte_unchanged() {
        for (level, enabled) in [(0x0c7, false), (0x0c6, true)] {
            let source = snapshot(level);
            assert_eq!(
                prepare_level_save_command(&source, enabled, unchanged(&source)).unwrap(),
                unchanged(&source)
            );
        }
    }

    #[test]
    fn descriptor_routes_region_game_and_mapper() {
        let mut source = snapshot(0x0c7);
        source.identity.region = Region::Japan;
        assert_eq!(patch_offsets(&source), SMW_J_PATCHES);
        source.identity.game = SupportedGame::AllStarsAndWorld;
        assert_eq!(patch_offsets(&source), ALL_STARS_WORLD_PATCHES);
        source.identity.mapper = Mapper::ExLoRom;
        assert_eq!(
            patch_offsets(&source),
            ALL_STARS_WORLD_PATCHES.map(|offset| offset + 0x40_0000)
        );
        assert_eq!(patch_bytes(Mapper::LoRom), [0xad, 0x01, 0x07]);
        assert_eq!(patch_bytes(Mapper::ExLoRom), [0xad, 0x01, 0x07]);
        assert_eq!(patch_bytes(Mapper::Sa1), [0xad, 0x01, 0x67]);
    }
}
