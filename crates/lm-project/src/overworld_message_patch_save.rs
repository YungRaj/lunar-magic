//! Transactional replacement of Lunar Magic's expanded overworld-message allocations.

use crate::{
    ExpandedOverworldMessageStorage, OverworldMessagePatchLocator, Project,
    payload::staging::commit_staged,
};
use lm_overworld::OverworldMessage;
use lm_rats::{AllocationError, AllocationPolicy, FreeSpaceAllocator, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, RomImage, compute_snes_checksum, pc_to_snes};

struct EncodedMessagePool {
    bytes: Vec<u8>,
    offsets: Vec<usize>,
}

#[derive(Debug)]
pub enum OverworldMessagePatchSaveError {
    InvalidMessageCount(usize),
    MessageContainsTerminator { index: usize },
    StaleStorage,
    MissingAllocation,
    LengthOverflow,
    Allocation(AllocationError),
    Rom(RomError),
    Detection(crate::OverworldMessagePatchError),
    Commit(crate::PayloadSaveError),
}

impl std::fmt::Display for OverworldMessagePatchSaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded overworld-message save failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldMessagePatchSaveError {}

impl From<AllocationError> for OverworldMessagePatchSaveError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<RomError> for OverworldMessagePatchSaveError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<crate::OverworldMessagePatchError> for OverworldMessagePatchSaveError {
    fn from(value: crate::OverworldMessagePatchError) -> Self {
        Self::Detection(value)
    }
}

impl From<crate::PayloadSaveError> for OverworldMessagePatchSaveError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Commit(value)
    }
}

impl Project {
    /// Replaces an installed message table and every owned pool as one undoable operation.
    ///
    /// # Errors
    ///
    /// Requires exact current detection evidence and caller-authorized allocation policy. Stale
    /// RATS descriptors, malformed messages, allocation/growth failures, mapping failures, and
    /// checksum failures leave the project unchanged.
    pub fn save_installed_overworld_messages(
        &mut self,
        messages: &[OverworldMessage],
        storage: &ExpandedOverworldMessageStorage,
        locator: OverworldMessagePatchLocator,
        allocation: &AllocationPolicy,
        checksum_field: usize,
        fill: u8,
    ) -> Result<bool, OverworldMessagePatchSaveError> {
        validate_count(messages.len())?;
        let detected = self.load_expanded_overworld_messages_detected(locator)?;
        if &detected.storage != storage {
            return Err(OverworldMessagePatchSaveError::StaleStorage);
        }
        if detected.messages == messages {
            return Ok(false);
        }
        let pools = encode_pools(messages)?;
        let original = self.rom.logical_bytes().to_vec();
        let old_table = exact_table_block(&original, storage)?;
        let mut bounded = allocation.clone();
        bounded.search.end = bounded.search.end.min(original.len());
        let attempted = stage_replacement(
            &original,
            locator.mapper,
            &bounded,
            &old_table,
            &storage.message_pools,
            &pools,
            fill,
        );
        let (mut staged, table) = match attempted {
            Ok(value) => value,
            Err(OverworldMessagePatchSaveError::Allocation(AllocationError::NoSpace {
                ..
            })) if allocation.search.end > original.len() => {
                let mut image = RomImage::from_bytes(original.clone())?;
                if !allocation.fill_bytes.contains(&fill) {
                    return Err(AllocationError::InvalidPolicy.into());
                }
                image.expand(locator.mapper, allocation.search.end, fill)?;
                stage_replacement(
                    image.logical_bytes(),
                    locator.mapper,
                    allocation,
                    &old_table,
                    &storage.message_pools,
                    &pools,
                    fill,
                )?
            }
            Err(error) => return Err(error),
        };
        publish_table_operands(&mut staged, locator.runtime_offset, locator.mapper, &table)?;
        let checksum = compute_snes_checksum(&staged, checksum_field)?;
        staged[checksum_field..checksum_field + 4].copy_from_slice(&checksum.encoded());
        commit_staged(
            self,
            "save expanded native overworld messages".into(),
            &original,
            &staged,
        )?;
        Ok(true)
    }
}

fn validate_count(count: usize) -> Result<(), OverworldMessagePatchSaveError> {
    if !(194..=512).contains(&count) || count % 2 != 0 {
        return Err(OverworldMessagePatchSaveError::InvalidMessageCount(count));
    }
    Ok(())
}

fn exact_table_block(
    bytes: &[u8],
    storage: &ExpandedOverworldMessageStorage,
) -> Result<RatsBlock, OverworldMessagePatchSaveError> {
    let header = storage
        .pointer_table_offset
        .checked_sub(lm_rats::HEADER_LEN)
        .ok_or(OverworldMessagePatchSaveError::MissingAllocation)?;
    let block =
        parse_at(bytes, header).map_err(|_| OverworldMessagePatchSaveError::MissingAllocation)?;
    if block.payload.start != storage.pointer_table_offset
        || block.payload.len() != storage.pointer_table_len
    {
        return Err(OverworldMessagePatchSaveError::MissingAllocation);
    }
    Ok(block)
}

fn encode_pools(
    messages: &[OverworldMessage],
) -> Result<Vec<EncodedMessagePool>, OverworldMessagePatchSaveError> {
    let mut pools = Vec::with_capacity(messages.len().div_ceil(0xc0));
    for (group_index, group) in messages.chunks(0xc0).enumerate() {
        let mut bytes = Vec::new();
        let mut offsets = Vec::with_capacity(group.len());
        let mut empty_offset = None;
        for (within_group, message) in group.iter().enumerate() {
            if message.0.contains(&0xfe) {
                return Err(OverworldMessagePatchSaveError::MessageContainsTerminator {
                    index: group_index * 0xc0 + within_group,
                });
            }
            let used = message
                .0
                .iter()
                .rposition(|byte| *byte != 0x1f)
                .map_or(0, |index| index + 1);
            if used == 0 {
                let offset = *empty_offset.get_or_insert_with(|| {
                    let offset = bytes.len();
                    bytes.push(0xfe);
                    offset
                });
                offsets.push(offset);
            } else {
                offsets.push(bytes.len());
                bytes.extend_from_slice(&message.0[..used]);
                if used < OverworldMessage::ENCODED_LEN {
                    bytes.push(0xfe);
                }
            }
        }
        pools.push(EncodedMessagePool { bytes, offsets });
    }
    Ok(pools)
}

fn stage_replacement(
    source: &[u8],
    mapper: Mapper,
    policy: &AllocationPolicy,
    old_table: &RatsBlock,
    old_pools: &[RatsBlock],
    pools: &[EncodedMessagePool],
    fill: u8,
) -> Result<(Vec<u8>, RatsBlock), OverworldMessagePatchSaveError> {
    let mut staged = source.to_vec();
    let mut placed = Vec::with_capacity(pools.len());
    {
        let mut allocator = FreeSpaceAllocator::new(&mut staged, policy.clone());
        for (index, pool) in pools.iter().enumerate() {
            placed.push(if let Some(old) = old_pools.get(index) {
                allocator.replace(old, &pool.bytes, fill)?
            } else {
                allocator.allocate(&pool.bytes)?
            });
        }
        for old in &old_pools[pools.len().min(old_pools.len())..] {
            allocator.erase(old, fill)?;
        }
    }
    let pointer_count = pools
        .iter()
        .map(|pool| pool.offsets.len())
        .try_fold(0_usize, usize::checked_add)
        .ok_or(OverworldMessagePatchSaveError::LengthOverflow)?;
    let mut table_bytes = vec![
        0;
        pointer_count
            .checked_mul(3)
            .ok_or(OverworldMessagePatchSaveError::LengthOverflow)?
    ];
    let mut table_index = 0;
    for (pool, block) in pools.iter().zip(&placed) {
        for offset in &pool.offsets {
            let target = block
                .payload
                .start
                .checked_add(*offset)
                .ok_or(OverworldMessagePatchSaveError::LengthOverflow)?;
            let address = pc_to_snes(mapper, target)?.to_le_bytes();
            table_bytes[table_index * 3..table_index * 3 + 3].copy_from_slice(&address[..3]);
            table_index += 1;
        }
    }
    let table = FreeSpaceAllocator::new(&mut staged, policy.clone()).replace(
        old_table,
        &table_bytes,
        fill,
    )?;
    Ok((staged, table))
}

fn publish_table_operands(
    bytes: &mut [u8],
    runtime_offset: usize,
    mapper: Mapper,
    table: &RatsBlock,
) -> Result<(), OverworldMessagePatchSaveError> {
    for (operand, addend) in [(0x49, 0_usize), (0x4f, 1)] {
        let target = table
            .payload
            .start
            .checked_add(addend)
            .ok_or(OverworldMessagePatchSaveError::LengthOverflow)?;
        let address = pc_to_snes(mapper, target)?.to_le_bytes();
        bytes
            .get_mut(runtime_offset + operand..runtime_offset + operand + 3)
            .ok_or(OverworldMessagePatchSaveError::MissingAllocation)?
            .copy_from_slice(&address[..3]);
    }
    Ok(())
}
