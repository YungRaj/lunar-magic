use crate::args::ExpandedSettingsTransferCommand;
use crate::atomic_output::write_new;
use crate::oracle_input::{read_bounded, read_rom};
use lm_level::ExpandedLevelSettingsRecord;
use lm_project::{ExpandedLevelSettingsLayout, Project};
use lm_rom::{Mapper, RomImage};
use std::path::Path;

pub fn execute(command: ExpandedSettingsTransferCommand) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        ExpandedSettingsTransferCommand::Export {
            rom,
            mapper,
            slot,
            table_offset,
            entries,
            stride,
            output,
        } => export(
            &rom,
            slot,
            layout(mapper, table_offset, entries, stride),
            &output,
        ),
        ExpandedSettingsTransferCommand::Import {
            input_rom,
            output_rom,
            mapper,
            slot,
            table_offset,
            entries,
            stride,
            record,
            checksum_field,
        } => import(
            &input_rom,
            &output_rom,
            slot,
            layout(mapper, table_offset, entries, stride),
            &record,
            checksum_field,
        ),
    }
}

fn export(
    rom: &Path,
    slot: usize,
    layout: ExpandedLevelSettingsLayout,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(rom, output)?;
    let project = Project::new(RomImage::from_bytes(read_rom(rom)?)?);
    let record = project.load_expanded_level_settings(slot, layout)?;
    write_new(output, record.encoded())?;
    println!("exported-expanded-settings: {slot:#05x}");
    println!("output: {}", output.display());
    Ok(())
}

fn import(
    input_rom: &Path,
    output_rom: &Path,
    slot: usize,
    layout: ExpandedLevelSettingsLayout,
    record_path: &Path,
    checksum_field: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(input_rom, output_rom)?;
    if output_rom == record_path {
        return Err("record and output ROM paths must differ".into());
    }
    let record = ExpandedLevelSettingsRecord::decode(&read_bounded(
        record_path,
        ExpandedLevelSettingsRecord::ENCODED_LEN,
    )?)?;
    let mut project = Project::new(RomImage::from_bytes(read_rom(input_rom)?)?);
    project.save_expanded_level_settings(slot, &record, layout, checksum_field)?;
    let snapshot = project.save_snapshot();
    let reopened = Project::new(RomImage::from_bytes(snapshot.clone())?);
    if reopened.load_expanded_level_settings(slot, layout)? != record {
        return Err("saved expanded settings failed semantic reopen verification".into());
    }
    write_new(output_rom, snapshot)?;
    println!("imported-expanded-settings: {slot:#05x}");
    println!("output: {}", output_rom.display());
    Ok(())
}

const fn layout(
    mapper: Mapper,
    table_offset: usize,
    entries: usize,
    stride: usize,
) -> ExpandedLevelSettingsLayout {
    ExpandedLevelSettingsLayout {
        mapper,
        table_offset,
        entries,
        stride,
    }
}

fn require_distinct(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        Err("input and output paths must differ".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{SnesChecksum, compute_snes_checksum};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn export_then_import_reopens_exact_record_and_checksum() {
        let directory = std::env::temp_dir().join(format!(
            "lm-expanded-settings-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let input = directory.join("input.smc");
        let record_file = directory.join("record.bin");
        let changed_file = directory.join("changed.bin");
        let output = directory.join("output.smc");
        fs::write(&input, vec![0xff; 0x8000]).unwrap();
        export(
            &input,
            3,
            layout(Mapper::LoRom, 0x100, 0x200, 0x20),
            &record_file,
        )
        .unwrap();
        assert_eq!(fs::read(&record_file).unwrap(), vec![0xff; 0x20]);
        fs::write(&changed_file, [0x5a; 0x20]).unwrap();
        import(
            &input,
            &output,
            3,
            layout(Mapper::LoRom, 0x100, 0x200, 0x20),
            &changed_file,
            0x7fdc,
        )
        .unwrap();
        let bytes = fs::read(&output).unwrap();
        assert_eq!(&bytes[0x160..0x180], &[0x5a; 0x20]);
        assert_eq!(
            SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
            compute_snes_checksum(&bytes, 0x7fdc).unwrap()
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
