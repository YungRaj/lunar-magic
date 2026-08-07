use crate::{
    Bgr555, ExAnimationRecord, IndexedTile, MaterializedPaletteOverride, MaterializedTileOverride,
};
use std::fmt;

const GRAPHICS_TRANSFER_BYTES: [u16; 19] = [
    0, 0x20, 0x40, 0x60, 0x80, 0xa0, 0xc0, 0xe0, 0x100, 0x180, 0x200, 0x280, 0x300, 0x380, 0x400,
    0x10, 0x20, 0x40, 0x80,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExAnimationTriggerPreviewState {
    pub blue_pow: bool,
    pub silver_pow: bool,
    pub on_off_switch_on: bool,
    pub have_star: bool,
    pub time_100: bool,
    pub five_yoshi_coins: bool,
    pub custom: [bool; 16],
    pub one_shot: [bool; 32],
    pub manual_frames: [u8; 16],
}

impl Default for ExAnimationTriggerPreviewState {
    fn default() -> Self {
        Self {
            blue_pow: false,
            silver_pow: false,
            on_off_switch_on: true,
            have_star: false,
            time_100: false,
            five_yoshi_coins: false,
            custom: [false; 16],
            one_shot: [false; 32],
            manual_frames: [0; 16],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectedExAnimationFrame {
    pub record: usize,
    pub frame: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationMaterializeError {
    UnsupportedGraphicsKind(u8),
    FrameOutOfRange { frame: u16, words: usize },
    SourceOutOfRange { index: usize, len: usize },
    DestinationOverflow,
    InvalidColor(u16),
    PaletteSourceOutOfRange { index: usize, len: usize },
    PaletteDestinationOutOfRange { index: usize, len: usize },
    UnsupportedPaletteKind(u8),
    InvalidGraphicsSource(u16),
    InvalidGraphicsDestination(u16),
    RelativeGraphicsSourceOutOfRange { source: u16, limit_bytes: u32 },
}

impl fmt::Display for ExAnimationMaterializeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot materialize ExAnimation frame: {self:?}")
    }
}

impl std::error::Error for ExAnimationMaterializeError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationPaletteTransfer {
    Palette(Vec<MaterializedPaletteOverride>),
    FixedColor(Bgr555),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExAnimationGraphicsAddressContext {
    pub two_bpp_enabled: bool,
    pub relative_source_base_tile: u32,
    pub relative_source_limit_bytes: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedExAnimationGraphicsAddress {
    pub source_tile: u32,
    pub destination_tile: u32,
    pub two_bpp_destination: bool,
}

#[must_use]
pub const fn exanimation_trigger_has_second_bank(trigger: u8) -> bool {
    matches!(trigger, 1..=5 | 7 | 9..=0x0e | 0x20..=0x2f)
}

pub fn exanimation_frame_source_word(
    record: &ExAnimationRecord,
    frame: u16,
) -> Result<u16, ExAnimationMaterializeError> {
    let bytes = record.frame_bytes(exanimation_trigger_has_second_bank(record.trigger()));
    let offset =
        usize::from(frame)
            .checked_mul(2)
            .ok_or(ExAnimationMaterializeError::FrameOutOfRange {
                frame,
                words: bytes.len() / 2,
            })?;
    let word =
        bytes
            .get(offset..offset + 2)
            .ok_or(ExAnimationMaterializeError::FrameOutOfRange {
                frame,
                words: bytes.len() / 2,
            })?;
    Ok(u16::from_le_bytes([word[0], word[1]]))
}

pub fn resolve_exanimation_graphics_address(
    record: &ExAnimationRecord,
    frame: u16,
    context: ExAnimationGraphicsAddressContext,
) -> Result<ResolvedExAnimationGraphicsAddress, ExAnimationMaterializeError> {
    let source_word = exanimation_frame_source_word(record, frame)?;
    let source_tile = if record.destination_flag() {
        let source_bytes = u32::from(source_word);
        let transfer_bytes = u32::from(
            GRAPHICS_TRANSFER_BYTES
                .get(usize::from(record.kind()))
                .copied()
                .filter(|bytes| *bytes != 0)
                .ok_or(ExAnimationMaterializeError::UnsupportedGraphicsKind(
                    record.kind(),
                ))?,
        );
        if source_bytes
            .checked_add(transfer_bytes)
            .is_none_or(|end| end > context.relative_source_limit_bytes)
        {
            return Err(
                ExAnimationMaterializeError::RelativeGraphicsSourceOutOfRange {
                    source: source_word,
                    limit_bytes: context.relative_source_limit_bytes,
                },
            );
        }
        context
            .relative_source_base_tile
            .checked_add(u32::from(source_word >> 5))
            .ok_or(ExAnimationMaterializeError::InvalidGraphicsSource(
                source_word,
            ))?
    } else if source_word >= 0x7d00 {
        u32::from((source_word - 0x7d00) >> 5) + 0x600
    } else if source_word >= 0x2000 {
        u32::from((source_word - 0x2000) >> 5) + 0x900
    } else {
        return Err(ExAnimationMaterializeError::InvalidGraphicsSource(
            source_word,
        ));
    };

    let mut destination = u32::from(record.destination());
    let mut two_bpp_destination = false;
    if context.two_bpp_enabled && destination < 0x4000 {
        destination *= 2;
        if destination >= 0x4000 {
            return Err(ExAnimationMaterializeError::InvalidGraphicsDestination(
                record.destination(),
            ));
        }
        two_bpp_destination = true;
    }
    let destination_tile = if destination < 0x4000 {
        destination >> 4
    } else if destination < 0x6000 {
        two_bpp_destination = true;
        ((destination - 0x4000) >> 3) + 0x1c00
    } else {
        ((destination - 0x6000) >> 4) + 0x400
    };
    Ok(ResolvedExAnimationGraphicsAddress {
        source_tile,
        destination_tile,
        two_bpp_destination,
    })
}

pub fn materialize_exanimation_graphics_transfer(
    record: &ExAnimationRecord,
    frame: u16,
    source_tiles: &[IndexedTile],
    source_tile: usize,
    destination_tile: u32,
    two_bpp_destination: bool,
) -> Result<Vec<MaterializedTileOverride>, ExAnimationMaterializeError> {
    let kind = record.kind();
    let bytes = GRAPHICS_TRANSFER_BYTES
        .get(usize::from(kind))
        .copied()
        .filter(|bytes| *bytes != 0)
        .ok_or(ExAnimationMaterializeError::UnsupportedGraphicsKind(kind))?;
    let _ = exanimation_frame_source_word(record, frame)?;
    let tile_count = usize::from(if two_bpp_destination {
        bytes >> 4
    } else {
        bytes >> 5
    });
    if tile_count == 0 {
        return Err(ExAnimationMaterializeError::UnsupportedGraphicsKind(kind));
    }

    let second_block = two_bpp_destination && (0x10..=0x12).contains(&kind);
    let source_count = if two_bpp_destination {
        (tile_count + usize::from(second_block) * tile_count).div_ceil(2)
    } else {
        tile_count
    };
    let source_end = source_tile.checked_add(source_count).ok_or(
        ExAnimationMaterializeError::SourceOutOfRange {
            index: usize::MAX,
            len: source_tiles.len(),
        },
    )?;
    if source_end > source_tiles.len() {
        return Err(ExAnimationMaterializeError::SourceOutOfRange {
            index: source_end - 1,
            len: source_tiles.len(),
        });
    }

    let mut overrides = Vec::with_capacity(tile_count * (1 + usize::from(second_block)));
    append_graphics_block(
        &mut overrides,
        source_tiles,
        source_tile,
        destination_tile,
        tile_count,
        two_bpp_destination,
    )?;
    if second_block {
        append_graphics_block(
            &mut overrides,
            source_tiles,
            source_tile + tile_count.div_ceil(2),
            destination_tile
                .checked_add(0x10)
                .ok_or(ExAnimationMaterializeError::DestinationOverflow)?,
            tile_count,
            true,
        )?;
    }
    Ok(overrides)
}

pub fn materialize_exanimation_palette_transfer(
    record: &ExAnimationRecord,
    frame: u16,
    palette: &[Bgr555],
    source_color: usize,
    alternate_bank: bool,
) -> Result<ExAnimationPaletteTransfer, ExAnimationMaterializeError> {
    let kind = record.kind();
    if !(0x13..=0x1b).contains(&kind) {
        return Err(ExAnimationMaterializeError::UnsupportedPaletteKind(kind));
    }
    let destination = record.destination();
    let first = usize::from(destination & 0xff);
    let count = usize::from(destination >> 8) + 1;

    if (0x16..=0x17).contains(&kind) {
        return Ok(ExAnimationPaletteTransfer::FixedColor(valid_color(
            exanimation_frame_source_word(record, frame)?,
        )?));
    }

    let end = first.checked_add(count).ok_or(
        ExAnimationMaterializeError::PaletteDestinationOutOfRange {
            index: usize::MAX,
            len: palette.len(),
        },
    )?;
    if end > palette.len() {
        return Err(ExAnimationMaterializeError::PaletteDestinationOutOfRange {
            index: end - 1,
            len: palette.len(),
        });
    }

    let colors = if kind < 0x16 {
        if count == 1 {
            vec![valid_color(exanimation_frame_source_word(record, frame)?)?]
        } else {
            let source_end = source_color.checked_add(count).ok_or(
                ExAnimationMaterializeError::PaletteSourceOutOfRange {
                    index: usize::MAX,
                    len: palette.len(),
                },
            )?;
            if source_end > palette.len() {
                return Err(ExAnimationMaterializeError::PaletteSourceOutOfRange {
                    index: source_end - 1,
                    len: palette.len(),
                });
            }
            palette[source_color..source_end].to_vec()
        }
    } else {
        let mut colors = palette[first..end].to_vec();
        if count > 1 {
            let rotate_right = matches!(kind, 0x18)
                || (kind == 0x19 && !alternate_bank)
                || (kind == 0x1b && alternate_bank);
            if rotate_right {
                colors.rotate_right(1);
            } else {
                colors.rotate_left(1);
            }
        }
        colors
    };
    Ok(ExAnimationPaletteTransfer::Palette(
        colors
            .into_iter()
            .enumerate()
            .map(|(offset, color)| MaterializedPaletteOverride {
                color_index: u32::try_from(first + offset)
                    .expect("a validated palette slice index fits u32"),
                color,
            })
            .collect(),
    ))
}

fn valid_color(word: u16) -> Result<Bgr555, ExAnimationMaterializeError> {
    if word & 0x8000 != 0 {
        Err(ExAnimationMaterializeError::InvalidColor(word))
    } else {
        Ok(Bgr555(word))
    }
}

fn append_graphics_block(
    output: &mut Vec<MaterializedTileOverride>,
    source_tiles: &[IndexedTile],
    source_tile: usize,
    destination_tile: u32,
    tile_count: usize,
    two_bpp: bool,
) -> Result<(), ExAnimationMaterializeError> {
    for index in 0..tile_count {
        let source_index = source_tile + if two_bpp { index / 2 } else { index };
        let mut tile = source_tiles[source_index].clone();
        if two_bpp {
            let shift = (index & 1) * 2;
            let pixels = std::array::from_fn(|pixel| (tile.pixels()[pixel] >> shift) & 3);
            tile = IndexedTile::new(pixels);
        }
        output.push(MaterializedTileOverride {
            tile_index: destination_tile
                .checked_add(
                    u32::try_from(index)
                        .map_err(|_| ExAnimationMaterializeError::DestinationOverflow)?,
                )
                .ok_or(ExAnimationMaterializeError::DestinationOverflow)?,
            tile,
        });
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExAnimationPreviewState {
    cursors: Vec<u8>,
}

impl ExAnimationPreviewState {
    #[must_use]
    pub fn new(record_count: usize) -> Self {
        Self {
            cursors: vec![0xff; record_count],
        }
    }

    pub fn reset(&mut self, record_count: usize) {
        self.cursors.clear();
        self.cursors.resize(record_count, 0xff);
    }

    #[must_use]
    pub fn cursors(&self) -> &[u8] {
        &self.cursors
    }

    pub fn process_phase(
        &mut self,
        records: &[ExAnimationRecord],
        phase: u8,
        advance: bool,
        triggers: &mut ExAnimationTriggerPreviewState,
    ) -> Vec<SelectedExAnimationFrame> {
        if self.cursors.len() != records.len() {
            self.reset(records.len());
        }
        let mut selected = Vec::new();
        let mut record_index = usize::from(phase & 7);
        while let Some(record) = records.get(record_index) {
            if record.kind() != 0
                && let Some(frame) =
                    select_frame(record, record_index, &mut self.cursors, advance, triggers)
            {
                selected.push(SelectedExAnimationFrame {
                    record: record_index,
                    frame,
                });
            }
            record_index += 8;
        }
        selected
    }
}

fn select_frame(
    record: &ExAnimationRecord,
    record_index: usize,
    cursors: &mut [u8],
    advance: bool,
    triggers: &mut ExAnimationTriggerPreviewState,
) -> Option<u16> {
    let trigger = record.trigger();
    let maximum = record.frame_count_minus_one();
    let cursor_index = if trigger == 0x0f {
        record_index & !7
    } else {
        record_index
    };
    let cursor = cursors.get_mut(cursor_index)?;
    let mut alternate_bank = false;

    match trigger {
        0 => {}
        1 => alternate_bank = triggers.blue_pow,
        2 => alternate_bank = triggers.silver_pow,
        3 => alternate_bank = !triggers.on_off_switch_on,
        4 => alternate_bank = triggers.have_star,
        5 => alternate_bank = triggers.time_100,
        6 => {
            if !one_shot_condition(*cursor, maximum, advance, triggers.time_100) {
                return None;
            }
        }
        7 => alternate_bank = triggers.five_yoshi_coins,
        8 => {
            if !one_shot_condition(*cursor, maximum, advance, triggers.five_yoshi_coins) {
                return None;
            }
        }
        9..=0x0f => alternate_bank = true,
        0x10..=0x1f => {
            let target = triggers.manual_frames[usize::from(trigger - 0x10)];
            if *cursor == target {
                return None;
            }
            *cursor = if advance {
                target.wrapping_sub(1)
            } else {
                target
            };
        }
        0x20..=0x2f => {
            alternate_bank = triggers.custom[usize::from(trigger - 0x20)];
        }
        0x30..=0x4f => {
            let one_shot = &mut triggers.one_shot[usize::from(trigger - 0x30)];
            if !(advance || *cursor != 0xff) || !*one_shot {
                return None;
            }
            if *cursor >= maximum && *cursor != 0xff {
                *cursor = 0xff;
                *one_shot = false;
                return None;
            }
        }
        _ => {}
    }

    if advance {
        if (0x18..=0x1b).contains(&record.kind()) {
            *cursor = cursor.wrapping_add(1);
            if maximum <= *cursor {
                *cursor = 0xff;
            }
        } else if *cursor < maximum {
            *cursor += 1;
        } else {
            *cursor = 0;
        }
    }

    let frame = u16::from(*cursor)
        + if alternate_bank {
            u16::from(maximum) + 1
        } else {
            0
        };
    Some(frame)
}

fn one_shot_condition(cursor: u8, maximum: u8, advance: bool, enabled: bool) -> bool {
    if !advance && cursor == 0xff {
        return false;
    }
    enabled && (cursor < maximum || cursor == 0xff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(kind: u8, maximum: u8, trigger: u8) -> ExAnimationRecord {
        let banks = usize::from(matches!(trigger, 1..=5 | 7 | 9..=0x0f | 0x20..=0x2f)) + 1;
        ExAnimationRecord::new(
            kind,
            maximum,
            trigger,
            0,
            false,
            &vec![0; (usize::from(maximum) + 1) * banks * 2],
            banks == 2,
        )
        .unwrap()
    }

    #[test]
    fn reset_and_interleaved_phase_progression_match_native_cursors() {
        let records = vec![record(1, 2, 0); 10];
        let mut state = ExAnimationPreviewState::new(records.len());
        let mut triggers = ExAnimationTriggerPreviewState::default();
        assert_eq!(state.cursors(), &[0xff; 10]);
        assert_eq!(
            state.process_phase(&records, 1, true, &mut triggers),
            vec![
                SelectedExAnimationFrame {
                    record: 1,
                    frame: 0
                },
                SelectedExAnimationFrame {
                    record: 9,
                    frame: 0
                },
            ]
        );
        assert_eq!(state.cursors()[0], 0xff);
        assert_eq!(state.cursors()[1], 0);
        assert_eq!(
            state.process_phase(&records, 1, true, &mut triggers)[0].frame,
            1
        );
    }

    #[test]
    fn conditional_and_custom_triggers_select_the_second_frame_bank() {
        let records = vec![record(1, 1, 4), record(1, 1, 0x2a)];
        let mut state = ExAnimationPreviewState::new(2);
        let mut triggers = ExAnimationTriggerPreviewState::default();
        triggers.have_star = true;
        triggers.custom[0x0a] = true;
        assert_eq!(
            state.process_phase(&records, 0, true, &mut triggers)[0].frame,
            2
        );
        assert_eq!(
            state.process_phase(&records, 1, true, &mut triggers)[0].frame,
            2
        );
    }

    #[test]
    fn manual_triggers_force_the_selected_byte_with_wrapping_values() {
        let records = vec![record(1, 7, 0x13)];
        let mut state = ExAnimationPreviewState::new(1);
        let mut triggers = ExAnimationTriggerPreviewState::default();
        triggers.manual_frames[3] = 6;
        assert_eq!(
            state.process_phase(&records, 0, true, &mut triggers)[0].frame,
            6
        );
        assert!(
            state
                .process_phase(&records, 0, false, &mut triggers)
                .is_empty()
        );
    }

    #[test]
    fn one_shot_triggers_run_once_and_clear_after_the_last_frame() {
        let records = vec![record(1, 1, 0x30)];
        let mut state = ExAnimationPreviewState::new(1);
        let mut triggers = ExAnimationTriggerPreviewState::default();
        triggers.one_shot[0] = true;
        assert_eq!(
            state.process_phase(&records, 0, true, &mut triggers)[0].frame,
            0
        );
        assert_eq!(
            state.process_phase(&records, 0, true, &mut triggers)[0].frame,
            1
        );
        assert!(
            state
                .process_phase(&records, 0, true, &mut triggers)
                .is_empty()
        );
        assert!(!triggers.one_shot[0]);
        assert_eq!(state.cursors(), &[0xff]);
    }

    #[test]
    fn complete_graphics_types_use_the_authenticated_transfer_sizes() {
        let record = ExAnimationRecord::new(3, 0, 0, 0, false, &[0, 0], false).unwrap();
        let source = (0..3)
            .map(|value| IndexedTile::new([value; 64]))
            .collect::<Vec<_>>();
        let overrides =
            materialize_exanimation_graphics_transfer(&record, 0, &source, 0, 0x120, false)
                .unwrap();
        assert_eq!(overrides.len(), 3);
        assert_eq!(overrides[0].tile_index, 0x120);
        assert_eq!(overrides[2].tile_index, 0x122);
        assert_eq!(overrides[2].tile.pixels(), &[2; 64]);
    }

    #[test]
    fn two_bpp_types_split_nibbles_and_write_the_second_destination_block() {
        let record = ExAnimationRecord::new(0x10, 0, 0, 0, false, &[0, 0], false).unwrap();
        let source = vec![IndexedTile::new([0x0d; 64]), IndexedTile::new([0x06; 64])];
        let overrides =
            materialize_exanimation_graphics_transfer(&record, 0, &source, 0, 0x200, true).unwrap();
        assert_eq!(overrides.len(), 4);
        assert_eq!(overrides[0].tile_index, 0x200);
        assert_eq!(overrides[0].tile.pixels(), &[1; 64]);
        assert_eq!(overrides[1].tile_index, 0x201);
        assert_eq!(overrides[1].tile.pixels(), &[3; 64]);
        assert_eq!(overrides[2].tile_index, 0x210);
        assert_eq!(overrides[2].tile.pixels(), &[2; 64]);
        assert_eq!(overrides[3].tile_index, 0x211);
        assert_eq!(overrides[3].tile.pixels(), &[1; 64]);
    }

    #[test]
    fn frame_and_source_bounds_fail_before_returning_partial_overrides() {
        let record = ExAnimationRecord::new(1, 0, 0, 0, false, &[0, 0], false).unwrap();
        assert_eq!(
            exanimation_frame_source_word(&record, 1),
            Err(ExAnimationMaterializeError::FrameOutOfRange { frame: 1, words: 1 })
        );
        assert_eq!(
            materialize_exanimation_graphics_transfer(&record, 0, &[], 0, 0, false),
            Err(ExAnimationMaterializeError::SourceOutOfRange { index: 0, len: 0 })
        );
    }

    #[test]
    fn palette_copy_direct_color_and_fixed_color_remain_distinct() {
        let palette = (0..16).map(Bgr555).collect::<Vec<_>>();
        let direct = ExAnimationRecord::new(0x13, 0, 0, 3, false, &[0x1f, 0], false).unwrap();
        assert_eq!(
            materialize_exanimation_palette_transfer(&direct, 0, &palette, 0, false).unwrap(),
            ExAnimationPaletteTransfer::Palette(vec![MaterializedPaletteOverride {
                color_index: 3,
                color: Bgr555(0x001f),
            }])
        );

        let copied = ExAnimationRecord::new(0x15, 0, 0, 0x0204, false, &[0, 0], false).unwrap();
        let ExAnimationPaletteTransfer::Palette(copied) =
            materialize_exanimation_palette_transfer(&copied, 0, &palette, 8, false).unwrap()
        else {
            panic!("kind 15 must target CGRAM");
        };
        assert_eq!(
            copied.iter().map(|entry| entry.color).collect::<Vec<_>>(),
            [Bgr555(8), Bgr555(9), Bgr555(10)]
        );
        assert_eq!(copied[0].color_index, 4);

        let fixed = ExAnimationRecord::new(0x16, 0, 0, 0, false, &[0x10, 0x42], false).unwrap();
        assert_eq!(
            materialize_exanimation_palette_transfer(&fixed, 0, &palette, 0, false).unwrap(),
            ExAnimationPaletteTransfer::FixedColor(Bgr555(0x4210))
        );
    }

    #[test]
    fn palette_rotation_types_follow_normal_and_alternate_directions() {
        let palette = [Bgr555(1), Bgr555(2), Bgr555(3), Bgr555(4)];
        for (kind, alternate, expected) in [
            (0x18, false, [4, 1, 2, 3]),
            (0x19, false, [4, 1, 2, 3]),
            (0x19, true, [2, 3, 4, 1]),
            (0x1a, false, [2, 3, 4, 1]),
            (0x1b, false, [2, 3, 4, 1]),
            (0x1b, true, [4, 1, 2, 3]),
        ] {
            let record = ExAnimationRecord::new(kind, 0, 0, 0x0300, false, &[], false).unwrap();
            let ExAnimationPaletteTransfer::Palette(overrides) =
                materialize_exanimation_palette_transfer(&record, 0, &palette, 0, alternate)
                    .unwrap()
            else {
                panic!("rotation types must target CGRAM");
            };
            assert_eq!(
                overrides
                    .iter()
                    .map(|entry| entry.color.0)
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn palette_ranges_and_literal_words_are_preflighted() {
        let palette = [Bgr555(0); 4];
        let invalid_destination =
            ExAnimationRecord::new(0x13, 0, 0, 0x0302, false, &[0, 0], false).unwrap();
        assert_eq!(
            materialize_exanimation_palette_transfer(&invalid_destination, 0, &palette, 0, false),
            Err(ExAnimationMaterializeError::PaletteDestinationOutOfRange { index: 5, len: 4 })
        );
        let invalid_color =
            ExAnimationRecord::new(0x13, 0, 0, 0, false, &[0, 0x80], false).unwrap();
        assert_eq!(
            materialize_exanimation_palette_transfer(&invalid_color, 0, &palette, 0, false),
            Err(ExAnimationMaterializeError::InvalidColor(0x8000))
        );
    }

    #[test]
    fn graphics_addresses_cover_absolute_relative_and_two_bpp_regions() {
        let absolute =
            ExAnimationRecord::new(1, 0, 0, 0x1230, false, &[0x00, 0x20], false).unwrap();
        assert_eq!(
            resolve_exanimation_graphics_address(
                &absolute,
                0,
                ExAnimationGraphicsAddressContext {
                    two_bpp_enabled: false,
                    relative_source_base_tile: 0,
                    relative_source_limit_bytes: 0,
                }
            )
            .unwrap(),
            ResolvedExAnimationGraphicsAddress {
                source_tile: 0x900,
                destination_tile: 0x123,
                two_bpp_destination: false,
            }
        );

        let upper = ExAnimationRecord::new(1, 0, 0, 0x4008, false, &[0, 0x7d], false).unwrap();
        assert_eq!(
            resolve_exanimation_graphics_address(
                &upper,
                0,
                ExAnimationGraphicsAddressContext {
                    two_bpp_enabled: false,
                    relative_source_base_tile: 0,
                    relative_source_limit_bytes: 0,
                }
            )
            .unwrap(),
            ResolvedExAnimationGraphicsAddress {
                source_tile: 0x600,
                destination_tile: 0x1c01,
                two_bpp_destination: true,
            }
        );

        let relative = ExAnimationRecord::new(1, 0, 0, 0x0100, true, &[0x40, 0], false).unwrap();
        assert_eq!(
            resolve_exanimation_graphics_address(
                &relative,
                0,
                ExAnimationGraphicsAddressContext {
                    two_bpp_enabled: true,
                    relative_source_base_tile: 0x80,
                    relative_source_limit_bytes: 0x100,
                }
            )
            .unwrap(),
            ResolvedExAnimationGraphicsAddress {
                source_tile: 0x82,
                destination_tile: 0x20,
                two_bpp_destination: true,
            }
        );
    }

    #[test]
    fn graphics_address_validation_rejects_unmapped_and_oversized_ranges() {
        let unmapped = ExAnimationRecord::new(1, 0, 0, 0, false, &[0xff, 0x1f], false).unwrap();
        assert_eq!(
            resolve_exanimation_graphics_address(
                &unmapped,
                0,
                ExAnimationGraphicsAddressContext {
                    two_bpp_enabled: false,
                    relative_source_base_tile: 0,
                    relative_source_limit_bytes: 0,
                }
            ),
            Err(ExAnimationMaterializeError::InvalidGraphicsSource(0x1fff))
        );
        let relative = ExAnimationRecord::new(2, 0, 0, 0, true, &[0xe0, 0], false).unwrap();
        assert_eq!(
            resolve_exanimation_graphics_address(
                &relative,
                0,
                ExAnimationGraphicsAddressContext {
                    two_bpp_enabled: false,
                    relative_source_base_tile: 0,
                    relative_source_limit_bytes: 0x100,
                }
            ),
            Err(
                ExAnimationMaterializeError::RelativeGraphicsSourceOutOfRange {
                    source: 0x00e0,
                    limit_bytes: 0x100,
                }
            )
        );
    }
}
