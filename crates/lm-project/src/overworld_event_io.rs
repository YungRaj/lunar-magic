use crate::{
    LevelLoadError, LevelPointerTable, PayloadLoadError, PayloadReadPolicy, PayloadSaveError,
    PayloadSaveRequest, PayloadSaveResult, Project,
};
use lm_overworld::{EventRevealTable, EventTableError};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventRevealRomLayout {
    pub mapper: Mapper,
    pub sources: LevelPointerTable,
    pub destinations: LevelPointerTable,
    pub entries_per_slot: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventRevealSaveOptions {
    pub source_allocation: AllocationPolicy,
    pub destination_allocation: AllocationPolicy,
    pub previous_sources: Option<RatsBlock>,
    pub previous_destinations: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedEventRevealTable {
    pub sources: PayloadSaveResult,
    pub destinations: PayloadSaveResult,
}

#[derive(Debug)]
pub enum EventRevealIoError {
    Layout(LevelLoadError),
    SizeOverflow,
    EntryCount { actual: usize, expected: usize },
    Load(PayloadLoadError),
    Decode(EventTableError),
    Save(PayloadSaveError),
}

impl fmt::Display for EventRevealIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "overworld event-reveal I/O failed: {self:?}")
    }
}

impl std::error::Error for EventRevealIoError {}

impl From<LevelLoadError> for EventRevealIoError {
    fn from(value: LevelLoadError) -> Self {
        Self::Layout(value)
    }
}

impl From<PayloadLoadError> for EventRevealIoError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}

impl From<EventTableError> for EventRevealIoError {
    fn from(value: EventTableError) -> Self {
        Self::Decode(value)
    }
}

impl From<PayloadSaveError> for EventRevealIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl Project {
    /// Loads the source and destination planes for one overworld event-reveal slot.
    ///
    /// # Errors
    ///
    /// Returns [`EventRevealIoError`] for invalid sizes, tables, pointers, or plane data.
    pub fn load_event_reveals(
        &self,
        slot: usize,
        layout: EventRevealRomLayout,
    ) -> Result<EventRevealTable, EventRevealIoError> {
        let plane_len = plane_len(layout)?;
        let sources = self.load_payload(
            layout.sources.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: plane_len },
        )?;
        let destinations = self.load_payload(
            layout.destinations.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: plane_len },
        )?;
        Ok(EventRevealTable::decode(
            &sources.bytes,
            &destinations.bytes,
        )?)
    }

    /// Saves both reveal planes atomically, preserving their different byte orders.
    ///
    /// # Errors
    ///
    /// Returns [`EventRevealIoError`] for entry-count, layout, allocation, or mapping failures.
    pub fn save_event_reveals(
        &mut self,
        slot: usize,
        table: &EventRevealTable,
        layout: EventRevealRomLayout,
        options: &EventRevealSaveOptions,
    ) -> Result<SavedEventRevealTable, EventRevealIoError> {
        let requests = event_save_requests(slot, table, layout, options)?;
        let mut results = self
            .save_tagged_payloads(format!("save complete event reveals {slot:02x}"), &requests)?;
        Ok(SavedEventRevealTable {
            sources: results.remove(0),
            destinations: results.remove(0),
        })
    }
}

pub(crate) fn event_save_requests(
    slot: usize,
    table: &EventRevealTable,
    layout: EventRevealRomLayout,
    options: &EventRevealSaveOptions,
) -> Result<[PayloadSaveRequest; 2], EventRevealIoError> {
    if table.entries.len() != layout.entries_per_slot {
        return Err(EventRevealIoError::EntryCount {
            actual: table.entries.len(),
            expected: layout.entries_per_slot,
        });
    }
    let maximum_payload_len = plane_len(layout)?;
    let (sources, destinations) = table.encode()?;
    Ok([
        PayloadSaveRequest {
            description: format!("save event reveals {slot:02x} sources"),
            payload: sources,
            pointer: layout.sources.pointer_offset(slot)?.into(),
            mapper: layout.mapper,
            allocation_policy: options.source_allocation.clone(),
            previous_block: options.previous_sources.clone(),
            reuse_identical: options.reuse_identical,
            maximum_payload_len,
            erase_fill: options.erase_fill,
        },
        PayloadSaveRequest {
            description: format!("save event reveals {slot:02x} destinations"),
            payload: destinations,
            pointer: layout.destinations.pointer_offset(slot)?.into(),
            mapper: layout.mapper,
            allocation_policy: options.destination_allocation.clone(),
            previous_block: options.previous_destinations.clone(),
            reuse_identical: options.reuse_identical,
            maximum_payload_len,
            erase_fill: options.erase_fill,
        },
    ])
}

fn plane_len(layout: EventRevealRomLayout) -> Result<usize, EventRevealIoError> {
    if layout.entries_per_slot > EventRevealTable::MAX_ENTRIES {
        return Err(EventRevealIoError::EntryCount {
            actual: layout.entries_per_slot,
            expected: EventRevealTable::MAX_ENTRIES,
        });
    }
    layout
        .entries_per_slot
        .checked_mul(2)
        .ok_or(EventRevealIoError::SizeOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::EventReveal;
    use lm_rats::ProtectedRange;
    use lm_rom::RomImage;

    fn layout() -> EventRevealRomLayout {
        EventRevealRomLayout {
            mapper: Mapper::LoRom,
            sources: LevelPointerTable {
                offset: 0x20,
                entries: 1,
                stride: 3,
            },
            destinations: LevelPointerTable {
                offset: 0x30,
                entries: 1,
                stride: 3,
            },
            entries_per_slot: 2,
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

    fn options() -> EventRevealSaveOptions {
        EventRevealSaveOptions {
            source_allocation: policy(),
            destination_allocation: policy(),
            previous_sources: None,
            previous_destinations: None,
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    fn table() -> EventRevealTable {
        EventRevealTable {
            entries: vec![
                EventReveal {
                    source_tile: 0x123,
                    destination_tile: 0x456,
                },
                EventReveal {
                    source_tile: 0x7ff,
                    destination_tile: 0x789,
                },
            ],
        }
    }

    #[test]
    fn reveal_planes_save_load_and_undo_together() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        project
            .save_event_reveals(0, &table(), layout(), &options())
            .unwrap();
        assert_eq!(project.load_event_reveals(0, layout()).unwrap(), table());
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn wrong_entry_count_cannot_partially_save() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let short = EventRevealTable {
            entries: table().entries[..1].to_vec(),
        };
        assert!(matches!(
            project.save_event_reveals(0, &short, layout(), &options()),
            Err(EventRevealIoError::EntryCount {
                actual: 1,
                expected: 2
            })
        ));
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn source_that_would_normalize_on_reopen_cannot_save() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let mut invalid = table();
        invalid.entries[1].source_tile = EventRevealTable::MAX_TILE + 1;
        assert!(matches!(
            project.save_event_reveals(0, &invalid, layout(), &options()),
            Err(EventRevealIoError::Decode(
                EventTableError::InvalidSourceTile {
                    index: 1,
                    tile: 0x800
                }
            ))
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }

    #[test]
    fn pristine_planes_load_with_native_endianness() {
        let (sources, destinations) = table().encode().unwrap();
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x81, 0x80]);
        bytes[0x30..0x33].copy_from_slice(&[0x20, 0x81, 0x80]);
        bytes[0x100..0x104].copy_from_slice(&sources);
        bytes[0x120..0x124].copy_from_slice(&destinations);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert_eq!(project.load_event_reveals(0, layout()).unwrap(), table());
    }
}
