use crate::atomic_output::write_new;
use crate::command_types::Command;
use crate::oracle_input::read_bounded;
use lm_level::{Lm16Map16File, Lm16Map16SectionKind};

pub fn execute_command(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    let Command::Lm16Map16File {
        input,
        normalized_output,
    } = command
    else {
        return Ok(false);
    };
    if normalized_output
        .as_ref()
        .is_some_and(|output| output == input)
    {
        return Err("LM16 Map16 input and normalized output paths must differ".into());
    }
    let file = Lm16Map16File::decode(&read_bounded(input, Lm16Map16File::MAX_FILE_LEN)?)?;
    println!("format-version: {:#010x}", file.format_version);
    println!("lunar-magic-version: {:#010x}", file.lunar_magic_version);
    println!("flags: {:#010x}", file.flags);
    for kind in [
        Lm16Map16SectionKind::CombinedTiles,
        Lm16Map16SectionKind::ActsLike,
        Lm16Map16SectionKind::ForegroundTiles,
        Lm16Map16SectionKind::BackgroundTiles,
        Lm16Map16SectionKind::ExtendedTiles,
        Lm16Map16SectionKind::AuxiliaryTiles,
        Lm16Map16SectionKind::SelectionState,
        Lm16Map16SectionKind::EditorState,
    ] {
        println!("section-{kind:?}: {:#x}", file.section(kind).len());
    }
    if let Some(output) = normalized_output {
        write_new(output, file.encode())?;
        println!("normalized-lm16-map16: {}", output.display());
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm16-map16-cli-{}-{}-{name}",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn container() -> Vec<u8> {
        let mut bytes = vec![0; Lm16Map16File::DATA_OFFSET + 1];
        bytes[..4].copy_from_slice(&Lm16Map16File::MAGIC);
        bytes[4..8].copy_from_slice(&0x0001_0100_u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&0x0001_0363_u32.to_le_bytes());
        bytes[0x10..0x14].copy_from_slice(
            &u32::try_from(Lm16Map16File::DIRECTORY_OFFSET)
                .unwrap()
                .to_le_bytes(),
        );
        bytes[0x14..0x18].copy_from_slice(
            &u32::try_from(Lm16Map16File::DIRECTORY_LEN)
                .unwrap()
                .to_le_bytes(),
        );
        bytes[0x28..0x2c].copy_from_slice(&2_u32.to_le_bytes());
        bytes[0x70..0x74].copy_from_slice(
            &u32::try_from(Lm16Map16File::DATA_OFFSET)
                .unwrap()
                .to_le_bytes(),
        );
        bytes[0x74..0x78].copy_from_slice(&1_u32.to_le_bytes());
        bytes
    }

    #[test]
    fn normalizes_losslessly_and_uses_create_new_output() {
        let input = path("input.map16");
        let output = path("output.map16");
        let bytes = container();
        fs::write(&input, &bytes).unwrap();
        let command = Command::Lm16Map16File {
            input: input.clone(),
            normalized_output: Some(output.clone()),
        };
        assert!(execute_command(&command).unwrap());
        assert_eq!(fs::read(&output).unwrap(), bytes);
        assert!(execute_command(&command).is_err());
        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
    }
}
