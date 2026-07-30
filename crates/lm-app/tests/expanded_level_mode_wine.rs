use lm_level::{MwlFile, MwlLevelHeaderSection, MwlSectionKind};
use lm_project::Project;
use lm_rom::RomImage;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn wine_path(path: &Path) -> String {
    let rendered = path.display().to_string().replace('/', r"\");
    format!(r"Z:\{}", rendered.trim_start_matches('\\'))
}

#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and retained installed-ROM fixture"]
fn lunar_magic_exports_rust_persisted_expanded_level_mode() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let installed = root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc");
    let directory = std::env::temp_dir().join(format!(
        "lm-expanded-mode-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let edited_rom = directory.join("Rust expanded mode.smc");
    let exported_mwl = directory.join("Level 105.mwl");

    let mut project = Project::new(RomImage::from_bytes(fs::read(installed).unwrap()).unwrap());
    project
        .save_expanded_level_mode(
            0x105,
            3,
            lm_profile::smw_us_v1_expanded_level_mode_locator(),
            0x7fdc,
        )
        .unwrap();
    fs::write(&edited_rom, project.save_snapshot()).unwrap();

    let output = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&lunar_magic)
        .arg("-ExportLevel")
        .arg(wine_path(&edited_rom))
        .arg(wine_path(&exported_mwl))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Lunar Magic export stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let exported = MwlFile::decode(&fs::read(exported_mwl).unwrap()).unwrap();
    let header =
        MwlLevelHeaderSection::decode(exported.section(MwlSectionKind::LevelHeader)).unwrap();
    assert_eq!(header.0[16] & 0x7f, 3);
    fs::remove_dir_all(directory).unwrap();
}
