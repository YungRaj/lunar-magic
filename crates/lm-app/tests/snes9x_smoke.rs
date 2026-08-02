use lm_app::{AppState, Command as AppCommand, Map16ControllerEdit, SmwMap16Controller};
use lm_level::{
    CustomTimeSettings, Map16Address, Map16Quadrant, NativeLayer2Data, ObjectEdit,
    SpriteLengthTable, SpriteToken, Subtile,
};
use lm_profile::{SmwUsV1CompleteMap16SaveOptions, load_smw_us_v1_transferred_map16};
use lm_project::Project;
use lm_project::{LevelLayer2SaveOptions, LevelSaveOptions};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct SmokeDirectory(PathBuf);

impl SmokeDirectory {
    fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lm-snes9x-smoke-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create Snes9x smoke directory");
        Self(path)
    }
}

impl Drop for SmokeDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn path_executable(names: &[&str]) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
            .find(|candidate| candidate.is_file())
    })
}

fn snes9x_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SNES9X_BIN").map(PathBuf::from) {
        return path.is_file().then_some(path);
    }

    #[cfg(target_os = "macos")]
    {
        let application = PathBuf::from("/Applications/Snes9x.app/Contents/MacOS/Snes9x");
        if application.is_file() {
            return Some(application);
        }
        path_executable(&["snes9x"])
    }
    #[cfg(target_os = "windows")]
    {
        path_executable(&["snes9x-x64.exe", "snes9x.exe"])
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        path_executable(&["snes9x-gtk", "snes9x"])
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        None
    }
}

fn require_snes9x_binary() -> PathBuf {
    snes9x_binary().unwrap_or_else(|| {
        panic!(
            "Snes9x executable was not found; set SNES9X_BIN to its executable path (the test also searches the platform default and PATH)"
        )
    })
}

fn source_rom(root: &Path) -> PathBuf {
    const PRISTINE_SMW_US_SHA256: &str =
        "0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b";
    for path in [
        root.join("Super Mario World (USA).sfc"),
        root.join("SMW-working.sfc"),
        root.join("sysLMRestore/smwOrig.smc"),
    ] {
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(image) = RomImage::from_bytes(bytes) else {
            continue;
        };
        if lm_oracle::sha256_hex(image.logical_bytes()) == PRISTINE_SMW_US_SHA256 {
            return path;
        }
    }
    panic!("verified pristine SMW-US fixture not found");
}

fn require_snes9x_initialization(snes9x: &Path, output: &Path) {
    let child = Command::new(snes9x)
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch Snes9x");
    let mut child = ChildGuard(child);
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if let Some(status) = child.0.try_wait().expect("query Snes9x process") {
            panic!("Snes9x exited during generated-ROM initialization with {status}");
        }
        thread::sleep(Duration::from_millis(100));
    }
    child.0.kill().expect("stop Snes9x smoke process");
    child.0.wait().expect("reap Snes9x smoke process");
}

#[test]
#[ignore = "requires local Snes9x plus the supplied legally obtained SMW ROM fixture"]
fn rust_expanded_rom_survives_snes9x_initialization() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();

    let mut project = Project::new(
        RomImage::from_bytes(fs::read(source_rom(&root)).expect("read source SMW ROM"))
            .expect("decode source SMW ROM"),
    );
    project
        .expand_rom(Mapper::LoRom, 0x10_0000, 0xff, 0x7fdc)
        .expect("expand and checksum generated ROM");

    let directory = SmokeDirectory::create();
    let output = directory.0.join("Rust-generated-SMW.sfc");
    fs::write(&output, project.save_snapshot()).expect("write generated ROM");

    require_snes9x_initialization(&snes9x, &output);
}

#[test]
#[ignore = "requires local Snes9x plus the supplied legally obtained SMW ROM fixture"]
fn rust_map16_edit_survives_snes9x_initialization() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let mut app = AppState::default();
    app.load_rom(fs::read(source_rom(&root)).expect("read source SMW ROM"))
        .expect("open source SMW ROM");
    app.dispatch(AppCommand::ShowMap16)
        .expect("enter Map16 mode");
    let mut controller =
        SmwMap16Controller::decode(&app.controller_snapshot().expect("capture ROM snapshot"))
            .expect("decode pristine native Map16");
    controller
        .apply_edits(&[Map16ControllerEdit::SetSubtile {
            address: Map16Address { page: 0, tile: 0 },
            quadrant: Map16Quadrant::BottomRight,
            subtile: Subtile(0x2345),
            resolution_limit: 2048,
        }])
        .expect("stage native Map16 edit");
    let prepared = controller
        .prepare_commit(
            "Snes9x native Map16 smoke edit",
            &SmwUsV1CompleteMap16SaveOptions {
                allocation: AllocationPolicy {
                    search: 0x80_000..0x10_0000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0, 0xff],
                    protected: vec![ProtectedRange(0x7fc0..0x8000)],
                },
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .expect("prepare expanding native Map16 commit");
    app.dispatch(prepared.into_command())
        .expect("dispatch native Map16 commit");
    let project = app.project().expect("retain edited project");
    assert_eq!(project.rom.logical_len(), 0x10_0000);
    assert_eq!(
        load_smw_us_v1_transferred_map16(project)
            .expect("reopen native Map16")
            .definitions[3],
        0x2345
    );

    let directory = SmokeDirectory::create();
    let output = directory.0.join("Rust-Map16-edited-SMW.sfc");
    fs::write(&output, project.save_snapshot()).expect("write Map16-edited ROM");
    require_snes9x_initialization(&snes9x, &output);
}

#[test]
#[ignore = "requires local Snes9x plus retained Lunar Magic 3.63 installed-ROM fixture"]
fn rust_layer2_edit_survives_snes9x_initialization() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let installed = root.join("oracle-work/lm363/pristine-us/level-save-105/after.smc");
    let mut project = Project::new(
        RomImage::from_bytes(fs::read(installed).expect("read installed SMW fixture"))
            .expect("decode installed SMW fixture"),
    );
    let layout =
        lm_profile::smw_us_v1_layer2_layout(&project.rom).expect("detect installed Layer 2 layout");
    let mut loaded = project
        .load_level_layer2_with_descriptor(0x105, 0, layout)
        .expect("load level 105 Layer 2");
    let NativeLayer2Data::Tilemap(bytes) = &mut loaded.data else {
        panic!("level 105 must use compressed Layer 2 tilemap storage");
    };
    bytes[0] ^= 1;

    let allocation_start = project.rom.logical_len();
    let logical_len = allocation_start + 0x8000;
    project
        .expand_rom(Mapper::LoRom, logical_len, 0xff, 0x7fdc)
        .expect("expand edited ROM");
    project
        .save_level_layer2_with_descriptor_and_checksum(
            0x105,
            0,
            &loaded,
            layout,
            &LevelLayer2SaveOptions {
                allocation: AllocationPolicy {
                    search: allocation_start..logical_len,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![
                        ProtectedRange(0x2e600..0x2ec00),
                        ProtectedRange(0x77310..0x77510),
                        ProtectedRange(0x7fc0..0x8000),
                    ],
                },
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
            0x7fdc,
        )
        .expect("save edited Layer 2 and checksum");
    assert_eq!(
        project
            .load_level_layer2_with_descriptor(0x105, 0, layout)
            .expect("reopen edited Layer 2"),
        loaded
    );

    let directory = SmokeDirectory::create();
    let output = directory.0.join("Rust-Layer2-edited-SMW.smc");
    fs::write(&output, project.save_snapshot()).expect("write Layer 2 edited ROM");
    require_snes9x_initialization(&snes9x, &output);
}

#[test]
#[ignore = "requires local Snes9x plus the supplied legally obtained SMW ROM fixture"]
fn rust_layer1_object_edit_survives_snes9x_initialization() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let sprite_lengths = SpriteLengthTable::standard();
    let mut project = Project::new(
        RomImage::from_bytes(fs::read(source_rom(&root)).expect("read source SMW ROM"))
            .expect("decode source SMW ROM"),
    );
    let mut level = project
        .load_level_slot(0x105, layout, &sprite_lengths)
        .expect("load level 105");
    let duplicate = level
        .layer1
        .objects
        .records
        .first()
        .expect("level 105 must contain an object")
        .clone();
    level
        .layer1
        .objects
        .apply_edits(&[ObjectEdit::Insert {
            index: 1,
            record: duplicate,
        }])
        .expect("insert standard Layer 1 object");

    let allocation_start = project.rom.logical_len();
    let logical_len = 0x10_0000;
    project
        .expand_rom(Mapper::LoRom, logical_len, 0xff, 0x7fdc)
        .expect("expand object-edited ROM");
    let allocation = AllocationPolicy {
        search: allocation_start..logical_len,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x2e000..0x2e600),
            ProtectedRange(0x7fc0..0x8000),
        ],
    };
    project
        .save_level_layer1_with_checksum(
            layout,
            &level,
            0x7fdc,
            &LevelSaveOptions {
                layer1_allocation: allocation.clone(),
                sprite_allocation: allocation,
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .expect("save standard Layer 1 object edit");
    assert_eq!(
        project
            .load_level_slot(0x105, layout, &sprite_lengths)
            .expect("reopen object-edited level")
            .layer1,
        level.layer1
    );

    let directory = SmokeDirectory::create();
    let output = directory.0.join("Rust-Layer1-object-edited-SMW.sfc");
    fs::write(&output, project.save_snapshot()).expect("write Layer 1 object-edited ROM");
    require_snes9x_initialization(&snes9x, &output);
}

#[test]
#[ignore = "requires local Snes9x plus the supplied legally obtained SMW ROM fixture"]
fn rust_custom_time_and_support_patch_b_survive_snes9x_initialization() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let sprite_lengths = SpriteLengthTable::standard();
    let custom_time = CustomTimeSettings::new(0xabc, true).expect("construct custom time");
    let mut project = Project::new(
        RomImage::from_bytes(fs::read(source_rom(&root)).expect("read source SMW ROM"))
            .expect("decode source SMW ROM"),
    );
    let patch =
        lm_profile::smw_us_v1_support_patch_b_installation_plan(project.rom.logical_bytes())
            .expect("build support patch B installation");
    project
        .install_relocatable_patch(&patch)
        .expect("install support patch B");
    let mut level = project
        .load_level_slot(0x105, layout, &sprite_lengths)
        .expect("load level 105");
    level
        .layer1
        .objects
        .set_custom_time(false, Some(custom_time))
        .expect("stage forced custom time");

    let allocation_start = project.rom.logical_len();
    let logical_len = 0x10_0000;
    project
        .expand_rom(Mapper::LoRom, logical_len, 0xff, 0x7fdc)
        .expect("expand custom-time ROM");
    let allocation = AllocationPolicy {
        search: allocation_start..logical_len,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x2e000..0x2e600),
            ProtectedRange(0x7fc0..0x8000),
        ],
    };
    project
        .save_level_layer1_with_checksum(
            layout,
            &level,
            0x7fdc,
            &LevelSaveOptions {
                layer1_allocation: allocation.clone(),
                sprite_allocation: allocation,
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .expect("save custom-time level");
    let reopened = project
        .load_level_slot(0x105, layout, &sprite_lengths)
        .expect("reopen custom-time level");
    assert_eq!(
        reopened.layer1.objects.custom_time(false),
        Some(custom_time)
    );
    assert_eq!(
        lm_profile::detect_smw_us_v1_support_patch_b(project.rom.logical_bytes())
            .expect("authenticate support patch B"),
        lm_profile::SmwUsV1SupportPatchBState::Installed
    );

    let directory = SmokeDirectory::create();
    let output = directory.0.join("Rust-custom-time-support-patch-B-SMW.sfc");
    fs::write(&output, project.save_snapshot()).expect("write custom-time ROM");
    require_snes9x_initialization(&snes9x, &output);
}

#[test]
#[ignore = "requires local Snes9x plus the supplied legally obtained SMW ROM fixture"]
fn rust_standard_sprite_edit_survives_snes9x_initialization() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let sprite_lengths = SpriteLengthTable::standard();
    let mut project = Project::new(
        RomImage::from_bytes(fs::read(source_rom(&root)).expect("read source SMW ROM"))
            .expect("decode source SMW ROM"),
    );
    let original = project
        .load_level_slot(0x105, layout, &sprite_lengths)
        .expect("load level 105");
    let mut replacement = original.clone();
    let SpriteToken::Record(sprite) = replacement
        .sprites
        .tokens
        .first_mut()
        .expect("level 105 must contain a sprite")
    else {
        panic!("level 105 must begin with an ordinary sprite");
    };
    let mut fields = sprite
        .native_fields()
        .expect("decode standard sprite placement fields");
    fields.x = (fields.x + 1) & 0x0f;
    sprite
        .set_native_fields(fields, &sprite_lengths)
        .expect("encode standard sprite placement fields");

    assert!(
        project
            .save_level_sprites_in_place_with_checksum(
                layout,
                &original,
                &replacement,
                &sprite_lengths,
                0x7fdc,
            )
            .expect("save standard sprite edit in place")
    );
    assert_eq!(
        project
            .load_level_slot(0x105, layout, &sprite_lengths)
            .expect("reopen sprite-edited level")
            .sprites,
        replacement.sprites
    );

    let directory = SmokeDirectory::create();
    let output = directory.0.join("Rust-standard-sprite-edited-SMW.sfc");
    fs::write(&output, project.save_snapshot()).expect("write sprite-edited ROM");
    require_snes9x_initialization(&snes9x, &output);
}
