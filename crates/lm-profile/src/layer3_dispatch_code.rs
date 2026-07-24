//! Generated selector dispatch and post-scroll continuation for the first Layer 3 hook.

use crate::Layer3ScrollHelperLibrary;
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload};
use lm_snes::{AssembledCode, BranchCondition, CodeBuilder, CodeBuilderError, LongAddressTarget};

const PHASE_CONSTANTS: [u16; 32] = [
    0, 0, 0, 0, 0, 0, 0x4000, 0x8000, 0, 1, 2, 0xffc0, 0xff80, 0xff00, 0xfe00, 0xfd00, 0xfc00,
    0x0300, 0x0400, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layer3ScrollDispatchProgram {
    pub payload: PatchPayload,
}

/// Generates selector normalization, helper calls, phase initialization, and the final return.
///
/// Logical payload 0 is the setup routine, while payload 2 is the generated helper library.
///
/// # Errors
///
/// Propagates checked local-branch and label-construction failures.
pub fn smw_us_v1_layer3_scroll_dispatch_program(
    helpers: &Layer3ScrollHelperLibrary,
) -> Result<Layer3ScrollDispatchProgram, CodeBuilderError> {
    let mut code = CodeBuilder::new();
    emit_horizontal_dispatch(&mut code, helpers)?;
    emit_vertical_dispatch(&mut code, helpers)?;
    emit_post_scroll_tail(&mut code)?;
    Ok(Layer3ScrollDispatchProgram {
        payload: patch_payload(code.finish()?),
    })
}

fn emit_horizontal_dispatch(
    code: &mut CodeBuilder,
    helpers: &Layer3ScrollHelperLibrary,
) -> Result<(), CodeBuilderError> {
    let normalized = code.label()?;
    code.ldy_absolute(0x145f);
    code.tya();
    code.and_immediate16(0x000f);
    code.bit_direct_page(1);
    code.branch(BranchCondition::Plus, normalized);
    code.ora_immediate16(0x0010);
    code.bind(normalized)?;
    code.asl_accumulator();
    code.tax();
    code.stx_absolute(0x145f);
    code.txa();
    code.lsr_accumulator();
    let targets = std::array::from_fn(|index| {
        if PHASE_CONSTANTS[index] == 0 {
            helpers.horizontal[index].offset
        } else {
            helpers.horizontal[0].offset
        }
    });
    emit_selector_cases(code, &targets, helpers, false)
}

fn emit_vertical_dispatch(
    code: &mut CodeBuilder,
    helpers: &Layer3ScrollHelperLibrary,
) -> Result<(), CodeBuilderError> {
    let normalized = code.label()?;
    code.tya();
    code.and_immediate16(0x00f0);
    code.bit_direct_page(1);
    code.branch(BranchCondition::OverflowClear, normalized);
    code.ora_immediate16(0x0100);
    code.bind(normalized)?;
    code.lsr_accumulator();
    code.lsr_accumulator();
    code.lsr_accumulator();
    code.tax();
    code.stx_absolute(0x1460);
    code.txa();
    code.lsr_accumulator();
    let targets = std::array::from_fn(|index| helpers.vertical[index].offset);
    emit_selector_cases(code, &targets, helpers, true)
}

fn emit_selector_cases(
    code: &mut CodeBuilder,
    targets: &[usize; 32],
    helpers: &Layer3ScrollHelperLibrary,
    vertical: bool,
) -> Result<(), CodeBuilderError> {
    let done = code.label()?;
    for index in 0..32 {
        let next = code.label()?;
        code.cmp_immediate16(u16::try_from(index).expect("selector fits in u16"));
        code.branch(BranchCondition::NotEqual, next);
        code.lda_immediate16(PHASE_CONSTANTS[index]);
        code.pha();
        if vertical && PHASE_CONSTANTS[index] != 0 {
            let selected = code.label()?;
            let called = code.label()?;
            code.ldx_absolute(0x1403);
            code.branch(BranchCondition::NotEqual, selected);
            emit_helper_call(code, helpers.vertical[0].offset);
            code.branch(BranchCondition::Always, called);
            code.bind(selected)?;
            emit_helper_call(code, targets[index]);
            code.bind(called)?;
        } else {
            emit_helper_call(code, targets[index]);
        }
        code.branch_long(done);
        code.bind(next)?;
    }
    code.bind(done)?;
    Ok(())
}

fn emit_helper_call(code: &mut CodeBuilder, offset: usize) {
    code.jsl(LongAddressTarget {
        payload: 2,
        addend: offset,
    });
}

fn emit_post_scroll_tail(code: &mut CodeBuilder) -> Result<(), CodeBuilderError> {
    let no_snapshot = code.label()?;
    let ordinary_level = code.label()?;
    let vertical_negative = code.label()?;
    let horizontal_negative = code.label()?;
    let horizontal_zero = code.label()?;
    let zero_status = code.label()?;
    let store_status = code.label()?;
    let special_level = code.label()?;

    code.lda_absolute(0x145e);
    code.lsr_accumulator();
    code.lsr_accumulator();
    code.branch(BranchCondition::CarryClear, no_snapshot);
    code.lda_direct_page(0x22);
    code.sta_absolute(0x1b78);
    code.lda_direct_page(0x24);
    code.sta_absolute(0x1b7a);
    code.bind(no_snapshot)?;
    code.ldx_absolute(0x0100);
    code.cpx_immediate8(0x1d);
    code.branch(BranchCondition::Equal, special_level);

    code.pla();
    code.branch(BranchCondition::Equal, ordinary_level);
    code.sta_absolute(0x145a);
    code.branch(BranchCondition::Minus, vertical_negative);
    code.lda_immediate16(0);
    code.tax();
    code.bind(vertical_negative)?;
    code.stx_absolute(0x145d);
    code.bind(ordinary_level)?;

    code.pla();
    code.branch(BranchCondition::Equal, horizontal_zero);
    code.sta_absolute(0x1458);
    code.branch(BranchCondition::Minus, horizontal_negative);
    code.lda_immediate16(0);
    code.tax();
    code.bind(horizontal_negative)?;
    code.stx_absolute(0x145c);

    code.bind(horizontal_zero)?;
    code.ldx_absolute(0x1403);
    code.branch(BranchCondition::NotEqual, zero_status);
    code.ldx_direct_page(0x5b);
    code.branch(BranchCondition::Plus, zero_status);
    code.lda_absolute(0x1413);
    code.and_immediate16(0xf0f0);
    code.branch(BranchCondition::Equal, zero_status);
    code.lda_immediate16(0x8080);
    code.tsb_absolute(0x0be6);
    code.bind(zero_status)?;
    code.ldx_immediate8(0);
    code.branch(BranchCondition::Always, store_status);

    code.bind(special_level)?;
    code.pla();
    code.pla();
    code.ldx_immediate8(1);
    code.bind(store_status)?;
    code.stx_absolute(0x13d5);
    code.sep(0x20);
    code.jml(LongAddressTarget {
        payload: 0,
        addend: 6,
    });
    Ok(())
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
    use crate::smw_us_v1_layer3_scroll_helper_library;

    #[test]
    fn dispatch_has_all_helper_calls_and_a_setup_return() {
        let helpers = smw_us_v1_layer3_scroll_helper_library().unwrap();
        let dispatch = smw_us_v1_layer3_scroll_dispatch_program(&helpers).unwrap();
        assert_eq!(
            dispatch
                .payload
                .fixups
                .iter()
                .filter(|fixup| fixup.target_payload == 2)
                .count(),
            76
        );
        assert_eq!(
            dispatch.payload.fixups.last(),
            Some(&PatchFixup {
                offset: dispatch.payload.bytes.len() - 3,
                target_payload: 0,
                target_addend: 6,
                encoding: PatchFixupEncoding::Long24,
            })
        );
        assert!(
            dispatch
                .payload
                .bytes
                .windows(3)
                .any(|bytes| bytes == [0x0c, 0xe6, 0x0b])
        );
        assert!(
            dispatch
                .payload
                .bytes
                .windows(3)
                .any(|bytes| bytes == [0x8e, 0xd5, 0x13])
        );
    }
}
