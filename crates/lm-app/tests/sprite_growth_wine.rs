use lm_app::{AppState, Command, LevelController, NativeLevelEdit};
use lm_level::{MwlFile, MwlSectionKind, NativeSpriteStream, SpriteLengthTable};
use lm_project::{LevelSaveOptions, SpritePointerTable};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, SnesPointer24, detect_identity};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn wine_path(path: &Path) -> String {
    let rendered = path.display().to_string().replace('/', r"\");
    format!(r"Z:\{}", rendered.trim_start_matches('\\'))
}

fn shared_sprite_bank(image: &RomImage, bank_offset: usize) -> std::ops::Range<usize> {
    let bank = image.logical_bytes()[bank_offset];
    let first = SnesPointer24::new((u32::from(bank) << 16) | 0x8000)
        .unwrap()
        .to_pc(Mapper::LoRom)
        .unwrap();
    first..first + 0x8000
}

/// Proves that Lunar Magic 3.63 accepts and semantically preserves a sprite stream grown and
/// relocated by the Rust editor.
///
/// This remains ignored for ordinary cross-platform test runs because it requires Wine, the
/// legally supplied Lunar Magic 3.63 executable, and the supplied clean SMW ROM.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn rust_sprite_growth_reopens_with_the_inserted_sprite_in_lunar_magic() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = root.join("Super Mario World (USA).sfc");
    assert!(lunar_magic.is_file(), "missing {}", lunar_magic.display());
    assert!(original_rom.is_file(), "missing {}", original_rom.display());

    let directory = std::env::temp_dir().join(format!(
        "lm-sprite-growth-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let edited = directory.join("rust sprite growth.sfc");
    let exported = directory.join("Lunar Magic level 105.mwl");

    let mut app = AppState::default();
    app.load_rom(fs::read(&original_rom).unwrap()).unwrap();
    app.dispatch(Command::SelectLevel(0x105)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let lengths = SpriteLengthTable::standard();
    let mut controller = LevelController::decode(&snapshot, layout, &lengths).unwrap();
    let inserted = controller.level().sprites.tokens[0].clone();
    controller
        .apply_edits(&[NativeLevelEdit::InsertSprite {
            index: 1,
            token: inserted,
        }])
        .unwrap();
    let expected = controller.level().sprites.clone();

    let image = RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap();
    let SpritePointerTable::SplitSharedBank { bank_offset, .. } = layout.sprites else {
        panic!("pristine SMW sprite layout must use a shared bank");
    };
    let protected = vec![ProtectedRange(
        snapshot.identity.internal_header_offset..snapshot.identity.internal_header_offset + 0x40,
    )];
    let allocation = AllocationPolicy {
        search: image.logical_len().min(0x80_000)..image.logical_len(),
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: protected.clone(),
    };
    let prepared = controller
        .prepare_commit_with_shared_bank_sprite_relocation(
            "Insert sprite for Lunar Magic compatibility oracle",
            &LevelSaveOptions {
                layer1_allocation: allocation,
                sprite_allocation: AllocationPolicy {
                    search: shared_sprite_bank(&image, bank_offset),
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected,
                },
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    fs::write(&edited, app.project().unwrap().save_snapshot()).unwrap();

    let reopen = ProcessCommand::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&lunar_magic)
        .arg("-ExportLevel")
        .arg(wine_path(&edited))
        .arg(wine_path(&exported))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        reopen.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&reopen.stdout),
        String::from_utf8_lossy(&reopen.stderr)
    );
    assert!(
        String::from_utf8_lossy(&reopen.stdout).contains("Level 105 exported"),
        "{}",
        String::from_utf8_lossy(&reopen.stdout)
    );

    let mwl_bytes = fs::read(&exported).unwrap();
    let mwl = MwlFile::decode(&mwl_bytes).unwrap();
    let exported_sprites = mwl.payload_section(MwlSectionKind::Sprites).unwrap();
    assert_eq!(
        NativeSpriteStream::parse(&exported_sprites.payload, false, &lengths).unwrap(),
        expected
    );
    assert_eq!(mwl.version, 0x0363);
    assert_eq!(mwl.encode().unwrap(), mwl_bytes);
    let reopened_rom = RomImage::from_bytes(fs::read(&edited).unwrap()).unwrap();
    assert!(detect_identity(&reopened_rom).unwrap().checksum_matches());
    fs::remove_dir_all(directory).unwrap();
}
