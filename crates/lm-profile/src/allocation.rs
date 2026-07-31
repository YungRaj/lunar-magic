//! Profile-wide protection for copy-on-write native allocations.

use crate::{RevisionProfile, RevisionProfileError};
use lm_project::{LevelPointerTable, SpritePointerTable};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::pc_to_snes;
use std::fmt;
use std::ops::Range;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RevisionAllocationError {
    Profile(RevisionProfileError),
    InvalidSearchRange {
        start: usize,
        end: usize,
        image_len: usize,
    },
    UnmappedSearchBoundary(usize),
    EmptyPointerTable {
        domain: &'static str,
    },
    RangeOverflow {
        domain: &'static str,
    },
    ProtectedRangeOutsideImage {
        domain: &'static str,
        end: usize,
        image_len: usize,
    },
    Installation(lm_project::InstalledLayoutError),
    PointerLocator(lm_project::PointerLocatorError),
    Rom(lm_rom::RomError),
    OptionalSubsystemUnavailable(&'static str),
}

impl fmt::Display for RevisionAllocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid revision allocation policy: {self:?}")
    }
}

impl std::error::Error for RevisionAllocationError {}

impl From<RevisionProfileError> for RevisionAllocationError {
    fn from(value: RevisionProfileError) -> Self {
        Self::Profile(value)
    }
}

impl From<lm_project::InstalledLayoutError> for RevisionAllocationError {
    fn from(value: lm_project::InstalledLayoutError) -> Self {
        Self::Installation(value)
    }
}

impl From<lm_project::PointerLocatorError> for RevisionAllocationError {
    fn from(value: lm_project::PointerLocatorError) -> Self {
        Self::PointerLocator(value)
    }
}

impl From<lm_rom::RomError> for RevisionAllocationError {
    fn from(value: lm_rom::RomError) -> Self {
        Self::Rom(value)
    }
}

impl RevisionProfile {
    /// Builds one bank-aware allocation policy protecting every profile table and the complete
    /// 64-byte SNES internal-header/vector block.
    ///
    /// The search extent is always explicit. No default free-space region is inferred from fill
    /// bytes because zero/`0xff` runs may contain untagged game or ecosystem-tool data.
    ///
    /// # Errors
    ///
    /// Rejects invalid profiles, empty/out-of-image/unmapped search ranges, arithmetic overflow,
    /// empty tables, or metadata ranges outside the current logical image.
    pub fn allocation_policy(
        &self,
        search: Range<usize>,
        image_len: usize,
        internal_header_offset: usize,
    ) -> Result<AllocationPolicy, RevisionAllocationError> {
        self.validate()?;
        validate_search(self, &search, image_len)?;
        let mut protected = Vec::with_capacity(18);
        for (domain, table) in profile_tables(self) {
            if domain != "level.sprites"
                && !(domain == "graphics" && self.graphics.split_pointer_planes.is_some())
            {
                protected.push(table_range(domain, table, image_len)?);
            }
        }
        protected.extend(sprite_ranges(self.level.sprites, image_len)?);
        protected.extend(graphics_ranges(self.graphics, image_len)?);
        protected.extend(installation_marker_ranges(self, image_len)?);
        if let Some(layer2) = self.layer2 {
            protected.push(table_range("level.layer2", layer2.pointers, image_len)?);
            if let Some(descriptor) = layer2.descriptor_table {
                protected.push(component_range(
                    "level.layer2.descriptor",
                    LevelPointerTable {
                        offset: descriptor.offset,
                        entries: descriptor.entries,
                        stride: descriptor.stride,
                    },
                    1,
                    image_len,
                )?);
            }
        }
        if let Some(layout) = self.expanded_settings {
            protected.push(expanded_settings_range(layout, image_len)?);
        }
        protected.push(protected_range(
            "internal_header",
            internal_header_offset,
            0x40,
            image_len,
        )?);
        Ok(AllocationPolicy {
            search,
            bank_size: Some(0x8000),
            fill_bytes: vec![0x00, 0xff],
            protected,
        })
    }

    /// Builds a policy that additionally resolves and protects allocator-dependent installed
    /// pointer tables and both embedded locator operands.
    ///
    /// # Errors
    ///
    /// Returns [`RevisionAllocationError`] for the ordinary policy checks or malformed installed
    /// hook operands.
    pub fn allocation_policy_for_rom(
        &self,
        search: Range<usize>,
        rom: &lm_rom::RomImage,
        internal_header_offset: usize,
    ) -> Result<AllocationPolicy, RevisionAllocationError> {
        let mut policy =
            self.allocation_policy(search, rom.logical_len(), internal_header_offset)?;
        if let Some(installed) = self.exanimation_installation.resolve(rom)? {
            let resolved = installed.resolve(rom)?;
            let table = table_range(
                "exanimation.installed",
                resolved.payload.pointers,
                rom.logical_len(),
            )?;
            if !policy.protected.contains(&table) {
                policy.protected.push(table);
            }
            if let Some(locator) = installed.pointer_locator {
                for (domain, offset) in [
                    (
                        "exanimation.locator.first_operand",
                        locator.first_operand_offset,
                    ),
                    (
                        "exanimation.locator.final_operand",
                        locator.final_operand_offset(rom)?,
                    ),
                ] {
                    let range = protected_range(domain, offset, 3, rom.logical_len())?;
                    if !policy.protected.contains(&range) {
                        policy.protected.push(range);
                    }
                }
            }
        }
        if let Some(features) = self.exanimation_feature_installation.resolve(rom)? {
            let table_offset = features.table_locator.resolve(rom)?;
            let marker_offset =
                table_offset
                    .checked_sub(1)
                    .ok_or(RevisionAllocationError::RangeOverflow {
                        domain: "exanimation.features.table",
                    })?;
            for range in [
                protected_range(
                    "exanimation.features.final_operand",
                    features.table_locator.final_operand_offset(rom)?,
                    3,
                    rom.logical_len(),
                )?,
                protected_range(
                    "exanimation.features.table",
                    marker_offset,
                    lm_project::EXANIMATION_FEATURE_LEVEL_COUNT + 1,
                    rom.logical_len(),
                )?,
            ] {
                if !policy.protected.contains(&range) {
                    policy.protected.push(range);
                }
            }
        }
        Ok(policy)
    }
}

fn installation_marker_ranges(
    profile: &RevisionProfile,
    image_len: usize,
) -> Result<Vec<ProtectedRange>, RevisionAllocationError> {
    let mut markers = Vec::with_capacity(3);
    let marker_range = |domain, marker: lm_project::InstallationMarker| {
        protected_range(domain, marker.offset, 1, image_len)
    };
    if let lm_project::InstalledLayout::Alternatives { primary, fallback } =
        profile.palette_installation
    {
        markers.push(marker_range("palette.installation_marker", primary.marker)?);
        if let Some(fallback) = fallback {
            markers.push(marker_range(
                "palette.fallback_installation_marker",
                fallback.marker,
            )?);
        }
    }
    if let lm_project::InstalledLayout::Alternatives { primary, fallback } =
        profile.exanimation_installation
    {
        markers.push(marker_range(
            "exanimation.installation_marker",
            primary.marker,
        )?);
        if let Some(locator) = primary.layout.pointer_locator {
            markers.push(protected_range(
                "exanimation.primary_locator_operand",
                locator.first_operand_offset,
                3,
                image_len,
            )?);
        }
        if let Some(fallback) = fallback {
            markers.push(marker_range(
                "exanimation.fallback_installation_marker",
                fallback.marker,
            )?);
            if let Some(locator) = fallback.layout.pointer_locator {
                markers.push(protected_range(
                    "exanimation.fallback_locator_operand",
                    locator.first_operand_offset,
                    3,
                    image_len,
                )?);
            }
        }
    } else if let lm_project::InstalledLayout::Unconditional(layout) =
        profile.exanimation_installation
        && let Some(locator) = layout.pointer_locator
    {
        markers.push(protected_range(
            "exanimation.locator_operand",
            locator.first_operand_offset,
            3,
            image_len,
        )?);
    }
    let mut feature_variant =
        |prefix: &'static str, layout: lm_project::InstalledExAnimationFeatureRomLayout| {
            markers.push(protected_range(
                prefix,
                layout.table_locator.first_operand_offset,
                3,
                image_len,
            )?);
            Ok::<_, RevisionAllocationError>(())
        };
    match profile.exanimation_feature_installation {
        lm_project::InstalledLayout::Absent => {}
        lm_project::InstalledLayout::Unconditional(layout) => {
            feature_variant("exanimation.features.locator_operand", layout)?;
        }
        lm_project::InstalledLayout::Alternatives { primary, fallback } => {
            feature_variant(
                "exanimation.features.primary_locator_operand",
                primary.layout,
            )?;
            if let Some(fallback) = fallback {
                feature_variant(
                    "exanimation.features.fallback_locator_operand",
                    fallback.layout,
                )?;
            }
        }
    }
    Ok(markers)
}

fn sprite_ranges(
    layout: SpritePointerTable,
    image_len: usize,
) -> Result<Vec<ProtectedRange>, RevisionAllocationError> {
    match layout {
        SpritePointerTable::Contiguous(table) => {
            Ok(vec![component_range("level.sprites", table, 3, image_len)?])
        }
        SpritePointerTable::SplitSharedBank {
            low_words,
            bank_offset,
        } => Ok(vec![
            component_range("level.sprites", low_words, 2, image_len)?,
            protected_range("level.sprites.bank", bank_offset, 1, image_len)?,
        ]),
        SpritePointerTable::SplitBankTable { low_words, banks } => Ok(vec![
            component_range("level.sprites", low_words, 2, image_len)?,
            component_range("level.sprites.banks", banks, 1, image_len)?,
        ]),
    }
}

fn graphics_ranges(
    layout: lm_project::GraphicsRomLayout,
    image_len: usize,
) -> Result<Vec<ProtectedRange>, RevisionAllocationError> {
    let Some(planes) = layout.split_pointer_planes else {
        return Ok(Vec::new());
    };
    let component = |domain, offset| {
        component_range(
            domain,
            LevelPointerTable {
                offset,
                entries: planes.entries,
                stride: planes.stride,
            },
            1,
            image_len,
        )
    };
    Ok(vec![
        component("graphics.low", planes.low_offset)?,
        component("graphics.high", planes.high_offset)?,
        component("graphics.bank", planes.bank_offset)?,
    ])
}

fn component_range(
    domain: &'static str,
    table: LevelPointerTable,
    width: usize,
    image_len: usize,
) -> Result<ProtectedRange, RevisionAllocationError> {
    let len = table
        .entries
        .checked_sub(1)
        .ok_or(RevisionAllocationError::EmptyPointerTable { domain })?
        .checked_mul(table.stride)
        .and_then(|last| last.checked_add(width))
        .ok_or(RevisionAllocationError::RangeOverflow { domain })?;
    protected_range(domain, table.offset, len, image_len)
}

fn expanded_settings_range(
    layout: lm_project::ExpandedLevelSettingsLayout,
    image_len: usize,
) -> Result<ProtectedRange, RevisionAllocationError> {
    let len = layout
        .entries
        .checked_sub(1)
        .and_then(|last| last.checked_mul(layout.stride))
        .and_then(|last| last.checked_add(lm_level::ExpandedLevelSettingsRecord::ENCODED_LEN))
        .ok_or(RevisionAllocationError::RangeOverflow {
            domain: "expanded_settings",
        })?;
    protected_range("expanded_settings", layout.table_offset, len, image_len)
}

fn validate_search(
    profile: &RevisionProfile,
    search: &Range<usize>,
    image_len: usize,
) -> Result<(), RevisionAllocationError> {
    if search.start >= search.end || search.end > image_len {
        return Err(RevisionAllocationError::InvalidSearchRange {
            start: search.start,
            end: search.end,
            image_len,
        });
    }
    for boundary in [search.start, search.end - 1] {
        pc_to_snes(profile.mapper, boundary)
            .map_err(|_| RevisionAllocationError::UnmappedSearchBoundary(boundary))?;
    }
    Ok(())
}

fn profile_tables(profile: &RevisionProfile) -> [(&'static str, LevelPointerTable); 16] {
    [
        ("level.layer1", profile.level.layer1),
        (
            "level.sprites",
            profile.level.sprites.low_or_contiguous_table(),
        ),
        ("map16.graphics", profile.map16.graphics),
        ("map16.acts_like", profile.map16.acts_like),
        ("graphics", profile.graphics.pointers),
        ("palette", profile.palette.pointers),
        ("exanimation", profile.exanimation.pointers),
        ("overworld.layer1", profile.overworld.layers.layer1),
        ("overworld.layer2", profile.overworld.layers.layer2),
        (
            "overworld.event_sources",
            profile.overworld.event_reveals.sources,
        ),
        (
            "overworld.event_destinations",
            profile.overworld.event_reveals.destinations,
        ),
        ("overworld.endpoints", profile.overworld.endpoints.pointers),
        ("overworld.messages", profile.overworld.messages.pointers),
        ("overworld.sprites", profile.overworld.sprites.pointers),
        ("overworld.palette", profile.overworld.palette.pointers),
        ("overworld.animation", profile.overworld.animation.pointers),
    ]
}

fn table_range(
    domain: &'static str,
    table: LevelPointerTable,
    image_len: usize,
) -> Result<ProtectedRange, RevisionAllocationError> {
    let last = table
        .entries
        .checked_sub(1)
        .ok_or(RevisionAllocationError::EmptyPointerTable { domain })?;
    let len = last
        .checked_mul(table.stride)
        .and_then(|offset| offset.checked_add(3))
        .ok_or(RevisionAllocationError::RangeOverflow { domain })?;
    protected_range(domain, table.offset, len, image_len)
}

fn protected_range(
    domain: &'static str,
    start: usize,
    len: usize,
    image_len: usize,
) -> Result<ProtectedRange, RevisionAllocationError> {
    let end = start
        .checked_add(len)
        .ok_or(RevisionAllocationError::RangeOverflow { domain })?;
    if end > image_len {
        return Err(RevisionAllocationError::ProtectedRangeOutsideImage {
            domain,
            end,
            image_len,
        });
    }
    Ok(ProtectedRange(start..end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_every_table_shape_and_full_internal_header() {
        let profile = crate::test_support::profile();
        let policy = profile
            .allocation_policy(0x6000..0x7000, 0x3_0000, 0x7fc0)
            .unwrap();
        assert_eq!(policy.protected.len(), 18);
        assert!(policy.protected.contains(&ProtectedRange(0x7fc0..0x8000)));
        let level = profile.level.layer1;
        let end = level.offset + (level.entries - 1) * level.stride + 3;
        assert!(
            policy
                .protected
                .contains(&ProtectedRange(level.offset..end))
        );
    }

    #[test]
    fn rejects_unsafe_search_and_metadata_outside_the_image() {
        let profile = crate::test_support::profile();
        assert!(matches!(
            profile.allocation_policy(0x8000..0x8000, 0x8000, 0x7fc0),
            Err(RevisionAllocationError::InvalidSearchRange { .. })
        ));
        assert!(matches!(
            profile.allocation_policy(0x100..0x200, 0x1000, 0x7fc0),
            Err(RevisionAllocationError::ProtectedRangeOutsideImage { .. })
        ));
    }

    #[test]
    fn split_sprite_components_are_all_protected() {
        let mut profile = crate::test_support::profile();
        let low_words = LevelPointerTable {
            stride: 2,
            ..profile.level.sprites.low_or_contiguous_table()
        };
        profile.level.sprites = SpritePointerTable::SplitBankTable {
            low_words,
            banks: LevelPointerTable {
                offset: 0x2_8000,
                entries: low_words.entries,
                stride: 1,
            },
        };
        let policy = profile
            .allocation_policy(0x6000..0x7000, 0x3_0000, 0x7fc0)
            .unwrap();
        assert!(policy.protected.contains(&ProtectedRange(
            low_words.offset..low_words.offset + low_words.entries * 2
        )));
        assert!(
            policy
                .protected
                .contains(&ProtectedRange(0x2_8000..0x2_8200))
        );
    }

    #[test]
    fn split_graphics_planes_are_independently_protected() {
        let mut profile = crate::test_support::profile();
        profile.graphics.pointers = LevelPointerTable {
            offset: 0x2_a000,
            entries: 0x100,
            stride: 1,
        };
        profile.graphics.split_pointer_planes = Some(lm_project::GraphicsPointerPlanes {
            low_offset: 0x2_a000,
            high_offset: 0x2_a100,
            bank_offset: 0x2_a200,
            entries: 0x100,
            stride: 1,
        });
        let policy = profile
            .allocation_policy(0x6000..0x7000, 0x3_0000, 0x7fc0)
            .unwrap();
        for start in [0x2_a000, 0x2_a100, 0x2_a200] {
            assert!(
                policy
                    .protected
                    .contains(&ProtectedRange(start..start + 0x100))
            );
        }
        assert!(
            !policy
                .protected
                .contains(&ProtectedRange(0x2_a000..0x2_a102))
        );
    }

    #[test]
    fn optional_layer2_pointer_and_descriptor_tables_are_protected() {
        let mut profile = crate::test_support::profile();
        let pointers = LevelPointerTable {
            offset: 0x2_9000,
            entries: 0x200,
            stride: 3,
        };
        let descriptor = lm_project::LevelLayer2DescriptorTable {
            offset: 0x2_8800,
            entries: 0x200,
            stride: 1,
        };
        profile.layer2 = Some(lm_project::LevelLayer2RomLayout {
            mapper: profile.mapper,
            pointers,
            background_bank_substitution: None,
            legacy_pointer_redirect: None,
            descriptor_table: Some(descriptor),
            maximum_compressed_len: 0x8000,
            tilemap_encoding: lm_project::LevelLayer2TilemapEncoding::SplitPlanes,
        });
        let policy = profile
            .allocation_policy(0x6000..0x7000, 0x3_0000, 0x7fc0)
            .unwrap();
        assert!(policy.protected.contains(&ProtectedRange(
            pointers.offset..pointers.offset + pointers.entries * pointers.stride
        )));
        assert!(policy.protected.contains(&ProtectedRange(
            descriptor.offset..descriptor.offset + descriptor.entries
        )));
    }

    #[test]
    fn optional_installation_markers_are_protected() {
        let mut profile = crate::test_support::profile();
        profile.palette_installation = lm_project::InstalledLayout::Alternatives {
            primary: lm_project::GatedLayout {
                marker: lm_project::InstallationMarker {
                    offset: 0x2_8800,
                    expected: 0xc2,
                },
                layout: profile.palette,
            },
            fallback: None,
        };
        profile.exanimation_installation = lm_project::InstalledLayout::Alternatives {
            primary: lm_project::GatedLayout {
                marker: lm_project::InstallationMarker {
                    offset: 0x2_8810,
                    expected: 0x22,
                },
                layout: lm_project::InstalledExAnimationRomLayout {
                    payload: profile.exanimation,
                    pointer_presence_mask: 0x00ff_ff00,
                    pointer_locator: None,
                },
            },
            fallback: Some(lm_project::GatedLayout {
                marker: lm_project::InstallationMarker {
                    offset: 0x2_8820,
                    expected: 0x22,
                },
                layout: lm_project::InstalledExAnimationRomLayout {
                    payload: profile.exanimation,
                    pointer_presence_mask: 0x00ff_0000,
                    pointer_locator: None,
                },
            }),
        };
        let policy = profile
            .allocation_policy(0x6000..0x7000, 0x3_0000, 0x7fc0)
            .unwrap();
        for offset in [0x2_8800, 0x2_8810, 0x2_8820] {
            assert!(
                policy
                    .protected
                    .contains(&ProtectedRange(offset..offset + 1))
            );
        }
    }

    #[test]
    fn resolved_feature_table_operands_marker_and_storage_are_protected() {
        let mut profile = crate::test_support::profile();
        let first_operand = 0x2_8811;
        let runtime = 0x2_c000;
        let final_operand = runtime + 0x46;
        let exanimation_operand = runtime - 0x20;
        let table = 0x2_a001;
        if let lm_project::InstalledLayout::Unconditional(layout) =
            &mut profile.exanimation_installation
        {
            layout.pointer_locator = Some(lm_project::ChainedSnesPointerLocator {
                mapper: profile.mapper,
                first_operand_offset: first_operand,
                final_operand_displacement: -0x20,
            });
        }
        profile.exanimation_feature_installation = lm_project::InstalledLayout::Unconditional(
            lm_project::InstalledExAnimationFeatureRomLayout {
                table_locator: lm_project::ChainedSnesPointerLocator {
                    mapper: profile.mapper,
                    first_operand_offset: first_operand,
                    final_operand_displacement: 0x46,
                },
            },
        );
        let mut rom = lm_rom::RomImage::from_bytes(vec![0xff; 0x3_0000]).unwrap();
        for (offset, target) in [
            (first_operand, runtime),
            (exanimation_operand, profile.exanimation.pointers.offset),
            (final_operand, table),
        ] {
            let mapped = lm_rom::pc_to_snes(profile.mapper, target).unwrap();
            rom.write(offset, &mapped.to_le_bytes()[..3]).unwrap();
        }
        let policy = profile
            .allocation_policy_for_rom(0x6000..0x7000, &rom, 0x7fc0)
            .unwrap();
        for range in [
            ProtectedRange(first_operand..first_operand + 3),
            ProtectedRange(final_operand..final_operand + 3),
            ProtectedRange(table - 1..table + lm_project::EXANIMATION_FEATURE_LEVEL_COUNT),
        ] {
            assert!(policy.protected.contains(&range), "missing {range:?}");
        }
    }
}
