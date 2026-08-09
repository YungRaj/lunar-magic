use lm_app::prepare_smw_us_v1_standard_graphics_install;
use lm_project::{GraphicsCompression, GraphicsRomLayout, LevelPointerTable, Project};
use lm_rats::AllocationPolicy;
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

fn run_change_compression(
    wine: &Path,
    lunar_magic: &Path,
    directory: &Path,
    rom_name: &str,
    format: &str,
) -> std::process::Output {
    let output = Command::new(wine)
        .arg(lunar_magic)
        .arg("-ChangeCompression")
        .arg(rom_name)
        .arg(format)
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Lunar Magic compression change failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
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

    let exgfx80 = (0..0x800_usize)
        .map(|index| index.to_le_bytes()[0].wrapping_mul(37).wrapping_add(11))
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
    let exlorom_lz2_bytes = fs::read(exlorom.0.join("exlorom.smc")).unwrap();
    if let Some(path) = std::env::var_os("LM_EXLOROM_LZ2_CAPTURE") {
        fs::copy(exlorom.0.join("exlorom.smc"), path).unwrap();
    }
    run_change_compression(&wine, &lunar_magic, &exlorom.0, "exlorom.smc", "LC_LZ3");
    if let Some(path) = std::env::var_os("LM_EXLOROM_LZ3_CAPTURE") {
        fs::copy(exlorom.0.join("exlorom.smc"), path).unwrap();
    }
    fs::remove_dir_all(exlorom.0.join("Graphics")).unwrap();
    fs::remove_dir_all(exlorom.0.join("ExGraphics")).unwrap();
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
            "LZ3 ExLoROM {name}"
        );
    }
    assert_eq!(
        fs::read(exlorom.0.join("ExGraphics/ExGFX80.bin")).unwrap(),
        exgfx80,
        "Lunar Magic did not preserve LZ3 ExLoROM ExGFX80"
    );
    assert_eq!(
        fs::read(exlorom.0.join("ExGraphics/ExGFX81.bin")).unwrap(),
        exgfx81,
        "Lunar Magic did not preserve LZ3 ExLoROM ExGFX81"
    );

    // Build the same transition with the Rust runtime and require original Lunar Magic to accept
    // and export every resulting stream. This is the cross-implementation gate, not merely a
    // self-consistency check between Rust encoders and decoders.
    let rust_source = RomImage::from_bytes(exlorom_lz2_bytes).unwrap();
    let replacement = lm_profile::smw_us_v1_compact_graphics_compression_migration_plan(
        &rust_source,
        0x7fdc,
        lm_profile::SmwUsV1GraphicsCompressionMode::Lz3,
    )
    .unwrap();
    let mut rust_project = Project::new(rust_source);
    rust_project
        .replace_relocatable_patch(&replacement.plan, &replacement.obsolete, 0xff)
        .unwrap();
    assert!(
        lm_rom::detect_identity(&rust_project.rom)
            .unwrap()
            .checksum_matches()
    );
    let rust = TemporaryDirectory::create("gfx-exlorom-rust-lz3");
    fs::write(
        rust.0.join("rust-exlorom.smc"),
        rust_project.rom.as_file_bytes(),
    )
    .unwrap();
    run_export(&wine, &lunar_magic, &rust.0, "rust-exlorom.smc");
    run_export_operation(
        &wine,
        &lunar_magic,
        &rust.0,
        "-ExportExGFX",
        "rust-exlorom.smc",
    );
    for number in 0..0x34 {
        let name = format!("GFX{number:02X}.bin");
        assert_eq!(
            fs::read(rust.0.join("Graphics").join(&name)).unwrap(),
            files[number],
            "Lunar Magic did not re-export Rust LZ3 ExLoROM {name}"
        );
    }
    assert_eq!(
        fs::read(rust.0.join("ExGraphics/ExGFX80.bin")).unwrap(),
        exgfx80
    );
    assert_eq!(
        fs::read(rust.0.join("ExGraphics/ExGFX81.bin")).unwrap(),
        exgfx81
    );
}

#[test]
#[ignore = "requires Wine, Lunar Magic 3.63, and an authentic SA-1 Pack SMW ROM"]
fn lunar_magic_reexports_rust_sa1_standard_graphics_install() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let wine = std::env::var_os("WINE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("wine"));
    let lunar_magic = std::env::var_os("LUNAR_MAGIC_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("lm363/Lunar Magic.exe"));
    let source = PathBuf::from(
        std::env::var_os("LM_SA1_PACK_ROM")
            .expect("LM_SA1_PACK_ROM must name an authentic checksum-valid SA-1 Pack SMW ROM"),
    );

    let baseline = TemporaryDirectory::create("sa1-gfx-baseline");
    fs::copy(&source, baseline.0.join("sa1.smc")).unwrap();
    run_export(&wine, &lunar_magic, &baseline.0, "sa1.smc");
    let mut files = (0..0x34)
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
    files[0][100] ^= 0xff;

    let original = RomImage::from_bytes(fs::read(&source).unwrap()).unwrap();
    assert_eq!(
        lm_rom::detect_identity(&original).unwrap().mapper,
        lm_rom::Mapper::Sa1
    );
    let commit = prepare_smw_us_v1_standard_graphics_install(0, original.clone(), &files).unwrap();
    let mut project = Project::new(original);
    project
        .apply_mutation(&commit.description, &commit.mutation)
        .unwrap();
    let identity = lm_rom::detect_identity(&project.rom).unwrap();
    assert_eq!(identity.mapper, lm_rom::Mapper::Sa1);
    assert!(identity.checksum_matches());

    let installed = TemporaryDirectory::create("sa1-gfx-installed");
    fs::write(
        installed.0.join("sa1-rust.smc"),
        project.rom.as_file_bytes(),
    )
    .unwrap();
    run_export(&wine, &lunar_magic, &installed.0, "sa1-rust.smc");
    for (number, expected) in files.iter().enumerate() {
        let actual = fs::read(
            installed
                .0
                .join("Graphics")
                .join(format!("GFX{number:02X}.bin")),
        )
        .unwrap();
        assert_eq!(actual, *expected, "SA-1 GFX{number:02X}");
    }

    let standard_rom = project.rom.clone();
    let exgfx80 = (0..0x800_usize)
        .map(|index| index.to_le_bytes()[0].wrapping_mul(37).wrapping_add(11))
        .collect::<Vec<_>>();
    let exgfx = lm_app::prepare_smw_us_v1_exgraphics_install(
        0,
        project.rom.clone(),
        &[(0x80, exgfx80.clone())],
    )
    .unwrap();
    project
        .apply_mutation(&exgfx.description, &exgfx.mutation)
        .unwrap();
    let identity = lm_rom::detect_identity(&project.rom).unwrap();
    assert_eq!(identity.mapper, lm_rom::Mapper::Sa1);
    assert!(identity.checksum_matches());
    fs::write(
        installed.0.join("sa1-rust-exgfx.smc"),
        project.rom.as_file_bytes(),
    )
    .unwrap();
    fs::create_dir(installed.0.join("ExGraphics")).unwrap();
    run_export_operation(
        &wine,
        &lunar_magic,
        &installed.0,
        "-ExportExGFX",
        "sa1-rust-exgfx.smc",
    );
    assert_eq!(
        fs::read(installed.0.join("ExGraphics/ExGFX80.bin")).unwrap(),
        exgfx80,
        "Lunar Magic did not re-export Rust SA-1 ExGFX80"
    );

    for file_number in [0x60_u16, 0x100] {
        let mut variant_project = Project::new(standard_rom.clone());
        let commit = lm_app::prepare_smw_us_v1_exgraphics_install(
            0,
            variant_project.rom.clone(),
            &[(file_number, exgfx80.clone())],
        )
        .unwrap();
        variant_project
            .apply_mutation(&commit.description, &commit.mutation)
            .unwrap();
        let identity = lm_rom::detect_identity(&variant_project.rom).unwrap();
        assert_eq!(identity.mapper, lm_rom::Mapper::Sa1);
        assert!(identity.checksum_matches());
        let variant = TemporaryDirectory::create(&format!("sa1-exgfx{file_number:02x}"));
        let rom_name = format!("sa1-exgfx{file_number:02x}.smc");
        fs::write(
            variant.0.join(&rom_name),
            variant_project.rom.as_file_bytes(),
        )
        .unwrap();
        fs::create_dir(variant.0.join("ExGraphics")).unwrap();
        run_export_operation(&wine, &lunar_magic, &variant.0, "-ExportExGFX", &rom_name);
        let name = format!("ExGFX{file_number:02X}.bin");
        let exported = fs::read(variant.0.join("ExGraphics").join(&name)).unwrap_or_else(|error| {
            let entries = fs::read_dir(variant.0.join("ExGraphics"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>();
            panic!("SA-1 ExGFX{file_number:02X} export failed: {error}; entries: {entries:?}")
        });
        assert_eq!(
            exported, exgfx80,
            "Lunar Magic did not re-export Rust SA-1 {name}"
        );
    }

    let mut mixed_project = Project::new(standard_rom);
    let mixed_files = [
        (0x60, exgfx80.clone()),
        (0x80, exgfx80.clone()),
        (0x100, exgfx80.clone()),
    ];
    let mixed =
        lm_app::prepare_smw_us_v1_exgraphics_install(0, mixed_project.rom.clone(), &mixed_files)
            .unwrap();
    mixed_project
        .apply_mutation(&mixed.description, &mixed.mutation)
        .unwrap();
    let mixed_directory = TemporaryDirectory::create("sa1-exgfx-mixed");
    fs::write(
        mixed_directory.0.join("sa1-exgfx-mixed.smc"),
        mixed_project.rom.as_file_bytes(),
    )
    .unwrap();
    fs::create_dir(mixed_directory.0.join("ExGraphics")).unwrap();
    run_export_operation(
        &wine,
        &lunar_magic,
        &mixed_directory.0,
        "-ExportExGFX",
        "sa1-exgfx-mixed.smc",
    );
    for (file_number, expected) in mixed_files {
        assert_eq!(
            fs::read(
                mixed_directory
                    .0
                    .join(format!("ExGraphics/ExGFX{file_number:02X}.bin")),
            )
            .unwrap(),
            expected,
            "Lunar Magic did not re-export mixed Rust SA-1 ExGFX{file_number:02X}"
        );
    }

    let mixed_rom = mixed_project.rom.clone();
    let mut replacement = exgfx80.clone();
    replacement[777] ^= 0xff;
    for (label, files, expected_numbers) in [
        (
            "replace-all",
            vec![
                (0x60, exgfx80.clone()),
                (0x80, replacement.clone()),
                (0x100, exgfx80.clone()),
            ],
            vec![0x60_u16, 0x80, 0x100],
        ),
        ("only-80", vec![(0x80, replacement.clone())], vec![0x80_u16]),
    ] {
        let mut synchronized = Project::new(mixed_rom.clone());
        let commit = lm_app::prepare_smw_us_v1_exgraphics_directory_install(
            0,
            synchronized.rom.clone(),
            &files,
        )
        .unwrap();
        synchronized
            .apply_mutation(&commit.description, &commit.mutation)
            .unwrap();
        assert!(
            lm_rom::detect_identity(&synchronized.rom)
                .unwrap()
                .checksum_matches()
        );
        let directory = TemporaryDirectory::create(&format!("sa1-exgfx-sync-{label}"));
        let rom_name = format!("sa1-exgfx-sync-{label}.smc");
        fs::write(
            directory.0.join(&rom_name),
            synchronized.rom.as_file_bytes(),
        )
        .unwrap();
        fs::create_dir(directory.0.join("ExGraphics")).unwrap();
        run_export_operation(&wine, &lunar_magic, &directory.0, "-ExportExGFX", &rom_name);
        for number in expected_numbers {
            let expected = files
                .iter()
                .find_map(|(file_number, bytes)| (*file_number == number).then_some(bytes))
                .unwrap();
            assert_eq!(
                fs::read(
                    directory
                        .0
                        .join(format!("ExGraphics/ExGFX{number:02X}.bin")),
                )
                .unwrap(),
                *expected,
                "Lunar Magic did not re-export synchronized ExGFX{number:02X}"
            );
        }
        if label == "only-80" {
            assert!(!directory.0.join("ExGraphics/ExGFX60.bin").exists());
            assert!(!directory.0.join("ExGraphics/ExGFX100.bin").exists());
        }
    }
}

#[test]
#[ignore = "requires Wine, Lunar Magic 3.63, and a locally supplied installed LZ2 SMW-US ROM"]
fn lunar_magic_reopens_rust_fast_lorom_lz3_across_copier_header_variants() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let wine = std::env::var_os("WINE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("wine"));
    let lunar_magic = std::env::var_os("LUNAR_MAGIC_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("lm363/Lunar Magic.exe"));
    let source_path = std::env::var_os("LM_LZ2_ORIGINAL_ROM")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("Super Mario World (USA).sfc"));
    let mut fast = RomImage::from_bytes(fs::read(source_path).unwrap()).unwrap();
    assert_eq!(
        lm_profile::detect_smw_us_v1_graphics_compression_mode(&fast).unwrap(),
        lm_profile::SmwUsV1GraphicsCompressionMode::Lz2Original
    );
    fast.write(0x7fd5, &[0x30]).unwrap();
    fast.update_snes_checksum(0x7fdc).unwrap();

    let oracle = TemporaryDirectory::create("fast-lorom-lz3-oracle");
    let mut headered = vec![0; 0x200];
    headered.extend_from_slice(fast.logical_bytes());
    fs::write(oracle.0.join("oracle.smc"), &headered).unwrap();
    // ChangeCompression has one required argument after the ROM path, so use the exact command
    // directly rather than the single-argument export helper.
    let output = Command::new(&wine)
        .arg(&lunar_magic)
        .arg("-ChangeCompression")
        .arg("oracle.smc")
        .arg("LC_LZ3")
        .current_dir(&oracle.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Lunar Magic Fast-LoROM oracle conversion failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::create_dir(oracle.0.join("Graphics")).unwrap();
    run_export(&wine, &lunar_magic, &oracle.0, "oracle.smc");
    let expected_files = (0..0x34)
        .map(|number| {
            fs::read(
                oracle
                    .0
                    .join("Graphics")
                    .join(format!("GFX{number:02X}.bin")),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();

    let mut expected_logical = None;
    for (label, bytes, expected_header) in [
        (
            "headerless",
            fast.logical_bytes().to_vec(),
            CopierHeader::Absent,
        ),
        ("headered", headered, CopierHeader::Present),
    ] {
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
        let plan = lm_profile::smw_us_v1_lz3_installation_plan(
            &image,
            AllocationPolicy::lorom(0x10_0000..0x20_0000),
            0x7fdc,
        )
        .unwrap();
        let mut project = Project::new(image);
        project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(project.rom.copier_header(), expected_header);
        assert_eq!(project.rom.logical_bytes()[0x7fd5], 0x30);
        assert!(
            lm_rom::detect_identity(&project.rom)
                .unwrap()
                .checksum_matches()
        );
        match &expected_logical {
            Some(expected) => assert_eq!(project.rom.logical_bytes(), expected),
            None => expected_logical = Some(project.rom.logical_bytes().to_vec()),
        }

        let reopen = TemporaryDirectory::create(&format!("fast-lorom-lz3-{label}"));
        let rom_name = format!("rust-{label}.smc");
        let rust_bytes = project.rom.as_file_bytes().to_vec();
        fs::write(reopen.0.join(&rom_name), &rust_bytes).unwrap();
        let output = Command::new(&wine)
            .arg(&lunar_magic)
            .arg("-ChangeCompression")
            .arg(&rom_name)
            .arg("LC_LZ3")
            .current_dir(&reopen.0)
            .output()
            .unwrap();
        assert!(output.status.success(), "{label}: {output:?}");
        let reopened = fs::read(reopen.0.join(&rom_name)).unwrap();
        if expected_header == CopierHeader::Present {
            assert_eq!(reopened, rust_bytes, "{label} was not recognized as LZ3");
        } else {
            assert_eq!(
                &reopened[0x200..],
                rust_bytes,
                "{label} logical ROM changed"
            );
        }
        fs::create_dir(reopen.0.join("Graphics")).unwrap();
        run_export(&wine, &lunar_magic, &reopen.0, &rom_name);
        for (number, expected) in expected_files.iter().enumerate() {
            assert_eq!(
                fs::read(
                    reopen
                        .0
                        .join("Graphics")
                        .join(format!("GFX{number:02X}.bin")),
                )
                .unwrap(),
                *expected,
                "{label} GFX{number:02X}"
            );
        }
        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), bytes);
    }
}
