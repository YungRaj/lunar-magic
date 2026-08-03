use crate::{ControllerSnapshot, EditorMode, PreparedRomCommit};
use lm_level::{SeparateMidwayEntrance, SeparateMidwayEntranceTable};
use lm_project::{
    Project, RelocatablePatchError, RomMutation, SeparateMidwayPatchError,
    SeparateMidwayPatchLocator, TransactionError, VanillaEntranceIoError, VanillaEntranceRomLayout,
    VanillaMainEntrance,
};
use lm_rom::{Mapper, RomError, RomImage};

#[derive(Debug)]
pub enum VanillaEntranceControllerError {
    WrongMode(EditorMode),
    MapperMismatch { snapshot: Mapper, layout: Mapper },
    Io(VanillaEntranceIoError),
    Midway(SeparateMidwayPatchError),
    MidwayInstallBuild(lm_profile::SeparateMidwayInstallBuildError),
    MidwayInstall(RelocatablePatchError),
    MidwayAlreadyInstalled,
    MidwayState,
    Rom(RomError),
    Mutation(TransactionError),
    InvalidLayer2ScrollTable { command: usize, value: u8 },
    MidwayUnavailable { command: usize },
}

impl std::fmt::Display for VanillaEntranceControllerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "vanilla entrance controller failed: {self:?}")
    }
}

impl std::error::Error for VanillaEntranceControllerError {}

#[derive(Clone, Debug)]
pub struct VanillaEntranceController {
    revision: u64,
    slot: usize,
    layout: VanillaEntranceRomLayout,
    checksum_field: usize,
    source_file_bytes: Vec<u8>,
    baseline: VanillaMainEntrance,
    entrance: VanillaMainEntrance,
    midway_locator: Option<SeparateMidwayPatchLocator>,
    midway_baseline: Option<SeparateMidwayEntranceTable>,
    midway: Option<SeparateMidwayEntranceTable>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VanillaEntranceEdit {
    SetMain(VanillaMainEntrance),
    SetLayer2ScrollTable(u8),
    SetMidway(SeparateMidwayEntrance),
}

impl VanillaEntranceController {
    /// Loads the selected pristine-SMW level's main entrance.
    ///
    /// # Errors
    ///
    /// Rejects non-level modes, mapper disagreement, malformed ROMs, and inaccessible tables.
    pub fn decode(
        snapshot: &ControllerSnapshot,
        layout: VanillaEntranceRomLayout,
    ) -> Result<Self, VanillaEntranceControllerError> {
        Self::decode_inner(snapshot, layout, None)
    }

    /// Loads the main entrance and, when installed, Lunar Magic's separate-midway table.
    ///
    /// # Errors
    ///
    /// Rejects malformed installed midway hooks or ownership instead of silently treating them as
    /// absent.
    pub fn decode_with_midway(
        snapshot: &ControllerSnapshot,
        layout: VanillaEntranceRomLayout,
        midway_locator: SeparateMidwayPatchLocator,
    ) -> Result<Self, VanillaEntranceControllerError> {
        Self::decode_inner(snapshot, layout, Some(midway_locator))
    }

    fn decode_inner(
        snapshot: &ControllerSnapshot,
        layout: VanillaEntranceRomLayout,
        midway_locator: Option<SeparateMidwayPatchLocator>,
    ) -> Result<Self, VanillaEntranceControllerError> {
        let EditorMode::Level(slot) = snapshot.mode else {
            return Err(VanillaEntranceControllerError::WrongMode(snapshot.mode));
        };
        if snapshot.identity.mapper != layout.mapper {
            return Err(VanillaEntranceControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: layout.mapper,
            });
        }
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(VanillaEntranceControllerError::Rom)?;
        let slot = usize::from(slot);
        let project = Project::new(image);
        let entrance = project
            .load_vanilla_main_entrance(slot, layout)
            .map_err(VanillaEntranceControllerError::Io)?;
        let midway = if let Some(locator) = midway_locator {
            match project.load_separate_midway_table(locator) {
                Ok(loaded) => Some(loaded),
                Err(SeparateMidwayPatchError::HookSignature)
                    if project
                        .rom
                        .read(locator.hook_offset, 4)
                        .is_ok_and(|bytes| bytes == [0x4a; 4]) =>
                {
                    None
                }
                Err(error) => return Err(VanillaEntranceControllerError::Midway(error)),
            }
        } else {
            None
        };
        Ok(Self {
            revision: snapshot.revision,
            slot,
            layout,
            checksum_field: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            baseline: entrance,
            entrance,
            midway_locator,
            midway_baseline: midway.as_ref().map(|loaded| loaded.table.clone()),
            midway: midway.map(|loaded| loaded.table),
        })
    }

    #[must_use]
    pub const fn entrance(&self) -> VanillaMainEntrance {
        self.entrance
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.entrance != self.baseline || self.midway != self.midway_baseline
    }

    pub fn set_entrance(&mut self, entrance: VanillaMainEntrance) {
        self.entrance = entrance;
    }

    #[must_use]
    pub fn midway_entrance(&self) -> Option<SeparateMidwayEntrance> {
        self.midway
            .as_ref()
            .and_then(|table| table.entries.get(self.slot))
            .copied()
    }

    pub fn set_midway_entrance(&mut self, entrance: SeparateMidwayEntrance) {
        if let Some(table) = &mut self.midway {
            table.entries[self.slot] = entrance;
        }
    }

    /// Applies an ordered entrance batch without exposing partial state on a late failure.
    ///
    /// The original Layer 2 scroll selector owns only the high nibble of the main entrance's
    /// position byte. Separate midway edits require the authenticated installed table.
    pub fn apply_edits(
        &mut self,
        edits: &[VanillaEntranceEdit],
    ) -> Result<(), VanillaEntranceControllerError> {
        let mut entrance = self.entrance;
        let mut midway = self.midway.clone();
        for (command, edit) in edits.iter().enumerate() {
            match edit {
                VanillaEntranceEdit::SetMain(value) => entrance = *value,
                VanillaEntranceEdit::SetLayer2ScrollTable(value) => {
                    if *value > 0x0f {
                        return Err(VanillaEntranceControllerError::InvalidLayer2ScrollTable {
                            command,
                            value: *value,
                        });
                    }
                    entrance.position = entrance.position & 0x0f | *value << 4;
                }
                VanillaEntranceEdit::SetMidway(value) => {
                    let table = midway
                        .as_mut()
                        .ok_or(VanillaEntranceControllerError::MidwayUnavailable { command })?;
                    table.entries[self.slot] = *value;
                }
            }
        }
        self.entrance = entrance;
        self.midway = midway;
        Ok(())
    }

    /// Prepares first-time installation of the complete Lfix3 and separate-midway runtimes.
    ///
    /// # Errors
    ///
    /// Rejects an already-installed table, malformed profile resources, incompatible source ROM,
    /// allocation/fixup failures, or mutation construction failures.
    pub fn prepare_midway_install(
        &self,
        entrance: SeparateMidwayEntrance,
    ) -> Result<PreparedRomCommit, VanillaEntranceControllerError> {
        if self.midway.is_some() {
            return Err(VanillaEntranceControllerError::MidwayAlreadyInstalled);
        }
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(VanillaEntranceControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let mut table = SeparateMidwayEntranceTable {
            entries: vec![
                SeparateMidwayEntrance::default();
                SeparateMidwayEntranceTable::ENTRY_COUNT
            ],
        };
        table.entries[self.slot] = entrance;
        let plan = lm_profile::smw_us_v1_separate_midway_installation_plan(
            &lm_profile::smw_us_v1_lfix3_runtime_template(),
            self.slot,
            &table,
        )
        .map_err(VanillaEntranceControllerError::MidwayInstallBuild)?;
        let mut project = Project::new(image);
        project
            .install_relocatable_patch(&plan)
            .map_err(VanillaEntranceControllerError::MidwayInstall)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(VanillaEntranceControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description: format!("Install level {:03X} separate midway entrance", self.slot),
            mutation,
        })
    }

    /// Prepares one revision-bound checksum-inclusive entrance mutation.
    ///
    /// # Errors
    ///
    /// Rejects invalid source ROMs, table I/O failures, or mutation construction failures.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
    ) -> Result<PreparedRomCommit, VanillaEntranceControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(VanillaEntranceControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let mut project = Project::new(image);
        if self.is_modified() {
            if self.entrance != self.baseline {
                project
                    .save_vanilla_main_entrance(
                        self.slot,
                        self.entrance,
                        self.layout,
                        self.checksum_field,
                    )
                    .map_err(VanillaEntranceControllerError::Io)?;
            }
            if self.midway != self.midway_baseline {
                let table = self
                    .midway
                    .as_ref()
                    .ok_or(VanillaEntranceControllerError::MidwayState)?;
                let locator = self
                    .midway_locator
                    .ok_or(VanillaEntranceControllerError::MidwayState)?;
                project
                    .save_separate_midway_table(table, locator, self.checksum_field)
                    .map_err(VanillaEntranceControllerError::Midway)?;
            }
        }
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(VanillaEntranceControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description: description.into(),
            mutation,
        })
    }
}
