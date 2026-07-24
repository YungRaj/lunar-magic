use super::{ExpandedSettingsEntryContinuation, ExpandedSettingsRuntimeBuildError};
use lm_project::PatchPayload;
use lm_snes::{BranchCondition, CodeBuilder, CodeBuilderError, Label};

/// Independently generates descriptor block `$213`.
///
/// The entry preserves processor status around vanilla subroutine `$F9F7`, restores X from
/// `$13C6`, returns A=`$18`, and exits through RTL. Lunar Magic reserves `$20` bytes for this
/// entry, so unused bytes retain the installer's `$FF` fill.
///
/// # Errors
///
/// Propagates deterministic 65C816 builder failures.
pub fn smw_us_v1_expanded_settings_index_restore_block() -> Result<PatchPayload, CodeBuilderError> {
    let mut code = CodeBuilder::new();
    code.php();
    code.jsr_absolute(0xf9f7);
    code.plp();
    code.ldx_absolute(0x13c6);
    code.lda_immediate8(0x18);
    code.rtl();
    let assembled = code.finish()?;
    let mut bytes = assembled.bytes;
    bytes.resize(0x20, 0xff);
    Ok(PatchPayload {
        bytes,
        fixups: Vec::new(),
    })
}

/// Independently generates descriptor block `$219`.
///
/// A mismatch between `$1F11` and `$1F12` writes `$0C` to `$0100`; both paths then continue at
/// revision-specific long address `$05DBF2`.
///
/// # Errors
///
/// Propagates deterministic 65C816 builder failures.
pub fn smw_us_v1_expanded_settings_state_compare_block() -> Result<PatchPayload, CodeBuilderError> {
    let mut code = CodeBuilder::new();
    let continue_execution = code.label()?;
    code.lda_absolute(0x1f11);
    code.cmp_absolute(0x1f12);
    code.branch(BranchCondition::Equal, continue_execution);
    code.lda_immediate8(0x0c);
    code.sta_absolute(0x0100);
    code.bind(continue_execution)?;
    code.jml_absolute(0x0005_dbf2);
    let assembled = code.finish()?;
    let mut bytes = assembled.bytes;
    bytes.resize(0x30, 0xff);
    Ok(PatchPayload {
        bytes,
        fixups: Vec::new(),
    })
}

/// Independently generates descriptor block `$220`.
///
/// The block publishes decoded expanded-header fields, resolves table-driven DMA parameters, and
/// exposes compact bit-extraction and DMA-register helper entries.
///
/// # Errors
///
/// Propagates deterministic 65C816 builder failures.
pub fn smw_us_v1_expanded_settings_field_runtime_block() -> Result<PatchPayload, CodeBuilderError> {
    let mut code = CodeBuilder::new();
    let setup = code.label()?;
    let secondary = code.label()?;
    let set_active = code.label()?;
    emit_field_runtime_header(&mut code, setup);
    code.bind(setup)?;
    emit_field_runtime_setup(&mut code, secondary);
    code.bind(secondary)?;
    emit_field_runtime_transfer(&mut code, set_active)?;
    code.bind(set_active)?;
    code.sep(0x30);
    code.lda_long(0x007f_c01a);
    code.ora_immediate8(0x80);
    code.sta_long(0x007f_c01a);
    code.ply();
    code.rts();
    emit_field_runtime_extract_entry(&mut code);
    emit_field_runtime_dma_entry(&mut code);
    let mut bytes = code.finish()?.bytes;
    bytes.extend_from_slice(&[
        0x00, 0x20, 0x00, 0x10, 0x00, 0x08, 0x00, 0x00, 0xa0, 0x50, 0x00, 0x50, 0x80, 0x50, 0x00,
        0x58, 0x40, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    ]);
    bytes.resize(0x150, 0xff);
    Ok(PatchPayload {
        bytes,
        fixups: Vec::new(),
    })
}

fn emit_field_runtime_header(code: &mut CodeBuilder, setup: Label) {
    code.lda_long(0x007f_c009);
    code.cmp_immediate8(0x42);
    code.branch(BranchCondition::Equal, setup);
    code.cmp_immediate8(0x41);
    code.branch(BranchCondition::Equal, setup);
    code.rep(0x20);
    code.stz_absolute(0x145e);
    code.lda_immediate16(0);
    code.sta_long(0x007f_c01a);
    code.sta_long(0x007f_c01b);
    code.sep(0x20);
    code.rts();
}

fn emit_field_runtime_setup(code: &mut CodeBuilder, secondary: Label) {
    code.phy();
    code.lda_long(0x007f_c006);
    code.sta_direct_page(0x8a);
    code.rep(0x20);
    code.lda_long(0x007f_c007);
    code.sta_direct_page(0x8b);
    code.sep(0x20);
    code.ldy_immediate8(0x17);
    code.lda_direct_page_indirect_long_indexed_y(0x8a);
    for _ in 0..4 {
        code.lsr_accumulator();
    }
    code.sta_long(0x007f_c01a);
    code.dey();
    code.dey();
    code.jsr_absolute(0xfe82);
    code.sta_long(0x007f_c01c);
    code.ldy_immediate8(7);
    code.jsr_absolute(0xfe82);
    code.sta_long(0x007f_c01b);
    code.ldy_immediate8(0x1f);
    code.jsr_absolute(0xfe82);
    code.xba();
    code.dey();
    code.dey();
    code.jsr_absolute(0xfe82);
    code.ldy_immediate8(2);
    code.rep(0x30);
    code.sta_absolute(0x145e);
    code.lda_direct_page_indirect_long(0x8a);
    code.asl_accumulator();
    code.asl_accumulator();
    code.branch(BranchCondition::Minus, secondary);
    code.sep(0x30);
    code.ply();
    code.rts();
}

fn emit_field_runtime_transfer(
    code: &mut CodeBuilder,
    set_active: Label,
) -> Result<(), CodeBuilderError> {
    let subtract_path = code.label()?;
    code.lda_direct_page_indirect_long_indexed_y(0x8a);
    code.tax();
    code.and_immediate16(0x0fff);
    code.cmp_immediate16(0x007f);
    code.branch(BranchCondition::Equal, set_active);
    code.txa();
    code.xba();
    for _ in 0..3 {
        code.lsr_accumulator();
    }
    code.tax();
    code.and_immediate16(6);
    code.tay();
    code.txa();
    code.lsr_accumulator();
    code.lsr_accumulator();
    code.and_immediate16(6);
    code.tax();
    code.phb();
    code.phk();
    code.plb();
    code.lda_absolute_indexed_x(0xfec4);
    code.sta_direct_page(0);
    code.lda_absolute_indexed_y(0xfeb4);
    code.ldy_absolute_indexed_x(0xfebc);
    code.ldx_absolute(0xfa85);
    code.plb();
    code.cmp_immediate16(0x1001);
    code.branch(BranchCondition::CarryClear, subtract_path);
    code.sbc_direct_page(0);
    code.phx();
    code.phy();
    code.pha();
    code.pei_direct_page(0);
    code.lda_immediate16(0x1000);
    code.ldy_immediate16(0xbd00);
    code.jsr_absolute(0xfe93);
    code.sep(0x30);
    code.lda_immediate8(1);
    code.jsr_absolute(0xf84e);
    code.rep(0x31);
    code.pla();
    code.adc_immediate16(0xad00);
    code.tay();
    code.pla();
    code.plx();
    code.jsr_absolute(0xfe93);
    code.rep(0x30);
    code.lda_immediate16(0x1000);
    code.sta_absolute(0x4325);
    code.pla();
    code.sta_absolute(0x2116);
    code.lda_absolute(0x2139);
    code.lda_immediate16(0x3981);
    code.ldy_immediate16(0xbd00);
    code.jsr_absolute(0xfe9c);
    code.branch(BranchCondition::Always, set_active);

    code.bind(subtract_path)?;
    code.sec();
    code.sbc_direct_page(0);
    code.phy();
    code.pha();
    code.pei_direct_page(0);
    code.sep(0x30);
    code.lda_immediate8(1);
    code.jsr_absolute(0xf84e);
    code.rep(0x31);
    code.pla();
    code.adc_immediate16(0xad00);
    code.tay();
    code.pla();
    code.plx();
    code.jsr_absolute(0xfe93);
    Ok(())
}

fn emit_field_runtime_extract_entry(code: &mut CodeBuilder) {
    code.lda_direct_page_indirect_long_indexed_y(0x8a);
    code.and_immediate8(0xf0);
    code.sta_direct_page(0);
    code.dey();
    code.dey();
    code.lda_direct_page_indirect_long_indexed_y(0x8a);
    for _ in 0..4 {
        code.lsr_accumulator();
    }
    code.ora_direct_page(0);
    code.rts();
}

fn emit_field_runtime_dma_entry(code: &mut CodeBuilder) {
    code.sta_absolute(0x4325);
    code.stx_absolute(0x2116);
    code.lda_immediate16(0x1801);
    code.sta_absolute(0x4320);
    code.sty_absolute(0x4322);
    code.sep(0x20);
    code.lda_immediate8(0x7e);
    code.sta_absolute(0x4324);
    code.lda_immediate8(0x80);
    code.sta_absolute(0x2115);
    code.lda_immediate8(4);
    code.sta_absolute(0x420b);
    code.rts();
}

/// Independently generates descriptor block `$72`.
///
/// The primary entry runs the recovered vanilla helpers, clears the four `$0105,X` state slots,
/// and publishes a zero state byte. A secondary entry at offset `$50` conditionally invokes
/// `$F840` according to `$7FC00B` bit 0.
///
/// # Errors
///
/// Propagates deterministic 65C816 builder failures.
pub fn smw_us_v1_expanded_settings_reset_block() -> Result<PatchPayload, CodeBuilderError> {
    let mut primary = CodeBuilder::new();
    let nonzero = primary.label()?;
    let loop_entry = primary.label()?;
    primary.jsr_absolute(0xfd80);
    primary.jsr_absolute(0xf9e0);
    primary.lda_immediate8(0);
    primary.jsr_absolute(0xf840);
    primary.branch(BranchCondition::NotEqual, nonzero);
    primary.lda_direct_page(0xfb);
    primary.branch(BranchCondition::Equal, loop_entry);
    primary.dec_accumulator();
    primary.jsr_absolute(0xf8b8);
    primary.bind(nonzero)?;
    primary.stz_direct_page(0xfb);
    primary.bind(loop_entry)?;
    primary.lda_immediate8(0);
    primary.sta_long(0x007f_c009);
    primary.ldx_immediate8(3);
    let clear_loop = primary.label()?;
    primary.bind(clear_loop)?;
    primary.lda_direct_page_indexed_x(4);
    primary.sta_absolute_indexed_x(0x0105);
    primary.dex();
    primary.branch(BranchCondition::Plus, clear_loop);
    primary.rtl();
    let mut bytes = primary.finish()?.bytes;
    bytes.resize(0x50, 0xff);

    let mut secondary = CodeBuilder::new();
    let done = secondary.label()?;
    secondary.sep(0x30);
    secondary.lda_long(0x007f_c00b);
    secondary.lsr_accumulator();
    secondary.branch(BranchCondition::CarryClear, done);
    secondary.lda_immediate8(1);
    secondary.jsr_absolute(0xf840);
    secondary.bind(done)?;
    secondary.rts();
    bytes.extend_from_slice(&secondary.finish()?.bytes);
    bytes.resize(0x60, 0xff);
    Ok(PatchPayload {
        bytes,
        fixups: Vec::new(),
    })
}

/// Independently generates descriptor block `$69`.
///
/// This dispatcher handles stack selectors `$0A/$4B`, state `$42`, the `$FC/$FD` reset paths,
/// vanilla helper calls, and the eight-byte X remap table at offset `$80`.
///
/// # Errors
///
/// Propagates deterministic 65C816 builder failures.
pub fn smw_us_v1_expanded_settings_selector_dispatch_block()
-> Result<PatchPayload, CodeBuilderError> {
    let mut code = CodeBuilder::new();
    let use_y = code.label()?;
    let stack_case_b = code.label()?;
    let state_42 = code.label()?;
    let x_ready = code.label()?;
    let compare_x4 = code.label()?;
    let compare_x2 = code.label()?;
    let clear_0f = code.label()?;
    let set_data_bank = code.label()?;
    let clear_primary_flag = code.label()?;
    let clear_secondary_flag = code.label()?;
    let stack_case_a = code.label()?;
    let common = code.label()?;
    let run_f8b8 = code.label()?;

    code.lda_stack_relative(4);
    code.cmp_immediate8(0x0a);
    code.branch(BranchCondition::Equal, stack_case_a);
    code.cmp_immediate8(0x4b);
    code.branch(BranchCondition::Equal, stack_case_b);
    code.bind(use_y)?;
    code.tya();
    code.branch(BranchCondition::Always, run_f8b8);

    code.bind(stack_case_b)?;
    code.lda_long(0x007f_c009);
    code.cmp_immediate8(0x42);
    code.branch(BranchCondition::Equal, state_42);
    code.lda_direct_page(0xfc);
    code.branch(BranchCondition::Equal, use_y);
    code.cpx_immediate8(0);
    code.branch(BranchCondition::NotEqual, common);
    code.stz_direct_page(0xfc);
    code.branch(BranchCondition::Always, common);

    code.bind(state_42)?;
    code.cpx_immediate8(0x10);
    code.branch(BranchCondition::CarrySet, x_ready);
    code.ldx_immediate8(0x17);
    code.stx_direct_page(0x0f);
    code.bind(x_ready)?;
    code.txa();
    code.and_immediate8(0x0f);
    code.tax();
    code.jsr_absolute(0xf840);
    code.lda_absolute(0x0100);
    code.cmp_immediate8(0x0c);
    code.branch(BranchCondition::Equal, compare_x4);
    code.lda_long(0x0000_81e2);
    code.cmp_immediate8(0x5c);
    code.branch(BranchCondition::Equal, compare_x2);
    code.bind(compare_x4)?;
    code.cpx_immediate8(4);
    code.branch(BranchCondition::Equal, clear_0f);
    code.bind(compare_x2)?;
    code.cpx_immediate8(2);
    code.branch(BranchCondition::NotEqual, set_data_bank);
    code.bind(clear_0f)?;
    code.stz_direct_page(0x0f);
    code.bind(set_data_bank)?;
    code.phb();
    code.phk();
    code.plb();
    code.lda_absolute_indexed_x(0xf1df);
    code.plb();
    code.sta_absolute(0x2117);
    code.ldy_immediate8(0xff);
    code.cpx_immediate8(4);
    code.branch(BranchCondition::CarryClear, clear_primary_flag);
    code.txa();
    code.and_immediate8(3);
    code.tax();
    code.ldy_direct_page_indexed_x(4);
    code.bind(clear_primary_flag)?;
    code.stz_direct_page(0xfc);
    code.rtl();

    code.bind(clear_secondary_flag)?;
    code.stz_direct_page(0xfd);
    code.rtl();

    code.bind(stack_case_a)?;
    code.txa();
    code.clc();
    code.adc_immediate8(8);
    code.jsr_absolute(0xf840);
    code.branch(BranchCondition::NotEqual, clear_secondary_flag);
    code.lda_direct_page(0xfd);
    code.branch(BranchCondition::Equal, use_y);
    code.cpx_immediate8(0);
    code.branch(BranchCondition::NotEqual, common);
    code.stz_direct_page(0xfd);
    code.bind(common)?;
    code.jsr_absolute(0xf8a0);
    code.bind(run_f8b8)?;
    code.jsr_absolute(0xf8b8);
    code.rtl();

    let mut bytes = code.finish()?.bytes;
    bytes.extend_from_slice(&[0x38, 0x30, 0x28, 0x20, 0x18, 0x10, 0x08, 0x00]);
    bytes.resize(0x90, 0xff);
    Ok(PatchPayload {
        bytes,
        fixups: Vec::new(),
    })
}

/// Independently generates descriptor block `$19F`.
///
/// The block contains an indexed table loader plus two scratch-pointer entry points sharing a
/// mapped helper call.
///
/// # Errors
///
/// Rejects addresses outside the SNES 24-bit bus and propagates builder failures.
pub fn smw_us_v1_expanded_settings_indexed_scratch_block(
    indexed_table_snes: u32,
    helper_snes: u32,
) -> Result<PatchPayload, ExpandedSettingsRuntimeBuildError> {
    ensure_snes_address(indexed_table_snes)?;
    ensure_snes_address(helper_snes)?;
    let mut code = CodeBuilder::new();
    let call_helper = code.label()?;

    code.phx();
    code.php();
    code.rep(0x30);
    code.and_immediate16(0x00ff);
    code.dec_accumulator();
    code.asl_accumulator();
    code.asl_accumulator();
    code.sta_direct_page(0x8a);
    code.txa();
    code.clc();
    code.adc_direct_page(0x8a);
    code.tax();
    code.lda_long_indexed_x(indexed_table_snes);
    code.plp();
    code.plx();
    code.rts();

    code.xba();
    code.stz_direct_page(0);
    code.lda_immediate8(0xad);
    code.sta_direct_page(1);
    code.lda_immediate8(0x7e);
    code.sta_direct_page(2);
    code.lda_immediate8(0);
    code.xba();
    code.bind(call_helper)?;
    code.jsl_absolute(helper_snes);
    code.rts();

    code.stz_direct_page(0);
    code.ldx_immediate16(0x7ead);
    code.stx_direct_page(1);
    code.cpy_immediate16(2);
    code.branch(BranchCondition::NotEqual, call_helper);
    code.tax();
    code.lda_long(0x007f_c00b);
    code.lsr_accumulator();
    code.txa();
    code.branch(BranchCondition::CarryClear, call_helper);
    code.ldx_immediate16(0x7f20);
    code.stx_direct_page(1);
    code.branch(BranchCondition::Always, call_helper);

    padded_payload(code, 0x50).map_err(Into::into)
}

/// Independently generates descriptor block `$172`.
///
/// The enabled path indexes the expanded record table, publishes the selected record pointer in
/// `$7FC006/$7FC008`, updates `$7FC009`, and calls vanilla `$F7D0`. The disabled path publishes
/// `$FF` state before the shared epilogue.
///
/// # Errors
///
/// Rejects addresses outside the SNES 24-bit bus and propagates builder failures.
pub fn smw_us_v1_expanded_settings_record_select_block(
    record_table_snes: u32,
) -> Result<PatchPayload, ExpandedSettingsRuntimeBuildError> {
    ensure_snes_address(record_table_snes)?;
    let address = record_table_snes.to_le_bytes();
    let low_word = u16::from_le_bytes([address[0], address[1]]);
    let bank = address[2];
    let mut code = CodeBuilder::new();
    let disabled = code.label()?;
    let finish = code.label()?;
    code.php();
    code.rep(0x30);
    code.lda_direct_page(0xfe);
    code.branch(BranchCondition::Equal, disabled);
    code.dec_accumulator();
    for _ in 0..5 {
        code.asl_accumulator();
    }
    code.tax();
    code.lda_long_indexed_x(record_table_snes);
    code.pha();
    code.txa();
    code.clc();
    code.adc_immediate16(low_word);
    code.sta_long(0x007f_c006);
    code.sep(0x20);
    code.lda_immediate8(bank);
    code.sta_long(0x007f_c008);
    code.pla();
    code.pla();
    code.asl_accumulator();
    code.lda_immediate8(0x41);
    code.adc_immediate8(0);
    code.sta_long(0x007f_c009);
    code.jsr_absolute(0xf7d0);
    code.branch(BranchCondition::Always, finish);
    code.bind(disabled)?;
    code.sep(0x20);
    code.lda_immediate8(0xff);
    code.sta_long(0x007f_c009);
    code.bind(finish)?;
    code.plp();
    code.lda_absolute(0x1925);
    code.cmp_immediate8(9);
    code.rtl();
    padded_payload(code, 0x50).map_err(Into::into)
}

/// Independently generates descriptor block `$1DB`.
///
/// Three index domains select vanilla long tables, the expanded allocation, or a fixed compatibility
/// table. A fourth sentinel skips directly to the shared register-restoring epilogue.
///
/// # Errors
///
/// Rejects addresses outside the SNES 24-bit bus and propagates builder failures.
pub fn smw_us_v1_expanded_settings_pointer_dispatch_block(
    allocation_base_snes: u32,
) -> Result<PatchPayload, ExpandedSettingsRuntimeBuildError> {
    ensure_snes_address(allocation_base_snes)?;
    let allocation_next = allocation_base_snes.checked_add(1).ok_or(
        ExpandedSettingsRuntimeBuildError::AddressOutOfRange(allocation_base_snes),
    )?;
    ensure_snes_address(allocation_next)?;
    let mut code = CodeBuilder::new();
    let expanded = code.label()?;
    let compatibility = code.label()?;
    let restore = code.label()?;
    let trampoline = code.label()?;

    code.phx();
    code.phy();
    code.php();
    code.rep(0x30);
    code.cmp_immediate16(0x0100);
    code.branch(BranchCondition::CarrySet, expanded);
    code.cmp_immediate16(0x0080);
    code.branch(BranchCondition::CarrySet, compatibility);
    code.cmp_immediate16(0x007f);
    code.branch(BranchCondition::Equal, restore);
    code.tax();
    code.sep(0x30);
    code.lda_long_indexed_x(0x0000_b992);
    code.sta_direct_page(0x8a);
    code.lda_long_indexed_x(0x0000_b9c4);
    code.sta_direct_page(0x8b);
    code.lda_long_indexed_x(0x0000_b9f6);
    code.sta_direct_page(0x8c);
    code.branch(BranchCondition::Always, trampoline);

    code.bind(expanded)?;
    code.sec();
    code.sbc_immediate16(0x0100);
    code.sta_direct_page(0x8a);
    code.asl_accumulator();
    code.clc();
    code.adc_direct_page(0x8a);
    code.tax();
    code.lda_long_indexed_x(allocation_base_snes);
    code.sta_direct_page(0x8a);
    code.lda_long_indexed_x(allocation_next);
    code.sta_direct_page(0x8b);
    code.branch(BranchCondition::Always, trampoline);

    code.bind(compatibility)?;
    code.and_immediate16(0x007f);
    code.sta_direct_page(0x8a);
    code.asl_accumulator();
    code.clc();
    code.adc_direct_page(0x8a);
    code.tax();
    code.lda_long_indexed_x(0x000f_f600);
    code.sta_direct_page(0x8a);
    code.lda_long_indexed_x(0x000f_f601);
    code.sta_direct_page(0x8b);

    code.bind(trampoline)?;
    code.sep(0x30);
    code.phk();
    code.per_relative(5);
    code.phb();
    code.phy();
    code.jml_absolute(0x0000_ba47);
    code.rep(0x30);
    code.lda_immediate16(0x0100);
    code.bind(restore)?;
    code.plp();
    code.ply();
    code.plx();
    code.rtl();
    padded_payload(code, 0x70).map_err(Into::into)
}

/// Independently generates descriptor block `$215`.
///
/// The entry dispatches `$0100` states, normalizes the active expanded pointer, drives four vanilla
/// DMA setup iterations, and writes the resulting long pointer back to `$7FC006/$7FC007`.
/// Lunar Magic patches one opcode according to compatibility mode: either RTS or CLC followed by
/// the expanded path.
///
/// # Errors
///
/// Propagates deterministic 65C816 builder failures.
pub fn smw_us_v1_expanded_settings_dma_block(
    continuation: ExpandedSettingsEntryContinuation,
) -> Result<PatchPayload, CodeBuilderError> {
    let mut code = CodeBuilder::new();
    let configurable_entry = code.label()?;
    let secondary_entry = code.label()?;
    let fallback_pointer = code.label()?;
    let pointer_ready = code.label()?;
    let loop_entry = code.label()?;
    let next_iteration = code.label()?;

    code.lda_absolute(0x0100);
    code.cmp_immediate8(0x12);
    code.branch(BranchCondition::Equal, configurable_entry);
    code.cmp_immediate8(0x0c);
    code.branch(BranchCondition::Equal, secondary_entry);
    code.cmp_immediate8(4);
    code.branch(BranchCondition::Equal, configurable_entry);
    code.rts();
    code.bind(secondary_entry)?;
    code.branch(BranchCondition::Equal, configurable_entry);
    code.lda_immediate8(0x77);
    code.sta_absolute(0x210b);
    code.bind(configurable_entry)?;
    match continuation {
        ExpandedSettingsEntryContinuation::Return => code.rts(),
        ExpandedSettingsEntryContinuation::Continue => code.clc(),
    }

    code.lda_long(0x007f_c009);
    code.tax();
    code.lda_long(0x007f_c006);
    code.pha();
    code.sta_direct_page(0x8a);
    code.rep(0x20);
    code.lda_long(0x007f_c007);
    code.pha();
    code.sta_direct_page(0x8b);
    code.cpx_immediate8(0x42);
    code.branch(BranchCondition::Equal, pointer_ready);
    code.cpx_immediate8(0x41);
    code.branch(BranchCondition::Equal, pointer_ready);
    code.bind(fallback_pointer)?;
    code.lda_immediate16(0xfa6f);
    code.sta_long(0x007f_c006);
    code.sep(0x20);
    code.phk();
    code.pla();
    code.sta_long(0x007f_c008);
    code.branch(BranchCondition::Always, loop_entry);
    code.bind(pointer_ready)?;
    code.lda_direct_page_indirect_long(0x8a);
    code.asl_accumulator();
    code.branch(BranchCondition::Plus, fallback_pointer);
    code.sep(0x20);

    code.bind(loop_entry)?;
    code.ldx_immediate8(3);
    let iteration = code.label()?;
    code.bind(iteration)?;
    code.txa();
    code.clc();
    code.adc_immediate8(0x0c);
    code.jsr_absolute(0xf84e);
    code.xba();
    code.branch(BranchCondition::Equal, next_iteration);
    emit_expanded_settings_dma_iteration(&mut code);
    code.bind(next_iteration)?;
    code.dex();
    code.branch(BranchCondition::Plus, iteration);
    code.rep(0x20);
    code.pla();
    code.sta_long(0x007f_c007);
    code.sep(0x20);
    code.pla();
    code.sta_long(0x007f_c006);
    code.rts();

    let mut bytes = code.finish()?.bytes;
    bytes.resize(0xa0, 0);
    bytes.extend_from_slice(&[
        0x4c, 0x00, 0x48, 0x00, 0x44, 0x00, 0x40, 0x2b, 0x00, 0x2a, 0x00, 0x29, 0x00, 0x28, 0x00,
    ]);
    bytes.resize(0xd0, 0xff);
    Ok(PatchPayload {
        bytes,
        fixups: Vec::new(),
    })
}

fn emit_expanded_settings_dma_iteration(code: &mut CodeBuilder) {
    code.rep(0x20);
    code.phx();
    code.txa();
    code.asl_accumulator();
    code.tax();
    code.phb();
    code.phk();
    code.plb();
    code.lda_absolute_indexed_x(0xfa7f);
    code.plb();
    code.sta_absolute(0x2116);
    code.lda_immediate16(0xad00);
    code.sta_absolute(0x4322);
    code.lda_immediate16(0x0800);
    code.sta_absolute(0x4325);
    code.ldx_immediate8(0x80);
    code.stx_absolute(0x2115);
    code.lda_immediate16(0x1801);
    code.sta_absolute(0x4320);
    code.ldx_immediate8(0x7e);
    code.stx_absolute(0x4324);
    code.ldx_immediate8(4);
    code.stx_absolute(0x420b);
    code.sep(0x20);
    code.plx();
}

/// Independently generates descriptor block `$173`.
///
/// The primary entry recognizes state `$42`, preserves the caller's A value, captures long
/// scratch values in direct page `$8A/$8B`, and invokes vanilla helper `$F8CB`. Its secondary entry
/// computes an allocation-relative index and performs the recovered long indexed load.
///
/// # Errors
///
/// Rejects addresses outside the SNES 24-bit bus and propagates builder failures.
pub fn smw_us_v1_expanded_settings_allocation_load_block(
    allocation_base_snes: u32,
) -> Result<PatchPayload, ExpandedSettingsRuntimeBuildError> {
    ensure_snes_address(allocation_base_snes)?;
    let mut code = CodeBuilder::new();
    let state_42 = code.label()?;
    code.pha();
    code.lda_long(0x007f_c009);
    code.cmp_immediate8(0x42);
    code.branch(BranchCondition::Equal, state_42);
    code.pla();
    code.lda_immediate8(0);
    code.rts();
    code.bind(state_42)?;
    code.pla();
    code.phx();
    code.phy();
    code.php();
    code.asl_accumulator();
    code.tay();
    code.rep(0x30);
    code.lda_long(0x007f_c006);
    code.sta_direct_page(0x8a);
    code.lda_long(0x007f_c007);
    code.sta_direct_page(0x8b);
    code.lda_direct_page_indirect_long_indexed_y(0x8a);
    code.and_immediate16(0x0fff);
    code.jsr_absolute(0xf8cb);
    code.plp();
    code.ply();
    code.plx();
    code.lda_immediate8(1);
    code.rts();
    code.adc_direct_page(0x8a);
    code.tax();
    code.lda_long_indexed_x(allocation_base_snes);
    padded_payload(code, 0x50).map_err(Into::into)
}

/// Independently generates descriptor block `$216`.
///
/// X selects a 16-byte record. The routine publishes the special-record low word and bank into
/// `$7FC006/$7FC008`, writes state `$42`, clears `$7FC00B`, and returns.
///
/// # Errors
///
/// Rejects addresses outside the SNES 24-bit bus and propagates builder failures.
pub fn smw_us_v1_expanded_settings_special_record_block(
    special_record_snes: u32,
) -> Result<PatchPayload, ExpandedSettingsRuntimeBuildError> {
    ensure_snes_address(special_record_snes)?;
    let address_bytes = special_record_snes.to_le_bytes();
    let low_word = u16::from_le_bytes([address_bytes[0], address_bytes[1]]);
    let bank = address_bytes[2];
    let mut code = CodeBuilder::new();
    code.sta_direct_page(0x20);
    code.txa();
    for _ in 0..4 {
        code.asl_accumulator();
    }
    code.clc();
    code.adc_immediate16(low_word);
    code.sta_long(0x007f_c006);
    code.sep(0x20);
    code.lda_immediate8(bank);
    code.sta_long(0x007f_c008);
    code.lda_immediate8(0x42);
    code.sta_long(0x007f_c009);
    code.lda_immediate8(0);
    code.sta_long(0x007f_c00b);
    code.rtl();
    padded_payload(code, 0x40).map_err(Into::into)
}

fn ensure_snes_address(address: u32) -> Result<(), ExpandedSettingsRuntimeBuildError> {
    if address > 0x00ff_ffff {
        Err(ExpandedSettingsRuntimeBuildError::AddressOutOfRange(
            address,
        ))
    } else {
        Ok(())
    }
}

fn padded_payload(code: CodeBuilder, len: usize) -> Result<PatchPayload, CodeBuilderError> {
    let assembled = code.finish()?;
    let mut bytes = assembled.bytes;
    bytes.resize(len, 0xff);
    Ok(PatchPayload {
        bytes,
        fixups: Vec::new(),
    })
}
