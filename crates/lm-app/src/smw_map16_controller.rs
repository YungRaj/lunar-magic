use crate::{ControllerSnapshot, EditorMode, Map16ControllerEdit, PreparedRomCommit};
use lm_level::{Map16EditError, Map16Page, Map16Set, Map16Tile};
use lm_profile::{
    SmwUsV1TransferredMap16Error, SmwUsV1TransferredMap16SaveOptions,
    load_smw_us_v1_transferred_map16, save_smw_us_v1_transferred_map16,
};
use lm_project::{Project, RomMutation, TransactionError};
use lm_rom::{Mapper, RomError, RomImage};
use std::{collections::BTreeSet, fmt};

#[derive(Debug)]
pub enum SmwMap16ControllerError {
    WrongMode(EditorMode),
    Mapper(Mapper),
    Rom(RomError),
    Native(SmwUsV1TransferredMap16Error),
    InsufficientActsLike {
        actual: usize,
        needed: usize,
    },
    Edit {
        command: usize,
        error: Map16EditError,
    },
    SemanticReopen,
    Mutation(TransactionError),
}

impl fmt::Display for SmwMap16ControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native SMW Map16 controller failed: {self:?}")
    }
}

impl std::error::Error for SmwMap16ControllerError {}

/// The complete graphical SMW Map16 definition domain plus its native Acts-Like tail.
#[derive(Clone, Debug)]
pub struct SmwMap16Controller {
    revision: u64,
    checksum_field_offset: usize,
    source_file_bytes: Vec<u8>,
    baseline: Map16Set,
    set: Map16Set,
    acts_like_tail: Vec<u16>,
}

impl SmwMap16Controller {
    /// Decodes pristine or Lunar Magic-transferred SMW-US Map16 tables.
    ///
    /// # Errors
    ///
    /// Rejects the wrong editor mode or mapper and malformed native tables.
    pub fn decode(snapshot: &ControllerSnapshot) -> Result<Self, SmwMap16ControllerError> {
        if snapshot.mode != EditorMode::Map16 {
            return Err(SmwMap16ControllerError::WrongMode(snapshot.mode));
        }
        if snapshot.identity.mapper != Mapper::LoRom {
            return Err(SmwMap16ControllerError::Mapper(snapshot.identity.mapper));
        }
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(SmwMap16ControllerError::Rom)?;
        let loaded = load_smw_us_v1_transferred_map16(&Project::new(image))
            .map_err(SmwMap16ControllerError::Native)?;
        let tile_count = loaded.definitions.len() / 4;
        if loaded.acts_like.len() < tile_count {
            return Err(SmwMap16ControllerError::InsufficientActsLike {
                actual: loaded.acts_like.len(),
                needed: tile_count,
            });
        }
        let pages = loaded
            .definitions
            .chunks_exact(4)
            .zip(&loaded.acts_like[..tile_count])
            .map(|(words, &acts_like)| Map16Tile {
                top_left: lm_level::Subtile(words[0]),
                top_right: lm_level::Subtile(words[1]),
                bottom_left: lm_level::Subtile(words[2]),
                bottom_right: lm_level::Subtile(words[3]),
                acts_like,
            })
            .collect::<Vec<_>>()
            .chunks(Map16Page::TILE_COUNT)
            .map(|tiles| Map16Page {
                tiles: tiles.to_vec(),
            })
            .collect();
        let set = Map16Set { pages };
        Ok(Self {
            revision: snapshot.revision,
            checksum_field_offset: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            baseline: set.clone(),
            set,
            acts_like_tail: loaded.acts_like[tile_count..].to_vec(),
        })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn set(&self) -> &Map16Set {
        &self.set
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.set != self.baseline
    }

    /// Applies a failure-atomic ordered edit batch.
    ///
    /// # Errors
    ///
    /// Returns the failing command and leaves the controller unchanged.
    pub fn apply_edits(
        &mut self,
        edits: &[Map16ControllerEdit],
    ) -> Result<(), SmwMap16ControllerError> {
        let mut staged = self.set.clone();
        for (command, edit) in edits.iter().enumerate() {
            let result = (|| {
                validate_shape(&staged)?;
                match edit {
                    Map16ControllerEdit::ReplaceTiles { replacements, .. } => {
                        let mut targets = BTreeSet::new();
                        for (address, _) in replacements {
                            validate_address(&staged, *address)?;
                            if !targets.insert(*address) {
                                return Err(Map16EditError::DuplicateTarget(*address));
                            }
                        }
                        for (address, tile) in replacements {
                            staged.pages[address.page].tiles[address.tile] = *tile;
                        }
                    }
                    Map16ControllerEdit::SetSubtile {
                        address,
                        quadrant,
                        subtile,
                        ..
                    } => {
                        validate_address(&staged, *address)?;
                        let tile = &mut staged.pages[address.page].tiles[address.tile];
                        match quadrant {
                            lm_level::Map16Quadrant::TopLeft => tile.top_left = *subtile,
                            lm_level::Map16Quadrant::TopRight => tile.top_right = *subtile,
                            lm_level::Map16Quadrant::BottomLeft => tile.bottom_left = *subtile,
                            lm_level::Map16Quadrant::BottomRight => tile.bottom_right = *subtile,
                        }
                    }
                    Map16ControllerEdit::SetActsLike {
                        address, acts_like, ..
                    } => {
                        validate_address(&staged, *address)?;
                        staged.pages[address.page].tiles[address.tile].acts_like = *acts_like;
                    }
                }
                Ok(())
            })();
            result.map_err(|error| SmwMap16ControllerError::Edit { command, error })?;
        }
        self.set = staged;
        Ok(())
    }

    /// Saves through the recovered native split-table format on a private project.
    ///
    /// # Errors
    ///
    /// Rejects malformed source bytes, allocation/save failures, or an invalid mutation.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
        options: &SmwUsV1TransferredMap16SaveOptions,
    ) -> Result<PreparedRomCommit, SmwMap16ControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(SmwMap16ControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let description = description.into();
        if !self.is_modified() {
            return Ok(PreparedRomCommit {
                expected_revision: self.revision,
                description,
                mutation: RomMutation::unchanged(Mapper::LoRom, before.len()),
            });
        }
        let mut definitions = Vec::with_capacity(self.set.pages.len() * 1024);
        let mut acts_like = Vec::with_capacity(
            self.set.pages.len() * Map16Page::TILE_COUNT + self.acts_like_tail.len(),
        );
        for page in &self.set.pages {
            for tile in &page.tiles {
                definitions.extend([
                    tile.top_left.0,
                    tile.top_right.0,
                    tile.bottom_left.0,
                    tile.bottom_right.0,
                ]);
                acts_like.push(tile.acts_like);
            }
        }
        acts_like.extend_from_slice(&self.acts_like_tail);
        let mut project = Project::new(image);
        save_smw_us_v1_transferred_map16(
            &mut project,
            &definitions,
            &acts_like,
            self.checksum_field_offset,
            options,
        )
        .map_err(SmwMap16ControllerError::Native)?;
        let reopened =
            load_smw_us_v1_transferred_map16(&project).map_err(SmwMap16ControllerError::Native)?;
        if reopened.definitions != definitions || reopened.acts_like != acts_like {
            return Err(SmwMap16ControllerError::SemanticReopen);
        }
        let mutation = RomMutation::between(Mapper::LoRom, &before, project.rom.logical_bytes())
            .map_err(SmwMap16ControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }
}

fn validate_shape(set: &Map16Set) -> Result<(), Map16EditError> {
    if set.pages.len() > Map16Set::MAX_PAGES {
        return Err(Map16EditError::TooManyPages(set.pages.len()));
    }
    for (page, value) in set.pages.iter().enumerate() {
        if value.tiles.len() != Map16Page::TILE_COUNT {
            return Err(Map16EditError::MalformedPage {
                page,
                tiles: value.tiles.len(),
            });
        }
    }
    Ok(())
}

fn validate_address(set: &Map16Set, address: lm_level::Map16Address) -> Result<(), Map16EditError> {
    if address.page >= set.pages.len() {
        return Err(Map16EditError::PageOutOfRange {
            page: address.page,
            len: set.pages.len(),
        });
    }
    if address.tile >= Map16Page::TILE_COUNT {
        return Err(Map16EditError::TileOutOfRange { tile: address.tile });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppState, Command};
    use lm_level::{Map16Address, Map16Quadrant, Subtile};
    use lm_profile::{
        SMW_US_V1_MAP16_ACTS_HIGH_BANK_OFFSET, SMW_US_V1_MAP16_ACTS_HIGH_WORD_OFFSET,
        SMW_US_V1_MAP16_ACTS_LOW_BANK_OFFSET, SMW_US_V1_MAP16_ACTS_LOW_WORD_OFFSET,
        SMW_US_V1_MAP16_DEFINITION_BANK_OFFSET, SMW_US_V1_MAP16_DEFINITION_ODD_WORD_OFFSET,
        SMW_US_V1_MAP16_DEFINITION_WORD_OFFSET,
    };
    use lm_rats::{AllocationPolicy, ProtectedRange};
    use std::{fs, path::Path};

    fn fixture() -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/overworld-transfer-positive/before.smc");
        let image = RomImage::from_bytes(fs::read(path).unwrap()).unwrap();
        let mut project = Project::new(image);
        project
            .expand_rom(Mapper::LoRom, 0x10_0000, 0xff, 0x7fdc)
            .unwrap();
        project.rom.as_file_bytes().to_vec()
    }

    fn options() -> SmwUsV1TransferredMap16SaveOptions {
        let mut protected = vec![ProtectedRange(0x7fc0..0x8000)];
        for (offset, len) in [
            (SMW_US_V1_MAP16_DEFINITION_WORD_OFFSET, 2),
            (SMW_US_V1_MAP16_DEFINITION_BANK_OFFSET, 1),
            (SMW_US_V1_MAP16_DEFINITION_ODD_WORD_OFFSET, 2),
            (SMW_US_V1_MAP16_ACTS_LOW_WORD_OFFSET, 2),
            (SMW_US_V1_MAP16_ACTS_LOW_BANK_OFFSET, 1),
            (SMW_US_V1_MAP16_ACTS_HIGH_WORD_OFFSET, 2),
            (SMW_US_V1_MAP16_ACTS_HIGH_BANK_OFFSET, 1),
        ] {
            protected.push(ProtectedRange(offset..offset + len));
        }
        SmwUsV1TransferredMap16SaveOptions {
            allocation: AllocationPolicy {
                search: 0x80_000..0x10_0000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected,
            },
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    #[test]
    fn vanilla_snapshot_edits_reopens_and_preserves_acts_tail() {
        let mut app = AppState::default();
        app.load_rom(fixture()).unwrap();
        app.dispatch(Command::ShowMap16).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let before = load_smw_us_v1_transferred_map16(app.project().unwrap()).unwrap();
        let mut controller = SmwMap16Controller::decode(&snapshot).unwrap();
        assert_eq!(controller.set().pages.len(), 8);
        controller
            .apply_edits(&[
                Map16ControllerEdit::SetSubtile {
                    address: Map16Address { page: 0, tile: 0 },
                    quadrant: Map16Quadrant::TopLeft,
                    subtile: Subtile(0x4321),
                    resolution_limit: 2048,
                },
                Map16ControllerEdit::SetActsLike {
                    address: Map16Address { page: 0, tile: 0 },
                    acts_like: 0x1234,
                    resolution_limit: 2048,
                },
            ])
            .unwrap();
        let prepared = controller
            .prepare_commit("Edit SMW Map16", &options())
            .unwrap();
        app.dispatch(prepared.into_command()).unwrap();
        let reopened = load_smw_us_v1_transferred_map16(app.project().unwrap()).unwrap();
        assert_eq!(reopened.definitions[0], 0x4321);
        assert_eq!(reopened.acts_like[0], 0x1234);
        assert_eq!(reopened.acts_like[2048..], before.acts_like[2048..]);
        assert_eq!(reopened.acts_like.len(), 2884);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            load_smw_us_v1_transferred_map16(app.project().unwrap())
                .unwrap()
                .definitions,
            before.definitions
        );
    }
}
