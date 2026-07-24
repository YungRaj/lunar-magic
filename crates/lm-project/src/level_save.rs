use crate::{
    LevelLoadError, LevelRomLayout, LoadedLevelSlot, PayloadReclamation, PayloadSaveError,
    PayloadSaveRequest, PayloadSaveResult, Project,
};
use lm_level::{
    LevelObjectData, NativeSpriteEncodingError, NativeSpriteStream, ObjectStreamError,
    SpriteLengthTable, SpriteStreamError,
};
use lm_rats::{AllocationPolicy, RatsBlock};
use std::fmt;

/// Allocation details for the two independently stored native level streams.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevelSaveOptions {
    pub layer1_allocation: AllocationPolicy,
    pub sprite_allocation: AllocationPolicy,
    pub previous_layer1: Option<RatsBlock>,
    pub previous_sprites: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedLevelSlot {
    pub layer1: PayloadSaveResult,
    pub sprites: PayloadSaveResult,
}

#[derive(Debug)]
pub enum LevelSaveError {
    Layout(LevelLoadError),
    Objects(ObjectStreamError),
    NonCanonicalObjectEncoding,
    SpriteVariantMismatch {
        layout_expanded: bool,
        stream_expanded: bool,
    },
    SpriteBankLimitExceeded(usize),
    SpriteEncoding(NativeSpriteEncodingError),
    SpriteParse(SpriteStreamError),
    NonCanonicalSpriteEncoding,
    Payload(PayloadSaveError),
}

impl fmt::Display for LevelSaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "level save failed: {self:?}")
    }
}

impl std::error::Error for LevelSaveError {}

impl From<LevelLoadError> for LevelSaveError {
    fn from(value: LevelLoadError) -> Self {
        Self::Layout(value)
    }
}

impl From<ObjectStreamError> for LevelSaveError {
    fn from(value: ObjectStreamError) -> Self {
        Self::Objects(value)
    }
}

impl From<NativeSpriteEncodingError> for LevelSaveError {
    fn from(value: NativeSpriteEncodingError) -> Self {
        Self::SpriteEncoding(value)
    }
}

impl From<PayloadSaveError> for LevelSaveError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Payload(value)
    }
}

impl Project {
    /// Saves only the Layer 1 object stream and repairs the checksum atomically.
    ///
    /// This path preserves the sprite pointer and payload byte-for-byte. It is particularly useful
    /// for pristine SMW, whose sprite pointers share a fixed bank until the expanded sprite-pointer
    /// runtime is installed.
    ///
    /// # Errors
    ///
    /// Returns [`LevelSaveError`] for serialization, allocation, mapping, or checksum failures.
    pub fn save_level_layer1_with_checksum(
        &mut self,
        layout: LevelRomLayout,
        level: &LoadedLevelSlot,
        checksum_field: usize,
        options: &LevelSaveOptions,
    ) -> Result<PayloadSaveResult, LevelSaveError> {
        let request = layer1_save_request(layout, level, options)?;
        Ok(self
            .save_tagged_payloads_with_checksum(
                format!("save level {:03x} layer 1", level.number),
                std::slice::from_ref(&request),
                checksum_field,
            )?
            .remove(0))
    }

    /// Serializes, allocates, and repoints both native streams as one undoable operation.
    ///
    /// The project is unchanged if serialization, either allocation, either mapper conversion, or
    /// either pointer update fails.
    ///
    /// # Errors
    ///
    /// Returns [`LevelSaveError`] for incompatible formats, oversized streams, invalid layouts, or
    /// failed allocation/transaction work.
    pub fn save_level_slot(
        &mut self,
        layout: LevelRomLayout,
        level: &LoadedLevelSlot,
        sprite_lengths: &SpriteLengthTable,
        options: &LevelSaveOptions,
    ) -> Result<SavedLevelSlot, LevelSaveError> {
        let requests = level_save_requests(layout, level, sprite_lengths, options)?;
        let mut saved = self.save_tagged_payloads(
            format!("save complete level {:03x}", level.number),
            &requests,
        )?;
        Ok(SavedLevelSlot {
            layer1: saved.remove(0),
            sprites: saved.remove(0),
        })
    }

    /// Saves both native streams and repairs the SNES checksum in the same undoable transaction.
    ///
    /// # Errors
    ///
    /// Returns [`LevelSaveError`] for the same conditions as [`Self::save_level_slot`], including
    /// an invalid checksum field. Failure leaves ROM bytes and history unchanged.
    pub fn save_level_slot_with_checksum(
        &mut self,
        layout: LevelRomLayout,
        level: &LoadedLevelSlot,
        sprite_lengths: &SpriteLengthTable,
        checksum_field: usize,
        options: &LevelSaveOptions,
    ) -> Result<SavedLevelSlot, LevelSaveError> {
        let requests = level_save_requests(layout, level, sprite_lengths, options)?;
        let mut saved = self.save_tagged_payloads_with_checksum(
            format!("save complete level {:03x}", level.number),
            &requests,
            checksum_field,
        )?;
        Ok(SavedLevelSlot {
            layer1: saved.remove(0),
            sprites: saved.remove(0),
        })
    }

    /// Saves both level streams, reclaims exactly owned displaced blocks, and repairs checksum as
    /// one undoable transaction.
    ///
    /// # Errors
    ///
    /// Returns [`LevelSaveError`] for serialization, ownership, allocation, overlap, mapping, or
    /// checksum failure without mutation.
    pub fn save_level_slot_with_checksum_and_reclamation(
        &mut self,
        layout: LevelRomLayout,
        level: &LoadedLevelSlot,
        sprite_lengths: &SpriteLengthTable,
        options: &LevelSaveOptions,
        reclamation: PayloadReclamation<'_>,
    ) -> Result<SavedLevelSlot, LevelSaveError> {
        let requests = level_save_requests(layout, level, sprite_lengths, options)?;
        let mut saved = self.save_tagged_payloads_with_checksum_and_reclamation(
            format!("save complete level {:03x}", level.number),
            &requests,
            reclamation.checksum_field,
            reclamation.manifest,
        )?;
        Ok(SavedLevelSlot {
            layer1: saved.remove(0),
            sprites: saved.remove(0),
        })
    }
}

fn layer1_save_request(
    layout: LevelRomLayout,
    level: &LoadedLevelSlot,
    options: &LevelSaveOptions,
) -> Result<PayloadSaveRequest, LevelSaveError> {
    let layer1 = level.layer1.encode_banked()?;
    if LevelObjectData::parse(&layer1)? != level.layer1 {
        return Err(LevelSaveError::NonCanonicalObjectEncoding);
    }
    Ok(PayloadSaveRequest {
        description: format!("save level {:03x} layer 1", level.number),
        payload: layer1,
        pointer: layout.layer1.pointer_offset(level.number)?.into(),
        mapper: layout.mapper,
        allocation_policy: options.layer1_allocation.clone(),
        previous_block: options.previous_layer1.clone(),
        reuse_identical: options.reuse_identical,
        maximum_payload_len: 0x8000,
        erase_fill: options.erase_fill,
    })
}

pub(crate) fn level_save_requests(
    layout: LevelRomLayout,
    level: &LoadedLevelSlot,
    sprite_lengths: &SpriteLengthTable,
    options: &LevelSaveOptions,
) -> Result<[PayloadSaveRequest; 2], LevelSaveError> {
    if layout.expanded_sprites != level.sprites.expanded {
        return Err(LevelSaveError::SpriteVariantMismatch {
            layout_expanded: layout.expanded_sprites,
            stream_expanded: level.sprites.expanded,
        });
    }
    let layer1_request = layer1_save_request(layout, level, options)?;
    let sprites = level.sprites.encode_for_table(sprite_lengths)?;
    let reparsed_sprites =
        NativeSpriteStream::parse(&sprites, level.sprites.expanded, sprite_lengths)
            .map_err(LevelSaveError::SpriteParse)?;
    if reparsed_sprites != level.sprites {
        return Err(LevelSaveError::NonCanonicalSpriteEncoding);
    }
    if sprites.len() > 0x8000 {
        return Err(LevelSaveError::SpriteBankLimitExceeded(sprites.len()));
    }
    let sprite_pointer = match layout.sprites {
        crate::SpritePointerTable::Contiguous(table) => {
            crate::PayloadPointer::contiguous(table.pointer_offset(level.number)?)
        }
        crate::SpritePointerTable::SplitSharedBank {
            low_words,
            bank_offset,
        } => crate::PayloadPointer::Split {
            low_word_offset: low_words.pointer_offset_16(level.number)?,
            bank_offset,
            shared_bank: true,
        },
        crate::SpritePointerTable::SplitBankTable { low_words, banks } => {
            crate::PayloadPointer::Split {
                low_word_offset: low_words.pointer_offset_16(level.number)?,
                bank_offset: banks.pointer_offset_8(level.number)?,
                shared_bank: false,
            }
        }
    };
    Ok([
        layer1_request,
        PayloadSaveRequest {
            description: format!("save level {:03x} sprites", level.number),
            payload: sprites,
            pointer: sprite_pointer,
            mapper: layout.mapper,
            allocation_policy: options.sprite_allocation.clone(),
            previous_block: options.previous_sprites.clone(),
            reuse_identical: options.reuse_identical,
            maximum_payload_len: 0x8000,
            erase_fill: options.erase_fill,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LevelPointerTable, LoadedLevelSlot};
    use lm_level::{
        LevelObjectData, NativeSpriteStream, ObjectRecord, SpriteLengthTable, SpriteRecord,
        SpriteToken,
    };
    use lm_rats::ProtectedRange;
    use lm_rom::{Mapper, RomImage};

    fn layout() -> LevelRomLayout {
        LevelRomLayout {
            mapper: Mapper::LoRom,
            layer1: LevelPointerTable {
                offset: 0x20,
                entries: 1,
                stride: 3,
            },
            sprites: LevelPointerTable {
                offset: 0x30,
                entries: 1,
                stride: 3,
            }
            .into(),
            expanded_sprites: false,
        }
    }

    fn policy() -> AllocationPolicy {
        AllocationPolicy {
            search: 0x100..0x8000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(0x20..0x33)],
        }
    }

    fn level() -> LoadedLevelSlot {
        LoadedLevelSlot {
            number: 0,
            layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]).unwrap(),
            sprites: NativeSpriteStream::parse(
                &[0x10, 0x00, 0x20, 0x01, 0xff],
                false,
                &SpriteLengthTable::standard(),
            )
            .unwrap(),
        }
    }

    #[test]
    fn saves_and_reloads_both_streams_as_one_edit() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let options = LevelSaveOptions {
            layer1_allocation: policy(),
            sprite_allocation: policy(),
            previous_layer1: None,
            previous_sprites: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let saved = project
            .save_level_slot(layout(), &level(), &SpriteLengthTable::standard(), &options)
            .unwrap();
        assert_ne!(saved.layer1.block, saved.sprites.block);
        assert_eq!(
            project
                .load_level_slot(0, layout(), &SpriteLengthTable::standard())
                .unwrap(),
            level()
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }

    #[test]
    fn second_allocation_failure_leaves_everything_unchanged() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let options = LevelSaveOptions {
            layer1_allocation: policy(),
            sprite_allocation: AllocationPolicy {
                search: 0x40..0x41,
                ..policy()
            },
            previous_layer1: None,
            previous_sprites: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        assert!(
            project
                .save_level_slot(layout(), &level(), &SpriteLengthTable::standard(), &options)
                .is_err()
        );
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }

    #[test]
    fn split_bank_table_level_save_reopens_and_undoes_atomically() {
        let split_layout = LevelRomLayout {
            sprites: crate::SpritePointerTable::SplitBankTable {
                low_words: LevelPointerTable {
                    offset: 0x30,
                    entries: 1,
                    stride: 2,
                },
                banks: LevelPointerTable {
                    offset: 0x40,
                    entries: 1,
                    stride: 1,
                },
            },
            ..layout()
        };
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let split_policy = AllocationPolicy {
            protected: vec![
                ProtectedRange(0x20..0x23),
                ProtectedRange(0x30..0x32),
                ProtectedRange(0x40..0x41),
            ],
            ..policy()
        };
        let options = LevelSaveOptions {
            layer1_allocation: split_policy.clone(),
            sprite_allocation: split_policy,
            previous_layer1: None,
            previous_sprites: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        project
            .save_level_slot(
                split_layout,
                &level(),
                &SpriteLengthTable::standard(),
                &options,
            )
            .unwrap();
        assert_eq!(
            project
                .load_level_slot(0, split_layout, &SpriteLengthTable::standard())
                .unwrap(),
            level()
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn invalid_public_sprite_model_cannot_reach_native_persistence() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let mut invalid = level();
        invalid.sprites.tokens.push(SpriteToken::Control(0xfe));
        let options = LevelSaveOptions {
            layer1_allocation: policy(),
            sprite_allocation: policy(),
            previous_layer1: None,
            previous_sprites: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        assert!(matches!(
            project.save_level_slot(layout(), &invalid, &SpriteLengthTable::standard(), &options,),
            Err(LevelSaveError::SpriteEncoding(
                NativeSpriteEncodingError::LegacyControlToken { .. }
            ))
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }

    #[test]
    fn revision_noncanonical_records_fail_before_allocation() {
        let options = LevelSaveOptions {
            layer1_allocation: policy(),
            sprite_allocation: policy(),
            previous_layer1: None,
            previous_sprites: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };

        let mut invalid_object = level();
        invalid_object.layer1.objects.records[0] = ObjectRecord::new(vec![0, 0, 0]).unwrap();
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        assert!(matches!(
            project.save_level_slot(
                layout(),
                &invalid_object,
                &SpriteLengthTable::standard(),
                &options,
            ),
            Err(LevelSaveError::Objects(_) | LevelSaveError::NonCanonicalObjectEncoding)
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());

        let mut invalid_sprite = level();
        invalid_sprite.sprites.tokens[0] = SpriteToken::Record(SpriteRecord {
            encoded: vec![0, 0x20, 1, 2],
        });
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        assert!(matches!(
            project.save_level_slot(
                layout(),
                &invalid_sprite,
                &SpriteLengthTable::standard(),
                &options,
            ),
            Err(LevelSaveError::SpriteEncoding(
                NativeSpriteEncodingError::RecordLengthMismatch {
                    token: 0,
                    expected: 3,
                    actual: 4,
                }
            ))
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }
}
