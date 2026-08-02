use crate::AppState;
use lm_profile::{
    SmwUsV1Layer2RuntimeGeneration, SmwUsV1Lfix3Generation, SmwUsV1Map16RuntimeGeneration,
    probe_smw_us_v1_layer2_runtime_generation, probe_smw_us_v1_lfix3_generation,
    probe_smw_us_v1_map16_runtime_generation,
};
use lm_rom::{CopierHeader, Mapper, Region, SupportedGame, detect_identity};
use std::fmt::Write as _;

/// Bounded, path-free compatibility evidence for one open ROM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RomCompatibilityReport {
    pub text: String,
    pub warnings: usize,
}

impl AppState {
    /// Builds a deterministic report suitable for support requests without exposing paths or ROM
    /// bytes.
    #[must_use]
    pub fn rom_compatibility_report(&self) -> RomCompatibilityReport {
        let Some(project) = self.project.as_ref() else {
            return RomCompatibilityReport {
                text: "ROM compatibility: no project open".into(),
                warnings: 0,
            };
        };
        let Some(identity) = project.identity.as_ref() else {
            return RomCompatibilityReport {
                text: "ROM compatibility report\nCompatibility warnings: 1\nWarning: open project has no qualified ROM identity".into(),
                warnings: 1,
            };
        };
        let physical = project.rom.as_file_bytes();
        let logical = project.rom.logical_bytes();
        let changed = project.rom.changed_ranges();
        let changed_bytes = changed.iter().map(|range| range.range.len()).sum::<usize>();
        let rats = lm_rats::scan(logical);
        let rats_payload_bytes = rats.iter().map(|block| block.payload.len()).sum::<usize>();
        let mut warnings = Vec::new();
        if !identity.checksum_matches() {
            warnings.push("stored SNES checksum does not match the current ROM".to_owned());
        }
        let current_identity = probe_current_identity(project, identity, &mut warnings);
        let profile = audit_profile(project, self.revision_profile.as_ref(), &mut warnings);

        let runtime_applicable = identity.game == SupportedGame::SuperMarioWorld
            && identity.region == Region::NorthAmerica
            && identity.revision == 0
            && matches!(identity.mapper, Mapper::LoRom | Mapper::ExLoRom);
        let (layer2, map16, lfix3) = if runtime_applicable {
            (
                probe_layer2(project, &mut warnings),
                probe_map16(logical, &mut warnings),
                probe_lfix3(logical, &mut warnings),
            )
        } else {
            (
                "not-applicable".into(),
                "not-applicable".into(),
                "not-applicable".into(),
            )
        };

        let mut text = format!(
            "ROM compatibility report\nGame: {}\nRegion: {}\nRevision: {}\nMapper: {}\nCurrent identity: {}\nMap mode: {:02X}\nCartridge type: {:02X}\nCopier header: {}\nPhysical bytes: {}\nLogical bytes: {}\nChecksum status: {}\nStored checksum: {:04X}\nComputed checksum: {:04X}\nProject revision: {}\nDirty: {}\nChanged logical ranges: {}\nChanged logical bytes: {}\nRATS blocks: {}\nRATS payload bytes: {}\nRevision profile: {}\nLayer 2 runtime: {}\nMap16 runtime: {}\nLfix3 runtime: {}\nCompatibility warnings: {}",
            game_label(identity.game),
            region_label(identity.region),
            identity.revision,
            mapper_label(identity.mapper),
            current_identity,
            identity.map_mode,
            identity.cartridge_type,
            header_label(project.rom.copier_header()),
            physical.len(),
            logical.len(),
            if identity.checksum_matches() {
                "valid"
            } else {
                "mismatch"
            },
            identity.stored_checksum.checksum,
            identity.computed_checksum.checksum,
            self.project_revision,
            project.is_modified(),
            changed.len(),
            changed_bytes,
            rats.len(),
            rats_payload_bytes,
            profile,
            layer2,
            map16,
            lfix3,
            warnings.len(),
        );
        for warning in &warnings {
            write!(text, "\nWarning: {warning}").expect("writing to a String cannot fail");
        }
        RomCompatibilityReport {
            text,
            warnings: warnings.len(),
        }
    }
}

fn probe_current_identity(
    project: &lm_project::Project,
    opened: &lm_rom::RomIdentity,
    warnings: &mut Vec<String>,
) -> &'static str {
    match detect_identity(&project.rom) {
        Ok(current)
            if current.game == opened.game
                && current.region == opened.region
                && current.revision == opened.revision
                && current.mapper == opened.mapper =>
        {
            "valid"
        }
        Ok(_) => {
            warnings.push("current ROM identity no longer matches the opened project".into());
            "changed"
        }
        Err(error) => {
            warnings.push(format!("current ROM identity validation failed: {error}"));
            "invalid"
        }
    }
}

fn audit_profile(
    project: &lm_project::Project,
    profile: Option<&lm_profile::RevisionProfile>,
    warnings: &mut Vec<String>,
) -> String {
    let Some(profile) = profile else {
        return "not-installed".into();
    };
    match profile.audit_rom(&project.rom) {
        Ok(audit) => format!(
            "audited ({} tables, {} pointer entries)",
            audit.tables.len(),
            audit.total_entries
        ),
        Err(error) => {
            warnings.push(format!("installed revision profile audit failed: {error}"));
            "audit-failed".into()
        }
    }
}

fn probe_layer2(project: &lm_project::Project, warnings: &mut Vec<String>) -> String {
    match probe_smw_us_v1_layer2_runtime_generation(&project.rom) {
        Ok(generation) => match generation {
            SmwUsV1Layer2RuntimeGeneration::Absent => "absent",
            SmwUsV1Layer2RuntimeGeneration::Format100Legacy => "format-100-legacy",
            SmwUsV1Layer2RuntimeGeneration::Format101Legacy => "format-101-legacy",
            SmwUsV1Layer2RuntimeGeneration::Format102Legacy => "format-102-legacy",
            SmwUsV1Layer2RuntimeGeneration::Format103Current => "format-103-current",
        }
        .into(),
        Err(error) => {
            warnings.push(format!("Layer 2 runtime probe failed: {error}"));
            "probe-failed".into()
        }
    }
}

fn probe_map16(bytes: &[u8], warnings: &mut Vec<String>) -> String {
    match probe_smw_us_v1_map16_runtime_generation(bytes) {
        Ok(generation) => match generation {
            SmwUsV1Map16RuntimeGeneration::Absent => "absent",
            SmwUsV1Map16RuntimeGeneration::StageOneLegacy => "stage-1-legacy",
            SmwUsV1Map16RuntimeGeneration::StageTwoLegacy => "stage-2-legacy",
            SmwUsV1Map16RuntimeGeneration::StageThreeLegacy => "stage-3-legacy",
            SmwUsV1Map16RuntimeGeneration::StageFourCurrent => "stage-4-current",
        }
        .into(),
        Err(error) => {
            warnings.push(format!("Map16 runtime probe failed: {error}"));
            "probe-failed".into()
        }
    }
}

fn probe_lfix3(bytes: &[u8], warnings: &mut Vec<String>) -> String {
    match probe_smw_us_v1_lfix3_generation(bytes) {
        Ok(generation) => match generation {
            SmwUsV1Lfix3Generation::Absent => "absent",
            SmwUsV1Lfix3Generation::Generation1 => "generation-1-legacy",
            SmwUsV1Lfix3Generation::Generation2 => "generation-2-legacy",
            SmwUsV1Lfix3Generation::Generation3Current => "generation-3-current",
        }
        .into(),
        Err(error) => {
            warnings.push(format!("Lfix3 runtime probe failed: {error}"));
            "probe-failed".into()
        }
    }
}

const fn game_label(game: SupportedGame) -> &'static str {
    match game {
        SupportedGame::SuperMarioWorld => "Super Mario World",
        SupportedGame::AllStarsAndWorld => "Super Mario All-Stars + World",
    }
}

const fn region_label(region: Region) -> &'static str {
    match region {
        Region::Japan => "Japan",
        Region::NorthAmerica => "North America",
    }
}

const fn mapper_label(mapper: Mapper) -> &'static str {
    match mapper {
        Mapper::LoRom => "LoROM",
        Mapper::ExLoRom => "ExLoROM",
        Mapper::Sa1 => "SA-1",
    }
}

const fn header_label(header: CopierHeader) -> &'static str {
    match header {
        CopierHeader::Absent => "absent",
        CopierHeader::Present => "present",
    }
}
