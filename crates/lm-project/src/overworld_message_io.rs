use crate::{
    LevelLoadError, LevelPointerTable, PayloadLoadError, PayloadReadPolicy, PayloadSaveError,
    PayloadSaveRequest, PayloadSaveResult, Project,
};
use lm_overworld::{FixedTableEncodingError, OverworldMessage};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessageRomLayout {
    pub mapper: Mapper,
    pub pointers: LevelPointerTable,
    pub messages_per_slot: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageSaveOptions {
    pub allocation: AllocationPolicy,
    pub previous_block: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Debug)]
pub enum MessageIoError {
    Layout(LevelLoadError),
    SizeOverflow,
    MessageCount { actual: usize, expected: usize },
    Load(PayloadLoadError),
    Decode(usize),
    Encode(FixedTableEncodingError),
    Save(PayloadSaveError),
}

impl fmt::Display for MessageIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "overworld message I/O failed: {self:?}")
    }
}

impl std::error::Error for MessageIoError {}

impl From<LevelLoadError> for MessageIoError {
    fn from(value: LevelLoadError) -> Self {
        Self::Layout(value)
    }
}
impl From<PayloadLoadError> for MessageIoError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}
impl From<PayloadSaveError> for MessageIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}
impl From<FixedTableEncodingError> for MessageIoError {
    fn from(value: FixedTableEncodingError) -> Self {
        Self::Encode(value)
    }
}

impl Project {
    /// Loads one fixed-count overworld message table.
    ///
    /// # Errors
    ///
    /// Returns [`MessageIoError`] for invalid sizes, pointers, payloads, or message records.
    pub fn load_overworld_messages(
        &self,
        slot: usize,
        layout: MessageRomLayout,
    ) -> Result<Vec<OverworldMessage>, MessageIoError> {
        let len = encoded_len(layout)?;
        let payload = self.load_payload(
            layout.pointers.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len },
        )?;
        OverworldMessage::decode_all(&payload.bytes).map_err(MessageIoError::Decode)
    }

    /// Saves one fixed-count message table transactionally.
    ///
    /// # Errors
    ///
    /// Returns [`MessageIoError`] for count, layout, allocation, or mapper failures.
    pub fn save_overworld_messages(
        &mut self,
        slot: usize,
        messages: &[OverworldMessage],
        layout: MessageRomLayout,
        options: &MessageSaveOptions,
    ) -> Result<PayloadSaveResult, MessageIoError> {
        Ok(self.save_tagged_payload(&message_save_request(slot, messages, layout, options)?)?)
    }
}

pub(crate) fn message_save_request(
    slot: usize,
    messages: &[OverworldMessage],
    layout: MessageRomLayout,
    options: &MessageSaveOptions,
) -> Result<PayloadSaveRequest, MessageIoError> {
    if messages.len() != layout.messages_per_slot {
        return Err(MessageIoError::MessageCount {
            actual: messages.len(),
            expected: layout.messages_per_slot,
        });
    }
    Ok(PayloadSaveRequest {
        description: format!("save overworld messages {slot:02x}"),
        payload: OverworldMessage::encode_all(messages)?,
        pointer: layout.pointers.pointer_offset(slot)?.into(),
        mapper: layout.mapper,
        allocation_policy: options.allocation.clone(),
        previous_block: options.previous_block.clone(),
        reuse_identical: options.reuse_identical,
        maximum_payload_len: encoded_len(layout)?,
        erase_fill: options.erase_fill,
    })
}

fn encoded_len(layout: MessageRomLayout) -> Result<usize, MessageIoError> {
    layout
        .messages_per_slot
        .checked_mul(OverworldMessage::ENCODED_LEN)
        .ok_or(MessageIoError::SizeOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::ProtectedRange;
    use lm_rom::RomImage;

    fn layout() -> MessageRomLayout {
        MessageRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x20,
                entries: 1,
                stride: 3,
            },
            messages_per_slot: 2,
        }
    }
    fn messages() -> Vec<OverworldMessage> {
        vec![
            OverworldMessage::decode(&[0x11; OverworldMessage::ENCODED_LEN]).unwrap(),
            OverworldMessage::decode(&[0x22; OverworldMessage::ENCODED_LEN]).unwrap(),
        ]
    }
    fn options() -> MessageSaveOptions {
        MessageSaveOptions {
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
    fn messages_save_load_and_undo() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        project
            .save_overworld_messages(0, &messages(), layout(), &options())
            .unwrap();
        assert_eq!(
            project.load_overworld_messages(0, layout()).unwrap(),
            messages()
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn wrong_count_is_atomic() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        assert!(matches!(
            project.save_overworld_messages(0, &messages()[..1], layout(), &options()),
            Err(MessageIoError::MessageCount {
                actual: 1,
                expected: 2
            })
        ));
        assert_eq!(project.save_snapshot(), original);
    }
}
