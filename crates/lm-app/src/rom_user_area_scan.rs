use crate::AppState;
use lm_profile::SMW_US_V1_LM_ATTRIBUTION_OFFSET;
use lm_rats::{RomUserAreaScan, scan_rom_user_area};
use lm_rom::{LunarMagicRomMetadata, Mapper};

const SMW_ORIGINAL_LOGICAL_SIZE: usize = 0x08_0000;
const EXLOROM_METADATA_BASE: usize = 0x40_0000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RomUserAreaReport {
    pub scan: RomUserAreaScan,
    pub last_lunar_magic_version: Option<String>,
    /// Physical-file displacement of logical offset zero (the optional copier header).
    pub physical_offset_base: usize,
}

impl AppState {
    /// Returns the original Scan ROM accounting for the currently open SMW ROM.
    #[must_use]
    pub fn rom_user_area_report(&self) -> Option<RomUserAreaReport> {
        let project = self.project.as_ref()?;
        let mapper = project.identity.as_ref()?.mapper;
        let mut report = report_for_smw(project.rom.logical_bytes(), mapper);
        report.physical_offset_base = project
            .rom
            .as_file_bytes()
            .len()
            .saturating_sub(project.rom.logical_bytes().len());
        Some(report)
    }
}

fn report_for_smw(bytes: &[u8], mapper: Mapper) -> RomUserAreaReport {
    RomUserAreaReport {
        scan: scan_rom_user_area(bytes, SMW_ORIGINAL_LOGICAL_SIZE, None),
        last_lunar_magic_version: lunar_magic_version(bytes, mapper),
        physical_offset_base: 0,
    }
}

fn lunar_magic_version(bytes: &[u8], mapper: Mapper) -> Option<String> {
    let base = usize::from(mapper == Mapper::ExLoRom) * EXLOROM_METADATA_BASE;
    let start = base.checked_add(SMW_US_V1_LM_ATTRIBUTION_OFFSET)?;
    let attribution = bytes.get(start..start + LunarMagicRomMetadata::ATTRIBUTION_LEN)?;
    let suffix = attribution.strip_prefix(LunarMagicRomMetadata::SIGNATURE)?;
    let end = suffix
        .iter()
        .position(|byte| byte.is_ascii_whitespace() || *byte == 0)
        .unwrap_or(suffix.len());
    let version = std::str::from_utf8(&suffix[..end]).ok()?;
    (!version.is_empty()
        && version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
        && version.bytes().any(|byte| byte == b'.'))
    .then(|| version.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modern_expanded_fixture_matches_retained_lunar_magic_scan_dialog() {
        let mut bytes = vec![0; 0x20_0000];
        let attribution = b"Lunar Magic Version 3.63 Public";
        bytes[SMW_US_V1_LM_ATTRIBUTION_OFFSET..SMW_US_V1_LM_ATTRIBUTION_OFFSET + attribution.len()]
            .copy_from_slice(attribution);
        for (offset, payload_len) in [
            (0x80_000, 0x20),
            (0x80_028, 0x6e00),
            (0xef_ff8, 0x096d),
            (0xf0_96d, 0x0c2a),
            (0xf1_59f, 0x08b6),
        ] {
            bytes[offset..offset + 8].copy_from_slice(&lm_rats::make_header(payload_len).unwrap());
            bytes[offset + 8..offset + 8 + payload_len].fill(0x55);
        }
        let report = report_for_smw(&bytes, Mapper::LoRom);
        assert_eq!(report.last_lunar_magic_version.as_deref(), Some("3.63"));
        assert_eq!(
            report.scan,
            RomUserAreaScan {
                rat_protected_space: 0x8c95,
                unprotected_map16: 0,
                unprotected_used_space: 0,
                unusable_space: 0,
                free_space: 0x17_736b,
                total_user_space: 0x18_0000,
                conflicting_rats: 0,
                conflicting_space: 0,
                rat_structures: 5,
                largest_free_32kb_bank: 0x8000,
                largest_free_area: 0x10_e1a3,
                conflicting_offsets: Vec::new(),
                conflicts: Vec::new(),
            }
        );
    }

    #[test]
    fn exlorom_reads_the_mirrored_attribution() {
        let mut bytes = vec![0xff; EXLOROM_METADATA_BASE + SMW_ORIGINAL_LOGICAL_SIZE];
        let start = EXLOROM_METADATA_BASE + SMW_US_V1_LM_ATTRIBUTION_OFFSET;
        let text = b"Lunar Magic Version 3.63 Public";
        bytes[start..start + text.len()].copy_from_slice(text);
        assert_eq!(
            lunar_magic_version(&bytes, Mapper::ExLoRom).as_deref(),
            Some("3.63")
        );
    }

    #[test]
    fn malformed_attribution_is_not_reported_as_a_version() {
        let mut bytes = vec![0xff; SMW_ORIGINAL_LOGICAL_SIZE];
        let text = b"Lunar Magic Version surprise";
        bytes[SMW_US_V1_LM_ATTRIBUTION_OFFSET..SMW_US_V1_LM_ATTRIBUTION_OFFSET + text.len()]
            .copy_from_slice(text);
        assert_eq!(lunar_magic_version(&bytes, Mapper::LoRom), None);
    }
}
