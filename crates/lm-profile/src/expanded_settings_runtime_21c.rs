//! Independently generated SMW US revision-0 descriptor `$21C` runtime.

use crate::ExpandedSettingsEntryContinuation;
use lm_project::PatchPayload;
use lm_snes::{BranchCondition, CodeBuilder, CodeBuilderError, Label};

/// Generates the final `$220`-byte expanded-settings runtime block.
///
/// `special_record_snes` supplies the split low-word/bank publication and `helper_snes` supplies
/// the mapped long helper called by the compact record entry.
///
/// # Errors
///
/// Rejects addresses outside the SNES 24-bit bus and propagates deterministic builder failures.
pub fn smw_us_v1_expanded_settings_transfer_runtime_block(
    special_record_snes: u32,
    helper_snes: u32,
    continuation: ExpandedSettingsEntryContinuation,
) -> Result<PatchPayload, ExpandedSettingsTransferRuntimeError> {
    ensure_address(special_record_snes)?;
    ensure_address(helper_snes)?;
    let special = special_record_snes.to_le_bytes();
    let mut code = CodeBuilder::new();
    let main = code.label()?;
    emit_alternate_entry(&mut code, main)?;
    code.bind(main)?;
    let (expanded_path, legacy_jump) = emit_pointer_and_dma_setup(
        &mut code,
        u16::from_le_bytes([special[0], special[1]]),
        special[2],
        continuation,
    )?;
    code.bind(expanded_path)?;
    emit_expanded_transfer(&mut code, legacy_jump)?;
    emit_video_setup_entry(&mut code)?;
    emit_record_helper_entry(&mut code, helper_snes);
    emit_dma_commit_entry(&mut code)?;
    emit_wait_entry(&mut code)?;
    let mut bytes = code.finish()?.bytes;
    bytes.resize(0x1f8, 0);
    bytes.extend_from_slice(&[
        0x28, 0x00, 0x20, 0x00, 0x18, 0x00, 0x10, 0x00, 0x08, 0x00, 0x00, 0x00, 0x78, 0x00, 0x70,
        0x00, 0x68, 0x00, 0x60,
    ]);
    bytes.resize(0x220, 0xff);
    Ok(PatchPayload {
        bytes,
        fixups: Vec::new(),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsTransferRuntimeError {
    Code(CodeBuilderError),
    AddressOutOfRange(u32),
}

impl std::fmt::Display for ExpandedSettingsTransferRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded-settings transfer runtime failed: {self:?}"
        )
    }
}

impl std::error::Error for ExpandedSettingsTransferRuntimeError {}

impl From<CodeBuilderError> for ExpandedSettingsTransferRuntimeError {
    fn from(value: CodeBuilderError) -> Self {
        Self::Code(value)
    }
}

fn ensure_address(address: u32) -> Result<(), ExpandedSettingsTransferRuntimeError> {
    if address <= 0x00ff_ffff {
        Ok(())
    } else {
        Err(ExpandedSettingsTransferRuntimeError::AddressOutOfRange(
            address,
        ))
    }
}

fn emit_alternate_entry(code: &mut CodeBuilder, main: Label) -> Result<(), CodeBuilderError> {
    let wait = code.label()?;
    code.branch(BranchCondition::Always, main);
    code.cmp_immediate8(0x30);
    code.branch(BranchCondition::CarryClear, main);
    code.rep(0x20);
    code.lda_immediate16(0xfb43);
    code.sta_absolute(0x0183);
    code.sep(0x20);
    code.phk();
    code.pla();
    code.sta_absolute(0x0185);
    code.lda_immediate8(0xd0);
    code.sta_absolute(0x2209);
    code.bind(wait)?;
    code.lda_absolute(0x018a);
    code.branch(BranchCondition::Equal, wait);
    code.stz_absolute(0x018a);
    code.rtl();
    Ok(())
}

fn emit_pointer_and_dma_setup(
    code: &mut CodeBuilder,
    special_low: u16,
    special_bank: u8,
    continuation: ExpandedSettingsEntryContinuation,
) -> Result<(Label, Label), CodeBuilderError> {
    let first_next = code.label()?;
    let second_loop = code.label()?;
    let second_next = code.label()?;
    let expanded = code.label()?;
    let legacy_jump = code.label()?;
    code.stz_absolute(0x0703);
    code.stz_absolute(0x0803);
    code.ldx_absolute(0x0db3);
    code.ldy_absolute_indexed_x(0x1f11);
    code.rep(0x20);
    code.tya();
    for _ in 0..5 {
        code.asl_accumulator();
    }
    code.clc();
    code.adc_immediate16(special_low);
    code.sta_long(0x007f_c006);
    code.sep(0x20);
    code.lda_immediate8(special_bank);
    code.sta_long(0x007f_c008);
    code.stz_absolute(0x4200);
    code.ldx_immediate8(9);
    let first_loop = code.label()?;
    code.bind(first_loop)?;
    emit_indexed_helper_call(code);
    code.branch(BranchCondition::Equal, first_next);
    emit_first_dma_iteration(code);
    code.sep(0x10);
    code.bind(first_next)?;
    code.ldx_immediate8(8);
    code.bind(second_loop)?;
    emit_indexed_helper_call(code);
    code.branch(BranchCondition::Equal, second_next);
    emit_second_dma_iteration(code);
    code.sep(0x10);
    code.dex();
    // Preserve Lunar Magic's deliberate overlapping-code target at SEP's `$10` operand.
    code.bind_at_offset(second_next, code.offset() - 1)?;
    code.cpx_immediate8(1);
    code.branch(BranchCondition::NotEqual, second_loop);
    match continuation {
        ExpandedSettingsEntryContinuation::Return => code.sec(),
        ExpandedSettingsEntryContinuation::Continue => code.clc(),
    }
    code.branch(BranchCondition::CarryClear, expanded);
    code.branch_long(legacy_jump);
    Ok((expanded, legacy_jump))
}

fn emit_indexed_helper_call(code: &mut CodeBuilder) {
    code.txa();
    code.clc();
    code.adc_immediate8(2);
    code.jsr_absolute(0xf84e);
    code.xba();
}

fn emit_first_dma_iteration(code: &mut CodeBuilder) {
    code.rep(0x30);
    code.txa();
    code.asl_accumulator();
    code.tax();
    code.phb();
    code.phk();
    code.plb();
    code.lda_absolute_indexed_x(0xfd17);
    code.plb();
    code.tax();
    code.clc();
    code.adc_immediate16(0x0160);
    code.pha();
    code.txa();
    code.adc_immediate16(0x0060);
    code.tax();
    code.ldy_immediate16(0xadc0);
    code.lda_immediate16(0x0140);
    code.jsr_absolute(0xfcdf);
    code.ldy_immediate16(0xafc0);
    code.sty_absolute(0x4322);
    code.ldx_immediate16(0x0d40);
    code.stx_absolute(0x4325);
    code.plx();
    code.stx_absolute(0x2116);
    code.sta_absolute(0x420b);
}

fn emit_second_dma_iteration(code: &mut CodeBuilder) {
    code.rep(0x30);
    code.phx();
    code.txa();
    code.asl_accumulator();
    code.tax();
    code.phb();
    code.phk();
    code.plb();
    code.lda_absolute_indexed_x(0xfd17);
    code.plb();
    code.tax();
    code.ldy_immediate16(0xad00);
    code.lda_immediate16(0x1000);
    code.jsr_absolute(0xfcdf);
    code.plx();
}

fn emit_expanded_transfer(
    code: &mut CodeBuilder,
    legacy_jump: Label,
) -> Result<(), CodeBuilderError> {
    let fallback = code.label()?;
    code.lda_long(0x007f_c006);
    code.pha();
    code.sta_direct_page(0x8a);
    code.rep(0x20);
    code.lda_long(0x007f_c007);
    code.pha();
    code.sta_direct_page(0x8b);
    code.lda_direct_page_indirect_long(0x8a);
    code.asl_accumulator();
    code.branch(BranchCondition::Minus, fallback);
    code.lda_immediate16(0xfa6f);
    code.sta_long(0x007f_c006);
    code.sep(0x20);
    code.phk();
    code.pla();
    code.sta_long(0x007f_c008);
    code.bind(fallback)?;

    emit_transfer_pair(code, 0x001c, 0x7ead, 0x0018, 0x7eb5, 0xfa83, 0xfa7f, true);
    code.rep(0x20);
    emit_transfer_pair(code, 0x001e, 0x7ead, 0x001a, 0x7eb5, 0xfa85, 0xfa81, false);
    code.rep(0x20);
    code.pla();
    code.sta_long(0x007f_c007);
    code.sep(0x30);
    code.pla();
    code.sta_long(0x007f_c006);
    code.bind(legacy_jump)?;
    code.lda_immediate8(0);
    code.jsr_absolute(0xf84e);
    code.phk();
    code.per_relative(6);
    code.pea_absolute(0x8413);
    code.jml_absolute(0x0004_8086);
    code.phk();
    code.per_relative(6);
    code.pea_absolute(0x8413);
    code.jml_absolute(0x0004_80e0);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_transfer_pair(
    code: &mut CodeBuilder,
    first_y: u16,
    first_x: u16,
    second_y: u16,
    second_x: u16,
    table_x: u16,
    table_a: u16,
    rep_after_y: bool,
) {
    if rep_after_y {
        code.ldy_immediate8(first_y.to_le_bytes()[0]);
        code.rep(0x30);
    } else {
        code.ldy_immediate16(first_y);
    }
    code.ldx_immediate16(first_x);
    code.jsr_absolute(0xfcc5);
    code.ldy_immediate16(second_y);
    code.ldx_immediate16(second_x);
    code.jsr_absolute(0xfcc5);
    code.phb();
    code.phk();
    code.plb();
    code.lda_absolute(table_x);
    code.tax();
    code.lda_absolute(table_a);
    code.plb();
    code.pha();
    code.ldy_immediate16(0xad00);
    code.lda_immediate16(0x0800);
    code.jsr_absolute(0xfcdf);
    code.ldy_immediate16(0x0800);
    code.sty_absolute(0x4325);
    code.plx();
    code.stx_absolute(0x2116);
    code.sta_absolute(0x420b);
}

fn emit_video_setup_entry(code: &mut CodeBuilder) -> Result<(), CodeBuilderError> {
    let wait = code.label()?;
    code.rep(0x30);
    code.lda_immediate16(0x0200);
    code.ldy_immediate16(0x0703);
    code.sta_absolute(0x4325);
    code.sty_absolute(0x4322);
    code.lda_immediate16(0x2200);
    code.sta_absolute(0x4320);
    code.sep(0x20);
    code.stz_absolute(0x4324);
    code.lda_immediate8(4);
    code.jsr_absolute(0xfd0c);
    code.stz_absolute(0x2121);
    code.sta_absolute(0x420b);
    code.sep(0x30);
    code.lda_immediate8(0x81);
    code.bind(wait)?;
    code.bit_absolute(0x4212);
    code.branch(BranchCondition::Minus, wait);
    code.sta_absolute(0x4200);
    code.rtl();
    Ok(())
}

fn emit_record_helper_entry(code: &mut CodeBuilder, helper_snes: u32) {
    code.lda_long(0x007f_c006);
    code.sta_direct_page(0x8a);
    code.lda_long(0x007f_c007);
    code.sta_direct_page(0x8b);
    code.lda_direct_page_indirect_long_indexed_y(0x8a);
    code.and_immediate16(0x0fff);
    code.stz_direct_page(0);
    code.stx_direct_page(1);
    code.jsl_absolute(helper_snes);
    code.rts();
}

fn emit_dma_commit_entry(code: &mut CodeBuilder) -> Result<(), CodeBuilderError> {
    let wait_clear = code.label()?;
    let wait_set = code.label()?;
    code.sta_absolute(0x4325);
    code.sty_absolute(0x4322);
    code.lda_immediate16(0x1801);
    code.sta_absolute(0x4320);
    code.sep(0x20);
    code.lda_immediate8(0x7e);
    code.sta_absolute(0x4324);
    code.lda_immediate8(4);
    code.xba();
    code.lda_immediate8(0x80);
    code.bind(wait_clear)?;
    code.bit_absolute(0x4212);
    code.branch(BranchCondition::Minus, wait_clear);
    code.bind(wait_set)?;
    code.bit_absolute(0x4212);
    code.branch(BranchCondition::Plus, wait_set);
    code.stx_absolute(0x2116);
    code.sta_absolute(0x2115);
    code.xba();
    code.sta_absolute(0x420b);
    code.rts();
    Ok(())
}

fn emit_wait_entry(code: &mut CodeBuilder) -> Result<(), CodeBuilderError> {
    let wait_clear = code.label()?;
    let wait_set = code.label()?;
    code.bind(wait_clear)?;
    code.bit_absolute(0x4212);
    code.branch(BranchCondition::Minus, wait_clear);
    code.bind(wait_set)?;
    code.bit_absolute(0x4212);
    code.branch(BranchCondition::Plus, wait_set);
    code.rts();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn generated_transfer_runtime_matches_template_and_resolved_oracles() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
        let installed = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let template = smw_us_v1_expanded_settings_transfer_runtime_block(
            0x00_8000,
            0x00_f900,
            ExpandedSettingsEntryContinuation::Return,
        )
        .unwrap();
        let source = 0x005b_5c70 - 0x0040_0000;
        assert_eq!(template.bytes, executable[source..source + 0x220]);
        let resolved = smw_us_v1_expanded_settings_transfer_runtime_block(
            0x11_ed00,
            0x0f_f900,
            ExpandedSettingsEntryContinuation::Continue,
        )
        .unwrap();
        assert_eq!(resolved.bytes, installed[0x7fd20..0x7ff40]);
    }
}
