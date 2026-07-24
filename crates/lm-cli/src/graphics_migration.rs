use crate::args::GraphicsMigrationCommand;
use crate::atomic_output::write_new;
use crate::oracle_input::read_rom;
use lm_project::{GraphicsMigrationOptions, GraphicsRomLayout, LevelPointerTable, Project};
use lm_rats::AllocationPolicy;
use lm_rom::RomImage;
#[cfg(test)]
use std::fs;

pub fn execute(command: &GraphicsMigrationCommand) -> Result<(), Box<dyn std::error::Error>> {
    if command.input_rom == command.output_rom {
        return Err("refusing to overwrite the input ROM; choose a different output path".into());
    }
    if command.source_compression == command.target_compression {
        return Err("source and target graphics compression must differ".into());
    }
    let mut project = Project::new(RomImage::from_bytes(read_rom(&command.input_rom)?)?);
    if command.search_start >= command.search_end || command.search_end > project.rom.logical_len()
    {
        return Err("allocation search range must be nonempty and inside the logical ROM".into());
    }
    let source = GraphicsRomLayout {
        mapper: command.mapper,
        pointers: LevelPointerTable {
            offset: command.pointer_table,
            entries: command.entries,
            stride: 3,
        },
        compression: command.source_compression,
        maximum_compressed_len: command.maximum_compressed_len,
        maximum_decompressed_len: command.maximum_decompressed_len,
    };
    project.migrate_graphics_compression(
        source,
        command.target_compression,
        &GraphicsMigrationOptions {
            allocation: AllocationPolicy {
                search: command.search_start..command.search_end,
                bank_size: Some(0x8000),
                fill_bytes: vec![0x00, 0xff],
                protected: vec![],
            },
            reuse_identical: false,
            erase_fill: 0xff,
            checksum_field: command.checksum_field,
        },
    )?;
    write_new(&command.output_rom, project.save_snapshot())?;
    println!(
        "recompressed-graphics: {:?} -> {:?}",
        command.source_compression, command.target_compression
    );
    println!("slots: {}", command.entries);
    println!("output: {}", command.output_rom.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{GraphicsFile4bpp, IndexedTile};
    use lm_project::{GraphicsCompression, GraphicsRomLayout, GraphicsSaveOptions};
    use lm_rats::ProtectedRange;
    use lm_rom::{Mapper, SnesChecksum, compute_snes_checksum};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn layout(compression: GraphicsCompression) -> GraphicsRomLayout {
        GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x200,
                entries: 2,
                stride: 3,
            },
            compression,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x10000,
        }
    }

    fn graphics(value: u8) -> GraphicsFile4bpp {
        GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([value; IndexedTile::PIXEL_COUNT])],
        }
    }

    #[test]
    fn copy_on_write_command_migrates_every_slot_and_refuses_replacement() {
        let directory = std::env::temp_dir().join(format!(
            "lm-cli-graphics-migration-日本語-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let input = directory.join("Input graphics.smc");
        let output = directory.join("Output graphics.smc");
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
        for (slot, value) in [3, 7].into_iter().enumerate() {
            project
                .save_graphics_file(
                    slot,
                    &graphics(value),
                    layout(GraphicsCompression::Lz2),
                    &GraphicsSaveOptions {
                        allocation: AllocationPolicy {
                            search: 0x1000..0x7000,
                            bank_size: Some(0x8000),
                            fill_bytes: vec![0xff],
                            protected: vec![ProtectedRange(0x200..0x206)],
                        },
                        previous_block: None,
                        reuse_identical: false,
                        erase_fill: 0xff,
                    },
                )
                .unwrap();
        }
        project.refresh_checksum(0x7fdc).unwrap();
        fs::write(&input, project.save_snapshot()).unwrap();
        let command = GraphicsMigrationCommand {
            input_rom: input.clone(),
            output_rom: output.clone(),
            mapper: Mapper::LoRom,
            pointer_table: 0x200,
            entries: 2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x10000,
            source_compression: GraphicsCompression::Lz2,
            target_compression: GraphicsCompression::Lz3,
            checksum_field: 0x7fdc,
            search_start: 0x1000,
            search_end: 0x7000,
        };
        execute(&command).unwrap();
        let bytes = fs::read(&output).unwrap();
        let reopened = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        for (slot, value) in [3, 7].into_iter().enumerate() {
            assert_eq!(
                reopened
                    .load_graphics_file(slot, layout(GraphicsCompression::Lz3))
                    .unwrap(),
                graphics(value)
            );
        }
        assert_eq!(
            SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
            compute_snes_checksum(&bytes, 0x7fdc).unwrap()
        );
        assert!(execute(&command).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
