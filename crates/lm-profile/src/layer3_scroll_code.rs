//! Pointer-table-free generated 65C816 helpers for Layer 3 scroll formulas.

use crate::{
    Layer3ScrollFormula, smw_us_v1_layer3_horizontal_scroll, smw_us_v1_layer3_vertical_scroll,
};
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload};
use lm_snes::{AssembledCode, BranchCondition, CodeBuilder, CodeBuilderError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Layer3ScrollHelperTarget {
    pub offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layer3ScrollHelperLibrary {
    pub payload: PatchPayload,
    pub horizontal: [Layer3ScrollHelperTarget; 32],
    pub vertical: [Layer3ScrollHelperTarget; 32],
}

#[derive(Clone, Copy)]
enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy)]
struct AxisOffsets {
    base: usize,
    full: usize,
    div2: usize,
    div4: usize,
    div8: usize,
    div16: usize,
    div32: usize,
    div64: Option<usize>,
    div5: usize,
}

#[derive(Clone, Copy)]
struct DynamicOffsets {
    horizontal: usize,
    vertical_accumulator: usize,
    vertical_camera: usize,
}

/// Generates all ordinary SMW US revision-0 Layer 3 scroll helpers.
///
/// Direct local entry offsets replace Lunar Magic's bank-relative pointer tables. This includes
/// the recovered horizontal accumulator and both distinct vertical dynamic state machines.
///
/// # Errors
///
/// Propagates deterministic code-builder failures.
///
/// # Panics
///
/// Panics only if a standard-library 32-element array supplies an index that cannot fit in `u8`,
/// or if the deterministic helper set exhausts the builder's complete 32-bit label namespace.
pub fn smw_us_v1_layer3_scroll_helper_library()
-> Result<Layer3ScrollHelperLibrary, CodeBuilderError> {
    let mut code = CodeBuilder::new();
    let horizontal_offsets = emit_axis_helpers(&mut code, Axis::Horizontal);
    let vertical_offsets = emit_axis_helpers(&mut code, Axis::Vertical);
    let dynamic_offsets = emit_dynamic_helpers(&mut code)?;
    let assembled = code.finish()?;
    let horizontal = std::array::from_fn(|index| {
        helper_target(
            smw_us_v1_layer3_horizontal_scroll(u8::try_from(index).unwrap()),
            horizontal_offsets,
            dynamic_offsets,
        )
    });
    let vertical = std::array::from_fn(|index| {
        helper_target(
            smw_us_v1_layer3_vertical_scroll(u8::try_from(index).unwrap()),
            vertical_offsets,
            dynamic_offsets,
        )
    });
    Ok(Layer3ScrollHelperLibrary {
        payload: patch_payload(assembled),
        horizontal,
        vertical,
    })
}

fn emit_axis_helpers(code: &mut CodeBuilder, axis: Axis) -> AxisOffsets {
    AxisOffsets {
        base: emit_shift_helper(code, axis, None),
        full: emit_shift_helper(code, axis, Some(0)),
        div2: emit_shift_helper(code, axis, Some(1)),
        div4: emit_shift_helper(code, axis, Some(2)),
        div8: emit_shift_helper(code, axis, Some(3)),
        div16: emit_shift_helper(code, axis, Some(4)),
        div32: emit_shift_helper(code, axis, Some(5)),
        div64: matches!(axis, Axis::Horizontal).then(|| emit_shift_helper(code, axis, Some(6))),
        div5: emit_divide_by_five_helper(code, axis),
    }
}

fn emit_shift_helper(code: &mut CodeBuilder, axis: Axis, shifts: Option<u8>) -> usize {
    let offset = code.offset();
    let (base, position, output) = axis_addresses(axis);
    if let Some(shifts) = shifts {
        code.lda_direct_page(position);
        for _ in 0..shifts {
            code.lsr_accumulator();
        }
        code.clc();
        code.adc_absolute(base);
    } else {
        code.lda_absolute(base);
    }
    code.sta_direct_page(output);
    code.rtl();
    offset
}

fn emit_divide_by_five_helper(code: &mut CodeBuilder, axis: Axis) -> usize {
    let offset = code.offset();
    let sa1 = code.label().expect("small deterministic label set");
    let (base, position, output) = axis_addresses(axis);
    code.tsc();
    code.cmp_immediate16(0x3000);
    code.lda_direct_page(position);
    code.branch(BranchCondition::CarrySet, sa1);
    code.ldx_immediate8(5);
    code.sta_absolute(0x4204);
    code.stx_absolute(0x4206);
    code.xba();
    code.xba();
    code.clc();
    code.adc_absolute(base);
    code.adc_absolute(0x4214);
    code.sta_direct_page(output);
    code.rtl();
    code.bind(sa1).expect("fresh label");
    code.ldx_immediate8(1);
    code.stx_absolute(0x2250);
    code.ldx_immediate8(5);
    code.rep(0x31);
    code.sta_absolute(0x2251);
    code.stx_absolute(0x2253);
    code.adc_absolute(base);
    code.adc_absolute(0x2306);
    code.sta_direct_page(output);
    code.sep(0x10);
    code.rtl();
    offset
}

const fn axis_addresses(axis: Axis) -> (u16, u8, u8) {
    match axis {
        Axis::Horizontal => (0x146a, 0x1a, 0x22),
        Axis::Vertical => (0x146c, 0x1c, 0x24),
    }
}

fn helper_target(
    formula: Layer3ScrollFormula,
    offsets: AxisOffsets,
    dynamic: DynamicOffsets,
) -> Layer3ScrollHelperTarget {
    let offset = match formula {
        Layer3ScrollFormula::BaseOnly => offsets.base,
        Layer3ScrollFormula::BasePlusPosition => offsets.full,
        Layer3ScrollFormula::BasePlusPositionDiv2 => offsets.div2,
        Layer3ScrollFormula::BasePlusPositionDiv4 => offsets.div4,
        Layer3ScrollFormula::BasePlusPositionDiv8 => offsets.div8,
        Layer3ScrollFormula::BasePlusPositionDiv16 => offsets.div16,
        Layer3ScrollFormula::BasePlusPositionDiv32 => offsets.div32,
        Layer3ScrollFormula::BasePlusPositionDiv64 => {
            offsets.div64.expect("divide-by-64 is horizontal-only")
        }
        Layer3ScrollFormula::BasePlusPositionDiv5 => offsets.div5,
        Layer3ScrollFormula::DynamicHorizontal => dynamic.horizontal,
        Layer3ScrollFormula::DynamicVerticalAccumulator => dynamic.vertical_accumulator,
        Layer3ScrollFormula::DynamicVerticalCamera => dynamic.vertical_camera,
    };
    Layer3ScrollHelperTarget { offset }
}

fn emit_dynamic_helpers(code: &mut CodeBuilder) -> Result<DynamicOffsets, CodeBuilderError> {
    let vertical_camera_layer2 = code.label()?;
    let vertical_camera_once = code.label()?;
    let horizontal = emit_dynamic_horizontal(code)?;
    let vertical_accumulator = emit_dynamic_vertical_accumulator(code, vertical_camera_layer2)?;
    let vertical_camera =
        emit_dynamic_vertical_camera(code, vertical_camera_layer2, vertical_camera_once)?;
    Ok(DynamicOffsets {
        horizontal,
        vertical_accumulator,
        vertical_camera,
    })
}

fn emit_dynamic_horizontal(code: &mut CodeBuilder) -> Result<usize, CodeBuilderError> {
    let offset = code.offset();
    let layer2 = code.label()?;
    let normal = code.label()?;
    let bias_ready = code.label()?;
    let advance = code.label()?;
    let phase_sign_ready = code.label()?;
    let combine = code.label()?;
    let scratch_ready = code.label()?;
    let layer2_normal = code.label()?;
    let layer2_bias_ready = code.label()?;
    let layer2_phase_sign_ready = code.label()?;

    code.ldx_absolute(0x1403);
    code.branch(BranchCondition::NotEqual, layer2);
    code.ldx_direct_page(0x9d);
    code.branch(BranchCondition::Equal, normal);
    emit_vertical_base_return(code);
    code.bind(normal)?;
    emit_signed_byte_to_dp(code, 0x17bd, 4, bias_ready)?;
    code.ldx_absolute(0x0be6);
    code.branch(BranchCondition::Plus, advance);
    code.lda_immediate16(0x8000);
    code.trb_absolute(0x0be6);
    code.lda_immediate16(0);
    code.branch(BranchCondition::Always, combine);
    code.bind(advance)?;
    emit_phase_increment(code, 0x145c, 0x1458, phase_sign_ready)?;
    code.bind(combine)?;
    code.clc();
    code.adc_direct_page(0x22);
    code.clc();
    code.adc_direct_page(4);
    code.sta_direct_page(0x22);
    code.rtl();

    code.bind(layer2)?;
    code.stz_direct_page(0x26);
    code.lda_direct_page(0x5b);
    code.lsr_accumulator();
    code.branch(BranchCondition::CarrySet, scratch_ready);
    code.ldx_direct_page(0x5e);
    code.dex();
    code.branch(BranchCondition::Equal, scratch_ready);
    code.lda_immediate16(0x8000);
    code.sec();
    code.sbc_direct_page(0x1a);
    code.sta_direct_page(0x26);
    code.bind(scratch_ready)?;
    code.ldx_direct_page(0x9d);
    code.branch(BranchCondition::Equal, layer2_normal);
    emit_vertical_base_return(code);
    code.bind(layer2_normal)?;
    emit_signed_byte_to_dp(code, 0x17bd, 4, layer2_bias_ready)?;
    emit_phase_increment(code, 0x145c, 0x1458, layer2_phase_sign_ready)?;
    code.clc();
    code.adc_direct_page(0x22);
    code.clc();
    code.adc_direct_page(4);
    code.sta_direct_page(0x22);
    code.txa();
    code.clc();
    code.adc_absolute(0x1458);
    code.xba();
    code.tax();
    code.stx_absolute(0x17bf);
    code.rtl();
    Ok(offset)
}

fn emit_dynamic_vertical_accumulator(
    code: &mut CodeBuilder,
    camera_layer2: lm_snes::Label,
) -> Result<usize, CodeBuilderError> {
    let offset = code.offset();
    let normal = code.label()?;
    let engine_normal = code.label()?;
    let bias_ready = code.label()?;
    let advance = code.label()?;
    let phase_sign_ready = code.label()?;
    let combine = code.label()?;
    code.ldx_absolute(0x1403);
    code.branch(BranchCondition::Equal, normal);
    code.jml_label(camera_layer2, 0);
    code.bind(normal)?;
    code.ldx_direct_page(0x9d);
    code.branch(BranchCondition::Equal, engine_normal);
    code.rtl();
    code.bind(engine_normal)?;
    emit_signed_byte_to_dp(code, 0x17bc, 4, bias_ready)?;
    code.ldx_absolute(0x0be7);
    code.branch(BranchCondition::Plus, advance);
    code.lda_immediate16(0x8000);
    code.trb_absolute(0x0be7);
    code.lda_immediate16(0);
    code.branch(BranchCondition::Always, combine);
    code.bind(advance)?;
    emit_phase_increment(code, 0x145d, 0x145a, phase_sign_ready)?;
    code.bind(combine)?;
    code.clc();
    code.adc_direct_page(0x24);
    code.clc();
    code.adc_direct_page(4);
    code.sta_direct_page(0x24);
    code.rtl();
    Ok(offset)
}

fn emit_dynamic_vertical_camera(
    code: &mut CodeBuilder,
    camera_layer2: lm_snes::Label,
    camera_once: lm_snes::Label,
) -> Result<usize, CodeBuilderError> {
    let offset = code.offset();
    let layer2 = code.label()?;
    let single = code.label()?;
    code.ldx_absolute(0x1403);
    code.branch(BranchCondition::NotEqual, layer2);
    code.lda_absolute(0x146c);
    code.clc();
    code.adc_direct_page(0x1c);
    code.sta_direct_page(0x24);
    code.rtl();
    code.bind(layer2)?;
    code.bind(camera_layer2)?;
    code.bit_absolute(0x190d);
    code.branch(BranchCondition::OverflowSet, single);
    code.jsl_label(camera_once, 0);
    code.pei_direct_page(0x24);
    code.lda_absolute(0x146c);
    code.pha();
    code.ldx_absolute(0x145d);
    code.phx();
    code.jsl_label(camera_once, 0);
    code.plx();
    code.stx_absolute(0x145d);
    code.pla();
    code.sta_absolute(0x146c);
    code.pla();
    code.sta_direct_page(0x24);
    code.rtl();
    code.bind(single)?;
    code.jml_label(camera_once, 0);

    code.bind(camera_once)?;
    emit_dynamic_vertical_camera_once(code)?;
    Ok(offset)
}

fn emit_dynamic_vertical_camera_once(code: &mut CodeBuilder) -> Result<(), CodeBuilderError> {
    let calculate = code.label()?;
    let phase_sign_ready = code.label()?;
    let in_range = code.label()?;
    let negative = code.label()?;
    let boundary_ready = code.label()?;
    let minimum_ready = code.label()?;
    let done = code.label()?;
    code.ldx_direct_page(0x9d);
    code.branch(BranchCondition::NotEqual, calculate);
    emit_phase_increment(code, 0x145d, 0x145a, phase_sign_ready)?;
    code.clc();
    code.adc_absolute(0x146c);
    code.sta_absolute(0x146c);
    code.bind(calculate)?;
    code.lda_absolute(0x146c);
    code.clc();
    code.adc_direct_page(0x1c);
    code.branch(BranchCondition::Minus, negative);
    code.cmp_immediate16(0x0118);
    code.branch(BranchCondition::CarryClear, in_range);
    code.sta_direct_page(2);
    code.and_immediate16(0x000f);
    code.eor_immediate16(8);
    code.clc();
    code.adc_immediate16(0x0108);
    code.sta_direct_page(0x24);
    code.lda_direct_page(0x5b);
    code.lsr_accumulator();
    code.lda_absolute(0x13d7);
    code.branch(BranchCondition::CarryClear, boundary_ready);
    code.lda_direct_page(0x5e);
    code.and_immediate16(0x00ff);
    code.bind(boundary_ready)?;
    code.sec();
    code.sbc_immediate16(0x0100);
    code.branch(BranchCondition::Minus, done);
    code.cmp_immediate16(0x0100);
    code.branch(BranchCondition::CarryClear, done);
    code.cmp_direct_page(2);
    code.branch(BranchCondition::CarryClear, minimum_ready);
    code.lda_direct_page(2);
    code.bind(minimum_ready)?;
    code.sec();
    code.sbc_direct_page(0x1c);
    code.sta_direct_page(0x28);
    code.rtl();
    code.bind(in_range)?;
    code.sta_direct_page(0x24);
    code.sec();
    code.sbc_direct_page(0x1c);
    code.sta_direct_page(0x28);
    code.rtl();
    code.bind(negative)?;
    code.stz_direct_page(0x24);
    code.sta_direct_page(2);
    code.lda_absolute(0x145e);
    code.and_immediate16(4);
    code.branch(BranchCondition::Equal, done);
    code.lda_direct_page(2);
    code.sec();
    code.sbc_direct_page(0x1c);
    code.sta_direct_page(0x28);
    code.bind(done)?;
    code.rtl();
    Ok(())
}

fn emit_signed_byte_to_dp(
    code: &mut CodeBuilder,
    address: u16,
    destination: u8,
    ready: lm_snes::Label,
) -> Result<(), CodeBuilderError> {
    code.ldx_absolute(address);
    code.txa();
    code.tax();
    code.branch(BranchCondition::Plus, ready);
    code.ora_immediate16(0xff00);
    code.bind(ready)?;
    code.sta_direct_page(destination);
    Ok(())
}

fn emit_phase_increment(
    code: &mut CodeBuilder,
    phase: u16,
    delta: u16,
    sign_ready: lm_snes::Label,
) -> Result<(), CodeBuilderError> {
    code.ldx_absolute(phase);
    code.txa();
    code.clc();
    code.adc_absolute(delta);
    code.tax();
    code.stx_absolute(phase);
    code.and_immediate16(0xff00);
    code.branch(BranchCondition::Plus, sign_ready);
    code.ora_immediate16(0x00ff);
    code.bind(sign_ready)?;
    code.xba();
    Ok(())
}

fn emit_vertical_base_return(code: &mut CodeBuilder) {
    code.lda_absolute(0x146c);
    code.sta_direct_page(0x24);
    code.rtl();
}

fn patch_payload(assembled: AssembledCode) -> PatchPayload {
    PatchPayload {
        bytes: assembled.bytes,
        fixups: assembled
            .long_address_fixups
            .into_iter()
            .map(|fixup| PatchFixup {
                offset: fixup.offset,
                target_payload: fixup.target.payload,
                target_addend: fixup.target.addend,
                encoding: PatchFixupEncoding::Long24,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_selector_has_a_bounded_generated_entry() {
        let library = smw_us_v1_layer3_scroll_helper_library().unwrap();
        for target in library.horizontal.iter().chain(&library.vertical) {
            assert!(target.offset < library.payload.bytes.len());
        }
        assert_eq!(library.payload.fixups.len(), 4);
        assert!(library.payload.fixups.iter().all(|fixup| {
            fixup.target_payload == 0
                && fixup.target_addend < library.payload.bytes.len()
                && fixup.offset + 3 <= library.payload.bytes.len()
        }));
        assert_eq!(library.horizontal[6], library.horizontal[17]);
        assert_eq!(library.vertical[6], library.vertical[17]);
        assert_eq!(library.vertical[1], library.vertical[18]);
    }

    #[test]
    fn generated_divide_by_five_contains_both_hardware_paths() {
        let library = smw_us_v1_layer3_scroll_helper_library().unwrap();
        let offset = library.horizontal[5].offset;
        let code = &library.payload.bytes[offset..];
        assert_eq!(&code[..8], &[0x3b, 0xc9, 0, 0x30, 0xa5, 0x1a, 0xb0, 0x14]);
        assert!(code.windows(3).any(|bytes| bytes == [0x8d, 0x04, 0x42]));
        assert!(code.windows(3).any(|bytes| bytes == [0x8d, 0x51, 0x22]));
        assert!(code.windows(3).any(|bytes| bytes == [0x6d, 0x06, 0x23]));
    }
}
