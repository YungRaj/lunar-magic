use lm_app::MwlDocumentController;
use lm_level::CustomTimeSettings;
use lm_project::Project;
use lm_rom::{RomImage, detect_identity};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const PRISTINE_SMW_US_SHA256: &str =
    "0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b";

#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn lunar_magic_preserves_rust_installed_support_patch_b_and_custom_time() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = pristine_smw_us_rom_path(&root);
    let directory = std::env::temp_dir().join(format!(
        "lm-support-patch-b-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let feature_rom = directory.join("feature.sfc");
    let source_mwl = directory.join("source.mwl");
    let feature_mwl = directory.join("feature.mwl");
    let reexported_mwl = directory.join("reexported.mwl");
    fs::copy(&original_rom, &feature_rom).unwrap();
    let restore_directory = directory.join("sysLMRestore");
    fs::create_dir(&restore_directory).unwrap();
    fs::copy(&original_rom, restore_directory.join("smwOrig.smc")).unwrap();

    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &feature_rom,
        &source_mwl,
        "105",
    );
    let mut document =
        MwlDocumentController::decode(feature_mwl.clone(), &fs::read(&source_mwl).unwrap())
            .unwrap();
    let mut feature_layer1 = document.layer1().unwrap();
    feature_layer1
        .objects
        .set_custom_time(false, Some(CustomTimeSettings::new(0xabc, true).unwrap()))
        .unwrap();
    document.replace_layer1(0, &feature_layer1).unwrap();
    fs::write(&feature_mwl, document.begin_save().unwrap().bytes).unwrap();

    let mut project = Project::new(RomImage::from_bytes(fs::read(&feature_rom).unwrap()).unwrap());
    let plan = lm_profile::smw_us_v1_support_patch_b_installation_plan(project.rom.logical_bytes())
        .unwrap();
    project.install_relocatable_patch(&plan).unwrap();
    fs::write(&feature_rom, project.save_snapshot()).unwrap();
    run_lunar_magic_level_command(
        &lunar_magic,
        "-ImportLevel",
        &feature_rom,
        &feature_mwl,
        "105",
    );
    let feature = RomImage::from_bytes(fs::read(&feature_rom).unwrap()).unwrap();
    assert_eq!(
        lm_profile::detect_smw_us_v1_support_patch_b(feature.logical_bytes()).unwrap(),
        lm_profile::SmwUsV1SupportPatchBState::Installed
    );
    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &feature_rom,
        &reexported_mwl,
        "105",
    );
    let reexported =
        MwlDocumentController::decode(reexported_mwl.clone(), &fs::read(&reexported_mwl).unwrap())
            .unwrap();
    assert_eq!(
        reexported.layer1().unwrap().objects.custom_time(false),
        Some(CustomTimeSettings::new(0xabc, true).unwrap())
    );
    fs::remove_dir_all(directory).unwrap();
}

fn run_lunar_magic_level_command(
    lunar_magic: &Path,
    operation: &str,
    rom: &Path,
    level_file: &Path,
    level: &str,
) {
    let status = Command::new("wine")
        .arg(lunar_magic)
        .arg(operation)
        .arg(wine_path(rom))
        .arg(wine_path(level_file))
        .arg(level)
        .status()
        .unwrap();
    assert!(status.success());
}

fn pristine_smw_us_rom_path(root: &Path) -> PathBuf {
    for path in [
        root.join("test.sfc"),
        root.join("smw.sfc"),
        root.join("sysLMRestore/smwOrig.smc"),
    ] {
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(image) = RomImage::from_bytes(bytes) else {
            continue;
        };
        if detect_identity(&image).is_ok()
            && lm_oracle::sha256_hex(image.logical_bytes()) == PRISTINE_SMW_US_SHA256
        {
            return path;
        }
    }
    panic!("verified pristine SMW-US fixture not found");
}

fn wine_path(path: &Path) -> String {
    let rendered = path.display().to_string().replace('/', r"\");
    format!(r"Z:\{}", rendered.trim_start_matches('\\'))
}
