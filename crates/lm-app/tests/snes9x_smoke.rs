use lm_app::{
    AppState, Command as AppCommand, Map16ControllerEdit, OverworldControllerEdit,
    OverworldLayerId, SmwMainOverworldLayer2Controller, SmwMap16Controller,
    decode_map16_bitmap_png_image,
};
use lm_graphics::{Bgr555, CompactExAnimation, Palette};
use lm_level::{
    CustomTimeSettings, Layer1VerticalScrollMode, Map16Address, Map16Quadrant, NativeLayer2Data,
    NativeSpriteHeader, NativeSpriteStream, ObjectEdit, SpriteLengthTable, SpriteToken, Subtile,
};
use lm_overworld::{
    EventReveal, EventRevealTable, OverworldEndpoint, OverworldLayer, OverworldMessage,
    OverworldSprite, Submap,
};
use lm_profile::{SmwUsV1CompleteMap16SaveOptions, load_smw_us_v1_transferred_map16};
use lm_project::{
    CompleteOverworldData, CompleteOverworldRomLayout, CompleteOverworldSaveOptions,
    EndpointRomLayout, EndpointSaveOptions, EventRevealRomLayout, EventRevealSaveOptions,
    ExAnimationRomLayout, ExAnimationSaveOptions, LevelLayer2SaveOptions, LevelPointerTable,
    LevelSaveOptions, MessageRomLayout, MessageSaveOptions, OverworldLayers,
    OverworldLayersRomLayout, OverworldSaveOptions, PaletteRomLayout, PaletteSaveOptions, Project,
    SpriteRomLayout, SpriteSaveOptions,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage};
use lm_title::{TitleScreenRecording, decode_snes9x_title_recording, decode_snes9x_wram};
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

const SMW_GAME_MODE: usize = 0x0100;
const SMW_OVERWORLD_MODE: u8 = 0x0e;
const SMW_MARIO_SUBMAP: usize = 0x1f11;
const SMW_MARIO_POSITION: usize = 0x1f17;
const SMW_MARIO_GRID_POSITION: usize = 0x1f1f;
const SMW_LEVEL_MODE: u8 = 0x14;
const SMW_CURRENT_MUSIC: usize = 0x0dda;
const SMW_SPRITE_MEMORY: usize = 0x1692;
const SMW_SPRITE_BUOYANCY: usize = 0x190e;
const SMW_LAYER1_VERTICAL_SCROLL_ENABLED: usize = 0x1411;
const SMW_LAYER1_VERTICAL_SCROLL_MODE: usize = 0x1412;
const SMW_TIMER_HUNDREDS: usize = 0x0f31;
const MAX_GAMEPLAY_STATE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_GAMEPLAY_SCREENSHOT_BYTES: u64 = 16 * 1024 * 1024;
const GAMEPLAY_DRIVER_TIMEOUT: Duration = Duration::from_secs(120);

fn read_regular_bounded(path: &Path, limit: u64, label: &str) -> Vec<u8> {
    let metadata = fs::symlink_metadata(path)
        .unwrap_or_else(|error| panic!("read {label} metadata at {}: {error}", path.display()));
    assert!(
        metadata.file_type().is_file(),
        "{label} is not a regular nonsymlink file: {}",
        path.display()
    );
    assert!(
        metadata.len() <= limit,
        "{label} exceeds {limit} bytes: {}",
        path.display()
    );
    fs::read(path).unwrap_or_else(|error| panic!("read {label} at {}: {error}", path.display()))
}

fn validate_overworld_path_gameplay_evidence(
    snapshot: &[u8],
    screenshot: &[u8],
    expected: OverworldEndpoint,
) {
    let wram = decode_snes9x_wram(snapshot).expect("decode post-traversal Snes9x WRAM");
    assert_eq!(wram[SMW_GAME_MODE], SMW_OVERWORLD_MODE);
    assert_eq!(wram[SMW_MARIO_SUBMAP], expected.submap);
    assert_eq!(
        u16::from_le_bytes([wram[SMW_MARIO_POSITION], wram[SMW_MARIO_POSITION + 1],]),
        expected.x
    );
    assert_eq!(
        u16::from_le_bytes([wram[SMW_MARIO_POSITION + 2], wram[SMW_MARIO_POSITION + 3],]),
        expected.y
    );
    assert_eq!(
        u16::from_le_bytes([
            wram[SMW_MARIO_GRID_POSITION],
            wram[SMW_MARIO_GRID_POSITION + 1],
        ]),
        expected.x >> 4
    );
    assert_eq!(
        u16::from_le_bytes([
            wram[SMW_MARIO_GRID_POSITION + 2],
            wram[SMW_MARIO_GRID_POSITION + 3],
        ]),
        expected.y >> 4
    );

    let image =
        decode_map16_bitmap_png_image(screenshot).expect("decode post-traversal Snes9x screenshot");
    assert!(
        (256..=512).contains(&image.width) && (224..=478).contains(&image.height),
        "unexpected Snes9x screenshot dimensions {}x{}",
        image.width,
        image.height
    );
    let first = image.pixels.first().expect("nonempty Snes9x screenshot");
    assert!(
        image.pixels.iter().any(|pixel| pixel != first),
        "Snes9x screenshot contains only one color"
    );
}

fn require_overworld_path_gameplay_evidence(
    snes9x: &Path,
    rom: &Path,
    source: OverworldEndpoint,
    expected: OverworldEndpoint,
) {
    let driver = std::env::var_os("SNES9X_GAMEPLAY_DRIVER")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .unwrap_or_else(|| {
            panic!(
                "SNES9X_GAMEPLAY_DRIVER must name the platform driver that traverses the route, saves a Snes9x snapshot, and captures a game screenshot"
            )
        });
    let directory = rom.parent().expect("temporary gameplay ROM parent");
    let snapshot = directory.join("overworld-path-after.frz");
    let screenshot = directory.join("overworld-path-after.png");
    let child = Command::new(&driver)
        .arg("--emulator")
        .arg(snes9x)
        .arg("--rom")
        .arg(rom)
        .arg("--scenario")
        .arg("smw-overworld-path-link")
        .arg("--source-x")
        .arg(format!("{:04X}", source.x))
        .arg("--source-y")
        .arg(format!("{:04X}", source.y))
        .arg("--source-submap")
        .arg(format!("{:02X}", source.submap))
        .arg("--expected-x")
        .arg(format!("{:04X}", expected.x))
        .arg("--expected-y")
        .arg(format!("{:04X}", expected.y))
        .arg("--expected-submap")
        .arg(format!("{:02X}", expected.submap))
        .arg("--snapshot")
        .arg(&snapshot)
        .arg("--screenshot")
        .arg(&screenshot)
        .stdin(Stdio::null())
        .spawn()
        .expect("launch Snes9x gameplay driver");
    let mut child = ChildGuard(child);
    let deadline = Instant::now() + GAMEPLAY_DRIVER_TIMEOUT;
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("query Snes9x gameplay driver") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "Snes9x gameplay driver exceeded {GAMEPLAY_DRIVER_TIMEOUT:?}"
        );
        thread::sleep(Duration::from_millis(100));
    };
    assert!(status.success(), "Snes9x gameplay driver failed: {status}");
    let snapshot = read_regular_bounded(
        &snapshot,
        MAX_GAMEPLAY_STATE_BYTES,
        "Snes9x gameplay snapshot",
    );
    let screenshot = read_regular_bounded(
        &screenshot,
        MAX_GAMEPLAY_SCREENSHOT_BYTES,
        "Snes9x gameplay screenshot",
    );
    validate_overworld_path_gameplay_evidence(&snapshot, &screenshot, expected);
}

fn require_level_header_gameplay_evidence(
    snes9x: &Path,
    rom: &Path,
    expected_timer: u16,
) -> Vec<u8> {
    let driver = std::env::var_os("SNES9X_GAMEPLAY_DRIVER")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .unwrap_or_else(|| {
            panic!(
                "SNES9X_GAMEPLAY_DRIVER must name the supplied deterministic libretro gameplay driver"
            )
        });
    let directory = rom.parent().expect("temporary gameplay ROM parent");
    let stem = rom
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("gameplay ROM has a UTF-8 file stem");
    let snapshot = directory.join(format!("{stem}-level-header-after.frz"));
    let screenshot = directory.join(format!("{stem}-level-header-after.png"));
    let child = Command::new(&driver)
        .arg("--emulator")
        .arg(snes9x)
        .arg("--rom")
        .arg(rom)
        .arg("--scenario")
        .arg("smw-level-header")
        .arg("--expected-timer")
        .arg(format!("{expected_timer:03X}"))
        .arg("--snapshot")
        .arg(&snapshot)
        .arg("--screenshot")
        .arg(&screenshot)
        .stdin(Stdio::null())
        .spawn()
        .expect("launch Snes9x level-header gameplay driver");
    let mut child = ChildGuard(child);
    let deadline = Instant::now() + GAMEPLAY_DRIVER_TIMEOUT;
    let status = loop {
        if let Some(status) = child
            .0
            .try_wait()
            .expect("query level-header gameplay driver")
        {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "Snes9x level-header gameplay driver exceeded {GAMEPLAY_DRIVER_TIMEOUT:?}"
        );
        thread::sleep(Duration::from_millis(100));
    };
    assert!(
        status.success(),
        "Snes9x level-header gameplay driver failed: {status}"
    );

    let snapshot = read_regular_bounded(
        &snapshot,
        MAX_GAMEPLAY_STATE_BYTES,
        "Snes9x level-header snapshot",
    );
    let screenshot = read_regular_bounded(
        &screenshot,
        MAX_GAMEPLAY_SCREENSHOT_BYTES,
        "Snes9x level-header screenshot",
    );
    let wram = decode_snes9x_wram(&snapshot).expect("decode level-header Snes9x WRAM");
    assert_eq!(wram[SMW_GAME_MODE], SMW_LEVEL_MODE);
    assert_eq!(wram[SMW_TIMER_HUNDREDS], (expected_timer >> 8) as u8);
    assert_eq!(
        wram[SMW_TIMER_HUNDREDS + 1],
        ((expected_timer >> 4) & 0x0f) as u8
    );
    assert_eq!(wram[SMW_TIMER_HUNDREDS + 2], (expected_timer & 0x0f) as u8);
    let image = decode_map16_bitmap_png_image(&screenshot).expect("decode level-header screenshot");
    assert!((256..=512).contains(&image.width));
    assert!((224..=478).contains(&image.height));
    let first = image.pixels.first().expect("nonempty gameplay screenshot");
    assert!(image.pixels.iter().any(|pixel| pixel != first));
    wram
}

fn require_title_recorder_gameplay_evidence(snes9x: &Path, rom: &Path) -> TitleScreenRecording {
    let driver = std::env::var_os("SNES9X_GAMEPLAY_DRIVER")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .unwrap_or_else(|| {
            panic!("SNES9X_GAMEPLAY_DRIVER must name the supplied deterministic libretro driver")
        });
    let directory = rom.parent().expect("temporary gameplay ROM parent");
    let snapshot = directory.join("title-recorder-after.frz");
    let screenshot = directory.join("title-recorder-after.png");
    let child = Command::new(&driver)
        .arg("--emulator")
        .arg(snes9x)
        .arg("--rom")
        .arg(rom)
        .arg("--scenario")
        .arg("smw-title-recorder")
        .arg("--snapshot")
        .arg(&snapshot)
        .arg("--screenshot")
        .arg(&screenshot)
        .stdin(Stdio::null())
        .spawn()
        .expect("launch title-recorder gameplay driver");
    let mut child = ChildGuard(child);
    let deadline = Instant::now() + GAMEPLAY_DRIVER_TIMEOUT;
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("query title-recorder driver") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "Snes9x title-recorder driver exceeded {GAMEPLAY_DRIVER_TIMEOUT:?}"
        );
        thread::sleep(Duration::from_millis(100));
    };
    assert!(status.success(), "title-recorder driver failed: {status}");
    let snapshot = read_regular_bounded(
        &snapshot,
        MAX_GAMEPLAY_STATE_BYTES,
        "title-recorder Snes9x snapshot",
    );
    let screenshot = read_regular_bounded(
        &screenshot,
        MAX_GAMEPLAY_SCREENSHOT_BYTES,
        "title-recorder Snes9x screenshot",
    );
    let wram = decode_snes9x_wram(&snapshot).expect("decode title-recorder WRAM");
    assert_eq!(wram[SMW_GAME_MODE], SMW_LEVEL_MODE);
    let mut tagged = b"#!s9xsnp:0007\nRAM:131072:".to_vec();
    tagged.extend_from_slice(&wram);
    let recording = decode_snes9x_title_recording(&tagged)
        .expect("decode emulator-captured title movement recording");
    assert!(recording.bytes().len() >= 10);
    assert_eq!(recording.bytes().last(), Some(&0xff));
    let records = &recording.bytes()[..recording.bytes().len() - 1];
    assert_eq!(records.len() % 3, 0);
    assert_eq!(
        recording.bytes(),
        &[
            0x00, 0x00, 0x00, // 256 idle frames
            0x00, 0x00, 0x00, // 256 idle frames
            0x00, 0x00, 0x58, // 88 idle frames
            0x80, 0x08, 0x01, // B transition
            0x80, 0x00, 0x0b, // B held
            0x80, 0xc0, 0x01, // A transition
            0x80, 0x80, 0x08, // A held
            0x00, 0x00, 0x07, // released
            0xff,
        ]
    );
    let image =
        decode_map16_bitmap_png_image(&screenshot).expect("decode title-recorder screenshot");
    assert!((256..=512).contains(&image.width));
    assert!((224..=478).contains(&image.height));
    let first = image
        .pixels
        .first()
        .expect("nonempty title-recorder screenshot");
    assert!(image.pixels.iter().any(|pixel| pixel != first));
    recording
}

fn synthetic_overworld_path_evidence(expected: OverworldEndpoint) -> (Vec<u8>, Vec<u8>) {
    let mut wram = vec![0; 0x2_0000];
    wram[SMW_GAME_MODE] = SMW_OVERWORLD_MODE;
    wram[SMW_MARIO_SUBMAP] = expected.submap;
    wram[SMW_MARIO_POSITION..SMW_MARIO_POSITION + 2].copy_from_slice(&expected.x.to_le_bytes());
    wram[SMW_MARIO_POSITION + 2..SMW_MARIO_POSITION + 4].copy_from_slice(&expected.y.to_le_bytes());
    wram[SMW_MARIO_GRID_POSITION..SMW_MARIO_GRID_POSITION + 2]
        .copy_from_slice(&(expected.x >> 4).to_le_bytes());
    wram[SMW_MARIO_GRID_POSITION + 2..SMW_MARIO_GRID_POSITION + 4]
        .copy_from_slice(&(expected.y >> 4).to_le_bytes());
    let mut snapshot = b"#!s9xsnp:0007\nRAM:131072:".to_vec();
    snapshot.extend_from_slice(&wram);

    let mut pixels = vec![0; 256 * 224 * 3];
    pixels[3..6].copy_from_slice(&[0xff, 0x80, 0x40]);
    let mut screenshot = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut screenshot, 256, 224);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("encode test PNG header");
        writer
            .write_image_data(&pixels)
            .expect("encode test PNG pixels");
    }
    (snapshot, screenshot)
}

#[test]
fn gameplay_evidence_requires_exact_overworld_runtime_destination_and_image() {
    let expected = OverworldEndpoint {
        x: 0x0150,
        y: 0x0058,
        submap: 2,
    };
    let (snapshot, screenshot) = synthetic_overworld_path_evidence(expected);
    validate_overworld_path_gameplay_evidence(&snapshot, &screenshot, expected);
}

#[test]
#[should_panic]
fn gameplay_evidence_rejects_a_boot_snapshot_that_did_not_reach_the_destination() {
    let actual = OverworldEndpoint {
        x: 0x0150,
        y: 0x0058,
        submap: 2,
    };
    let (snapshot, screenshot) = synthetic_overworld_path_evidence(actual);
    validate_overworld_path_gameplay_evidence(
        &snapshot,
        &screenshot,
        OverworldEndpoint {
            x: actual.x + 0x10,
            ..actual
        },
    );
}

#[test]
#[ignore = "requires an official Snes9x libretro core, the gameplay driver, and the legally supplied SMW ROM"]
fn rust_expanded_rom_reaches_level_gameplay_after_controller_input() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();

    let mut project = Project::new(
        RomImage::from_bytes(fs::read(source_rom(&root)).expect("read source SMW ROM"))
            .expect("decode source SMW ROM"),
    );
    let plan = lm_profile::smw_us_v1_expanded_settings_installation_plan_for_rom(&project.rom)
        .expect("build expanded-settings runtime installation");
    project
        .install_relocatable_patch_with_expansion_retry(
            &plan,
            lm_profile::SMW_US_V1_EXPANDED_SETTINGS_MAXIMUM_LOROM_LEN,
        )
        .expect("install expanded-settings runtime and expand generated ROM");

    let directory = SmokeDirectory::create();
    let output = directory.0.join("Rust-generated-SMW.sfc");
    fs::write(&output, project.save_snapshot()).expect("write generated ROM");

    let _ = require_level_header_gameplay_evidence(&snes9x, &output, 0x000);
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
fn native_main_overworld_layer2_paint_survives_snes9x_initialization() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let installed =
        root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive/after.smc");
    let mut app = AppState::default();
    app.load_rom(fs::read(installed).expect("read installed overworld fixture"))
        .expect("open installed overworld fixture");
    app.dispatch(AppCommand::ShowOverworld)
        .expect("enter overworld mode");
    let snapshot = app.controller_snapshot().expect("capture ROM snapshot");
    let mut controller = SmwMainOverworldLayer2Controller::decode(&snapshot)
        .expect("decode gameplay-consumed main-overworld Layer 2");
    let cells = [(12, 9), (13, 9), (12, 10), (13, 10)];
    let edits = cells
        .into_iter()
        .map(|(x, y)| OverworldControllerEdit::SetLayerTile {
            layer: OverworldLayerId::Layer2,
            x,
            y,
            tile: controller
                .layer()
                .tile(x, y)
                .expect("read original playable tile")
                ^ 1,
        })
        .collect::<Vec<_>>();
    controller
        .apply_edits(&edits)
        .expect("paint four native overworld tiles");
    let expected = controller.layer().clone();
    let prepared = controller
        .prepare_commit(
            "Snes9x native main-overworld Layer 2 paint",
            AllocationPolicy {
                search: 0x0e_0000..0x0f_0000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff, 0],
                protected: vec![
                    ProtectedRange(
                        lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD
                            ..lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD + 2,
                    ),
                    ProtectedRange(
                        lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK
                            ..lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK + 1,
                    ),
                    ProtectedRange(
                        lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD
                            ..lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD + 2,
                    ),
                    ProtectedRange(lm_profile::SMW_US_V1_CHECKSUM_FIELD..0x7fe0),
                ],
            },
        )
        .expect("prepare native overworld commit");
    app.dispatch(prepared.into_command())
        .expect("commit native overworld paint");
    let project = app.project().expect("retain edited project");
    assert_eq!(
        lm_profile::load_smw_us_v1_main_overworld_layer2(project)
            .expect("reopen gameplay-consumed main-overworld Layer 2")
            .layer,
        expected
    );
    assert_eq!(
        lm_rom::SnesChecksum::decode(
            project.rom.logical_bytes(),
            lm_profile::SMW_US_V1_CHECKSUM_FIELD,
        )
        .expect("decode edited ROM checksum"),
        lm_rom::compute_snes_checksum(
            project.rom.logical_bytes(),
            lm_profile::SMW_US_V1_CHECKSUM_FIELD,
        )
        .expect("compute edited ROM checksum")
    );

    let directory = SmokeDirectory::create();
    let output = directory
        .0
        .join("Rust-native-overworld-Layer2-painted-SMW.smc");
    fs::write(&output, project.save_snapshot()).expect("write native-overworld-edited ROM");
    require_snes9x_initialization(&snes9x, &output);
}

#[test]
#[ignore = "requires local Snes9x, a platform gameplay driver, and the supplied legally obtained SMW ROM fixture"]
fn native_overworld_path_link_edit_is_traversed_in_snes9x() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let mut app = AppState::default();
    app.load_rom(fs::read(source_rom(&root)).expect("read source SMW ROM"))
        .expect("open source SMW ROM");
    app.dispatch(AppCommand::ShowOverworld)
        .expect("enter overworld mode");
    let mut expected = app
        .project()
        .expect("retain source project")
        .load_overworld_path_links_detected(lm_profile::smw_us_v1_overworld_path_patch_locator())
        .expect("load gameplay-consumed path-link table")
        .table;
    let link = expected
        .links
        .first_mut()
        .expect("native path-link table must not be empty");
    let source = link.source;
    link.destination = OverworldEndpoint {
        x: 0x0150,
        y: 0x0058,
        submap: 2,
    };
    link.target.x_tile = 0x15;
    link.target.y_tile = 0x05;
    let destination = link.destination;
    app.dispatch(AppCommand::ReplaceNativeOverworldPathLinks {
        rev: app.project_revision(),
        table: Box::new(expected.clone()),
    })
    .expect("commit native path-link edit");
    let project = app.project().expect("retain path-edited project");
    assert_eq!(
        project
            .load_overworld_path_links_detected(
                lm_profile::smw_us_v1_overworld_path_patch_locator(),
            )
            .expect("reopen gameplay-consumed path-link table")
            .table,
        expected
    );
    assert_eq!(
        lm_rom::SnesChecksum::decode(
            project.rom.logical_bytes(),
            lm_profile::SMW_US_V1_CHECKSUM_FIELD,
        )
        .expect("decode path-edited ROM checksum"),
        lm_rom::compute_snes_checksum(
            project.rom.logical_bytes(),
            lm_profile::SMW_US_V1_CHECKSUM_FIELD,
        )
        .expect("compute path-edited ROM checksum")
    );

    let directory = SmokeDirectory::create();
    let output = directory
        .0
        .join("Rust-native-overworld-path-link-edited-SMW.sfc");
    fs::write(&output, project.save_snapshot()).expect("write native-path-edited ROM");
    require_overworld_path_gameplay_evidence(&snes9x, &output, source, destination);
}

fn smoke_overworld_layout() -> CompleteOverworldRomLayout {
    let table = |offset| LevelPointerTable {
        offset,
        entries: 1,
        stride: 3,
    };
    CompleteOverworldRomLayout {
        layers: OverworldLayersRomLayout {
            mapper: Mapper::LoRom,
            layer1: table(0x10_0000),
            layer2: table(0x10_0003),
            width: 4,
            height: 4,
        },
        event_reveals: EventRevealRomLayout {
            mapper: Mapper::LoRom,
            sources: table(0x10_0006),
            destinations: table(0x10_0009),
            entries_per_slot: 2,
        },
        endpoints: EndpointRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(0x10_000c),
            endpoints_per_slot: 2,
        },
        messages: MessageRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(0x10_000f),
            messages_per_slot: 1,
        },
        sprites: SpriteRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(0x10_0012),
            sprites_per_slot: 1,
            record_len: 9,
        },
        palette: PaletteRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(0x10_0015),
            colors_per_palette: 16,
        },
        animation: ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(0x10_0018),
            maximum_records: 32,
            maximum_encoded_len: 0x4000,
        },
    }
}

fn smoke_overworld_data() -> CompleteOverworldData {
    CompleteOverworldData {
        layers: OverworldLayers {
            layer1: OverworldLayer::new(
                4,
                4,
                vec![
                    0x0100, 0x0101, 0x0102, 0x0103, 0x0110, 0x0111, 0x0112, 0x0113, 0x0120, 0x0121,
                    0x0122, 0x0123, 0x0130, 0x0131, 0x0132, 0x0133,
                ],
            )
            .unwrap(),
            layer2: OverworldLayer::new(4, 4, vec![0x0200; 16]).unwrap(),
        },
        event_reveals: EventRevealTable {
            entries: vec![
                EventReveal {
                    source_tile: 0x10,
                    destination_tile: 0x20,
                },
                EventReveal {
                    source_tile: 0x11,
                    destination_tile: 0x21,
                },
            ],
        },
        endpoints: vec![
            OverworldEndpoint {
                x: 1,
                y: 2,
                submap: 0,
            },
            OverworldEndpoint {
                x: 3,
                y: 1,
                submap: 1,
            },
        ],
        messages: vec![OverworldMessage::decode(&[0x11; OverworldMessage::ENCODED_LEN]).unwrap()],
        sprites: vec![OverworldSprite {
            id: 7,
            x: 2,
            y: 3,
            submap: Submap::Main,
            extra: vec![0xaa, 0xbb],
        }],
        palette: Palette {
            colors: (0_u16..16).map(Bgr555).collect(),
        },
        animation: CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: Vec::new(),
        },
    }
}

fn smoke_overworld_options() -> CompleteOverworldSaveOptions {
    let allocation = AllocationPolicy {
        search: 0x10_8000..0x12_0000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x7fc0..0x8000),
            ProtectedRange(0x10_0000..0x10_001b),
        ],
    };
    CompleteOverworldSaveOptions {
        layers: OverworldSaveOptions {
            layer1_allocation: allocation.clone(),
            layer2_allocation: allocation.clone(),
            previous_layer1: None,
            previous_layer2: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        event_reveals: EventRevealSaveOptions {
            source_allocation: allocation.clone(),
            destination_allocation: allocation.clone(),
            previous_sources: None,
            previous_destinations: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        endpoints: EndpointSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        messages: MessageSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        sprites: SpriteSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        palette: PaletteSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        animation: ExAnimationSaveOptions {
            allocation,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    }
}

#[test]
#[ignore = "requires local Snes9x plus the supplied legally obtained SMW ROM fixture"]
fn rust_complete_overworld_transaction_survives_snes9x_initialization() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let mut project = Project::new(
        RomImage::from_bytes(fs::read(source_rom(&root)).expect("read source SMW ROM"))
            .expect("decode source SMW ROM"),
    );
    project
        .expand_rom(Mapper::LoRom, 0x12_0000, 0xff, 0x7fdc)
        .expect("expand complete-overworld smoke ROM");
    let layout = smoke_overworld_layout();
    let data = smoke_overworld_data();
    let modes = [false; 256];
    project
        .save_complete_overworld_with_checksum(
            0,
            &data,
            layout,
            &smoke_overworld_options(),
            &modes,
            0x7fdc,
        )
        .expect("save all nine complete-overworld payloads");
    assert_eq!(
        project
            .load_complete_overworld(0, layout, &modes)
            .expect("semantically reopen complete overworld"),
        data
    );

    let directory = SmokeDirectory::create();
    let output = directory.0.join("Rust-complete-overworld-edited-SMW.sfc");
    fs::write(&output, project.save_snapshot()).expect("write complete-overworld-edited ROM");
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
#[ignore = "requires an official Snes9x libretro core, the gameplay driver, and the legally supplied SMW ROM"]
fn rust_layer1_object_edit_reaches_level_gameplay_after_controller_input() {
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
    let _ = require_level_header_gameplay_evidence(&snes9x, &output, 0x000);
}

#[test]
#[ignore = "requires local Snes9x plus the supplied legally obtained SMW ROM fixture"]
fn rust_normal_vram_patch_survives_snes9x_initialization() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let mut project = Project::new(
        RomImage::from_bytes(fs::read(source_rom(&root)).expect("read source SMW ROM"))
            .expect("decode source SMW ROM"),
    );
    project
        .expand_rom(Mapper::LoRom, 0x10_0000, 0xff, 0x7fdc)
        .expect("expand VRAM-patched ROM");
    let plan = lm_profile::smw_us_v1_normal_vram_patch_installation_plan(project.rom.logical_len())
        .expect("build Normal VRAM patch");
    project
        .install_relocatable_patch(&plan)
        .expect("install Normal VRAM patch");

    let directory = SmokeDirectory::create();
    let output = directory.0.join("Rust-normal-VRAM-patched-SMW.sfc");
    fs::write(&output, project.save_snapshot()).expect("write Normal VRAM-patched ROM");
    require_snes9x_initialization(&snes9x, &output);
}

#[test]
#[ignore = "requires an official Snes9x libretro core, the gameplay driver, and the legally supplied SMW ROM"]
fn rust_title_recorder_captures_real_joypad_input_in_snes9x() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let source = fs::read(source_rom(&root)).expect("read source SMW ROM");
    let mut recorder = AppState::default();
    recorder.load_rom(source.clone()).expect("open source ROM");
    recorder
        .dispatch(AppCommand::InstallNativeTitleRecordingRecorder { rev: 0 })
        .expect("install title movement recorder");
    let directory = SmokeDirectory::create();
    let output = directory.0.join("Rust-title-recorder-SMW.smc");
    fs::write(&output, recorder.project().unwrap().save_snapshot())
        .expect("write title-recorder ROM");
    let recording = require_title_recorder_gameplay_evidence(&snes9x, &output);

    let mut playback = AppState::default();
    playback.load_rom(source).expect("reopen source ROM");
    playback
        .dispatch(AppCommand::ReplaceNativeTitleRecording {
            rev: 0,
            recording: recording.clone(),
        })
        .expect("install captured movement playback");
    assert_eq!(
        playback
            .project()
            .unwrap()
            .load_title_recording_detected(&lm_profile::smw_us_v1_title_recording_locator())
            .expect("reopen captured movement playback")
            .recording,
        Some(recording)
    );
}

#[test]
#[ignore = "requires an official Snes9x libretro core, the gameplay driver, and the legally supplied SMW ROM"]
fn rust_custom_time_and_support_patch_b_are_applied_in_snes9x_gameplay() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let sprite_lengths = SpriteLengthTable::standard();
    let custom_time = CustomTimeSettings::new(0x456, true).expect("construct custom time");
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
    for level_number in [0x104, 0x105] {
        let mut level = project
            .load_level_slot(level_number, layout, &sprite_lengths)
            .expect("load starting level");
        level
            .layer1
            .objects
            .set_custom_time(false, Some(custom_time))
            .expect("stage forced custom time");
        project
            .save_level_layer1_with_checksum(
                layout,
                &level,
                0x7fdc,
                &LevelSaveOptions {
                    layer1_allocation: allocation.clone(),
                    sprite_allocation: allocation.clone(),
                    previous_layer1: None,
                    previous_sprites: None,
                    reuse_identical: true,
                    erase_fill: 0xff,
                },
            )
            .expect("save custom-time starting level");
        let reopened = project
            .load_level_slot(level_number, layout, &sprite_lengths)
            .expect("reopen custom-time starting level");
        assert_eq!(
            reopened.layer1.objects.custom_time(false),
            Some(custom_time)
        );
    }
    assert_eq!(
        lm_profile::detect_smw_us_v1_support_patch_b(project.rom.logical_bytes())
            .expect("authenticate support patch B"),
        lm_profile::SmwUsV1SupportPatchBState::Installed
    );

    let directory = SmokeDirectory::create();
    let output = directory.0.join("Rust-custom-time-support-patch-B-SMW.sfc");
    fs::write(&output, project.save_snapshot()).expect("write custom-time ROM");
    let _ = require_level_header_gameplay_evidence(&snes9x, &output, custom_time.value());
}

#[test]
#[ignore = "requires an official Snes9x libretro core, the gameplay driver, and the legally supplied SMW ROM"]
fn rust_standard_time_music_and_sprite_headers_are_applied_in_snes9x_gameplay() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let sprite_lengths = SpriteLengthTable::standard();
    let mut project = Project::new(
        RomImage::from_bytes(fs::read(source_rom(&root)).expect("read source SMW ROM"))
            .expect("decode source SMW ROM"),
    );
    let allocation_start = project.rom.logical_len();
    let logical_len = 0x10_0000;
    project
        .expand_rom(Mapper::LoRom, logical_len, 0xff, 0x7fdc)
        .expect("expand standard-time ROM");
    let allocation = AllocationPolicy {
        search: allocation_start..logical_len,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x2e000..0x2e600),
            ProtectedRange(0x7fc0..0x8000),
        ],
    };
    for level_number in [0x104, 0x105] {
        let mut level = project
            .load_level_slot(level_number, layout, &sprite_lengths)
            .expect("load starting level");
        level
            .layer1
            .header
            .set_time_limit_selector(3)
            .expect("select standard 400-second timer");
        level
            .layer1
            .header
            .set_default_music_selector(7)
            .expect("select music 7");
        level
            .layer1
            .header
            .set_layer1_vertical_scroll(Layer1VerticalScrollMode::NoneVerticalOrHorizontal);
        level
            .layer1
            .objects
            .set_custom_time(false, None)
            .expect("disable custom timer");
        project
            .save_level_layer1_with_checksum(
                layout,
                &level,
                0x7fdc,
                &LevelSaveOptions {
                    layer1_allocation: allocation.clone(),
                    sprite_allocation: allocation.clone(),
                    previous_layer1: None,
                    previous_sprites: None,
                    reuse_identical: true,
                    erase_fill: 0xff,
                },
            )
            .expect("save standard-time starting level");
        let reopened = project
            .load_level_slot(level_number, layout, &sprite_lengths)
            .expect("reopen standard-time starting level");
        assert_eq!(reopened.layer1.header.time_limit_selector(), 3);
        assert_eq!(reopened.layer1.header.default_music_selector(), 7);
        assert_eq!(
            reopened.layer1.header.layer1_vertical_scroll(),
            Layer1VerticalScrollMode::NoneVerticalOrHorizontal
        );
        assert_eq!(reopened.layer1.objects.custom_time(false), None);

        let original = reopened;
        let mut replacement = original.clone();
        replacement.sprites.header = NativeSpriteHeader::from_raw(replacement.sprites.header)
            .with_properties(0x0b, false, false)
            .expect("set discriminating sprite-header properties")
            .raw();
        assert!(
            project
                .save_level_sprites_in_place_with_checksum(
                    layout,
                    &original,
                    &replacement,
                    &sprite_lengths,
                    0x7fdc,
                )
                .expect("save starting-level sprite header in place")
        );
        assert_eq!(
            project
                .load_level_slot(level_number, layout, &sprite_lengths)
                .expect("reopen starting-level sprite header")
                .sprites
                .header,
            replacement.sprites.header
        );
    }

    let directory = SmokeDirectory::create();
    let output = directory.0.join("Rust-standard-time-header-SMW.sfc");
    fs::write(&output, project.save_snapshot()).expect("write standard-time ROM");
    let wram = require_level_header_gameplay_evidence(&snes9x, &output, 0x400);
    assert_eq!(wram[SMW_CURRENT_MUSIC], 0x12);
    assert_eq!(wram[SMW_SPRITE_MEMORY], 0x0b);
    assert_eq!(wram[SMW_SPRITE_BUOYANCY], 0x00);
    assert_eq!(wram[SMW_LAYER1_VERTICAL_SCROLL_ENABLED], 0x00);
    assert_eq!(wram[SMW_LAYER1_VERTICAL_SCROLL_MODE], 0x00);
}

#[test]
#[ignore = "requires an official Snes9x libretro core, the gameplay driver, and the legally supplied SMW ROM"]
fn rust_standard_sprite_edit_reaches_level_gameplay_after_controller_input() {
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
    let _ = require_level_header_gameplay_evidence(&snes9x, &output, 0x000);
}

#[test]
#[ignore = "requires local Snes9x plus retained Lunar Magic 3.63 installed-ROM fixture"]
fn rust_expanded_sprite_transition_edit_survives_snes9x_initialization() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snes9x = require_snes9x_binary();
    let installed = root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc");
    let image = RomImage::from_bytes(fs::read(installed).expect("read installed SMW fixture"))
        .expect("decode installed SMW fixture");
    let mut project = Project::new(image.clone());
    let lengths = SpriteLengthTable::standard();
    let mut layout = lm_profile::smw_us_v1_vanilla_level_layout();
    layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image)
        .expect("detect installed sprite pointers");
    let sprite_offset = layout
        .sprites
        .read_snes_pointer(&image, 0x105)
        .expect("read installed sprite pointer")
        .to_pc(layout.mapper)
        .expect("map installed sprite pointer");
    layout.expanded_sprites = NativeSpriteStream::header_uses_expanded_framing(
        image.read(sprite_offset, 1).expect("read sprite header")[0],
    );
    let mut level = project
        .load_level_slot(0x105, layout, &lengths)
        .expect("load installed level 105");
    let first_record = level
        .sprites
        .tokens
        .iter()
        .position(|token| matches!(token, SpriteToken::Record(_)))
        .expect("level 105 must contain a sprite");
    level
        .sprites
        .tokens
        .insert(first_record, SpriteToken::Screen(2));
    level.sprites.canonicalize_framing();
    layout.expanded_sprites = true;

    project
        .relocate_level_sprites_with_checksum(
            layout,
            &level,
            &lengths,
            0x7fdc,
            &LevelSaveOptions {
                layer1_allocation: AllocationPolicy {
                    search: 0..0,
                    bank_size: None,
                    fill_bytes: vec![0xff],
                    protected: vec![ProtectedRange(0x7fc0..0x8000)],
                },
                sprite_allocation: AllocationPolicy {
                    search: 0x80_000..image.logical_len(),
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0x00, 0xff],
                    protected: vec![ProtectedRange(0x7fc0..0x8000)],
                },
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .expect("save expanded sprite transition edit");
    assert_eq!(
        project
            .load_level_slot(0x105, layout, &lengths)
            .expect("reopen expanded sprite transition edit")
            .sprites,
        level.sprites
    );

    let directory = SmokeDirectory::create();
    let output = directory
        .0
        .join("Rust-expanded-sprite-transition-edited-SMW.smc");
    fs::write(&output, project.save_snapshot()).expect("write expanded-sprite-edited ROM");
    require_snes9x_initialization(&snes9x, &output);
}
