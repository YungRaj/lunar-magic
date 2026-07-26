//! Pristine installation plan for Lunar Magic-compatible separate midway entrances.

use crate::{Lfix3RuntimeLengthError, smw_us_v1_lfix3_installation_plan};
use lm_level::{SeparateMidwayEntranceTable, SeparateMidwayEntranceTableError};
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};

const HELPER_HEX: &str = concat!(
    "4a4a4a4ac21148a60ebf188710a8291003018301988920f06429084a4a4a8595",
    "9829c78d2a19bf188910a829f08596980a0a0a0a8594bf00fc0629808504bf00",
    "fe06293f8dcd13bf188d10297f0404293f8597a900ebbf188b1085028920d01f",
    "a82903aabf0cd705852098290c4a4aaabf08d705851c9829c00ccd1338686bad",
    "1a1418d0f89c2a19840ea5022901850ffafa5cb7d805ffffffffffff4c4d1001",
    "2c2a19501a48ad1a14f013b900f422408610a40e9008fafa85015ca1d9056829",
    "384a4a6bffffffffffffffff4c4d1001",
);

#[derive(Debug)]
pub enum SeparateMidwayInstallBuildError {
    Lfix3(Lfix3RuntimeLengthError),
    Table(SeparateMidwayEntranceTableError),
    LevelOutOfRange(usize),
    MissingLfixFlagsWrite,
}

impl std::fmt::Display for SeparateMidwayInstallBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "separate-midway installation construction failed: {self:?}"
        )
    }
}

impl std::error::Error for SeparateMidwayInstallBuildError {}

impl From<Lfix3RuntimeLengthError> for SeparateMidwayInstallBuildError {
    fn from(value: Lfix3RuntimeLengthError) -> Self {
        Self::Lfix3(value)
    }
}

impl From<SeparateMidwayEntranceTableError> for SeparateMidwayInstallBuildError {
    fn from(value: SeparateMidwayEntranceTableError) -> Self {
        Self::Table(value)
    }
}

/// Builds the complete pristine Lfix3 plus separate-midway installation.
///
/// The selected level's main-entrance flags gain bit `$20`, which is Lunar Magic's enable bit for
/// the corresponding four-plane midway record.
///
/// # Errors
///
/// Rejects malformed Lfix3/template data, invalid table shapes, or levels above `$1FF`.
pub fn smw_us_v1_separate_midway_installation_plan(
    lfix3_template: &[u8],
    level: usize,
    table: &SeparateMidwayEntranceTable,
) -> Result<RelocatablePatchPlan, SeparateMidwayInstallBuildError> {
    if level >= SeparateMidwayEntranceTable::ENTRY_COUNT {
        return Err(SeparateMidwayInstallBuildError::LevelOutOfRange(level));
    }
    let mut plan = smw_us_v1_lfix3_installation_plan(lfix3_template)?;
    plan.description = "install SMW US v1 separate midway entrances".into();
    let helper_target = plan.payloads.len();
    let table_target = helper_target + 1;
    let mut helper = decode_hex(HELPER_HEX);
    for range in [0x0a..0x0d, 0x27..0x2a, 0x48..0x4b, 0x57..0x5a, 0xaf..0xb2] {
        helper[range].fill(0);
    }
    plan.payloads.push(PatchPayload {
        bytes: helper,
        fixups: vec![
            table_fixup(0x0a, table_target, 0),
            table_fixup(0x27, table_target, 0x200),
            table_fixup(0x57, table_target, 0x400),
            table_fixup(0x48, table_target, 0x600),
            table_fixup(0xaf, helper_target, 0),
        ],
    });
    plan.payloads.push(PatchPayload {
        bytes: table.encode()?,
        fixups: Vec::new(),
    });
    plan.writes.push(PatchWrite {
        offset: 0x2d9e3,
        expected: vec![0x4a; 4],
        replacement: vec![0x22, 0, 0, 0],
        fixups: vec![PatchFixup {
            offset: 1,
            target_payload: helper_target,
            target_addend: 0,
            encoding: PatchFixupEncoding::Long24LowBank,
        }],
    });
    let flags = plan
        .writes
        .iter_mut()
        .find(|write| write.offset == 0x2de00)
        .ok_or(SeparateMidwayInstallBuildError::MissingLfixFlagsWrite)?;
    flags.replacement[level] |= 0x20;
    Ok(plan)
}

fn table_fixup(offset: usize, target_payload: usize, target_addend: usize) -> PatchFixup {
    PatchFixup {
        offset,
        target_payload,
        target_addend,
        encoding: PatchFixupEncoding::Long24LowBank,
    }
}

fn decode_hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII"), 16)
                .expect("embedded hexadecimal")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{smw_us_v1_lfix3_runtime_template, smw_us_v1_separate_midway_locator};
    use lm_level::SeparateMidwayEntrance;
    use lm_project::Project;
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn pristine_install_reopens_owned_table_and_undoes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let mut table = SeparateMidwayEntranceTable {
            entries: vec![SeparateMidwayEntrance::default(); 0x200],
        };
        table.entries[0x105] = SeparateMidwayEntrance {
            flags: 0xa5,
            position: 0x6c,
            additional_flags: 0x87,
            high_position: 0x21,
        };
        project
            .install_relocatable_patch(
                &smw_us_v1_separate_midway_installation_plan(
                    &smw_us_v1_lfix3_runtime_template(),
                    0x105,
                    &table,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            project
                .load_separate_midway_table(smw_us_v1_separate_midway_locator())
                .unwrap()
                .table,
            table
        );
        assert_eq!(project.rom.logical_bytes()[0x2de00 + 0x105] & 0x20, 0x20);
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }
}
