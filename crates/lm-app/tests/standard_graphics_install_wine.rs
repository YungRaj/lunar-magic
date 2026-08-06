use lm_app::prepare_smw_us_v1_standard_graphics_install;
use lm_project::Project;
use lm_rom::RomImage;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn create(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "lm-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_export(wine: &Path, lunar_magic: &Path, directory: &Path, rom_name: &str) {
    let output = Command::new(wine)
        .arg(lunar_magic)
        .arg("-ExportGFX")
        .arg(rom_name)
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Lunar Magic export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires Wine, Lunar Magic 3.63, and the local legally obtained pristine SMW ROM"]
fn lunar_magic_reexports_every_rust_first_install_standard_gfx_file() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let wine = std::env::var_os("WINE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("wine"));
    let lunar_magic = std::env::var_os("LUNAR_MAGIC_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("lm363/Lunar Magic.exe"));
    let pristine = std::env::var_os("LM_PRISTINE_GFX_ROM")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("sysLMRestore/smwOrig.smc"));

    let baseline = TemporaryDirectory::create("gfx-baseline");
    let installed = TemporaryDirectory::create("gfx-installed");
    fs::copy(&pristine, baseline.0.join("baseline.smc")).unwrap();
    run_export(&wine, &lunar_magic, &baseline.0, "baseline.smc");
    let files = (0..0x34)
        .map(|number| {
            fs::read(
                baseline
                    .0
                    .join("Graphics")
                    .join(format!("GFX{number:02X}.bin")),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();

    let original = RomImage::from_bytes(fs::read(&pristine).unwrap()).unwrap();
    let commit = prepare_smw_us_v1_standard_graphics_install(0, original.clone(), &files).unwrap();
    let mut project = Project::new(original);
    project
        .apply_mutation(&commit.description, &commit.mutation)
        .unwrap();
    fs::write(
        installed.0.join("installed.smc"),
        project.rom.as_file_bytes(),
    )
    .unwrap();
    run_export(&wine, &lunar_magic, &installed.0, "installed.smc");

    for number in 0..0x34 {
        let name = format!("GFX{number:02X}.bin");
        let expected = fs::read(baseline.0.join("Graphics").join(&name)).unwrap();
        let actual = fs::read(installed.0.join("Graphics").join(&name)).unwrap();
        assert_eq!(actual, expected, "{name}");
    }
}
