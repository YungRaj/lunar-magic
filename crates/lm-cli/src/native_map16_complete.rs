use crate::atomic_output::write_new;
use crate::oracle_input::{read_bounded, read_rom};
use lm_level::{Lm16Map16File, Lm16Map16SectionKind};
use lm_profile::{
    SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES, SmwUsV1CompleteMap16SaveOptions,
    load_smw_us_v1_complete_map16, save_smw_us_v1_complete_map16,
};
use lm_project::Project;
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

const CHECKSUM_FIELD: usize = 0x7fdc;
const MINIMUM_EXPANDED_SIZE: usize = 0x10_0000;

pub(crate) fn export(
    rom: &Path,
    template: Option<&Path>,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(rom, output, "Map16 export output")?;
    if template.is_some_and(|path| path == output) {
        return Err("Map16 template and output paths must differ".into());
    }
    let project = open_smw_us_v1(rom)?;
    let loaded = load_smw_us_v1_complete_map16(&project)?;
    let foreground = export_foreground(&loaded.foreground.definitions);
    let combined = encode_combined(&foreground, &loaded.background.definitions);
    let acts_like = encode_words(&loaded.foreground.acts_like);
    let file = if let Some(template) = template {
        let mut file =
            Lm16Map16File::decode(&read_bounded(template, Lm16Map16File::MAX_FILE_LEN)?)?;
        file.replace_complete_core(&combined, &acts_like)?;
        file
    } else {
        Lm16Map16File::from_complete_core(&combined, &acts_like)?
    };
    write_new(output, file.encode())?;
    println!("exported-complete-map16: {}", output.display());
    println!(
        "foreground-tiles: {:#x}",
        loaded.foreground.definitions.len() / 4
    );
    println!(
        "background-tiles: {:#x}",
        loaded.background.definitions.len() / 4
    );
    println!("acts-like-words: {:#x}", loaded.foreground.acts_like.len());
    Ok(())
}

pub(crate) fn import(
    input_rom: &Path,
    map16: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(input_rom, output_rom, "Map16 import output ROM")?;
    if map16 == output_rom {
        return Err("Map16 input and output ROM paths must differ".into());
    }
    let file = Lm16Map16File::decode(&read_bounded(map16, Lm16Map16File::MAX_FILE_LEN)?)?;
    let combined = file.section(Lm16Map16SectionKind::CombinedTiles);
    let acts_bytes = file.section(Lm16Map16SectionKind::ActsLike);
    if combined.len() != Lm16Map16File::COMBINED_TILES_LEN
        || acts_bytes.len() != Lm16Map16File::ACTS_LIKE_LEN
    {
        return Err(format!(
            "complete Map16 core has lengths {:#x}/{:#x}, expected {:#x}/{:#x}",
            combined.len(),
            acts_bytes.len(),
            Lm16Map16File::COMBINED_TILES_LEN,
            Lm16Map16File::ACTS_LIKE_LEN
        )
        .into());
    }
    let foreground_bytes = &combined[..Lm16Map16File::FOREGROUND_TILES_LEN];
    let background_bytes = &combined[Lm16Map16File::FOREGROUND_TILES_LEN..];
    let mut foreground = decode_words(foreground_bytes);
    let background = decode_words(background_bytes);
    let acts_like = decode_words(acts_bytes);

    let mut project = open_smw_us_v1(input_rom)?;
    let original = load_smw_us_v1_complete_map16(&project)?;
    let protected_words = SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES / 2;
    foreground[..protected_words]
        .copy_from_slice(&original.foreground.definitions[..protected_words]);

    let identity = project
        .identity
        .as_ref()
        .ok_or("opened project has no detected identity")?;
    let allocation_end = project.rom.logical_len().max(MINIMUM_EXPANDED_SIZE);
    let options = SmwUsV1CompleteMap16SaveOptions {
        allocation: AllocationPolicy {
            search: 0x80_000..allocation_end,
            bank_size: Some(0x8000),
            fill_bytes: vec![0, 0xff],
            protected: vec![ProtectedRange(
                identity.internal_header_offset..identity.internal_header_offset + 0x40,
            )],
        },
        reuse_identical: true,
        erase_fill: 0xff,
    };
    let saved = save_smw_us_v1_complete_map16(
        &mut project,
        &foreground,
        &background,
        &acts_like,
        CHECKSUM_FIELD,
        &options,
    )?;
    let reopened = load_smw_us_v1_complete_map16(&project)?;
    if let Some((domain, index, expected, actual)) = first_reopen_mismatch(
        &foreground,
        &background,
        &acts_like,
        &reopened.foreground.definitions,
        &reopened.background.definitions,
        &reopened.foreground.acts_like,
    ) {
        return Err(format!(
            "complete Map16 import failed semantic reopen verification: \
             {domain} word {index:#x} expected {expected:#06x}, reopened {actual:#06x}"
        )
        .into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!("imported-complete-map16: {}", output_rom.display());
    println!("installed-runtime: {}", saved.installed_runtime);
    println!("changed: {}", saved.changed);
    println!(
        "protected-foreground-prefix-bytes: \
         {SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES:#x}"
    );
    Ok(())
}

fn open_smw_us_v1(path: &Path) -> Result<Project, Box<dyn std::error::Error>> {
    let project = Project::open_supported(RomImage::from_bytes(read_rom(path)?)?)?;
    let identity = project
        .identity
        .as_ref()
        .ok_or("opened project has no detected identity")?;
    if identity.game != SupportedGame::SuperMarioWorld
        || identity.region != Region::NorthAmerica
        || identity.revision != 0
        || identity.mapper != Mapper::LoRom
    {
        return Err("complete Map16 operation requires SMW US revision 0 LoROM".into());
    }
    Ok(project)
}

fn encode_combined(foreground: &[u16], background: &[u16]) -> Vec<u8> {
    let mut output = encode_words(foreground);
    output.extend_from_slice(&encode_words(background));
    output
}

fn export_foreground(definitions: &[u16]) -> Vec<u16> {
    let mut output = definitions.to_vec();
    output[..SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES / 2].fill(0);
    output
}

fn encode_words(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn decode_words(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks_exact(2)
        .map(|word| u16::from_le_bytes([word[0], word[1]]))
        .collect()
}

fn first_reopen_mismatch(
    expected_foreground: &[u16],
    expected_background: &[u16],
    expected_acts_like: &[u16],
    actual_foreground: &[u16],
    actual_background: &[u16],
    actual_acts_like: &[u16],
) -> Option<(&'static str, usize, u16, u16)> {
    for (domain, expected, actual) in [
        ("foreground", expected_foreground, actual_foreground),
        ("background", expected_background, actual_background),
        ("acts-like", expected_acts_like, actual_acts_like),
    ] {
        if let Some((index, (&expected, &actual))) = expected
            .iter()
            .zip(actual)
            .enumerate()
            .find(|(_, (expected, actual))| expected != actual)
        {
            return Some((domain, index, expected, actual));
        }
    }
    None
}

fn require_distinct(
    input: &Path,
    output: &Path,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        Err(format!("{label} must differ from its input").into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_word_encoding_preserves_foreground_background_boundary() {
        let foreground = vec![0x1234; Lm16Map16File::FOREGROUND_TILES_LEN / 2];
        let background = vec![0xabcd; Lm16Map16File::BACKGROUND_TILES_LEN / 2];
        let bytes = encode_combined(&foreground, &background);
        assert_eq!(bytes.len(), Lm16Map16File::COMBINED_TILES_LEN);
        assert_eq!(
            decode_words(&bytes[..Lm16Map16File::FOREGROUND_TILES_LEN]),
            foreground
        );
        assert_eq!(
            decode_words(&bytes[Lm16Map16File::FOREGROUND_TILES_LEN..]),
            background
        );
    }

    #[test]
    fn complete_export_zeroes_lunar_magics_protected_definition_prefix() {
        let definitions = vec![0x1c75; Lm16Map16File::FOREGROUND_TILES_LEN / 2];
        let exported = export_foreground(&definitions);
        let protected_words = SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES / 2;
        assert_eq!(&exported[..protected_words], vec![0; protected_words]);
        assert_eq!(
            &exported[protected_words..],
            &definitions[protected_words..]
        );
    }

    #[test]
    fn aliases_are_rejected_before_file_access() {
        let same = Path::new("same");
        assert!(export(same, None, same).is_err());
        assert!(import(same, Path::new("input.map16"), same).is_err());
    }
}
