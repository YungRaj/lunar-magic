use super::*;

#[test]
fn custom_object_shell_opens_edits_saves_and_closes_paired_sidecars() {
    let directory =
        std::env::temp_dir().join(format!("lm-app-custom-shell-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let data = directory.join("My Objects.mw0");
    let descriptions = directory.join("My Objects.mw0t");
    let script = directory.join("Custom edits.lmedit");
    fs::write(&data, [1, 0, 3, 0xff]).unwrap();
    fs::write(&descriptions, b"Original\n").unwrap();
    fs::write(
        &script,
        "LMCUSED1\nreplace 0 020004 4368616e676564\ninsert 1 030005 5365636f6e6420e29883\nformat no-bom lf trailing\n",
    )
    .unwrap();
    let mut session = None;
    open_custom_objects(&mut session, &data).unwrap();
    edit_custom_objects(&mut session, &script).unwrap();
    navigate_custom_object_history(&mut session, true).unwrap();
    assert_eq!(session.as_ref().unwrap().library().entries().len(), 1);
    assert_eq!(
        session.as_ref().unwrap().library().entries()[0].description,
        "Original"
    );
    navigate_custom_object_history(&mut session, false).unwrap();
    assert_eq!(session.as_ref().unwrap().revision(), 3);
    assert!(session.as_ref().unwrap().is_modified());
    assert!(close_custom_objects(&mut session, false).is_err());
    save_custom_objects(&mut session).unwrap();
    let decoded = lm_level::CustomObjectLibrary::decode(
        &fs::read(&data).unwrap(),
        &fs::read(&descriptions).unwrap(),
    )
    .unwrap();
    assert_eq!(decoded.entries().len(), 2);
    assert_eq!(decoded.entries()[1].description, "Second ☃");
    assert!(!decoded.description_format().utf8_bom);
    close_custom_objects(&mut session, false).unwrap();
    assert!(session.is_none());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn failed_custom_object_pair_save_releases_pending_slot_for_retry() {
    let directory =
        std::env::temp_dir().join(format!("lm-app-custom-retry-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let data = directory.join("objects.mw0");
    let descriptions = directory.join("objects.mw0t");
    let script = directory.join("edits.lmedit");
    fs::write(&data, [1, 0, 3, 0xff]).unwrap();
    fs::write(&descriptions, b"Original\n").unwrap();
    fs::write(&script, "LMCUSED1\nreplace 0 020004 4368616e676564\n").unwrap();
    let mut session = None;
    open_custom_objects(&mut session, &data).unwrap();
    edit_custom_objects(&mut session, &script).unwrap();
    fs::remove_file(&descriptions).unwrap();
    assert!(save_custom_objects(&mut session).is_err());
    let controller = session.as_ref().unwrap();
    assert!(controller.is_modified());
    assert!(!controller.save_pending());
    assert_eq!(fs::read(&data).unwrap(), [1, 0, 3, 0xff]);
    fs::write(&descriptions, b"Original\n").unwrap();
    save_custom_objects(&mut session).unwrap();
    assert!(!session.as_ref().unwrap().is_modified());
    fs::remove_dir_all(directory).unwrap();
}

fn metadata_fixture() -> lm_overworld::OverworldMetadata {
    lm_overworld::OverworldMetadata {
        level_names: vec![lm_overworld::OverworldLevelName {
            level: 0x105,
            tiles: [1; lm_overworld::OverworldLevelName::TILE_COUNT],
            raw_flags: 0x80,
        }],
        player_starts: vec![lm_overworld::PlayerStart {
            player: 0,
            x: 1,
            y: 2,
            submap: Submap::Main,
            raw_flags: 0xa0,
        }],
        submap_settings: vec![lm_overworld::SubmapSettings {
            submap: Submap::Main,
            music: 1,
            palette: 2,
            layer1_scroll: 3,
            layer2_scroll: 4,
            raw_flags: 0x8123,
            unknown: [5, 6, 7, 8, 9],
        }],
    }
}

#[test]
fn metadata_shell_edits_all_domains_saves_round_trips_and_closes() {
    let directory =
        std::env::temp_dir().join(format!("lm-app-metadata-shell-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let document = directory.join("Overworld metadata.lmowmeta");
    let script = directory.join("Metadata edits.lmedit");
    fs::write(&document, metadata_fixture().encode_file().unwrap()).unwrap();
    fs::write(
        &script,
        "LMOMEDT1\nname upsert 105 81 12121212121212121212121212121212121212\nname upsert 106 40 13131313131313131313131313131313131313\nstart upsert 0 1234 5678 6 a1\nsettings upsert 0 7 8 9 a 9234 0a0b0c0d0e\n",
    )
    .unwrap();
    let mut session = None;
    open_metadata_document(&mut session, &document).unwrap();
    edit_metadata_document(&mut session, &script).unwrap();
    navigate_metadata_history(&mut session, true).unwrap();
    assert_eq!(session.as_ref().unwrap().metadata(), &metadata_fixture());
    navigate_metadata_history(&mut session, false).unwrap();
    assert!(close_metadata_document(&mut session, false).is_err());
    save_metadata_document(&mut session).unwrap();
    let decoded =
        lm_overworld::OverworldMetadata::decode_file(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(decoded.level_names.len(), 2);
    assert_eq!(decoded.level_names[0].raw_flags, 0x81);
    assert_eq!(decoded.player_starts[0].submap, Submap::StarWorld);
    assert_eq!(decoded.submap_settings[0].unknown, [10, 11, 12, 13, 14]);
    close_metadata_document(&mut session, false).unwrap();
    assert!(session.is_none());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn failed_metadata_save_preserves_dirty_state_and_is_retryable() {
    let directory =
        std::env::temp_dir().join(format!("lm-app-metadata-retry-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let document = directory.join("metadata.lmowmeta");
    let script = directory.join("edits.lmedit");
    let original = metadata_fixture().encode_file().unwrap();
    fs::write(&document, &original).unwrap();
    fs::write(&script, "LMOMEDT1\nname remove 105\n").unwrap();
    let mut session = None;
    open_metadata_document(&mut session, &document).unwrap();
    edit_metadata_document(&mut session, &script).unwrap();
    fs::remove_file(&document).unwrap();
    assert!(save_metadata_document(&mut session).is_err());
    assert!(session.as_ref().unwrap().is_modified());
    assert!(!session.as_ref().unwrap().save_pending());
    fs::write(&document, &original).unwrap();
    save_metadata_document(&mut session).unwrap();
    let decoded =
        lm_overworld::OverworldMetadata::decode_file(&fs::read(&document).unwrap()).unwrap();
    assert!(decoded.level_names.is_empty());
    fs::remove_dir_all(directory).unwrap();
}

fn path_fixture() -> lm_overworld::OverworldPathGraph {
    let mut edge = lm_overworld::PathEdge {
        from: 1,
        to: 2,
        direction: lm_overworld::PathDirection::Right,
        exit_index: None,
        raw_flags: 0,
    };
    edge.set_one_way(true);
    lm_overworld::OverworldPathGraph {
        nodes: vec![
            lm_overworld::PathNode {
                id: 1,
                x: 1,
                y: 2,
                submap: Submap::Main,
                level: Some(0x105),
                raw_flags: 0x80,
            },
            lm_overworld::PathNode {
                id: 2,
                x: 3,
                y: 4,
                submap: Submap::Main,
                level: None,
                raw_flags: 0x40,
            },
        ],
        edges: vec![edge],
    }
}

#[test]
fn path_shell_edits_nodes_edges_saves_round_trips_and_closes() {
    let directory = std::env::temp_dir().join(format!("lm-app-path-shell-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let document = directory.join("Overworld paths.lmowpath");
    let script = directory.join("Path edits.lmedit");
    fs::write(&document, path_fixture().encode_file().unwrap()).unwrap();
    fs::write(
        &script,
        "LMOPEDT1\nnode upsert 1 123 456 0 105 81\nnode upsert 3 7 8 6 none 20\nedge upsert 2 3 down none 00\nedge upsert 3 2 up fe 00\n",
    )
    .unwrap();
    let mut session = None;
    open_path_document(&mut session, &document).unwrap();
    edit_path_document(&mut session, &script).unwrap();
    navigate_path_history(&mut session, true).unwrap();
    assert_eq!(session.as_ref().unwrap().graph(), &path_fixture());
    navigate_path_history(&mut session, false).unwrap();
    assert!(close_path_document(&mut session, false).is_err());
    save_path_document(&mut session).unwrap();
    let decoded =
        lm_overworld::OverworldPathGraph::decode_file(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(decoded.nodes.len(), 3);
    assert_eq!(decoded.nodes[0].x, 0x123);
    assert_eq!(decoded.nodes[2].submap, Submap::StarWorld);
    assert_eq!(decoded.edges.len(), 3);
    decoded.validate_reciprocal().unwrap();
    close_path_document(&mut session, false).unwrap();
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn invalid_path_batch_and_failed_save_preserve_retryable_state() {
    let directory = std::env::temp_dir().join(format!("lm-app-path-retry-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let document = directory.join("paths.lmowpath");
    let bad_script = directory.join("bad.lmedit");
    let good_script = directory.join("good.lmedit");
    let original = path_fixture().encode_file().unwrap();
    fs::write(&document, &original).unwrap();
    fs::write(
        &bad_script,
        "LMOPEDT1\nnode upsert 1 9 9 0 105 80\nedge upsert 2 9 down none 01\n",
    )
    .unwrap();
    fs::write(&good_script, "LMOPEDT1\nnode upsert 1 9 9 0 105 80\n").unwrap();
    let mut session = None;
    open_path_document(&mut session, &document).unwrap();
    let before = session.as_ref().unwrap().graph().clone();
    assert!(edit_path_document(&mut session, &bad_script).is_err());
    assert_eq!(session.as_ref().unwrap().graph(), &before);
    edit_path_document(&mut session, &good_script).unwrap();
    fs::remove_file(&document).unwrap();
    assert!(save_path_document(&mut session).is_err());
    assert!(session.as_ref().unwrap().is_modified());
    assert!(!session.as_ref().unwrap().save_pending());
    fs::write(&document, &original).unwrap();
    save_path_document(&mut session).unwrap();
    let decoded =
        lm_overworld::OverworldPathGraph::decode_file(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(decoded.nodes[0].x, 9);
    fs::remove_dir_all(directory).unwrap();
}

fn layer3_fixture() -> lm_level::Layer3File {
    lm_level::Layer3File(lm_level::Layer3Data {
        settings: lm_level::Layer3Settings {
            start_position: 1,
            tilemap_size: 2,
            liquid_type: 3,
            flags: 4,
            graphics_files: [0, 1, 2, 3],
            reserved: [0x55; 16],
        },
        tilemap: vec![0, 1, 2, 3],
        remap_commands: vec![0xfe, 7],
    })
}

#[test]
fn layer3_shell_edits_complete_portable_surface_and_round_trips() {
    let directory =
        std::env::temp_dir().join(format!("lm-app-layer3-shell-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let document = directory.join("Layer 3.lmlayer3");
    let script = directory.join("Layer 3 edits.lmedit");
    fs::write(&document, layer3_fixture().encode().unwrap()).unwrap();
    fs::write(
        &script,
        "LML3EDT1\nstart fe\nsize 03\nliquid 81\nflags a5\ngraphics 2 abc\nreserved 5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a\ntilemap 00010203\ntilemap-range 1 aabb\nremap fe0708\n",
    )
    .unwrap();
    let mut session = None;
    open_layer3_document(&mut session, &document).unwrap();
    edit_layer3_document(&mut session, &script).unwrap();
    navigate_layer3_document_history(&mut session, true).unwrap();
    assert_eq!(session.as_ref().unwrap().value(), &layer3_fixture());
    navigate_layer3_document_history(&mut session, false).unwrap();
    assert!(close_layer3_document(&mut session, false).is_err());
    save_layer3_document(&mut session).unwrap();
    let decoded = lm_level::Layer3File::decode(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(decoded.0.settings.start_position, 0xfe);
    assert_eq!(decoded.0.settings.graphics_files[2], 0xabc);
    assert_eq!(decoded.0.settings.reserved, [0x5a; 16]);
    assert_eq!(decoded.0.tilemap, [0, 0xaa, 0xbb, 3]);
    assert_eq!(decoded.0.remap_commands, [0xfe, 7, 8]);
    close_layer3_document(&mut session, false).unwrap();
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn invalid_layer3_batch_and_failed_save_are_atomic_and_retryable() {
    let directory =
        std::env::temp_dir().join(format!("lm-app-layer3-retry-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let document = directory.join("layer3.lmlayer3");
    let bad_script = directory.join("bad.lmedit");
    let good_script = directory.join("good.lmedit");
    let original = layer3_fixture().encode().unwrap();
    fs::write(&document, &original).unwrap();
    fs::write(&bad_script, "LML3EDT1\nflags 80\ngraphics 0 1000\n").unwrap();
    fs::write(&good_script, "LML3EDT1\nflags 80\n").unwrap();
    let mut session = None;
    open_layer3_document(&mut session, &document).unwrap();
    let before = session.as_ref().unwrap().value().clone();
    assert!(edit_layer3_document(&mut session, &bad_script).is_err());
    assert_eq!(session.as_ref().unwrap().value(), &before);
    edit_layer3_document(&mut session, &good_script).unwrap();
    fs::remove_file(&document).unwrap();
    assert!(save_layer3_document(&mut session).is_err());
    assert!(session.as_ref().unwrap().is_modified());
    assert!(!session.as_ref().unwrap().save_pending());
    fs::write(&document, &original).unwrap();
    save_layer3_document(&mut session).unwrap();
    assert_eq!(
        lm_level::Layer3File::decode(&fs::read(&document).unwrap())
            .unwrap()
            .0
            .settings
            .flags,
        0x80
    );
    fs::remove_dir_all(directory).unwrap();
}
