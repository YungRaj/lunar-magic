use crate::{ControllerSnapshot, EditorMode, Map16ControllerEdit, PreparedRomCommit};
use lm_level::{Map16EditError, Map16Page, Map16Set, Map16Tile};
use lm_profile::{
    SmwUsV1CompleteMap16Error, SmwUsV1CompleteMap16SaveOptions, load_smw_us_v1_complete_map16,
    save_smw_us_v1_complete_map16,
};
use lm_project::{Project, RomMutation, TransactionError};
use lm_rom::{Mapper, RomError, RomImage};
use std::{collections::BTreeSet, fmt};

pub const SMW_COMPLETE_MAP16_FOREGROUND_PAGES: usize = 0x80;
pub const SMW_COMPLETE_MAP16_PAGES: usize = 0x100;

#[derive(Debug)]
pub enum SmwMap16ControllerError {
    WrongMode(EditorMode),
    Mapper(Mapper),
    Rom(RomError),
    Native(SmwUsV1CompleteMap16Error),
    PageCount(usize),
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
}

impl SmwMap16Controller {
    /// Decodes all foreground and background Map16 definitions plus foreground Acts-Like words.
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
        let loaded = load_smw_us_v1_complete_map16(&Project::new(image))
            .map_err(SmwMap16ControllerError::Native)?;
        let mut pages = map16_pages(
            &loaded.foreground.definitions,
            Some(&loaded.foreground.acts_like),
        );
        pages.extend(map16_pages(&loaded.background.definitions, None));
        let set = Map16Set { pages };
        Ok(Self {
            revision: snapshot.revision,
            checksum_field_offset: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            baseline: set.clone(),
            set,
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
                            let mut tile = *tile;
                            if address.page >= SMW_COMPLETE_MAP16_FOREGROUND_PAGES {
                                tile.acts_like = 0;
                            }
                            staged.pages[address.page].tiles[address.tile] = tile;
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
                        if address.page >= SMW_COMPLETE_MAP16_FOREGROUND_PAGES {
                            return Err(Map16EditError::BackgroundActsLike(*address));
                        }
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

    /// Saves the coordinated base, foreground, Acts-Like, and background formats privately.
    ///
    /// # Errors
    ///
    /// Rejects malformed source bytes, allocation/save failures, or an invalid mutation.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
        options: &SmwUsV1CompleteMap16SaveOptions,
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
        if self.set.pages.len() != SMW_COMPLETE_MAP16_PAGES {
            return Err(SmwMap16ControllerError::PageCount(self.set.pages.len()));
        }
        let mut foreground = Vec::with_capacity(0x20_000);
        let mut background = Vec::with_capacity(0x20_000);
        let mut acts_like = Vec::with_capacity(0x8000);
        for (page_index, page) in self.set.pages.iter().enumerate() {
            for tile in &page.tiles {
                let definitions = if page_index < SMW_COMPLETE_MAP16_FOREGROUND_PAGES {
                    acts_like.push(tile.acts_like);
                    &mut foreground
                } else {
                    &mut background
                };
                definitions.extend([
                    tile.top_left.0,
                    tile.top_right.0,
                    tile.bottom_left.0,
                    tile.bottom_right.0,
                ]);
            }
        }
        let mut project = Project::new(image);
        save_smw_us_v1_complete_map16(
            &mut project,
            &foreground,
            &background,
            &acts_like,
            self.checksum_field_offset,
            options,
        )
        .map_err(SmwMap16ControllerError::Native)?;
        let reopened =
            load_smw_us_v1_complete_map16(&project).map_err(SmwMap16ControllerError::Native)?;
        if reopened.foreground.definitions != foreground
            || reopened.background.definitions != background
            || reopened.foreground.acts_like != acts_like
        {
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

fn map16_pages(definitions: &[u16], acts_like: Option<&[u16]>) -> Vec<Map16Page> {
    definitions
        .chunks_exact(4)
        .enumerate()
        .map(|(tile, words)| Map16Tile {
            top_left: lm_level::Subtile(words[0]),
            top_right: lm_level::Subtile(words[1]),
            bottom_left: lm_level::Subtile(words[2]),
            bottom_right: lm_level::Subtile(words[3]),
            acts_like: acts_like.map_or(0, |values| values[tile]),
        })
        .collect::<Vec<_>>()
        .chunks(Map16Page::TILE_COUNT)
        .map(|tiles| Map16Page {
            tiles: tiles.to_vec(),
        })
        .collect()
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
    use lm_rats::{AllocationPolicy, ProtectedRange};
    use std::{fs, path::Path};

    fn fixture() -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/overworld-transfer-positive/before.smc");
        fs::read(path).unwrap()
    }

    fn options() -> SmwUsV1CompleteMap16SaveOptions {
        SmwUsV1CompleteMap16SaveOptions {
            allocation: AllocationPolicy {
                search: 0x80_000..0x10_0000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0, 0xff],
                protected: vec![ProtectedRange(0x7fc0..0x8000)],
            },
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    #[test]
    fn vanilla_snapshot_edits_all_complete_domains_and_reopens() {
        let mut app = AppState::default();
        app.load_rom(fixture()).unwrap();
        app.dispatch(Command::ShowMap16).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let before = load_smw_us_v1_complete_map16(app.project().unwrap()).unwrap();
        let mut controller = SmwMap16Controller::decode(&snapshot).unwrap();
        assert_eq!(controller.set().pages.len(), SMW_COMPLETE_MAP16_PAGES);
        assert!(matches!(
            controller.apply_edits(&[Map16ControllerEdit::SetActsLike {
                address: Map16Address {
                    page: SMW_COMPLETE_MAP16_FOREGROUND_PAGES,
                    tile: 0,
                },
                acts_like: 1,
                resolution_limit: 0x1_0000,
            }]),
            Err(SmwMap16ControllerError::Edit {
                error: Map16EditError::BackgroundActsLike(_),
                ..
            })
        ));
        assert!(!controller.is_modified());
        controller
            .apply_edits(&[
                Map16ControllerEdit::SetSubtile {
                    address: Map16Address { page: 0, tile: 0 },
                    quadrant: Map16Quadrant::TopLeft,
                    subtile: Subtile(0x4321),
                    resolution_limit: 0x1_0000,
                },
                Map16ControllerEdit::SetActsLike {
                    address: Map16Address { page: 0, tile: 0 },
                    acts_like: 0x1234,
                    resolution_limit: 0x1_0000,
                },
                Map16ControllerEdit::SetSubtile {
                    address: Map16Address {
                        page: SMW_COMPLETE_MAP16_FOREGROUND_PAGES,
                        tile: 0,
                    },
                    quadrant: Map16Quadrant::BottomRight,
                    subtile: Subtile(0x5678),
                    resolution_limit: 0x1_0000,
                },
            ])
            .unwrap();
        let prepared = controller
            .prepare_commit("Edit SMW Map16", &options())
            .unwrap();
        app.dispatch(prepared.into_command()).unwrap();
        assert_eq!(app.project().unwrap().rom.logical_len(), 0x10_0000);
        let reopened = load_smw_us_v1_complete_map16(app.project().unwrap()).unwrap();
        assert_eq!(reopened.foreground.definitions[0], 0x4321);
        assert_eq!(reopened.foreground.acts_like[0], 0x1234);
        assert_eq!(reopened.background.definitions[3], 0x5678);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.logical_len(), 0x80_000);
        assert_eq!(
            load_smw_us_v1_complete_map16(app.project().unwrap()).unwrap(),
            before
        );
    }
}
