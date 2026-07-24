use super::*;
use crate::{GraphicsSaveOptions, LevelPointerTable};
use lm_graphics::{GraphicsFile4bpp, IndexedTile};
use lm_rats::AllocationPolicy;
use lm_rom::{Mapper, RomImage, SnesChecksum, compute_snes_checksum};

fn layout(compression: GraphicsCompression) -> GraphicsRomLayout {
    GraphicsRomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: 0x200,
            entries: 3,
            stride: 3,
        },
        split_pointer_planes: None,
        compression,
        maximum_compressed_len: 0x8000,
        maximum_decompressed_len: 0x10000,
    }
}

fn file(seed: u8) -> GraphicsFile4bpp {
    GraphicsFile4bpp {
        tiles: vec![
            IndexedTile::new(std::array::from_fn(|index| {
                seed.wrapping_add(index.to_le_bytes()[0]) & 0x0f
            })),
            IndexedTile::new([seed & 0x0f; IndexedTile::PIXEL_COUNT]),
        ],
    }
}

fn source_project() -> (Project, Vec<GraphicsFile4bpp>) {
    source_project_with_header(false)
}

fn source_project_with_header(headered: bool) -> (Project, Vec<GraphicsFile4bpp>) {
    let mut bytes = vec![0xff; 0x10000];
    if headered {
        bytes.splice(0..0, vec![0x5a; 512]);
    }
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let files = vec![file(1), file(5), file(9)];
    for (slot, graphics) in files.iter().enumerate() {
        project
            .save_graphics_file(
                slot,
                graphics,
                layout(GraphicsCompression::Lz2),
                &GraphicsSaveOptions {
                    allocation: AllocationPolicy {
                        search: 0x1000..0x7000,
                        bank_size: Some(0x8000),
                        fill_bytes: vec![0xff],
                        protected: vec![],
                    },
                    previous_block: None,
                    reuse_identical: false,
                    erase_fill: 0xff,
                },
            )
            .unwrap();
    }
    project.refresh_checksum(0x7fdc).unwrap();
    project.mark_saved();
    project.history.clear();
    (project, files)
}

fn options(search: std::ops::Range<usize>) -> GraphicsMigrationOptions {
    GraphicsMigrationOptions {
        allocation: AllocationPolicy {
            search,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![],
        },
        reuse_identical: false,
        erase_fill: 0xff,
        checksum_field: 0x7fdc,
    }
}

#[test]
fn whole_set_migration_is_one_undoable_verified_checksum_repaired_operation() {
    let (mut project, files) = source_project();
    let before = project.save_snapshot();
    assert!(
        project
            .migrate_graphics_compression(
                layout(GraphicsCompression::Lz2),
                GraphicsCompression::Lz3,
                &options(0x1000..0x7000),
            )
            .unwrap()
    );
    for (slot, expected) in files.iter().enumerate() {
        assert_eq!(
            project
                .load_graphics_file(slot, layout(GraphicsCompression::Lz3))
                .unwrap(),
            *expected
        );
    }
    let logical = project.rom.logical_bytes();
    assert_eq!(
        SnesChecksum::decode(logical, 0x7fdc).unwrap(),
        compute_snes_checksum(logical, 0x7fdc).unwrap()
    );
    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.save_snapshot(), before);
    assert!(!project.history.can_undo());
    assert!(project.history.redo(&mut project.rom).unwrap());
    for (slot, expected) in files.iter().enumerate() {
        assert_eq!(
            project
                .load_graphics_file(slot, layout(GraphicsCompression::Lz3))
                .unwrap(),
            *expected
        );
    }
}

#[test]
fn late_allocation_failure_and_same_codec_request_are_non_mutating() {
    let (mut project, _) = source_project();
    let before = project.save_snapshot();
    assert!(
        !project
            .migrate_graphics_compression(
                layout(GraphicsCompression::Lz2),
                GraphicsCompression::Lz2,
                &options(0x1000..0x7000),
            )
            .unwrap()
    );
    assert!(
        project
            .migrate_graphics_compression(
                layout(GraphicsCompression::Lz2),
                GraphicsCompression::Lz3,
                &options(0x300..0x301),
            )
            .is_err()
    );
    assert_eq!(project.save_snapshot(), before);
    assert!(!project.history.can_undo());
}

#[test]
fn migration_preserves_copier_header_bytes() {
    let (mut project, files) = source_project_with_header(true);
    let before = project.save_snapshot();
    assert_eq!(&before[..512], &[0x5a; 512]);
    project
        .migrate_graphics_compression(
            layout(GraphicsCompression::Lz2),
            GraphicsCompression::Lz3,
            &options(0x1000..0x7000),
        )
        .unwrap();
    let after = project.save_snapshot();
    assert_eq!(&after[..512], &before[..512]);
    for (slot, expected) in files.iter().enumerate() {
        assert_eq!(
            project
                .load_graphics_file(slot, layout(GraphicsCompression::Lz3))
                .unwrap(),
            *expected
        );
    }
}
