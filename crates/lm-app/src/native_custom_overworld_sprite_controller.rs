use crate::{ControllerSnapshot, EditorMode, PreparedRomCommit};
use lm_overworld::{
    CUSTOM_OVERWORLD_SPRITE_ID_COUNT, NativeCustomOverworldSprite,
    NativeCustomOverworldSpriteError, NativeCustomOverworldSpriteTable,
};
use lm_project::{
    NativeCustomOverworldSpriteIoError, NativeCustomOverworldSpriteRomLayout,
    NativeCustomOverworldSpriteSaveOptions, Project, RomMutation, TransactionError,
};
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomError, RomImage};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeCustomOverworldSpriteEdit {
    Insert {
        map: usize,
        index: usize,
        sprite: NativeCustomOverworldSprite,
    },
    Replace {
        map: usize,
        index: usize,
        sprite: NativeCustomOverworldSprite,
    },
    Remove {
        map: usize,
        index: usize,
    },
    /// Moves one record before `before` in the pre-move ordering; the list length means the end.
    MoveBefore {
        map: usize,
        from: usize,
        before: usize,
    },
}

#[derive(Debug)]
pub enum NativeCustomOverworldSpriteControllerError {
    WrongMode(EditorMode),
    MapperMismatch {
        snapshot: Mapper,
        layout: Mapper,
    },
    BaseRevisionMismatch {
        controller: u64,
        prepared: u64,
    },
    MapOutOfRange(usize),
    IndexOutOfRange {
        command: usize,
        map: usize,
        index: usize,
        len: usize,
    },
    Codec(NativeCustomOverworldSpriteError),
    Io(NativeCustomOverworldSpriteIoError),
    Rom(RomError),
    Mutation(TransactionError),
}

impl fmt::Display for NativeCustomOverworldSpriteControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native custom overworld sprite controller failed: {self:?}"
        )
    }
}

impl std::error::Error for NativeCustomOverworldSpriteControllerError {}

/// Revision-bound controller for Lunar Magic's seven native custom-overworld-sprite lists.
#[derive(Clone, Debug)]
pub struct NativeCustomOverworldSpriteController {
    revision: u64,
    layout: NativeCustomOverworldSpriteRomLayout,
    checksum_field: usize,
    source_file_bytes: Vec<u8>,
    record_sizes: [u8; CUSTOM_OVERWORLD_SPRITE_ID_COUNT],
    baseline: NativeCustomOverworldSpriteTable,
    table: NativeCustomOverworldSpriteTable,
    previous_block: Option<RatsBlock>,
}

impl NativeCustomOverworldSpriteController {
    /// Loads the exact RATS-owned seven-map stream from an immutable overworld snapshot.
    pub fn decode(
        snapshot: &ControllerSnapshot,
        layout: NativeCustomOverworldSpriteRomLayout,
        record_sizes: [u8; CUSTOM_OVERWORLD_SPRITE_ID_COUNT],
    ) -> Result<Self, NativeCustomOverworldSpriteControllerError> {
        if snapshot.mode != EditorMode::Overworld {
            return Err(NativeCustomOverworldSpriteControllerError::WrongMode(
                snapshot.mode,
            ));
        }
        if snapshot.identity.mapper != layout.mapper {
            return Err(NativeCustomOverworldSpriteControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: layout.mapper,
            });
        }
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(NativeCustomOverworldSpriteControllerError::Rom)?;
        let loaded = Project::new(image)
            .load_native_custom_overworld_sprites(layout, &record_sizes)
            .map_err(NativeCustomOverworldSpriteControllerError::Io)?;
        Ok(Self {
            revision: snapshot.revision,
            layout,
            checksum_field: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            record_sizes,
            baseline: loaded.table.clone(),
            table: loaded.table,
            previous_block: loaded.block,
        })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn table(&self) -> &NativeCustomOverworldSpriteTable {
        &self.table
    }

    /// Returns the extension-byte width selected by the authenticated record-size table.
    #[must_use]
    pub fn required_extra_len(&self, id: u8) -> Option<usize> {
        self.record_sizes
            .get(usize::from(id))
            .and_then(|size| usize::from(*size).checked_sub(3))
    }

    /// Returns the authenticated current stream owner, if the vanilla empty sentinel is replaced.
    #[must_use]
    pub const fn owned_block(&self) -> Option<&RatsBlock> {
        self.previous_block.as_ref()
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.table != self.baseline
    }

    /// Applies an ordered edit batch to a private clone and canonically reopens it before publish.
    pub fn apply_edits(
        &mut self,
        edits: &[NativeCustomOverworldSpriteEdit],
    ) -> Result<(), NativeCustomOverworldSpriteControllerError> {
        let mut staged = self.table.clone();
        for (command, edit) in edits.iter().enumerate() {
            apply_edit(&mut staged, command, edit)?;
        }
        let encoded = staged
            .encode(&self.record_sizes)
            .map_err(NativeCustomOverworldSpriteControllerError::Codec)?;
        self.table = NativeCustomOverworldSpriteTable::decode(&encoded, &self.record_sizes)
            .map_err(NativeCustomOverworldSpriteControllerError::Codec)?;
        Ok(())
    }

    /// Builds one checksum-inclusive ROM mutation bound to the source application revision.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
        options: &NativeCustomOverworldSpriteSaveOptions,
    ) -> Result<PreparedRomCommit, NativeCustomOverworldSpriteControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(NativeCustomOverworldSpriteControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let description = description.into();
        if !self.is_modified() {
            return Ok(PreparedRomCommit {
                expected_revision: self.revision,
                description,
                mutation: RomMutation::unchanged(self.layout.mapper, before.len()),
            });
        }
        let mut options = options.clone();
        options.previous_block = self.previous_block.clone();
        let mut project = Project::new(image);
        project
            .save_native_custom_overworld_sprites(
                &self.table,
                &self.record_sizes,
                self.layout,
                &options,
            )
            .map_err(NativeCustomOverworldSpriteControllerError::Io)?;
        project
            .rom
            .update_snes_checksum(self.checksum_field)
            .map_err(NativeCustomOverworldSpriteControllerError::Rom)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(NativeCustomOverworldSpriteControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }

    /// Rebases this staged stream onto an already prepared mutation and returns one transaction.
    ///
    /// Allocation runs against the materialized result of `prepared`, so newly allocated ordinary
    /// overworld payloads cannot collide with the custom sprite stream. The returned mutation is
    /// still relative to this controller's immutable source image and therefore remains one Undo
    /// step at the application boundary.
    pub fn merge_into_commit(
        &self,
        mut prepared: PreparedRomCommit,
        options: &NativeCustomOverworldSpriteSaveOptions,
    ) -> Result<PreparedRomCommit, NativeCustomOverworldSpriteControllerError> {
        if prepared.expected_revision != self.revision {
            return Err(
                NativeCustomOverworldSpriteControllerError::BaseRevisionMismatch {
                    controller: self.revision,
                    prepared: prepared.expected_revision,
                },
            );
        }
        if !self.is_modified() {
            return Ok(prepared);
        }
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(NativeCustomOverworldSpriteControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let mut project = Project::new(image);
        project
            .apply_mutation("stage preceding overworld edits", &prepared.mutation)
            .map_err(NativeCustomOverworldSpriteControllerError::Mutation)?;
        let mut options = options.clone();
        options.previous_block = self.previous_block.clone();
        project
            .save_native_custom_overworld_sprites(
                &self.table,
                &self.record_sizes,
                self.layout,
                &options,
            )
            .map_err(NativeCustomOverworldSpriteControllerError::Io)?;
        project
            .rom
            .update_snes_checksum(self.checksum_field)
            .map_err(NativeCustomOverworldSpriteControllerError::Rom)?;
        prepared.description = format!(
            "{} and native custom overworld sprites",
            prepared.description
        );
        prepared.mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(NativeCustomOverworldSpriteControllerError::Mutation)?;
        Ok(prepared)
    }
}

fn apply_edit(
    table: &mut NativeCustomOverworldSpriteTable,
    command: usize,
    edit: &NativeCustomOverworldSpriteEdit,
) -> Result<(), NativeCustomOverworldSpriteControllerError> {
    let map = match edit {
        NativeCustomOverworldSpriteEdit::Insert { map, .. }
        | NativeCustomOverworldSpriteEdit::Replace { map, .. }
        | NativeCustomOverworldSpriteEdit::Remove { map, .. }
        | NativeCustomOverworldSpriteEdit::MoveBefore { map, .. } => *map,
    };
    let records = table.maps.get_mut(map).ok_or(
        NativeCustomOverworldSpriteControllerError::MapOutOfRange(map),
    )?;
    match edit {
        NativeCustomOverworldSpriteEdit::Insert { index, sprite, .. } => {
            if *index > records.len() {
                return Err(index_error(command, map, *index, records.len()));
            }
            records.insert(*index, sprite.clone());
        }
        NativeCustomOverworldSpriteEdit::Replace { index, sprite, .. } => {
            let len = records.len();
            let target = records
                .get_mut(*index)
                .ok_or_else(|| index_error(command, map, *index, len))?;
            *target = sprite.clone();
        }
        NativeCustomOverworldSpriteEdit::Remove { index, .. } => {
            if *index >= records.len() {
                return Err(index_error(command, map, *index, records.len()));
            }
            records.remove(*index);
        }
        NativeCustomOverworldSpriteEdit::MoveBefore { from, before, .. } => {
            let len = records.len();
            if *from >= len {
                return Err(index_error(command, map, *from, len));
            }
            if *before > len {
                return Err(index_error(command, map, *before, len));
            }
            if from != before && from.checked_add(1) != Some(*before) {
                let record = records.remove(*from);
                records.insert(if before > from { before - 1 } else { *before }, record);
            }
        }
    }
    Ok(())
}

fn index_error(
    command: usize,
    map: usize,
    index: usize,
    len: usize,
) -> NativeCustomOverworldSpriteControllerError {
    NativeCustomOverworldSpriteControllerError::IndexOutOfRange {
        command,
        map,
        index,
        len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppState, Command};
    use lm_overworld::CUSTOM_OVERWORLD_SPRITES_PER_MAP;
    use lm_rats::{AllocationPolicy, ProtectedRange};
    use lm_rom::compute_snes_checksum;

    const POINTER: usize = 0x2000;
    const CHECKSUM: usize = 0x7fdc;

    fn sprite(id: u8) -> NativeCustomOverworldSprite {
        NativeCustomOverworldSprite {
            id,
            x: 0x80,
            y: 0x118,
            screen: 0x20,
            extra: vec![id],
        }
    }

    fn table() -> NativeCustomOverworldSpriteTable {
        NativeCustomOverworldSpriteTable {
            maps: std::array::from_fn(|map| {
                (map == 2).then(|| vec![sprite(5)]).unwrap_or_default()
            }),
        }
    }

    const fn layout() -> NativeCustomOverworldSpriteRomLayout {
        NativeCustomOverworldSpriteRomLayout {
            mapper: Mapper::LoRom,
            pointer_offset: POINTER,
            maximum_payload_len: 0x1000,
        }
    }

    fn save_options() -> NativeCustomOverworldSpriteSaveOptions {
        NativeCustomOverworldSpriteSaveOptions {
            allocation: AllocationPolicy {
                search: 0x3000..0x7000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![
                    ProtectedRange(POINTER..POINTER + 3),
                    ProtectedRange(CHECKSUM..CHECKSUM + 4),
                ],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    fn application() -> AppState {
        let mut bytes = vec![0xff; 0x8000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes[0x7fd9] = 1;
        bytes[0x7fdb] = 0;
        let checksum = compute_snes_checksum(&bytes, CHECKSUM).unwrap().encoded();
        bytes[CHECKSUM..CHECKSUM + checksum.len()].copy_from_slice(&checksum);
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        project
            .save_native_custom_overworld_sprites(&table(), &[4; 128], layout(), &save_options())
            .unwrap();
        project.rom.update_snes_checksum(CHECKSUM).unwrap();
        let mut app = AppState::default();
        app.load_rom(project.save_snapshot()).unwrap();
        app.dispatch(Command::ShowOverworld).unwrap();
        app
    }

    #[test]
    fn all_record_edits_commit_reopen_checksum_undo_and_reject_stale_publish() {
        let mut app = application();
        let mut controller = NativeCustomOverworldSpriteController::decode(
            &app.controller_snapshot().unwrap(),
            layout(),
            [4; 128],
        )
        .unwrap();
        controller
            .apply_edits(&[
                NativeCustomOverworldSpriteEdit::Insert {
                    map: 2,
                    index: 1,
                    sprite: sprite(6),
                },
                NativeCustomOverworldSpriteEdit::Insert {
                    map: 2,
                    index: 2,
                    sprite: sprite(7),
                },
                NativeCustomOverworldSpriteEdit::MoveBefore {
                    map: 2,
                    from: 2,
                    before: 0,
                },
                NativeCustomOverworldSpriteEdit::Replace {
                    map: 2,
                    index: 1,
                    sprite: sprite(8),
                },
                NativeCustomOverworldSpriteEdit::Remove { map: 2, index: 2 },
            ])
            .unwrap();
        assert_eq!(
            controller.table().maps[2]
                .iter()
                .map(|record| record.id)
                .collect::<Vec<_>>(),
            [7, 8]
        );
        let commit = controller
            .prepare_commit("Edit native custom overworld sprites", &save_options())
            .unwrap();
        app.dispatch(commit.into_command()).unwrap();
        assert_eq!(
            app.project()
                .unwrap()
                .load_native_custom_overworld_sprites(layout(), &[4; 128])
                .unwrap()
                .table,
            *controller.table()
        );
        assert!(
            app.project()
                .unwrap()
                .identity
                .as_ref()
                .unwrap()
                .checksum_matches()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project()
                .unwrap()
                .load_native_custom_overworld_sprites(layout(), &[4; 128])
                .unwrap()
                .table,
            table()
        );
        assert!(
            app.dispatch(
                controller
                    .prepare_commit("stale", &save_options())
                    .unwrap()
                    .into_command()
            )
            .is_err()
        );
    }

    #[test]
    fn late_invalid_id_and_twenty_fifth_record_are_batch_atomic() {
        let app = application();
        let mut controller = NativeCustomOverworldSpriteController::decode(
            &app.controller_snapshot().unwrap(),
            layout(),
            [4; 128],
        )
        .unwrap();
        let original = controller.table().clone();
        assert!(
            controller
                .apply_edits(&[
                    NativeCustomOverworldSpriteEdit::Insert {
                        map: 0,
                        index: 0,
                        sprite: sprite(1),
                    },
                    NativeCustomOverworldSpriteEdit::Insert {
                        map: 1,
                        index: 0,
                        sprite: sprite(0x80),
                    },
                ])
                .is_err()
        );
        assert_eq!(controller.table(), &original);

        let edits = (0..=CUSTOM_OVERWORLD_SPRITES_PER_MAP)
            .map(|index| NativeCustomOverworldSpriteEdit::Insert {
                map: 0,
                index,
                sprite: sprite(1),
            })
            .collect::<Vec<_>>();
        assert!(controller.apply_edits(&edits).is_err());
        assert_eq!(controller.table(), &original);
    }

    #[test]
    fn merge_materializes_prior_writes_avoids_them_and_undoes_both_domains_once() {
        let mut app = application();
        let original = app.project().unwrap().save_snapshot();
        let mut controller = NativeCustomOverworldSpriteController::decode(
            &app.controller_snapshot().unwrap(),
            layout(),
            [4; 128],
        )
        .unwrap();
        let growth = (0..20)
            .map(|index| NativeCustomOverworldSpriteEdit::Insert {
                map: 0,
                index,
                sprite: sprite(9),
            })
            .collect::<Vec<_>>();
        controller.apply_edits(&growth).unwrap();
        let preceding = PreparedRomCommit {
            expected_revision: controller.revision(),
            description: "Edit ordinary overworld terrain".into(),
            mutation: RomMutation {
                mapper: Mapper::LoRom,
                expected_len: 0x8000,
                appended: Vec::new(),
                writes: vec![lm_project::RomWrite {
                    offset: 0x3050,
                    bytes: vec![0x5a; 0x100],
                }],
            },
        };
        let merged = controller
            .merge_into_commit(preceding, &save_options())
            .unwrap();
        app.dispatch(merged.into_command()).unwrap();
        let project = app.project().unwrap();
        assert_eq!(project.rom.read(0x3050, 0x100).unwrap(), vec![0x5a; 0x100]);
        let loaded = project
            .load_native_custom_overworld_sprites(layout(), &[4; 128])
            .unwrap();
        assert_eq!(loaded.table, *controller.table());
        assert!(loaded.block.unwrap().header_offset >= 0x3150);
        assert!(project.identity.as_ref().unwrap().checksum_matches());
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);

        let mismatch = PreparedRomCommit {
            expected_revision: controller.revision() + 1,
            description: "stale base".into(),
            mutation: RomMutation::unchanged(Mapper::LoRom, 0x8000),
        };
        assert!(matches!(
            controller.merge_into_commit(mismatch, &save_options()),
            Err(NativeCustomOverworldSpriteControllerError::BaseRevisionMismatch { .. })
        ));
    }
}
