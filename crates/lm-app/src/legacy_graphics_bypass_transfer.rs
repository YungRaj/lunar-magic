use crate::{ControllerSnapshot, PreparedRomCommit};
use lm_level::LegacyGraphicsBypassTable;
use lm_project::{Project, RomMutation};
use lm_rom::{Mapper, Region, RomImage, SupportedGame, compute_snes_checksum};

/// Encodes Lunar Magic's complete legacy `Bypass.lst` payload from one application snapshot.
///
/// The file is the table's exact `$400` stored bytes. It does not include a signature, level
/// selector, copier header, or ownership sidecar.
pub fn export_legacy_graphics_bypass_list(
    snapshot: &ControllerSnapshot,
) -> Result<[u8; LegacyGraphicsBypassTable::ENCODED_LEN], String> {
    require_supported_identity(snapshot)?;
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    lm_profile::load_smw_us_v1_legacy_graphics_bypass_table(&image)
        .map(|table| table.encode())
        .map_err(|error| error.to_string())
}

/// Prepares one atomic import of an exact legacy `Bypass.lst` payload.
///
/// Lunar Magic installs its expanded-settings prerequisite when the table is imported into a
/// pristine ROM. The temporary project follows that behavior before replacing the table so the
/// frontend publishes installation, table replacement, checksum repair, and any required ROM
/// expansion as one revision-bound mutation.
pub fn prepare_legacy_graphics_bypass_list_import(
    snapshot: &ControllerSnapshot,
    bytes: &[u8],
) -> Result<PreparedRomCommit, String> {
    require_supported_identity(snapshot)?;
    let expected = LegacyGraphicsBypassTable::decode(bytes).map_err(|error| error.to_string())?;
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let before = image.logical_bytes().to_vec();
    let mut project = Project::new(image);

    let settings = lm_profile::load_smw_us_v1_overworld_settings(&project)
        .map_err(|error| error.to_string())?;
    if !settings.installed {
        let plan = lm_profile::smw_us_v1_expanded_settings_installation_plan_for_rom(&project.rom)
            .map_err(|error| error.to_string())?;
        project
            .install_relocatable_patch_with_expansion_retry(
                &plan,
                lm_profile::SMW_US_V1_EXPANDED_SETTINGS_MAXIMUM_LOROM_LEN,
            )
            .map_err(|error| error.to_string())?;
    }

    project
        .rom
        .write(
            lm_profile::SMW_US_V1_LEGACY_GRAPHICS_BYPASS_TABLE_OFFSET,
            bytes,
        )
        .map_err(|error| error.to_string())?;
    let checksum_field = snapshot.identity.internal_header_offset + 0x1c;
    let checksum = compute_snes_checksum(project.rom.logical_bytes(), checksum_field)
        .map_err(|error| error.to_string())?;
    project
        .rom
        .write(checksum_field, &checksum.encoded())
        .map_err(|error| error.to_string())?;

    let reopened = lm_profile::load_smw_us_v1_legacy_graphics_bypass_table(&project.rom)
        .map_err(|error| error.to_string())?;
    if reopened != expected {
        return Err("legacy Bypass.lst table did not reopen exactly".into());
    }
    let stored = lm_rom::SnesChecksum::decode(project.rom.logical_bytes(), checksum_field)
        .map_err(|error| error.to_string())?;
    if stored
        != compute_snes_checksum(project.rom.logical_bytes(), checksum_field)
            .map_err(|error| error.to_string())?
    {
        return Err("legacy Bypass.lst import checksum did not reopen exactly".into());
    }

    let mutation = RomMutation::between(Mapper::LoRom, &before, project.rom.logical_bytes())
        .map_err(|error| error.to_string())?;
    Ok(PreparedRomCommit {
        expected_revision: snapshot.revision,
        description: "Insert old ExGFX bypass list".into(),
        mutation,
    })
}

fn require_supported_identity(snapshot: &ControllerSnapshot) -> Result<(), String> {
    if snapshot.identity.game == SupportedGame::SuperMarioWorld
        && snapshot.identity.region == Region::NorthAmerica
        && snapshot.identity.revision == 0
        && snapshot.identity.mapper == Mapper::LoRom
    {
        Ok(())
    } else {
        Err("legacy Bypass.lst transfer requires SMW-US revision 0 with LoROM mapping".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppState, Command};
    use lm_rom::{CopierHeader, RomImage};

    fn patterned_list() -> [u8; LegacyGraphicsBypassTable::ENCODED_LEN] {
        let mut bytes = [0; LegacyGraphicsBypassTable::ENCODED_LEN];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = ((index * 29 + index / 7) & 0xff) as u8;
        }
        bytes
    }

    #[test]
    fn pristine_import_installs_prerequisite_reopens_exports_and_undoes_atomically() {
        let source = crate::test_support::pristine_smw_us_rom_bytes();
        let list = patterned_list();
        let mut logical_results = Vec::new();
        for copier in [CopierHeader::Absent, CopierHeader::Present] {
            let mut image = RomImage::from_bytes(source.clone()).unwrap();
            if copier == CopierHeader::Present {
                image
                    .replace_copier_header_exact(None, Some(&[0x5a; 512]))
                    .unwrap();
            }
            let original = image.as_file_bytes().to_vec();
            let mut app = AppState::default();
            app.load_rom(original.clone()).unwrap();
            let prepared = prepare_legacy_graphics_bypass_list_import(
                &app.controller_snapshot().unwrap(),
                &list,
            )
            .unwrap();
            app.dispatch(prepared.into_command()).unwrap();

            assert_eq!(
                export_legacy_graphics_bypass_list(&app.controller_snapshot().unwrap()).unwrap(),
                list
            );
            assert!(
                lm_profile::load_smw_us_v1_overworld_settings(app.project().unwrap())
                    .unwrap()
                    .installed
            );
            assert_eq!(app.project().unwrap().history.undo_len(), 1);
            let after = app.project().unwrap().save_snapshot();
            app.dispatch(Command::Undo).unwrap();
            assert_eq!(app.project().unwrap().save_snapshot(), original);

            let logical = RomImage::from_bytes(after)
                .unwrap()
                .logical_bytes()
                .to_vec();
            if logical_results.is_empty() {
                logical_results = logical;
            } else {
                assert_eq!(logical_results, logical);
            }
        }
    }

    #[test]
    fn import_requires_exact_400_byte_payload_before_mutating() {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        for bytes in [vec![0; 0x3ff], vec![0; 0x401]] {
            assert!(prepare_legacy_graphics_bypass_list_import(&snapshot, &bytes).is_err());
        }
        assert_eq!(app.project_revision(), snapshot.revision);
        assert_eq!(app.project().unwrap().rom.logical_len(), 0x80_000);
    }
}
