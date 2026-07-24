use crate::{
    Observation, ObservationError, observe_boss_sequence_messages, observe_event_reveals,
    observe_event_tilemap_buffers, observe_expanded_settings, observe_overworld_messages,
    observe_transferred_map16,
};
use lm_level::ExpandedOverworldSettings;
use lm_overworld::{
    BossSequenceMessageTable, EventNumberMap, EventRevealTable, EventTilemapBuffers,
    NativeOverworldLevelNameTable, NativeOverworldPlayerStarts, OverworldMessage,
    OverworldPathLinkTable, OverworldWarpLinkTable, SpecialEventRevealTable,
};

/// Every independently decoded native table currently qualified by the real Lunar Magic 3.63
/// `-TransferOverworld` transition.
#[derive(Clone, Copy)]
pub struct TransferOverworldDomains<'a> {
    pub map16_definitions: &'a [u16],
    pub map16_acts_like: &'a [u16],
    pub reveals: &'a EventRevealTable,
    pub event_numbers: &'a EventNumberMap,
    pub special: &'a SpecialEventRevealTable,
    pub tilemaps: &'a EventTilemapBuffers,
    pub paths: &'a OverworldPathLinkTable,
    pub warps: &'a OverworldWarpLinkTable,
    pub level_names: &'a NativeOverworldLevelNameTable,
    pub player_starts: &'a NativeOverworldPlayerStarts,
    pub settings: &'a ExpandedOverworldSettings,
    pub messages: &'a [OverworldMessage],
    pub boss_sequence: &'a BossSequenceMessageTable,
}

/// Observes all native domains currently recovered from Lunar Magic's overworld transfer.
///
/// Runtime addresses, mirror spellings, compression packets, and allocation positions remain
/// byte-oracle concerns. This view makes every decoded table entry independently addressable.
///
/// # Errors
///
/// Returns an observation error if any generated semantic path collides.
pub fn observe_transfer_overworld(
    domains: TransferOverworldDomains<'_>,
) -> Result<Observation, ObservationError> {
    let mut result = observe_transfer_overworld_events(
        domains.reveals,
        domains.event_numbers,
        domains.special,
        domains.tilemaps,
    )?;
    merge(
        &mut result,
        &observe_transferred_map16(domains.map16_definitions, domains.map16_acts_like)?,
    )?;
    observe_paths(&mut result, domains.paths)?;
    observe_warps(&mut result, domains.warps)?;
    observe_level_names(&mut result, domains.level_names)?;
    observe_player_starts(&mut result, domains.player_starts)?;
    result.insert(
        "overworld/expanded-settings/count",
        domains.settings.records.len().to_string(),
    )?;
    for (index, record) in domains.settings.records.iter().enumerate() {
        merge_at(
            &mut result,
            &format!("overworld/expanded-settings/{index:02x}"),
            &observe_expanded_settings(record),
            "expanded-settings",
        )?;
    }
    merge(&mut result, &observe_overworld_messages(domains.messages))?;
    merge(
        &mut result,
        &observe_boss_sequence_messages(domains.boss_sequence)?,
    )?;
    Ok(result)
}

/// Observes the four native event domains written by Lunar Magic's overworld transfer.
///
/// Physical runtime addresses, `LoROM` mirror choices, compression, and RATS placement are
/// deliberately excluded. Those details remain covered by the oracle manifest's changed ranges
/// and ownership inventory, while this observation records the decoded editor meaning.
///
/// # Errors
///
/// Returns an observation error if any generated semantic path collides.
pub fn observe_transfer_overworld_events(
    reveals: &EventRevealTable,
    event_numbers: &EventNumberMap,
    special: &SpecialEventRevealTable,
    tilemaps: &EventTilemapBuffers,
) -> Result<Observation, ObservationError> {
    let mut result = Observation::new();
    merge(&mut result, &observe_event_reveals(reveals)?)?;
    result.insert(
        "overworld/event-number-map/count",
        event_numbers.stored_len().to_string(),
    )?;
    for (index, value) in event_numbers.encode().iter().enumerate() {
        result.insert(
            format!("overworld/event-number-map/{index:02x}"),
            format!("{value:02x}"),
        )?;
    }
    result.insert(
        "overworld/special-event-reveals/count",
        SpecialEventRevealTable::ENTRY_COUNT.to_string(),
    )?;
    for (index, reveal) in special.reveals.iter().enumerate() {
        let base = format!("overworld/special-event-reveals/{index:02x}");
        result.insert(
            format!("{base}/source"),
            format!("{:04x}", reveal.source_tile),
        )?;
        result.insert(
            format!("{base}/destination"),
            format!("{:04x}", reveal.destination_tile),
        )?;
        result.insert(
            format!("{base}/direction"),
            format!("{:02x}", special.directions[index]),
        )?;
    }
    merge(&mut result, &observe_event_tilemap_buffers(tilemaps)?)?;
    Ok(result)
}

fn merge(result: &mut Observation, source: &Observation) -> Result<(), ObservationError> {
    for (path, value) in source.entries() {
        result.insert(path, value)?;
    }
    Ok(())
}

fn merge_at(
    result: &mut Observation,
    target_base: &str,
    source: &Observation,
    source_base: &str,
) -> Result<(), ObservationError> {
    for (path, value) in source.entries() {
        let suffix = path.strip_prefix(source_base).unwrap_or(path);
        result.insert(format!("{target_base}{suffix}"), value)?;
    }
    Ok(())
}

fn observe_paths(
    result: &mut Observation,
    table: &OverworldPathLinkTable,
) -> Result<(), ObservationError> {
    result.insert(
        "overworld/native-path-links/count",
        table.links.len().to_string(),
    )?;
    for (index, link) in table.links.iter().enumerate() {
        let base = format!("overworld/native-path-links/{index:02x}");
        for (name, value) in [
            ("source-x", u32::from(link.source.x)),
            ("source-y", u32::from(link.source.y)),
            ("source-submap", u32::from(link.source.submap)),
            ("destination-x", u32::from(link.destination.x)),
            ("destination-y", u32::from(link.destination.y)),
            ("destination-submap", u32::from(link.destination.submap)),
            ("target-y", u32::from(link.target.y_tile)),
            ("target-x", u32::from(link.target.x_tile)),
        ] {
            result.insert(format!("{base}/{name}"), format!("{value:04x}"))?;
        }
    }
    Ok(())
}

fn observe_warps(
    result: &mut Observation,
    table: &OverworldWarpLinkTable,
) -> Result<(), ObservationError> {
    result.insert(
        "overworld/native-warp-links/count",
        table.links.len().to_string(),
    )?;
    for (index, link) in table.links.iter().enumerate() {
        let base = format!("overworld/native-warp-links/{index:02x}");
        for (name, value) in [
            ("source-vertical", link.source.packed_vertical),
            ("source-horizontal", link.source.horizontal_tile),
            ("destination-vertical", link.destination.packed_vertical),
            ("destination-horizontal", link.destination.horizontal_tile),
        ] {
            result.insert(format!("{base}/{name}"), format!("{value:04x}"))?;
        }
    }
    Ok(())
}

fn observe_level_names(
    result: &mut Observation,
    table: &NativeOverworldLevelNameTable,
) -> Result<(), ObservationError> {
    result.insert(
        "overworld/native-level-names/count",
        table.names.len().to_string(),
    )?;
    for (index, name) in table.names.iter().enumerate() {
        let base = format!("overworld/native-level-names/{index:02x}");
        result.insert(format!("{base}/level"), format!("{:03x}", name.level))?;
        result.insert(format!("{base}/tiles"), hex(&name.tiles))?;
        result.insert(
            format!("{base}/raw-flags"),
            format!("{:02x}", name.raw_flags),
        )?;
    }
    Ok(())
}

fn observe_player_starts(
    result: &mut Observation,
    starts: &NativeOverworldPlayerStarts,
) -> Result<(), ObservationError> {
    result.insert("overworld/native-player-starts/count", "2")?;
    result.insert(
        "overworld/native-player-starts/reserved",
        hex(&starts.reserved),
    )?;
    for (index, start) in starts.starts.iter().enumerate() {
        let base = format!("overworld/native-player-starts/{index}");
        result.insert(format!("{base}/player"), start.player.to_string())?;
        result.insert(format!("{base}/x"), format!("{:04x}", start.x))?;
        result.insert(format!("{base}/y"), format!("{:04x}", start.y))?;
        result.insert(
            format!("{base}/submap"),
            format!("{:02x}", start.submap.encoded()),
        )?;
        result.insert(
            format!("{base}/raw-flags"),
            format!("{:02x}", start.raw_flags),
        )?;
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("String writes cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::EventReveal;

    #[test]
    fn all_four_event_domains_are_independently_addressable() {
        let reveals = EventRevealTable {
            entries: vec![EventReveal {
                source_tile: 0x123,
                destination_tile: 0x456,
            }],
        };
        let mut event_numbers = EventNumberMap::default();
        event_numbers.set(0x5f, 7);
        let mut special = SpecialEventRevealTable::default();
        special.reveals[0].source_tile = 0x321;
        special.directions[0] = 3;
        let mut tilemaps = EventTilemapBuffers::default();
        tilemaps.primary_bytes_mut()[0] = 1;
        let observation =
            observe_transfer_overworld_events(&reveals, &event_numbers, &special, &tilemaps)
                .unwrap();
        assert_eq!(observation.get("overworld/event-reveals/count"), Some("1"));
        assert_eq!(observation.get("overworld/event-number-map/5f"), Some("07"));
        assert_eq!(
            observation.get("overworld/special-event-reveals/00/source"),
            Some("0321")
        );
        assert!(
            observation
                .get("overworld/event-tilemap/index-plane-sha256")
                .is_some()
        );
    }

    #[test]
    fn complete_observation_addresses_every_non_event_domain() {
        use lm_level::ExpandedLevelSettingsRecord;
        use lm_overworld::{
            BossSequenceMessageTable, NativeOverworldPlayerStarts, OverworldEndpoint,
            OverworldLevelName, OverworldPathLink, OverworldPathTarget, OverworldWarpEndpoint,
            OverworldWarpLink,
        };

        let reveals = EventRevealTable::default();
        let event_numbers = EventNumberMap::default();
        let special = SpecialEventRevealTable::default();
        let tilemaps = EventTilemapBuffers::default();
        let paths = OverworldPathLinkTable {
            links: vec![OverworldPathLink {
                source: OverworldEndpoint {
                    x: 1,
                    y: 2,
                    submap: 3,
                },
                destination: OverworldEndpoint {
                    x: 4,
                    y: 5,
                    submap: 6,
                },
                target: OverworldPathTarget {
                    y_tile: 7,
                    x_tile: 8,
                },
            }],
        };
        let warps = OverworldWarpLinkTable {
            links: vec![OverworldWarpLink {
                source: OverworldWarpEndpoint {
                    packed_vertical: 9,
                    horizontal_tile: 10,
                },
                destination: OverworldWarpEndpoint {
                    packed_vertical: 11,
                    horizontal_tile: 12,
                },
            }],
        };
        let level_names = NativeOverworldLevelNameTable {
            names: vec![OverworldLevelName {
                level: 0,
                tiles: [0x13; OverworldLevelName::TILE_COUNT],
                raw_flags: 0,
            }],
        };
        let player_starts = NativeOverworldPlayerStarts::decode(&[
            1, 1, 2, 0, 2, 0, 0x68, 0, 0x78, 0, 0x68, 0, 0x78, 0, 6, 0, 7, 0, 6, 0, 7, 0,
        ])
        .unwrap();
        let settings = ExpandedOverworldSettings {
            records: std::array::from_fn(|index| {
                ExpandedLevelSettingsRecord::from_encoded([u8::try_from(index).unwrap(); 32])
            }),
        };
        let messages = vec![OverworldMessage([0x1f; OverworldMessage::ENCODED_LEN])];
        let boss_sequence = BossSequenceMessageTable::default();
        let observation = observe_transfer_overworld(TransferOverworldDomains {
            map16_definitions: &[0x1234],
            map16_acts_like: &[0x5678],
            reveals: &reveals,
            event_numbers: &event_numbers,
            special: &special,
            tilemaps: &tilemaps,
            paths: &paths,
            warps: &warps,
            level_names: &level_names,
            player_starts: &player_starts,
            settings: &settings,
            messages: &messages,
            boss_sequence: &boss_sequence,
        })
        .unwrap();
        assert_eq!(
            observation.get("overworld/native-path-links/00/target-x"),
            Some("0008")
        );
        assert_eq!(
            observation.get("overworld/native-warp-links/00/destination-horizontal"),
            Some("000c")
        );
        assert_eq!(
            observation.get("overworld/native-level-names/00/tiles"),
            Some("13131313131313131313131313131313131313")
        );
        assert_eq!(
            observation.get("overworld/native-player-starts/1/x"),
            Some("0068")
        );
        assert_eq!(
            observation.get("overworld/expanded-settings/06/words/00"),
            Some("1542")
        );
        assert_eq!(observation.get("overworld/messages/count"), Some("1"));
        assert_eq!(
            observation.get("overworld/boss-sequence/message-count"),
            Some("7")
        );
    }
}
