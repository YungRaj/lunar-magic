use crate::{ControllerSnapshot, EditorMode, Map16ControllerEdit, PreparedRomCommit};
use lm_level::{Map16EditError, Map16Page, Map16Set, Map16Tile};
use lm_profile::{
    SmwUsV1CompleteMap16Error, SmwUsV1CompleteMap16SaveOptions, load_smw_us_v1_complete_map16,
    save_smw_us_v1_complete_map16,
};
use lm_project::{Project, RomMutation, TransactionError};
use lm_rom::{Mapper, RomError, RomImage};
use std::fmt;

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
            let result = match edit {
                Map16ControllerEdit::ReplaceTiles {
                    replacements,
                    resolution_limit,
                } => {
                    let replacements = replacements
                        .iter()
                        .map(|(address, tile)| {
                            let mut tile = *tile;
                            if address.page >= SMW_COMPLETE_MAP16_FOREGROUND_PAGES {
                                tile.acts_like = 0;
                            }
                            (*address, tile)
                        })
                        .collect::<Vec<_>>();
                    staged.replace_tiles(&replacements, *resolution_limit)
                }
                Map16ControllerEdit::SetSubtile {
                    address,
                    quadrant,
                    subtile,
                    resolution_limit,
                } => staged.set_subtile(*address, *quadrant, *subtile, *resolution_limit),
                Map16ControllerEdit::SetActsLike {
                    address,
                    acts_like,
                    resolution_limit,
                } => {
                    if address.page >= SMW_COMPLETE_MAP16_FOREGROUND_PAGES {
                        Err(Map16EditError::BackgroundActsLike(*address))
                    } else {
                        staged.set_acts_like(*address, *acts_like, *resolution_limit)
                    }
                }
            };
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

    #[test]
    fn native_edits_reject_acts_like_cycles_atomically_and_normalize_background_behavior() {
        let mut app = AppState::default();
        app.load_rom(fixture()).unwrap();
        app.dispatch(Command::ShowMap16).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut controller = SmwMap16Controller::decode(&snapshot).unwrap();
        let before = controller.set().clone();
        let first = Map16Address { page: 2, tile: 0 };
        let second = Map16Address { page: 2, tile: 1 };
        let mut first_tile = before.pages[first.page].tiles[first.tile];
        let mut second_tile = before.pages[second.page].tiles[second.tile];
        first_tile.acts_like = 0x0201;
        second_tile.acts_like = 0x0200;

        assert!(matches!(
            controller.apply_edits(&[Map16ControllerEdit::ReplaceTiles {
                replacements: vec![(first, first_tile), (second, second_tile)],
                resolution_limit: SMW_COMPLETE_MAP16_PAGES * Map16Page::TILE_COUNT,
            }]),
            Err(SmwMap16ControllerError::Edit {
                command: 0,
                error: Map16EditError::ActsLike(lm_level::Map16SetError::ActsLikeCycle { .. }),
            })
        ));
        assert_eq!(controller.set(), &before);
        assert!(!controller.is_modified());

        let background = Map16Address {
            page: SMW_COMPLETE_MAP16_FOREGROUND_PAGES,
            tile: 7,
        };
        let mut background_tile = before.pages[background.page].tiles[background.tile];
        background_tile.top_left = Subtile(0x1234);
        background_tile.acts_like = 0xbeef;
        controller
            .apply_edits(&[Map16ControllerEdit::ReplaceTiles {
                replacements: vec![(background, background_tile)],
                resolution_limit: SMW_COMPLETE_MAP16_PAGES * Map16Page::TILE_COUNT,
            }])
            .unwrap();
        assert_eq!(
            controller.set().pages[background.page].tiles[background.tile],
            Map16Tile {
                acts_like: 0,
                ..background_tile
            }
        );
    }

    #[test]
    fn native_edits_enforce_the_caller_resolution_limit() {
        let mut app = AppState::default();
        app.load_rom(fixture()).unwrap();
        app.dispatch(Command::ShowMap16).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut controller = SmwMap16Controller::decode(&snapshot).unwrap();
        let before = controller.set().clone();

        assert!(matches!(
            controller.apply_edits(&[Map16ControllerEdit::SetSubtile {
                address: Map16Address { page: 2, tile: 0 },
                quadrant: Map16Quadrant::TopLeft,
                subtile: Subtile(0x0123),
                resolution_limit: 0,
            }]),
            Err(SmwMap16ControllerError::Edit {
                command: 0,
                error: Map16EditError::ActsLike(lm_level::Map16SetError::ResolutionLimit(0)),
            })
        ));
        assert_eq!(controller.set(), &before);
        assert!(!controller.is_modified());
    }
}
