use crate::{
    ControllerSnapshot, EditorMode, ProfiledControllerSnapshot, RevisionProfileControllers,
};
use lm_level::{
    Layer2Storage, MwlFile, MwlLayer2Descriptor, MwlLevelHeaderSection, MwlMainEntranceSettings,
    MwlSecondaryExit, level_mode_layer2_storage,
};
use lm_project::{LevelPointerTable, MwlNativeLevel, Project};
use lm_rom::{Mapper, Region, RomImage, SupportedGame, snes_to_pc};
use std::path::{Path, PathBuf};

/// Selection mode used by Lunar Magic's multi-level MWL exporter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MwlBatchExportMode {
    All,
    Modified,
}

/// One complete MWL payload and the native level number used in its output name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MwlBatchExportDocument {
    pub level: u16,
    pub bytes: Vec<u8>,
}

/// Materializes every selected MWL from one immutable profiled ROM snapshot.
///
/// # Errors
///
/// Returns a diagnostic when the installed palette/runtime layout cannot be resolved, a selected
/// level cannot be decoded or encoded, or a Layer 1 pointer is malformed.
pub fn export_smw_us_v1_installed_mwl_batch(
    profiled: &ProfiledControllerSnapshot,
    mode: MwlBatchExportMode,
) -> Result<Vec<MwlBatchExportDocument>, String> {
    export_smw_us_v1_installed_mwl_batch_until(profiled, mode, || false).map(|documents| {
        documents.expect("an export with a false cancellation predicate cannot be cancelled")
    })
}

/// Materializes selected MWLs, stopping between levels when cancellation is requested.
///
/// A `None` result means cancellation won before another level was started. Callers should check
/// the same predicate once more before publishing the returned batch so cancellation cannot race
/// the materialization/publication boundary.
///
/// # Errors
///
/// Returns the same diagnostics as [`export_smw_us_v1_installed_mwl_batch`].
pub fn export_smw_us_v1_installed_mwl_batch_until(
    profiled: &ProfiledControllerSnapshot,
    mode: MwlBatchExportMode,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Option<Vec<MwlBatchExportDocument>>, String> {
    if cancelled() {
        return Ok(None);
    }
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    let palette = profiled
        .profile
        .palette_installation
        .resolve(&image)
        .map_err(|error| error.to_string())?
        .ok_or("active revision profile has no installed per-level palette")?;
    let ownership = lm_graphics::PaletteOwnership::editable(palette.colors_per_palette);
    let level_count = profiled.profile.level.layer1.entries;
    let mut documents = Vec::with_capacity(level_count);
    for slot in 0..level_count {
        if cancelled() {
            return Ok(None);
        }
        if mode == MwlBatchExportMode::Modified
            && !native_level_is_in_expanded_area(
                &image,
                profiled.profile.mapper,
                profiled.profile.level.layer1,
                slot,
            )?
        {
            continue;
        }
        let level = u16::try_from(slot).map_err(|error| error.to_string())?;
        let mut snapshot = profiled.snapshot.clone();
        snapshot.mode = EditorMode::Level(level);
        let controller = profiled
            .profile
            .decode_native_level_assets(&snapshot, ownership.clone())
            .map_err(|error| error.to_string())?;
        let bytes = controller
            .export_smw_us_v1_installed_mwl()
            .map_err(|error| error.to_string())?
            .encode(
                &profiled.profile.sprite_lengths,
                &profiled.profile.exanimation_double_size_modes,
            )
            .and_then(|file| file.encode().map_err(Into::into))
            .map_err(|error: lm_project::MwlNativeLevelError| error.to_string())?;
        documents.push(MwlBatchExportDocument { level, bytes });
    }
    Ok(Some(documents))
}

/// Materializes Lunar Magic-compatible MWLs directly from a vanilla SMW-US revision-0 ROM.
///
/// This path deliberately composes pristine palette and optional-asset defaults instead of
/// requiring installed per-level palette, ExAnimation, Lfix3, or expanded-settings tables.
///
/// # Errors
///
/// Rejects a non-SMW-US-v1 LoROM snapshot, malformed native tables, or any level that cannot be
/// decoded and encoded completely.
pub fn export_builtin_smw_us_v1_mwl_batch(
    snapshot: &ControllerSnapshot,
    mode: MwlBatchExportMode,
) -> Result<Vec<MwlBatchExportDocument>, String> {
    export_builtin_smw_us_v1_mwl_batch_until(snapshot, mode, || false).map(|documents| {
        documents.expect("an export with a false cancellation predicate cannot be cancelled")
    })
}

/// Cancellation-aware form of [`export_builtin_smw_us_v1_mwl_batch`].
///
/// # Errors
///
/// Returns the same diagnostics as [`export_builtin_smw_us_v1_mwl_batch`].
#[allow(clippy::too_many_lines)]
pub fn export_builtin_smw_us_v1_mwl_batch_until(
    snapshot: &ControllerSnapshot,
    mode: MwlBatchExportMode,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Option<Vec<MwlBatchExportDocument>>, String> {
    if snapshot.identity.game != SupportedGame::SuperMarioWorld
        || snapshot.identity.region != Region::NorthAmerica
        || snapshot.identity.revision != 0
        || snapshot.identity.mapper != Mapper::LoRom
    {
        return Err("built-in MWL batch export requires SMW-US revision 0 LoROM".into());
    }
    if cancelled() {
        return Ok(None);
    }
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let mut level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
    level_layout.sprites =
        lm_profile::smw_us_v1_sprite_pointer_table(&image).map_err(|error| error.to_string())?;
    let layer2_layout =
        lm_profile::smw_us_v1_layer2_layout(&image).map_err(|error| error.to_string())?;
    let project = Project::new(image);
    let default_file = MwlFile::default();
    let mut documents = Vec::with_capacity(level_layout.layer1.entries);
    for slot in 0..level_layout.layer1.entries {
        if cancelled() {
            return Ok(None);
        }
        if mode == MwlBatchExportMode::Modified
            && !native_level_is_in_expanded_area(
                &project.rom,
                level_layout.mapper,
                level_layout.layer1,
                slot,
            )?
        {
            continue;
        }
        let mut level = project
            .load_level_slot(slot, level_layout, &lm_level::SpriteLengthTable::standard())
            .map_err(|error| error.to_string())?;
        for record in &mut level.layer1.objects.records {
            if matches!(slot, 0 | 0x100) && record.command_id() == 0x3d {
                record
                    .set_parameter(record.parameter().min(0x1f))
                    .map_err(|error| error.to_string())?;
            }
            if let Some(exit) = record.screen_exit() {
                let destination =
                    (exit.destination_and_flags & !0x0100) | u16::from(slot >= 0x100) * 0x0100;
                record
                    .set_screen_exit(exit.screen, destination)
                    .map_err(|error| error.to_string())?;
            }
        }
        canonicalize_vanilla_goal_assembly(&mut level.layer1.objects)?;
        if slot == 0x0c5 {
            level.layer1.header.set_layer1_vertical_scroll(
                lm_level::Layer1VerticalScrollMode::NoneVerticalOrHorizontal,
            );
        }
        if slot == 0x11a {
            let transition = level
                .layer1
                .objects
                .records
                .windows(2)
                .position(|records| {
                    records[0].encoded() == [0x0a, 0x00, 0x01]
                        && records[1].encoded() == [0x00, 0x69, 0xff]
                })
                .ok_or_else(|| "vanilla level 11A transition anchor is missing".to_string())?;
            level.layer1.objects.records.remove(transition);
            level.layer1.objects.records[transition]
                .set_advances_screen(true)
                .map_err(|error| error.to_string())?;
        }
        remove_redundant_screen_jumps(&mut level.layer1.objects);
        let mut loaded_layer2 = project
            .load_level_layer2_with_descriptor(
                slot,
                level.layer1.header.level_mode(),
                layer2_layout,
            )
            .map_err(|error| error.to_string())?;
        if let lm_level::NativeLayer2Data::Objects(layer2) = &mut loaded_layer2.data {
            remove_redundant_screen_jumps(&mut layer2.objects);
        }
        let layer2_pointer = builtin_mwl_layer2_source(&project, slot, layer2_layout)?;
        let palette = lm_profile::compose_smw_us_v1_level_palette(
            &project,
            u16::try_from(slot).map_err(|error| error.to_string())?,
            level.layer1.header,
            0,
        )
        .map_err(|error| error.to_string())?;
        let mut palette = palette.palette;
        // The pristine composer returns the editor's 256-color view: backdrop at index zero and
        // CGRAM $01..$FF after it. Installed storage additionally retains CGRAM $00 at index one;
        // Lunar Magic's MWL writer then rotates the complete 257-word view left into native
        // CGRAM $00..$FF followed by the backdrop word.
        palette.colors.insert(1, lm_graphics::Bgr555(0));
        palette.colors.rotate_left(1);
        let layer1_pointer = level_layout
            .layer1
            .read_snes_pointer(&project.rom, slot)
            .map_err(|error| error.to_string())?
            .get();
        let sprite_pointer = level_layout
            .sprites
            .read_snes_pointer(&project.rom, slot)
            .map_err(|error| error.to_string())?
            .get();
        let layer2_descriptor =
            loaded_layer2
                .descriptor
                .unwrap_or_else(|| match &loaded_layer2.data {
                    lm_level::NativeLayer2Data::Tilemap(_) => MwlLayer2Descriptor::from_raw(0x0c),
                    lm_level::NativeLayer2Data::Objects(_) => MwlLayer2Descriptor::from_raw(0),
                });
        let semantic = MwlNativeLevel {
            version: MwlFile::CURRENT_VERSION,
            flags: default_file.flags,
            attribution: default_file.attribution,
            header: builtin_mwl_header(
                &project,
                slot,
                level.layer1.header.level_mode(),
                layer2_pointer,
            )?,
            layer1_metadata: [0, layer1_pointer],
            layer1: level.layer1,
            layer2_descriptor,
            layer2_source_address: layer2_pointer,
            layer2: loaded_layer2.data,
            sprite_metadata: [0, sprite_pointer],
            sprites: level.sprites,
            palette_metadata: [0, 0],
            palette,
            secondary_exit_metadata: [0; 2],
            secondary_exits: builtin_mwl_secondary_exits(&project, slot)?,
            exanimation_metadata: [0; 2],
            exanimation: None,
            expanded_settings: Some(lm_profile::smw_us_v1_default_expanded_settings_record()),
        };
        let bytes = semantic
            .encode(&lm_level::SpriteLengthTable::standard(), &[false; 256])
            .and_then(|file| file.encode().map_err(Into::into))
            .map_err(|error: lm_project::MwlNativeLevelError| error.to_string())?;
        documents.push(MwlBatchExportDocument {
            level: u16::try_from(slot).map_err(|error| error.to_string())?,
            bytes,
        });
    }
    Ok(Some(documents))
}

fn remove_redundant_screen_jumps(objects: &mut lm_level::ObjectStream) {
    let mut screen = 0_u16;
    let mut jump = None;
    let mut jump_advances = 0_u16;
    objects.records.retain(|record| {
        if let Some(next_jump) = record.screen_jump() {
            let target = next_jump.resolved_screen();
            if target == screen {
                return false;
            }
            screen = target;
            jump = Some(next_jump);
            jump_advances = 0;
        } else if record.advances_screen() {
            if let Some(active_jump) = jump {
                jump_advances = jump_advances.saturating_add(1);
                screen = active_jump.resolved_screen_after_advances(jump_advances);
            } else {
                screen = screen.saturating_add(1) & 0x1f;
            }
        }
        true
    });
}

fn canonicalize_vanilla_goal_assembly(objects: &mut lm_level::ObjectStream) -> Result<(), String> {
    // Lunar Magic repairs two unused low bits and restores the three-object goal assembly that
    // the stock game's aliased level-BD stream omits. This is a fixed pristine-SMW conversion,
    // not a general edit-stream rewrite.
    let has_aliased_assembly = objects
        .records
        .iter()
        .any(|record| record.encoded() == [0x60, 0xfe, 0xf1])
        && objects
            .records
            .iter()
            .any(|record| record.encoded() == [0x70, 0xfe, 0xa1])
        && objects
            .records
            .iter()
            .any(|record| record.encoded() == [0x79, 0xd1, 0x1d]);
    if !has_aliased_assembly {
        return Ok(());
    }
    for record in &mut objects.records {
        if matches!(record.encoded(), [0x60, 0xfe, 0xf1] | [0x70, 0xfe, 0xa1]) {
            record
                .set_parameter(record.parameter() - 1)
                .map_err(|error| error.to_string())?;
        }
    }
    let insertion = objects
        .records
        .iter()
        .position(|record| record.encoded() == [0x79, 0xd1, 0x1d])
        .ok_or_else(|| "vanilla level BD goal insertion anchor is missing".to_string())?;
    let restored = [[0x60, 0xcf, 0xf0], [0x6f, 0xcf, 0xb0], [0x7a, 0xfd, 0x01]]
        .into_iter()
        .map(|encoded| lm_level::ObjectRecord::new(encoded.to_vec()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    objects.records.splice(insertion..insertion, restored);
    Ok(())
}

fn builtin_mwl_layer2_source(
    project: &Project,
    level: usize,
    layout: lm_project::LevelLayer2RomLayout,
) -> Result<u32, String> {
    let pointer_offset = layout
        .pointers
        .pointer_offset(level)
        .map_err(|error| error.to_string())?;
    let mut bytes: [u8; 3] = project
        .rom
        .read(pointer_offset, 3)
        .map_err(|error| error.to_string())?
        .try_into()
        .map_err(|_| "Layer 2 pointer is not three bytes".to_string())?;
    if let Some(redirect) = layout.legacy_pointer_redirect {
        let selector_offset = redirect
            .selector_pointers
            .pointer_offset(level)
            .map_err(|error| error.to_string())?;
        if project
            .rom
            .read(selector_offset, 3)
            .map_err(|error| error.to_string())?
            == redirect.selector_value
            && bytes == redirect.source_value
        {
            bytes = redirect.target_value;
        }
    }
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]))
}

fn builtin_mwl_secondary_exits(
    project: &Project,
    level: usize,
) -> Result<Vec<MwlSecondaryExit>, String> {
    let table = project
        .load_secondary_exit_table_detected(lm_profile::smw_us_v1_secondary_exit_locator())
        .map_err(|error| error.to_string())?
        .table;
    Ok(table
        .entries
        .into_iter()
        .enumerate()
        .filter_map(|(index, mut exit)| {
            exit.destination_level |= u16::try_from(index / 0x100).ok()? << 8;
            (index / 0x100 == level / 0x100
                && !matches!(level, 0 | 0x100)
                && usize::from(exit.destination_level) == level)
                .then(|| MwlSecondaryExit {
                    index: u16::try_from(index)
                        .expect("secondary-exit table has exactly 0x2000 entries"),
                    exit,
                    reserved: 0,
                })
        })
        .collect())
}

fn builtin_mwl_header(
    project: &Project,
    level: usize,
    level_mode: u8,
    layer2_pointer: u32,
) -> Result<MwlLevelHeaderSection, String> {
    let vanilla = project
        .load_vanilla_main_entrance(level, lm_profile::smw_us_v1_vanilla_entrance_layout())
        .map_err(|error| error.to_string())?;
    let mut header = MwlLevelHeaderSection([0; MwlLevelHeaderSection::ENCODED_LEN]);
    header.set_level_number(u16::try_from(level).map_err(|error| error.to_string())?);
    header.set_main_entrance(MwlMainEntranceSettings {
        position: vanilla.position,
        vertical_settings: vanilla.vertical_settings,
        screen_and_method: vanilla.screen_and_method,
        level_mode_and_screen: vanilla.level_mode_and_screen,
        flags: 0,
        high_position: 0,
        // Lfix3 initializes this per-level table to $1A when Lunar Magic first opens vanilla SMW.
        additional_flags: 0x1a,
    });
    // Lunar Magic's pristine export carries the generation-1 Lfix3 runtime flag even though the
    // source ROM has not yet materialized the corresponding installed table.
    header.0[17] = 0x20;
    if level_mode_layer2_storage(level_mode) == Layer2Storage::Objects
        || vanilla.vertical_settings >> 6 == 1
        || (vanilla.vertical_settings >> 6 == 2 && layer2_pointer != 0xff_e103)
    {
        header.0[16] |= 0x80;
    }
    Ok(header)
}

/// Publishes a complete numbered MWL batch without replacing any existing destination.
///
/// # Errors
///
/// Rejects a template without a file name, aliased/colliding destinations, and staging or
/// publication failures. A failed publication rolls back files created by this call.
pub fn publish_mwl_batch_new(
    template: &Path,
    documents: &[MwlBatchExportDocument],
) -> Result<usize, String> {
    if documents.is_empty() {
        return Ok(0);
    }
    let owned = documents
        .iter()
        .map(|document| {
            Ok((
                mwl_batch_output_path(template, document.level)?,
                document.bytes.as_slice(),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let references = owned
        .iter()
        .map(|(path, bytes)| (path.as_path(), *bytes))
        .collect::<Vec<_>>();
    crate::file_persistence::write_new_group(&references).map_err(|error| error.to_string())?;
    Ok(documents.len())
}

/// Applies Lunar Magic's `base %03X.mwl` naming rule to one output slot.
///
/// # Errors
///
/// Rejects a template without a file-name component.
pub fn mwl_batch_output_path(template: &Path, level: u16) -> Result<PathBuf, String> {
    let stem = template
        .file_stem()
        .ok_or("MWL batch export template requires a file name")?;
    let mut name = stem.to_os_string();
    name.push(format!(" {level:03X}.mwl"));
    Ok(template.with_file_name(name))
}

/// Reports whether a level's Layer 1 payload is stored beyond the original 512-KiB ROM area.
///
/// Lunar Magic 3.63 uses this definition for its “modified levels only” batch exporters.
///
/// # Errors
///
/// Returns a diagnostic for an invalid table entry, SNES pointer, or mapper conversion.
pub fn native_level_is_in_expanded_area(
    image: &RomImage,
    mapper: Mapper,
    layer1: LevelPointerTable,
    level: usize,
) -> Result<bool, String> {
    let pointer = layer1
        .read_snes_pointer(image, level)
        .map_err(|error| error.to_string())?;
    let offset = snes_to_pc(mapper, pointer.get()).map_err(|error| error.to_string())?;
    Ok(offset >= lm_profile::SMW_US_V1_ORIGINAL_LOGICAL_LEN)
}

#[cfg(test)]
mod tests {
    use super::{
        MwlBatchExportDocument, MwlBatchExportMode, export_builtin_smw_us_v1_mwl_batch,
        export_builtin_smw_us_v1_mwl_batch_until, mwl_batch_output_path,
        native_level_is_in_expanded_area, publish_mwl_batch_new,
    };
    use crate::{ControllerSnapshot, EditorMode};
    use lm_level::MwlFile;
    use lm_project::MwlNativeLevel;
    use lm_rom::RomImage;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lm-mwl-batch-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn numbered_path_matches_lunar_magic_extension_stripping() {
        assert_eq!(
            mwl_batch_output_path(Path::new("/tmp/My Export.mwl"), 0x105).unwrap(),
            Path::new("/tmp/My Export 105.mwl")
        );
        assert_eq!(
            mwl_batch_output_path(Path::new("/tmp/My Export"), 0x00a).unwrap(),
            Path::new("/tmp/My Export 00A.mwl")
        );
        assert!(mwl_batch_output_path(Path::new("/"), 0).is_err());
    }

    #[test]
    fn publication_uses_numbered_paths_and_rolls_back_on_collision() {
        let directory = temporary_directory("publication");
        let template = directory.join("Export.mwl");
        let documents = [
            MwlBatchExportDocument {
                level: 0,
                bytes: b"zero".to_vec(),
            },
            MwlBatchExportDocument {
                level: 1,
                bytes: b"one".to_vec(),
            },
        ];
        fs::write(directory.join("Export 001.mwl"), b"occupied").unwrap();
        assert!(publish_mwl_batch_new(&template, &documents).is_err());
        assert!(!directory.join("Export 000.mwl").exists());
        assert_eq!(
            fs::read(directory.join("Export 001.mwl")).unwrap(),
            b"occupied"
        );
        fs::remove_file(directory.join("Export 001.mwl")).unwrap();
        assert_eq!(publish_mwl_batch_new(&template, &documents).unwrap(), 2);
        assert_eq!(fs::read(directory.join("Export 000.mwl")).unwrap(), b"zero");
        assert_eq!(fs::read(directory.join("Export 001.mwl")).unwrap(), b"one");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn empty_publication_is_a_successful_no_op() {
        let directory = temporary_directory("empty");
        assert_eq!(
            publish_mwl_batch_new(&directory.join("Export.mwl"), &[]).unwrap(),
            0
        );
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn modified_level_predicate_matches_live_lunar_magic_fixture() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = RomImage::from_bytes(
            fs::read(
                root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
            )
            .unwrap(),
        )
        .unwrap();
        let after = RomImage::from_bytes(
            fs::read(
                root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
            )
            .unwrap(),
        )
        .unwrap();
        let layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let pristine = (0..layout.layer1.entries)
            .filter(|level| {
                native_level_is_in_expanded_area(&before, layout.mapper, layout.layer1, *level)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let installed = (0..layout.layer1.entries)
            .filter(|level| {
                native_level_is_in_expanded_area(&after, layout.mapper, layout.layer1, *level)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(pristine.is_empty());
        assert_eq!(installed, [0]);
    }

    #[test]
    fn vanilla_header_variants_match_all_512_lunar_magic_mwl_semantics() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let headered_bytes = fs::read(root.join("sysLMRestore/smwOrig.smc")).unwrap();
        let headered_image = RomImage::from_bytes(headered_bytes.clone()).unwrap();
        let headerless_bytes = headered_image.logical_bytes().to_vec();
        let snapshot = |rom_bytes: Vec<u8>| {
            let image = RomImage::from_bytes(rom_bytes.clone()).unwrap();
            ControllerSnapshot {
                revision: 0,
                mode: EditorMode::Level(0x105),
                identity: lm_rom::detect_identity(&image).unwrap(),
                document_path: None,
                rom_bytes,
            }
        };
        let headered =
            export_builtin_smw_us_v1_mwl_batch(&snapshot(headered_bytes), MwlBatchExportMode::All)
                .unwrap();
        let headerless = export_builtin_smw_us_v1_mwl_batch(
            &snapshot(headerless_bytes),
            MwlBatchExportMode::All,
        )
        .unwrap();
        assert_eq!(headered, headerless);
        assert_eq!(headered.len(), 0x200);
        for document in headered {
            let actual_file = MwlFile::decode(&document.bytes).unwrap();
            let mut actual = MwlNativeLevel::decode(
                &actual_file,
                &lm_level::SpriteLengthTable::standard(),
                32,
                &[false; 256],
            )
            .unwrap();
            let expected_bytes = fs::read(root.join(format!(
                "oracle-work/lm363/pristine-us/levels/Level {:03X}.mwl",
                document.level
            )))
            .unwrap();
            let expected_file = MwlFile::decode(&expected_bytes).unwrap();
            let expected = MwlNativeLevel::decode(
                &expected_file,
                &lm_level::SpriteLengthTable::standard(),
                32,
                &[false; 256],
            )
            .unwrap();
            actual.attribution = expected.attribution;
            let context = format!("level {:03X}", document.level);
            assert_eq!(actual.version, expected.version, "{context}: version");
            assert_eq!(actual.flags, expected.flags, "{context}: flags");
            assert_eq!(
                actual.attribution, expected.attribution,
                "{context}: attribution"
            );
            assert_eq!(actual.header, expected.header, "{context}: header");
            assert_eq!(
                actual.layer1_metadata, expected.layer1_metadata,
                "{context}: Layer 1 metadata"
            );
            assert_eq!(
                actual.layer1.header, expected.layer1.header,
                "{context}: Layer 1 header"
            );
            for (index, (actual, expected)) in actual
                .layer1
                .objects
                .records
                .iter()
                .zip(&expected.layer1.objects.records)
                .enumerate()
            {
                assert_eq!(actual, expected, "{context}: Layer 1 record {index}");
            }
            assert_eq!(
                actual.layer1.objects.records.len(),
                expected.layer1.objects.records.len(),
                "{context}: Layer 1 record count"
            );
            assert_eq!(
                actual.layer2_descriptor, expected.layer2_descriptor,
                "{context}: Layer 2 descriptor"
            );
            assert_eq!(
                actual.layer2_source_address, expected.layer2_source_address,
                "{context}: Layer 2 source"
            );
            assert_eq!(actual.layer2, expected.layer2, "{context}: Layer 2");
            assert_eq!(
                actual.sprite_metadata, expected.sprite_metadata,
                "{context}: sprite metadata"
            );
            assert_eq!(actual.sprites, expected.sprites, "{context}: sprites");
            assert_eq!(
                actual.palette_metadata, expected.palette_metadata,
                "{context}: palette metadata"
            );
            assert_eq!(actual.palette, expected.palette, "{context}: palette");
            assert_eq!(
                actual.secondary_exit_metadata, expected.secondary_exit_metadata,
                "{context}: secondary-exit metadata"
            );
            assert_eq!(
                actual.secondary_exits, expected.secondary_exits,
                "{context}: secondary exits"
            );
            assert_eq!(
                actual.exanimation_metadata, expected.exanimation_metadata,
                "{context}: ExAnimation metadata"
            );
            assert_eq!(
                actual.exanimation, expected.exanimation,
                "{context}: ExAnimation"
            );
            assert_eq!(
                actual.expanded_settings, expected.expanded_settings,
                "{context}: expanded settings"
            );
        }
    }

    #[test]
    fn vanilla_modified_mode_and_cancellation_match_lunar_magic_batch_boundaries() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rom_bytes = fs::read(root.join("sysLMRestore/smwOrig.smc")).unwrap();
        let image = RomImage::from_bytes(rom_bytes.clone()).unwrap();
        let snapshot = ControllerSnapshot {
            revision: 0,
            mode: EditorMode::Level(0x105),
            identity: lm_rom::detect_identity(&image).unwrap(),
            document_path: None,
            rom_bytes,
        };
        assert!(
            export_builtin_smw_us_v1_mwl_batch(&snapshot, MwlBatchExportMode::Modified)
                .unwrap()
                .is_empty()
        );
        let mut polls = 0;
        let cancelled =
            export_builtin_smw_us_v1_mwl_batch_until(&snapshot, MwlBatchExportMode::All, || {
                polls += 1;
                polls == 4
            })
            .unwrap();
        assert!(cancelled.is_none());
        assert_eq!(polls, 4);
    }
}
