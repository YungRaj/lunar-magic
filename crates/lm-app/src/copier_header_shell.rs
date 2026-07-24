use crate::{editor_shell::read_bounded_utf8, file_persistence, read_bounded_bytes, spec_text};
use lm_rom::{COPIER_HEADER_LEN, CopierHeader, RomImage};
use std::path::{Path, PathBuf};

const MAX_ROM_BYTES: usize = 32 * 1024 * 1024;

struct ConversionSpec {
    input: PathBuf,
    output: PathBuf,
    fill: u8,
}

pub(crate) fn add(spec_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let spec = parse_spec(spec_path, true)?;
    convert(&spec, CopierHeader::Present)
}

pub(crate) fn remove(spec_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let spec = parse_spec(spec_path, false)?;
    convert(&spec, CopierHeader::Absent)
}

fn convert(spec: &ConversionSpec, target: CopierHeader) -> Result<(), Box<dyn std::error::Error>> {
    if spec.input == spec.output {
        return Err("copier-header output must differ from its input".into());
    }
    let mut image = RomImage::from_bytes(read_bounded_bytes(&spec.input, MAX_ROM_BYTES, "ROM")?)?;
    if image.copier_header() == target {
        return Err("ROM already has the requested copier-header state".into());
    }
    if target == CopierHeader::Present
        && image
            .as_file_bytes()
            .len()
            .checked_add(COPIER_HEADER_LEN)
            .is_none_or(|length| length > MAX_ROM_BYTES)
    {
        return Err("headered ROM would exceed the bounded ROM file limit".into());
    }
    image.set_copier_header(target, spec.fill);
    file_persistence::write_new(&spec.output, image.as_file_bytes())?;
    println!(
        "copier header converted to {target:?}: {}",
        spec.output.display()
    );
    Ok(())
}

fn parse_spec(path: &Path, adding: bool) -> Result<ConversionSpec, Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        spec_text::MAX_SPEC_BYTES,
        "copier-header specification",
    )?;
    let mut fields = spec_text::parse_fields(&text, if adding { "LMHDRAD1" } else { "LMHDRRM1" })?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let input = spec_text::take_path(&mut fields, "input", base)?;
    let output = spec_text::take_path(&mut fields, "output", base)?;
    let fill = if adding {
        u8::try_from(spec_text::take_usize(&mut fields, "fill")?)
            .map_err(|_| "copier-header fill must be in 0..=255")?
    } else {
        0
    };
    spec_text::reject_unknown(&fields)?;
    Ok(ConversionSpec {
        input,
        output,
        fill,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn add_and_remove_specs_preserve_logical_rom() {
        let directory =
            std::env::temp_dir().join(format!("lm-app-copier-header-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let logical = vec![0x42; 0x8000];
        fs::write(directory.join("plain rom.smc"), &logical).unwrap();
        let add_spec = directory.join("add.txt");
        fs::write(
            &add_spec,
            "LMHDRAD1\ninput plain rom.smc\noutput headered rom.smc\nfill 165\n",
        )
        .unwrap();
        add(&add_spec).unwrap();
        let remove_spec = directory.join("remove.txt");
        fs::write(
            &remove_spec,
            "LMHDRRM1\ninput headered rom.smc\noutput restored rom.smc\n",
        )
        .unwrap();
        remove(&remove_spec).unwrap();
        assert_eq!(
            fs::read(directory.join("restored rom.smc")).unwrap(),
            logical
        );
        assert!(remove(&remove_spec).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
