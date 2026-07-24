use crate::{
    LevelLoadError, LevelPointerTable, PayloadLoadError, PayloadReadPolicy, PayloadSaveError,
    PayloadSaveRequest, PayloadSaveResult, Project,
};
use lm_overworld::{OverworldSprite, OverworldSpriteError};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpriteRomLayout {
    pub mapper: Mapper,
    pub pointers: LevelPointerTable,
    pub sprites_per_slot: usize,
    pub record_len: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpriteSaveOptions {
    pub allocation: AllocationPolicy,
    pub previous_block: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Debug)]
pub enum SpriteIoError {
    Layout(LevelLoadError),
    SizeOverflow,
    SpriteCount { actual: usize, expected: usize },
    Load(PayloadLoadError),
    Codec(OverworldSpriteError),
    Save(PayloadSaveError),
}

impl fmt::Display for SpriteIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "overworld sprite I/O failed: {self:?}")
    }
}
impl std::error::Error for SpriteIoError {}
impl From<LevelLoadError> for SpriteIoError {
    fn from(value: LevelLoadError) -> Self {
        Self::Layout(value)
    }
}
impl From<PayloadLoadError> for SpriteIoError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}
impl From<PayloadSaveError> for SpriteIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}
impl From<OverworldSpriteError> for SpriteIoError {
    fn from(value: OverworldSpriteError) -> Self {
        Self::Codec(value)
    }
}

impl Project {
    /// Loads fixed-size overworld sprite records while preserving record extensions.
    ///
    /// # Errors
    ///
    /// Returns [`SpriteIoError`] for invalid layouts, pointers, or sprite records.
    pub fn load_overworld_sprites(
        &self,
        slot: usize,
        layout: SpriteRomLayout,
    ) -> Result<Vec<OverworldSprite>, SpriteIoError> {
        let len = encoded_len(layout)?;
        let payload = self.load_payload(
            layout.pointers.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len },
        )?;
        Ok(OverworldSprite::decode_all(
            &payload.bytes,
            layout.record_len,
        )?)
    }

    /// Saves fixed-size overworld sprite records transactionally.
    ///
    /// # Errors
    ///
    /// Returns [`SpriteIoError`] for count, record, allocation, or mapper failures.
    pub fn save_overworld_sprites(
        &mut self,
        slot: usize,
        sprites: &[OverworldSprite],
        layout: SpriteRomLayout,
        options: &SpriteSaveOptions,
    ) -> Result<PayloadSaveResult, SpriteIoError> {
        Ok(self.save_tagged_payload(&sprite_save_request(slot, sprites, layout, options)?)?)
    }
}

pub(crate) fn sprite_save_request(
    slot: usize,
    sprites: &[OverworldSprite],
    layout: SpriteRomLayout,
    options: &SpriteSaveOptions,
) -> Result<PayloadSaveRequest, SpriteIoError> {
    if sprites.len() != layout.sprites_per_slot {
        return Err(SpriteIoError::SpriteCount {
            actual: sprites.len(),
            expected: layout.sprites_per_slot,
        });
    }
    Ok(PayloadSaveRequest {
        description: format!("save overworld sprites {slot:02x}"),
        payload: OverworldSprite::encode_all(sprites, layout.record_len)?,
        pointer: layout.pointers.pointer_offset(slot)?.into(),
        mapper: layout.mapper,
        allocation_policy: options.allocation.clone(),
        previous_block: options.previous_block.clone(),
        reuse_identical: options.reuse_identical,
        maximum_payload_len: encoded_len(layout)?,
        erase_fill: options.erase_fill,
    })
}

fn encoded_len(layout: SpriteRomLayout) -> Result<usize, SpriteIoError> {
    if layout.record_len < OverworldSprite::OWNED_LEN {
        return Err(SpriteIoError::Codec(OverworldSpriteError::RecordTooShort(
            layout.record_len,
        )));
    }
    layout
        .sprites_per_slot
        .checked_mul(layout.record_len)
        .ok_or(SpriteIoError::SizeOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::Submap;
    use lm_rats::ProtectedRange;
    use lm_rom::RomImage;

    fn layout() -> SpriteRomLayout {
        SpriteRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x20,
                entries: 1,
                stride: 3,
            },
            sprites_per_slot: 2,
            record_len: 9,
        }
    }
    fn sprites() -> Vec<OverworldSprite> {
        vec![
            OverworldSprite {
                id: 1,
                x: 2,
                y: 3,
                submap: Submap::Main,
                extra: vec![0xaa, 0xbb],
            },
            OverworldSprite {
                id: 4,
                x: 5,
                y: 6,
                submap: Submap::StarWorld,
                extra: vec![0xcc, 0xdd],
            },
        ]
    }
    fn options() -> SpriteSaveOptions {
        SpriteSaveOptions {
            allocation: AllocationPolicy {
                search: 0x100..0x8000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![ProtectedRange(0x20..0x23)],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    #[test]
    fn sprites_save_load_preserve_extensions_and_undo() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        project
            .save_overworld_sprites(0, &sprites(), layout(), &options())
            .unwrap();
        assert_eq!(
            project.load_overworld_sprites(0, layout()).unwrap(),
            sprites()
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn extension_mismatch_is_atomic() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let mut invalid = sprites();
        invalid[1].extra.clear();
        assert!(matches!(
            project.save_overworld_sprites(0, &invalid, layout(), &options()),
            Err(SpriteIoError::Codec(OverworldSpriteError::ExtraLength {
                record: 1,
                ..
            }))
        ));
        assert_eq!(project.save_snapshot(), original);
    }
}
