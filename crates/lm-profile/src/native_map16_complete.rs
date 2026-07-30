//! Atomic complete Lunar Magic Map16 load/save for SMW US revision 0.

use crate::{
    LoadedSmwUsV1PrimaryMap16, LoadedSmwUsV1SecondaryMap16,
    SMW_US_V1_PRIMARY_MAP16_ACTS_LIKE_WORDS, SMW_US_V1_PRIMARY_MAP16_DEFINITION_WORDS,
    SMW_US_V1_PRIMARY_MAP16_RUNTIME_MARKER_OFFSET, SMW_US_V1_SECONDARY_MAP16_DEFINITION_WORDS,
    SmwUsV1Map16RuntimeInstallBuildError, SmwUsV1PrimaryMap16Error, SmwUsV1PrimaryMap16SaveOptions,
    SmwUsV1SecondaryMap16Error, SmwUsV1SecondaryMap16SaveOptions, load_smw_us_v1_primary_map16,
    load_smw_us_v1_secondary_map16, save_smw_us_v1_primary_map16, save_smw_us_v1_secondary_map16,
    smw_us_v1_map16_runtime_installation_plan,
};
use lm_project::{Project, RelocatablePatchError, TransactionError};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSmwUsV1CompleteMap16 {
    pub foreground: LoadedSmwUsV1PrimaryMap16,
    pub background: LoadedSmwUsV1SecondaryMap16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1CompleteMap16SaveOptions {
    pub allocation: AllocationPolicy,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedSmwUsV1CompleteMap16 {
    pub installed_runtime: bool,
    pub changed: bool,
}

#[derive(Debug)]
pub enum SmwUsV1CompleteMap16Error {
    Foreground(SmwUsV1PrimaryMap16Error),
    Background(SmwUsV1SecondaryMap16Error),
    RuntimeBuild(SmwUsV1Map16RuntimeInstallBuildError),
    RuntimeInstall(RelocatablePatchError),
    ForegroundWordCount(usize),
    BackgroundWordCount(usize),
    ActsLikeWordCount(usize),
    Transaction(TransactionError),
}

impl fmt::Display for SmwUsV1CompleteMap16Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot access complete SMW US Map16 data: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1CompleteMap16Error {}

impl From<SmwUsV1PrimaryMap16Error> for SmwUsV1CompleteMap16Error {
    fn from(value: SmwUsV1PrimaryMap16Error) -> Self {
        Self::Foreground(value)
    }
}

impl From<SmwUsV1SecondaryMap16Error> for SmwUsV1CompleteMap16Error {
    fn from(value: SmwUsV1SecondaryMap16Error) -> Self {
        Self::Background(value)
    }
}

impl From<SmwUsV1Map16RuntimeInstallBuildError> for SmwUsV1CompleteMap16Error {
    fn from(value: SmwUsV1Map16RuntimeInstallBuildError) -> Self {
        Self::RuntimeBuild(value)
    }
}

impl From<RelocatablePatchError> for SmwUsV1CompleteMap16Error {
    fn from(value: RelocatablePatchError) -> Self {
        Self::RuntimeInstall(value)
    }
}

impl From<TransactionError> for SmwUsV1CompleteMap16Error {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

/// Loads all foreground definitions, foreground Acts-Like words, and background definitions.
///
/// # Errors
///
/// Returns the focused primary or secondary storage error without mutating the project.
pub fn load_smw_us_v1_complete_map16(
    project: &Project,
) -> Result<LoadedSmwUsV1CompleteMap16, SmwUsV1CompleteMap16Error> {
    Ok(LoadedSmwUsV1CompleteMap16 {
        foreground: load_smw_us_v1_primary_map16(project)?,
        background: load_smw_us_v1_secondary_map16(project)?,
    })
}

/// Saves one complete `.map16` semantic core as a single atomic project operation.
///
/// All revision-specific installers and block saves run against a cloned staging project. The
/// resulting complete logical image is compare-published to the real project only after every
/// allocation, pointer rewrite, reopen-compatible payload, and checksum operation succeeds.
///
/// # Errors
///
/// Rejects malformed shapes, unsupported non-installed source images, allocation or pointer
/// failures, and final mapper replacement failures without changing the caller's project.
pub fn save_smw_us_v1_complete_map16(
    project: &mut Project,
    foreground: &[u16],
    background: &[u16],
    acts_like: &[u16],
    checksum_field: usize,
    options: &SmwUsV1CompleteMap16SaveOptions,
) -> Result<SavedSmwUsV1CompleteMap16, SmwUsV1CompleteMap16Error> {
    if foreground.len() != SMW_US_V1_PRIMARY_MAP16_DEFINITION_WORDS {
        return Err(SmwUsV1CompleteMap16Error::ForegroundWordCount(
            foreground.len(),
        ));
    }
    if background.len() != SMW_US_V1_SECONDARY_MAP16_DEFINITION_WORDS {
        return Err(SmwUsV1CompleteMap16Error::BackgroundWordCount(
            background.len(),
        ));
    }
    if acts_like.len() != SMW_US_V1_PRIMARY_MAP16_ACTS_LIKE_WORDS {
        return Err(SmwUsV1CompleteMap16Error::ActsLikeWordCount(
            acts_like.len(),
        ));
    }

    let mut staged = project.clone();
    let installed_runtime = staged
        .rom
        .logical_bytes()
        .get(SMW_US_V1_PRIMARY_MAP16_RUNTIME_MARKER_OFFSET)
        .copied()
        != Some(0x22);
    if installed_runtime {
        let plan = smw_us_v1_map16_runtime_installation_plan(
            staged.rom.logical_bytes(),
            options.allocation.clone(),
            checksum_field,
        )?;
        staged.install_relocatable_patch(&plan)?;
    }
    let primary_options = SmwUsV1PrimaryMap16SaveOptions {
        allocation: options.allocation.clone(),
        reuse_identical: options.reuse_identical,
        erase_fill: options.erase_fill,
    };
    save_smw_us_v1_primary_map16(
        &mut staged,
        foreground,
        acts_like,
        checksum_field,
        &primary_options,
    )?;
    let secondary_options = SmwUsV1SecondaryMap16SaveOptions {
        allocation: options.allocation.clone(),
        reuse_identical: options.reuse_identical,
        erase_fill: options.erase_fill,
    };
    save_smw_us_v1_secondary_map16(&mut staged, background, checksum_field, &secondary_options)?;
    let changed = project.apply_logical_replacement(
        "save complete Lunar Magic Map16 data",
        Mapper::LoRom,
        staged.rom.logical_bytes(),
    )?;
    Ok(SavedSmwUsV1CompleteMap16 {
        installed_runtime,
        changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::ProtectedRange;
    use lm_rom::{RomImage, compute_snes_checksum};

    fn options() -> SmwUsV1CompleteMap16SaveOptions {
        SmwUsV1CompleteMap16SaveOptions {
            allocation: AllocationPolicy {
                search: 0x80_000..0x10_0000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0, 0xff],
                protected: vec![ProtectedRange(0x7fdc..0x7fe0)],
            },
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    #[test]
    fn pristine_complete_save_installs_and_publishes_one_undo_step() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project = Project::new(RomImage::from_bytes(original.clone()).unwrap());
        let loaded = load_smw_us_v1_complete_map16(&project).unwrap();
        let mut foreground = loaded.foreground.definitions;
        let mut background = loaded.background.definitions;
        let acts_like = loaded.foreground.acts_like;
        foreground[0x800 * 4..0x800 * 4 + 4].copy_from_slice(&[1, 2, 3, 4]);
        background[0x200 * 4..0x200 * 4 + 4].copy_from_slice(&[5, 6, 7, 8]);

        let saved = save_smw_us_v1_complete_map16(
            &mut project,
            &foreground,
            &background,
            &acts_like,
            0x7fdc,
            &options(),
        )
        .unwrap();

        assert!(saved.installed_runtime);
        assert!(saved.changed);
        let reopened = load_smw_us_v1_complete_map16(&project).unwrap();
        assert_eq!(reopened.foreground.definitions, foreground);
        assert_eq!(reopened.background.definitions, background);
        assert_eq!(reopened.foreground.acts_like, acts_like);
        let checksum = compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap();
        assert_eq!(
            &project.rom.logical_bytes()[0x7fdc..0x7fe0],
            checksum.encoded()
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.logical_bytes(), original);
        assert!(!project.undo().unwrap());
    }
}
