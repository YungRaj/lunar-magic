//! Native compressed overworld event-tilemap stream detection and persistence.

use crate::{
    PayloadLoadError, PayloadPointer, PayloadReadPolicy, PayloadSaveError, PayloadSaveRequest,
    Project, RelocatablePatchError, RelocatablePatchPlan,
};
use lm_codec::{CodecError, decode_lz2, decode_lz3, encode_lz2, encode_lz3};
use lm_overworld::{EventTilemapBufferError, EventTilemapBuffers};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::{Mapper, RomError, SnesPointer24};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventTilemapCompression {
    Lz2,
    Lz3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventTilemapPatchLocator {
    pub mapper: Mapper,
    pub loader_marker: usize,
    pub secondary_marker: usize,
    pub primary_low_word: usize,
    pub primary_bank: usize,
    pub secondary_low_word: usize,
    pub secondary_bank: usize,
    pub primary_runtime: [u8; 64],
    pub index_hook: usize,
    pub index_hook_bytes: [u8; 4],
    pub index_runtime: usize,
    pub index_runtime_bytes: [u8; 32],
    pub reveal_hook: usize,
    pub reveal_hook_bytes: [u8; 5],
    pub reveal_runtime: usize,
    pub reveal_runtime_bytes: [u8; 48],
    pub reveal_opcode: usize,
    pub reveal_opcode_byte: u8,
    pub state_hook: usize,
    pub state_hook_bytes: [u8; 4],
    pub state_runtime: usize,
    pub state_runtime_bytes: [u8; 160],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedEventTilemapBuffers {
    pub buffers: EventTilemapBuffers,
    pub primary_block: RatsBlock,
    pub secondary_block: RatsBlock,
}

#[derive(Debug)]
pub enum EventTilemapPatchError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    Marker { offset: usize, actual: u8 },
    Runtime { offset: usize },
    Rom(RomError),
    Load(PayloadLoadError),
    Codec(CodecError),
    Buffers(EventTilemapBufferError),
    DecodedLength { stream: &'static str, actual: usize },
    MissingOwnership { stream: &'static str },
    Save(PayloadSaveError),
    Install(RelocatablePatchError),
    ReopenMismatch,
}

impl std::fmt::Display for EventTilemapPatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native event-tilemap patch failed: {self:?}")
    }
}

impl std::error::Error for EventTilemapPatchError {}

impl From<RomError> for EventTilemapPatchError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<PayloadLoadError> for EventTilemapPatchError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}

impl From<CodecError> for EventTilemapPatchError {
    fn from(value: CodecError) -> Self {
        Self::Codec(value)
    }
}

impl From<EventTilemapBufferError> for EventTilemapPatchError {
    fn from(value: EventTilemapBufferError) -> Self {
        Self::Buffers(value)
    }
}

impl From<PayloadSaveError> for EventTilemapPatchError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl From<RelocatablePatchError> for EventTilemapPatchError {
    fn from(value: RelocatablePatchError) -> Self {
        Self::Install(value)
    }
}

impl Project {
    /// Installs an identity-bound pristine loader plan and verifies both streams semantically.
    ///
    /// # Errors
    ///
    /// Rejects malformed plans, any fixed-byte precondition disagreement, allocation/fixup
    /// failure, or semantic disagreement after reopening the installed runtime.
    pub fn install_event_tilemap_buffers(
        &mut self,
        buffers: &EventTilemapBuffers,
        locator: EventTilemapPatchLocator,
        compression: EventTilemapCompression,
        plan: &RelocatablePatchPlan,
    ) -> Result<bool, EventTilemapPatchError> {
        if plan.mapper != locator.mapper || plan.payloads.len() != 2 {
            return Err(EventTilemapPatchError::Runtime {
                offset: locator.loader_marker,
            });
        }
        self.install_relocatable_patch(plan)?;
        if self
            .load_event_tilemap_buffers_detected(locator, compression)?
            .buffers
            != *buffers
        {
            return Err(EventTilemapPatchError::ReopenMismatch);
        }
        Ok(true)
    }

    /// Loads an installed Lunar Magic event-tilemap pair from its split pointers.
    ///
    /// # Errors
    ///
    /// Rejects mapper disagreement, missing `A2` loader markers, untagged streams, malformed
    /// compression, incorrect decoded extents, or malformed runtime fragments.
    pub fn load_event_tilemap_buffers_detected(
        &self,
        locator: EventTilemapPatchLocator,
        compression: EventTilemapCompression,
    ) -> Result<LoadedEventTilemapBuffers, EventTilemapPatchError> {
        validate_locator(self, &locator)?;
        let primary = load_split(
            self,
            locator.primary_low_word,
            locator.primary_bank,
            locator.mapper,
        )?;
        let secondary = load_split(
            self,
            locator.secondary_low_word,
            locator.secondary_bank,
            locator.mapper,
        )?;
        let primary_bytes = decode(
            compression,
            &primary.bytes,
            EventTilemapBuffers::PRIMARY_LEN,
        )?;
        let secondary_bytes = decode(
            compression,
            &secondary.bytes,
            EventTilemapBuffers::SECONDARY_HIGH_PLANE_LEN,
        )?;
        require_len("primary", &primary_bytes, EventTilemapBuffers::PRIMARY_LEN)?;
        require_len(
            "secondary-high",
            &secondary_bytes,
            EventTilemapBuffers::SECONDARY_HIGH_PLANE_LEN,
        )?;
        Ok(LoadedEventTilemapBuffers {
            buffers: EventTilemapBuffers::decode_streams(&primary_bytes, &secondary_bytes)?,
            primary_block: primary
                .block
                .ok_or(EventTilemapPatchError::MissingOwnership { stream: "primary" })?,
            secondary_block: secondary
                .block
                .ok_or(EventTilemapPatchError::MissingOwnership {
                    stream: "secondary-high",
                })?,
        })
    }

    /// Replaces both installed compressed streams as one checksum-valid transaction.
    ///
    /// # Errors
    ///
    /// Rejects invalid current ownership, compression failures, allocation/pointer failures, or
    /// semantic disagreement after reopening the staged result.
    pub fn save_event_tilemap_buffers_detected(
        &mut self,
        buffers: &EventTilemapBuffers,
        locator: EventTilemapPatchLocator,
        compression: EventTilemapCompression,
        allocation: &AllocationPolicy,
        checksum_field: usize,
        fill: u8,
    ) -> Result<bool, EventTilemapPatchError> {
        let loaded = self.load_event_tilemap_buffers_detected(locator, compression)?;
        if loaded.buffers == *buffers {
            return Ok(false);
        }
        let primary = encode(compression, &buffers.encode_primary_stream());
        let secondary = encode(compression, &buffers.encode_secondary_high_stream());
        let request =
            |description: &str, payload: Vec<u8>, low_word_offset, bank_offset, previous_block| {
                PayloadSaveRequest {
                    description: description.into(),
                    maximum_payload_len: EventTilemapBuffers::PRIMARY_LEN + 3,
                    payload,
                    pointer: PayloadPointer::Split {
                        low_word_offset,
                        bank_offset,
                        shared_bank: false,
                    },
                    mapper: locator.mapper,
                    allocation_policy: allocation.clone(),
                    previous_block: Some(previous_block),
                    reuse_identical: true,
                    erase_fill: fill,
                }
            };
        self.save_tagged_payloads_with_checksum(
            "save native overworld event tilemaps",
            &[
                request(
                    "primary overworld event tilemap",
                    primary,
                    locator.primary_low_word,
                    locator.primary_bank,
                    loaded.primary_block,
                ),
                request(
                    "secondary overworld event tilemap",
                    secondary,
                    locator.secondary_low_word,
                    locator.secondary_bank,
                    loaded.secondary_block,
                ),
            ],
            checksum_field,
        )?;
        if self
            .load_event_tilemap_buffers_detected(locator, compression)?
            .buffers
            != *buffers
        {
            return Err(EventTilemapPatchError::ReopenMismatch);
        }
        Ok(true)
    }
}

fn validate_locator(
    project: &Project,
    locator: &EventTilemapPatchLocator,
) -> Result<(), EventTilemapPatchError> {
    if let Some(identity) = &project.identity
        && identity.mapper != locator.mapper
    {
        return Err(EventTilemapPatchError::MapperMismatch {
            expected: identity.mapper,
            actual: locator.mapper,
        });
    }
    for offset in [locator.loader_marker, locator.secondary_marker] {
        let actual = project.rom.read(offset, 1)?[0];
        if actual != 0xa2 {
            return Err(EventTilemapPatchError::Marker { offset, actual });
        }
    }
    let primary = project.rom.read(locator.loader_marker, 64)?;
    for (index, expected) in locator.primary_runtime.iter().enumerate() {
        let pointer_operand = (0x0a..0x0c).contains(&index)
            || index == 0x0f
            || (0x29..0x2b).contains(&index)
            || index == 0x2e;
        if !pointer_operand && primary[index] != *expected {
            return Err(EventTilemapPatchError::Runtime {
                offset: locator.loader_marker,
            });
        }
    }
    for (offset, actual, expected) in [
        (
            locator.index_hook,
            project
                .rom
                .read(locator.index_hook, locator.index_hook_bytes.len())?,
            locator.index_hook_bytes.as_slice(),
        ),
        (
            locator.index_runtime,
            project
                .rom
                .read(locator.index_runtime, locator.index_runtime_bytes.len())?,
            locator.index_runtime_bytes.as_slice(),
        ),
        (
            locator.reveal_hook,
            project
                .rom
                .read(locator.reveal_hook, locator.reveal_hook_bytes.len())?,
            locator.reveal_hook_bytes.as_slice(),
        ),
        (
            locator.reveal_runtime,
            project
                .rom
                .read(locator.reveal_runtime, locator.reveal_runtime_bytes.len())?,
            locator.reveal_runtime_bytes.as_slice(),
        ),
        (
            locator.state_hook,
            project
                .rom
                .read(locator.state_hook, locator.state_hook_bytes.len())?,
            locator.state_hook_bytes.as_slice(),
        ),
        (
            locator.state_runtime,
            project
                .rom
                .read(locator.state_runtime, locator.state_runtime_bytes.len())?,
            locator.state_runtime_bytes.as_slice(),
        ),
    ] {
        if actual != expected {
            return Err(EventTilemapPatchError::Runtime { offset });
        }
    }
    if project.rom.read(locator.reveal_opcode, 1)?[0] != locator.reveal_opcode_byte {
        return Err(EventTilemapPatchError::Runtime {
            offset: locator.reveal_opcode,
        });
    }
    Ok(())
}

fn load_split(
    project: &Project,
    low_word: usize,
    bank: usize,
    mapper: Mapper,
) -> Result<crate::LoadedPayload, EventTilemapPatchError> {
    let low = project.rom.read(low_word, 2)?;
    let pointer = SnesPointer24::new(
        u32::from(low[0]) | u32::from(low[1]) << 8 | u32::from(project.rom.read(bank, 1)?[0]) << 16,
    )
    .map_err(|_| RomError::RangeOutOfBounds {
        offset: low_word,
        len: 2,
        image_len: project.rom.logical_len(),
    })?;
    Ok(project.load_payload_from_pointer(pointer, mapper, &PayloadReadPolicy::Tagged)?)
}

fn decode(
    compression: EventTilemapCompression,
    bytes: &[u8],
    limit: usize,
) -> Result<Vec<u8>, CodecError> {
    match compression {
        EventTilemapCompression::Lz2 => decode_lz2(bytes, limit),
        EventTilemapCompression::Lz3 => decode_lz3(bytes, limit),
    }
}

fn encode(compression: EventTilemapCompression, bytes: &[u8]) -> Vec<u8> {
    match compression {
        EventTilemapCompression::Lz2 => encode_lz2(bytes),
        EventTilemapCompression::Lz3 => encode_lz3(bytes),
    }
}

fn require_len(
    stream: &'static str,
    bytes: &[u8],
    expected: usize,
) -> Result<(), EventTilemapPatchError> {
    if bytes.len() == expected {
        Ok(())
    } else {
        Err(EventTilemapPatchError::DecodedLength {
            stream,
            actual: bytes.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::FreeSpaceAllocator;
    use lm_rom::{RomImage, pc_to_snes};

    const LOCATOR: EventTilemapPatchLocator = EventTilemapPatchLocator {
        mapper: Mapper::LoRom,
        loader_marker: 0x20,
        secondary_marker: 0x3f,
        primary_low_word: 0x2a,
        primary_bank: 0x2f,
        secondary_low_word: 0x49,
        secondary_bank: 0x4e,
        primary_runtime: {
            let mut bytes = [0; 64];
            bytes[0] = 0xa2;
            bytes[0x1f] = 0xa2;
            bytes
        },
        index_hook: 0x80,
        index_hook_bytes: [1; 4],
        index_runtime: 0x90,
        index_runtime_bytes: [2; 32],
        reveal_hook: 0xc0,
        reveal_hook_bytes: [3; 5],
        reveal_runtime: 0xd0,
        reveal_runtime_bytes: [4; 48],
        reveal_opcode: 0x110,
        reveal_opcode_byte: 5,
        state_hook: 0x120,
        state_hook_bytes: [6; 4],
        state_runtime: 0x130,
        state_runtime_bytes: [7; 160],
    };

    fn fixture(compression: EventTilemapCompression) -> (Project, EventTilemapBuffers) {
        let mut buffers = EventTilemapBuffers::default();
        buffers.primary_bytes_mut()[3] = 0x12;
        buffers.primary_bytes_mut()[0x803] = 0x34;
        buffers.secondary_high_bytes_mut()[3] = 0xab;
        let encode_stream = |bytes: &[u8]| encode(compression, bytes);
        let primary = encode_stream(&buffers.encode_primary_stream());
        let secondary = encode_stream(&buffers.encode_secondary_high_stream());
        let mut bytes = vec![0xff; 0x20_000];
        bytes[LOCATOR.loader_marker] = 0xa2;
        bytes[LOCATOR.secondary_marker] = 0xa2;
        bytes[LOCATOR.loader_marker..LOCATOR.loader_marker + 64]
            .copy_from_slice(&LOCATOR.primary_runtime);
        for (offset, runtime) in [
            (LOCATOR.index_hook, LOCATOR.index_hook_bytes.as_slice()),
            (
                LOCATOR.index_runtime,
                LOCATOR.index_runtime_bytes.as_slice(),
            ),
            (LOCATOR.reveal_hook, LOCATOR.reveal_hook_bytes.as_slice()),
            (
                LOCATOR.reveal_runtime,
                LOCATOR.reveal_runtime_bytes.as_slice(),
            ),
            (LOCATOR.state_hook, LOCATOR.state_hook_bytes.as_slice()),
            (
                LOCATOR.state_runtime,
                LOCATOR.state_runtime_bytes.as_slice(),
            ),
        ] {
            bytes[offset..offset + runtime.len()].copy_from_slice(runtime);
        }
        bytes[LOCATOR.reveal_opcode] = LOCATOR.reveal_opcode_byte;
        let policy = AllocationPolicy::lorom(0x1000..0x18_000);
        let (primary_block, secondary_block) = {
            let mut allocator = FreeSpaceAllocator::new(&mut bytes, policy);
            (
                allocator.allocate(&primary).unwrap(),
                allocator.allocate(&secondary).unwrap(),
            )
        };
        for (block, low, bank) in [
            (
                primary_block,
                LOCATOR.primary_low_word,
                LOCATOR.primary_bank,
            ),
            (
                secondary_block,
                LOCATOR.secondary_low_word,
                LOCATOR.secondary_bank,
            ),
        ] {
            let pointer = pc_to_snes(Mapper::LoRom, block.payload.start)
                .unwrap()
                .to_le_bytes();
            bytes[low..low + 2].copy_from_slice(&pointer[..2]);
            bytes[bank] = pointer[2];
        }
        (Project::new(RomImage::from_bytes(bytes).unwrap()), buffers)
    }

    #[test]
    fn detects_both_recovered_compression_modes_and_split_pointer_owners() {
        for compression in [EventTilemapCompression::Lz2, EventTilemapCompression::Lz3] {
            let (project, expected) = fixture(compression);
            assert_eq!(
                project
                    .load_event_tilemap_buffers_detected(LOCATOR, compression)
                    .unwrap()
                    .buffers,
                expected
            );
        }
    }

    #[test]
    fn exact_markers_are_required() {
        let (mut project, _) = fixture(EventTilemapCompression::Lz2);
        project
            .rom
            .write(LOCATOR.secondary_marker, &[0xea])
            .unwrap();
        assert!(matches!(
            project.load_event_tilemap_buffers_detected(LOCATOR, EventTilemapCompression::Lz2,),
            Err(EventTilemapPatchError::Marker { .. })
        ));
    }
}
