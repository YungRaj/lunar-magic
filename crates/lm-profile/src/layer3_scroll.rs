//! Revision-specific semantic contracts for the installed Layer 3 scroll dispatch tables.

/// One calculation selected by a five-bit Layer 3 horizontal or vertical scroll index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layer3ScrollFormula {
    BaseOnly,
    BasePlusPosition,
    BasePlusPositionDiv2,
    BasePlusPositionDiv4,
    BasePlusPositionDiv8,
    BasePlusPositionDiv16,
    BasePlusPositionDiv32,
    BasePlusPositionDiv64,
    BasePlusPositionDiv5,
    DynamicHorizontal,
    DynamicVerticalAccumulator,
    DynamicVerticalCamera,
}

impl Layer3ScrollFormula {
    /// Evaluates formulas whose result depends only on a base and camera position.
    ///
    /// Dynamic formulas require additional SMW runtime state and therefore return `None` instead
    /// of silently approximating their behavior.
    #[must_use]
    pub const fn evaluate_simple(self, base: u16, position: u16) -> Option<u16> {
        let offset = match self {
            Self::BaseOnly => 0,
            Self::BasePlusPosition => position,
            Self::BasePlusPositionDiv2 => position >> 1,
            Self::BasePlusPositionDiv4 => position >> 2,
            Self::BasePlusPositionDiv8 => position >> 3,
            Self::BasePlusPositionDiv16 => position >> 4,
            Self::BasePlusPositionDiv32 => position >> 5,
            Self::BasePlusPositionDiv64 => position >> 6,
            Self::BasePlusPositionDiv5 => position / 5,
            Self::DynamicHorizontal
            | Self::DynamicVerticalAccumulator
            | Self::DynamicVerticalCamera => return None,
        };
        Some(base.wrapping_add(offset))
    }
}

const HORIZONTAL: [Layer3ScrollFormula; 32] = [
    Layer3ScrollFormula::BaseOnly,
    Layer3ScrollFormula::BasePlusPosition,
    Layer3ScrollFormula::BasePlusPositionDiv2,
    Layer3ScrollFormula::BasePlusPositionDiv4,
    Layer3ScrollFormula::BasePlusPositionDiv32,
    Layer3ScrollFormula::BasePlusPositionDiv5,
    Layer3ScrollFormula::DynamicHorizontal,
    Layer3ScrollFormula::DynamicHorizontal,
    Layer3ScrollFormula::DynamicHorizontal,
    Layer3ScrollFormula::DynamicHorizontal,
    Layer3ScrollFormula::DynamicHorizontal,
    Layer3ScrollFormula::DynamicHorizontal,
    Layer3ScrollFormula::DynamicHorizontal,
    Layer3ScrollFormula::DynamicHorizontal,
    Layer3ScrollFormula::DynamicHorizontal,
    Layer3ScrollFormula::DynamicHorizontal,
    Layer3ScrollFormula::DynamicHorizontal,
    Layer3ScrollFormula::DynamicHorizontal,
    Layer3ScrollFormula::BasePlusPosition,
    Layer3ScrollFormula::BasePlusPosition,
    Layer3ScrollFormula::BasePlusPosition,
    Layer3ScrollFormula::BasePlusPosition,
    Layer3ScrollFormula::BasePlusPosition,
    Layer3ScrollFormula::BasePlusPosition,
    Layer3ScrollFormula::BasePlusPositionDiv8,
    Layer3ScrollFormula::BasePlusPositionDiv16,
    Layer3ScrollFormula::BasePlusPositionDiv64,
    Layer3ScrollFormula::BasePlusPosition,
    Layer3ScrollFormula::BasePlusPosition,
    Layer3ScrollFormula::BasePlusPosition,
    Layer3ScrollFormula::BasePlusPosition,
    Layer3ScrollFormula::BasePlusPosition,
];

const VERTICAL: [Layer3ScrollFormula; 32] = [
    Layer3ScrollFormula::BaseOnly,
    Layer3ScrollFormula::DynamicVerticalCamera,
    Layer3ScrollFormula::BasePlusPosition,
    Layer3ScrollFormula::BasePlusPositionDiv2,
    Layer3ScrollFormula::BasePlusPositionDiv16,
    Layer3ScrollFormula::BasePlusPositionDiv5,
    Layer3ScrollFormula::DynamicVerticalAccumulator,
    Layer3ScrollFormula::DynamicVerticalAccumulator,
    Layer3ScrollFormula::DynamicVerticalAccumulator,
    Layer3ScrollFormula::DynamicVerticalAccumulator,
    Layer3ScrollFormula::DynamicVerticalAccumulator,
    Layer3ScrollFormula::DynamicVerticalAccumulator,
    Layer3ScrollFormula::DynamicVerticalAccumulator,
    Layer3ScrollFormula::DynamicVerticalAccumulator,
    Layer3ScrollFormula::DynamicVerticalAccumulator,
    Layer3ScrollFormula::DynamicVerticalAccumulator,
    Layer3ScrollFormula::DynamicVerticalAccumulator,
    Layer3ScrollFormula::DynamicVerticalAccumulator,
    Layer3ScrollFormula::DynamicVerticalCamera,
    Layer3ScrollFormula::DynamicVerticalCamera,
    Layer3ScrollFormula::DynamicVerticalCamera,
    Layer3ScrollFormula::DynamicVerticalCamera,
    Layer3ScrollFormula::DynamicVerticalCamera,
    Layer3ScrollFormula::DynamicVerticalCamera,
    Layer3ScrollFormula::BasePlusPositionDiv4,
    Layer3ScrollFormula::BasePlusPositionDiv8,
    Layer3ScrollFormula::BasePlusPositionDiv32,
    Layer3ScrollFormula::DynamicVerticalCamera,
    Layer3ScrollFormula::DynamicVerticalCamera,
    Layer3ScrollFormula::DynamicVerticalCamera,
    Layer3ScrollFormula::DynamicVerticalCamera,
    Layer3ScrollFormula::DynamicVerticalCamera,
];

/// Resolves the installed runtime's five-bit horizontal scroll selector.
#[must_use]
pub const fn smw_us_v1_layer3_horizontal_scroll(index: u8) -> Layer3ScrollFormula {
    HORIZONTAL[(index & 0x1f) as usize]
}

/// Resolves the installed runtime's five-bit vertical scroll selector.
#[must_use]
pub const fn smw_us_v1_layer3_vertical_scroll(index: u8) -> Layer3ScrollFormula {
    VERTICAL[(index & 0x1f) as usize]
}

/// Mutable SMW state consumed by the installed dynamic horizontal Layer 3 helper.
///
/// Names retain RAM addresses where the owning engine concept is not yet proven.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layer3DynamicHorizontalState {
    pub layer2_mode_1403: u8,
    pub engine_mode_009d: u8,
    pub signed_bias_17bd: u8,
    pub control_0be6: u16,
    pub phase_145c: u8,
    pub phase_delta_1458: u16,
    pub scroll_x_22: u16,
    pub scroll_y_24: u16,
    pub vertical_base_146c: u16,
    pub camera_x_1a: u16,
    pub state_005b: u8,
    pub state_005e: u8,
    pub scratch_26: u16,
    pub phase_high_17bf: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layer3DynamicHorizontalOutcome {
    HorizontalUpdated,
    VerticalBaseSelected,
}

/// Applies the recovered dynamic-horizontal helper exactly at its RAM-state boundary.
///
/// All arithmetic wraps at the width used by the 65C816 routine. A negative 8-bit X load from
/// `$0BE6` suppresses one phase advance and clears word bit `$8000`, while `$1403` selects the
/// secondary state path.
#[must_use]
pub fn smw_us_v1_step_dynamic_horizontal(
    state: &mut Layer3DynamicHorizontalState,
) -> Layer3DynamicHorizontalOutcome {
    if state.layer2_mode_1403 == 0 {
        if state.engine_mode_009d != 0 {
            state.scroll_y_24 = state.vertical_base_146c;
            return Layer3DynamicHorizontalOutcome::VerticalBaseSelected;
        }
        let signed_bias = sign_extend_u8(state.signed_bias_17bd);
        let phase_increment = if state.control_0be6.to_le_bytes()[0] & 0x80 != 0 {
            state.control_0be6 &= 0x7fff;
            0
        } else {
            advance_phase(&mut state.phase_145c, state.phase_delta_1458)
        };
        state.scroll_x_22 = state
            .scroll_x_22
            .wrapping_add(phase_increment)
            .wrapping_add(signed_bias);
        return Layer3DynamicHorizontalOutcome::HorizontalUpdated;
    }

    state.scratch_26 = 0;
    if state.state_005b & 1 == 0 && state.state_005e != 1 {
        state.scratch_26 = 0x8000_u16.wrapping_sub(state.camera_x_1a);
    }
    if state.engine_mode_009d != 0 {
        state.scroll_y_24 = state.vertical_base_146c;
        return Layer3DynamicHorizontalOutcome::VerticalBaseSelected;
    }

    let signed_bias = sign_extend_u8(state.signed_bias_17bd);
    let phase_increment = advance_phase(&mut state.phase_145c, state.phase_delta_1458);
    state.scroll_x_22 = state
        .scroll_x_22
        .wrapping_add(phase_increment)
        .wrapping_add(signed_bias);
    state.phase_high_17bf = u16::from(state.phase_145c)
        .wrapping_add(state.phase_delta_1458)
        .to_le_bytes()[1];
    Layer3DynamicHorizontalOutcome::HorizontalUpdated
}

/// Mutable SMW state shared by both installed dynamic vertical helpers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layer3DynamicVerticalState {
    pub layer2_mode_1403: u8,
    pub engine_mode_009d: u8,
    pub signed_bias_17bc: u8,
    pub control_0be7: u16,
    pub phase_145d: u8,
    pub phase_delta_145a: u16,
    pub scroll_y_24: u16,
    pub vertical_base_146c: u16,
    pub camera_y_1c: u16,
    pub control_190d: u16,
    pub state_005b: u8,
    pub state_005e: u8,
    pub state_13d7: u16,
    pub layer_flags_145e: u16,
    pub scratch_02: u16,
    pub scratch_28: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layer3DynamicVerticalOutcome {
    AccumulatorUpdated,
    CameraUpdated,
    Unchanged,
}

/// Applies the dynamic-accumulator vertical entry selected by installed table address `$9C21`.
#[must_use]
pub fn smw_us_v1_step_dynamic_vertical_accumulator(
    state: &mut Layer3DynamicVerticalState,
) -> Layer3DynamicVerticalOutcome {
    if state.layer2_mode_1403 != 0 {
        return step_dynamic_vertical_camera_layer2(state);
    }
    if state.engine_mode_009d != 0 {
        return Layer3DynamicVerticalOutcome::Unchanged;
    }
    let signed_bias = sign_extend_u8(state.signed_bias_17bc);
    let phase_increment = if state.control_0be7.to_le_bytes()[0] & 0x80 != 0 {
        state.control_0be7 &= 0x7fff;
        0
    } else {
        advance_phase(&mut state.phase_145d, state.phase_delta_145a)
    };
    state.scroll_y_24 = state
        .scroll_y_24
        .wrapping_add(phase_increment)
        .wrapping_add(signed_bias);
    Layer3DynamicVerticalOutcome::AccumulatorUpdated
}

/// Applies the camera/clamp vertical entry selected by installed table address `$9C59`.
#[must_use]
pub fn smw_us_v1_step_dynamic_vertical_camera(
    state: &mut Layer3DynamicVerticalState,
) -> Layer3DynamicVerticalOutcome {
    if state.layer2_mode_1403 == 0 {
        state.scroll_y_24 = state.vertical_base_146c.wrapping_add(state.camera_y_1c);
        return Layer3DynamicVerticalOutcome::CameraUpdated;
    }
    step_dynamic_vertical_camera_layer2(state)
}

fn step_dynamic_vertical_camera_layer2(
    state: &mut Layer3DynamicVerticalState,
) -> Layer3DynamicVerticalOutcome {
    if state.control_190d & 0x4000 != 0 {
        step_dynamic_vertical_camera_once(state);
        return Layer3DynamicVerticalOutcome::CameraUpdated;
    }

    step_dynamic_vertical_camera_once(state);
    let phase = state.phase_145d;
    let base = state.vertical_base_146c;
    let scroll = state.scroll_y_24;
    step_dynamic_vertical_camera_once(state);
    state.phase_145d = phase;
    state.vertical_base_146c = base;
    state.scroll_y_24 = scroll;
    Layer3DynamicVerticalOutcome::CameraUpdated
}

fn step_dynamic_vertical_camera_once(state: &mut Layer3DynamicVerticalState) {
    if state.engine_mode_009d == 0 {
        let increment = advance_phase(&mut state.phase_145d, state.phase_delta_145a);
        state.vertical_base_146c = state.vertical_base_146c.wrapping_add(increment);
    }

    let candidate = state.vertical_base_146c.wrapping_add(state.camera_y_1c);
    if candidate & 0x8000 != 0 {
        state.scroll_y_24 = 0;
        state.scratch_02 = candidate;
        if state.layer_flags_145e & 4 != 0 {
            state.scratch_28 = candidate.wrapping_sub(state.camera_y_1c);
        }
        return;
    }
    if candidate < 0x0118 {
        state.scroll_y_24 = candidate;
        state.scratch_28 = candidate.wrapping_sub(state.camera_y_1c);
        return;
    }

    state.scratch_02 = candidate;
    state.scroll_y_24 = (candidate & 0x000f) ^ 8;
    state.scroll_y_24 = state.scroll_y_24.wrapping_add(0x0108);
    let mut boundary = state.state_13d7;
    if state.state_005b & 1 != 0 {
        boundary = u16::from(state.state_005e);
    }
    boundary = boundary.wrapping_sub(0x0100);
    if boundary & 0x8000 != 0 || boundary < 0x0100 {
        return;
    }
    let clamped = boundary.min(state.scratch_02);
    state.scratch_28 = clamped.wrapping_sub(state.camera_y_1c);
}

fn sign_extend_u8(value: u8) -> u16 {
    if value & 0x80 == 0 {
        u16::from(value)
    } else {
        0xff00 | u16::from(value)
    }
}

fn advance_phase(phase: &mut u8, delta: u16) -> u16 {
    let sum = u16::from(*phase).wrapping_add(delta);
    *phase = sum.to_le_bytes()[0];
    sign_extend_u8(sum.to_le_bytes()[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn simple_formulas_wrap_like_the_65c816() {
        assert_eq!(
            Layer3ScrollFormula::BasePlusPosition.evaluate_simple(0xfff0, 0x20),
            Some(0x10)
        );
        assert_eq!(
            Layer3ScrollFormula::BasePlusPositionDiv8.evaluate_simple(0x100, 0x80),
            Some(0x110)
        );
        assert_eq!(
            Layer3ScrollFormula::BasePlusPositionDiv5.evaluate_simple(0x20, 25),
            Some(0x25)
        );
        assert_eq!(
            Layer3ScrollFormula::DynamicHorizontal.evaluate_simple(0, 0),
            None
        );
    }

    #[test]
    fn semantic_tables_match_every_retained_runtime_dispatch_entry() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rom = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let payload = &rom[0x81a0d + 0x200..0x81a0d + 0x200 + 0x4c0];
        let horizontal_addresses = [
            0x9b16, 0x9b1c, 0x9b25, 0x9b2f, 0x9b34, 0x9b3a, 0x9b41, 0x9b49, 0x9b5d, 0x9cf8,
        ];
        let horizontal_formulas = [
            Layer3ScrollFormula::BaseOnly,
            Layer3ScrollFormula::BasePlusPosition,
            Layer3ScrollFormula::BasePlusPositionDiv2,
            Layer3ScrollFormula::BasePlusPositionDiv4,
            Layer3ScrollFormula::BasePlusPositionDiv8,
            Layer3ScrollFormula::BasePlusPositionDiv16,
            Layer3ScrollFormula::BasePlusPositionDiv32,
            Layer3ScrollFormula::BasePlusPositionDiv64,
            Layer3ScrollFormula::DynamicHorizontal,
            Layer3ScrollFormula::BasePlusPositionDiv5,
        ];
        let vertical_addresses = [
            0x9be3, 0x9be9, 0x9bf3, 0x9bf8, 0x9bfe, 0x9c05, 0x9c0d, 0x9c21, 0x9c59, 0x9d2e,
        ];
        let vertical_formulas = [
            Layer3ScrollFormula::BaseOnly,
            Layer3ScrollFormula::BasePlusPosition,
            Layer3ScrollFormula::BasePlusPositionDiv2,
            Layer3ScrollFormula::BasePlusPositionDiv4,
            Layer3ScrollFormula::BasePlusPositionDiv8,
            Layer3ScrollFormula::BasePlusPositionDiv16,
            Layer3ScrollFormula::BasePlusPositionDiv32,
            Layer3ScrollFormula::DynamicVerticalAccumulator,
            Layer3ScrollFormula::DynamicVerticalCamera,
            Layer3ScrollFormula::BasePlusPositionDiv5,
        ];

        for index in 0..32 {
            let selector = u8::try_from(index).unwrap();
            let horizontal =
                u16::from_le_bytes([payload[0x357 + index * 2], payload[0x358 + index * 2]]);
            let horizontal_formula = horizontal_addresses
                .iter()
                .position(|address| *address == horizontal)
                .map(|position| horizontal_formulas[position])
                .unwrap();
            assert_eq!(
                smw_us_v1_layer3_horizontal_scroll(selector),
                horizontal_formula
            );

            let vertical =
                u16::from_le_bytes([payload[0x397 + index * 2], payload[0x398 + index * 2]]);
            let vertical_formula = vertical_addresses
                .iter()
                .position(|address| *address == vertical)
                .map(|position| vertical_formulas[position])
                .unwrap();
            assert_eq!(smw_us_v1_layer3_vertical_scroll(selector), vertical_formula);
        }
    }

    #[test]
    fn dynamic_horizontal_normal_path_matches_fixed_point_and_reset_semantics() {
        let mut state = Layer3DynamicHorizontalState {
            layer2_mode_1403: 0,
            engine_mode_009d: 0,
            signed_bias_17bd: 0xfe,
            control_0be6: 0,
            phase_145c: 0xf0,
            phase_delta_1458: 0x20,
            scroll_x_22: 0x100,
            scroll_y_24: 0,
            vertical_base_146c: 0,
            camera_x_1a: 0,
            state_005b: 0,
            state_005e: 0,
            scratch_26: 0,
            phase_high_17bf: 0,
        };
        assert_eq!(
            smw_us_v1_step_dynamic_horizontal(&mut state),
            Layer3DynamicHorizontalOutcome::HorizontalUpdated
        );
        assert_eq!(state.phase_145c, 0x10);
        assert_eq!(state.scroll_x_22, 0xff);

        state.control_0be6 = 0x8085;
        state.signed_bias_17bd = 3;
        state.phase_145c = 7;
        assert_eq!(
            smw_us_v1_step_dynamic_horizontal(&mut state),
            Layer3DynamicHorizontalOutcome::HorizontalUpdated
        );
        assert_eq!(state.control_0be6, 0x0085);
        assert_eq!(state.phase_145c, 7);
        assert_eq!(state.scroll_x_22, 0x102);
    }

    #[test]
    fn dynamic_horizontal_layer2_and_cross_axis_paths_preserve_side_effects() {
        let mut state = Layer3DynamicHorizontalState {
            layer2_mode_1403: 1,
            engine_mode_009d: 0,
            signed_bias_17bd: 0,
            control_0be6: 0,
            phase_145c: 0xf0,
            phase_delta_1458: 0x0120,
            scroll_x_22: 0,
            scroll_y_24: 0,
            vertical_base_146c: 0x4567,
            camera_x_1a: 0x1234,
            state_005b: 0,
            state_005e: 2,
            scratch_26: 0xffff,
            phase_high_17bf: 0,
        };
        assert_eq!(
            smw_us_v1_step_dynamic_horizontal(&mut state),
            Layer3DynamicHorizontalOutcome::HorizontalUpdated
        );
        assert_eq!(state.scratch_26, 0x6dcc);
        assert_eq!(state.phase_145c, 0x10);
        assert_eq!(state.scroll_x_22, 2);
        assert_eq!(state.phase_high_17bf, 1);

        state.engine_mode_009d = 1;
        assert_eq!(
            smw_us_v1_step_dynamic_horizontal(&mut state),
            Layer3DynamicHorizontalOutcome::VerticalBaseSelected
        );
        assert_eq!(state.scroll_y_24, 0x4567);
    }

    fn vertical_state() -> Layer3DynamicVerticalState {
        Layer3DynamicVerticalState {
            layer2_mode_1403: 0,
            engine_mode_009d: 0,
            signed_bias_17bc: 0,
            control_0be7: 0,
            phase_145d: 0,
            phase_delta_145a: 0,
            scroll_y_24: 0,
            vertical_base_146c: 0,
            camera_y_1c: 0,
            control_190d: 0,
            state_005b: 0,
            state_005e: 0,
            state_13d7: 0,
            layer_flags_145e: 0,
            scratch_02: 0,
            scratch_28: 0,
        }
    }

    #[test]
    fn dynamic_vertical_accumulator_honors_fixed_point_reset_and_bias() {
        let mut state = vertical_state();
        state.phase_145d = 0xf0;
        state.phase_delta_145a = 0x20;
        state.signed_bias_17bc = 0xff;
        state.scroll_y_24 = 0x100;
        assert_eq!(
            smw_us_v1_step_dynamic_vertical_accumulator(&mut state),
            Layer3DynamicVerticalOutcome::AccumulatorUpdated
        );
        assert_eq!(state.phase_145d, 0x10);
        assert_eq!(state.scroll_y_24, 0x100);

        state.control_0be7 = 0x8084;
        state.signed_bias_17bc = 2;
        assert_eq!(
            smw_us_v1_step_dynamic_vertical_accumulator(&mut state),
            Layer3DynamicVerticalOutcome::AccumulatorUpdated
        );
        assert_eq!(state.control_0be7, 0x0084);
        assert_eq!(state.phase_145d, 0x10);
        assert_eq!(state.scroll_y_24, 0x102);
    }

    #[test]
    fn dynamic_vertical_camera_reproduces_double_step_and_restored_primary_state() {
        let mut state = vertical_state();
        state.layer2_mode_1403 = 1;
        state.phase_145d = 0xf0;
        state.phase_delta_145a = 0x0120;
        state.vertical_base_146c = 0x100;
        state.camera_y_1c = 0x20;
        state.state_13d7 = 0x300;
        assert_eq!(
            smw_us_v1_step_dynamic_vertical_camera(&mut state),
            Layer3DynamicVerticalOutcome::CameraUpdated
        );
        assert_eq!(state.phase_145d, 0x10);
        assert_eq!(state.vertical_base_146c, 0x102);
        assert_eq!(state.scroll_y_24, 0x0112);
        // The installed routine performs a second calculation for secondary side effects, then
        // restores phase/base/scroll from the first calculation.
        assert_eq!(state.scratch_28, 0x103);

        let mut simple = vertical_state();
        simple.vertical_base_146c = 0x120;
        simple.camera_y_1c = 0x34;
        assert_eq!(
            smw_us_v1_step_dynamic_vertical_camera(&mut simple),
            Layer3DynamicVerticalOutcome::CameraUpdated
        );
        assert_eq!(simple.scroll_y_24, 0x154);
    }
}
