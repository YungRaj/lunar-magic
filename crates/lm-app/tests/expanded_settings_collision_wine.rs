use lm_level::MwlFile;
use lm_project::Project;
use lm_rom::{CopierHeader, RomImage, detect_identity};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn wine_path(path: &Path) -> String {
    let rendered = path.display().to_string().replace('/', r"\");
    format!(r"Z:\{}", rendered.trim_start_matches('\\'))
}

#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and retained installed-ROM fixtures"]
fn lunar_magic_and_rust_match_relocated_expanded_settings_owned_bytes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let collision = root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc");
    let mwl = root.join(
        "oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/Level 000 layer3-settings.mwl",
    );
    assert!(lunar_magic.is_file(), "missing {}", lunar_magic.display());
    assert!(collision.is_file(), "missing {}", collision.display());
    assert!(mwl.is_file(), "missing {}", mwl.display());
    let physical = RomImage::from_bytes(fs::read(&collision).unwrap()).unwrap();
    let retained_header = physical.copier_header_bytes().unwrap().to_vec();
    let logical = physical.logical_bytes().to_vec();
    let mwl_record = MwlFile::decode(&fs::read(&mwl).unwrap())
        .unwrap()
        .expanded_settings_section()
        .unwrap();
    let directory = std::env::temp_dir().join(format!(
        "lm-expanded-settings-collision-wine-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();

    for copier_header in [CopierHeader::Absent, CopierHeader::Present] {
        let mut source = RomImage::from_bytes(logical.clone()).unwrap();
        if copier_header == CopierHeader::Present {
            source
                .replace_copier_header_exact(None, Some(&retained_header))
                .unwrap();
        }
        let original = source.as_file_bytes().to_vec();
        let state = if copier_header == CopierHeader::Present {
            "headered"
        } else {
            "headerless"
        };
        let lunar_magic_rom = directory.join(format!("Lunar Magic {state}.smc"));
        fs::write(&lunar_magic_rom, &original).unwrap();
        let output = Command::new("wine")
            .env("WINEDEBUG", "-all")
            .arg(&lunar_magic)
            .arg("-ImportLevel")
            .arg(wine_path(&lunar_magic_rom))
            .arg(wine_path(&mwl))
            .arg("000")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Lunar Magic {state} import stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let lunar_magic_result = RomImage::from_bytes(fs::read(&lunar_magic_rom).unwrap()).unwrap();

        let mut rust = Project::new(source);
        let plan = lm_profile::smw_us_v1_expanded_settings_installation_plan().unwrap();
        let result = rust.install_relocatable_patch(&plan).unwrap();
        assert_eq!(result.blocks[0].header_offset, 0x09_0000);
        let layout = lm_profile::smw_us_v1_installed_expanded_settings_layout(&rust)
            .unwrap()
            .unwrap();
        rust.save_expanded_level_settings(0, &mwl_record, layout, 0x7fdc)
            .unwrap();
        assert!(detect_identity(&rust.rom).unwrap().checksum_matches());
        assert_eq!(rust.rom.copier_header(), copier_header);
        assert_eq!(
            rust.rom.copier_header_bytes(),
            (copier_header == CopierHeader::Present).then_some(retained_header.as_slice())
        );

        assert_eq!(
            rust.rom.read(0x09_0000, 0x6e08).unwrap(),
            lunar_magic_result.read(0x09_0000, 0x6e08).unwrap()
        );
        for write in &plan.writes {
            assert_eq!(
                rust.rom
                    .read(write.offset, write.replacement.len())
                    .unwrap(),
                lunar_magic_result
                    .read(write.offset, write.replacement.len())
                    .unwrap(),
                "owned runtime write differs at {:x}",
                write.offset
            );
        }
        assert_eq!(
            rust.rom
                .read(
                    lm_profile::SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START,
                    0x8008,
                )
                .unwrap(),
            &logical[lm_profile::SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START..0x09_0000]
        );
        let installed = rust.save_snapshot();
        assert!(rust.undo().unwrap());
        assert!(rust.undo().unwrap());
        assert_eq!(rust.rom.as_file_bytes(), original);
        assert!(rust.redo().unwrap());
        assert!(rust.redo().unwrap());
        assert_eq!(rust.rom.as_file_bytes(), installed);
    }

    fs::remove_dir_all(directory).unwrap();
}
