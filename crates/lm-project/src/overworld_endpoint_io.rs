use crate::{
    LevelLoadError, LevelPointerTable, PayloadLoadError, PayloadReadPolicy, PayloadSaveError,
    PayloadSaveRequest, PayloadSaveResult, Project,
};
use lm_overworld::{FixedTableEncodingError, OverworldEndpoint};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EndpointRomLayout {
    pub mapper: Mapper,
    pub pointers: LevelPointerTable,
    pub endpoints_per_slot: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointSaveOptions {
    pub allocation: AllocationPolicy,
    pub previous_block: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Debug)]
pub enum EndpointIoError {
    Layout(LevelLoadError),
    SizeOverflow,
    EndpointCount { actual: usize, expected: usize },
    Load(PayloadLoadError),
    Decode(usize),
    Encode(FixedTableEncodingError),
    Save(PayloadSaveError),
}

impl fmt::Display for EndpointIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "overworld endpoint I/O failed: {self:?}")
    }
}

impl std::error::Error for EndpointIoError {}

impl From<LevelLoadError> for EndpointIoError {
    fn from(value: LevelLoadError) -> Self {
        Self::Layout(value)
    }
}
impl From<PayloadLoadError> for EndpointIoError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}
impl From<PayloadSaveError> for EndpointIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}
impl From<FixedTableEncodingError> for EndpointIoError {
    fn from(value: FixedTableEncodingError) -> Self {
        Self::Encode(value)
    }
}

impl Project {
    /// Loads a fixed-shape overworld path/warp endpoint table.
    ///
    /// # Errors
    ///
    /// Returns [`EndpointIoError`] for invalid sizes, tables, pointers, or packed records.
    pub fn load_overworld_endpoints(
        &self,
        slot: usize,
        layout: EndpointRomLayout,
    ) -> Result<Vec<OverworldEndpoint>, EndpointIoError> {
        let encoded_len = endpoint_len(layout)?;
        let payload = self.load_payload(
            layout.pointers.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: encoded_len },
        )?;
        OverworldEndpoint::decode_all(&payload.bytes).map_err(EndpointIoError::Decode)
    }

    /// Transactionally saves one fixed-shape endpoint table.
    ///
    /// # Errors
    ///
    /// Returns [`EndpointIoError`] for count, layout, allocation, or mapper failures.
    pub fn save_overworld_endpoints(
        &mut self,
        slot: usize,
        endpoints: &[OverworldEndpoint],
        layout: EndpointRomLayout,
        options: &EndpointSaveOptions,
    ) -> Result<PayloadSaveResult, EndpointIoError> {
        Ok(self.save_tagged_payload(&endpoint_save_request(slot, endpoints, layout, options)?)?)
    }
}

pub(crate) fn endpoint_save_request(
    slot: usize,
    endpoints: &[OverworldEndpoint],
    layout: EndpointRomLayout,
    options: &EndpointSaveOptions,
) -> Result<PayloadSaveRequest, EndpointIoError> {
    if endpoints.len() != layout.endpoints_per_slot {
        return Err(EndpointIoError::EndpointCount {
            actual: endpoints.len(),
            expected: layout.endpoints_per_slot,
        });
    }
    Ok(PayloadSaveRequest {
        description: format!("save overworld endpoints {slot:02x}"),
        payload: OverworldEndpoint::encode_all(endpoints)?,
        pointer: layout.pointers.pointer_offset(slot)?.into(),
        mapper: layout.mapper,
        allocation_policy: options.allocation.clone(),
        previous_block: options.previous_block.clone(),
        reuse_identical: options.reuse_identical,
        maximum_payload_len: endpoint_len(layout)?,
        erase_fill: options.erase_fill,
    })
}

fn endpoint_len(layout: EndpointRomLayout) -> Result<usize, EndpointIoError> {
    layout
        .endpoints_per_slot
        .checked_mul(OverworldEndpoint::ENCODED_LEN)
        .ok_or(EndpointIoError::SizeOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::ProtectedRange;
    use lm_rom::RomImage;

    fn layout() -> EndpointRomLayout {
        EndpointRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x20,
                entries: 1,
                stride: 3,
            },
            endpoints_per_slot: 2,
        }
    }

    fn endpoints() -> Vec<OverworldEndpoint> {
        vec![
            OverworldEndpoint {
                x: 1,
                y: 2,
                submap: 3,
            },
            OverworldEndpoint {
                x: 0x1234,
                y: 0xabcd,
                submap: 6,
            },
        ]
    }

    fn options() -> EndpointSaveOptions {
        EndpointSaveOptions {
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
    fn endpoint_table_save_load_and_undo_round_trip() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        project
            .save_overworld_endpoints(0, &endpoints(), layout(), &options())
            .unwrap();
        assert_eq!(
            project.load_overworld_endpoints(0, layout()).unwrap(),
            endpoints()
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn pristine_fixed_endpoint_table_loads() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x81, 0x80]);
        bytes[0x100..0x10a].copy_from_slice(&OverworldEndpoint::encode_all(&endpoints()).unwrap());
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert_eq!(
            project.load_overworld_endpoints(0, layout()).unwrap(),
            endpoints()
        );
    }

    #[test]
    fn wrong_endpoint_count_preserves_project() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        assert!(matches!(
            project.save_overworld_endpoints(0, &endpoints()[..1], layout(), &options()),
            Err(EndpointIoError::EndpointCount {
                actual: 1,
                expected: 2
            })
        ));
        assert_eq!(project.save_snapshot(), original);
    }
}
