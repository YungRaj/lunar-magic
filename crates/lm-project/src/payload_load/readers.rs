use super::{LoadedPayload, PayloadLoadError, policy::bank_remaining};
use crate::Project;
use lm_rats::parse_at;
use lm_rom::SnesPointer24;

impl Project {
    pub(super) fn load_tagged_at(
        &self,
        pointer: SnesPointer24,
        payload_offset: usize,
    ) -> Result<LoadedPayload, PayloadLoadError> {
        let header_offset = payload_offset
            .checked_sub(lm_rats::HEADER_LEN)
            .ok_or(PayloadLoadError::PointerNotTagged { payload_offset })?;
        let block = parse_at(self.rom.logical_bytes(), header_offset)
            .map_err(|_| PayloadLoadError::PointerNotTagged { payload_offset })?;
        if block.payload.start != payload_offset {
            return Err(PayloadLoadError::PointerNotTagged { payload_offset });
        }
        let bytes = self.rom.logical_bytes()[block.payload.clone()].to_vec();
        Ok(LoadedPayload {
            pointer,
            pc_offset: payload_offset,
            block: Some(block),
            bytes,
        })
    }

    pub(super) fn load_terminated_at(
        &self,
        pointer: SnesPointer24,
        payload_offset: usize,
        terminator: &[u8],
        maximum_len: usize,
        bank_size: Option<usize>,
    ) -> Result<LoadedPayload, PayloadLoadError> {
        if terminator.is_empty() {
            return Err(PayloadLoadError::EmptyTerminator);
        }
        let image = self.rom.logical_bytes();
        let bank_remaining = bank_remaining(payload_offset, bank_size)?;
        let available = image.len().saturating_sub(payload_offset);
        let searched = maximum_len.min(bank_remaining).min(available);
        let search = self.rom.read(payload_offset, searched)?;
        let Some(relative_end) = search
            .windows(terminator.len())
            .position(|window| window == terminator)
            .map(|position| position + terminator.len())
        else {
            return Err(PayloadLoadError::MissingTerminator {
                payload_offset,
                searched,
            });
        };
        Ok(LoadedPayload {
            pointer,
            pc_offset: payload_offset,
            block: None,
            bytes: search[..relative_end].to_vec(),
        })
    }

    pub(super) fn load_bounded_at(
        &self,
        pointer: SnesPointer24,
        payload_offset: usize,
        maximum_len: usize,
        bank_size: Option<usize>,
    ) -> Result<LoadedPayload, PayloadLoadError> {
        let bank_remaining = bank_remaining(payload_offset, bank_size)?;
        let available = self.rom.logical_len().saturating_sub(payload_offset);
        let len = maximum_len.min(bank_remaining).min(available);
        Ok(LoadedPayload {
            pointer,
            pc_offset: payload_offset,
            block: None,
            bytes: self.rom.read(payload_offset, len)?.to_vec(),
        })
    }
}
