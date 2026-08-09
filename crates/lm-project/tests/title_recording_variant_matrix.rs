use lm_project::{Project, TitleRecordingPatchLocator, TitleRecordingStorage};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, compute_snes_checksum, detect_identity};
use lm_title::TitleScreenRecording;

const COPIER_PREFIX: [u8; 512] = {
    let mut prefix = [0_u8; 512];
    prefix[0] = 0x40;
    prefix[8] = 0xaa;
    prefix[9] = 0xbb;
    prefix[10] = 0x04;
    prefix
};

#[derive(Clone, Copy)]
struct IdentityCase {
    title: &'static [u8; 21],
    region: u8,
    map_mode: u8,
}

#[derive(Clone, Copy)]
enum StorageCase {
    Absent,
    Installed,
}

fn mapper(map_mode: u8) -> Mapper {
    match map_mode {
        0x20 | 0x30 => Mapper::LoRom,
        0x23 => Mapper::Sa1,
        0x32 => Mapper::ExLoRom,
        _ => unreachable!(),
    }
}

fn locator(mapper: Mapper) -> TitleRecordingPatchLocator {
    let mut hook_template = [0x6a; TitleRecordingPatchLocator::HOOK_LEN];
    hook_template[0] = 0x22;
    let mut runtime_template = [0x5a; TitleRecordingPatchLocator::RUNTIME_LEN];
    runtime_template[TitleRecordingPatchLocator::RUNTIME_LEN - 1] = 0xff;
    TitleRecordingPatchLocator {
        mapper,
        hook: 0x300,
        pristine_hook: [0x44; TitleRecordingPatchLocator::HOOK_LEN],
        hook_template,
        runtime_template,
        rom_size_field: None,
        expansion_writes: &[],
        checksum_compensation: None,
    }
}

fn recording(seed: u8, len: usize) -> TitleScreenRecording {
    let mut bytes = vec![seed; len];
    *bytes.last_mut().unwrap() = 0xff;
    TitleScreenRecording::from_bytes(bytes).unwrap()
}

fn options() -> AllocationPolicy {
    AllocationPolicy {
        search: 0x1_0000..0x3_0000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![ProtectedRange(0x300..0x311), ProtectedRange(0x7fdc..0x7fe0)],
    }
}

fn variant_rom(case: IdentityCase, storage: StorageCase, copier_header: bool) -> Vec<u8> {
    let mapper = mapper(case.map_mode);
    let logical_len = if case.map_mode == 0x32 {
        0x40_8000
    } else {
        0x8_0000
    };
    let mut logical = vec![0xff; logical_len];
    let locator = locator(mapper);
    logical[locator.hook..locator.hook + TitleRecordingPatchLocator::HOOK_LEN]
        .copy_from_slice(&locator.pristine_hook);
    let header = 0x7fc0;
    logical[header..header + 21].copy_from_slice(case.title);
    logical[header + 0x15] = case.map_mode;
    logical[header + 0x19] = case.region;
    logical[header + 0x1b] = 0;
    let checksum = compute_snes_checksum(&logical, header + 0x1c).unwrap();
    logical[header + 0x1c..header + 0x20].copy_from_slice(&checksum.encoded());
    let mut project = Project::new(RomImage::from_bytes(logical).unwrap());
    if matches!(storage, StorageCase::Installed) {
        project
            .save_title_recording_detected(
                &recording(0x12, 7),
                &locator,
                &options(),
                header + 0x1c,
                0xff,
            )
            .unwrap();
    }
    let logical = project.rom.logical_bytes();
    if copier_header {
        let mut physical = COPIER_PREFIX.to_vec();
        physical.extend_from_slice(logical);
        physical
    } else {
        logical.to_vec()
    }
}

fn edit_variant(physical: Vec<u8>, storage: StorageCase) -> Vec<u8> {
    let original = physical.clone();
    let image = RomImage::from_bytes(physical).unwrap();
    let identity = detect_identity(&image).unwrap();
    let locator = locator(identity.mapper);
    let mut project = Project::new(image);
    let loaded = project.load_title_recording_detected(&locator).unwrap();
    assert!(matches!(
        (&storage, loaded.storage),
        (StorageCase::Absent, TitleRecordingStorage::Absent)
            | (
                StorageCase::Installed,
                TitleRecordingStorage::Installed { .. }
            )
    ));
    let edited_recording = recording(0x56, 0x101);
    project
        .save_title_recording_detected(&edited_recording, &locator, &options(), 0x7fdc, 0xff)
        .unwrap();
    assert_eq!(
        project
            .load_title_recording_detected(&locator)
            .unwrap()
            .recording,
        Some(edited_recording)
    );
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    let edited = project.rom.as_file_bytes().to_vec();
    assert!(project.undo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), original);
    assert!(project.redo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), edited);
    edited
}

#[test]
fn install_and_update_match_every_supported_mapper_header_and_storage_variant() {
    const SMW: &[u8; 21] = b"SUPER MARIOWORLD     ";
    const ALL_STARS_WORLD: &[u8; 21] = b"ALL_STARS + WORLD    ";
    let identities = [(SMW, 0), (SMW, 1), (ALL_STARS_WORLD, 1)];
    for &(title, region) in &identities {
        for map_mode in [0x20, 0x30, 0x23, 0x32] {
            for storage in [StorageCase::Absent, StorageCase::Installed] {
                let case = IdentityCase {
                    title,
                    region,
                    map_mode,
                };
                let headerless = variant_rom(case, storage, false);
                let headered = variant_rom(case, storage, true);
                let edited_headerless = edit_variant(headerless, storage);
                let edited_headered = edit_variant(headered, storage);
                assert_eq!(&edited_headered[..512], &COPIER_PREFIX);
                assert_eq!(&edited_headered[512..], edited_headerless);
            }
        }
    }
}
