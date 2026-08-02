use crate::Layer3ExpandedModeFlags;

/// The source Lunar Magic assigns to one of its five level painter slots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LevelLayerSlotSource {
    Layer2,
    Layer1,
    Layer3,
}

/// Which priority half of the Layer 3 tilemap a painter slot consumes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layer3PrioritySelection {
    Both,
    Low,
    High,
}

/// One entry in Lunar Magic's five-slot level-layer painter dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelLayerPainterSlot {
    pub source: Option<LevelLayerSlotSource>,
    pub enabled: bool,
    pub additive: bool,
    pub half_color: bool,
    pub layer3_priority: Layer3PrioritySelection,
}

impl LevelLayerPainterSlot {
    const EMPTY: Self = Self {
        source: None,
        enabled: false,
        additive: false,
        half_color: false,
        layer3_priority: Layer3PrioritySelection::Both,
    };
}

/// Lunar Magic 3.63's exact five level-layer painter slots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelLayerSlotAssignments {
    pub slots: [LevelLayerPainterSlot; 5],
}

// Runtime bytes captured from Lunar Magic 3.63 at $0091F330/$0091F350/$0091F370 after opening
// an authentic SMW-US ROM. LoadLevelModeConfiguration indexes these tables by the five-bit mode.
const PRIMARY_SOURCE_MASKS: [u8; 32] = [
    0x15, 0x15, 0x17, 0x15, 0x15, 0x15, 0x17, 0x15, 0x17, 0x15, 0x15, 0x15, 0x15, 0x15, 0x04, 0x04,
    0x15, 0x17, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x01, 0x02,
];
const ALTERNATE_SOURCE_MASKS: [u8; 32] = [
    0x02, 0x02, 0x00, 0x02, 0x02, 0x02, 0x00, 0x02, 0x00, 0x00, 0x02, 0x00, 0x02, 0x02, 0x13, 0x13,
    0x00, 0x00, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x16, 0x15,
];
const COMPOSITION_MASKS: [u8; 32] = [
    0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x20, 0x24, 0x24, 0x20, 0x24, 0x20, 0x70, 0x70, 0x24, 0x24,
    0x20, 0xff, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x21, 0x22,
];

/// Replays `ConfigureLevelLayerSlotAssignments @ 004692B0` for an authentic level mode.
///
/// Modes `$12..=$1D` are rejected because Lunar Magic itself replaces them with mode `$00` before
/// indexing these tables. An enabled expanded-settings record applies packed bits 30 and 31 before
/// the slot dispatcher runs. `split_layer3_priority` is legacy-header byte 2 bit 7.
#[must_use]
pub fn lunar_magic_level_layer_slots(
    level_mode: u8,
    split_layer3_priority: bool,
    expanded_mode: Option<Layer3ExpandedModeFlags>,
) -> Option<LevelLayerSlotAssignments> {
    if level_mode >= 32 || (0x12..=0x1d).contains(&level_mode) {
        return None;
    }
    let index = usize::from(level_mode);
    let mut primary = PRIMARY_SOURCE_MASKS[index];
    let mut alternate = ALTERNATE_SOURCE_MASKS[index];
    let mut composition = COMPOSITION_MASKS[index];
    if let Some(expanded) = expanded_mode.filter(|flags| flags.enabled()) {
        if expanded.packed() & 0x8000_0000 != 0 {
            primary &= !4;
            alternate |= 4;
        }
        if expanded.packed() & 0x4000_0000 == 0 {
            composition &= !4;
        } else {
            composition |= 4;
        }
    }

    let mut slots = [LevelLayerPainterSlot::EMPTY; 5];
    let (layer1_slot, layer2_slot) = if primary & 1 == 0 && primary & 2 != 0 {
        (1, 3)
    } else {
        (3, 1)
    };
    let primary_layer3 = primary & 4 != 0;

    let layer3_slot = if primary_layer3 && primary & 3 == 0 {
        4
    } else if !split_layer3_priority {
        if primary_layer3 && alternate & 3 != 0 && primary & 3 != 3 {
            2
        } else {
            0
        }
    } else if primary_layer3 {
        let slot = if matches!(alternate & 3, 1 | 2) { 2 } else { 0 };
        slots[slot] = layer3_slot_state(composition, false, Layer3PrioritySelection::Low);
        slots[4] = LevelLayerPainterSlot {
            source: Some(LevelLayerSlotSource::Layer3),
            enabled: false,
            additive: false,
            half_color: false,
            layer3_priority: Layer3PrioritySelection::High,
        };
        4
    } else if alternate & 4 == 0 {
        2
    } else if alternate & 3 != 0 && primary & 3 != 3 {
        slots[0] = LevelLayerPainterSlot {
            source: Some(LevelLayerSlotSource::Layer3),
            enabled: true,
            additive: false,
            half_color: composition & 0x60 == 0x60,
            layer3_priority: Layer3PrioritySelection::Low,
        };
        slots[2] = LevelLayerPainterSlot {
            source: Some(LevelLayerSlotSource::Layer3),
            enabled: false,
            additive: false,
            half_color: false,
            layer3_priority: Layer3PrioritySelection::High,
        };
        2
    } else {
        0
    };

    slots[layer1_slot].source = Some(LevelLayerSlotSource::Layer1);
    slots[layer2_slot].source = Some(LevelLayerSlotSource::Layer2);
    slots[layer3_slot].source = Some(LevelLayerSlotSource::Layer3);
    slots[layer1_slot].enabled = primary & 1 != 0 || alternate & 1 != 0;
    slots[layer2_slot].enabled = primary & 2 != 0 || alternate & 2 != 0;
    slots[layer3_slot].enabled = primary_layer3 || alternate & 4 != 0;
    slots[layer1_slot].additive =
        composition & 1 != 0 && primary & 1 != 0 && composition & 0x80 == 0;
    slots[layer2_slot].additive =
        composition & 2 != 0 && primary & 2 != 0 && composition & 0x80 == 0;
    slots[layer3_slot].additive = composition & 4 != 0 && primary_layer3 && composition & 0x80 == 0;
    if slots[layer3_slot].additive {
        slots[layer3_slot].half_color = composition & 0x44 == 0x44;
    } else if alternate & 4 != 0 && !primary_layer3 {
        slots[layer3_slot].half_color = composition & 0x60 == 0x60;
    }

    Some(LevelLayerSlotAssignments { slots })
}

const fn layer3_slot_state(
    composition: u8,
    alternate: bool,
    layer3_priority: Layer3PrioritySelection,
) -> LevelLayerPainterSlot {
    let additive = !alternate && composition & 4 != 0 && composition & 0x80 == 0;
    LevelLayerPainterSlot {
        source: Some(LevelLayerSlotSource::Layer3),
        enabled: true,
        additive,
        half_color: if alternate {
            composition & 0x60 == 0x60
        } else {
            additive && composition & 0x44 == 0x44
        },
        layer3_priority,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_five_bytes(hex: &str) -> [u8; 5] {
        assert_eq!(hex.len(), 10);
        let mut bytes = [0; 5];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
        }
        bytes
    }

    #[test]
    fn mode_zero_matches_live_lunar_magic_slot_capture() {
        let assignments = lunar_magic_level_layer_slots(0, false, None).unwrap();
        assert_eq!(
            assignments.slots.map(|slot| slot.source),
            [
                None,
                Some(LevelLayerSlotSource::Layer2),
                Some(LevelLayerSlotSource::Layer3),
                Some(LevelLayerSlotSource::Layer1),
                None,
            ]
        );
        assert_eq!(
            assignments.slots.map(|slot| slot.enabled),
            [false, true, true, true, false]
        );
        assert_eq!(
            assignments.slots.map(|slot| slot.additive),
            [false, false, true, false, false]
        );
    }

    #[test]
    fn expanded_route_and_priority_split_can_assign_two_layer3_slots() {
        let assignments = lunar_magic_level_layer_slots(
            0,
            true,
            Some(Layer3ExpandedModeFlags::from_packed(0xc000_0001)),
        )
        .unwrap();
        assert_eq!(
            assignments.slots[0].source,
            Some(LevelLayerSlotSource::Layer3)
        );
        assert_eq!(
            assignments.slots[0].layer3_priority,
            Layer3PrioritySelection::Low
        );
        assert!(assignments.slots[0].enabled);
        assert_eq!(
            assignments.slots[2].source,
            Some(LevelLayerSlotSource::Layer3)
        );
        assert_eq!(
            assignments.slots[2].layer3_priority,
            Layer3PrioritySelection::High
        );
        assert!(assignments.slots[2].enabled);
        assert!(!assignments.slots[0].additive);
        assert!(!assignments.slots[2].additive);
    }

    #[test]
    fn alternate_route_uses_slot_zero_and_mode_table_half_color() {
        let assignments = lunar_magic_level_layer_slots(
            0x0c,
            false,
            Some(Layer3ExpandedModeFlags::from_packed(0xc000_0001)),
        )
        .unwrap();
        let layer3 = assignments.slots[0];
        assert_eq!(layer3.source, Some(LevelLayerSlotSource::Layer3));
        assert!(layer3.enabled);
        assert!(!layer3.additive);
        assert!(layer3.half_color);
        assert_eq!(layer3.layer3_priority, Layer3PrioritySelection::Both);
    }

    #[test]
    fn special_modes_assign_addition_to_layer_one_or_layer_two() {
        let mode_1e = lunar_magic_level_layer_slots(0x1e, false, None).unwrap();
        let layer1 = mode_1e
            .slots
            .into_iter()
            .find(|slot| slot.source == Some(LevelLayerSlotSource::Layer1))
            .unwrap();
        let layer2 = mode_1e
            .slots
            .into_iter()
            .find(|slot| slot.source == Some(LevelLayerSlotSource::Layer2))
            .unwrap();
        assert!(layer1.additive);
        assert!(!layer2.additive);

        let mode_1f = lunar_magic_level_layer_slots(0x1f, false, None).unwrap();
        let layer1 = mode_1f
            .slots
            .into_iter()
            .find(|slot| slot.source == Some(LevelLayerSlotSource::Layer1))
            .unwrap();
        let layer2 = mode_1f
            .slots
            .into_iter()
            .find(|slot| slot.source == Some(LevelLayerSlotSource::Layer2))
            .unwrap();
        assert!(!layer1.additive);
        assert!(layer2.additive);
    }

    #[test]
    fn invalid_editor_modes_are_not_silently_indexed() {
        for mode in 0x12..=0x1d {
            assert_eq!(lunar_magic_level_layer_slots(mode, false, None), None);
        }
        assert!(lunar_magic_level_layer_slots(0x1e, false, None).is_some());
        assert!(lunar_magic_level_layer_slots(0x1f, false, None).is_some());
    }

    #[test]
    fn every_valid_mode_and_expanded_route_matches_the_retained_live_slot_arrays() {
        let fixture = include_str!(
            "../../../docs/oracle-work/lm363/pristine-us/level-layer-slots/slot-arrays.tsv"
        );
        let mut cases = 0;
        for line in fixture.lines().skip(1) {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(fields.len(), 13, "malformed retained row: {line}");
            let mode = u8::from_str_radix(fields[0], 16).unwrap();
            let split = match fields[1] {
                "0" => false,
                "1" => true,
                value => panic!("invalid split value {value}"),
            };
            let expanded_mode = match fields[2] {
                "0" => None,
                "1" => {
                    let route = u32::from(fields[3] == "1");
                    let additive = u32::from(fields[4] == "1");
                    Some(Layer3ExpandedModeFlags::from_packed(
                        1 | route << 31 | additive << 30,
                    ))
                }
                value => panic!("invalid expanded value {value}"),
            };
            let source = decode_five_bytes(fields[8]);
            let enabled = decode_five_bytes(fields[9]);
            let additive = decode_five_bytes(fields[10]);
            let half_color = decode_five_bytes(fields[11]);
            let priority = decode_five_bytes(fields[12]);
            let expected = std::array::from_fn(|index| LevelLayerPainterSlot {
                source: match source[index] {
                    0xff => None,
                    0 => Some(LevelLayerSlotSource::Layer2),
                    1 => Some(LevelLayerSlotSource::Layer1),
                    2 => Some(LevelLayerSlotSource::Layer3),
                    value => panic!("invalid retained source {value}"),
                },
                enabled: enabled[index] != 0,
                additive: additive[index] != 0,
                half_color: half_color[index] != 0,
                layer3_priority: match priority[index] {
                    0 => Layer3PrioritySelection::Both,
                    1 => Layer3PrioritySelection::Low,
                    2 => Layer3PrioritySelection::High,
                    value => panic!("invalid retained priority {value}"),
                },
            });
            assert_eq!(
                lunar_magic_level_layer_slots(mode, split, expanded_mode)
                    .unwrap()
                    .slots,
                expected,
                "mode {mode:02X} split {split}"
            );
            cases += 1;
        }
        assert_eq!(cases, 200);
    }
}
