use lm_level::{ExpandedLevelHeader, MwlFile, SuperGraphicsBypass};
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
#[ignore = "requires Wine plus local Lunar Magic 3.63 and retained installed-ROM fixture"]
fn lunar_magic_exports_all_rust_super_gfx_bypass_fields_in_both_states() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let installed =
        root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc");
    assert!(lunar_magic.is_file(), "missing {}", lunar_magic.display());
    assert!(installed.is_file(), "missing {}", installed.display());
    let directory = std::env::temp_dir().join(format!(
        "lm-super-gfx-bypass-wine-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let expected_files = SuperGraphicsBypass {
        enabled: true,
        foreground_background: [0x012, 0x023, 0x030, 0x031, 0x032, 0x033],
        sprites: [0x000, 0x001, 0x017, 0x02f],
    };
    let installed = RomImage::from_bytes(fs::read(&installed).unwrap()).unwrap();
    let retained_copier_header = installed.copier_header_bytes().unwrap().to_vec();
    let logical_fixture = installed.logical_bytes().to_vec();
    let mut logical_results = [Vec::new(), Vec::new()];

    for (state_index, enabled) in [false, true].into_iter().enumerate() {
        for copier_header in [CopierHeader::Absent, CopierHeader::Present] {
            let expected = SuperGraphicsBypass {
                enabled,
                ..expected_files
            };
            let mut image = RomImage::from_bytes(logical_fixture.clone()).unwrap();
            if copier_header == CopierHeader::Present {
                image
                    .replace_copier_header_exact(None, Some(&retained_copier_header))
                    .unwrap();
            }
            let original_copier_header = image.copier_header_bytes().map(<[u8]>::to_vec);
            let mut project = Project::new(image);
            let layout = lm_profile::smw_us_v1_expanded_settings_layout();
            assert!(
                lm_profile::load_smw_us_v1_expanded_level_settings(&project, 0)
                    .unwrap()
                    .installed
            );
            let record = project.load_expanded_level_settings(0, layout).unwrap();
            assert!(
                !ExpandedLevelHeader::from(&record)
                    .super_graphics_bypass()
                    .enabled
            );
            let mut header = ExpandedLevelHeader::from(&record);
            header.set_super_graphics_bypass(expected).unwrap();
            let before = project.save_snapshot();
            project
                .save_expanded_level_settings(0, &header.into(), layout, 0x7fdc)
                .unwrap();
            assert!(detect_identity(&project.rom).unwrap().checksum_matches());
            assert_eq!(project.rom.copier_header(), copier_header);
            assert_eq!(
                project.rom.copier_header_bytes(),
                original_copier_header.as_deref()
            );
            let after = project.save_snapshot();
            assert!(project.undo().unwrap());
            assert_eq!(project.rom.as_file_bytes(), before);
            assert!(project.redo().unwrap());
            assert_eq!(project.rom.as_file_bytes(), after);

            let state = if enabled { "enabled" } else { "disabled" };
            let header_state = match copier_header {
                CopierHeader::Absent => "headerless",
                CopierHeader::Present => "headered",
            };
            let rom = directory.join(format!("Rust Super GFX {state} {header_state}.smc"));
            let mwl = directory.join(format!("Lunar Magic Super GFX {state} {header_state}.mwl"));
            let snapshot = after;
            let reopened = Project::new(RomImage::from_bytes(snapshot.clone()).unwrap());
            let reopened = reopened.load_expanded_level_settings(0, layout).unwrap();
            assert_eq!(
                ExpandedLevelHeader::from(&reopened).super_graphics_bypass(),
                expected
            );
            let logical = RomImage::from_bytes(snapshot.clone())
                .unwrap()
                .logical_bytes()
                .to_vec();
            if logical_results[state_index].is_empty() {
                logical_results[state_index] = logical;
            } else {
                assert_eq!(logical_results[state_index], logical);
            }
            fs::write(&rom, snapshot).unwrap();
            let output = Command::new("wine")
                .env("WINEDEBUG", "-all")
                .arg(&lunar_magic)
                .arg("-ExportLevel")
                .arg(wine_path(&rom))
                .arg(wine_path(&mwl))
                .arg("000")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "Lunar Magic {state} {header_state} export stdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );

            let exported = MwlFile::decode(&fs::read(&mwl).unwrap()).unwrap();
            let exported = ExpandedLevelHeader::from(exported.expanded_settings_section().unwrap())
                .super_graphics_bypass();
            assert_eq!(exported, expected);
        }
    }

    fs::remove_dir_all(directory).unwrap();
}
