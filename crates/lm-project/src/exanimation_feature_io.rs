use crate::{Project, RomWrite, TransactionError};
use lm_graphics::ExAnimationFeatureOptions;
use lm_rom::{RomError, compute_snes_checksum};
use std::fmt;

pub const EXANIMATION_FEATURE_LEVEL_COUNT: usize = 0x200;
pub const LEGACY_SPECIAL_LEVEL: usize = 0x110;
pub const LEGACY_SPECIAL_LEVEL_FEATURE_BYTE: u8 = 0x30;

/// Fixed storage reserved by Lunar Magic's expanded `ExAnimation` runtime.
///
/// The byte immediately before `table_offset` selects the representation. A zero byte enables the
/// complete 512-byte table; any nonzero value is the legacy sentinel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExAnimationFeatureRomLayout {
    pub table_offset: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExAnimationFeatureStorage {
    LegacySentinel(u8),
    ExpandedTable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadedExAnimationFeatures {
    pub options: ExAnimationFeatureOptions,
    pub storage: ExAnimationFeatureStorage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExAnimationFeatureWritePlan {
    pub writes: Vec<RomWrite>,
    pub requires_runtime_installation: bool,
}

#[derive(Debug)]
pub enum ExAnimationFeatureIoError {
    InvalidTableOffset,
    LevelOutOfRange(usize),
    OffsetOverflow,
    ChecksumOverlap,
    RuntimeInstallationRequired,
    Rom(RomError),
    Transaction(TransactionError),
}

impl fmt::Display for ExAnimationFeatureIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ExAnimation feature I/O failed: {self:?}")
    }
}

impl std::error::Error for ExAnimationFeatureIoError {}

impl From<RomError> for ExAnimationFeatureIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for ExAnimationFeatureIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads one level's four Super GFX animation switches.
    ///
    /// Lunar Magic treats every level as byte zero while the nonzero legacy sentinel is present.
    ///
    /// # Errors
    ///
    /// Rejects an invalid layout, level, arithmetic overflow, or truncated ROM.
    pub fn load_exanimation_features(
        &self,
        level: usize,
        layout: ExAnimationFeatureRomLayout,
    ) -> Result<LoadedExAnimationFeatures, ExAnimationFeatureIoError> {
        let marker_offset = validate(self, level, layout)?;
        let marker = self.rom.read(marker_offset, 1)?[0];
        let (packed, storage) = if marker == 0 {
            (
                self.rom.read(level_offset(level, layout)?, 1)?[0],
                ExAnimationFeatureStorage::ExpandedTable,
            )
        } else {
            (0, ExAnimationFeatureStorage::LegacySentinel(marker))
        };
        Ok(LoadedExAnimationFeatures {
            options: ExAnimationFeatureOptions::decode(packed),
            storage,
        })
    }

    /// Builds Lunar Magic's exact legacy-to-table conversion or one-byte table update.
    ///
    /// A nonzero target byte also requires the feature-control runtime. The returned plan reports
    /// that requirement separately so callers cannot mistake data persistence for an installed
    /// executable patch.
    ///
    /// # Errors
    ///
    /// Rejects an invalid layout, level, arithmetic overflow, or truncated ROM.
    pub fn plan_exanimation_feature_write(
        &self,
        level: usize,
        options: ExAnimationFeatureOptions,
        layout: ExAnimationFeatureRomLayout,
    ) -> Result<ExAnimationFeatureWritePlan, ExAnimationFeatureIoError> {
        let marker_offset = validate(self, level, layout)?;
        let marker = self.rom.read(marker_offset, 1)?[0];
        let packed = options.encode();
        let writes = if marker == 0 {
            vec![RomWrite {
                offset: level_offset(level, layout)?,
                bytes: vec![packed],
            }]
        } else {
            let mut converted = vec![0; EXANIMATION_FEATURE_LEVEL_COUNT + 1];
            converted[level + 1] = packed;
            // This assignment occurs after the selected-level assignment in Lunar Magic.
            converted[LEGACY_SPECIAL_LEVEL + 1] = LEGACY_SPECIAL_LEVEL_FEATURE_BYTE;
            vec![RomWrite {
                offset: marker_offset,
                bytes: converted,
            }]
        };
        Ok(ExAnimationFeatureWritePlan {
            writes,
            requires_runtime_installation: packed != 0,
        })
    }

    /// Saves one level's switches and checksum as one undoable operation.
    ///
    /// `runtime_installed` must be true for a nonzero feature byte. Installing Lunar Magic's
    /// feature-control runtime is a separate patch transaction.
    ///
    /// # Errors
    ///
    /// Rejects unsafe storage, a missing required runtime, checksum overlap, or failed writes.
    pub fn save_exanimation_features(
        &mut self,
        level: usize,
        options: ExAnimationFeatureOptions,
        layout: ExAnimationFeatureRomLayout,
        runtime_installed: bool,
        checksum_field: usize,
    ) -> Result<bool, ExAnimationFeatureIoError> {
        let mut plan = self.plan_exanimation_feature_write(level, options, layout)?;
        if plan.requires_runtime_installation && !runtime_installed {
            return Err(ExAnimationFeatureIoError::RuntimeInstallationRequired);
        }
        let checksum_end = checksum_field
            .checked_add(4)
            .ok_or(ExAnimationFeatureIoError::OffsetOverflow)?;
        if plan.writes.iter().any(|write| {
            write
                .offset
                .checked_add(write.bytes.len())
                .is_none_or(|end| write.offset < checksum_end && checksum_field < end)
        }) {
            return Err(ExAnimationFeatureIoError::ChecksumOverlap);
        }
        let mut staged = self.rom.clone();
        for write in &plan.writes {
            staged.write(write.offset, &write.bytes)?;
        }
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        plan.writes.push(RomWrite {
            offset: checksum_field,
            bytes: checksum.encoded().to_vec(),
        });
        Ok(self.apply_writes(
            format!("save ExAnimation features {level:03x}"),
            &plan.writes,
        )?)
    }
}

fn validate(
    project: &Project,
    level: usize,
    layout: ExAnimationFeatureRomLayout,
) -> Result<usize, ExAnimationFeatureIoError> {
    if layout.table_offset == 0 {
        return Err(ExAnimationFeatureIoError::InvalidTableOffset);
    }
    if level >= EXANIMATION_FEATURE_LEVEL_COUNT {
        return Err(ExAnimationFeatureIoError::LevelOutOfRange(level));
    }
    let marker_offset = layout.table_offset - 1;
    project
        .rom
        .read(marker_offset, EXANIMATION_FEATURE_LEVEL_COUNT + 1)?;
    Ok(marker_offset)
}

fn level_offset(
    level: usize,
    layout: ExAnimationFeatureRomLayout,
) -> Result<usize, ExAnimationFeatureIoError> {
    layout
        .table_offset
        .checked_add(level)
        .ok_or(ExAnimationFeatureIoError::OffsetOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::ExAnimationFeature;
    use lm_rom::RomImage;

    const LAYOUT: ExAnimationFeatureRomLayout = ExAnimationFeatureRomLayout {
        table_offset: 0x401,
    };
    const CHECKSUM: usize = 0x7fdc;

    fn project(marker: u8) -> Project {
        let mut bytes = vec![0xff; 0x8000];
        bytes[LAYOUT.table_offset - 1] = marker;
        Project::new(RomImage::from_bytes(bytes).unwrap())
    }

    fn nonzero_options() -> ExAnimationFeatureOptions {
        let mut options = ExAnimationFeatureOptions::decode(0);
        options.set_enabled(ExAnimationFeature::LevelExAnimation, false);
        options
    }

    #[test]
    fn legacy_sentinel_loads_all_features_enabled() {
        let loaded = project(0x30)
            .load_exanimation_features(0x105, LAYOUT)
            .unwrap();
        assert_eq!(
            loaded.storage,
            ExAnimationFeatureStorage::LegacySentinel(0x30)
        );
        assert_eq!(loaded.options.encode(), 0);
    }

    #[test]
    fn expanded_table_loads_the_indexed_byte() {
        let mut project = project(0);
        project
            .rom
            .write(LAYOUT.table_offset + 0x105, &[0xa5])
            .unwrap();
        let loaded = project.load_exanimation_features(0x105, LAYOUT).unwrap();
        assert_eq!(loaded.storage, ExAnimationFeatureStorage::ExpandedTable);
        assert_eq!(loaded.options.encode(), 0xa5);
    }

    #[test]
    fn legacy_conversion_matches_lunar_magic_special_entry_and_order() {
        let plan = project(0x30)
            .plan_exanimation_feature_write(0x105, nonzero_options(), LAYOUT)
            .unwrap();
        assert!(plan.requires_runtime_installation);
        assert_eq!(plan.writes.len(), 1);
        assert_eq!(plan.writes[0].offset, LAYOUT.table_offset - 1);
        assert_eq!(plan.writes[0].bytes.len(), 0x201);
        assert_eq!(plan.writes[0].bytes[0], 0);
        assert_eq!(plan.writes[0].bytes[0x106], 0x10);
        assert_eq!(plan.writes[0].bytes[0x111], 0x30);

        let special = project(0x30)
            .plan_exanimation_feature_write(LEGACY_SPECIAL_LEVEL, nonzero_options(), LAYOUT)
            .unwrap();
        assert_eq!(special.writes[0].bytes[0x111], 0x30);
    }

    #[test]
    fn expanded_update_is_one_byte_and_save_is_undoable() {
        let mut project = project(0);
        let original = project.save_snapshot();
        project
            .save_exanimation_features(0x105, nonzero_options(), LAYOUT, true, CHECKSUM)
            .unwrap();
        assert_eq!(
            project.rom.read(LAYOUT.table_offset + 0x105, 1).unwrap(),
            &[0x10]
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn missing_runtime_and_invalid_inputs_do_not_mutate() {
        let mut project = project(0x30);
        let original = project.save_snapshot();
        assert!(matches!(
            project.save_exanimation_features(0x105, nonzero_options(), LAYOUT, false, CHECKSUM),
            Err(ExAnimationFeatureIoError::RuntimeInstallationRequired)
        ));
        assert!(matches!(
            project.load_exanimation_features(0x200, LAYOUT),
            Err(ExAnimationFeatureIoError::LevelOutOfRange(0x200))
        ));
        assert_eq!(project.save_snapshot(), original);
    }
}
