use crate::ExAnimationRecord;

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
}
