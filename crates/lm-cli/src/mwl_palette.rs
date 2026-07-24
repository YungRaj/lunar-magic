use crate::atomic_output::write_new;
use crate::oracle_input::read_bounded;
use lm_graphics::{Bgr555, Palette, TplPaletteFile};
use lm_level::MwlFile;
use std::path::Path;

pub fn export_tpl(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        return Err("MWL palette TPL output must differ from input".into());
    }
    let file = MwlFile::decode(&read_bounded(input, MwlFile::MAX_FILE_BYTES)?)?;
    let section = file.palette_section()?;
    let palette = Palette {
        colors: section.tpl_order_colors().into_iter().map(Bgr555).collect(),
    };
    write_new(output, TplPaletteFile { palette }.encode()?)?;
    println!("mwl-backdrop: 0x{:04x}", section.backdrop);
    println!(
        "source-snes-address: 0x{:06x}",
        section.metadata[1] & 0x00ff_ffff
    );
    println!("palette-tpl: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{MwlPaletteSection, MwlSectionKind};
    use std::fs;

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("lm-cli-mwl-palette-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn exports_natural_tpl_order_without_the_backdrop_word() {
        let directory = directory();
        let input = directory.join("level.mwl");
        let output = directory.join("palette.tpl");
        let colors = std::array::from_fn(|index| u16::try_from(index).unwrap());
        let mut file = MwlFile::default();
        file.set_palette_section(&MwlPaletteSection::from_tpl_order(
            [0, 0x10_8031],
            0x1234,
            colors,
        ));
        assert_eq!(
            file.section(MwlSectionKind::Palette).len(),
            MwlPaletteSection::ENCODED_LEN
        );
        fs::write(&input, file.encode().unwrap()).unwrap();

        export_tpl(&input, &output).unwrap();

        let exported = TplPaletteFile::decode(&fs::read(&output).unwrap()).unwrap();
        assert_eq!(
            exported
                .palette
                .colors
                .iter()
                .map(|color| color.0)
                .collect::<Vec<_>>(),
            colors
        );
        assert!(export_tpl(&input, &input).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
