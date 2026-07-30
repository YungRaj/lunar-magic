use super::*;
#[test]
fn parses_editor_overlay_normalization_and_observation() {
    assert_eq!(
        parse_from(&[
            "editor-overlay-file".into(),
            "input.lmovly".into(),
            "normalized.lmovly".into(),
            "overlays.obs".into(),
        ])
        .unwrap(),
        Command::EditorOverlayFile {
            input: "input.lmovly".into(),
            normalized_output: Some("normalized.lmovly".into()),
            observation: Some("overlays.obs".into()),
        }
    );
    assert_eq!(
        parse_from(&[
            "mwl-edit-layer3-settings".into(),
            "input.mwl".into(),
            "on".into(),
            "abc".into(),
            "2".into(),
            "3".into(),
            "output.mwl".into(),
        ])
        .unwrap(),
        Command::MwlEditLayer3Settings {
            input: "input.mwl".into(),
            enabled: true,
            file: 0xabc,
            length_selector: 2,
            offset_selector: 3,
            output: "output.mwl".into(),
        }
    );
    assert!(parse_from(&["editor-overlay-file".into()]).is_err());
    assert!(
        parse_from(&[
            "editor-overlay-file".into(),
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
        ])
        .is_err()
    );
}

#[test]
fn parses_mwl_normalization_workflow() {
    assert_eq!(
        parse_from(&["mwl-corpus".into(), "levels".into()]).unwrap(),
        Command::MwlCorpus {
            root: "levels".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "mwl-normalize".into(),
            "input.mwl".into(),
            "normalized.mwl".into(),
        ])
        .unwrap(),
        Command::MwlNormalize {
            input: "input.mwl".into(),
            output: "normalized.mwl".into(),
        }
    );
    assert_eq!(
        parse_from(&["mwl-observe".into(), "input.mwl".into(), "level.obs".into(),]).unwrap(),
        Command::MwlObserve {
            input: "input.mwl".into(),
            output: "level.obs".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "mwl-observe-optional-assets".into(),
            "input.mwl".into(),
            "modes.bin".into(),
            "20".into(),
            "optional.obs".into(),
        ])
        .unwrap(),
        Command::MwlObserveOptionalAssets {
            input: "input.mwl".into(),
            size_modes: "modes.bin".into(),
            maximum_records: 32,
            output: "optional.obs".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "mwl-palette-tpl".into(),
            "input.mwl".into(),
            "palette.tpl".into(),
        ])
        .unwrap(),
        Command::MwlPaletteTpl {
            input: "input.mwl".into(),
            output: "palette.tpl".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "mwl-transfer-optional-assets".into(),
            "source.mwl".into(),
            "target.mwl".into(),
            "modes.bin".into(),
            "20".into(),
            "output.mwl".into(),
        ])
        .unwrap(),
        Command::MwlTransferOptionalAssets {
            source: "source.mwl".into(),
            target: "target.mwl".into(),
            size_modes: "modes.bin".into(),
            maximum_records: 32,
            output: "output.mwl".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "mwl-edit-optional-assets".into(),
            "input.mwl".into(),
            "modes.bin".into(),
            "20".into(),
            "edits.txt".into(),
            "output.mwl".into(),
        ])
        .unwrap(),
        Command::MwlEditOptionalAssets {
            input: "input.mwl".into(),
            size_modes: "modes.bin".into(),
            maximum_records: 32,
            edits: "edits.txt".into(),
            output: "output.mwl".into(),
        }
    );
}

#[test]
fn parses_mwl_layer3_settings_observation() {
    assert_eq!(
        parse_from(&[
            "mwl-observe-layer3-settings".into(),
            "input.mwl".into(),
            "layer3.obs".into(),
        ])
        .unwrap(),
        Command::MwlObserveLayer3Settings {
            input: "input.mwl".into(),
            output: "layer3.obs".into(),
        }
    );
}

#[test]
fn parses_shared_revision_profile_inspection() {
    assert_eq!(
        parse_from(&["profile".into(), "smw.lmrev".into()]).unwrap(),
        Command::Profile {
            profile: "smw.lmrev".into(),
            rom: None,
        }
    );
    assert_eq!(
        parse_from(&["profile".into(), "smw.lmrev".into(), "hack.smc".into()]).unwrap(),
        Command::Profile {
            profile: "smw.lmrev".into(),
            rom: Some("hack.smc".into()),
        }
    );
}
#[test]
fn parses_every_profile_export_domain_and_rejects_unknown_domains() {
    for (name, expected) in [
        ("native-assets", ProfileExportKind::NativeAssets),
        ("level", ProfileExportKind::Level),
        ("layer2", ProfileExportKind::Layer2),
        ("map16", ProfileExportKind::Map16),
        ("graphics", ProfileExportKind::Graphics),
        ("palette", ProfileExportKind::Palette),
        ("exanimation", ProfileExportKind::ExAnimation),
        ("expanded-settings", ProfileExportKind::ExpandedSettings),
        ("overworld", ProfileExportKind::Overworld),
    ] {
        assert_eq!(
            parse_from(&[
                "profile-export".into(),
                name.into(),
                "game.smc".into(),
                "smw.lmrev".into(),
                "105".into(),
                "asset.bin".into(),
            ])
            .unwrap(),
            Command::ProfileExport {
                kind: expected,
                rom: "game.smc".into(),
                profile: "smw.lmrev".into(),
                slot: 0x105,
                output: "asset.bin".into(),
            }
        );
    }
    assert!(
        parse_from(&[
            "profile-export".into(),
            "unknown".into(),
            "game.smc".into(),
            "smw.lmrev".into(),
            "0".into(),
            "asset.bin".into(),
        ])
        .is_err()
    );
}

#[test]
fn parses_expanded_settings_file_normalization_and_observation() {
    assert_eq!(
        parse_from(&[
            "expanded-settings-file".into(),
            "record.bin".into(),
            "normalized.bin".into(),
            "record.obs".into(),
        ])
        .unwrap(),
        Command::ExpandedSettingsFile {
            input: "record.bin".into(),
            normalized_output: Some("normalized.bin".into()),
            observation: Some("record.obs".into()),
        }
    );
    assert_eq!(
        parse_from(&[
            "expanded-settings-layer3".into(),
            "record.bin".into(),
            "on".into(),
            "abc".into(),
            "2".into(),
            "3".into(),
            "edited.bin".into(),
        ])
        .unwrap(),
        Command::ExpandedSettingsLayer3 {
            input: "record.bin".into(),
            enabled: true,
            file: 0xabc,
            length_selector: 2,
            offset_selector: 3,
            output: "edited.bin".into(),
        }
    );
}

#[test]
fn parses_native_assets_normalization_with_profile_interpretation() {
    assert_eq!(
        parse_from(&[
            "native-assets-file".into(),
            "level assets.lmna".into(),
            "smw.lmrev".into(),
            "normalized.lmna".into(),
            "assets.obs".into(),
        ])
        .unwrap(),
        Command::NativeAssetsFile {
            input: "level assets.lmna".into(),
            profile: "smw.lmrev".into(),
            normalized_output: Some("normalized.lmna".into()),
            observation: Some("assets.obs".into()),
        }
    );
}

#[test]
fn parses_profile_native_imports() {
    for (name, expected) in [
        ("native-assets", ProfileImportKind::NativeAssets),
        ("level", ProfileImportKind::Level),
        ("map16", ProfileImportKind::Map16),
        ("graphics", ProfileImportKind::Graphics),
        ("palette", ProfileImportKind::Palette),
        ("exanimation", ProfileImportKind::ExAnimation),
        ("expanded-settings", ProfileImportKind::ExpandedSettings),
        ("overworld", ProfileImportKind::Overworld),
    ] {
        assert_eq!(
            parse_from(&[
                "profile-import".into(),
                name.into(),
                "before.smc".into(),
                "after.smc".into(),
                "smw.lmrev".into(),
                "105".into(),
                "asset.bin".into(),
                "300000".into(),
                "400000".into(),
            ])
            .unwrap(),
            Command::ProfileImport {
                kind: expected,
                input_rom: "before.smc".into(),
                output_rom: "after.smc".into(),
                profile: "smw.lmrev".into(),
                slot: 0x105,
                asset: "asset.bin".into(),
                search_start: 0x30_0000,
                search_end: 0x40_0000,
            }
        );
    }
}

#[test]
fn parses_identity_bound_revision_patch_install() {
    assert_eq!(
        parse_from(&[
            "revision-patch-install".into(),
            "before.smc".into(),
            "after.smc".into(),
            "smw-us.lmrev".into(),
            "layer3.lmpatch".into(),
            "300000".into(),
            "400000".into(),
            "ff".into(),
        ])
        .unwrap(),
        Command::RevisionPatchInstall {
            input_rom: "before.smc".into(),
            output_rom: "after.smc".into(),
            profile: "smw-us.lmrev".into(),
            template: "layer3.lmpatch".into(),
            search_start: 0x30_0000,
            search_end: 0x40_0000,
            fill: 0xff,
        }
    );
    assert!(
        parse_from(&[
            "revision-patch-install".into(),
            "a".into(),
            "b".into(),
            "p".into(),
            "t".into(),
            "1".into(),
            "2".into(),
            "100".into(),
        ])
        .is_err()
    );
}

#[test]
fn parses_built_in_expanded_settings_install() {
    assert_eq!(
        parse_from(&[
            "expanded-settings-install".into(),
            "before.smc".into(),
            "after.smc".into(),
        ])
        .unwrap(),
        Command::ExpandedSettingsInstall {
            input_rom: "before.smc".into(),
            output_rom: "after.smc".into(),
        }
    );
}

#[test]
fn parses_built_in_map16_runtime_install() {
    assert_eq!(
        parse_from(&[
            "smw-map16-runtime-install".into(),
            "input.smc".into(),
            "output.smc".into(),
        ])
        .unwrap(),
        Command::Map16RuntimeInstall {
            input_rom: "input.smc".into(),
            output_rom: "output.smc".into(),
        }
    );
}

#[test]
fn parses_complete_layer3_install() {
    assert_eq!(
        parse_from(&[
            "layer3-install".into(),
            "before.smc".into(),
            "after.smc".into(),
        ])
        .unwrap(),
        Command::Layer3Install {
            input_rom: "before.smc".into(),
            output_rom: "after.smc".into(),
        }
    );
}

#[test]
fn parses_native_smw_overworld_path_workflows() {
    assert_eq!(
        parse_from(&[
            "smw-overworld-path-export".into(),
            "input.sfc".into(),
            "links.lmow".into(),
        ])
        .unwrap(),
        Command::SmwOverworldPathExport {
            rom: "input.sfc".into(),
            output: "links.lmow".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "smw-overworld-path-import".into(),
            "input.sfc".into(),
            "links.lmow".into(),
            "output.sfc".into(),
        ])
        .unwrap(),
        Command::SmwOverworldPathImport {
            input_rom: "input.sfc".into(),
            links: "links.lmow".into(),
            output_rom: "output.sfc".into(),
        }
    );
}

#[test]
fn parses_native_smw_overworld_message_workflows() {
    assert_eq!(
        parse_from(&[
            "smw-overworld-message-export".into(),
            "input.sfc".into(),
            "messages.lmowmsg".into(),
        ])
        .unwrap(),
        Command::SmwOverworldMessageExport {
            rom: "input.sfc".into(),
            output: "messages.lmowmsg".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "smw-overworld-message-install".into(),
            "input.sfc".into(),
            "messages.lmowmsg".into(),
            "output.sfc".into(),
        ])
        .unwrap(),
        Command::SmwOverworldMessageInstall {
            input_rom: "input.sfc".into(),
            messages: "messages.lmowmsg".into(),
            output_rom: "output.sfc".into(),
        }
    );
}

#[test]
fn parses_native_smw_overworld_event_workflows() {
    assert_eq!(
        parse_from(&[
            "smw-overworld-event-export".into(),
            "input.sfc".into(),
            "events.lmowevt".into(),
        ])
        .unwrap(),
        Command::SmwOverworldEventExport {
            rom: "input.sfc".into(),
            output: "events.lmowevt".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "smw-overworld-event-import".into(),
            "input.sfc".into(),
            "events.lmowevt".into(),
            "output.sfc".into(),
        ])
        .unwrap(),
        Command::SmwOverworldEventImport {
            input_rom: "input.sfc".into(),
            events: "events.lmowevt".into(),
            output_rom: "output.sfc".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "smw-overworld-event-map-export".into(),
            "input.sfc".into(),
            "event map.lmowmap".into(),
        ])
        .unwrap(),
        Command::SmwOverworldEventMapExport {
            rom: "input.sfc".into(),
            output: "event map.lmowmap".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "smw-overworld-event-map-import".into(),
            "input.sfc".into(),
            "event map.lmowmap".into(),
            "output.sfc".into(),
        ])
        .unwrap(),
        Command::SmwOverworldEventMapImport {
            input_rom: "input.sfc".into(),
            event_map: "event map.lmowmap".into(),
            output_rom: "output.sfc".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "smw-overworld-transfer-observe".into(),
            "input.sfc".into(),
            "events.obs".into(),
        ])
        .unwrap(),
        Command::SmwOverworldTransferObserve {
            rom: "input.sfc".into(),
            output: "events.obs".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "smw-overworld-special-event-export".into(),
            "input.sfc".into(),
            "special events.lmowspc".into(),
        ])
        .unwrap(),
        Command::SmwOverworldSpecialEventExport {
            rom: "input.sfc".into(),
            output: "special events.lmowspc".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "smw-overworld-special-event-import".into(),
            "input.sfc".into(),
            "special events.lmowspc".into(),
            "output.sfc".into(),
        ])
        .unwrap(),
        Command::SmwOverworldSpecialEventImport {
            input_rom: "input.sfc".into(),
            events: "special events.lmowspc".into(),
            output_rom: "output.sfc".into(),
        }
    );
}

#[test]
fn parses_complete_overworld_transfer_observation() {
    assert_eq!(
        parse_from(&[
            "smw-overworld-transfer-full-observe".into(),
            "input.sfc".into(),
            "overworld.obs".into(),
        ])
        .unwrap(),
        Command::SmwOverworldTransferFullObserve {
            rom: "input.sfc".into(),
            output: "overworld.obs".into(),
        }
    );
}

#[test]
fn parses_native_transferred_map16_observation() {
    assert_eq!(
        parse_from(&[
            "smw-transferred-map16-observe".into(),
            "Transferred.smc".into(),
            "Map16.obs".into(),
        ])
        .unwrap(),
        Command::SmwTransferredMap16Observe {
            rom: "Transferred.smc".into(),
            output: "Map16.obs".into(),
        }
    );
}

#[test]
fn parses_installed_map16_remap_observation() {
    assert_eq!(
        parse_from(&[
            "smw-installed-map16-remaps-observe".into(),
            "Transferred.smc".into(),
            "Remaps.obs".into(),
        ])
        .unwrap(),
        Command::SmwInstalledMap16RemapsObserve {
            rom: "Transferred.smc".into(),
            output: "Remaps.obs".into(),
        }
    );
}

#[test]
fn parses_native_smw_overworld_event_tilemap_workflows() {
    assert_eq!(
        parse_from(&[
            "smw-overworld-event-tilemap-export".into(),
            "input.sfc".into(),
            "tilemaps.lmowtil".into(),
        ])
        .unwrap(),
        Command::SmwOverworldEventTilemapExport {
            rom: "input.sfc".into(),
            output: "tilemaps.lmowtil".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "smw-overworld-event-tilemap-import".into(),
            "input.sfc".into(),
            "tilemaps.lmowtil".into(),
            "output.sfc".into(),
        ])
        .unwrap(),
        Command::SmwOverworldEventTilemapImport {
            input_rom: "input.sfc".into(),
            tilemaps: "tilemaps.lmowtil".into(),
            output_rom: "output.sfc".into(),
        }
    );
}

#[test]
fn parses_native_smw_overworld_warp_workflows() {
    assert_eq!(
        parse_from(&[
            "smw-overworld-warp-export".into(),
            "input.sfc".into(),
            "warps.lmow".into(),
        ])
        .unwrap(),
        Command::SmwOverworldWarpExport {
            rom: "input.sfc".into(),
            output: "warps.lmow".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "smw-overworld-warp-import".into(),
            "input.sfc".into(),
            "warps.lmow".into(),
            "output.sfc".into(),
        ])
        .unwrap(),
        Command::SmwOverworldWarpImport {
            input_rom: "input.sfc".into(),
            links: "warps.lmow".into(),
            output_rom: "output.sfc".into(),
        }
    );
}

#[test]
fn parses_native_smw_overworld_settings_workflows() {
    assert_eq!(
        parse_from(&[
            "smw-overworld-settings-export".into(),
            "input.sfc".into(),
            "settings.lmowset".into(),
        ])
        .unwrap(),
        Command::SmwOverworldSettingsExport {
            rom: "input.sfc".into(),
            output: "settings.lmowset".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "smw-overworld-settings-import".into(),
            "input.sfc".into(),
            "settings.lmowset".into(),
            "output.sfc".into(),
        ])
        .unwrap(),
        Command::SmwOverworldSettingsImport {
            input_rom: "input.sfc".into(),
            settings: "settings.lmowset".into(),
            output_rom: "output.sfc".into(),
        }
    );
}

#[test]
fn parses_native_smw_overworld_player_start_workflows() {
    assert_eq!(
        parse_from(&[
            "smw-overworld-start-export".into(),
            "input.sfc".into(),
            "starts.lmowst".into(),
        ])
        .unwrap(),
        Command::SmwOverworldStartExport {
            rom: "input.sfc".into(),
            output: "starts.lmowst".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "smw-overworld-start-import".into(),
            "input.sfc".into(),
            "starts.lmowst".into(),
            "output.sfc".into(),
        ])
        .unwrap(),
        Command::SmwOverworldStartImport {
            input_rom: "input.sfc".into(),
            starts: "starts.lmowst".into(),
            output_rom: "output.sfc".into(),
        }
    );
}

#[test]
fn parses_revision_patch_normalization_and_observation() {
    assert_eq!(
        parse_from(&[
            "revision-patch-file".into(),
            "input.lmpatch".into(),
            "normalized.lmpatch".into(),
            "template.obs".into(),
        ])
        .unwrap(),
        Command::RevisionPatchFile {
            input: "input.lmpatch".into(),
            normalized_output: Some("normalized.lmpatch".into()),
            observation: Some("template.obs".into()),
        }
    );
}

#[test]
fn parses_layer3_workspace_application() {
    assert_eq!(
        parse_from(&[
            "layer3-workspace-apply".into(),
            "c028".into(),
            "workspace.bin".into(),
            "graphics.bin".into(),
            "result.bin".into(),
            "result.obs".into(),
        ])
        .unwrap(),
        Command::Layer3WorkspaceApply {
            packed_descriptor: 0xc028,
            workspace: "workspace.bin".into(),
            decoded_graphics: "graphics.bin".into(),
            output: "result.bin".into(),
            observation: Some("result.obs".into()),
        }
    );
    assert!(
        parse_from(&[
            "layer3-workspace-apply".into(),
            "10000".into(),
            "workspace.bin".into(),
            "graphics.bin".into(),
            "result.bin".into(),
        ])
        .is_err()
    );
}

#[test]
fn parses_graphics_remap_workflows() {
    assert_eq!(
        parse_from(&[
            "graphics-remap-file".into(),
            "stream.bin".into(),
            "normalized.bin".into(),
            "stream.obs".into(),
        ])
        .unwrap(),
        Command::GraphicsRemapFile {
            input: "stream.bin".into(),
            normalized_output: Some("normalized.bin".into()),
            observation: Some("stream.obs".into()),
        }
    );
    assert_eq!(
        parse_from(&[
            "graphics-remap-apply".into(),
            "stream.bin".into(),
            "scratch.bin".into(),
            "output.bin".into(),
            "apply.obs".into(),
        ])
        .unwrap(),
        Command::GraphicsRemapApply {
            stream: "stream.bin".into(),
            scratch: "scratch.bin".into(),
            output: "output.bin".into(),
            observation: Some("apply.obs".into()),
        }
    );
}

#[test]
fn parses_complete_level_bundle_with_observation() {
    let values = [
        "level-bundle",
        "level.lmlevel",
        "normalized.lmlevel",
        "level.obs",
    ];
    assert_eq!(
        parse_from(&values.iter().map(OsString::from).collect::<Vec<OsString>>()).unwrap(),
        Command::CompleteLevel {
            input: "level.lmlevel".into(),
            normalized_output: Some("normalized.lmlevel".into()),
            observation: Some("level.obs".into()),
        }
    );
}

#[test]
fn parses_complete_level_auxiliary_edit_workflow() {
    let values = [
        "level-bundle-edit",
        "input.lmlevel",
        "edits.txt",
        "output.lmlevel",
    ];
    assert_eq!(
        parse_from(&values.iter().map(OsString::from).collect::<Vec<_>>()).unwrap(),
        Command::EditCompleteLevel {
            input: "input.lmlevel".into(),
            script: "edits.txt".into(),
            output: "output.lmlevel".into(),
        }
    );
}

#[test]
fn parses_layer_three_inspection_and_normalization() {
    assert_eq!(
        parse_from(&[
            "layer3-file".into(),
            "layer3.lmlayer3".into(),
            "normalized.lmlayer3".into(),
        ])
        .unwrap(),
        Command::Layer3File {
            input: "layer3.lmlayer3".into(),
            normalized_output: Some("normalized.lmlayer3".into()),
            observation: None,
        }
    );
    assert_eq!(
        parse_from(&[
            "layer3-file".into(),
            "layer3.lmlayer3".into(),
            "normalized.lmlayer3".into(),
            "layer3.obs".into(),
        ])
        .unwrap(),
        Command::Layer3File {
            input: "layer3.lmlayer3".into(),
            normalized_output: Some("normalized.lmlayer3".into()),
            observation: Some("layer3.obs".into()),
        }
    );
}

#[test]
fn parses_custom_object_sidecar_inspection_and_normalization() {
    assert_eq!(
        parse_from(&[
            "custom-object-library".into(),
            "library.mw0".into(),
            "library.mw0t".into(),
            "normalized.mw0".into(),
            "normalized.mw0t".into(),
        ])
        .unwrap(),
        Command::CustomObjectLibrary {
            data: "library.mw0".into(),
            descriptions: "library.mw0t".into(),
            normalized_outputs: Some(("normalized.mw0".into(), "normalized.mw0t".into())),
            observation: None,
        }
    );
    assert!(matches!(
        parse_from(&[
            "custom-object-library".into(),
            "library.mw0".into(),
            "library.mw0t".into(),
            "normalized.mw0".into(),
            "normalized.mw0t".into(),
            "library.obs".into(),
        ])
        .unwrap(),
        Command::CustomObjectLibrary {
            observation: Some(_),
            ..
        }
    ));
}
#[test]
fn parses_complete_map16_set_with_observation() {
    let values = [
        "map16-set-file",
        "all.lm16set",
        "normalized.lm16set",
        "all.obs",
    ];
    assert_eq!(
        parse_from(&values.iter().map(OsString::from).collect::<Vec<OsString>>()).unwrap(),
        Command::CompleteMap16 {
            input: "all.lm16set".into(),
            normalized_output: Some("normalized.lm16set".into()),
            observation: Some("all.obs".into()),
        }
    );
}

#[test]
fn parses_deterministic_map16_page_render() {
    let values = [
        "render-map16-page",
        "graphics.lmgfx",
        "palette.lmpal",
        "page.map16",
        "page.png",
    ];
    assert_eq!(
        parse_from(&values.iter().map(OsString::from).collect::<Vec<OsString>>()).unwrap(),
        Command::RenderMap16Page {
            graphics: "graphics.lmgfx".into(),
            palette: "palette.lmpal".into(),
            page: "page.map16".into(),
            output: "page.png".into(),
        }
    );
}

#[test]
fn parses_standalone_graphics_and_palette_renders() {
    let graphics = [
        "render-graphics",
        "graphics.lmgfx",
        "palette.lmpal",
        "3",
        "10",
        "sheet.png",
    ];
    assert_eq!(
        parse_from(&graphics.map(OsString::from)).unwrap(),
        Command::RenderGraphics {
            graphics: "graphics.lmgfx".into(),
            palette: "palette.lmpal".into(),
            palette_row: 3,
            columns: 0x10,
            output: "sheet.png".into(),
        }
    );
    let palette = ["render-palette", "palette.lmpal", "10", "c", "swatches.png"];
    assert_eq!(
        parse_from(&palette.map(OsString::from)).unwrap(),
        Command::RenderPalette {
            palette: "palette.lmpal".into(),
            columns: 0x10,
            cell_size: 0xc,
            output: "swatches.png".into(),
        }
    );
    let tpl_palette = [
        "tpl-palette-file",
        "palette.tpl",
        "normalized.tpl",
        "palette.obs",
    ];
    assert_eq!(
        parse_from(&tpl_palette.map(OsString::from)).unwrap(),
        Command::TplPaletteFile {
            input: "palette.tpl".into(),
            normalized_output: Some("normalized.tpl".into()),
            observation: Some("palette.obs".into()),
        }
    );
    let smw_palette = [
        "smw-palette-file",
        "shared.smwpal",
        "normalized.smwpal",
        "shared.obs",
    ];
    assert_eq!(
        parse_from(&smw_palette.map(OsString::from)).unwrap(),
        Command::SmwPaletteFile {
            input: "shared.smwpal".into(),
            normalized_output: Some("normalized.smwpal".into()),
            observation: Some("shared.obs".into()),
        }
    );
    let mut invalid = graphics;
    invalid[3] = "not-hex";
    assert!(parse_from(&invalid.map(OsString::from)).is_err());
}

#[test]
fn parses_portable_graphics_and_palette_file_workflows() {
    let graphics = [
        "graphics-file",
        "graphics.lmgfx",
        "normalized.lmgfx",
        "graphics.obs",
    ];
    assert_eq!(
        parse_from(&graphics.map(OsString::from)).unwrap(),
        Command::GraphicsFile {
            input: "graphics.lmgfx".into(),
            normalized_output: Some("normalized.lmgfx".into()),
            observation: Some("graphics.obs".into()),
        }
    );
    let palette = [
        "palette-file",
        "palette.lmpal",
        "normalized.lmpal",
        "palette.obs",
    ];
    assert_eq!(
        parse_from(&palette.map(OsString::from)).unwrap(),
        Command::PaletteFile {
            input: "palette.lmpal".into(),
            normalized_output: Some("normalized.lmpal".into()),
            observation: Some("palette.obs".into()),
        }
    );
    let page = [
        "map16-page-file",
        "page.map16",
        "normalized.map16",
        "page.obs",
    ];
    assert_eq!(
        parse_from(&page.map(OsString::from)).unwrap(),
        Command::Map16PageFile {
            input: "page.map16".into(),
            normalized_output: Some("normalized.map16".into()),
            observation: Some("page.obs".into()),
        }
    );
    let plane = [
        "layer3-plane-file",
        "plane.lml3frame",
        "normalized.lml3frame",
        "plane.obs",
    ];
    assert_eq!(
        parse_from(&plane.map(OsString::from)).unwrap(),
        Command::Layer3PlaneFile {
            input: "plane.lml3frame".into(),
            normalized_output: Some("normalized.lml3frame".into()),
            observation: Some("plane.obs".into()),
        }
    );
    let frame = [
        "animation-frame-file",
        "frame.lmanfrm",
        "normalized.lmanfrm",
        "frame.obs",
    ];
    assert_eq!(
        parse_from(&frame.map(OsString::from)).unwrap(),
        Command::AnimationFrameFile {
            input: "frame.lmanfrm".into(),
            normalized_output: Some("normalized.lmanfrm".into()),
            observation: Some("frame.obs".into()),
        }
    );
}

#[test]
fn parses_provider_appearance_and_native_level_file_workflows() {
    let entity_appearance = [
        "appearance-file",
        "entities.lmentapp",
        "normalized.lmentapp",
        "entities.obs",
    ];
    assert_eq!(
        parse_from(&entity_appearance.map(OsString::from)).unwrap(),
        Command::AppearanceFile {
            input: "entities.lmentapp".into(),
            normalized_output: Some("normalized.lmentapp".into()),
            observation: Some("entities.obs".into()),
        }
    );
    let overworld_appearance = [
        "overworld-appearance-file",
        "sprites.lmowapp",
        "normalized.lmowapp",
        "sprites.obs",
    ];
    assert_eq!(
        parse_from(&overworld_appearance.map(OsString::from)).unwrap(),
        Command::OverworldAppearanceFile {
            input: "sprites.lmowapp".into(),
            normalized_output: Some("normalized.lmowapp".into()),
            observation: Some("sprites.obs".into()),
        }
    );
    let native_level = [
        "native-level-file",
        "level.lmlvl",
        "sprite-lengths.bin",
        "normalized.lmlvl",
        "level.obs",
    ];
    assert_eq!(
        parse_from(&native_level.map(OsString::from)).unwrap(),
        Command::NativeLevelFile {
            input: "level.lmlvl".into(),
            sprite_lengths: Some("sprite-lengths.bin".into()),
            normalized_output: Some("normalized.lmlvl".into()),
            observation: Some("level.obs".into()),
        }
    );
    assert_eq!(
        parse_from(&[
            "native-level-file".into(),
            "level.lmlvl".into(),
            "standard".into(),
        ])
        .unwrap(),
        Command::NativeLevelFile {
            input: "level.lmlvl".into(),
            sprite_lengths: None,
            normalized_output: None,
            observation: None,
        }
    );
}

#[test]
fn parses_interpretation_bound_exanimation_file_workflow() {
    let values = [
        "exanimation-file",
        "animation.lmexan",
        "modes.bin",
        "20",
        "normalized.lmexan",
        "animation.obs",
    ];
    assert_eq!(
        parse_from(&values.map(OsString::from)).unwrap(),
        Command::ExAnimationFile {
            input: "animation.lmexan".into(),
            size_modes: "modes.bin".into(),
            maximum_records: 0x20,
            normalized_output: Some("normalized.lmexan".into()),
            observation: Some("animation.obs".into()),
        }
    );
    let mut invalid = values;
    invalid[3] = "xyz";
    assert!(parse_from(&invalid.map(OsString::from)).is_err());
}

#[test]
fn parses_interpretation_bound_complete_overworld_file_workflow() {
    let values = [
        "overworld-file",
        "world.lmow",
        "modes.bin",
        "20",
        "normalized.lmow",
        "world.obs",
    ];
    assert_eq!(
        parse_from(&values.map(OsString::from)).unwrap(),
        Command::OverworldFile {
            input: "world.lmow".into(),
            size_modes: "modes.bin".into(),
            maximum_animation_records: 0x20,
            normalized_output: Some("normalized.lmow".into()),
            observation: Some("world.obs".into()),
        }
    );
    let mut invalid = values;
    invalid[3] = "xyz";
    assert!(parse_from(&invalid.map(OsString::from)).is_err());
}

#[test]
fn parses_deterministic_level_render_with_explicit_shapes() {
    let values = [
        "render-level",
        "level.lmlevel",
        "all.lm16set",
        "graphics.lmgfx",
        "palette.lmpal",
        "10",
        "20",
        "0",
        "0",
        "level.png",
    ];
    assert_eq!(
        parse_from(&values.iter().map(OsString::from).collect::<Vec<OsString>>()).unwrap(),
        Command::RenderLevel {
            level: "level.lmlevel".into(),
            map16: "all.lm16set".into(),
            graphics: "graphics.lmgfx".into(),
            palette: "palette.lmpal".into(),
            appearances: None,
            layer3_plane: None,
            layer1_width: 0x10,
            layer1_height: 0x20,
            layer2_width: 0,
            layer2_height: 0,
            output: "level.png".into(),
        }
    );
    let mut invalid = values;
    invalid[5] = "not-a-number";
    assert!(parse_from(&invalid.iter().map(OsString::from).collect::<Vec<_>>()).is_err());
    let with_appearances = [
        "render-level",
        "level.lmlevel",
        "all.lm16set",
        "graphics.lmgfx",
        "palette.lmpal",
        "entities.lmentapp",
        "10",
        "20",
        "0",
        "0",
        "level.png",
    ];
    assert!(matches!(
        parse_from(
            &with_appearances
                .iter()
                .map(OsString::from)
                .collect::<Vec<_>>()
        )
        .unwrap(),
        Command::RenderLevel {
            appearances: Some(_),
            ..
        }
    ));
    let with_layer3 = [
        "render-level",
        "level.lmlevel",
        "all.lm16set",
        "graphics.lmgfx",
        "palette.lmpal",
        "none",
        "layer3.lml3frame",
        "10",
        "20",
        "0",
        "0",
        "level.png",
    ];
    assert!(matches!(
        parse_from(&with_layer3.iter().map(OsString::from).collect::<Vec<_>>()).unwrap(),
        Command::RenderLevel {
            appearances: None,
            layer3_plane: Some(_),
            ..
        }
    ));
}

#[test]
fn parses_deterministic_overworld_render_with_explicit_reveal_state() {
    let values = [
        "render-overworld",
        "world.lmow",
        "modes.bin",
        "20",
        "all.lm16set",
        "graphics.lmgfx",
        "f",
        "world.png",
    ];
    assert_eq!(
        parse_from(&values.iter().map(OsString::from).collect::<Vec<_>>()).unwrap(),
        Command::RenderOverworld {
            overworld: "world.lmow".into(),
            size_modes: "modes.bin".into(),
            maximum_animation_records: 0x20,
            map16: "all.lm16set".into(),
            graphics: "graphics.lmgfx".into(),
            appearances: None,
            animation_frame: None,
            completed_reveals: 0xf,
            output: "world.png".into(),
        }
    );
    let with_appearances = [
        "render-overworld",
        "world.lmow",
        "modes.bin",
        "20",
        "all.lm16set",
        "graphics.lmgfx",
        "sprites.lmowapp",
        "f",
        "world.png",
    ];
    assert!(matches!(
        parse_from(
            &with_appearances
                .iter()
                .map(OsString::from)
                .collect::<Vec<_>>()
        )
        .unwrap(),
        Command::RenderOverworld {
            appearances: Some(_),
            ..
        }
    ));
    let with_frame = [
        "render-overworld",
        "world.lmow",
        "modes.bin",
        "20",
        "all.lm16set",
        "graphics.lmgfx",
        "none",
        "tick.lmanim",
        "f",
        "world.png",
    ];
    assert!(matches!(
        parse_from(&with_frame.iter().map(OsString::from).collect::<Vec<_>>()).unwrap(),
        Command::RenderOverworld {
            appearances: None,
            animation_frame: Some(_),
            ..
        }
    ));
}

#[test]
fn parses_overworld_path_inspection_and_copy_on_write_normalization() {
    let parse = |values: &[&str]| {
        parse_from(&values.iter().map(OsString::from).collect::<Vec<OsString>>()).unwrap()
    };
    assert_eq!(
        parse(&["overworld-path", "world.lmowpath"]),
        Command::OverworldPath {
            input: "world.lmowpath".into(),
            normalized_output: None,
            observation: None,
        }
    );
    assert_eq!(
        parse(&["overworld-path", "world.lmowpath", "normalized.lmowpath"]),
        Command::OverworldPath {
            input: "world.lmowpath".into(),
            normalized_output: Some("normalized.lmowpath".into()),
            observation: None,
        }
    );
    assert_eq!(
        parse(&[
            "overworld-path",
            "world.lmowpath",
            "normalized.lmowpath",
            "paths.obs",
        ]),
        Command::OverworldPath {
            input: "world.lmowpath".into(),
            normalized_output: Some("normalized.lmowpath".into()),
            observation: Some("paths.obs".into()),
        }
    );
}

#[test]
fn parses_overworld_metadata_inspection_and_normalization() {
    let values = [
        "overworld-metadata",
        "world.lmowmeta",
        "normalized.lmowmeta",
    ];
    assert_eq!(
        parse_from(&values.iter().map(OsString::from).collect::<Vec<OsString>>()).unwrap(),
        Command::OverworldMetadata {
            input: "world.lmowmeta".into(),
            normalized_output: Some("normalized.lmowmeta".into()),
            observation: None,
        }
    );
    let observed = [
        "overworld-metadata",
        "world.lmowmeta",
        "normalized.lmowmeta",
        "metadata.obs",
    ];
    assert!(matches!(
        parse_from(&observed.map(OsString::from)).unwrap(),
        Command::OverworldMetadata {
            observation: Some(_),
            ..
        }
    ));
}
