use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_level::MwlFile;
use lm_project::{
    MAX_MWL_OPTIONAL_ASSETS_EDIT_SCRIPT_LEN, MwlOptionalLevelAssets,
    apply_mwl_optional_assets_edit, parse_mwl_optional_assets_edit_script,
};
use std::path::Path;

pub fn execute(
    input: &Path,
    size_modes: &Path,
    maximum_records: usize,
    edits: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if [input, size_modes, edits].contains(&output) {
        return Err("MWL optional-assets edit output must differ from every input".into());
    }
    let modes = crate::size_mode_file::read(size_modes)?;
    let mut file = MwlFile::decode(&read_bounded(input, MwlFile::MAX_FILE_BYTES)?)?;
    let script_bytes = read_bounded(edits, MAX_MWL_OPTIONAL_ASSETS_EDIT_SCRIPT_LEN)?;
    let script = std::str::from_utf8(&script_bytes)?;
    let commands = parse_mwl_optional_assets_edit_script(script)?;
    let mut assets = MwlOptionalLevelAssets::decode(&file, maximum_records, &modes)?;
    for command in &commands {
        apply_mwl_optional_assets_edit(&mut assets, &modes, command)?;
    }
    assets.install_into(&mut file, &modes)?;
    let encoded = file.encode()?;
    let reopened = MwlFile::decode(&encoded)?;
    if MwlOptionalLevelAssets::decode(&reopened, maximum_records, &modes)? != assets {
        return Err("edited MWL optional assets failed semantic reopen verification".into());
    }
    write_new(output, encoded)?;
    println!("edited-mwl-optional-assets: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, CompactExAnimation, ExAnimationRecord, Palette};
    use lm_level::MwlSectionKind;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-cli-mwl-optional-edit-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        path
    }

    fn assets() -> MwlOptionalLevelAssets {
        MwlOptionalLevelAssets {
            palette_metadata: [1, 2],
            palette: Palette {
                colors: (0_u16..257).map(Bgr555).collect(),
            },
            exanimation_metadata: [3, 4],
            exanimation: Some(CompactExAnimation {
                setting: 5,
                header_value: 6,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: vec![
                    ExAnimationRecord::new(1, 0, 0, 0x100, false, &[0, 6], false).unwrap(),
                ],
            }),
        }
    }

    #[test]
    fn edits_semantics_preserves_unrelated_sections_and_publishes_last() {
        let directory = directory();
        let input = directory.join("input.mwl");
        let modes = directory.join("modes.bin");
        let edits = directory.join("edits.txt");
        let output = directory.join("output.mwl");
        let mut file = MwlFile::default();
        assets().install_into(&mut file, &[false; 256]).unwrap();
        file.set_section(MwlSectionKind::Layer1, vec![1, 2, 3]);
        fs::write(&input, file.encode().unwrap()).unwrap();
        fs::write(&modes, [0; 256]).unwrap();
        fs::write(
            &edits,
            "LMMWLOE1\npalette-color 256 1234\nexanimation-globals 09 0000000A\nframe-replace 0 0 1234\n",
        )
        .unwrap();

        execute(&input, &modes, 32, &edits, &output).unwrap();

        let result = MwlFile::decode(&fs::read(&output).unwrap()).unwrap();
        let edited = MwlOptionalLevelAssets::decode(&result, 32, &[false; 256]).unwrap();
        assert_eq!(result.section(MwlSectionKind::Layer1), &[1, 2, 3]);
        assert_eq!(edited.palette.colors[256], Bgr555(0x1234));
        let animation = edited.exanimation.unwrap();
        assert_eq!(animation.setting, 9);
        assert_eq!(animation.records[0].frame_bytes(false), [0x34, 0x12]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn late_failure_and_alias_do_not_publish() {
        let directory = directory();
        let input = directory.join("bad-input.mwl");
        let modes = directory.join("bad-modes.bin");
        let edits = directory.join("bad-edits.txt");
        let output = directory.join("missing-output.mwl");
        let mut file = MwlFile::default();
        assets().install_into(&mut file, &[false; 256]).unwrap();
        fs::write(&input, file.encode().unwrap()).unwrap();
        fs::write(&modes, [0; 256]).unwrap();
        fs::write(&edits, "LMMWLOE1\npalette-color 0 1234\nrecord-remove 99\n").unwrap();
        assert!(execute(&input, &modes, 32, &edits, &output).is_err());
        assert!(!output.exists());
        assert!(execute(&input, &modes, 32, &edits, &input).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
