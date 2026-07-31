use lm_graphics::{ExAnimationFeature, ExAnimationFeatureOptions};
use lm_level::{MwlFile, MwlSectionKind};
use lm_project::{
    ChainedSnesPointerLocator, GatedLayout, InstallationMarker,
    InstalledExAnimationFeatureRomLayout, InstalledLayout, MwlExAnimationSection, Project,
};
use lm_rom::{Mapper, RomImage};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn wine_path(path: &Path) -> String {
    let rendered = path.display().to_string().replace('/', r"\");
    format!(r"Z:\{}", rendered.trim_start_matches('\\'))
}

fn installed_feature_layout() -> InstalledLayout<InstalledExAnimationFeatureRomLayout> {
    // Lunar Magic 3.63 installed this expanded-ExAnimation hook and runtime in the retained
    // pristine-US fixture. Offsets are logical (the source file has a 0x200-byte copier header).
    InstalledLayout::Alternatives {
        primary: GatedLayout {
            marker: InstallationMarker {
                offset: 0x28_3ad,
                expected: 0x22,
            },
            layout: InstalledExAnimationFeatureRomLayout {
                table_locator: ChainedSnesPointerLocator {
                    mapper: Mapper::LoRom,
                    first_operand_offset: 0x28_3ae,
                    final_operand_displacement: 0x46,
                },
            },
        },
        fallback: None,
    }
}

#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and retained installed-ROM fixture"]
fn lunar_magic_exports_rust_persisted_animation_feature_options() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = root.join("lm363/Lunar Magic.exe");
    let installed =
        root.join("oracle-work/lm363/pristine-us/exanimation-install-positive/after.smc");
    assert!(executable.is_file(), "missing {}", executable.display());
    assert!(installed.is_file(), "missing {}", installed.display());

    let mut project = Project::new(RomImage::from_bytes(fs::read(&installed).unwrap()).unwrap());
    let loaded = project
        .load_installed_exanimation_features(0, installed_feature_layout())
        .unwrap();
    assert_eq!(loaded.options.encode(), 0);

    let mut edited = ExAnimationFeatureOptions::decode(0x0b);
    edited.set_enabled(ExAnimationFeature::PaletteAnimation, true);
    edited.set_enabled(ExAnimationFeature::VanillaAnimation, false);
    edited.set_enabled(ExAnimationFeature::GlobalExAnimation, true);
    edited.set_enabled(ExAnimationFeature::LevelExAnimation, false);
    assert_eq!(edited.encode(), 0x5b);
    project
        .save_installed_exanimation_features(0, edited, installed_feature_layout(), 0x7fdc)
        .unwrap();

    let directory = std::env::temp_dir().join(format!(
        "lm-exanimation-features-wine-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let rom = directory.join("Rust animation features.smc");
    let mwl = directory.join("Lunar Magic exported level 000.mwl");
    fs::write(&rom, project.save_snapshot()).unwrap();

    let output = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&executable)
        .arg("-ExportLevel")
        .arg(wine_path(&rom))
        .arg(wine_path(&mwl))
        .arg("000")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Lunar Magic export stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let exported = MwlFile::decode(&fs::read(&mwl).unwrap()).unwrap();
    let section = MwlExAnimationSection::decode(
        exported.section(MwlSectionKind::ExAnimation),
        32,
        &[false; 256],
    )
    .unwrap();
    assert_eq!(section.metadata[0].to_le_bytes()[0], 0x5b);

    fs::remove_dir_all(directory).unwrap();
}
