use crate::RevisionProfile;
use crate::text_schema::{EXPANDED_SETTINGS_KEYS, SCALARS, TABLES};
use lm_rom::{Mapper, Region, SupportedGame};
use std::fmt::Write;

pub(super) fn encode(profile: &RevisionProfile) -> String {
    let mut out = format!(
        "{}\nname={}\ngame={}\nregion={}\nrevision={}\nmapper={}\n",
        RevisionProfile::MAGIC,
        profile.name,
        game_name(profile.game),
        region_name(profile.region),
        profile.revision,
        mapper_name(profile.mapper)
    );
    let tables = [
        (TABLES[0], profile.level.layer1),
        (TABLES[1], profile.level.sprites.low_or_contiguous_table()),
        (TABLES[2], profile.map16.graphics),
        (TABLES[3], profile.map16.acts_like),
        (TABLES[4], profile.graphics.pointers),
        (TABLES[5], profile.palette.pointers),
        (TABLES[6], profile.exanimation.pointers),
        (TABLES[7], profile.overworld.layers.layer1),
        (TABLES[8], profile.overworld.layers.layer2),
        (TABLES[9], profile.overworld.event_reveals.sources),
        (TABLES[10], profile.overworld.event_reveals.destinations),
        (TABLES[11], profile.overworld.endpoints.pointers),
        (TABLES[12], profile.overworld.messages.pointers),
        (TABLES[13], profile.overworld.sprites.pointers),
        (TABLES[14], profile.overworld.palette.pointers),
        (TABLES[15], profile.overworld.animation.pointers),
    ];
    for (key, value) in tables {
        writeln!(
            out,
            "{key}.offset=0x{:x}\n{key}.entries={}\n{key}.stride={}",
            value.offset, value.entries, value.stride
        )
        .unwrap();
    }
    if let Some(planes) = profile.graphics.split_pointer_planes {
        writeln!(
            out,
            "graphics.pointer_encoding=split_planes\ngraphics.pointer_high_offset=0x{:x}\ngraphics.pointer_bank_offset=0x{:x}",
            planes.high_offset, planes.bank_offset
        )
        .unwrap();
    }
    encode_sprite_pointers(&mut out, profile);
    encode_layer2(&mut out, profile);
    encode_installations(&mut out, profile);
    if let Some(layout) = profile.expanded_settings {
        for (key, value) in EXPANDED_SETTINGS_KEYS.into_iter().zip([
            layout.table_offset,
            layout.entries,
            layout.stride,
        ]) {
            writeln!(out, "{key}=0x{value:x}").unwrap();
        }
    }
    encode_tail(&mut out, profile);
    out
}

fn encode_installations(out: &mut String, profile: &RevisionProfile) {
    match profile.palette_installation {
        lm_project::InstalledLayout::Absent => {
            writeln!(out, "palette.installation=absent").unwrap();
        }
        lm_project::InstalledLayout::Unconditional(_) => {
            writeln!(out, "palette.installation=unconditional").unwrap();
        }
        lm_project::InstalledLayout::Alternatives {
            primary,
            fallback: _,
        } => {
            writeln!(
                out,
                "palette.installation=marker\npalette.marker_offset=0x{:x}\npalette.marker_value=0x{:02x}",
                primary.marker.offset, primary.marker.expected
            )
            .unwrap();
        }
    }
    match profile.exanimation_installation {
        lm_project::InstalledLayout::Absent => {
            writeln!(out, "exanimation.installation=absent").unwrap();
        }
        lm_project::InstalledLayout::Unconditional(layout) => {
            writeln!(
                out,
                "exanimation.installation=unconditional\nexanimation.primary_pointer_mask=0x{:06x}",
                layout.pointer_presence_mask
            )
            .unwrap();
            encode_pointer_locator(out, "primary", layout.pointer_locator);
        }
        lm_project::InstalledLayout::Alternatives { primary, fallback } => {
            writeln!(
                out,
                "exanimation.installation=alternatives\nexanimation.primary_marker_offset=0x{:x}\nexanimation.primary_marker_value=0x{:02x}\nexanimation.primary_pointer_mask=0x{:06x}",
                primary.marker.offset,
                primary.marker.expected,
                primary.layout.pointer_presence_mask
            )
            .unwrap();
            encode_pointer_locator(out, "primary", primary.layout.pointer_locator);
            if let Some(fallback) = fallback {
                writeln!(
                    out,
                    "exanimation.fallback_marker_offset=0x{:x}\nexanimation.fallback_marker_value=0x{:02x}\nexanimation.fallback_pointer_mask=0x{:06x}",
                    fallback.marker.offset,
                    fallback.marker.expected,
                    fallback.layout.pointer_presence_mask
                )
                .unwrap();
                encode_pointer_locator(out, "fallback", fallback.layout.pointer_locator);
            }
        }
    }
    encode_exanimation_features(out, profile);
}

fn encode_exanimation_features(out: &mut String, profile: &RevisionProfile) {
    match profile.exanimation_feature_installation {
        lm_project::InstalledLayout::Absent => {
            writeln!(out, "exanimation.features=absent").unwrap();
        }
        lm_project::InstalledLayout::Unconditional(layout) => {
            writeln!(out, "exanimation.features=installed").unwrap();
            encode_exanimation_feature_variant(out, "primary", layout);
        }
        lm_project::InstalledLayout::Alternatives { primary, fallback } => {
            writeln!(out, "exanimation.features=installed").unwrap();
            encode_exanimation_feature_variant(out, "primary", primary.layout);
            if let Some(fallback) = fallback {
                encode_exanimation_feature_variant(out, "fallback", fallback.layout);
            }
        }
    }
}

fn encode_exanimation_feature_variant(
    out: &mut String,
    prefix: &str,
    layout: lm_project::InstalledExAnimationFeatureRomLayout,
) {
    let displacement = layout.table_locator.final_operand_displacement;
    let sign = if displacement < 0 { "-" } else { "" };
    let magnitude = displacement.unsigned_abs();
    writeln!(
        out,
        "exanimation.{prefix}_feature_table_displacement={sign}0x{magnitude:x}"
    )
    .unwrap();
}

fn encode_pointer_locator(
    out: &mut String,
    prefix: &str,
    locator: Option<lm_project::ChainedSnesPointerLocator>,
) {
    let Some(locator) = locator else {
        return;
    };
    let displacement = locator.final_operand_displacement;
    let sign = if displacement < 0 { "-" } else { "" };
    let magnitude = displacement.unsigned_abs();
    writeln!(
        out,
        "exanimation.{prefix}_locator_operand_offset=0x{:x}\nexanimation.{prefix}_locator_displacement={sign}0x{magnitude:x}",
        locator.first_operand_offset
    )
    .unwrap();
}

fn encode_layer2(out: &mut String, profile: &RevisionProfile) {
    let Some(layout) = profile.layer2 else {
        return;
    };
    writeln!(
        out,
        "level.layer2.offset=0x{:x}\nlevel.layer2.entries={}\nlevel.layer2.stride={}\nlevel.layer2.maximum_compressed_len={}",
        layout.pointers.offset,
        layout.pointers.entries,
        layout.pointers.stride,
        layout.maximum_compressed_len
    )
    .unwrap();
    if let Some(descriptor) = layout.descriptor_table {
        writeln!(
            out,
            "level.layer2.descriptor_offset=0x{:x}\nlevel.layer2.descriptor_entries={}\nlevel.layer2.descriptor_stride={}",
            descriptor.offset, descriptor.entries, descriptor.stride
        )
        .unwrap();
    }
    match layout.tilemap_encoding {
        lm_project::LevelLayer2TilemapEncoding::Legacy { high_byte } => {
            writeln!(
                out,
                "level.layer2.tilemap_encoding=legacy\nlevel.layer2.high_byte=0x{high_byte:02x}"
            )
            .unwrap();
        }
        lm_project::LevelLayer2TilemapEncoding::SplitPlanes => {
            writeln!(out, "level.layer2.tilemap_encoding=split-planes").unwrap();
        }
    }
}

fn encode_sprite_pointers(out: &mut String, profile: &RevisionProfile) {
    match profile.level.sprites {
        lm_project::SpritePointerTable::Contiguous(_) => {
            writeln!(out, "level.sprites.encoding=contiguous").unwrap();
        }
        lm_project::SpritePointerTable::SplitSharedBank { bank_offset, .. } => {
            writeln!(
                out,
                "level.sprites.encoding=split-shared-bank\nlevel.sprites.bank_offset=0x{bank_offset:x}"
            )
            .unwrap();
        }
        lm_project::SpritePointerTable::SplitBankTable { banks, .. } => {
            writeln!(
                out,
                "level.sprites.encoding=split-bank-table\nlevel.sprites.bank_offset=0x{:x}\nlevel.sprites.bank_stride={}",
                banks.offset, banks.stride
            )
            .unwrap();
        }
    }
}

fn encode_tail(out: &mut String, profile: &RevisionProfile) {
    let shape = profile.overworld_shape;
    let scalars = [
        usize::from(profile.level.expanded_sprites),
        profile.graphics.maximum_compressed_len,
        profile.graphics.maximum_decompressed_len,
        profile.palette.colors_per_palette,
        profile.exanimation.maximum_records,
        profile.exanimation.maximum_encoded_len,
        shape.width,
        shape.height,
        shape.event_reveals,
        shape.endpoints,
        shape.messages,
        shape.sprites,
        shape.sprite_record_len,
        shape.palette_colors,
        profile.overworld.animation.maximum_records,
        profile.overworld.animation.maximum_encoded_len,
    ];
    for (key, value) in SCALARS.into_iter().zip(scalars) {
        writeln!(out, "{key}={value}").unwrap();
    }
    writeln!(
        out,
        "graphics.compression={}",
        match profile.graphics.compression {
            lm_project::GraphicsCompression::Lz2 => "lz2",
            lm_project::GraphicsCompression::Lz3 => "lz3",
        }
    )
    .unwrap();
    writeln!(
        out,
        "sprite_lengths={}",
        encode_hex(profile.sprite_lengths.encoded())
    )
    .unwrap();
    let mut mode_bytes = [0_u8; 32];
    for (index, enabled) in profile.exanimation_double_size_modes.iter().enumerate() {
        if *enabled {
            mode_bytes[index / 8] |= 1 << (index % 8);
        }
    }
    writeln!(
        out,
        "exanimation_double_size_modes={}",
        encode_hex(&mode_bytes)
    )
    .unwrap();
}

fn mapper_name(mapper: Mapper) -> &'static str {
    match mapper {
        Mapper::LoRom => "lorom",
        Mapper::ExLoRom => "exlorom",
        Mapper::Sa1 => "sa1",
    }
}

fn game_name(game: SupportedGame) -> &'static str {
    match game {
        SupportedGame::SuperMarioWorld => "super-mario-world",
        SupportedGame::AllStarsAndWorld => "all-stars-and-world",
    }
}

fn region_name(region: Region) -> &'static str {
    match region {
        Region::Japan => "japan",
        Region::NorthAmerica => "north-america",
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            write!(output, "{byte:02x}").unwrap();
            output
        },
    )
}
