use lm_overworld::{OverworldWarpEndpoint, OverworldWarpLink, OverworldWarpLinkTable};
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, SMW_US_V1_OVERWORLD_WARP_ENTRY_HOOK_OFFSET,
    SMW_US_V1_OVERWORLD_WARP_RETURN_HOOK_OFFSET, smw_us_v1_overworld_warp_patch_locator,
};
use lm_project::{OverworldWarpLinkStorage, Project};
use lm_rats::{AllocationPolicy, FreeSpaceAllocator};
use lm_rom::{Mapper, RomImage, compute_snes_checksum, detect_identity, pc_to_snes};
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn table(count: u16) -> OverworldWarpLinkTable {
    OverworldWarpLinkTable {
        links: (0..count)
            .map(|value| OverworldWarpLink {
                source: OverworldWarpEndpoint {
                    packed_vertical: value,
                    horizontal_tile: value + 0x100,
                },
                destination: OverworldWarpEndpoint {
                    packed_vertical: value + 0x200,
                    horizontal_tile: value + 0x300,
                },
            })
            .collect(),
    }
}

fn run(args: &[&std::path::Path], operation: &str) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lm-cli"));
    command.arg(operation);
    command.args(args);
    command.output().unwrap()
}

fn legacy_rom(original: Vec<u8>, links: &OverworldWarpLinkTable) -> Vec<u8> {
    let mut image = RomImage::from_bytes(original).unwrap();
    image.expand(Mapper::LoRom, 0x90_000, 0xff).unwrap();
    let mut bytes = image.logical_bytes().to_vec();
    let policy = AllocationPolicy::lorom(0x80_000..0x90_000);
    let runtime = FreeSpaceAllocator::new(&mut bytes, policy.clone())
        .allocate(&[0xff; 0x80])
        .unwrap();
    let planes = links.encode_planes().unwrap();
    let plane_len = planes.source_vertical.len();
    let mut payload = planes.source_vertical;
    payload.extend_from_slice(&planes.source_horizontal);
    payload.extend_from_slice(&planes.destination_vertical);
    payload.extend_from_slice(&planes.destination_horizontal);
    let data = FreeSpaceAllocator::new(&mut bytes, policy)
        .allocate(&payload)
        .unwrap();
    let patch = runtime.payload.start;
    bytes[patch + 0x10] = u8::try_from(links.links.len()).unwrap();
    for (operand, addend) in
        [0x14, 0x24, 0x47, 0x59]
            .into_iter()
            .zip([0, plane_len, plane_len * 2, plane_len * 3])
    {
        bytes[patch + operand..patch + operand + 3].copy_from_slice(
            &pc_to_snes(Mapper::LoRom, data.payload.start + addend)
                .unwrap()
                .to_le_bytes()[..3],
        );
    }
    let entry = pc_to_snes(Mapper::LoRom, patch).unwrap().to_le_bytes();
    let return_target = pc_to_snes(Mapper::LoRom, patch + 0x40)
        .unwrap()
        .to_le_bytes();
    bytes[SMW_US_V1_OVERWORLD_WARP_ENTRY_HOOK_OFFSET
        ..SMW_US_V1_OVERWORLD_WARP_ENTRY_HOOK_OFFSET + 5]
        .copy_from_slice(&[0x22, entry[0], entry[1], entry[2], 0x60]);
    bytes[SMW_US_V1_OVERWORLD_WARP_RETURN_HOOK_OFFSET
        ..SMW_US_V1_OVERWORLD_WARP_RETURN_HOOK_OFFSET + 4]
        .copy_from_slice(&[0x22, return_target[0], return_target[1], return_target[2]]);
    let checksum = compute_snes_checksum(&bytes, SMW_US_V1_CHECKSUM_FIELD).unwrap();
    bytes[SMW_US_V1_CHECKSUM_FIELD..SMW_US_V1_CHECKSUM_FIELD + 4]
        .copy_from_slice(&checksum.encoded());
    bytes
}

#[test]
fn built_cli_installs_exports_grows_and_reopens_lunar_magic_warp_patch() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = std::env::temp_dir().join(format!(
        "lm overworld warps 日本語 {} {}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = root.join("Super Mario World (USA).sfc");
    let links30 = directory.join("thirty.lmow");
    let expanded30 = directory.join("expanded thirty.sfc");
    let exported = directory.join("exported.lmow");
    let links40 = directory.join("forty.lmow");
    let expanded40 = directory.join("expanded forty.sfc");
    fs::write(&links30, table(30).encode_native_warp_file().unwrap()).unwrap();

    let install = run(
        &[&input, &links30, &expanded30],
        "smw-overworld-warp-import",
    );
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let image30 = RomImage::from_bytes(fs::read(&expanded30).unwrap()).unwrap();
    assert_eq!(image30.logical_len(), 0x90_000);
    assert!(detect_identity(&image30).unwrap().checksum_matches());
    let project30 = Project::open_supported(image30).unwrap();
    let loaded30 = project30
        .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
        .unwrap();
    assert_eq!(loaded30.table, table(30));
    assert!(matches!(
        loaded30.storage,
        OverworldWarpLinkStorage::CurrentPatch { .. }
    ));

    let export = run(&[&expanded30, &exported], "smw-overworld-warp-export");
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    assert_eq!(
        OverworldWarpLinkTable::decode_native_warp_file(&fs::read(&exported).unwrap()).unwrap(),
        table(30)
    );

    fs::write(&links40, table(40).encode_native_warp_file().unwrap()).unwrap();
    let grow = run(
        &[&expanded30, &links40, &expanded40],
        "smw-overworld-warp-import",
    );
    assert!(
        grow.status.success(),
        "{}",
        String::from_utf8_lossy(&grow.stderr)
    );
    let image40 = RomImage::from_bytes(fs::read(&expanded40).unwrap()).unwrap();
    assert!(detect_identity(&image40).unwrap().checksum_matches());
    assert_eq!(
        Project::open_supported(image40)
            .unwrap()
            .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
            .unwrap()
            .table,
        table(40)
    );
    assert_eq!(fs::read(&input).unwrap().len(), 0x80_000);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn built_cli_migrates_legacy_lunar_magic_warp_patch_to_current() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = std::env::temp_dir().join(format!(
        "lm legacy warps {} {}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let original_path = root.join("Super Mario World (USA).sfc");
    let legacy_path = directory.join("legacy.sfc");
    let links_path = directory.join("thirty five.lmow");
    let migrated_path = directory.join("migrated.sfc");
    fs::write(
        &legacy_path,
        legacy_rom(fs::read(&original_path).unwrap(), &table(30)),
    )
    .unwrap();
    let legacy =
        Project::open_supported(RomImage::from_bytes(fs::read(&legacy_path).unwrap()).unwrap())
            .unwrap()
            .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
            .unwrap();
    assert!(matches!(
        legacy.storage,
        OverworldWarpLinkStorage::LegacyPatch { .. }
    ));
    fs::write(&links_path, table(35).encode_native_warp_file().unwrap()).unwrap();
    let migrate = run(
        &[&legacy_path, &links_path, &migrated_path],
        "smw-overworld-warp-import",
    );
    assert!(
        migrate.status.success(),
        "{}",
        String::from_utf8_lossy(&migrate.stderr)
    );
    let image = RomImage::from_bytes(fs::read(&migrated_path).unwrap()).unwrap();
    assert_eq!(image.logical_len(), 0x90_000);
    assert!(detect_identity(&image).unwrap().checksum_matches());
    let loaded = Project::open_supported(image)
        .unwrap()
        .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
        .unwrap();
    assert_eq!(loaded.table, table(35));
    assert!(matches!(
        loaded.storage,
        OverworldWarpLinkStorage::CurrentPatch { .. }
    ));
    fs::remove_dir_all(directory).unwrap();
}
