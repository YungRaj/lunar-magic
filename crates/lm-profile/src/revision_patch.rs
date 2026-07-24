//! Identity-bound external runtime-patch templates.

use crate::RevisionProfile;
use lm_project::{PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::fmt;
use std::ops::Range;

mod codec;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionPatchTemplate {
    pub name: String,
    pub game: SupportedGame,
    pub region: Region,
    pub revision: u8,
    pub mapper: Mapper,
    pub payloads: Vec<PatchPayload>,
    pub writes: Vec<PatchWrite>,
}

impl RevisionPatchTemplate {
    pub const MAGIC: &'static [u8; 8] = b"LMPAT001";
    pub const MAX_FILE_LEN: usize = 512 * 1024;
    pub const MAX_NAME_LEN: usize = 256;
    pub const MAX_PAYLOADS: usize = 16;
    pub const MAX_WRITES: usize = 128;
    pub const MAX_FIXUPS: usize = 2048;
    pub const MAX_BODY_BYTES: usize = 256 * 1024;

    /// Decodes and fully validates one bounded binary template.
    ///
    /// # Errors
    ///
    /// Rejects wrong framing, unknown identities, excessive counts/bytes, malformed fixups,
    /// trailing bytes, and noncanonical templates.
    pub fn decode(bytes: &[u8]) -> Result<Self, RevisionPatchTemplateError> {
        codec::decode(bytes)
    }

    /// Produces the canonical deterministic binary representation.
    ///
    /// # Errors
    ///
    /// Rejects the same structural bounds as [`Self::decode`].
    pub fn encode(&self) -> Result<Vec<u8>, RevisionPatchTemplateError> {
        codec::encode(self)
    }

    /// Confirms that this template belongs to the selected audited revision profile.
    ///
    /// # Errors
    ///
    /// Returns [`RevisionPatchTemplateError::ProfileMismatch`] when any stable identity differs.
    pub fn ensure_profile(
        &self,
        profile: &RevisionProfile,
    ) -> Result<(), RevisionPatchTemplateError> {
        if self.game == profile.game
            && self.region == profile.region
            && self.revision == profile.revision
            && self.mapper == profile.mapper
        {
            Ok(())
        } else {
            Err(RevisionPatchTemplateError::ProfileMismatch)
        }
    }

    /// Binds this address-independent template to one audited ROM/profile allocation policy.
    ///
    /// # Errors
    ///
    /// Rejects identity disagreement or any invalid/out-of-image profile allocation boundary.
    pub fn installation_plan(
        &self,
        profile: &RevisionProfile,
        rom: &RomImage,
        search: Range<usize>,
        internal_header_offset: usize,
        checksum_field: usize,
        expansion_fill: u8,
    ) -> Result<RelocatablePatchPlan, RevisionPatchPlanError> {
        self.ensure_profile(profile)
            .map_err(RevisionPatchPlanError::Template)?;
        let mut policy_rom = rom.clone();
        if search.end > policy_rom.logical_len() {
            policy_rom
                .expand(profile.mapper, search.end, expansion_fill)
                .map_err(RevisionPatchPlanError::Rom)?;
        }
        let allocation = profile
            .allocation_policy_for_rom(search, &policy_rom, internal_header_offset)
            .map_err(RevisionPatchPlanError::Allocation)?;
        Ok(RelocatablePatchPlan {
            description: format!("install revision patch {}", self.name),
            mapper: profile.mapper,
            allocation,
            checksum_field,
            expansion_fill,
            payloads: self.payloads.clone(),
            writes: self.writes.clone(),
        })
    }
}

#[derive(Debug)]
pub enum RevisionPatchPlanError {
    Template(RevisionPatchTemplateError),
    Allocation(crate::RevisionAllocationError),
    Rom(lm_rom::RomError),
}

impl fmt::Display for RevisionPatchPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot construct revision patch plan: {self:?}")
    }
}

impl std::error::Error for RevisionPatchPlanError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RevisionPatchTemplateError {
    TooLarge { actual: usize, maximum: usize },
    WrongMagic,
    Truncated,
    TrailingBytes(usize),
    UnknownGame(u8),
    UnknownRegion(u8),
    UnknownMapper(u8),
    InvalidName,
    TooManyPayloads(usize),
    TooManyWrites(usize),
    TooManyFixups(usize),
    BodyTooLarge(usize),
    EmptyPayload(usize),
    EmptyWrite(usize),
    WriteLengthMismatch(usize),
    InvalidFixup { owner: usize, index: usize },
    NumberOutOfRange,
    NonCanonical,
    ProfileMismatch,
}

impl fmt::Display for RevisionPatchTemplateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "revision patch template failed: {self:?}")
    }
}

impl std::error::Error for RevisionPatchTemplateError {}

#[cfg(test)]
#[path = "revision_patch_tests.rs"]
mod tests;
