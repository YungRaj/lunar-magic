//! Lunar Magic-compatible SA-1 6/8-MiB expansion.

use crate::{Project, RomMutation, TransactionError};
use lm_rom::{
    IdentityError, Mapper, RomError, RomImage, SupportedGame, compute_snes_checksum,
    detect_identity,
};

pub const SA1_6_MIB_LEN: usize = 0x60_0000;
pub const SA1_8_MIB_LEN: usize = 0x80_0000;
const CHECKSUM_FIELD: usize = 0x7fdc;
const ROM_SIZE_BYTE: usize = 0x7fd7;
const COMPENSATION: std::ops::Range<usize> = 0x7f000..0x7f0a0;
const ATTRIBUTION_OFFSET: usize = 0x7f0a0;
const RUNTIME_RECORD_OFFSET: usize = 0x7ffe6;
const INTERNAL_HEADER: std::ops::Range<usize> = 0x7fc0..0x8000;
const HEADER_MIRROR_BLOCK: usize = 0x407fb8;
const LOCK_RANGES: [std::ops::Range<usize>; 3] =
    [0x400000..0x407fb8, 0x408000..0x410000, 0x410000..0x420000];
const LOCK_PAYLOAD_LENGTHS: [usize; 3] = [0x7fb0, 0x7ff8, 0xfff8];
const ATTRIBUTION: &[u8; 0xa0] = b"Lunar Magic Version 3.63 Public \xA92025 FuSoYa, Defender of Relm http://fusoya.eludevisibility.org                                I am Naaall, and I love fiiiish!";
const DEFAULT_SA1_RUNTIME_RECORD: [u8; 0x1a] = [
    0xff, 0x02, 0x00, 0xf8, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0xff, 0x00, 0x02, 0x10,
    0x00, 0x02, 0x08, 0x00, 0x02, 0x04, 0x00, 0x02, 0x08, 0x03,
];
const LOCK_MESSAGE: &[u8] = b"ZSNES 1.51 compatibility bank lock. If ROM is larger than 6MB, or you're using FuSoYa's custom 8MB build of ZSNES, or you don't care about ZSNES, then you can free this data.";

#[derive(Debug)]
pub enum Sa1ExpansionError {
    UnqualifiedProject,
    UnsupportedGame(SupportedGame),
    SourceMapper(Mapper),
    InvalidSourceChecksum,
    InvalidTarget(usize),
    CompensationOverflow(usize),
    ChecksumMismatch,
    Rom(RomError),
    Identity(IdentityError),
    Transaction(TransactionError),
}

impl std::fmt::Display for Sa1ExpansionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "SA-1 ROM expansion failed: {self:?}")
    }
}

impl std::error::Error for Sa1ExpansionError {}

impl From<RomError> for Sa1ExpansionError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}
impl From<IdentityError> for Sa1ExpansionError {
    fn from(value: IdentityError) -> Self {
        Self::Identity(value)
    }
}
impl From<TransactionError> for Sa1ExpansionError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Expands a qualified SA-1 SMW ROM to Lunar Magic's fixed 6- or 8-MiB layout.
    pub fn expand_sa1_rom(&mut self, target: usize) -> Result<bool, Sa1ExpansionError> {
        let identity = self
            .identity
            .clone()
            .ok_or(Sa1ExpansionError::UnqualifiedProject)?;
        if identity.game != SupportedGame::SuperMarioWorld {
            return Err(Sa1ExpansionError::UnsupportedGame(identity.game));
        }
        if identity.mapper != Mapper::Sa1 {
            return Err(Sa1ExpansionError::SourceMapper(identity.mapper));
        }
        if !identity.checksum_matches() {
            return Err(Sa1ExpansionError::InvalidSourceChecksum);
        }
        if ![SA1_6_MIB_LEN, SA1_8_MIB_LEN].contains(&target) || target <= self.rom.logical_len() {
            return Err(Sa1ExpansionError::InvalidTarget(target));
        }
        let before = self.rom.logical_bytes();
        let converted = build_sa1_expansion(before, target, identity.stored_checksum.checksum)?;
        let staged_identity = detect_identity(&RomImage::from_bytes(converted.clone())?)?;
        if staged_identity.mapper != Mapper::Sa1 || !staged_identity.checksum_matches() {
            return Err(Sa1ExpansionError::ChecksumMismatch);
        }
        let mutation = RomMutation::between(Mapper::Sa1, before, &converted)?;
        let changed = self.apply_mutation(
            if target == SA1_6_MIB_LEN {
                "Expand SA-1 ROM to 6 MiB"
            } else {
                "Expand SA-1 ROM to 8 MiB"
            },
            &mutation,
        )?;
        self.identity = Some(staged_identity);
        Ok(changed)
    }
}

fn build_sa1_expansion(
    source: &[u8],
    target: usize,
    stored_checksum: u16,
) -> Result<Vec<u8>, Sa1ExpansionError> {
    let mut output = vec![0; target];
    output[..source.len()].copy_from_slice(source);
    output[ROM_SIZE_BYTE] = 0x0d;
    output[ATTRIBUTION_OFFSET..ATTRIBUTION_OFFSET + ATTRIBUTION.len()].copy_from_slice(ATTRIBUTION);
    if source[RUNTIME_RECORD_OFFSET..RUNTIME_RECORD_OFFSET + 0x1a]
        .iter()
        .all(|byte| *byte == u8::MAX)
    {
        output[RUNTIME_RECORD_OFFSET..RUNTIME_RECORD_OFFSET + 0x1a]
            .copy_from_slice(&DEFAULT_SA1_RUNTIME_RECORD);
    }

    for range in &LOCK_RANGES {
        output[range.clone()].fill(0);
    }
    let mirrored_header = output[INTERNAL_HEADER.clone()].to_vec();
    write_rats_block(&mut output, HEADER_MIRROR_BLOCK, &mirrored_header);
    if target == SA1_6_MIB_LEN {
        for (range, payload_len) in LOCK_RANGES.iter().zip(LOCK_PAYLOAD_LENGTHS) {
            let mut payload = vec![u8::MAX; payload_len];
            payload[..LOCK_MESSAGE.len()].copy_from_slice(LOCK_MESSAGE);
            write_rats_block(&mut output, range.start, &payload);
        }
    }

    output[COMPENSATION.clone()].fill(0);
    let current = compute_snes_checksum(&output, CHECKSUM_FIELD)?.checksum;
    let difference = usize::from(stored_checksum.wrapping_sub(current));
    if difference > COMPENSATION.len() * usize::from(u8::MAX) {
        return Err(Sa1ExpansionError::CompensationOverflow(difference));
    }
    let full = difference / usize::from(u8::MAX);
    let remainder = difference % usize::from(u8::MAX);
    output[COMPENSATION.start..COMPENSATION.start + full].fill(u8::MAX);
    if remainder != 0 {
        output[COMPENSATION.start + full] = u8::try_from(remainder).unwrap_or_default();
    }
    if compute_snes_checksum(&output, CHECKSUM_FIELD)?.checksum != stored_checksum {
        return Err(Sa1ExpansionError::ChecksumMismatch);
    }
    Ok(output)
}

fn write_rats_block(output: &mut [u8], offset: usize, payload: &[u8]) {
    let length = u16::try_from(payload.len() - 1).expect("authenticated blocks fit u16");
    output[offset..offset + 4].copy_from_slice(b"STAR");
    output[offset + 4..offset + 6].copy_from_slice(&length.to_le_bytes());
    output[offset + 6..offset + 8].copy_from_slice(&(!length).to_le_bytes());
    output[offset + 8..offset + 8 + payload.len()].copy_from_slice(payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn sa1_fixture() -> Project {
        let mut bytes = fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join("sysLMRestore/smwOrig.smc"),
        )
        .unwrap();
        let header = bytes.len() % 0x8000;
        bytes[header + 0x7fd5] = 0x23;
        bytes[header + 0x7fd6] = 0x34;
        let mut image = RomImage::from_bytes(bytes).unwrap();
        image.update_snes_checksum(CHECKSUM_FIELD).unwrap();
        Project::open_supported(image).unwrap()
    }

    #[test]
    fn six_mib_installs_exact_locks_and_eight_mib_removes_only_them() {
        let mut project = sa1_fixture();
        let original = project.rom.as_file_bytes().to_vec();
        assert!(project.expand_sa1_rom(SA1_6_MIB_LEN).unwrap());
        assert_eq!(project.rom.logical_len(), SA1_6_MIB_LEN);
        assert_eq!(
            &project.rom.logical_bytes()[0x400008..0x400008 + LOCK_MESSAGE.len()],
            LOCK_MESSAGE
        );
        assert_eq!(
            &project.rom.logical_bytes()[HEADER_MIRROR_BLOCK + 8..HEADER_MIRROR_BLOCK + 0x48],
            &project.rom.logical_bytes()[INTERNAL_HEADER]
        );
        assert!(project.identity.as_ref().unwrap().checksum_matches());
        if let Some(path) = std::env::var_os("LM_SA1_6_MIB_ORACLE") {
            assert_eq!(project.rom.as_file_bytes(), fs::read(path).unwrap());
        }
        assert!(project.expand_sa1_rom(SA1_8_MIB_LEN).unwrap());
        assert!(LOCK_RANGES.iter().all(|range| {
            project.rom.logical_bytes()[range.clone()]
                .iter()
                .all(|byte| *byte == 0)
        }));
        assert_eq!(
            &project.rom.logical_bytes()[HEADER_MIRROR_BLOCK..HEADER_MIRROR_BLOCK + 4],
            b"STAR"
        );
        if let Some(path) = std::env::var_os("LM_SA1_8_MIB_ORACLE") {
            assert_eq!(project.rom.as_file_bytes(), fs::read(path).unwrap());
        }
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.logical_len(), SA1_6_MIB_LEN);
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.as_file_bytes(), original);
        assert!(project.redo().unwrap());
        assert!(project.redo().unwrap());
        assert_eq!(project.rom.logical_len(), SA1_8_MIB_LEN);
    }
}
