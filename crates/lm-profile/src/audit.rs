use crate::{RevisionProfile, RevisionProfileError};
mod layout;

use layout::{expanded_settings_span, graphics_spans, overlaps, sprite_spans, table_span, tables};
use lm_project::{
    InstalledExAnimationRomLayout, InstalledLayoutError, LevelPointerTable, SpritePointerTable,
};
use lm_rom::{IdentityError, RomError, RomImage, detect_identity, snes_to_pc};
use std::fmt;
use std::ops::Range;

const INTERNAL_HEADER_AND_VECTORS_LEN: usize = 0x40;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointerTableAudit {
    pub domain: &'static str,
    pub entries: usize,
    pub unique_targets: usize,
    pub minimum_target: usize,
    pub maximum_target: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectTableAudit {
    pub domain: &'static str,
    pub offset: usize,
    pub entries: usize,
    pub stride: usize,
    pub byte_span: Range<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionProfileAudit {
    pub tables: Vec<PointerTableAudit>,
    pub total_entries: usize,
    pub expanded_settings: Option<DirectTableAudit>,
}

#[derive(Debug)]
pub enum RevisionProfileAuditError {
    Profile(RevisionProfileError),
    Identity(IdentityError),
    PointerOffset {
        domain: &'static str,
        index: usize,
    },
    PointerRead {
        domain: &'static str,
        index: usize,
        error: RomError,
    },
    InvalidTarget {
        domain: &'static str,
        index: usize,
        address: u32,
        error: RomError,
    },
    TargetOutOfBounds {
        domain: &'static str,
        index: usize,
        address: u32,
        target: usize,
        logical_len: usize,
    },
    MetadataOverlap {
        domain: &'static str,
        reserved: &'static str,
    },
    MetadataOutOfBounds {
        domain: &'static str,
        end: usize,
        logical_len: usize,
    },
    TargetInMetadata {
        domain: &'static str,
        index: usize,
        target: usize,
        metadata: &'static str,
    },
    EntryCountOverflow,
    Installation(InstalledLayoutError),
    PointerLocator(lm_project::PointerLocatorError),
}

impl fmt::Display for RevisionProfileAuditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "revision-profile ROM audit failed: {self:?}")
    }
}

impl std::error::Error for RevisionProfileAuditError {}

impl From<InstalledLayoutError> for RevisionProfileAuditError {
    fn from(value: InstalledLayoutError) -> Self {
        Self::Installation(value)
    }
}

impl From<lm_project::PointerLocatorError> for RevisionProfileAuditError {
    fn from(value: lm_project::PointerLocatorError) -> Self {
        Self::PointerLocator(value)
    }
}

pub(super) fn audit(
    profile: &RevisionProfile,
    rom: &RomImage,
) -> Result<RevisionProfileAudit, RevisionProfileAuditError> {
    profile
        .validate()
        .map_err(RevisionProfileAuditError::Profile)?;
    let identity = detect_identity(rom).map_err(RevisionProfileAuditError::Identity)?;
    profile
        .ensure_identity(&identity)
        .map_err(RevisionProfileAuditError::Profile)?;
    let palette_installed = profile.palette_installation.resolve(rom)?.is_some();
    let selected_exanimation = profile.exanimation_installation.resolve(rom)?;
    let selected_exanimation_features = profile.exanimation_feature_installation.resolve(rom)?;
    let resolved_exanimation = selected_exanimation
        .map(|layout| layout.resolve(rom))
        .transpose()?;
    let mut declared_tables = tables(profile);
    if let Some(resolved) = resolved_exanimation
        && let Some((_, table)) = declared_tables
            .iter_mut()
            .find(|(domain, _)| *domain == "exanimation")
    {
        *table = resolved.payload.pointers;
    }
    let exanimation_installed = resolved_exanimation.is_some();
    let active_tables = declared_tables
        .into_iter()
        .filter(|(domain, _)| {
            (*domain != "palette" || palette_installed)
                && (*domain != "exanimation" || exanimation_installed)
        })
        .collect::<Vec<_>>();
    let mut metadata = active_tables
        .iter()
        .copied()
        .filter(|(domain, _)| {
            *domain != "level.sprites"
                && !(*domain == "graphics" && profile.graphics.split_pointer_planes.is_some())
        })
        .map(|(domain, table)| Ok((domain, table_span(domain, table)?)))
        .collect::<Result<Vec<_>, RevisionProfileAuditError>>()?;
    metadata.extend(sprite_spans(profile.level.sprites)?);
    if profile.graphics.split_pointer_planes.is_some() {
        metadata.extend(graphics_spans(profile.graphics)?);
    }
    append_installation_metadata(
        profile,
        rom,
        selected_exanimation,
        selected_exanimation_features,
        &mut metadata,
    )?;
    if let Some(offset) = profile.object_tileset_graphics_offset {
        append_metadata_span(
            rom,
            &mut metadata,
            "graphics.object_tileset_assignments",
            offset,
            crate::OBJECT_TILESET_GRAPHICS_TILESETS * crate::OBJECT_TILESET_GRAPHICS_SLOTS,
        )?;
    }
    let expanded_settings = if let Some(layout) = profile.expanded_settings {
        let expanded_settings = expanded_settings_span(layout)?;
        if expanded_settings.end > rom.logical_len() {
            return Err(RevisionProfileAuditError::MetadataOutOfBounds {
                domain: "expanded_settings",
                end: expanded_settings.end,
                logical_len: rom.logical_len(),
            });
        }
        metadata.push(("expanded_settings", expanded_settings.clone()));
        Some(DirectTableAudit {
            domain: "expanded_settings",
            offset: layout.table_offset,
            entries: layout.entries,
            stride: layout.stride,
            byte_span: expanded_settings,
        })
    } else {
        None
    };
    let header_end = identity
        .internal_header_offset
        .checked_add(INTERNAL_HEADER_AND_VECTORS_LEN)
        .ok_or(RevisionProfileAuditError::EntryCountOverflow)?;
    let header = identity.internal_header_offset..header_end;
    for (domain, span) in &metadata {
        if overlaps(span, &header) {
            return Err(RevisionProfileAuditError::MetadataOverlap {
                domain,
                reserved: "rom.internal_header_and_vectors",
            });
        }
    }
    metadata.push(("rom.internal_header_and_vectors", header));
    let (tables, total_entries) = audit_active_tables(profile, rom, active_tables, &metadata)?;
    Ok(RevisionProfileAudit {
        tables,
        total_entries,
        expanded_settings,
    })
}

fn audit_active_tables(
    profile: &RevisionProfile,
    rom: &RomImage,
    active_tables: Vec<(&'static str, LevelPointerTable)>,
    metadata: &[(&'static str, Range<usize>)],
) -> Result<(Vec<PointerTableAudit>, usize), RevisionProfileAuditError> {
    let mut reports = Vec::with_capacity(metadata.len());
    let mut total_entries = 0usize;
    for (domain, table) in active_tables {
        let report = if domain == "level.sprites" {
            audit_sprite_table(profile, rom, profile.level.sprites, metadata)?
        } else if domain == "graphics" {
            audit_graphics_table(profile, rom, metadata)?
        } else {
            audit_table(profile, rom, domain, table, metadata)?
        };
        reports.push(report);
        total_entries = total_entries
            .checked_add(table.entries)
            .ok_or(RevisionProfileAuditError::EntryCountOverflow)?;
    }
    Ok((reports, total_entries))
}

fn audit_graphics_table(
    profile: &RevisionProfile,
    rom: &RomImage,
    metadata: &[(&'static str, Range<usize>)],
) -> Result<PointerTableAudit, RevisionProfileAuditError> {
    let entries = profile.graphics.pointers.entries;
    let mut targets = Vec::with_capacity(entries);
    for index in 0..entries {
        let displacement = index.checked_mul(profile.graphics.pointers.stride).ok_or(
            RevisionProfileAuditError::PointerOffset {
                domain: "graphics",
                index,
            },
        )?;
        let read = |base: usize, width: usize| {
            let offset =
                base.checked_add(displacement)
                    .ok_or(RevisionProfileAuditError::PointerOffset {
                        domain: "graphics",
                        index,
                    })?;
            rom.read(offset, width)
                .map_err(|error| RevisionProfileAuditError::PointerRead {
                    domain: "graphics",
                    index,
                    error,
                })
        };
        let address = if let Some(planes) = profile.graphics.split_pointer_planes {
            u32::from(read(planes.low_offset, 1)?[0])
                | (u32::from(read(planes.high_offset, 1)?[0]) << 8)
                | (u32::from(read(planes.bank_offset, 1)?[0]) << 16)
        } else {
            let bytes = read(profile.graphics.pointers.offset, 3)?;
            u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16)
        };
        // GFX00..GFX33 are the complete required standard set. Expanded tables reserve later
        // indices for optional auxiliary GFX and ExGFX entries.
        if index >= 0x34 && address == 0 {
            continue;
        }
        push_audited_target(
            profile,
            rom,
            "graphics",
            metadata,
            index,
            address,
            &mut targets,
        )?;
    }
    Ok(finish_audit("graphics", entries, targets))
}

fn append_installation_metadata(
    profile: &RevisionProfile,
    rom: &RomImage,
    selected_exanimation: Option<InstalledExAnimationRomLayout>,
    selected_features: Option<lm_project::InstalledExAnimationFeatureRomLayout>,
    metadata: &mut Vec<(&'static str, Range<usize>)>,
) -> Result<(), RevisionProfileAuditError> {
    for (domain, offset) in installation_markers(profile) {
        append_metadata_span(rom, metadata, domain, offset, 1)?;
    }
    if let Some(locator) = selected_exanimation.and_then(|layout| layout.pointer_locator) {
        append_metadata_span(
            rom,
            metadata,
            "exanimation.locator.first_operand",
            locator.first_operand_offset,
            3,
        )?;
        append_metadata_span(
            rom,
            metadata,
            "exanimation.locator.final_operand",
            locator.final_operand_offset(rom)?,
            3,
        )?;
    }
    if let Some(features) = selected_features {
        let table_offset = features.table_locator.resolve(rom)?;
        let marker_offset = table_offset
            .checked_sub(1)
            .ok_or(RevisionProfileAuditError::EntryCountOverflow)?;
        append_metadata_span(
            rom,
            metadata,
            "exanimation.features.locator.first_operand",
            features.table_locator.first_operand_offset,
            3,
        )?;
        append_metadata_span(
            rom,
            metadata,
            "exanimation.features.locator.final_operand",
            features.table_locator.final_operand_offset(rom)?,
            3,
        )?;
        append_metadata_span(
            rom,
            metadata,
            "exanimation.features.table",
            marker_offset,
            lm_project::EXANIMATION_FEATURE_LEVEL_COUNT + 1,
        )?;
    }
    Ok(())
}

fn append_metadata_span(
    rom: &RomImage,
    metadata: &mut Vec<(&'static str, Range<usize>)>,
    domain: &'static str,
    offset: usize,
    len: usize,
) -> Result<(), RevisionProfileAuditError> {
    let end = offset
        .checked_add(len)
        .ok_or(RevisionProfileAuditError::EntryCountOverflow)?;
    if end > rom.logical_len() {
        return Err(RevisionProfileAuditError::MetadataOutOfBounds {
            domain,
            end,
            logical_len: rom.logical_len(),
        });
    }
    metadata.push((domain, offset..end));
    Ok(())
}

fn installation_markers(profile: &RevisionProfile) -> Vec<(&'static str, usize)> {
    let mut markers = Vec::with_capacity(3);
    if let lm_project::InstalledLayout::Alternatives { primary, fallback } =
        profile.palette_installation
    {
        markers.push(("palette.installation_marker", primary.marker.offset));
        if let Some(fallback) = fallback {
            markers.push((
                "palette.fallback_installation_marker",
                fallback.marker.offset,
            ));
        }
    }
    if let lm_project::InstalledLayout::Alternatives { primary, fallback } =
        profile.exanimation_installation
    {
        markers.push(("exanimation.installation_marker", primary.marker.offset));
        if let Some(fallback) = fallback {
            markers.push((
                "exanimation.fallback_installation_marker",
                fallback.marker.offset,
            ));
        }
    }
    markers
}

fn audit_sprite_table(
    profile: &RevisionProfile,
    rom: &RomImage,
    table: SpritePointerTable,
    metadata: &[(&'static str, Range<usize>)],
) -> Result<PointerTableAudit, RevisionProfileAuditError> {
    let entries = table.low_or_contiguous_table().entries;
    let mut targets = Vec::with_capacity(entries);
    for index in 0..entries {
        let (low, bank) =
            table
                .pointer_ranges(index)
                .map_err(|_| RevisionProfileAuditError::PointerOffset {
                    domain: "level.sprites",
                    index,
                })?;
        let low_bytes = rom.read(low.start, low.len()).map_err(|error| {
            RevisionProfileAuditError::PointerRead {
                domain: "level.sprites",
                index,
                error,
            }
        })?;
        let address = if let Some(bank) = bank {
            let bank = rom.read(bank.start, 1).map_err(|error| {
                RevisionProfileAuditError::PointerRead {
                    domain: "level.sprites",
                    index,
                    error,
                }
            })?[0];
            u32::from_le_bytes([low_bytes[0], low_bytes[1], bank, 0])
        } else {
            u32::from_le_bytes([low_bytes[0], low_bytes[1], low_bytes[2], 0])
        };
        push_audited_target(
            profile,
            rom,
            "level.sprites",
            metadata,
            index,
            address,
            &mut targets,
        )?;
    }
    Ok(finish_audit("level.sprites", entries, targets))
}

fn audit_table(
    profile: &RevisionProfile,
    rom: &RomImage,
    domain: &'static str,
    table: LevelPointerTable,
    metadata: &[(&'static str, Range<usize>)],
) -> Result<PointerTableAudit, RevisionProfileAuditError> {
    let mut targets = Vec::with_capacity(table.entries);
    for index in 0..table.entries {
        let offset = table
            .offset
            .checked_add(
                index
                    .checked_mul(table.stride)
                    .ok_or(RevisionProfileAuditError::PointerOffset { domain, index })?,
            )
            .ok_or(RevisionProfileAuditError::PointerOffset { domain, index })?;
        let bytes =
            rom.read(offset, 3)
                .map_err(|error| RevisionProfileAuditError::PointerRead {
                    domain,
                    index,
                    error,
                })?;
        let address = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
        push_audited_target(profile, rom, domain, metadata, index, address, &mut targets)?;
    }
    Ok(finish_audit(domain, table.entries, targets))
}

fn push_audited_target(
    profile: &RevisionProfile,
    rom: &RomImage,
    domain: &'static str,
    metadata: &[(&'static str, Range<usize>)],
    index: usize,
    address: u32,
    targets: &mut Vec<usize>,
) -> Result<(), RevisionProfileAuditError> {
    let target = snes_to_pc(profile.mapper, address).map_err(|error| {
        RevisionProfileAuditError::InvalidTarget {
            domain,
            index,
            address,
            error,
        }
    })?;
    if target >= rom.logical_len() {
        return Err(RevisionProfileAuditError::TargetOutOfBounds {
            domain,
            index,
            address,
            target,
            logical_len: rom.logical_len(),
        });
    }
    if let Some((metadata, _)) = metadata.iter().find(|(_, range)| range.contains(&target)) {
        return Err(RevisionProfileAuditError::TargetInMetadata {
            domain,
            index,
            target,
            metadata,
        });
    }
    targets.push(target);
    Ok(())
}

fn finish_audit(
    domain: &'static str,
    entries: usize,
    mut targets: Vec<usize>,
) -> PointerTableAudit {
    targets.sort_unstable();
    let minimum_target = targets.first().copied().unwrap_or(0);
    let maximum_target = targets.last().copied().unwrap_or(0);
    targets.dedup();
    PointerTableAudit {
        domain,
        entries,
        unique_targets: targets.len(),
        minimum_target,
        maximum_target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{Mapper, pc_to_snes};

    fn audited_fixture() -> (RevisionProfile, RomImage) {
        let profile = crate::test_support::profile();
        let mut bytes = vec![0; 0x40_8000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes[0x7fd9] = 1;
        let pointer = pc_to_snes(Mapper::ExLoRom, 0x1_0000).unwrap().to_le_bytes();
        for (_, table) in tables(&profile) {
            for index in 0..table.entries {
                let offset = table.offset + index * table.stride;
                bytes[offset..offset + 3].copy_from_slice(&pointer[..3]);
            }
        }
        (profile, RomImage::from_bytes(bytes).unwrap())
    }

    #[test]
    fn every_declared_entry_is_audited_and_summarized() {
        let (profile, rom) = audited_fixture();
        let report = audit(&profile, &rom).unwrap();
        assert_eq!(report.tables.len(), 16);
        assert_eq!(
            report.total_entries,
            tables(&profile)
                .iter()
                .map(|(_, table)| table.entries)
                .sum()
        );
        assert!(report.tables.iter().all(|table| {
            table.unique_targets == 1
                && table.minimum_target == 0x1_0000
                && table.maximum_target == 0x1_0000
        }));
        assert_eq!(
            report.expanded_settings,
            Some(DirectTableAudit {
                domain: "expanded_settings",
                offset: 0x2_0000,
                entries: 0x200,
                stride: 0x20,
                byte_span: 0x2_0000..0x2_4000,
            })
        );
    }

    #[test]
    fn optional_layer2_table_is_audited_as_pointer_metadata() {
        let (mut profile, mut rom) = audited_fixture();
        let pointers = LevelPointerTable {
            offset: 0x2_9000,
            entries: 0x200,
            stride: 3,
        };
        profile.layer2 = Some(lm_project::LevelLayer2RomLayout {
            mapper: profile.mapper,
            pointers,
            background_bank_substitution: None,
            legacy_pointer_redirect: None,
            descriptor_table: None,
            maximum_compressed_len: 0x8000,
            tilemap_encoding: lm_project::LevelLayer2TilemapEncoding::SplitPlanes,
        });
        let pointer = pc_to_snes(profile.mapper, 0x1_0000).unwrap().to_le_bytes();
        for index in 0..pointers.entries {
            rom.write(pointers.offset + index * pointers.stride, &pointer[..3])
                .unwrap();
        }
        let report = audit(&profile, &rom).unwrap();
        let layer2 = report
            .tables
            .iter()
            .find(|table| table.domain == "level.layer2")
            .unwrap();
        assert_eq!(layer2.entries, 0x200);
        assert_eq!(layer2.unique_targets, 1);
    }

    #[test]
    fn split_sprite_low_words_and_parallel_banks_are_audited() {
        let (mut profile, mut rom) = audited_fixture();
        let original = profile.level.sprites.low_or_contiguous_table();
        let low_words = LevelPointerTable {
            stride: 2,
            ..original
        };
        let banks = LevelPointerTable {
            offset: 0x2_8000,
            entries: original.entries,
            stride: 1,
        };
        profile.level.sprites = SpritePointerTable::SplitBankTable { low_words, banks };
        let pointer = pc_to_snes(profile.mapper, 0x1_0000).unwrap().to_le_bytes();
        for index in 0..original.entries {
            rom.write(low_words.offset + index * 2, &pointer[..2])
                .unwrap();
            rom.write(banks.offset + index, &pointer[2..3]).unwrap();
        }
        let report = audit(&profile, &rom).unwrap();
        let sprites = report
            .tables
            .iter()
            .find(|table| table.domain == "level.sprites")
            .unwrap();
        assert_eq!(sprites.entries, original.entries);
        assert_eq!(sprites.unique_targets, 1);
        assert_eq!(sprites.minimum_target, 0x1_0000);
    }

    #[test]
    fn split_graphics_planes_are_audited_as_one_pointer_table() {
        let (mut profile, mut rom) = audited_fixture();
        profile.graphics.pointers = LevelPointerTable {
            offset: 0x2_a000,
            entries: 0x100,
            stride: 1,
        };
        let planes = lm_project::GraphicsPointerPlanes {
            low_offset: 0x2_a000,
            high_offset: 0x2_a100,
            bank_offset: 0x2_a200,
            entries: 0x100,
            stride: 1,
        };
        profile.graphics.split_pointer_planes = Some(planes);
        let pointer = pc_to_snes(profile.mapper, 0x1_0000).unwrap().to_le_bytes();
        for index in 0..planes.entries {
            rom.write(planes.low_offset + index, &pointer[0..1])
                .unwrap();
            rom.write(planes.high_offset + index, &pointer[1..2])
                .unwrap();
            rom.write(planes.bank_offset + index, &pointer[2..3])
                .unwrap();
        }
        let report = audit(&profile, &rom).unwrap();
        let graphics = report
            .tables
            .iter()
            .find(|table| table.domain == "graphics")
            .unwrap();
        assert_eq!(graphics.entries, 0x100);
        assert_eq!(graphics.unique_targets, 1);
        assert_eq!(graphics.minimum_target, 0x1_0000);
        assert_eq!(graphics.maximum_target, 0x1_0000);
    }

    #[test]
    fn expanded_graphics_audit_accepts_only_extended_zero_pointer_sentinels() {
        let (mut profile, mut rom) = audited_fixture();
        profile.graphics.pointers = LevelPointerTable {
            offset: 0x2_a000,
            entries: 0x82,
            stride: 3,
        };
        profile.graphics.split_pointer_planes = None;
        let pointer = pc_to_snes(profile.mapper, 0x1_0000).unwrap().to_le_bytes();
        for index in 0..0x34 {
            rom.write(profile.graphics.pointers.offset + index * 3, &pointer[..3])
                .unwrap();
        }
        rom.write(profile.graphics.pointers.offset + 0x81 * 3, &pointer[..3])
            .unwrap();
        let report = audit(&profile, &rom).unwrap();
        let graphics = report
            .tables
            .iter()
            .find(|table| table.domain == "graphics")
            .unwrap();
        assert_eq!(graphics.entries, 0x82);
        assert_eq!(graphics.unique_targets, 1);

        rom.write(profile.graphics.pointers.offset + 0x33 * 3, &[0; 3])
            .unwrap();
        assert!(matches!(
            audit(&profile, &rom),
            Err(RevisionProfileAuditError::InvalidTarget {
                domain: "graphics",
                index: 0x33,
                address: 0,
                ..
            })
        ));
    }

    #[test]
    fn profiles_without_direct_table_report_absence_without_weakening_pointer_audit() {
        let (mut profile, rom) = audited_fixture();
        profile.expanded_settings = None;
        let report = audit(&profile, &rom).unwrap();
        assert_eq!(report.tables.len(), 16);
        assert_eq!(report.expanded_settings, None);
    }

    #[test]
    fn absent_optional_installations_skip_candidate_pointer_tables() {
        let (mut profile, mut rom) = audited_fixture();
        profile.palette_installation = lm_project::InstalledLayout::Absent;
        profile.exanimation_installation = lm_project::InstalledLayout::Absent;
        rom.write(profile.palette.pointers.offset, &[0xff; 3])
            .unwrap();
        rom.write(profile.exanimation.pointers.offset, &[0xff; 3])
            .unwrap();
        let report = audit(&profile, &rom).unwrap();
        assert!(
            report
                .tables
                .iter()
                .all(|table| table.domain != "palette" && table.domain != "exanimation")
        );
        assert_eq!(report.tables.len(), tables(&profile).len() - 2);
    }

    #[test]
    fn marker_gated_table_is_audited_only_when_installed() {
        let (mut profile, mut rom) = audited_fixture();
        let marker = lm_project::InstallationMarker {
            offset: 0x2_8800,
            expected: 0xc2,
        };
        profile.palette_installation = lm_project::InstalledLayout::Alternatives {
            primary: lm_project::GatedLayout {
                marker,
                layout: profile.palette,
            },
            fallback: None,
        };
        rom.write(marker.offset, &[0xff]).unwrap();
        assert!(
            audit(&profile, &rom)
                .unwrap()
                .tables
                .iter()
                .all(|table| table.domain != "palette")
        );
        rom.write(marker.offset, &[marker.expected]).unwrap();
        assert!(
            audit(&profile, &rom)
                .unwrap()
                .tables
                .iter()
                .any(|table| table.domain == "palette")
        );
    }

    #[test]
    fn invalid_mapped_and_out_of_image_targets_name_domain_and_index() {
        let (profile, mut rom) = audited_fixture();
        rom.write(profile.level.layer1.offset, &[0, 0, 0]).unwrap();
        assert!(matches!(
            audit(&profile, &rom),
            Err(RevisionProfileAuditError::InvalidTarget {
                domain: "level.layer1",
                index: 0,
                ..
            })
        ));

        let (profile, mut rom) = audited_fixture();
        let outside = pc_to_snes(profile.mapper, 0x50_0000).unwrap().to_le_bytes();
        rom.write(profile.level.layer1.offset, &outside[..3])
            .unwrap();
        assert!(matches!(
            audit(&profile, &rom),
            Err(RevisionProfileAuditError::TargetOutOfBounds {
                domain: "level.layer1",
                index: 0,
                ..
            })
        ));
    }

    #[test]
    fn table_bytes_must_fit_the_actual_rom_not_only_mapper_space() {
        let (mut profile, rom) = audited_fixture();
        profile.level.layer1 = LevelPointerTable {
            offset: rom.logical_len() - 2,
            entries: 1,
            stride: 3,
        };
        assert!(matches!(
            audit(&profile, &rom),
            Err(RevisionProfileAuditError::PointerRead {
                domain: "level.layer1",
                index: 0,
                ..
            })
        ));
    }

    #[test]
    fn tables_and_payload_targets_cannot_alias_reserved_metadata() {
        let (mut profile, rom) = audited_fixture();
        profile.level.layer1 = LevelPointerTable {
            offset: 0x7fc0,
            entries: 1,
            stride: 3,
        };
        assert!(matches!(
            audit(&profile, &rom),
            Err(RevisionProfileAuditError::MetadataOverlap {
                domain: "level.layer1",
                reserved: "rom.internal_header_and_vectors",
            })
        ));

        let (profile, mut rom) = audited_fixture();
        let table_target = pc_to_snes(profile.mapper, profile.level.layer1.offset)
            .unwrap()
            .to_le_bytes();
        rom.write(profile.level.layer1.offset, &table_target[..3])
            .unwrap();
        assert!(matches!(
            audit(&profile, &rom),
            Err(RevisionProfileAuditError::TargetInMetadata {
                domain: "level.layer1",
                index: 0,
                metadata: "level.layer1",
                ..
            })
        ));

        let (profile, mut rom) = audited_fixture();
        let header_target = pc_to_snes(profile.mapper, 0x7fc0).unwrap().to_le_bytes();
        rom.write(profile.level.layer1.offset, &header_target[..3])
            .unwrap();
        assert!(matches!(
            audit(&profile, &rom),
            Err(RevisionProfileAuditError::TargetInMetadata {
                metadata: "rom.internal_header_and_vectors",
                ..
            })
        ));
    }
}
