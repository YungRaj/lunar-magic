//! Lunar Magic-compatible 64-Mbit ExLoROM conversion.

use crate::{EditKind, Project, RomMutation, TransactionError};
use lm_rom::{
    IdentityError, Mapper, RomError, RomImage, SupportedGame, compute_snes_checksum,
    detect_identity,
};

pub const EXLOROM_CONVERSION_TARGET_LEN: usize = 0x80_0000;
const MAX_LOROM_SOURCE_LEN: usize = 0x40_0000;
const RELOCATED_SOURCE_LIMIT: usize = 0x38_0000;
const RELOCATED_BASE: usize = 0x40_0000;
const FIRST_BANK_LEN: usize = 0x8000;
const ROM_SIZE_BYTE: usize = 0x7fd7;
const RELOCATED_ROM_SIZE_BYTE: usize = RELOCATED_BASE + ROM_SIZE_BYTE;
const CHECKSUM_FIELD: usize = 0x7fdc;
const RELOCATED_COMPENSATION: std::ops::Range<usize> = 0x47_f000..0x47_f0a0;
const RELOCATED_ATTRIBUTION: usize = 0x47_f0a0;
const RELOCATED_RUNTIME_RECORD: usize = 0x47_ffe6;
const SOURCE_RUNTIME_RECORD: usize = 0x07_ffe6;
const NULL_BANKS: [usize; 2] = [0x7f_0000, 0x7f_8000];

const LM363_ATTRIBUTION: &[u8; 0xa0] =
    b"Lunar Magic Version 3.63 Public \xA92025 FuSoYa, Defender of Relm http://fusoya.eludevisibility.org                                I am Naaall, and I love fiiiish!";
const PRISTINE_CONVERSION_RUNTIME_RECORD: [u8; 0x1a] = [
    0xff, 0x00, 0x00, 0xf8, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0xff, 0x00, 0x02, 0x18,
    0x00, 0x82, 0x00, 0x00, 0x02, 0x10, 0x00, 0x82, 0x00, 0x00,
];
const NULL_BANK_MESSAGE: &[u8] = b"ExLoROM NULL bank lock. This bank is not mapped and cannot be accessed by the game.     **DO NOT USE!!**";

#[derive(Debug)]
pub enum ExLoRomConversionError {
    UnqualifiedProject,
    UnsupportedGame(SupportedGame),
    SourceMapper(Mapper),
    SourceLength(usize),
    InvalidSourceChecksum,
    CompensationOverflow(usize),
    ChecksumMismatch,
    Rom(RomError),
    Identity(IdentityError),
    Transaction(TransactionError),
}

impl std::fmt::Display for ExLoRomConversionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "64-Mbit ExLoROM conversion failed: {self:?}")
    }
}

impl std::error::Error for ExLoRomConversionError {}

impl From<RomError> for ExLoRomConversionError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<IdentityError> for ExLoRomConversionError {
    fn from(value: IdentityError) -> Self {
        Self::Identity(value)
    }
}

impl From<TransactionError> for ExLoRomConversionError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Converts a checksum-valid SMW LoROM into Lunar Magic 3.63's 64-Mbit ExLoROM layout as one
    /// failure-atomic, undoable mapper transition.
    ///
    /// The recovered transformation retains the first LoROM bank, moves the first 3.5 MiB into
    /// the upper ExLoROM half, retains an existing final 512 KiB in the lower half, installs both
    /// inaccessible-bank RATS locks, writes the compatibility metadata, and preserves the source
    /// checksum through Lunar Magic's compensation area. The physical copier header is untouched.
    ///
    /// # Errors
    ///
    /// Rejects unqualified/non-SMW/non-LoROM projects, invalid source checksums, sources larger
    /// than 4 MiB, a checksum that cannot fit the recovered compensation area, or any failed
    /// staged identity/transaction check. Every failure preserves ROM bytes, identity, and history.
    pub fn convert_to_64_mbit_exlorom(&mut self) -> Result<bool, ExLoRomConversionError> {
        let source_identity = self
            .identity
            .clone()
            .ok_or(ExLoRomConversionError::UnqualifiedProject)?;
        if source_identity.game != SupportedGame::SuperMarioWorld {
            return Err(ExLoRomConversionError::UnsupportedGame(
                source_identity.game,
            ));
        }
        if source_identity.mapper != Mapper::LoRom {
            return Err(ExLoRomConversionError::SourceMapper(source_identity.mapper));
        }
        let source = self.rom.logical_bytes();
        if !(SOURCE_RUNTIME_RECORD + PRISTINE_CONVERSION_RUNTIME_RECORD.len()
            ..=MAX_LOROM_SOURCE_LEN)
            .contains(&source.len())
        {
            return Err(ExLoRomConversionError::SourceLength(source.len()));
        }
        if !source_identity.checksum_matches() {
            return Err(ExLoRomConversionError::InvalidSourceChecksum);
        }

        let converted = build_converted_image(source, source_identity.stored_checksum.checksum)?;
        let staged_identity = detect_identity(&RomImage::from_bytes(converted.clone())?)?;
        if staged_identity.mapper != Mapper::ExLoRom
            || staged_identity.stored_checksum.checksum != source_identity.stored_checksum.checksum
            || !staged_identity.checksum_matches()
        {
            return Err(ExLoRomConversionError::ChecksumMismatch);
        }
        let mutation = RomMutation::between(Mapper::ExLoRom, source, &converted)?;

        // The ordinary mutation gate correctly rejects a mapper that differs from the currently
        // qualified image. Bind the already-validated target mapper only for this private commit;
        // restore the complete prior identity on every failure.
        self.identity.as_mut().expect("qualified above").mapper = Mapper::ExLoRom;
        let committed = self.apply_mutation_with_kind(
            "Convert ROM to 64-Mbit ExLoROM",
            &mutation,
            EditKind::MapperConversion {
                source: Mapper::LoRom,
                target: Mapper::ExLoRom,
            },
        );
        match committed {
            Ok(changed) => {
                debug_assert!(changed);
                self.identity = Some(staged_identity);
                Ok(changed)
            }
            Err(error) => {
                self.identity = Some(source_identity);
                Err(error.into())
            }
        }
    }
}

fn build_converted_image(
    source: &[u8],
    stored_checksum: u16,
) -> Result<Vec<u8>, ExLoRomConversionError> {
    let mut converted = vec![0; EXLOROM_CONVERSION_TARGET_LEN];
    let relocated_len = source.len().min(RELOCATED_SOURCE_LIMIT);
    converted[RELOCATED_BASE..RELOCATED_BASE + relocated_len]
        .copy_from_slice(&source[..relocated_len]);
    converted[..FIRST_BANK_LEN].copy_from_slice(&source[..FIRST_BANK_LEN]);
    if source.len() > RELOCATED_SOURCE_LIMIT {
        converted[RELOCATED_SOURCE_LIMIT..source.len()]
            .copy_from_slice(&source[RELOCATED_SOURCE_LIMIT..]);
    }

    converted[ROM_SIZE_BYTE] = 0x0d;
    converted[RELOCATED_ROM_SIZE_BYTE] = 0x0d;
    converted[RELOCATED_ATTRIBUTION..RELOCATED_ATTRIBUTION + LM363_ATTRIBUTION.len()]
        .copy_from_slice(LM363_ATTRIBUTION);
    converted[RELOCATED_RUNTIME_RECORD
        ..RELOCATED_RUNTIME_RECORD + PRISTINE_CONVERSION_RUNTIME_RECORD.len()]
        .copy_from_slice(&PRISTINE_CONVERSION_RUNTIME_RECORD);
    for offset in NULL_BANKS {
        converted[offset..offset + FIRST_BANK_LEN].copy_from_slice(&null_bank_lock());
    }

    converted[RELOCATED_COMPENSATION.clone()].fill(0);
    let current = compute_snes_checksum(&converted, CHECKSUM_FIELD)?.checksum;
    let difference = usize::from(stored_checksum.wrapping_sub(current));
    let capacity = RELOCATED_COMPENSATION.len() * usize::from(u8::MAX);
    if difference > capacity {
        return Err(ExLoRomConversionError::CompensationOverflow(difference));
    }
    let full = difference / usize::from(u8::MAX);
    let remainder = difference % usize::from(u8::MAX);
    converted[RELOCATED_COMPENSATION.start..RELOCATED_COMPENSATION.start + full].fill(u8::MAX);
    if remainder != 0 {
        converted[RELOCATED_COMPENSATION.start + full] =
            u8::try_from(remainder).unwrap_or_default();
    }
    if compute_snes_checksum(&converted, CHECKSUM_FIELD)?.checksum != stored_checksum {
        return Err(ExLoRomConversionError::ChecksumMismatch);
    }
    Ok(converted)
}

fn null_bank_lock() -> [u8; FIRST_BANK_LEN] {
    let mut lock = [u8::MAX; FIRST_BANK_LEN];
    lock[..8].copy_from_slice(b"STAR\xf7\x7f\x08\x80");
    lock[8..8 + NULL_BANK_MESSAGE.len()].copy_from_slice(NULL_BANK_MESSAGE);
    lock
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn pristine() -> Vec<u8> {
        fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join("sysLMRestore/smwOrig.smc"),
        )
        .unwrap()
    }

    #[test]
    fn pristine_conversion_matches_the_lm363_oracle_structure_and_is_one_reversible_step() {
        let original = pristine();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let source = project.rom.logical_bytes().to_vec();
        let header = project.rom.copier_header_bytes().unwrap().to_vec();
        assert!(project.convert_to_64_mbit_exlorom().unwrap());

        let converted = project.rom.logical_bytes();
        assert_eq!(converted.len(), EXLOROM_CONVERSION_TARGET_LEN);
        assert_eq!(project.rom.copier_header_bytes(), Some(header.as_slice()));
        let mut expected_first_bank = source[..FIRST_BANK_LEN].to_vec();
        expected_first_bank[ROM_SIZE_BYTE] = 0x0d;
        assert_eq!(&converted[..FIRST_BANK_LEN], expected_first_bank);
        assert_eq!(converted[ROM_SIZE_BYTE], 0x0d);
        let mut expected_relocated = source.clone();
        expected_relocated[ROM_SIZE_BYTE] = 0x0d;
        expected_relocated[0x07_f000..0x07_f0a0]
            .copy_from_slice(&converted[RELOCATED_COMPENSATION]);
        expected_relocated[0x07_f0a0..0x07_f140].copy_from_slice(LM363_ATTRIBUTION);
        expected_relocated[0x07_ffe6..0x08_0000]
            .copy_from_slice(&PRISTINE_CONVERSION_RUNTIME_RECORD);
        assert_eq!(
            &converted[RELOCATED_BASE..RELOCATED_BASE + source.len()],
            expected_relocated
        );
        assert_eq!(converted[RELOCATED_ROM_SIZE_BYTE], 0x0d);
        assert_eq!(
            &converted[RELOCATED_ATTRIBUTION..RELOCATED_ATTRIBUTION + 0xa0],
            LM363_ATTRIBUTION
        );
        assert_eq!(
            &converted[RELOCATED_RUNTIME_RECORD..RELOCATED_RUNTIME_RECORD + 0x1a],
            PRISTINE_CONVERSION_RUNTIME_RECORD
        );
        for offset in NULL_BANKS {
            assert_eq!(
                &converted[offset..offset + FIRST_BANK_LEN],
                &null_bank_lock()
            );
        }
        assert_eq!(project.identity.as_ref().unwrap().mapper, Mapper::ExLoRom);
        assert!(project.identity.as_ref().unwrap().checksum_matches());
        assert_eq!(project.history.undo_len(), 1);

        if let Some(oracle_path) = std::env::var_os("LM_EXLOROM_ORACLE") {
            assert_eq!(
                project.rom.as_file_bytes(),
                fs::read(oracle_path).expect("read Lunar Magic ExLoROM oracle")
            );
        }

        assert!(project.undo().unwrap());
        assert_eq!(project.rom.as_file_bytes(), original);
        assert_eq!(project.identity.as_ref().unwrap().mapper, Mapper::LoRom);
        assert!(project.redo().unwrap());
        assert_eq!(project.identity.as_ref().unwrap().mapper, Mapper::ExLoRom);
        assert!(project.identity.as_ref().unwrap().checksum_matches());
    }

    #[test]
    fn final_half_mib_stays_low_while_first_three_and_a_half_mib_relocates() {
        let original = pristine();
        let mut project = Project::open_supported(RomImage::from_bytes(original).unwrap()).unwrap();
        project
            .expand_rom(Mapper::LoRom, MAX_LOROM_SOURCE_LEN, 0, CHECKSUM_FIELD)
            .unwrap();
        project.rom.write(0x10_0000, &[0x11; 4]).unwrap();
        project.rom.write(0x37_ff00, &[0x37; 4]).unwrap();
        project.rom.write(0x38_0000, &[0x38; 4]).unwrap();
        project.rom.write(0x3f_0000, &[0x3f; 4]).unwrap();
        project.rom.update_snes_checksum(CHECKSUM_FIELD).unwrap();
        project.synchronize_identity_checksums();
        project.history.clear();

        project.convert_to_64_mbit_exlorom().unwrap();
        let converted = project.rom.logical_bytes();
        assert_eq!(&converted[0x10_0000..0x10_0004], &[0; 4]);
        assert_eq!(&converted[0x37_ff00..0x37_ff04], &[0; 4]);
        assert_eq!(&converted[0x50_0000..0x50_0004], &[0x11; 4]);
        assert_eq!(&converted[0x77_ff00..0x77_ff04], &[0x37; 4]);
        assert_eq!(&converted[0x38_0000..0x38_0004], &[0x38; 4]);
        assert_eq!(&converted[0x3f_0000..0x3f_0004], &[0x3f; 4]);
    }
}
