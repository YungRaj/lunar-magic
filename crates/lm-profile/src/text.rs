mod values;

use values::{
    boolean, byte, hex, number, parse_game, parse_graphics_compression, parse_mapper, parse_region,
    parse_values, signed_number, take,
};

use crate::RevisionProfile;
use crate::text_schema::{EXPANDED_SETTINGS_KEYS, LAYER2_KEYS, TABLES};
use lm_level::SpriteLengthTable;
use lm_project::{
    ChainedSnesPointerLocator, CompleteOverworldRomLayout, CompleteOverworldShape,
    EndpointRomLayout, EventRevealRomLayout, ExAnimationRomLayout, ExpandedLevelSettingsLayout,
    GatedLayout, GraphicsRomLayout, InstallationMarker, InstalledExAnimationRomLayout,
    InstalledLayout, LevelLayer2RomLayout, LevelLayer2TilemapEncoding, LevelPointerTable,
    LevelRomLayout, Map16RomLayout, MessageRomLayout, OverworldLayersRomLayout, PaletteRomLayout,
    SpritePointerTable, SpriteRomLayout,
};
use lm_rom::{Mapper, Region, SupportedGame};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RevisionProfileError {
    TextTooLong {
        actual: usize,
        maximum: usize,
    },
    TooManyLines {
        maximum: usize,
    },
    LineTooLong {
        line: usize,
        actual: usize,
        maximum: usize,
    },
    MissingMagic,
    UnsupportedVersion(String),
    MalformedLine(usize),
    UnknownKey {
        line: usize,
        key: String,
    },
    DuplicateKey(String),
    MissingKey(String),
    InvalidNumber {
        key: String,
        value: String,
    },
    InvalidBoolean {
        key: String,
        value: String,
    },
    InvalidMapper(String),
    InvalidGraphicsCompression(String),
    InvalidSpritePointerEncoding(String),
    IncompleteSpritePointerLayout,
    InvalidLayer2TilemapEncoding(String),
    IncompleteLayer2Layout,
    InvalidInstallationMode {
        domain: &'static str,
        value: String,
    },
    IncompleteInstallationLayout(&'static str),
    InvalidPointerPresenceMask,
    InstallationLayoutMismatch(&'static str),
    InvalidGame(String),
    InvalidRegion(String),
    InvalidHex {
        key: &'static str,
    },
    InvalidTableLength {
        key: &'static str,
        actual: usize,
        expected: usize,
    },
    InvalidName,
    NameTooLong {
        actual: usize,
        maximum: usize,
    },
    MapperMismatch {
        domain: &'static str,
        actual: Mapper,
    },
    ZeroValue(&'static str),
    InvalidPointerStride {
        domain: &'static str,
        stride: usize,
    },
    InvalidExpandedSettingsLayout,
    IncompleteExpandedSettingsLayout,
    ExpandedSettingsTableOverlap {
        pointer_table: &'static str,
    },
    PointerTableEntryLimit {
        domain: &'static str,
        actual: usize,
        maximum: usize,
    },
    OverlappingPointerTables {
        first: &'static str,
        second: &'static str,
    },
    AddressOverflow(&'static str),
    UnmappedPointerTable(&'static str),
    OverworldShapeMismatch,
    InvalidSpriteLength,
    IdentityMismatch {
        actual_game: SupportedGame,
        actual_region: Region,
        actual_revision: u8,
        actual_mapper: Mapper,
    },
}

impl fmt::Display for RevisionProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid revision profile: {self:?}")
    }
}
impl std::error::Error for RevisionProfileError {}

pub(super) fn parse(input: &str) -> Result<RevisionProfile, RevisionProfileError> {
    let mut values = parse_values(input)?;
    let name = take(&mut values, "name")?;
    let game = parse_game(&take(&mut values, "game")?)?;
    let region = parse_region(&take(&mut values, "region")?)?;
    let revision = byte(&mut values, "revision")?;
    let mapper = parse_mapper(&take(&mut values, "mapper")?)?;
    let tables = parse_tables(&mut values)?;
    let sprite_pointers = parse_sprite_pointers(&mut values, tables[1])?;
    let layer2 = parse_layer2(&mut values, mapper)?;
    let expanded_settings = parse_expanded_settings(&mut values, mapper)?;
    let expanded_sprites = boolean(&mut values, "level.expanded_sprites")?;
    let graphics_maximum_compressed_len = number(&mut values, "graphics.maximum_compressed_len")?;
    let graphics_maximum_decompressed_len =
        number(&mut values, "graphics.maximum_decompressed_len")?;
    let graphics_compression =
        parse_graphics_compression(&take(&mut values, "graphics.compression")?)?;
    let palette_colors = number(&mut values, "palette.colors")?;
    let exanimation_maximum_records = number(&mut values, "exanimation.maximum_records")?;
    let exanimation_maximum_encoded_len = number(&mut values, "exanimation.maximum_encoded_len")?;
    let shape = parse_shape(&mut values)?;
    let ow_animation_records = number(&mut values, "overworld.animation_maximum_records")?;
    let ow_animation_len = number(&mut values, "overworld.animation_maximum_encoded_len")?;
    let sprite_lengths = parse_sprite_lengths(&mut values)?;
    let modes = parse_modes(&mut values)?;
    let palette = PaletteRomLayout {
        mapper,
        pointers: tables[5],
        colors_per_palette: palette_colors,
    };
    let exanimation = ExAnimationRomLayout {
        mapper,
        pointers: tables[6],
        maximum_records: exanimation_maximum_records,
        maximum_encoded_len: exanimation_maximum_encoded_len,
    };
    let palette_installation = parse_palette_installation(&mut values, palette)?;
    let exanimation_installation = parse_exanimation_installation(&mut values, exanimation)?;
    let profile = RevisionProfile {
        name,
        game,
        region,
        revision,
        mapper,
        level: LevelRomLayout {
            mapper,
            layer1: tables[0],
            sprites: sprite_pointers,
            expanded_sprites,
        },
        layer2,
        map16: Map16RomLayout {
            mapper,
            graphics: tables[2],
            acts_like: tables[3],
        },
        graphics: GraphicsRomLayout {
            mapper,
            pointers: tables[4],
            compression: graphics_compression,
            maximum_compressed_len: graphics_maximum_compressed_len,
            maximum_decompressed_len: graphics_maximum_decompressed_len,
        },
        palette,
        palette_installation,
        exanimation,
        exanimation_installation,
        expanded_settings,
        overworld: build_overworld(
            mapper,
            &tables,
            shape,
            ow_animation_records,
            ow_animation_len,
        ),
        overworld_shape: shape,
        sprite_lengths,
        exanimation_double_size_modes: modes,
    };
    profile.validate()?;
    Ok(profile)
}

fn parse_palette_installation(
    values: &mut BTreeMap<String, String>,
    layout: PaletteRomLayout,
) -> Result<InstalledLayout<PaletteRomLayout>, RevisionProfileError> {
    let Some(mode) = values.remove("palette.installation") else {
        if values.contains_key("palette.marker_offset")
            || values.contains_key("palette.marker_value")
        {
            return Err(RevisionProfileError::IncompleteInstallationLayout(
                "palette",
            ));
        }
        return Ok(InstalledLayout::Unconditional(layout));
    };
    match mode.as_str() {
        "absent" | "unconditional"
            if values.contains_key("palette.marker_offset")
                || values.contains_key("palette.marker_value") =>
        {
            Err(RevisionProfileError::IncompleteInstallationLayout(
                "palette",
            ))
        }
        "absent" => Ok(InstalledLayout::Absent),
        "unconditional" => Ok(InstalledLayout::Unconditional(layout)),
        "marker" => Ok(InstalledLayout::Alternatives {
            primary: GatedLayout {
                marker: InstallationMarker {
                    offset: number(values, "palette.marker_offset")?,
                    expected: byte(values, "palette.marker_value")?,
                },
                layout,
            },
            fallback: None,
        }),
        _ => Err(RevisionProfileError::InvalidInstallationMode {
            domain: "palette",
            value: mode,
        }),
    }
}

fn parse_exanimation_installation(
    values: &mut BTreeMap<String, String>,
    payload: ExAnimationRomLayout,
) -> Result<InstalledLayout<InstalledExAnimationRomLayout>, RevisionProfileError> {
    let Some(mode) = values.remove("exanimation.installation") else {
        if exanimation_installation_key_count(values) != 0 {
            return Err(RevisionProfileError::IncompleteInstallationLayout(
                "exanimation",
            ));
        }
        return Ok(InstalledLayout::Unconditional(
            InstalledExAnimationRomLayout {
                payload,
                pointer_presence_mask: 0x00ff_0000,
                pointer_locator: parse_pointer_locator(values, "primary", payload.mapper)?,
            },
        ));
    };
    if mode == "absent" {
        if exanimation_installation_key_count(values) != 0 {
            return Err(RevisionProfileError::IncompleteInstallationLayout(
                "exanimation",
            ));
        }
        return Ok(InstalledLayout::Absent);
    }
    if mode == "unconditional" {
        if [
            "exanimation.primary_marker_offset",
            "exanimation.primary_marker_value",
            "exanimation.fallback_marker_offset",
            "exanimation.fallback_marker_value",
            "exanimation.fallback_pointer_mask",
            "exanimation.fallback_locator_operand_offset",
            "exanimation.fallback_locator_displacement",
        ]
        .iter()
        .any(|key| values.contains_key(*key))
        {
            return Err(RevisionProfileError::IncompleteInstallationLayout(
                "exanimation",
            ));
        }
        return Ok(InstalledLayout::Unconditional(
            InstalledExAnimationRomLayout {
                payload,
                pointer_presence_mask: pointer_mask(values, "primary")?,
                pointer_locator: parse_pointer_locator(values, "primary", payload.mapper)?,
            },
        ));
    }
    if mode != "alternatives" {
        return Err(RevisionProfileError::InvalidInstallationMode {
            domain: "exanimation",
            value: mode,
        });
    }
    let primary = parse_exanimation_gate(values, payload, "primary")?;
    let fallback_gate_keys = [
        "exanimation.fallback_marker_offset",
        "exanimation.fallback_marker_value",
        "exanimation.fallback_pointer_mask",
    ];
    let fallback_count = fallback_gate_keys
        .iter()
        .filter(|key| values.contains_key(**key))
        .count();
    let fallback = match fallback_count {
        0 => None,
        3 => Some(parse_exanimation_gate(values, payload, "fallback")?),
        _ => {
            return Err(RevisionProfileError::IncompleteInstallationLayout(
                "exanimation",
            ));
        }
    };
    Ok(InstalledLayout::Alternatives { primary, fallback })
}

fn exanimation_installation_key_count(values: &BTreeMap<String, String>) -> usize {
    [
        "exanimation.primary_marker_offset",
        "exanimation.primary_marker_value",
        "exanimation.primary_pointer_mask",
        "exanimation.primary_locator_operand_offset",
        "exanimation.primary_locator_displacement",
        "exanimation.fallback_marker_offset",
        "exanimation.fallback_marker_value",
        "exanimation.fallback_pointer_mask",
        "exanimation.fallback_locator_operand_offset",
        "exanimation.fallback_locator_displacement",
    ]
    .iter()
    .filter(|key| values.contains_key(**key))
    .count()
}

fn parse_exanimation_gate(
    values: &mut BTreeMap<String, String>,
    payload: ExAnimationRomLayout,
    prefix: &'static str,
) -> Result<GatedLayout<InstalledExAnimationRomLayout>, RevisionProfileError> {
    Ok(GatedLayout {
        marker: InstallationMarker {
            offset: number(values, &format!("exanimation.{prefix}_marker_offset"))?,
            expected: byte(values, &format!("exanimation.{prefix}_marker_value"))?,
        },
        layout: InstalledExAnimationRomLayout {
            payload,
            pointer_presence_mask: pointer_mask(values, prefix)?,
            pointer_locator: parse_pointer_locator(values, prefix, payload.mapper)?,
        },
    })
}

fn parse_pointer_locator(
    values: &mut BTreeMap<String, String>,
    prefix: &'static str,
    mapper: Mapper,
) -> Result<Option<ChainedSnesPointerLocator>, RevisionProfileError> {
    let operand_key = format!("exanimation.{prefix}_locator_operand_offset");
    let displacement_key = format!("exanimation.{prefix}_locator_displacement");
    match (
        values.contains_key(&operand_key),
        values.contains_key(&displacement_key),
    ) {
        (false, false) => Ok(None),
        (true, true) => Ok(Some(ChainedSnesPointerLocator {
            mapper,
            first_operand_offset: number(values, &operand_key)?,
            final_operand_displacement: signed_number(values, &displacement_key)?,
        })),
        _ => Err(RevisionProfileError::IncompleteInstallationLayout(
            "exanimation pointer locator",
        )),
    }
}

fn pointer_mask(
    values: &mut BTreeMap<String, String>,
    prefix: &'static str,
) -> Result<u32, RevisionProfileError> {
    let value = number(values, &format!("exanimation.{prefix}_pointer_mask"))?;
    let mask =
        u32::try_from(value).map_err(|_| RevisionProfileError::InvalidPointerPresenceMask)?;
    if mask == 0 || mask & !0x00ff_ffff != 0 {
        return Err(RevisionProfileError::InvalidPointerPresenceMask);
    }
    Ok(mask)
}

fn parse_layer2(
    values: &mut BTreeMap<String, String>,
    mapper: Mapper,
) -> Result<Option<LevelLayer2RomLayout>, RevisionProfileError> {
    let present = LAYER2_KEYS
        .iter()
        .filter(|key| values.contains_key(**key))
        .count();
    if present == 0 {
        return Ok(None);
    }
    let encoding = values
        .remove("level.layer2.tilemap_encoding")
        .ok_or(RevisionProfileError::IncompleteLayer2Layout)?;
    let high_byte = values.remove("level.layer2.high_byte");
    let tilemap_encoding = match (encoding.as_str(), high_byte) {
        ("legacy", Some(value)) => LevelLayer2TilemapEncoding::Legacy {
            high_byte: parse_byte_value("level.layer2.high_byte", value)?,
        },
        ("split-planes", None) => LevelLayer2TilemapEncoding::SplitPlanes,
        ("legacy" | "split-planes", _) => {
            return Err(RevisionProfileError::IncompleteLayer2Layout);
        }
        _ => return Err(RevisionProfileError::InvalidLayer2TilemapEncoding(encoding)),
    };
    Ok(Some(LevelLayer2RomLayout {
        mapper,
        pointers: LevelPointerTable {
            offset: number(values, "level.layer2.offset")?,
            entries: number(values, "level.layer2.entries")?,
            stride: number(values, "level.layer2.stride")?,
        },
        maximum_compressed_len: number(values, "level.layer2.maximum_compressed_len")?,
        tilemap_encoding,
    }))
}

fn parse_byte_value(key: &str, value: String) -> Result<u8, RevisionProfileError> {
    let parsed = value
        .strip_prefix("0x")
        .map_or_else(|| value.parse(), |hex| usize::from_str_radix(hex, 16));
    parsed
        .ok()
        .and_then(|value| u8::try_from(value).ok())
        .ok_or(RevisionProfileError::InvalidNumber {
            key: key.into(),
            value,
        })
}

fn parse_sprite_pointers(
    values: &mut BTreeMap<String, String>,
    table: LevelPointerTable,
) -> Result<SpritePointerTable, RevisionProfileError> {
    let Some(encoding) = values.remove("level.sprites.encoding") else {
        if values.contains_key("level.sprites.bank_offset")
            || values.contains_key("level.sprites.bank_stride")
        {
            return Err(RevisionProfileError::IncompleteSpritePointerLayout);
        }
        return Ok(table.into());
    };
    match encoding.as_str() {
        "contiguous" => {
            if values.contains_key("level.sprites.bank_offset")
                || values.contains_key("level.sprites.bank_stride")
            {
                return Err(RevisionProfileError::IncompleteSpritePointerLayout);
            }
            Ok(table.into())
        }
        "split-shared-bank" => {
            let bank_offset = number(values, "level.sprites.bank_offset")?;
            if values.contains_key("level.sprites.bank_stride") {
                return Err(RevisionProfileError::IncompleteSpritePointerLayout);
            }
            Ok(SpritePointerTable::SplitSharedBank {
                low_words: table,
                bank_offset,
            })
        }
        "split-bank-table" => Ok(SpritePointerTable::SplitBankTable {
            low_words: table,
            banks: LevelPointerTable {
                offset: number(values, "level.sprites.bank_offset")?,
                entries: table.entries,
                stride: number(values, "level.sprites.bank_stride")?,
            },
        }),
        _ => Err(RevisionProfileError::InvalidSpritePointerEncoding(encoding)),
    }
}

fn parse_expanded_settings(
    values: &mut BTreeMap<String, String>,
    mapper: Mapper,
) -> Result<Option<ExpandedLevelSettingsLayout>, RevisionProfileError> {
    let present = EXPANDED_SETTINGS_KEYS
        .iter()
        .filter(|key| values.contains_key(**key))
        .count();
    if present == 0 {
        return Ok(None);
    }
    if present != EXPANDED_SETTINGS_KEYS.len() {
        return Err(RevisionProfileError::IncompleteExpandedSettingsLayout);
    }
    Ok(Some(ExpandedLevelSettingsLayout {
        mapper,
        table_offset: number(values, EXPANDED_SETTINGS_KEYS[0])?,
        entries: number(values, EXPANDED_SETTINGS_KEYS[1])?,
        stride: number(values, EXPANDED_SETTINGS_KEYS[2])?,
    }))
}

fn parse_tables(
    values: &mut BTreeMap<String, String>,
) -> Result<[LevelPointerTable; 16], RevisionProfileError> {
    let mut table = |prefix: &'static str| -> Result<LevelPointerTable, RevisionProfileError> {
        Ok(LevelPointerTable {
            offset: number(values, &format!("{prefix}.offset"))?,
            entries: number(values, &format!("{prefix}.entries"))?,
            stride: number(values, &format!("{prefix}.stride"))?,
        })
    };
    let mut tables = [LevelPointerTable {
        offset: 0,
        entries: 0,
        stride: 0,
    }; 16];
    for (index, prefix) in TABLES.into_iter().enumerate() {
        tables[index] = table(prefix)?;
    }
    Ok(tables)
}

fn parse_shape(
    values: &mut BTreeMap<String, String>,
) -> Result<CompleteOverworldShape, RevisionProfileError> {
    let width = number(values, "overworld.width")?;
    let height = number(values, "overworld.height")?;
    let event_reveals = number(values, "overworld.event_reveals")?;
    let endpoints = number(values, "overworld.endpoints_per_slot")?;
    let messages = number(values, "overworld.messages_per_slot")?;
    let sprites = number(values, "overworld.sprites_per_slot")?;
    let sprite_record_len = number(values, "overworld.sprite_record_len")?;
    let ow_palette_colors = number(values, "overworld.palette_colors")?;
    Ok(CompleteOverworldShape {
        width,
        height,
        event_reveals,
        endpoints,
        messages,
        sprites,
        sprite_record_len,
        palette_colors: ow_palette_colors,
    })
}

fn parse_sprite_lengths(
    values: &mut BTreeMap<String, String>,
) -> Result<SpriteLengthTable, RevisionProfileError> {
    let sprite_bytes = hex(
        &take(values, "sprite_lengths")?,
        "sprite_lengths",
        SpriteLengthTable::ENCODED_LEN,
    )?;
    SpriteLengthTable::decode(&sprite_bytes).map_err(|actual| {
        RevisionProfileError::InvalidTableLength {
            key: "sprite_lengths",
            actual,
            expected: SpriteLengthTable::ENCODED_LEN,
        }
    })
}

fn parse_modes(values: &mut BTreeMap<String, String>) -> Result<[bool; 256], RevisionProfileError> {
    let mode_bytes = hex(
        &take(values, "exanimation_double_size_modes")?,
        "exanimation_double_size_modes",
        32,
    )?;
    let mut modes = [false; 256];
    for (index, mode) in modes.iter_mut().enumerate() {
        *mode = mode_bytes[index / 8] & (1 << (index % 8)) != 0;
    }
    Ok(modes)
}

fn build_overworld(
    mapper: Mapper,
    tables: &[LevelPointerTable; 16],
    shape: CompleteOverworldShape,
    animation_records: usize,
    animation_len: usize,
) -> CompleteOverworldRomLayout {
    CompleteOverworldRomLayout {
        layers: OverworldLayersRomLayout {
            mapper,
            layer1: tables[7],
            layer2: tables[8],
            width: shape.width,
            height: shape.height,
        },
        event_reveals: EventRevealRomLayout {
            mapper,
            sources: tables[9],
            destinations: tables[10],
            entries_per_slot: shape.event_reveals,
        },
        endpoints: EndpointRomLayout {
            mapper,
            pointers: tables[11],
            endpoints_per_slot: shape.endpoints,
        },
        messages: MessageRomLayout {
            mapper,
            pointers: tables[12],
            messages_per_slot: shape.messages,
        },
        sprites: SpriteRomLayout {
            mapper,
            pointers: tables[13],
            sprites_per_slot: shape.sprites,
            record_len: shape.sprite_record_len,
        },
        palette: PaletteRomLayout {
            mapper,
            pointers: tables[14],
            colors_per_palette: shape.palette_colors,
        },
        animation: ExAnimationRomLayout {
            mapper,
            pointers: tables[15],
            maximum_records: animation_records,
            maximum_encoded_len: animation_len,
        },
    }
}
