use lm_graphics::{ExAnimationSlotOptionTable, ExAnimationSlotOptions};
use lm_oracle::Observation;
use lm_project::{ExAnimationSlotOptionRomLayout, ExAnimationSlotOptionSaveOptions, Project};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage};
use std::{fs, process::Command};

const POINTER: usize = 0x20;

#[test]
fn built_cli_observes_transactionally_written_slot_options() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let table = ExAnimationSlotOptionTable {
        slots: std::array::from_fn(|slot| ExAnimationSlotOptions {
            preserved_low_nibble: u8::try_from(slot).unwrap(),
            enabled: [slot == 0, slot == 1, slot == 2, slot == 3],
        }),
    };
    project
        .save_exanimation_slot_options(
            &table,
            ExAnimationSlotOptionRomLayout {
                mapper: Mapper::LoRom,
                pointer_offset: POINTER,
            },
            &ExAnimationSlotOptionSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x100..0x8000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![ProtectedRange(POINTER..POINTER + 3)],
                },
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();

    let directory = std::env::temp_dir().join(format!(
        "lm-exanimation-slot-options-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let rom = directory.join("Options.smc");
    let output = directory.join("Options.obs");
    fs::write(&rom, project.save_snapshot()).unwrap();
    let process = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("exanimation-slot-options")
        .arg(&rom)
        .arg("lorom")
        .arg(format!("{POINTER:x}"))
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        process.status.success(),
        "{}",
        String::from_utf8_lossy(&process.stderr)
    );
    let observed = Observation::from_text(&fs::read_to_string(output).unwrap()).unwrap();
    assert_eq!(
        observed.get("exanimation/slot-options/0/bit4-enabled"),
        Some("true")
    );
    assert_eq!(
        observed.get("exanimation/slot-options/1/bit4-enabled"),
        Some("false")
    );
    assert_eq!(
        observed.get("exanimation/slot-options/6/preserved-low-nibble"),
        Some("6")
    );
    fs::remove_dir_all(directory).unwrap();
}
