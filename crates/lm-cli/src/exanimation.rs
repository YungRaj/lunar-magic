use crate::atomic_output::write_new;
use lm_project::{ExAnimationRomLayout, LevelPointerTable, Project};
use lm_rom::{Mapper, RomImage};

#[derive(Clone, Copy)]
pub struct InspectOptions<'a> {
    pub maximum_records: usize,
    pub maximum_encoded_len: usize,
    pub size_mode_bytes: &'a [u8],
    pub observation: Option<&'a std::path::Path>,
}

pub fn inspect(
    bytes: Vec<u8>,
    mapper: Mapper,
    slot: usize,
    pointer_table: usize,
    options: InspectOptions<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    if options.size_mode_bytes.len() != 256 {
        return Err(format!(
            "ExAnimation size-mode table must contain 256 bytes, got {}",
            options.size_mode_bytes.len()
        )
        .into());
    }
    let size_modes: Vec<_> = options
        .size_mode_bytes
        .iter()
        .map(|value| *value != 0)
        .collect();
    let project = Project::new(RomImage::from_bytes(bytes)?);
    let animation = project.load_exanimation(
        slot,
        ExAnimationRomLayout {
            mapper,
            pointers: LevelPointerTable {
                offset: pointer_table,
                entries: 0x200,
                stride: 3,
            },
            maximum_records: options.maximum_records,
            maximum_encoded_len: options.maximum_encoded_len,
        },
        &size_modes,
    )?;
    println!("slot: {slot:#05x}");
    println!("setting: {:#04x}", animation.setting);
    println!("header-value: {:#010x}", animation.header_value);
    println!("trigger-mask: {:#06x}", animation.trigger_mask);
    println!("records: {}", animation.records.len());
    println!(
        "active-records: {}",
        animation
            .records
            .iter()
            .filter(|record| record.kind() != 0)
            .count()
    );
    if let Some(path) = options.observation {
        write_new(
            path,
            lm_oracle::observe_compact_exanimation(&animation).to_text(),
        )?;
        println!("observation: {}", path.display());
    }
    Ok(())
}
