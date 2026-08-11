//! Transactional SMW US revision-0 expanded-settings installation plan.

use crate::{
    ExpandedSettingsAllocationFixupEncoding, ExpandedSettingsEntryContinuation,
    ExpandedSettingsRuntimeBundleError, ExpandedSettingsRuntimeLayout,
    SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_ALLOCATION_FIXUPS,
    SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS, SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER_OFFSET,
    SmwUsV1ExpandedSettingsAllocation, SmwUsV1ExpandedSettingsAllocationError,
    SmwUsV1ExpandedSettingsRecordGeneration, smw_us_v1_expanded_settings_fixed_writes,
    smw_us_v1_upgrade_expanded_settings_record,
};
use lm_level::ExpandedOverworldSettings;
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, RelocatablePatchPlan};
use lm_rats::{AllocationPolicy, HeaderError, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, RomImage, pc_to_snes, snes_to_pc};
use sha2::{Digest, Sha256};

pub const SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START: usize = 0x08_7ff8;
pub const SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_END: usize = 0x10_0000;
pub const SMW_US_V1_EXPANDED_SETTINGS_MAXIMUM_LOROM_LEN: usize = 0x40_0000;
pub const SMW_US_V1_GFX_EXPANDED_SETTINGS_ALLOCATION_START: usize = 0x08_0028;
pub const SMW_US_V1_GFX_EXPANDED_SETTINGS_ALLOCATION_END: usize = 0x08_6e30;
pub const SMW_US_V1_CHECKSUM_FIELD: usize = 0x00_7fdc;
pub const SMW_US_V1_EXPANDED_SETTINGS_GENERATION_102_MARKER: [u8; 4] = [0x4c, 0x4d, 0x02, 0x01];
pub const SMW_US_V1_LEGACY_GRAPHICS_GENERATION_100_MARKER_OFFSET: usize = 0x0f_b604;
pub const SMW_US_V1_LEGACY_GRAPHICS_GENERATION_100_MARKER: [u8; 4] = [0x4c, 0x4d, 0x00, 0x01];
pub const SMW_US_V1_LEGACY_GRAPHICS_GENERATION_101_MARKER_OFFSET: usize = 0x06_ff37;
pub const SMW_US_V1_LEGACY_GRAPHICS_GENERATION_101_MARKER: [u8; 4] = [0x4c, 0x4d, 0x01, 0x01];
pub const SMW_US_V1_EXPANDED_SETTINGS_GENERATION_101_ALLOCATION_LEN: usize = 0x6d00;

const GENERATION_102_BLOCK_220_HEX: &str =
    include_str!("assets/expanded_settings_runtime_generation_102_block_220.hex");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1ExpandedSettingsGeneration102Migration {
    pub plan: RelocatablePatchPlan,
    pub previous_allocation: RatsBlock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1ExpandedSettingsGeneration101Migration {
    pub plan: RelocatablePatchPlan,
    pub previous_allocation: RatsBlock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1ExpandedSettingsGeneration100Migration {
    pub plan: RelocatablePatchPlan,
    pub previous_allocation: RatsBlock,
}

#[derive(Debug)]
pub enum SmwUsV1ExpandedSettingsGeneration100MigrationError {
    Legacy(SmwUsV1ExpandedSettingsGeneration101MigrationError),
}

impl std::fmt::Display for SmwUsV1ExpandedSettingsGeneration100MigrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cannot migrate SMW-US expanded settings generation 1.00: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1ExpandedSettingsGeneration100MigrationError {}

#[derive(Debug)]
pub enum SmwUsV1ExpandedSettingsGeneration101MigrationError {
    CurrentMarkerPresent,
    LegacyMarkerMismatch,
    InvalidRuntimePointer,
    RuntimeAddress(RomError),
    RuntimeBeforeHeader(usize),
    RuntimeHeader(HeaderError),
    RuntimeOwnership { expected: usize, actual: usize },
    RuntimeLength(usize),
    NonFillPrefix { offset: usize, value: u8 },
    SourceRange { offset: usize, len: usize },
    RuntimeOperandMismatch { offset: usize },
    RuntimeDigestMismatch { offset: usize },
    RecordIndex(usize),
    Plan(ExpandedSettingsInstallPlanError),
}

impl std::fmt::Display for SmwUsV1ExpandedSettingsGeneration101MigrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cannot migrate SMW-US expanded settings generation 1.01: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1ExpandedSettingsGeneration101MigrationError {}

#[derive(Debug)]
pub enum SmwUsV1ExpandedSettingsGeneration102MigrationError {
    MissingGeneration,
    MarkerMismatch,
    InvalidRuntimePointer,
    RuntimeAddress(RomError),
    RuntimeBeforeHeader(usize),
    RuntimeHeader(HeaderError),
    RuntimeOwnership {
        expected: usize,
        actual: usize,
    },
    RuntimeLength(usize),
    Allocation(SmwUsV1ExpandedSettingsAllocationError),
    Runtime(ExpandedSettingsRuntimeBundleError),
    FixedByteMismatch {
        offset: usize,
        expected: u8,
        actual: Option<u8>,
    },
    EmbeddedRuntimeLength(usize),
    Plan(ExpandedSettingsInstallPlanError),
    SourceRange {
        offset: usize,
        len: usize,
    },
}

impl std::fmt::Display for SmwUsV1ExpandedSettingsGeneration102MigrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cannot migrate SMW-US expanded settings generation 1.02: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1ExpandedSettingsGeneration102MigrationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsInstallPlanError {
    Runtime(ExpandedSettingsRuntimeBundleError),
    MissingRuntimeWrite {
        descriptor_index: usize,
        destination_offset: usize,
    },
    MissingFixupDescriptor {
        descriptor_index: usize,
    },
    UnexpectedRuntimeFixups {
        descriptor_index: usize,
    },
    MissingSa1RuntimeByte {
        offset: usize,
    },
    Sa1RuntimeByteMismatch {
        offset: usize,
        expected: u8,
        actual: u8,
    },
    SpecialRecordIndex(usize),
}

impl std::fmt::Display for ExpandedSettingsInstallPlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded-settings installation plan failed: {self:?}"
        )
    }
}

impl std::error::Error for ExpandedSettingsInstallPlanError {}

impl From<ExpandedSettingsRuntimeBundleError> for ExpandedSettingsInstallPlanError {
    fn from(value: ExpandedSettingsRuntimeBundleError) -> Self {
        Self::Runtime(value)
    }
}

/// Builds the complete failure-atomic installation plan.
///
/// The allocation policy reproduces Lunar Magic's retained placement: the eight-byte RATS header
/// starts at `$087FF8`, and the `$6E00` payload begins at `$088000` (`$11:8000`). Runtime operands
/// remain typed relocations, so the transaction still derives their values from the allocator's
/// result rather than embedding that address.
///
/// # Errors
///
/// Rejects runtime generation failures or disagreement between the recovered descriptor/fixup
/// catalog and the generated fixed-write family.
pub fn smw_us_v1_expanded_settings_installation_plan()
-> Result<RelocatablePatchPlan, ExpandedSettingsInstallPlanError> {
    smw_us_v1_expanded_settings_installation_plan_with_overworld_settings(None)
}

/// Builds the ordinary installation plan against all space already present in a LoROM image.
///
/// Pristine SMW retains the authenticated one-MiB first expansion target. A pre-expanded source
/// instead searches through its complete current extent, matching Lunar Magic's top-level
/// allocator before its expansion retry path is entered.
///
/// # Errors
///
/// Propagates the same generated-runtime validation as the default constructor.
pub fn smw_us_v1_expanded_settings_installation_plan_for_rom(
    rom: &RomImage,
) -> Result<RelocatablePatchPlan, ExpandedSettingsInstallPlanError> {
    smw_us_v1_expanded_settings_installation_plan_for_rom_with_overworld_settings(rom, None)
}

/// Builds the current-ROM-aware installation plan with optional exact overworld records.
///
/// # Errors
///
/// Propagates the same generated-runtime validation as the default constructor.
pub fn smw_us_v1_expanded_settings_installation_plan_for_rom_with_overworld_settings(
    rom: &RomImage,
    overworld: Option<&ExpandedOverworldSettings>,
) -> Result<RelocatablePatchPlan, ExpandedSettingsInstallPlanError> {
    let search_end = rom
        .logical_len()
        .max(SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_END)
        .min(SMW_US_V1_EXPANDED_SETTINGS_MAXIMUM_LOROM_LEN);
    smw_us_v1_expanded_settings_installation_plan_for_range(
        overworld,
        SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START..search_end,
        0xff,
    )
}

/// Builds the current expanded-settings prerequisite against Lunar Magic's authenticated SA-1
/// Pack hook skeleton.
///
/// SA-1 uses the ordinary `$087FF8..$0FFFFF` first-fit range, but its two pre-install hook bodies
/// differ from pristine SMW and allocation-dependent pointers must retain the mapper's canonical
/// bank. All other fixed-write preconditions are shared with the recovered SMW-US runtime.
///
/// # Errors
///
/// Propagates the same generated-runtime and relocation validation as the LoROM constructor.
pub fn smw_us_v1_sa1_expanded_settings_installation_plan()
-> Result<RelocatablePatchPlan, ExpandedSettingsInstallPlanError> {
    smw_us_v1_sa1_expanded_settings_installation_plan_with_ram_remap(true)
}

/// Builds the SA-1 expanded-settings prerequisite while honoring ROM feature bit 17.
///
/// The two mapper adaptations are unconditional. The fifteen IRAM high-byte relocations and
/// `$3B $EB` runtime operand are emitted only when RAM remapping is enabled, matching
/// `InstallExpandedLevelHeaderRuntime` and its relocation helpers.
pub fn smw_us_v1_sa1_expanded_settings_installation_plan_with_ram_remap(
    ram_remap: bool,
) -> Result<RelocatablePatchPlan, ExpandedSettingsInstallPlanError> {
    const SA1_ALWAYS_RUNTIME_BYTES: &[(usize, u8, u8)] =
        &[(0x07f9f7, 0x18, 0x60), (0x07fbd6, 0x18, 0x38)];
    const SA1_RAM_REMAP_RUNTIME_BYTES: &[(usize, u8, u8)] = &[
        (0x07f192, 0x01, 0x61),
        (0x07f7a3, 0x01, 0x61),
        (0x07f82f, 0x19, 0x79),
        (0x07f9c7, 0x13, 0x73),
        (0x07f9e2, 0x01, 0x61),
        (0x07faf2, 0x1f, 0x7f),
        (0x07faf5, 0x1f, 0x7f),
        (0x07fafc, 0x01, 0x61),
        (0x07fb20, 0x80, 0x3b),
        (0x07fb21, 0x21, 0xeb),
        (0x07fb45, 0x07, 0x67),
        (0x07fb48, 0x08, 0x68),
        (0x07fb4b, 0x0d, 0x6d),
        (0x07fb4e, 0x1f, 0x7f),
        (0x07fc9b, 0x07, 0x67),
        (0x07fd90, 0x14, 0x74),
        (0x07fddf, 0x14, 0x74),
    ];
    let mut plan = smw_us_v1_expanded_settings_installation_plan()?;
    plan.description = "install SMW US SA-1 expanded level settings".into();
    plan.mapper = Mapper::Sa1;
    for write in &mut plan.writes {
        match write.offset {
            0x001471 => write.expected = vec![0xae, 0xc6, 0x73, 0xa9, 0x18],
            0x0283b8 => write.expected = vec![0xad, 0x25, 0x79, 0xc9, 0x09],
            _ => {}
        }
        for fixup in &mut write.fixups {
            fixup.encoding = canonical_mapper_fixup(fixup.encoding);
        }
    }
    let runtime_bytes = SA1_ALWAYS_RUNTIME_BYTES.iter().chain(
        ram_remap
            .then_some(SA1_RAM_REMAP_RUNTIME_BYTES)
            .into_iter()
            .flatten(),
    );
    for &(offset, expected, replacement) in runtime_bytes {
        let write = plan
            .writes
            .iter_mut()
            .find(|write| write.offset <= offset && offset < write.offset + write.replacement.len())
            .ok_or(ExpandedSettingsInstallPlanError::MissingSa1RuntimeByte { offset })?;
        let local = offset - write.offset;
        let actual = write.replacement[local];
        if actual != expected {
            return Err(ExpandedSettingsInstallPlanError::Sa1RuntimeByteMismatch {
                offset,
                expected,
                actual,
            });
        }
        write.replacement[local] = replacement;
    }
    for payload in &mut plan.payloads {
        for fixup in &mut payload.fixups {
            fixup.encoding = canonical_mapper_fixup(fixup.encoding);
        }
    }
    Ok(plan)
}

fn canonical_mapper_fixup(encoding: PatchFixupEncoding) -> PatchFixupEncoding {
    match encoding {
        PatchFixupEncoding::Long24LowBank => PatchFixupEncoding::Long24,
        PatchFixupEncoding::Bank8LowBank => PatchFixupEncoding::Bank8,
        other => other,
    }
}

/// Builds the complete installation plan with optional exact records for submaps zero through six.
///
/// # Errors
///
/// Propagates the same recovered runtime/fixup validation as the default installation plan.
pub fn smw_us_v1_expanded_settings_installation_plan_with_overworld_settings(
    overworld: Option<&ExpandedOverworldSettings>,
) -> Result<RelocatablePatchPlan, ExpandedSettingsInstallPlanError> {
    smw_us_v1_expanded_settings_installation_plan_for_range(
        overworld,
        SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START
            ..SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_END,
        0xff,
    )
}

/// Builds the exact prerequisite route used after Lunar Magic has inserted regular 4bpp GFX.
///
/// That workflow leaves a small tagged block at `$080000` and zero-filled expansion space after
/// it. Lunar Magic places the expanded-settings tag at `$080028`, ending before the fixed ExGFX
/// pointer domains. Keeping this route separate prevents the `$088000..$08ACFF` extended pointer
/// table from being mistaken for generic free space.
///
/// # Errors
///
/// Propagates runtime/fixup validation from the ordinary installation-plan constructor.
pub fn smw_us_v1_gfx_expanded_settings_installation_plan()
-> Result<RelocatablePatchPlan, ExpandedSettingsInstallPlanError> {
    smw_us_v1_expanded_settings_installation_plan_for_range(
        None,
        SMW_US_V1_GFX_EXPANDED_SETTINGS_ALLOCATION_START
            ..SMW_US_V1_GFX_EXPANDED_SETTINGS_ALLOCATION_END,
        0x00,
    )
}

fn smw_us_v1_expanded_settings_installation_plan_for_range(
    overworld: Option<&ExpandedOverworldSettings>,
    search: std::ops::Range<usize>,
    expansion_fill: u8,
) -> Result<RelocatablePatchPlan, ExpandedSettingsInstallPlanError> {
    // The builder needs well-formed placeholder addresses; every allocation-dependent byte is
    // replaced by a typed transaction fixup before publication.
    let continuation = if overworld.is_some() {
        // Lunar Magic's overworld-settings/complete-overworld route terminates the two runtime
        // entry blocks directly. Its level/graphics prerequisite routes continue into the caller.
        ExpandedSettingsEntryContinuation::Return
    } else {
        ExpandedSettingsEntryContinuation::Continue
    };
    let layout = ExpandedSettingsRuntimeLayout::smw_us_v1(0x00_8000, continuation);
    let mut writes = smw_us_v1_expanded_settings_fixed_writes(layout)?;
    for (slot, block) in SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
        .iter()
        .copied()
        .enumerate()
    {
        let destination_offset = layout.destination_offsets[slot];
        let write = writes
            .iter()
            .find(|write| write.offset == destination_offset)
            .ok_or(ExpandedSettingsInstallPlanError::MissingRuntimeWrite {
                descriptor_index: block.descriptor_index,
                destination_offset,
            })?;
        if !write.fixups.is_empty() {
            return Err(ExpandedSettingsInstallPlanError::UnexpectedRuntimeFixups {
                descriptor_index: block.descriptor_index,
            });
        }
    }
    for recovered in SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_ALLOCATION_FIXUPS {
        let slot = SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
            .iter()
            .position(|block| block.descriptor_index == recovered.descriptor_index)
            .ok_or(ExpandedSettingsInstallPlanError::MissingFixupDescriptor {
                descriptor_index: recovered.descriptor_index,
            })?;
        let block = SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS[slot];
        let destination_offset = layout.destination_offsets[slot];
        let write = writes
            .iter_mut()
            .find(|write| write.offset == destination_offset)
            .ok_or(ExpandedSettingsInstallPlanError::MissingRuntimeWrite {
                descriptor_index: block.descriptor_index,
                destination_offset,
            })?;
        write.fixups.push(PatchFixup {
            offset: recovered.offset,
            target_payload: 0,
            target_addend: recovered.target_addend,
            encoding: match recovered.encoding {
                ExpandedSettingsAllocationFixupEncoding::Long24 => {
                    PatchFixupEncoding::Long24LowBank
                }
                ExpandedSettingsAllocationFixupEncoding::Low16 => PatchFixupEncoding::Low16,
                ExpandedSettingsAllocationFixupEncoding::Low8 => PatchFixupEncoding::Low8,
                ExpandedSettingsAllocationFixupEncoding::Bank8 => PatchFixupEncoding::Bank8LowBank,
            },
        });
    }

    let mut allocation = SmwUsV1ExpandedSettingsAllocation::new_default();
    if let Some(overworld) = overworld {
        for (index, record) in overworld.records.iter().cloned().enumerate() {
            allocation
                .set_record(0x200 + index, record)
                .map_err(|_| ExpandedSettingsInstallPlanError::SpecialRecordIndex(index))?;
        }
    }
    Ok(RelocatablePatchPlan {
        description: "install SMW US expanded level settings".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy {
            search,
            // Lunar Magic places the tag in the preceding bank's final eight bytes.
            bank_size: None,
            fill_bytes: if expansion_fill == 0xff {
                vec![0xff, 0x00]
            } else {
                vec![expansion_fill]
            },
            protected: Vec::new(),
        },
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill,
        payloads: vec![PatchPayload {
            bytes: allocation.encode(),
            fixups: Vec::new(),
        }],
        writes,
    })
}

const GENERATION_101_RUNTIME_DIGESTS: &[(usize, usize, &str)] = &[
    (
        0x07_f160,
        0x90,
        "fee419db30673bec07babf53f20dffd1f33228fa48db5af911f2fe6a5f938179",
    ),
    (
        0x07_f780,
        0x60,
        "287c3f8c3514134c1255c69f35ac9b4ef0ead40a362ab475861ade624200a86c",
    ),
    (
        0x07_f7f0,
        0x50,
        "e3eacb98542238a94e7198eba122a4ebc18821baa751d20f3cdad52af5c22ce0",
    ),
    (
        0x07_f840,
        0x50,
        "d1f7a66f04e461b49a78ad037c148270c8d20b6f09fd7e25aec9ba007a4612ec",
    ),
    (
        0x07_f8a0,
        0x50,
        "ed3070cd141a979c956d8e09f2f129f79e0fc510a8f32dc264103458908aa6ef",
    ),
    (
        0x07_f900,
        0x70,
        "ca9913250b771ad0ee144afe03ec0618604103ba9335aaedfee45f83dd30d95c",
    ),
    (
        0x07_f9c0,
        0x20,
        "af9613760f72635fbdb44a5a0a63c39f12af30f950a6ee5c971be188e89c4051",
    ),
    (
        0x07_f9e0,
        0xd0,
        "86ff85227f7cc42cc3293ebdeeb10d8660b51eb429dd1d6c999ddf14cb59db73",
    ),
    (
        0x07_fab0,
        0x40,
        "8667e718294e9e0df1d30600ba3eeb201f764aad2dad72748643e4a285e1d1f7",
    ),
    (
        0x07_faf0,
        0x30,
        "80a76a18acf8cb64fec3a659ffc4bab4a87cd9a6fde4dab2161a8751d136c9d2",
    ),
    (
        0x07_fb20,
        0x220,
        "af71f904a725ecb2d9bae2414f79a793539f38f39a6bbf14da7be38a8b49f683",
    ),
    (
        0x07_fd80,
        0x150,
        "77071fc978963dce5058c795643c95c195d0d2506c5f02ff38c09e4c8d6ec517",
    ),
    (
        0x06_f0f0,
        0x10,
        "6e14aebb5d4519a9b463173ad662ad446094195b5d17e64940072208597c6704",
    ),
    (
        0x06_a4c1,
        3,
        "282dac78870e292d4a8db35f86cba1d2468179e95ef4045002e7eb565962b284",
    ),
    (
        0x06_c206,
        3,
        "282dac78870e292d4a8db35f86cba1d2468179e95ef4045002e7eb565962b284",
    ),
    (
        0x06_ce06,
        3,
        "282dac78870e292d4a8db35f86cba1d2468179e95ef4045002e7eb565962b284",
    ),
    (
        0x06_da06,
        3,
        "282dac78870e292d4a8db35f86cba1d2468179e95ef4045002e7eb565962b284",
    ),
    (
        0x06_e906,
        3,
        "282dac78870e292d4a8db35f86cba1d2468179e95ef4045002e7eb565962b284",
    ),
    (
        0x00_2a50,
        4,
        "77e65f2e015f4e3d512204af098e8ce54392173f61fa1036e372e8a68e2f31f1",
    ),
    (
        0x02_83b8,
        5,
        "04b1a6c4a6e75c3e9128c872d5da0f64eb4c6facdde2e72e31cb2cd6d212f468",
    ),
    (
        0x00_1471,
        5,
        "35c62b46696bd0dd15b898a71cfa81d8133055f7e79a79e77e2ff963aab04441",
    ),
    (
        0x00_2140,
        4,
        "3673d6ba717acb4a14679e71ffe5e05a858a343a544d933997dd7f02537514bf",
    ),
    (
        0x02_1dfe,
        3,
        "927c8242d29ff105e9ae693e95258ddb23d81d63165b25b5a5439487c55dabd8",
    ),
];

/// Authenticates and builds the pre-Layer-3 generation-1.01 expanded-header migration.
///
/// Lunar Magic 2.22 owns only `$6D00`: a `$2D00` fill prefix followed by 512 records. The current
/// installer grows that model to `$6E00`, preserving and normalizing the ordinary records while
/// initializing the eight later special slots from their recovered defaults.
pub fn smw_us_v1_expanded_settings_generation_101_migration(
    bytes: &[u8],
) -> Result<
    SmwUsV1ExpandedSettingsGeneration101Migration,
    SmwUsV1ExpandedSettingsGeneration101MigrationError,
> {
    legacy_expanded_settings_migration(
        bytes,
        SMW_US_V1_LEGACY_GRAPHICS_GENERATION_101_MARKER_OFFSET,
        SMW_US_V1_LEGACY_GRAPHICS_GENERATION_101_MARKER,
        "migrate SMW US expanded settings generation 1.01",
    )
}

/// Authenticates and builds the generation-1.00 expanded-header migration.
///
/// Lunar Magic 1.71 uses the same `$6D00` owner, relocation operands, and immutable runtime family
/// as generation 1.01. Its active legacy-graphics marker is instead `LM 00 01` at `$0FB604`.
/// The original editor classifies both legacy generations through the same reference-normalization
/// path, so all 512 ordinary records are upgraded identically and the eight later special slots
/// are initialized from their recovered defaults.
pub fn smw_us_v1_expanded_settings_generation_100_migration(
    bytes: &[u8],
) -> Result<
    SmwUsV1ExpandedSettingsGeneration100Migration,
    SmwUsV1ExpandedSettingsGeneration100MigrationError,
> {
    let migration = legacy_expanded_settings_migration(
        bytes,
        SMW_US_V1_LEGACY_GRAPHICS_GENERATION_100_MARKER_OFFSET,
        SMW_US_V1_LEGACY_GRAPHICS_GENERATION_100_MARKER,
        "migrate SMW US expanded settings generation 1.00",
    )
    .map_err(SmwUsV1ExpandedSettingsGeneration100MigrationError::Legacy)?;
    Ok(SmwUsV1ExpandedSettingsGeneration100Migration {
        plan: migration.plan,
        previous_allocation: migration.previous_allocation,
    })
}

fn legacy_expanded_settings_migration(
    bytes: &[u8],
    legacy_marker_offset: usize,
    legacy_marker: [u8; 4],
    description: &str,
) -> Result<
    SmwUsV1ExpandedSettingsGeneration101Migration,
    SmwUsV1ExpandedSettingsGeneration101MigrationError,
> {
    let current_marker = bytes.get(
        SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER_OFFSET
            ..SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER_OFFSET + 4,
    );
    if current_marker != Some(&[0xff; 4]) {
        return Err(SmwUsV1ExpandedSettingsGeneration101MigrationError::CurrentMarkerPresent);
    }
    if bytes.get(legacy_marker_offset..legacy_marker_offset + legacy_marker.len())
        != Some(&legacy_marker)
    {
        return Err(SmwUsV1ExpandedSettingsGeneration101MigrationError::LegacyMarkerMismatch);
    }

    let operand_offset = 0x07_f840 + 0x33;
    let operand = bytes
        .get(operand_offset..operand_offset + 3)
        .ok_or(SmwUsV1ExpandedSettingsGeneration101MigrationError::InvalidRuntimePointer)?;
    let allocation_base_snes = u32::from_le_bytes([operand[0], operand[1], operand[2], 0]);
    let payload_offset = snes_to_pc(Mapper::LoRom, allocation_base_snes)
        .map_err(SmwUsV1ExpandedSettingsGeneration101MigrationError::RuntimeAddress)?;
    let header_offset = payload_offset.checked_sub(8).ok_or(
        SmwUsV1ExpandedSettingsGeneration101MigrationError::RuntimeBeforeHeader(payload_offset),
    )?;
    let previous_allocation = parse_at(bytes, header_offset)
        .map_err(SmwUsV1ExpandedSettingsGeneration101MigrationError::RuntimeHeader)?;
    if previous_allocation.payload.start != payload_offset {
        return Err(
            SmwUsV1ExpandedSettingsGeneration101MigrationError::RuntimeOwnership {
                expected: payload_offset,
                actual: previous_allocation.payload.start,
            },
        );
    }
    if previous_allocation.payload.len()
        != SMW_US_V1_EXPANDED_SETTINGS_GENERATION_101_ALLOCATION_LEN
    {
        return Err(
            SmwUsV1ExpandedSettingsGeneration101MigrationError::RuntimeLength(
                previous_allocation.payload.len(),
            ),
        );
    }
    let legacy = bytes.get(previous_allocation.payload.clone()).ok_or(
        SmwUsV1ExpandedSettingsGeneration101MigrationError::SourceRange {
            offset: previous_allocation.payload.start,
            len: previous_allocation.payload.len(),
        },
    )?;
    if let Some((offset, value)) = legacy[..crate::SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN]
        .iter()
        .copied()
        .enumerate()
        .find(|(_, value)| *value != 0xff)
    {
        return Err(
            SmwUsV1ExpandedSettingsGeneration101MigrationError::NonFillPrefix { offset, value },
        );
    }

    authenticate_generation_101_runtime(bytes, payload_offset)?;

    let mut allocation = SmwUsV1ExpandedSettingsAllocation::new_default();
    for (index, encoded) in legacy[crate::SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN..]
        .chunks_exact(lm_level::ExpandedLevelSettingsRecord::ENCODED_LEN)
        .enumerate()
    {
        let mut bytes = [0; lm_level::ExpandedLevelSettingsRecord::ENCODED_LEN];
        bytes.copy_from_slice(encoded);
        let mut record = lm_level::ExpandedLevelSettingsRecord::from_encoded(bytes);
        smw_us_v1_upgrade_expanded_settings_record(
            &mut record,
            SmwUsV1ExpandedSettingsRecordGeneration::LegacyReferenceLayout,
        );
        allocation
            .set_record(index, record)
            .map_err(|_| SmwUsV1ExpandedSettingsGeneration101MigrationError::RecordIndex(index))?;
    }

    let mut plan = smw_us_v1_expanded_settings_installation_plan()
        .map_err(SmwUsV1ExpandedSettingsGeneration101MigrationError::Plan)?;
    plan.description = description.into();
    plan.payloads[0].bytes = allocation.encode();
    for write in &mut plan.writes {
        let source = bytes
            .get(write.offset..write.offset + write.expected.len())
            .ok_or(
                SmwUsV1ExpandedSettingsGeneration101MigrationError::SourceRange {
                    offset: write.offset,
                    len: write.expected.len(),
                },
            )?;
        write.expected.copy_from_slice(source);
    }
    Ok(SmwUsV1ExpandedSettingsGeneration101Migration {
        plan,
        previous_allocation,
    })
}

fn authenticate_generation_101_runtime(
    bytes: &[u8],
    payload_offset: usize,
) -> Result<(), SmwUsV1ExpandedSettingsGeneration101MigrationError> {
    let base = pc_to_snes(Mapper::LoRom, payload_offset)
        .map_err(SmwUsV1ExpandedSettingsGeneration101MigrationError::RuntimeAddress)?
        & 0x7f_ffff;
    let table = pc_to_snes(
        Mapper::LoRom,
        payload_offset + crate::SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN,
    )
    .map_err(SmwUsV1ExpandedSettingsGeneration101MigrationError::RuntimeAddress)?
        & 0x7f_ffff;
    for (offset, expected) in [
        (0x07_f7f0 + 0x0f, &table.to_le_bytes()[..3]),
        (0x07_f7f0 + 0x1a, &(table + 2).to_le_bytes()[..2]),
        (0x07_f7f0 + 0x23, &table.to_le_bytes()[2..3]),
        (0x07_f840 + 0x33, &base.to_le_bytes()[..3]),
        (0x07_f900 + 0x37, &base.to_le_bytes()[..3]),
        (0x07_f900 + 0x3d, &(base + 1).to_le_bytes()[..3]),
    ] {
        if bytes.get(offset..offset + expected.len()) != Some(expected) {
            return Err(
                SmwUsV1ExpandedSettingsGeneration101MigrationError::RuntimeOperandMismatch {
                    offset,
                },
            );
        }
    }
    for &(offset, len, expected) in GENERATION_101_RUNTIME_DIGESTS {
        let mut range = bytes
            .get(offset..offset + len)
            .ok_or(SmwUsV1ExpandedSettingsGeneration101MigrationError::SourceRange { offset, len })?
            .to_vec();
        match offset {
            0x07_f7f0 => {
                range[0x0f..0x12].fill(0);
                range[0x1a..0x1c].fill(0);
                range[0x23] = 0;
            }
            0x07_f840 => range[0x33..0x36].fill(0),
            0x07_f900 => {
                range[0x37..0x3a].fill(0);
                range[0x3d..0x40].fill(0);
            }
            _ => {}
        }
        if format!("{:x}", Sha256::digest(&range)) != expected {
            return Err(
                SmwUsV1ExpandedSettingsGeneration101MigrationError::RuntimeDigestMismatch {
                    offset,
                },
            );
        }
    }
    Ok(())
}

/// Authenticates and builds Lunar Magic's generation-1.02 whole-table migration.
///
/// The generation marker, exact RATS owner, complete fixed runtime family, and the historical
/// descriptor `$220` body are all immutable identity evidence. Only after those checks pass are
/// reference words in the 512 standard-level records normalized. The eight special records are
/// retained byte-for-byte, matching `UpgradeLegacyExpandedLevelHeaderTable`.
///
/// # Errors
///
/// Rejects absent or modified generation-1.02 runtime evidence, malformed ownership/allocation,
/// truncated source ranges, or an inconsistent current installation plan.
pub fn smw_us_v1_expanded_settings_generation_102_migration(
    bytes: &[u8],
) -> Result<
    SmwUsV1ExpandedSettingsGeneration102Migration,
    SmwUsV1ExpandedSettingsGeneration102MigrationError,
> {
    let marker = bytes.get(
        SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER_OFFSET
            ..SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER_OFFSET + 4,
    );
    if marker.is_none_or(|marker| marker.iter().all(|byte| *byte == 0xff)) {
        return Err(SmwUsV1ExpandedSettingsGeneration102MigrationError::MissingGeneration);
    }
    if marker != Some(&SMW_US_V1_EXPANDED_SETTINGS_GENERATION_102_MARKER) {
        return Err(SmwUsV1ExpandedSettingsGeneration102MigrationError::MarkerMismatch);
    }

    let operand_offset = SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
        .iter()
        .position(|block| block.descriptor_index == 0x173)
        .map(|slot| {
            ExpandedSettingsRuntimeLayout::smw_us_v1(0, ExpandedSettingsEntryContinuation::Continue)
                .destination_offsets[slot]
                + 0x33
        })
        .ok_or(SmwUsV1ExpandedSettingsGeneration102MigrationError::InvalidRuntimePointer)?;
    let operand = bytes
        .get(operand_offset..operand_offset + 3)
        .ok_or(SmwUsV1ExpandedSettingsGeneration102MigrationError::InvalidRuntimePointer)?;
    let allocation_base_snes = u32::from_le_bytes([operand[0], operand[1], operand[2], 0]);
    let payload_offset = snes_to_pc(Mapper::LoRom, allocation_base_snes)
        .map_err(SmwUsV1ExpandedSettingsGeneration102MigrationError::RuntimeAddress)?;
    let header_offset = payload_offset.checked_sub(8).ok_or(
        SmwUsV1ExpandedSettingsGeneration102MigrationError::RuntimeBeforeHeader(payload_offset),
    )?;
    let previous_allocation = parse_at(bytes, header_offset)
        .map_err(SmwUsV1ExpandedSettingsGeneration102MigrationError::RuntimeHeader)?;
    if previous_allocation.payload.start != payload_offset {
        return Err(
            SmwUsV1ExpandedSettingsGeneration102MigrationError::RuntimeOwnership {
                expected: payload_offset,
                actual: previous_allocation.payload.start,
            },
        );
    }
    if previous_allocation.payload.len() != crate::SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_LEN {
        return Err(
            SmwUsV1ExpandedSettingsGeneration102MigrationError::RuntimeLength(
                previous_allocation.payload.len(),
            ),
        );
    }
    let mut allocation = SmwUsV1ExpandedSettingsAllocation::decode(
        bytes.get(previous_allocation.payload.clone()).ok_or(
            SmwUsV1ExpandedSettingsGeneration102MigrationError::SourceRange {
                offset: previous_allocation.payload.start,
                len: previous_allocation.payload.len(),
            },
        )?,
    )
    .map_err(SmwUsV1ExpandedSettingsGeneration102MigrationError::Allocation)?;

    let layout = ExpandedSettingsRuntimeLayout::smw_us_v1(
        allocation_base_snes,
        ExpandedSettingsEntryContinuation::Continue,
    );
    let mut authenticated = smw_us_v1_expanded_settings_fixed_writes(layout)
        .map_err(SmwUsV1ExpandedSettingsGeneration102MigrationError::Runtime)?;
    let legacy_220 = decode_embedded_hex(GENERATION_102_BLOCK_220_HEX);
    let block_220 = SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
        .iter()
        .position(|block| block.descriptor_index == 0x220)
        .ok_or(SmwUsV1ExpandedSettingsGeneration102MigrationError::EmbeddedRuntimeLength(0))?;
    if legacy_220.len() != SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS[block_220].len {
        return Err(
            SmwUsV1ExpandedSettingsGeneration102MigrationError::EmbeddedRuntimeLength(
                legacy_220.len(),
            ),
        );
    }
    for write in &mut authenticated {
        if write.offset == SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER_OFFSET {
            write.replacement = SMW_US_V1_EXPANDED_SETTINGS_GENERATION_102_MARKER.to_vec();
        } else if write.offset == layout.destination_offsets[block_220] {
            write.replacement.clone_from(&legacy_220);
        }
        require_exact_generation_102(bytes, write.offset, &write.replacement)?;
    }

    for index in 0..crate::SMW_US_V1_EXPANDED_SETTINGS_STANDARD_LEVEL_COUNT {
        let mut record = allocation
            .record(index)
            .map_err(SmwUsV1ExpandedSettingsGeneration102MigrationError::Allocation)?
            .clone();
        smw_us_v1_upgrade_expanded_settings_record(
            &mut record,
            SmwUsV1ExpandedSettingsRecordGeneration::LegacyReferenceLayout,
        );
        allocation
            .set_record(index, record)
            .map_err(SmwUsV1ExpandedSettingsGeneration102MigrationError::Allocation)?;
    }

    let mut plan = smw_us_v1_expanded_settings_installation_plan()
        .map_err(SmwUsV1ExpandedSettingsGeneration102MigrationError::Plan)?;
    plan.description = "migrate SMW US expanded settings generation 1.02".into();
    plan.payloads[0].bytes = allocation.encode();
    for write in &mut plan.writes {
        let source = bytes
            .get(write.offset..write.offset + write.expected.len())
            .ok_or(
                SmwUsV1ExpandedSettingsGeneration102MigrationError::SourceRange {
                    offset: write.offset,
                    len: write.expected.len(),
                },
            )?;
        write.expected.copy_from_slice(source);
    }
    Ok(SmwUsV1ExpandedSettingsGeneration102Migration {
        plan,
        previous_allocation,
    })
}

fn require_exact_generation_102(
    bytes: &[u8],
    offset: usize,
    expected: &[u8],
) -> Result<(), SmwUsV1ExpandedSettingsGeneration102MigrationError> {
    for (index, expected) in expected.iter().copied().enumerate() {
        let actual = bytes.get(offset + index).copied();
        if actual != Some(expected) {
            return Err(
                SmwUsV1ExpandedSettingsGeneration102MigrationError::FixedByteMismatch {
                    offset: offset + index,
                    expected,
                    actual,
                },
            );
        }
    }
    Ok(())
}

fn decode_embedded_hex(source: &str) -> Vec<u8> {
    let digits = source
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    digits
        .chunks_exact(2)
        .map(|pair| {
            let nibble = |byte| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => unreachable!("embedded runtime is lowercase hexadecimal"),
            };
            (nibble(pair[0]) << 4) | nibble(pair[1])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN, SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT};
    use lm_level::ExpandedLevelSettingsRecord;
    use lm_project::{
        ExpandedLevelSettingsLayout, Project, RatsOwnershipManifest, RelocatablePatchError,
    };
    use lm_rom::{RomImage, SnesChecksum};
    use std::{fs, path::PathBuf};

    fn fixtures() -> (Vec<u8>, RomImage) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let after = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let before = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
        )
        .unwrap();
        (after, RomImage::from_bytes(before).unwrap())
    }

    fn fixed_replacement_byte(plan: &RelocatablePatchPlan, offset: usize) -> u8 {
        let write = plan
            .writes
            .iter()
            .find(|write| write.offset <= offset && offset < write.offset + write.replacement.len())
            .unwrap();
        write.replacement[offset - write.offset]
    }

    #[test]
    fn sa1_ram_remap_controls_exactly_fifteen_iram_operands_and_one_word() {
        const CONDITIONAL: &[(usize, u8, u8)] = &[
            (0x07f192, 0x01, 0x61),
            (0x07f7a3, 0x01, 0x61),
            (0x07f82f, 0x19, 0x79),
            (0x07f9c7, 0x13, 0x73),
            (0x07f9e2, 0x01, 0x61),
            (0x07faf2, 0x1f, 0x7f),
            (0x07faf5, 0x1f, 0x7f),
            (0x07fafc, 0x01, 0x61),
            (0x07fb20, 0x80, 0x3b),
            (0x07fb21, 0x21, 0xeb),
            (0x07fb45, 0x07, 0x67),
            (0x07fb48, 0x08, 0x68),
            (0x07fb4b, 0x0d, 0x6d),
            (0x07fb4e, 0x1f, 0x7f),
            (0x07fc9b, 0x07, 0x67),
            (0x07fd90, 0x14, 0x74),
            (0x07fddf, 0x14, 0x74),
        ];
        let disabled =
            smw_us_v1_sa1_expanded_settings_installation_plan_with_ram_remap(false).unwrap();
        let enabled =
            smw_us_v1_sa1_expanded_settings_installation_plan_with_ram_remap(true).unwrap();
        for &(offset, original, remapped) in CONDITIONAL {
            assert_eq!(fixed_replacement_byte(&disabled, offset), original);
            assert_eq!(fixed_replacement_byte(&enabled, offset), remapped);
        }
        for (offset, replacement) in [(0x07f9f7, 0x60), (0x07fbd6, 0x38)] {
            assert_eq!(fixed_replacement_byte(&disabled, offset), replacement);
            assert_eq!(fixed_replacement_byte(&enabled, offset), replacement);
        }
    }

    #[test]
    #[ignore = "requires retained authentic SA-1 Pack before/after first-ExGFX oracle images"]
    fn authentic_sa1_expanded_settings_owned_family_is_reproduced_exactly() {
        let before = RomImage::from_bytes(
            fs::read(std::env::var_os("LM_SA1_EXGFX_BEFORE").expect("LM_SA1_EXGFX_BEFORE"))
                .unwrap(),
        )
        .unwrap();
        let oracle = RomImage::from_bytes(
            fs::read(std::env::var_os("LM_SA1_EXGFX_AFTER").expect("LM_SA1_EXGFX_AFTER")).unwrap(),
        )
        .unwrap();
        let plan = smw_us_v1_sa1_expanded_settings_installation_plan().unwrap();
        let write_ranges = plan
            .writes
            .iter()
            .map(|write| write.offset..write.offset + write.replacement.len())
            .collect::<Vec<_>>();
        let mut project = Project::new(before);
        project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(
            project.rom.read(0x087ff8, 0x6e08).unwrap(),
            oracle.read(0x087ff8, 0x6e08).unwrap()
        );
        let mut mismatches = Vec::new();
        for range in write_ranges {
            let actual = project
                .rom
                .read(range.start, range.end - range.start)
                .unwrap();
            let expected = oracle.read(range.start, range.end - range.start).unwrap();
            mismatches.extend(actual.iter().zip(expected).enumerate().filter_map(
                |(index, (actual, expected))| {
                    (actual != expected).then_some((range.start + index, *actual, *expected))
                },
            ));
        }
        assert!(mismatches.is_empty(), "mismatches: {mismatches:02X?}");
    }

    #[test]
    fn plan_installs_all_owned_bytes_reopens_semantically_and_undoes_exactly() {
        let (after_file, before_image) = fixtures();
        let after = RomImage::from_bytes(after_file).unwrap();
        let original = before_image.logical_bytes().to_vec();
        let mut project = Project::new(before_image);
        let plan = smw_us_v1_expanded_settings_installation_plan().unwrap();
        let result = project.install_relocatable_patch(&plan).unwrap();

        assert_eq!(result.blocks.len(), 1);
        assert_eq!(
            result.blocks[0].header_offset,
            SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START
        );
        assert_eq!(result.blocks[0].payload.start, 0x08_8000);
        // The retained oracle imported level 000 after installing the table. Its tag, fill prefix,
        // and every other record are nevertheless exact installation evidence.
        let installed = project
            .rom
            .read(result.blocks[0].header_offset, 0x6e08)
            .unwrap();
        let oracle = after.read(result.blocks[0].header_offset, 0x6e08).unwrap();
        let record_zero = 8 + SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN;
        assert_eq!(&installed[..record_zero], &oracle[..record_zero]);
        assert_eq!(
            &installed[record_zero + 0x20..],
            &oracle[record_zero + 0x20..]
        );
        for write in &plan.writes {
            assert_eq!(
                project
                    .rom
                    .read(write.offset, write.replacement.len())
                    .unwrap(),
                after.read(write.offset, write.replacement.len()).unwrap()
            );
        }
        assert!(
            SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                .unwrap()
                .is_complementary()
        );
        let settings_layout = ExpandedLevelSettingsLayout {
            mapper: Mapper::LoRom,
            table_offset: result.blocks[0].payload.start + SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN,
            entries: SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT,
            stride: 0x20,
        };
        assert_eq!(
            project
                .load_expanded_level_settings(0x207, settings_layout)
                .unwrap(),
            crate::smw_us_v1_default_expanded_settings_record()
        );
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.rom.logical_bytes(), original);
    }

    #[test]
    fn late_hook_precondition_failure_preserves_rom_and_history() {
        let (_, mut before) = fixtures();
        before.write(0x1471, &[0]).unwrap();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        let plan = smw_us_v1_expanded_settings_installation_plan().unwrap();
        assert!(matches!(
            project.install_relocatable_patch(&plan),
            Err(RelocatablePatchError::HookPreconditionMismatch { .. })
        ));
        assert_eq!(project.rom.logical_bytes(), original);
        assert_eq!(project.history.undo_len(), 0);
    }

    #[test]
    fn authentic_first_fit_collision_relocates_settings_to_the_next_lunar_magic_bank() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
        let original_header = image.copier_header_bytes().unwrap().to_vec();
        let mut project = Project::new(image);
        let mut settings = ExpandedOverworldSettings {
            records: std::array::from_fn(|_| {
                crate::smw_us_v1_default_special_expanded_settings_record()
            }),
        };
        settings.records[4].set_word(7, 0x4567).unwrap();

        let result = project
            .install_relocatable_patch(
                &smw_us_v1_expanded_settings_installation_plan_with_overworld_settings(Some(
                    &settings,
                ))
                .unwrap(),
            )
            .unwrap();
        assert_eq!(result.blocks[0].header_offset, 0x09_0000);
        assert_eq!(result.blocks[0].payload.start, 0x09_0008);
        let allocation_base_snes =
            lm_rom::pc_to_snes(Mapper::LoRom, result.blocks[0].payload.start).unwrap() & 0x7f_ffff;
        for write in
            smw_us_v1_expanded_settings_fixed_writes(ExpandedSettingsRuntimeLayout::smw_us_v1(
                allocation_base_snes,
                ExpandedSettingsEntryContinuation::Return,
            ))
            .unwrap()
        {
            assert_eq!(
                project
                    .rom
                    .read(write.offset, write.replacement.len())
                    .unwrap(),
                write.replacement,
                "runtime write at {:x}",
                write.offset
            );
        }
        let layout = crate::smw_us_v1_installed_expanded_settings_layout(&project)
            .unwrap()
            .unwrap();
        assert_eq!(layout.table_offset, 0x09_2d08);
        assert_eq!(
            project
                .load_expanded_overworld_settings(
                    crate::SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
                    layout,
                )
                .unwrap(),
            settings
        );
        assert_eq!(
            project.rom.copier_header_bytes(),
            Some(original_header.as_slice())
        );
        assert!(
            lm_rom::detect_identity(&project.rom)
                .unwrap()
                .checksum_matches()
        );
        let installed = project.save_snapshot();
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.as_file_bytes(), bytes);
        assert!(project.redo().unwrap());
        assert_eq!(project.rom.as_file_bytes(), installed);
    }

    #[test]
    fn post_gfx_plan_uses_the_exact_zero_filled_gap_before_exgfx_tables() {
        let (_, mut before) = fixtures();
        before.expand(Mapper::LoRom, 0x09_0000, 0x00).unwrap();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        let result = project
            .install_relocatable_patch(
                &smw_us_v1_gfx_expanded_settings_installation_plan().unwrap(),
            )
            .unwrap();

        assert_eq!(
            result.blocks[0].header_offset,
            SMW_US_V1_GFX_EXPANDED_SETTINGS_ALLOCATION_START
        );
        assert_eq!(result.blocks[0].payload.start, 0x08_0030);
        assert_eq!(result.blocks[0].payload.end, 0x08_6e30);
        project.undo().unwrap();
        assert_eq!(project.rom.logical_bytes(), original);
    }

    #[test]
    fn irregular_relocation_updates_the_complete_table_low_word() {
        let (_, mut before) = fixtures();
        before.expand(Mapper::LoRom, 0x10_0000, 0xff).unwrap();
        let mut plan = smw_us_v1_expanded_settings_installation_plan().unwrap();
        plan.allocation.search = 0x09_1234..0x09_803c;
        let mut project = Project::new(before);
        let result = project.install_relocatable_patch(&plan).unwrap();

        assert_eq!(result.blocks[0].header_offset, 0x09_1234);
        let base = pc_to_snes(Mapper::LoRom, result.blocks[0].payload.start).unwrap() & 0x7f_ffff;
        let table = base + crate::SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN as u32;
        assert_eq!(
            project.rom.read(0x07_f7f0 + 0x16, 2).unwrap(),
            &table.to_le_bytes()[..2]
        );
        assert!(
            crate::smw_us_v1_installed_expanded_settings_layout(&project)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn preexpanded_headered_and_headerless_roms_use_existing_late_space_and_undo_exactly() {
        let (_, pristine) = fixtures();
        for headered in [false, true] {
            let mut file = if headered {
                let mut bytes = (0..lm_rom::COPIER_HEADER_LEN)
                    .map(|index| (index as u8).wrapping_mul(37))
                    .collect::<Vec<_>>();
                bytes.extend_from_slice(pristine.logical_bytes());
                bytes
            } else {
                pristine.logical_bytes().to_vec()
            };
            let mut image = RomImage::from_bytes(std::mem::take(&mut file)).unwrap();
            image.expand(Mapper::LoRom, 0x20_0000, 0x11).unwrap();
            image.write(0x18_0000, &vec![0xff; 0x8000]).unwrap();
            let original_file = image.as_file_bytes().to_vec();
            let original_header = image.copier_header_bytes().map(<[u8]>::to_vec);
            let mut project = Project::new(image);

            let plan = smw_us_v1_expanded_settings_installation_plan_for_rom(&project.rom).unwrap();
            assert_eq!(plan.allocation.search.end, 0x20_0000);
            let result = project
                .install_relocatable_patch_with_expansion_retry(
                    &plan,
                    SMW_US_V1_EXPANDED_SETTINGS_MAXIMUM_LOROM_LEN,
                )
                .unwrap();

            assert_eq!(result.blocks[0].header_offset, 0x18_0000);
            assert_eq!(project.rom.logical_len(), 0x20_0000);
            assert_eq!(
                project.rom.copier_header_bytes().map(<[u8]>::to_vec),
                original_header
            );
            assert!(
                crate::smw_us_v1_installed_expanded_settings_layout(&project)
                    .unwrap()
                    .is_some()
            );
            assert!(
                SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                    .unwrap()
                    .is_complementary()
            );
            assert_eq!(project.history.undo_len(), 1);
            assert!(project.undo().unwrap());
            assert_eq!(project.rom.as_file_bytes(), original_file);
        }
    }

    #[test]
    fn exhausted_preexpanded_rom_grows_one_bank_reopens_and_undoes_atomically() {
        let (_, mut image) = fixtures();
        image.expand(Mapper::LoRom, 0x10_0000, 0x11).unwrap();
        image
            .write(
                SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START,
                &vec![0x11; 0x10_0000 - SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START],
            )
            .unwrap();
        let original = image.as_file_bytes().to_vec();
        let mut project = Project::new(image);
        let plan = smw_us_v1_expanded_settings_installation_plan_for_rom(&project.rom).unwrap();

        let result = project
            .install_relocatable_patch_with_expansion_retry(&plan, 0x10_8000)
            .unwrap();

        assert_eq!(result.blocks[0].header_offset, 0x10_0000);
        assert_eq!(project.rom.logical_len(), 0x10_8000);
        assert!(
            crate::smw_us_v1_installed_expanded_settings_layout(&project)
                .unwrap()
                .is_some()
        );
        assert!(
            SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                .unwrap()
                .is_complementary()
        );
        assert_eq!(project.history.undo_len(), 1);
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.as_file_bytes(), original);
    }

    fn generation_102_fixture() -> Vec<u8> {
        let mut bytes = crate::test_support::pristine_smw_us_rom_bytes();
        bytes.resize(0x10_0000, 0xff);
        let payload_offset = 0x08_01e0;
        let header_offset = payload_offset - 8;
        bytes[header_offset..payload_offset]
            .copy_from_slice(&[b'S', b'T', b'A', b'R', 0xff, 0x6d, 0x00, 0x92]);

        let mut allocation = SmwUsV1ExpandedSettingsAllocation::new_default();
        let mut standard = [0; ExpandedLevelSettingsRecord::ENCODED_LEN];
        for (index, value) in [(2, 0xf123_u16), (4, 0xffff), (8, 0xffff), (10, 0xabcd)] {
            standard[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        allocation
            .set_record(0, ExpandedLevelSettingsRecord::from_encoded(standard))
            .unwrap();
        let special = ExpandedLevelSettingsRecord::from_encoded([0xa5; 0x20]);
        allocation.set_record(0x200, special).unwrap();
        bytes[payload_offset..payload_offset + 0x6e00].copy_from_slice(&allocation.encode());

        let layout = ExpandedSettingsRuntimeLayout::smw_us_v1(
            0x10_81e0,
            ExpandedSettingsEntryContinuation::Continue,
        );
        let block_220 = SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
            .iter()
            .position(|block| block.descriptor_index == 0x220)
            .unwrap();
        for mut write in smw_us_v1_expanded_settings_fixed_writes(layout).unwrap() {
            if write.offset == SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER_OFFSET {
                write.replacement = SMW_US_V1_EXPANDED_SETTINGS_GENERATION_102_MARKER.to_vec();
            } else if write.offset == layout.destination_offsets[block_220] {
                write.replacement = decode_embedded_hex(GENERATION_102_BLOCK_220_HEX);
            }
            bytes[write.offset..write.offset + write.replacement.len()]
                .copy_from_slice(&write.replacement);
        }
        bytes
    }

    #[test]
    fn generation_102_migration_authenticates_normalizes_reclaims_and_undoes() {
        let bytes = generation_102_fixture();
        let before = bytes.clone();
        let migration = smw_us_v1_expanded_settings_generation_102_migration(&bytes).unwrap();
        assert_eq!(migration.previous_allocation.header_offset, 0x08_01d8);

        let migrated =
            SmwUsV1ExpandedSettingsAllocation::decode(&migration.plan.payloads[0].bytes).unwrap();
        assert_eq!(migrated.record(0).unwrap().word(2).unwrap(), 0x0123);
        assert_eq!(migrated.record(0).unwrap().word(4).unwrap(), 0x007f);
        assert_eq!(migrated.record(0).unwrap().word(8).unwrap(), 0xffff);
        assert_eq!(migrated.record(0).unwrap().word(10).unwrap(), 0x0bcd);
        assert_eq!(migrated.record(0x200).unwrap().encoded(), &[0xa5; 0x20]);

        let previous = migration.previous_allocation.clone();
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let result = project
            .replace_relocatable_patch(
                &migration.plan,
                &RatsOwnershipManifest {
                    owned: vec![previous],
                    retained: Vec::new(),
                },
                0xff,
            )
            .unwrap();
        assert_eq!(result.blocks[0].header_offset, 0x08_7ff8);
        assert!(
            crate::smw_us_v1_installed_expanded_settings_layout(&project)
                .unwrap()
                .is_some()
        );
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.rom.logical_bytes(), before);

        let mut corrupt = before;
        corrupt[0x07_fd80 + 7] ^= 1;
        assert!(matches!(
            smw_us_v1_expanded_settings_generation_102_migration(&corrupt),
            Err(
                SmwUsV1ExpandedSettingsGeneration102MigrationError::FixedByteMismatch {
                    offset: 0x07_fd87,
                    ..
                }
            )
        ));
    }

    #[test]
    fn external_generation_102_oracle_migrates_when_supplied() {
        let Ok(path) = std::env::var("LM_EXPANDED_SETTINGS_102_ROM") else {
            return;
        };
        let image = RomImage::from_bytes(fs::read(path).unwrap()).unwrap();
        let before = image.as_file_bytes().to_vec();
        let migration =
            smw_us_v1_expanded_settings_generation_102_migration(image.logical_bytes()).unwrap();
        let mut project = Project::new(image);
        project
            .replace_relocatable_patch(
                &migration.plan,
                &RatsOwnershipManifest {
                    owned: vec![migration.previous_allocation],
                    retained: Vec::new(),
                },
                0xff,
            )
            .unwrap();
        assert!(
            crate::smw_us_v1_installed_expanded_settings_layout(&project)
                .unwrap()
                .is_some()
        );
        project.undo().unwrap();
        assert_eq!(project.rom.as_file_bytes(), before);
    }

    #[test]
    fn external_generation_101_oracles_migrate_when_supplied() {
        let Ok(paths) = std::env::var("LM_EXPANDED_SETTINGS_101_ROMS") else {
            return;
        };
        for path in std::env::split_paths(&paths) {
            let image = RomImage::from_bytes(fs::read(&path).unwrap()).unwrap();
            let before = image.as_file_bytes().to_vec();
            let migration =
                smw_us_v1_expanded_settings_generation_101_migration(image.logical_bytes())
                    .unwrap();
            assert_eq!(migration.previous_allocation.payload.len(), 0x6d00);
            let allocation =
                SmwUsV1ExpandedSettingsAllocation::decode(&migration.plan.payloads[0].bytes)
                    .unwrap();
            assert_eq!(allocation.records().len(), 0x208);
            assert_eq!(
                allocation.record(0x200).unwrap(),
                &crate::smw_us_v1_default_special_expanded_settings_record()
            );

            let mut corrupt = image.logical_bytes().to_vec();
            corrupt[0x07_fd80 + 7] ^= 1;
            assert!(matches!(
                smw_us_v1_expanded_settings_generation_101_migration(&corrupt),
                Err(
                    SmwUsV1ExpandedSettingsGeneration101MigrationError::RuntimeDigestMismatch {
                        offset: 0x07_fd80,
                    }
                )
            ));

            let mut project = Project::new(image);
            let result = project
                .replace_relocatable_patch(
                    &migration.plan,
                    &RatsOwnershipManifest {
                        owned: vec![migration.previous_allocation],
                        retained: Vec::new(),
                    },
                    0xff,
                )
                .unwrap();
            let installed_base =
                pc_to_snes(Mapper::LoRom, result.blocks[0].payload.start).unwrap() & 0x7f_ffff;
            let installed_runtime = ExpandedSettingsRuntimeLayout::smw_us_v1(
                installed_base,
                ExpandedSettingsEntryContinuation::Continue,
            );
            for write in smw_us_v1_expanded_settings_fixed_writes(installed_runtime).unwrap() {
                assert_eq!(
                    project
                        .rom
                        .read(write.offset, write.replacement.len())
                        .unwrap(),
                    write.replacement,
                    "current runtime mismatch at {:#08x} for {}",
                    write.offset,
                    path.display()
                );
            }
            assert!(
                crate::smw_us_v1_installed_expanded_settings_layout(&project)
                    .unwrap()
                    .is_some()
            );
            project.undo().unwrap();
            assert_eq!(project.rom.as_file_bytes(), before, "{}", path.display());
        }
    }

    #[test]
    fn external_generation_100_oracles_migrate_when_supplied() {
        let Ok(paths) = std::env::var("LM_EXPANDED_SETTINGS_100_ROMS") else {
            return;
        };
        for path in std::env::split_paths(&paths) {
            let image = RomImage::from_bytes(fs::read(&path).unwrap()).unwrap();
            let before = image.as_file_bytes().to_vec();
            let migration =
                smw_us_v1_expanded_settings_generation_100_migration(image.logical_bytes())
                    .unwrap();
            assert_eq!(migration.previous_allocation.payload.len(), 0x6d00);
            let allocation =
                SmwUsV1ExpandedSettingsAllocation::decode(&migration.plan.payloads[0].bytes)
                    .unwrap();
            assert_eq!(allocation.records().len(), 0x208);
            assert_eq!(
                allocation.record(0x200).unwrap(),
                &crate::smw_us_v1_default_special_expanded_settings_record()
            );

            let mut corrupt_marker = image.logical_bytes().to_vec();
            corrupt_marker[SMW_US_V1_LEGACY_GRAPHICS_GENERATION_100_MARKER_OFFSET + 2] ^= 1;
            assert!(matches!(
                smw_us_v1_expanded_settings_generation_100_migration(&corrupt_marker),
                Err(SmwUsV1ExpandedSettingsGeneration100MigrationError::Legacy(
                    SmwUsV1ExpandedSettingsGeneration101MigrationError::LegacyMarkerMismatch
                ))
            ));

            let mut corrupt_owner = image.logical_bytes().to_vec();
            corrupt_owner[migration.previous_allocation.header_offset + 6] ^= 1;
            assert!(matches!(
                smw_us_v1_expanded_settings_generation_100_migration(&corrupt_owner),
                Err(SmwUsV1ExpandedSettingsGeneration100MigrationError::Legacy(
                    SmwUsV1ExpandedSettingsGeneration101MigrationError::RuntimeHeader(_)
                ))
            ));

            let mut corrupt = image.logical_bytes().to_vec();
            corrupt[0x07_fd80 + 7] ^= 1;
            assert!(matches!(
                smw_us_v1_expanded_settings_generation_100_migration(&corrupt),
                Err(SmwUsV1ExpandedSettingsGeneration100MigrationError::Legacy(
                    SmwUsV1ExpandedSettingsGeneration101MigrationError::RuntimeDigestMismatch {
                        offset: 0x07_fd80,
                    }
                ))
            ));

            let mut project = Project::new(image);
            let result = project
                .replace_relocatable_patch(
                    &migration.plan,
                    &RatsOwnershipManifest {
                        owned: vec![migration.previous_allocation],
                        retained: Vec::new(),
                    },
                    0xff,
                )
                .unwrap();
            let installed_base =
                pc_to_snes(Mapper::LoRom, result.blocks[0].payload.start).unwrap() & 0x7f_ffff;
            let installed_runtime = ExpandedSettingsRuntimeLayout::smw_us_v1(
                installed_base,
                ExpandedSettingsEntryContinuation::Continue,
            );
            for write in smw_us_v1_expanded_settings_fixed_writes(installed_runtime).unwrap() {
                assert_eq!(
                    project
                        .rom
                        .read(write.offset, write.replacement.len())
                        .unwrap(),
                    write.replacement,
                    "current runtime mismatch at {:#08x} for {}",
                    write.offset,
                    path.display()
                );
            }
            assert!(
                crate::smw_us_v1_installed_expanded_settings_layout(&project)
                    .unwrap()
                    .is_some()
            );
            project.undo().unwrap();
            assert_eq!(project.rom.as_file_bytes(), before, "{}", path.display());
        }
    }
}
