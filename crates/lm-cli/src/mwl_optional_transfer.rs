use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_level::MwlFile;
use lm_project::MwlOptionalLevelAssets;
use std::path::Path;

pub fn execute(
    source: &Path,
    target: &Path,
    size_modes: &Path,
    maximum_records: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if [source, target, size_modes].contains(&output) {
        return Err("MWL optional-assets output must differ from every input".into());
    }
    let modes = crate::size_mode_file::read(size_modes)?;
    let source = MwlFile::decode(&read_bounded(source, MwlFile::MAX_FILE_BYTES)?)?;
    let mut target = MwlFile::decode(&read_bounded(target, MwlFile::MAX_FILE_BYTES)?)?;
    let assets = MwlOptionalLevelAssets::decode(&source, maximum_records, &modes)?;
    assets.install_into(&mut target, &modes)?;
    let encoded = target.encode()?;
    let reopened = MwlFile::decode(&encoded)?;
    if MwlOptionalLevelAssets::decode(&reopened, maximum_records, &modes)? != assets {
        return Err("transferred MWL optional assets failed semantic reopen verification".into());
    }
    write_new(output, encoded)?;
    println!("transferred-mwl-optional-assets: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, CompactExAnimation, ExAnimationRecord, Palette};
    use lm_level::MwlSectionKind;
    use lm_project::MwlOptionalLevelAssets;
    use std::fs;

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("lm-cli-mwl-optional-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        path
    }

    fn assets() -> MwlOptionalLevelAssets {
        MwlOptionalLevelAssets {
            palette_metadata: [0, 0x10_8031],
            palette: Palette {
                colors: (0_u16..257).map(Bgr555).collect(),
            },
            exanimation_metadata: [0, 0x10_97e9],
            exanimation: Some(CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: vec![
                    ExAnimationRecord::new(1, 0, 0, 0x100, false, &[0, 6], false).unwrap(),
                ],
            }),
        }
    }

    #[test]
    fn transfers_only_typed_optional_sections_and_reopens() {
        let directory = directory();
        let source_path = directory.join("source.mwl");
        let target_path = directory.join("target.mwl");
        let modes_path = directory.join("modes.bin");
        let output_path = directory.join("output.mwl");
        let modes = [false; 256];
        let mut source = MwlFile::default();
        assets().install_into(&mut source, &modes).unwrap();
        let mut target = MwlFile::default();
        target.set_section(MwlSectionKind::Layer1, vec![1, 2, 3]);
        target.set_section(MwlSectionKind::Palette, vec![0; 8]);
        target.set_section(MwlSectionKind::ExAnimation, vec![0; 8]);
        fs::write(&source_path, source.encode().unwrap()).unwrap();
        fs::write(&target_path, target.encode().unwrap()).unwrap();
        fs::write(&modes_path, [0; 256]).unwrap();

        execute(&source_path, &target_path, &modes_path, 32, &output_path).unwrap();

        let output = MwlFile::decode(&fs::read(&output_path).unwrap()).unwrap();
        assert_eq!(output.section(MwlSectionKind::Layer1), &[1, 2, 3]);
        assert_eq!(
            MwlOptionalLevelAssets::decode(&output, 32, &modes).unwrap(),
            assets()
        );
        assert!(execute(&source_path, &target_path, &modes_path, 32, &source_path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
