use lm_app::prepare_smw_us_v1_standard_graphics_install;
use lm_project::{GraphicsCompression, GraphicsRomLayout, LevelPointerTable, Project};
use lm_rom::{CopierHeader, RomImage};
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

fn run_export_operation(
    wine: &Path,
    lunar_magic: &Path,
    directory: &Path,
    operation: &str,
    rom_name: &str,
) {
    let output = Command::new(wine)
        .arg(lunar_magic)
        .arg(operation)
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

fn run_export(wine: &Path, lunar_magic: &Path, directory: &Path, rom_name: &str) {
    run_export_operation(wine, lunar_magic, directory, "-ExportGFX", rom_name);
}

#[test]
#[ignore = "requires Wine, Lunar Magic 3.63, and the local legally obtained pristine SMW ROM"]
fn lunar_magic_reexports_rust_standard_and_exgfx_across_legacy_migration() {
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

    let exgfx80 = (0..0x800)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect::<Vec<_>>();
    let oracle = TemporaryDirectory::create("gfx-original-exgfx-install");
    fs::copy(&pristine, oracle.0.join("original-install.smc")).unwrap();
    fs::create_dir(oracle.0.join("Graphics")).unwrap();
    for (number, bytes) in files.iter().enumerate() {
        fs::write(
            oracle
                .0
                .join("Graphics")
                .join(format!("GFX{number:02X}.bin")),
            bytes,
        )
        .unwrap();
    }
    run_export_operation(
        &wine,
        &lunar_magic,
        &oracle.0,
        "-ImportGFX",
        "original-install.smc",
    );
    fs::create_dir(oracle.0.join("ExGraphics")).unwrap();
    fs::write(oracle.0.join("ExGraphics/ExGFX80.bin"), &exgfx80).unwrap();
    run_export_operation(
        &wine,
        &lunar_magic,
        &oracle.0,
        "-ImportExGFX",
        "original-install.smc",
    );
    let original_exgfx =
        RomImage::from_bytes(fs::read(oracle.0.join("original-install.smc")).unwrap()).unwrap();
    fs::remove_file(oracle.0.join("ExGraphics/ExGFX80.bin")).unwrap();
    run_export_operation(
        &wine,
        &lunar_magic,
        &oracle.0,
        "-ExportExGFX",
        "original-install.smc",
    );
    assert_eq!(
        fs::read(oracle.0.join("ExGraphics/ExGFX80.bin")).unwrap(),
        exgfx80,
        "Lunar Magic did not re-export its own ExGFX80 insertion"
    );

    let legacy = TemporaryDirectory::create("gfx-legacy-upgrade");
    let mut legacy_project = project;
    let exgfx_commit = lm_app::prepare_smw_us_v1_exgraphics_install(
        0,
        legacy_project.rom.clone(),
        &[(0x80, exgfx80.clone())],
    )
    .unwrap();
    legacy_project
        .apply_mutation(&exgfx_commit.description, &exgfx_commit.mutation)
        .unwrap();
    assert_eq!(
        lm_profile::probe_smw_us_v1_expanded_exanimation_runtime_generation(
            legacy_project.rom.logical_bytes()
        )
        .unwrap(),
        lm_profile::SmwUsV1ExpandedExAnimationRuntimeGeneration::Current
    );
    for (offset, len) in [(0x26b8, 4), (0x2d8e2, 4), (0x77550, 0x20)] {
        assert_eq!(
            legacy_project.rom.read(offset, len).unwrap(),
            original_exgfx.read(offset, len).unwrap(),
            "expanded ExAnimation prerequisite differs at {offset:#08X}"
        );
    }
    for (offset, original) in lm_profile::SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS
        .into_iter()
        .zip([0x08, 0x1e])
    {
        legacy_project.rom.write(offset, &[original]).unwrap();
    }
    legacy_project.rom.update_snes_checksum(0x7fdc).unwrap();
    assert!(lm_profile::requires_smw_us_v1_4bpp_graphics_warning(
        &legacy_project.rom
    ));
    let legacy_before = legacy_project.rom.clone();
    let legacy_commit =
        prepare_smw_us_v1_standard_graphics_install(0, legacy_before.clone(), &files).unwrap();
    legacy_project
        .apply_mutation(&legacy_commit.description, &legacy_commit.mutation)
        .unwrap();
    assert_eq!(
        legacy_project.rom.logical_len(),
        legacy_before.logical_len()
    );
    assert!(lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(
        &legacy_project.rom
    ));
    fs::write(
        legacy.0.join("legacy-upgraded.smc"),
        legacy_project.rom.as_file_bytes(),
    )
    .unwrap();
    run_export(&wine, &lunar_magic, &legacy.0, "legacy-upgraded.smc");
    for number in 0..0x34 {
        let name = format!("GFX{number:02X}.bin");
        assert_eq!(
            fs::read(legacy.0.join("Graphics").join(&name)).unwrap(),
            files[number],
            "legacy-upgraded {name}"
        );
    }
    fs::create_dir(legacy.0.join("ExGraphics")).unwrap();
    run_export_operation(
        &wine,
        &lunar_magic,
        &legacy.0,
        "-ExportExGFX",
        "legacy-upgraded.smc",
    );
    assert_eq!(
        fs::read(legacy.0.join("ExGraphics/ExGFX80.bin")).unwrap(),
        exgfx80,
        "Lunar Magic did not re-export the retained ExGFX80 bytes"
    );
    let route = lm_profile::smw_us_v1_exgraphics_pointer(0x80).unwrap();
    assert_eq!(
        legacy_project
            .load_decompressed_graphics_file(
                0,
                GraphicsRomLayout {
                    mapper: lm_rom::Mapper::LoRom,
                    pointers: LevelPointerTable {
                        offset: route.pointer_offset,
                        entries: 1,
                        stride: 3,
                    },
                    split_pointer_planes: None,
                    compression: GraphicsCompression::Lz2,
                    maximum_compressed_len: 0x8000,
                    maximum_decompressed_len: 0x1000,
                },
            )
            .unwrap(),
        exgfx80,
        "legacy ExGFX80 changed during standard-GFX migration"
    );

    let retained_source = RomImage::from_bytes(fs::read(&pristine).unwrap()).unwrap();
    let retained_header = retained_source.copier_header_bytes().unwrap().to_vec();
    let headerless_source = RomImage::from_bytes(retained_source.logical_bytes().to_vec()).unwrap();
    let headerless_standard =
        prepare_smw_us_v1_standard_graphics_install(0, headerless_source.clone(), &files).unwrap();
    let mut headerless_project = Project::new(headerless_source);
    headerless_project
        .apply_mutation(
            &headerless_standard.description,
            &headerless_standard.mutation,
        )
        .unwrap();
    let headerless_exgfx = lm_app::prepare_smw_us_v1_exgraphics_install(
        0,
        headerless_project.rom.clone(),
        &[(0x80, exgfx80.clone())],
    )
    .unwrap();
    headerless_project
        .apply_mutation(&headerless_exgfx.description, &headerless_exgfx.mutation)
        .unwrap();
    for (offset, original) in lm_profile::SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS
        .into_iter()
        .zip([0x08, 0x1e])
    {
        headerless_project.rom.write(offset, &[original]).unwrap();
    }
    headerless_project.rom.update_snes_checksum(0x7fdc).unwrap();
    let headerless_migration =
        prepare_smw_us_v1_standard_graphics_install(0, headerless_project.rom.clone(), &files)
            .unwrap();
    headerless_project
        .apply_mutation(
            &headerless_migration.description,
            &headerless_migration.mutation,
        )
        .unwrap();
    assert_eq!(legacy_project.rom.copier_header(), CopierHeader::Present);
    assert_eq!(
        legacy_project.rom.copier_header_bytes(),
        Some(retained_header.as_slice())
    );
    assert_eq!(headerless_project.rom.copier_header(), CopierHeader::Absent);
    assert_eq!(headerless_project.rom.copier_header_bytes(), None);
    assert_eq!(
        headerless_project.rom.logical_bytes(),
        legacy_project.rom.logical_bytes(),
        "headered and headerless pipelines produced different logical ROMs"
    );
    let headerless = TemporaryDirectory::create("gfx-headerless-variant");
    fs::write(
        headerless.0.join("headerless.smc"),
        headerless_project.rom.as_file_bytes(),
    )
    .unwrap();
    run_export(&wine, &lunar_magic, &headerless.0, "headerless.smc");
    run_export_operation(
        &wine,
        &lunar_magic,
        &headerless.0,
        "-ExportExGFX",
        "headerless.smc",
    );
    for number in 0..0x34 {
        let name = format!("GFX{number:02X}.bin");
        assert_eq!(
            fs::read(headerless.0.join("Graphics").join(&name)).unwrap(),
            files[number],
            "headerless {name}"
        );
    }
    assert_eq!(
        fs::read(headerless.0.join("ExGraphics/ExGFX80.bin")).unwrap(),
        exgfx80,
        "Lunar Magic did not re-export headerless ExGFX80"
    );

    let mut fast_source = RomImage::from_bytes(retained_source.logical_bytes().to_vec()).unwrap();
    fast_source.write(0x7fd5, &[0x30]).unwrap();
    fast_source.update_snes_checksum(0x7fdc).unwrap();
    let fast_standard =
        prepare_smw_us_v1_standard_graphics_install(0, fast_source.clone(), &files).unwrap();
    let mut fast_project = Project::new(fast_source);
    fast_project
        .apply_mutation(&fast_standard.description, &fast_standard.mutation)
        .unwrap();
    let fast_exgfx = lm_app::prepare_smw_us_v1_exgraphics_install(
        0,
        fast_project.rom.clone(),
        &[(0x80, exgfx80.clone())],
    )
    .unwrap();
    fast_project
        .apply_mutation(&fast_exgfx.description, &fast_exgfx.mutation)
        .unwrap();
    for (offset, original) in lm_profile::SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS
        .into_iter()
        .zip([0x08, 0x1e])
    {
        fast_project.rom.write(offset, &[original]).unwrap();
    }
    fast_project.rom.update_snes_checksum(0x7fdc).unwrap();
    let fast_migration =
        prepare_smw_us_v1_standard_graphics_install(0, fast_project.rom.clone(), &files).unwrap();
    fast_project
        .apply_mutation(&fast_migration.description, &fast_migration.mutation)
        .unwrap();
    assert_eq!(fast_project.rom.copier_header(), CopierHeader::Absent);
    assert_eq!(fast_project.rom.read(0x7fd5, 1).unwrap(), [0x30]);
    assert!(
        lm_rom::detect_identity(&fast_project.rom)
            .unwrap()
            .checksum_matches()
    );
    let fast = TemporaryDirectory::create("gfx-fast-lorom-variant");
    fs::write(
        fast.0.join("fast-lorom.smc"),
        fast_project.rom.as_file_bytes(),
    )
    .unwrap();
    run_export(&wine, &lunar_magic, &fast.0, "fast-lorom.smc");
    run_export_operation(
        &wine,
        &lunar_magic,
        &fast.0,
        "-ExportExGFX",
        "fast-lorom.smc",
    );
    for number in 0..0x34 {
        let name = format!("GFX{number:02X}.bin");
        assert_eq!(
            fs::read(fast.0.join("Graphics").join(&name)).unwrap(),
            files[number],
            "Fast LoROM {name}"
        );
    }
    assert_eq!(
        fs::read(fast.0.join("ExGraphics/ExGFX80.bin")).unwrap(),
        exgfx80,
        "Lunar Magic did not re-export Fast-LoROM ExGFX80"
    );

    let mut exlorom_project = Project::open_supported(legacy_project.rom.clone()).unwrap();
    exlorom_project.convert_to_64_mbit_exlorom().unwrap();
    let exgfx81 = vec![0x81; 0x800];
    let exlorom_insert = lm_app::prepare_smw_us_v1_exgraphics_install(
        0,
        exlorom_project.rom.clone(),
        &[(0x81, exgfx81.clone())],
    )
    .unwrap();
    exlorom_project
        .apply_mutation(&exlorom_insert.description, &exlorom_insert.mutation)
        .unwrap();
    assert_eq!(
        lm_rom::detect_identity(&exlorom_project.rom)
            .unwrap()
            .mapper,
        lm_rom::Mapper::ExLoRom
    );
    assert!(
        lm_rom::detect_identity(&exlorom_project.rom)
            .unwrap()
            .checksum_matches()
    );
    let exlorom = TemporaryDirectory::create("gfx-exlorom-variant");
    fs::write(
        exlorom.0.join("exlorom.smc"),
        exlorom_project.rom.as_file_bytes(),
    )
    .unwrap();
    run_export(&wine, &lunar_magic, &exlorom.0, "exlorom.smc");
    run_export_operation(
        &wine,
        &lunar_magic,
        &exlorom.0,
        "-ExportExGFX",
        "exlorom.smc",
    );
    for number in 0..0x34 {
        let name = format!("GFX{number:02X}.bin");
        assert_eq!(
            fs::read(exlorom.0.join("Graphics").join(&name)).unwrap(),
            files[number],
            "ExLoROM {name}"
        );
    }
    assert_eq!(
        fs::read(exlorom.0.join("ExGraphics/ExGFX80.bin")).unwrap(),
        exgfx80,
        "Lunar Magic did not preserve converted ExLoROM ExGFX80"
    );
    assert_eq!(
        fs::read(exlorom.0.join("ExGraphics/ExGFX81.bin")).unwrap(),
        exgfx81,
        "Lunar Magic did not re-export Rust ExLoROM ExGFX81"
    );
}
