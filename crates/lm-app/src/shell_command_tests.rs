use super::*;

#[test]
fn parses_complete_mwl_import_with_unicode_path_and_allocation_range() {
    assert_eq!(
        parse("level-mwl-import levels/My Level 日本語.mwl 300000 400000").unwrap(),
        ShellCommand::ImportMwlLevel {
            path: "levels/My Level 日本語.mwl".into(),
            search_start: 0x300000,
            search_end: 0x400000,
        }
    );
    assert!(matches!(
        parse("level-mwl-import level.mwl 300000"),
        Err(ShellCommandError::MissingArgument("level-mwl-import"))
    ));
}

#[test]
fn parses_complete_mwl_export_with_unicode_path() {
    assert_eq!(
        parse("level-mwl-export levels/My Level 日本語.mwl").unwrap(),
        ShellCommand::ExportMwlLevel("levels/My Level 日本語.mwl".into())
    );
    assert!(matches!(
        parse("level-mwl-export"),
        Err(ShellCommandError::MissingArgument("level-mwl-export"))
    ));
}

#[test]
fn parses_mwl_document_lifecycle_commands() {
    assert_eq!(
        parse("mwl-open levels/My Level.mwl").unwrap(),
        ShellCommand::MwlDocument(MwlDocumentCommand::Open("levels/My Level.mwl".into()))
    );
    assert_eq!(
        parse("mwl-edit-file scripts/edit level.txt").unwrap(),
        ShellCommand::MwlDocument(MwlDocumentCommand::Edit("scripts/edit level.txt".into()))
    );
    assert_eq!(
        parse("mwl-import-optional-assets-file specs/import options.txt").unwrap(),
        ShellCommand::MwlDocument(MwlDocumentCommand::ImportOptionalAssets(
            "specs/import options.txt".into()
        ))
    );
    assert_eq!(
        parse("mwl-edit-optional-assets-file specs/edit options.txt").unwrap(),
        ShellCommand::MwlDocument(MwlDocumentCommand::EditOptionalAssets(
            "specs/edit options.txt".into()
        ))
    );
    assert_eq!(
        parse("mwl-edit-layer3-settings-file specs/layer 3.txt").unwrap(),
        ShellCommand::MwlDocument(MwlDocumentCommand::EditLayer3Settings(
            "specs/layer 3.txt".into()
        ))
    );
    assert_eq!(
        parse("mwl-status").unwrap(),
        ShellCommand::MwlDocument(MwlDocumentCommand::Status)
    );
    assert_eq!(
        parse("mwl-undo").unwrap(),
        ShellCommand::MwlDocument(MwlDocumentCommand::Undo)
    );
    assert_eq!(
        parse("mwl-redo").unwrap(),
        ShellCommand::MwlDocument(MwlDocumentCommand::Redo)
    );
    assert_eq!(
        parse("mwl-save").unwrap(),
        ShellCommand::MwlDocument(MwlDocumentCommand::Save)
    );
    assert_eq!(
        parse("mwl-close").unwrap(),
        ShellCommand::MwlDocument(MwlDocumentCommand::Close)
    );
    assert_eq!(
        parse("mwl-discard").unwrap(),
        ShellCommand::MwlDocument(MwlDocumentCommand::Discard)
    );
    assert!(matches!(
        parse("mwl-save extra"),
        Err(ShellCommandError::UnexpectedArgument("mwl-save"))
    ));
}

#[test]
fn parses_revision_patch_install_spec_path() {
    assert_eq!(
        parse("revision-patch-install-file specs/Layer 3 install.txt").unwrap(),
        ShellCommand::InstallRevisionPatch("specs/Layer 3 install.txt".into())
    );
    assert!(matches!(
        parse("revision-patch-install-file"),
        Err(ShellCommandError::MissingArgument(
            "revision-patch-install-file"
        ))
    ));
}

#[test]
fn parses_built_in_expanded_settings_install() {
    assert_eq!(
        parse("expanded-settings-install").unwrap(),
        ShellCommand::InstallSettings
    );
    assert_eq!(
        parse("expanded-settings-install extra"),
        Err(ShellCommandError::UnexpectedArgument(
            "expanded-settings-install"
        ))
    );
}

#[test]
fn parses_complete_layer3_install() {
    assert_eq!(
        parse("layer3-install").unwrap(),
        ShellCommand::InstallLayer3
    );
    assert_eq!(
        parse("layer3-install extra"),
        Err(ShellCommandError::UnexpectedArgument("layer3-install"))
    );
}

#[test]
fn parses_native_overworld_path_import_and_export() {
    assert_eq!(
        parse("overworld-native-path-export paths/Native Links.lmow").unwrap(),
        ShellCommand::NativeOverworldPathExport("paths/Native Links.lmow".into())
    );
    assert_eq!(
        parse("overworld-native-path-import paths/Native Links.lmow").unwrap(),
        ShellCommand::NativeOverworldPathImport("paths/Native Links.lmow".into())
    );
}

#[test]
fn parses_native_overworld_message_import_and_export() {
    assert_eq!(
        parse("overworld-native-message-export paths/Native Messages.lmowmsg").unwrap(),
        ShellCommand::NativeOverworldMessageExport("paths/Native Messages.lmowmsg".into())
    );
    assert_eq!(
        parse("overworld-native-message-import paths/Native Messages.lmowmsg").unwrap(),
        ShellCommand::NativeOverworldMessageImport("paths/Native Messages.lmowmsg".into())
    );
}

#[test]
fn parses_native_overworld_event_import_and_export() {
    assert_eq!(
        parse("overworld-native-event-export paths/Native Events.lmowevt").unwrap(),
        ShellCommand::NativeOverworldEventExport("paths/Native Events.lmowevt".into())
    );
    assert_eq!(
        parse("overworld-native-event-import paths/Native Events.lmowevt").unwrap(),
        ShellCommand::NativeOverworldEventImport("paths/Native Events.lmowevt".into())
    );
    assert_eq!(
        parse("overworld-native-event-map-export paths/Event Map.lmowmap").unwrap(),
        ShellCommand::NativeOverworldEventMapExport("paths/Event Map.lmowmap".into())
    );
    assert_eq!(
        parse("overworld-native-event-map-import paths/Event Map.lmowmap").unwrap(),
        ShellCommand::NativeOverworldEventMapImport("paths/Event Map.lmowmap".into())
    );
    assert_eq!(
        parse("overworld-native-special-event-export paths/Special Events.lmowspc").unwrap(),
        ShellCommand::NativeOverworldSpecialEventExport("paths/Special Events.lmowspc".into())
    );
    assert_eq!(
        parse("overworld-native-special-event-import paths/Special Events.lmowspc").unwrap(),
        ShellCommand::NativeOverworldSpecialEventImport("paths/Special Events.lmowspc".into())
    );
    assert_eq!(
        parse("overworld-native-event-tilemap-export paths/Event Tilemaps.lmowtil").unwrap(),
        ShellCommand::NativeOverworldEventTilemapExport("paths/Event Tilemaps.lmowtil".into())
    );
    assert_eq!(
        parse("overworld-native-event-tilemap-import paths/Event Tilemaps.lmowtil").unwrap(),
        ShellCommand::NativeOverworldEventTilemapImport("paths/Event Tilemaps.lmowtil".into())
    );
}

#[test]
fn parses_native_overworld_warp_import_and_export() {
    assert_eq!(
        parse("overworld-native-warp-export paths/Native Warps.lmow").unwrap(),
        ShellCommand::NativeOverworldWarpExport("paths/Native Warps.lmow".into())
    );
    assert_eq!(
        parse("overworld-native-warp-import paths/Native Warps.lmow").unwrap(),
        ShellCommand::NativeOverworldWarpImport("paths/Native Warps.lmow".into())
    );
}

#[test]
fn parses_native_overworld_level_name_import_and_export() {
    assert_eq!(
        parse("overworld-native-name-export paths/Native Names.lmowmeta").unwrap(),
        ShellCommand::NativeOverworldLevelNameExport("paths/Native Names.lmowmeta".into())
    );
    assert_eq!(
        parse("overworld-native-name-import paths/Native Names.lmowmeta").unwrap(),
        ShellCommand::NativeOverworldLevelNameImport("paths/Native Names.lmowmeta".into())
    );
    assert_eq!(
        parse("overworld-native-settings-export paths/Native Settings.lmowset").unwrap(),
        ShellCommand::NativeOverworldSettingsExport("paths/Native Settings.lmowset".into())
    );
    assert_eq!(
        parse("overworld-native-settings-import paths/Native Settings.lmowset").unwrap(),
        ShellCommand::NativeOverworldSettingsImport("paths/Native Settings.lmowset".into())
    );
    assert_eq!(
        parse("overworld-native-start-export paths/Native Starts.lmowst").unwrap(),
        ShellCommand::NativeOverworldPlayerStartExport("paths/Native Starts.lmowst".into())
    );
    assert_eq!(
        parse("overworld-native-start-import paths/Native Starts.lmowst").unwrap(),
        ShellCommand::NativeOverworldPlayerStartImport("paths/Native Starts.lmowst".into())
    );
}

#[test]
fn parses_native_level_document_lifecycle_commands() {
    assert_eq!(
        parse("native-level-open specs/Open Level.txt").unwrap(),
        ShellCommand::NativeLevelDocument(NativeLevelDocumentCommand::Open(
            "specs/Open Level.txt".into()
        ))
    );
    assert_eq!(
        parse("native-level-edit-file scripts/Edit Level.txt").unwrap(),
        ShellCommand::NativeLevelDocument(NativeLevelDocumentCommand::Edit(
            "scripts/Edit Level.txt".into()
        ))
    );
    for (text, command) in [
        ("native-level-undo", NativeLevelDocumentCommand::Undo),
        ("native-level-redo", NativeLevelDocumentCommand::Redo),
        ("native-level-status", NativeLevelDocumentCommand::Status),
        ("native-level-save", NativeLevelDocumentCommand::Save),
        ("native-level-close", NativeLevelDocumentCommand::Close),
        ("native-level-discard", NativeLevelDocumentCommand::Discard),
    ] {
        assert_eq!(
            parse(text).unwrap(),
            ShellCommand::NativeLevelDocument(command)
        );
    }
    assert!(matches!(
        parse("native-level-save extra"),
        Err(ShellCommandError::UnexpectedArgument("native-level-save"))
    ));
}

#[test]
fn parses_native_assets_document_lifecycle_commands() {
    assert_eq!(
        parse("native-assets-open-file specs/Open Aggregate.txt").unwrap(),
        ShellCommand::NativeAssetsDocument(NativeAssetsDocumentCommand::Open(
            "specs/Open Aggregate.txt".into()
        ))
    );
    assert_eq!(
        parse("native-assets-edit-file scripts/Edit Aggregate.txt").unwrap(),
        ShellCommand::NativeAssetsDocument(NativeAssetsDocumentCommand::Edit(
            "scripts/Edit Aggregate.txt".into()
        ))
    );
    assert_eq!(
        parse("native-assets-render-file specs/Render Palette.txt").unwrap(),
        ShellCommand::NativeAssetsDocument(NativeAssetsDocumentCommand::Render(
            "specs/Render Palette.txt".into()
        ))
    );
    for (text, command) in [
        ("native-assets-undo", NativeAssetsDocumentCommand::Undo),
        ("native-assets-redo", NativeAssetsDocumentCommand::Redo),
        ("native-assets-status", NativeAssetsDocumentCommand::Status),
        ("native-assets-save", NativeAssetsDocumentCommand::Save),
        ("native-assets-close", NativeAssetsDocumentCommand::Close),
        (
            "native-assets-discard",
            NativeAssetsDocumentCommand::Discard,
        ),
    ] {
        assert_eq!(
            parse(text).unwrap(),
            ShellCommand::NativeAssetsDocument(command)
        );
    }
    assert!(matches!(
        parse("native-assets-save extra"),
        Err(ShellCommandError::UnexpectedArgument("native-assets-save"))
    ));
}

#[test]
fn parses_map16_page_document_lifecycle_commands() {
    assert_eq!(
        parse("map16-page-open Pages/Page 12.map16").unwrap(),
        ShellCommand::Map16PageDocument(Map16PageDocumentCommand::Open(
            "Pages/Page 12.map16".into()
        ))
    );
    assert_eq!(
        parse("map16-page-edit-file Scripts/Page edit.txt").unwrap(),
        ShellCommand::Map16PageDocument(Map16PageDocumentCommand::Edit(
            "Scripts/Page edit.txt".into()
        ))
    );
    assert_eq!(
        parse("map16-page-render-file Specs/Page preview.txt").unwrap(),
        ShellCommand::Map16PageDocument(Map16PageDocumentCommand::Render(
            "Specs/Page preview.txt".into()
        ))
    );
    assert_eq!(
        parse("map16-page-undo").unwrap(),
        ShellCommand::Map16PageDocument(Map16PageDocumentCommand::Undo)
    );
    assert_eq!(
        parse("map16-page-redo").unwrap(),
        ShellCommand::Map16PageDocument(Map16PageDocumentCommand::Redo)
    );
    for (text, command) in [
        ("map16-page-status", Map16PageDocumentCommand::Status),
        ("map16-page-save", Map16PageDocumentCommand::Save),
        ("map16-page-close", Map16PageDocumentCommand::Close),
        ("map16-page-discard", Map16PageDocumentCommand::Discard),
    ] {
        assert_eq!(
            parse(text).unwrap(),
            ShellCommand::Map16PageDocument(command)
        );
    }
    assert!(matches!(
        parse("map16-page-save extra"),
        Err(ShellCommandError::UnexpectedArgument("map16-page-save"))
    ));
}

#[test]
fn parses_entity_appearance_document_lifecycle_commands() {
    assert_eq!(
        parse("entity-app-open Entity 日本語.lmentapp").unwrap(),
        ShellCommand::EntityAppearanceDocument(EntityAppearanceDocumentCommand::Open(
            "Entity 日本語.lmentapp".into()
        ))
    );
    assert_eq!(
        parse("entity-app-edit-file Entity edits.txt").unwrap(),
        ShellCommand::EntityAppearanceDocument(EntityAppearanceDocumentCommand::Edit(
            "Entity edits.txt".into()
        ))
    );
    for (text, command) in [
        ("entity-app-undo", EntityAppearanceDocumentCommand::Undo),
        ("entity-app-redo", EntityAppearanceDocumentCommand::Redo),
        ("entity-app-status", EntityAppearanceDocumentCommand::Status),
        ("entity-app-save", EntityAppearanceDocumentCommand::Save),
        ("entity-app-close", EntityAppearanceDocumentCommand::Close),
        (
            "entity-app-discard",
            EntityAppearanceDocumentCommand::Discard,
        ),
    ] {
        assert_eq!(
            parse(text).unwrap(),
            ShellCommand::EntityAppearanceDocument(command)
        );
    }
}

#[test]
fn parses_overworld_appearance_document_lifecycle_commands() {
    assert_eq!(
        parse("world-app-open Sprites 日本語.lmowapp").unwrap(),
        ShellCommand::OverworldAppearanceDocument(OverworldAppearanceDocumentCommand::Open(
            "Sprites 日本語.lmowapp".into()
        ))
    );
    assert_eq!(
        parse("world-app-edit-file Sprite edits.txt").unwrap(),
        ShellCommand::OverworldAppearanceDocument(OverworldAppearanceDocumentCommand::Edit(
            "Sprite edits.txt".into()
        ))
    );
    for (text, command) in [
        ("world-app-undo", OverworldAppearanceDocumentCommand::Undo),
        ("world-app-redo", OverworldAppearanceDocumentCommand::Redo),
        (
            "world-app-status",
            OverworldAppearanceDocumentCommand::Status,
        ),
        ("world-app-save", OverworldAppearanceDocumentCommand::Save),
        ("world-app-close", OverworldAppearanceDocumentCommand::Close),
        (
            "world-app-discard",
            OverworldAppearanceDocumentCommand::Discard,
        ),
    ] {
        assert_eq!(
            parse(text).unwrap(),
            ShellCommand::OverworldAppearanceDocument(command)
        );
    }
    assert!(matches!(
        parse("world-app-save extra"),
        Err(ShellCommandError::UnexpectedArgument("world-app-save"))
    ));
}

#[test]
fn paths_preserve_spaces_and_unicode() {
    assert_eq!(
        parse("open   /tmp/SMW 日本語 hack.smc ").unwrap(),
        ShellCommand::Open("/tmp/SMW 日本語 hack.smc".into())
    );
    assert_eq!(
        parse("save-as relative folder/output.smc").unwrap(),
        ShellCommand::SaveAs("relative folder/output.smc".into())
    );
    assert_eq!(
        parse("profile profiles/SMW USA.lmrev").unwrap(),
        ShellCommand::InstallRevisionProfile("profiles/SMW USA.lmrev".into())
    );
    assert_eq!(
        parse("custom-open /tmp/My Objects 日本語.mw0").unwrap(),
        ShellCommand::CustomObjectOpen("/tmp/My Objects 日本語.mw0".into())
    );
    assert_eq!(
        parse("custom-edit scripts/My custom edits.lmedit").unwrap(),
        ShellCommand::CustomObjectEdit("scripts/My custom edits.lmedit".into())
    );
    assert_eq!(
        parse("custom-sprite-open specs/My sprites 日本語.txt").unwrap(),
        ShellCommand::CustomSpriteOpen("specs/My sprites 日本語.txt".into())
    );
    assert_eq!(
        parse("custom-sprite-edit scripts/My sprite edits.txt").unwrap(),
        ShellCommand::CustomSpriteEdit("scripts/My sprite edits.txt".into())
    );
    assert_eq!(
        parse("native-sidecar-open specs/Native sidecar 日本語.txt").unwrap(),
        ShellCommand::NativeMap16SidecarOpen("specs/Native sidecar 日本語.txt".into())
    );
    assert_eq!(
        parse("metadata-open /tmp/My Overworld 日本語.lmowmeta").unwrap(),
        ShellCommand::MetadataDocument(MetadataDocumentCommand::Open(
            "/tmp/My Overworld 日本語.lmowmeta".into()
        ))
    );
    assert_eq!(
        parse("metadata-edit scripts/My metadata edits.lmedit").unwrap(),
        ShellCommand::MetadataDocument(MetadataDocumentCommand::Edit(
            "scripts/My metadata edits.lmedit".into()
        ))
    );
    assert_eq!(
        parse("path-open /tmp/My Paths 日本語.lmowpath").unwrap(),
        ShellCommand::PathDocument(PathDocumentCommand::Open(
            "/tmp/My Paths 日本語.lmowpath".into()
        ))
    );
    assert_eq!(
        parse("path-edit scripts/My path edits.lmedit").unwrap(),
        ShellCommand::PathDocument(PathDocumentCommand::Edit(
            "scripts/My path edits.lmedit".into()
        ))
    );
    assert_eq!(
        parse("layer3-open /tmp/My Layer 3 日本語.lmlayer3").unwrap(),
        ShellCommand::Layer3Document(Layer3DocumentCommand::Open(
            "/tmp/My Layer 3 日本語.lmlayer3".into()
        ))
    );
    assert_eq!(
        parse("layer3-edit-file scripts/My Layer 3 edits.lmedit").unwrap(),
        ShellCommand::Layer3Document(Layer3DocumentCommand::Edit(
            "scripts/My Layer 3 edits.lmedit".into()
        ))
    );
    assert_eq!(
        parse("ui-config /tmp/My Frontend 日本語.lmuicfg").unwrap(),
        ShellCommand::Ui(UiCommand::Install("/tmp/My Frontend 日本語.lmuicfg".into()))
    );
    assert_eq!(
        parse("ui-shortcut primary+shift+s").unwrap(),
        ShellCommand::Ui(UiCommand::Shortcut("primary+shift+s".into()))
    );
    assert!(parse("ui-action save now").is_err());
    assert_eq!(
        parse("tools-config /tmp/My Tools 日本語.lmtools").unwrap(),
        ShellCommand::Tool(ToolCommand::Install("/tmp/My Tools 日本語.lmtools".into()))
    );
    assert_eq!(
        parse("tool-run emulator").unwrap(),
        ShellCommand::Tool(ToolCommand::Run("emulator".into()))
    );
    assert_eq!(
        parse("tool-event saved").unwrap(),
        ShellCommand::Tool(ToolCommand::Event("saved".into()))
    );
    assert_eq!(
        parse("tool-exec emulator").unwrap(),
        ShellCommand::Tool(ToolCommand::Execute("emulator".into()))
    );
    assert_eq!(
        parse("tool-event-exec level").unwrap(),
        ShellCommand::Tool(ToolCommand::ExecuteEvent("level".into()))
    );
    assert!(parse("tool-run two ids").is_err());
    assert!(parse("tool-exec two ids").is_err());
    assert!(parse("tool-event-exec").is_err());
}

#[test]
fn layer3_history_commands_are_typed_and_argument_free() {
    assert_eq!(
        parse("layer3-undo").unwrap(),
        ShellCommand::Layer3Document(Layer3DocumentCommand::Undo)
    );
    assert_eq!(
        parse("layer3-redo").unwrap(),
        ShellCommand::Layer3Document(Layer3DocumentCommand::Redo)
    );
    assert!(parse("layer3-undo now").is_err());
    assert!(parse("layer3-redo now").is_err());
}

#[test]
fn overworld_support_history_commands_are_typed_and_argument_free() {
    for (text, command) in [
        (
            "metadata-undo",
            ShellCommand::MetadataDocument(MetadataDocumentCommand::Undo),
        ),
        (
            "metadata-redo",
            ShellCommand::MetadataDocument(MetadataDocumentCommand::Redo),
        ),
        (
            "path-undo",
            ShellCommand::PathDocument(PathDocumentCommand::Undo),
        ),
        (
            "path-redo",
            ShellCommand::PathDocument(PathDocumentCommand::Redo),
        ),
    ] {
        assert_eq!(parse(text).unwrap(), command);
        assert!(parse(&format!("{text} extra")).is_err());
    }
}

#[test]
fn custom_object_history_commands_are_typed_and_argument_free() {
    assert_eq!(
        parse("custom-undo").unwrap(),
        ShellCommand::CustomObjectUndo
    );
    assert_eq!(
        parse("custom-redo").unwrap(),
        ShellCommand::CustomObjectRedo
    );
    assert!(parse("custom-undo extra").is_err());
    assert!(parse("custom-redo extra").is_err());
}

#[test]
fn custom_sprite_history_commands_are_typed_and_argument_free() {
    assert_eq!(
        parse("custom-sprite-undo").unwrap(),
        ShellCommand::CustomSpriteUndo
    );
    assert_eq!(
        parse("custom-sprite-redo").unwrap(),
        ShellCommand::CustomSpriteRedo
    );
    assert!(parse("custom-sprite-undo extra").is_err());
    assert!(parse("custom-sprite-redo extra").is_err());
}

#[test]
fn credits_tilemap_commands_preserve_paths_with_spaces() {
    assert_eq!(
        parse("credits-native-tilemap-export paths/Credits Tilemap.lmcred").unwrap(),
        ShellCommand::NativeCreditsTilemapExport("paths/Credits Tilemap.lmcred".into())
    );
    assert_eq!(
        parse("credits-native-tilemap-import paths/Credits Tilemap.lmcred").unwrap(),
        ShellCommand::NativeCreditsTilemapImport("paths/Credits Tilemap.lmcred".into())
    );
}

#[test]
fn title_tilemap_and_recording_commands_preserve_paths_with_spaces() {
    assert_eq!(
        parse("title-native-tilemap-export paths/Title Layer.lmowlyr").unwrap(),
        ShellCommand::NativeTitleTilemapExport("paths/Title Layer.lmowlyr".into())
    );
    assert_eq!(
        parse("title-native-recording-import paths/Title Demo.lmtitle").unwrap(),
        ShellCommand::NativeTitleRecordingImport("paths/Title Demo.lmtitle".into())
    );
    assert_eq!(
        parse("title-recording-zst-export paths/Title Demo.zst").unwrap(),
        ShellCommand::NativeTitleRecordingZsnesExport("paths/Title Demo.zst".into())
    );
    assert_eq!(
        parse("title-recording-s9x-import paths/Title Demo.000").unwrap(),
        ShellCommand::NativeTitleRecordingSnes9xImport("paths/Title Demo.000".into())
    );
    assert_eq!(
        parse("lm-metadata-export paths/ROM metadata.lmrommd").unwrap(),
        ShellCommand::LunarMagicMetadataExport("paths/ROM metadata.lmrommd".into())
    );
    assert_eq!(
        parse("secondary-exit-native-import paths/Secondary Exits.lmsexit").unwrap(),
        ShellCommand::NativeSecondaryExitImport("paths/Secondary Exits.lmsexit".into())
    );
}

#[test]
fn dsc_paths_preserve_spaces_and_unicode() {
    assert_eq!(
        parse("dsc-open sidecars/Display names 日本語.dsc").unwrap(),
        ShellCommand::DscSidecarOpen("sidecars/Display names 日本語.dsc".into())
    );
    assert_eq!(
        parse("dsc-replace sidecars/Replacement 日本語.dsc").unwrap(),
        ShellCommand::DscSidecarReplace("sidecars/Replacement 日本語.dsc".into())
    );
}

#[test]
fn standalone_sidecar_history_commands_are_typed_and_argument_free() {
    assert_eq!(parse("dsc-undo").unwrap(), ShellCommand::DscSidecarUndo);
    assert_eq!(parse("dsc-redo").unwrap(), ShellCommand::DscSidecarRedo);
    assert_eq!(
        parse("native-sidecar-undo").unwrap(),
        ShellCommand::NativeMap16SidecarUndo
    );
    assert_eq!(
        parse("native-sidecar-redo").unwrap(),
        ShellCommand::NativeMap16SidecarRedo
    );
    for command in [
        "dsc-undo extra",
        "dsc-redo extra",
        "native-sidecar-undo extra",
        "native-sidecar-redo extra",
    ] {
        assert!(parse(command).is_err());
    }
}

#[test]
fn complete_level_document_commands_preserve_paths_and_require_exact_arity() {
    assert_eq!(
        parse("bundle-open /tmp/My Level 日本語.lmlevel").unwrap(),
        ShellCommand::CompleteLevelDocument(CompleteLevelDocumentCommand::Open(
            "/tmp/My Level 日本語.lmlevel".into()
        ))
    );
    assert_eq!(
        parse("bundle-edit-file scripts/My auxiliary edits.lmedit").unwrap(),
        ShellCommand::CompleteLevelDocument(CompleteLevelDocumentCommand::Edit(
            "scripts/My auxiliary edits.lmedit".into()
        ))
    );
    assert_eq!(
        parse("bundle-save").unwrap(),
        ShellCommand::CompleteLevelDocument(CompleteLevelDocumentCommand::Save)
    );
    assert_eq!(
        parse("bundle-render-file specs/My render 日本語.txt").unwrap(),
        ShellCommand::CompleteLevelDocument(CompleteLevelDocumentCommand::Render(
            "specs/My render 日本語.txt".into()
        ))
    );
    assert_eq!(
        parse("bundle-undo").unwrap(),
        ShellCommand::CompleteLevelDocument(CompleteLevelDocumentCommand::Undo)
    );
    assert_eq!(
        parse("bundle-redo").unwrap(),
        ShellCommand::CompleteLevelDocument(CompleteLevelDocumentCommand::Redo)
    );
    assert!(parse("bundle-status now").is_err());
    assert!(parse("bundle-open").is_err());
}

#[test]
fn portable_render_commands_preserve_unicode_paths() {
    assert_eq!(
        parse("map16-render-file specs/My Map16 日本語.txt").unwrap(),
        ShellCommand::RenderMap16("specs/My Map16 日本語.txt".into())
    );
    assert_eq!(
        parse("overworld-render-file specs/My World 日本語.txt").unwrap(),
        ShellCommand::RenderOverworld("specs/My World 日本語.txt".into())
    );
    assert!(parse("map16-render-file").is_err());
}

#[test]
fn complete_map16_document_commands_preserve_paths_and_arity() {
    assert_eq!(
        parse("map16-set-open sets/All Map16 日本語.lm16set").unwrap(),
        ShellCommand::Map16Document(Map16DocumentCommand::Open(
            "sets/All Map16 日本語.lm16set".into()
        ))
    );
    assert_eq!(
        parse("map16-set-render-file specs/Page preview.txt").unwrap(),
        ShellCommand::Map16Document(Map16DocumentCommand::Render(
            "specs/Page preview.txt".into()
        ))
    );
    assert_eq!(
        parse("map16-set-discard").unwrap(),
        ShellCommand::Map16Document(Map16DocumentCommand::Discard)
    );
    assert_eq!(
        parse("map16-set-undo").unwrap(),
        ShellCommand::Map16Document(Map16DocumentCommand::Undo)
    );
    assert_eq!(
        parse("map16-set-redo").unwrap(),
        ShellCommand::Map16Document(Map16DocumentCommand::Redo)
    );
    assert!(parse("map16-set-save now").is_err());
}

#[test]
fn complete_overworld_document_commands_preserve_paths_and_arity() {
    assert_eq!(
        parse("world-open-file specs/Open World 日本語.txt").unwrap(),
        ShellCommand::OverworldDocument(OverworldDocumentCommand::Open(
            "specs/Open World 日本語.txt".into()
        ))
    );
    assert_eq!(
        parse("world-render-file specs/Preview World.txt").unwrap(),
        ShellCommand::OverworldDocument(OverworldDocumentCommand::Render(
            "specs/Preview World.txt".into()
        ))
    );
    assert_eq!(
        parse("world-discard").unwrap(),
        ShellCommand::OverworldDocument(OverworldDocumentCommand::Discard)
    );
    assert_eq!(
        parse("world-undo").unwrap(),
        ShellCommand::OverworldDocument(OverworldDocumentCommand::Undo)
    );
    assert_eq!(
        parse("world-redo").unwrap(),
        ShellCommand::OverworldDocument(OverworldDocumentCommand::Redo)
    );
    assert!(parse("world-save now").is_err());
}

#[test]
fn graphics_document_commands_preserve_paths_and_arity() {
    assert_eq!(
        parse("gfx-open assets/Graphics 日本語.lmgfx").unwrap(),
        ShellCommand::GraphicsDocument(GraphicsDocumentCommand::Open(
            "assets/Graphics 日本語.lmgfx".into()
        ))
    );
    assert_eq!(
        parse("gfx-render-file specs/GFX Preview.txt").unwrap(),
        ShellCommand::GraphicsDocument(GraphicsDocumentCommand::Render(
            "specs/GFX Preview.txt".into()
        ))
    );
    assert_eq!(
        parse("gfx-discard").unwrap(),
        ShellCommand::GraphicsDocument(GraphicsDocumentCommand::Discard)
    );
    assert_eq!(
        parse("gfx-undo").unwrap(),
        ShellCommand::GraphicsDocument(GraphicsDocumentCommand::Undo)
    );
    assert_eq!(
        parse("gfx-redo").unwrap(),
        ShellCommand::GraphicsDocument(GraphicsDocumentCommand::Redo)
    );
    assert!(parse("gfx-save now").is_err());
}

#[test]
fn palette_document_commands_preserve_paths_and_arity() {
    assert_eq!(
        parse("pal-open assets/Palette 日本語.lmpal").unwrap(),
        ShellCommand::PaletteDocument(PaletteDocumentCommand::Open(
            "assets/Palette 日本語.lmpal".into()
        ))
    );
    assert_eq!(
        parse("pal-render-file specs/Palette Preview.txt").unwrap(),
        ShellCommand::PaletteDocument(PaletteDocumentCommand::Render(
            "specs/Palette Preview.txt".into()
        ))
    );
    assert_eq!(
        parse("pal-discard").unwrap(),
        ShellCommand::PaletteDocument(PaletteDocumentCommand::Discard)
    );
    assert_eq!(
        parse("pal-undo").unwrap(),
        ShellCommand::PaletteDocument(PaletteDocumentCommand::Undo)
    );
    assert_eq!(
        parse("pal-redo").unwrap(),
        ShellCommand::PaletteDocument(PaletteDocumentCommand::Redo)
    );
    assert!(parse("pal-save now").is_err());
}

#[test]
fn exanimation_document_commands_preserve_paths_and_arity() {
    assert_eq!(
        parse("ex-open-file specs/Animation 日本語.txt").unwrap(),
        ShellCommand::ExAnimationDocument(ExAnimationDocumentCommand::Open(
            "specs/Animation 日本語.txt".into()
        ))
    );
    assert_eq!(
        parse("ex-edit-file scripts/Animation edits.txt").unwrap(),
        ShellCommand::ExAnimationDocument(ExAnimationDocumentCommand::Edit(
            "scripts/Animation edits.txt".into()
        ))
    );
    assert_eq!(
        parse("ex-discard").unwrap(),
        ShellCommand::ExAnimationDocument(ExAnimationDocumentCommand::Discard)
    );
    assert_eq!(
        parse("ex-undo").unwrap(),
        ShellCommand::ExAnimationDocument(ExAnimationDocumentCommand::Undo)
    );
    assert_eq!(
        parse("ex-redo").unwrap(),
        ShellCommand::ExAnimationDocument(ExAnimationDocumentCommand::Redo)
    );
    assert!(parse("ex-save now").is_err());
}

#[test]
fn hexadecimal_editor_targets_are_strict() {
    assert_eq!(
        parse("level 0x105").unwrap(),
        ShellCommand::SelectLevel(0x105)
    );
    assert_eq!(
        parse("graphics FF").unwrap(),
        ShellCommand::ShowGraphics(0xff)
    );
    assert_eq!(
        parse("layer3 105").unwrap(),
        ShellCommand::ShowLayer3(0x105)
    );
    assert!(matches!(
        parse("palette 1 2"),
        Err(ShellCommandError::UnexpectedArgument("palette"))
    ));
    assert!(matches!(
        parse("exanimation nope"),
        Err(ShellCommandError::InvalidHex { .. })
    ));
}

#[test]
fn level_history_commands_reject_arguments() {
    assert_eq!(parse("level-back").unwrap(), ShellCommand::LevelBack);
    assert_eq!(parse("level-forward").unwrap(), ShellCommand::LevelForward);
    assert!(matches!(
        parse("level-back now"),
        Err(ShellCommandError::UnexpectedArgument("level-back"))
    ));
}

#[test]
fn level_view_parses_signed_origin_and_exact_zoom() {
    assert_eq!(
        parse("level-view -32 144 3 2").unwrap(),
        ShellCommand::SetLevelViewport {
            x: -32,
            y: 144,
            zoom_numerator: 3,
            zoom_denominator: 2,
        }
    );
    assert!(parse("level-view 1 2 3").is_err());
    assert!(parse("level-view east 2 3 4").is_err());
}

#[test]
fn level_header_edit_requires_a_field_value_and_explicit_search_range() {
    assert_eq!(
        parse("level-header mode 1f 300000 400000").unwrap(),
        ShellCommand::EditLevelHeader {
            field: LevelHeaderField::LevelMode,
            value: 0x1f,
            search_start: 0x30_0000,
            search_end: 0x40_0000,
        }
    );
    assert!(matches!(
        parse("level-header unknown 1 100 200"),
        Err(ShellCommandError::InvalidLevelHeaderField(_))
    ));
    assert!(parse("level-header mode 100 100 200").is_err());
    assert!(parse("level-header mode 1 100").is_err());
    assert_eq!(
        parse("level-header sprite-tileset f 300000 400000").unwrap(),
        ShellCommand::EditLevelHeader {
            field: LevelHeaderField::SpriteTileset,
            value: 0x0f,
            search_start: 0x30_0000,
            search_end: 0x40_0000,
        }
    );
    assert_eq!(
        parse("level-header object-tileset a 300000 400000").unwrap(),
        ShellCommand::EditLevelHeader {
            field: LevelHeaderField::ObjectTileset,
            value: 0x0a,
            search_start: 0x30_0000,
            search_end: 0x40_0000,
        }
    );
}

#[test]
fn owned_level_edit_parses_paths_and_reclamation_arguments() {
    assert_eq!(
        parse("level-edit-owned scripts/My level edits.lmedit 10000 20000 evidence.lmrats")
            .unwrap(),
        ShellCommand::ApplyOwnedEditorScript {
            editor: ScriptEditor::Level,
            script: "scripts/My level edits.lmedit".into(),
            ownership_manifest: "evidence.lmrats".into(),
            search_start: 0x1_0000,
            search_end: 0x2_0000,
        }
    );
}

#[test]
fn owned_map16_edit_parses_paths_and_reclamation_arguments() {
    assert_eq!(
        parse("map16-edit-owned scripts/My map edits.lmedit 10000 20000 evidence.lmrats").unwrap(),
        ShellCommand::ApplyOwnedEditorScript {
            editor: ScriptEditor::Map16,
            script: "scripts/My map edits.lmedit".into(),
            ownership_manifest: "evidence.lmrats".into(),
            search_start: 0x1_0000,
            search_end: 0x2_0000,
        }
    );
}

#[test]
fn owned_overworld_edit_parses_paths_and_reclamation_arguments() {
    assert_eq!(
        parse("overworld-edit-owned scripts/My world edits.lmedit 10000 20000 evidence.lmrats")
            .unwrap(),
        ShellCommand::ApplyOwnedEditorScript {
            editor: ScriptEditor::Overworld,
            script: "scripts/My world edits.lmedit".into(),
            ownership_manifest: "evidence.lmrats".into(),
            search_start: 0x1_0000,
            search_end: 0x2_0000,
        }
    );
}

#[test]
fn owned_native_assets_edit_parses_paths_and_reclamation_arguments() {
    assert_eq!(
        parse("native-assets-edit-owned Specs/My aggregate edit.txt 10000 20000 evidence.lmrats")
            .unwrap(),
        ShellCommand::ApplyOwnedEditorScript {
            editor: ScriptEditor::NativeAssets,
            script: "Specs/My aggregate edit.txt".into(),
            ownership_manifest: "evidence.lmrats".into(),
            search_start: 0x1_0000,
            search_end: 0x2_0000,
        }
    );
}

#[test]
fn level_edit_script_preserves_path_spaces_and_parses_hex_range_from_the_right() {
    assert_eq!(
        parse("level-edit scripts/My level edits.lmedit 300000 400000").unwrap(),
        ShellCommand::ApplyEditorScript {
            editor: ScriptEditor::Level,
            script: "scripts/My level edits.lmedit".into(),
            search_start: 0x30_0000,
            search_end: 0x40_0000,
        }
    );
    assert!(parse("level-edit only-a-path").is_err());
    assert_eq!(
        parse("native-assets-edit Specs/My aggregate edit.txt 10000 20000").unwrap(),
        ShellCommand::ApplyEditorScript {
            editor: ScriptEditor::NativeAssets,
            script: "Specs/My aggregate edit.txt".into(),
            search_start: 0x1_0000,
            search_end: 0x2_0000,
        }
    );
    assert_eq!(
        parse("map16-edit scripts/My map edits.lmedit 10000 20000").unwrap(),
        ShellCommand::ApplyEditorScript {
            editor: ScriptEditor::Map16,
            script: "scripts/My map edits.lmedit".into(),
            search_start: 0x1_0000,
            search_end: 0x2_0000,
        }
    );
    assert_eq!(
        parse("palette-edit scripts/My palette edits.lmedit 10000 20000").unwrap(),
        ShellCommand::ApplyEditorScript {
            editor: ScriptEditor::Palette,
            script: "scripts/My palette edits.lmedit".into(),
            search_start: 0x1_0000,
            search_end: 0x2_0000,
        }
    );
    assert_eq!(
        parse("graphics-edit scripts/My graphics edits.lmedit 10000 20000").unwrap(),
        ShellCommand::ApplyEditorScript {
            editor: ScriptEditor::Graphics,
            script: "scripts/My graphics edits.lmedit".into(),
            search_start: 0x1_0000,
            search_end: 0x2_0000,
        }
    );
    assert_eq!(
        parse("graphics-edit-owned scripts/My graphics edits.lmedit 10000 20000 evidence.lmrats")
            .unwrap(),
        ShellCommand::ApplyOwnedEditorScript {
            editor: ScriptEditor::Graphics,
            script: "scripts/My graphics edits.lmedit".into(),
            ownership_manifest: "evidence.lmrats".into(),
            search_start: 0x1_0000,
            search_end: 0x2_0000,
        }
    );
    assert_eq!(
        parse("palette-edit-owned scripts/My palette edits.lmedit 10000 20000 evidence.lmrats")
            .unwrap(),
        ShellCommand::ApplyOwnedEditorScript {
            editor: ScriptEditor::Palette,
            script: "scripts/My palette edits.lmedit".into(),
            ownership_manifest: "evidence.lmrats".into(),
            search_start: 0x1_0000,
            search_end: 0x2_0000,
        }
    );
    assert_eq!(
        parse("exanimation-edit scripts/My animation edits.lmedit 20000 30000").unwrap(),
        ShellCommand::ApplyEditorScript {
            editor: ScriptEditor::ExAnimation,
            script: "scripts/My animation edits.lmedit".into(),
            search_start: 0x2_0000,
            search_end: 0x3_0000,
        }
    );
    assert_eq!(
        parse(
            "exanimation-edit-owned scripts/My animation edits.lmedit 20000 30000 evidence.lmrats"
        )
        .unwrap(),
        ShellCommand::ApplyOwnedEditorScript {
            editor: ScriptEditor::ExAnimation,
            script: "scripts/My animation edits.lmedit".into(),
            ownership_manifest: "evidence.lmrats".into(),
            search_start: 0x2_0000,
            search_end: 0x3_0000,
        }
    );
    assert_eq!(
        parse("overworld-edit scripts/My overworld edits.lmedit 30000 40000").unwrap(),
        ShellCommand::ApplyEditorScript {
            editor: ScriptEditor::Overworld,
            script: "scripts/My overworld edits.lmedit".into(),
            search_start: 0x3_0000,
            search_end: 0x4_0000,
        }
    );
}

#[test]
fn graphics_recompression_parses_a_typed_codec_and_hex_range() {
    assert_eq!(
        parse("graphics-recompress lz3 10000 20000").unwrap(),
        ShellCommand::MigrateGraphicsCompression {
            target: lm_project::GraphicsCompression::Lz3,
            search_start: 0x1_0000,
            search_end: 0x2_0000,
        }
    );
    assert!(matches!(
        parse("graphics-recompress lz4 10000 20000"),
        Err(ShellCommandError::InvalidGraphicsCompression(value)) if value == "lz4"
    ));
    assert!(matches!(
        parse("graphics-recompress lz2 10000"),
        Err(ShellCommandError::MissingArgument("graphics-recompress"))
    ));
    assert!(matches!(
        parse("graphics-recompress lz2 10000 20000 extra"),
        Err(ShellCommandError::UnexpectedArgument("graphics-recompress"))
    ));
}

#[test]
fn rom_expansion_parses_target_and_fill() {
    assert_eq!(
        parse("rom-expand 100000 ff").unwrap(),
        ShellCommand::ExpandRom {
            target_logical_len: 0x10_0000,
            fill: 0xff,
        }
    );
    assert!(matches!(
        parse("rom-expand 100000"),
        Err(ShellCommandError::MissingArgument("rom-expand"))
    ));
    assert!(matches!(
        parse("rom-expand 100000 ff extra"),
        Err(ShellCommandError::UnexpectedArgument("rom-expand"))
    ));
    assert!(parse("rom-expand 100000 100").is_err());
}

#[test]
fn ips_commands_accept_one_specification_path() {
    assert_eq!(
        parse("ips-create Specs/Create Patch 日本語.txt").unwrap(),
        ShellCommand::IpsCreate("Specs/Create Patch 日本語.txt".into())
    );
    assert_eq!(
        parse("ips-apply Specs/Apply Patch 日本語.txt").unwrap(),
        ShellCommand::IpsApply("Specs/Apply Patch 日本語.txt".into())
    );
    assert!(matches!(
        parse("ips-create"),
        Err(ShellCommandError::MissingArgument("ips-create"))
    ));
}

#[test]
fn copier_header_commands_accept_one_specification_path() {
    assert_eq!(
        parse("copier-header-add Specs/Add Header 日本語.txt").unwrap(),
        ShellCommand::CopierHeaderAdd("Specs/Add Header 日本語.txt".into())
    );
    assert_eq!(
        parse("copier-header-remove Specs/Remove Header 日本語.txt").unwrap(),
        ShellCommand::CopierHeaderRemove("Specs/Remove Header 日本語.txt".into())
    );
}

#[test]
fn arity_and_unknown_commands_are_explicit() {
    assert_eq!(parse("  ").unwrap(), ShellCommand::Empty);
    assert!(matches!(
        parse("open"),
        Err(ShellCommandError::MissingArgument("open"))
    ));
    assert!(matches!(
        parse("undo now"),
        Err(ShellCommandError::UnexpectedArgument("undo"))
    ));
    assert!(matches!(
        parse("profile-clear now"),
        Err(ShellCommandError::UnexpectedArgument("profile-clear"))
    ));
    assert_eq!(
        parse("mystery anything").unwrap(),
        ShellCommand::Unknown("mystery".into())
    );
    assert_eq!(parse("recent").unwrap(), ShellCommand::Recent);
    assert_eq!(parse("open-recent 3").unwrap(), ShellCommand::OpenRecent(3));
    assert!(parse("open-recent nope").is_err());
}

#[test]
fn expanded_settings_document_commands_preserve_unicode_paths() {
    assert_eq!(
        parse("expanded-settings-open Records/設定 105.bin").unwrap(),
        ShellCommand::ExpandedSettingsDocument(ExpandedSettingsDocumentCommand::Open(
            "Records/設定 105.bin".into()
        ))
    );
    assert_eq!(
        parse("expanded-settings-edit-file Scripts/設定 edit.txt").unwrap(),
        ShellCommand::ExpandedSettingsDocument(ExpandedSettingsDocumentCommand::Edit(
            "Scripts/設定 edit.txt".into()
        ))
    );
    assert!(matches!(
        parse("expanded-settings-save extra"),
        Err(ShellCommandError::UnexpectedArgument(
            "expanded-settings-save"
        ))
    ));
    assert_eq!(
        parse("expanded-settings-undo").unwrap(),
        ShellCommand::ExpandedSettingsDocument(ExpandedSettingsDocumentCommand::Undo)
    );
    assert_eq!(
        parse("expanded-settings-redo").unwrap(),
        ShellCommand::ExpandedSettingsDocument(ExpandedSettingsDocumentCommand::Redo)
    );
}

#[test]
fn native_expanded_settings_word_edit_is_typed_and_bounded() {
    assert_eq!(
        parse("expanded-settings-word f a55a").unwrap(),
        ShellCommand::EditExpandedSettingsWord {
            index: 0xf,
            value: 0xa55a,
        }
    );
    assert!(matches!(
        parse("expanded-settings-word f"),
        Err(ShellCommandError::MissingArgument("expanded-settings-word"))
    ));
    assert!(parse("expanded-settings-word 0 10000").is_err());
    assert_eq!(
        parse("expanded-settings-edit Scripts/設定 batch.txt").unwrap(),
        ShellCommand::EditExpandedSettings("Scripts/設定 batch.txt".into())
    );
}
