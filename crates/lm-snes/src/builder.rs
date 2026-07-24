use crate::{BranchCondition, IndexWidth, RegisterWidth};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Label(u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LongAddressTarget {
    pub payload: usize,
    pub addend: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LongAddressFixup {
    /// Offset of the first byte of the little-endian 24-bit operand.
    pub offset: usize,
    pub target: LongAddressTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssembledCode {
    pub bytes: Vec<u8>,
    pub long_address_fixups: Vec<LongAddressFixup>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodeBuilderError {
    TooManyLabels,
    LabelAlreadyBound(Label),
    UnboundLabel(Label),
    RelativeBranchOutOfRange {
        label: Label,
        instruction_offset: usize,
        displacement: isize,
    },
    RelativeLongBranchOutOfRange {
        label: Label,
        instruction_offset: usize,
        displacement: isize,
    },
    CodeTooLarge,
}

impl std::fmt::Display for CodeBuilderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "65C816 code construction failed: {self:?}")
    }
}

impl std::error::Error for CodeBuilderError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingBranch {
    instruction_offset: usize,
    operand_offset: usize,
    target: Label,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingLongBranch {
    instruction_offset: usize,
    operand_offset: usize,
    target: Label,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingLongLabelFixup {
    operand_offset: usize,
    target: Label,
    payload: usize,
}

#[derive(Clone, Debug, Default)]
pub struct CodeBuilder {
    bytes: Vec<u8>,
    labels: BTreeMap<Label, Option<usize>>,
    branches: Vec<PendingBranch>,
    long_branches: Vec<PendingLongBranch>,
    long_address_fixups: Vec<LongAddressFixup>,
    long_label_fixups: Vec<PendingLongLabelFixup>,
    next_label: u32,
}

impl CodeBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn offset(&self) -> usize {
        self.bytes.len()
    }

    /// Allocates a typed local label. It must be bound exactly once before finishing.
    ///
    /// # Errors
    ///
    /// Returns an error only after exhausting the complete 32-bit label namespace.
    pub fn label(&mut self) -> Result<Label, CodeBuilderError> {
        let label = Label(self.next_label);
        self.next_label = self
            .next_label
            .checked_add(1)
            .ok_or(CodeBuilderError::TooManyLabels)?;
        self.labels.insert(label, None);
        Ok(label)
    }

    /// Binds a local label to the current byte offset.
    ///
    /// # Errors
    ///
    /// Rejects binding the same label more than once.
    pub fn bind(&mut self, label: Label) -> Result<(), CodeBuilderError> {
        self.bind_at_offset(label, self.bytes.len())
    }

    /// Binds a local label to an already-emitted byte offset.
    ///
    /// This supports intentional overlapping 65C816 instruction streams found in legacy code.
    ///
    /// # Errors
    ///
    /// Rejects rebinding, unknown labels, and offsets beyond the emitted code.
    pub fn bind_at_offset(&mut self, label: Label, offset: usize) -> Result<(), CodeBuilderError> {
        if offset > self.bytes.len() {
            return Err(CodeBuilderError::CodeTooLarge);
        }
        let position = self
            .labels
            .get_mut(&label)
            .ok_or(CodeBuilderError::UnboundLabel(label))?;
        if position.is_some() {
            return Err(CodeBuilderError::LabelAlreadyBound(label));
        }
        *position = Some(offset);
        Ok(())
    }

    pub fn php(&mut self) {
        self.byte(0x08);
    }

    pub fn plp(&mut self) {
        self.byte(0x28);
    }

    pub fn pla(&mut self) {
        self.byte(0x68);
    }

    pub fn pha(&mut self) {
        self.byte(0x48);
    }

    pub fn phk(&mut self) {
        self.byte(0x4b);
    }

    pub fn phx(&mut self) {
        self.byte(0xda);
    }

    pub fn plx(&mut self) {
        self.byte(0xfa);
    }

    pub fn phy(&mut self) {
        self.byte(0x5a);
    }

    pub fn ply(&mut self) {
        self.byte(0x7a);
    }

    pub fn pei_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0xd4, address]);
    }

    pub fn pea_absolute(&mut self, value: u16) {
        self.byte(0xf4);
        self.word(value);
    }

    pub fn per_relative(&mut self, displacement: i16) {
        self.byte(0x62);
        self.bytes.extend_from_slice(&displacement.to_le_bytes());
    }

    pub fn phb(&mut self) {
        self.byte(0x8b);
    }

    pub fn plb(&mut self) {
        self.byte(0xab);
    }

    pub fn rtl(&mut self) {
        self.byte(0x6b);
    }

    pub fn rts(&mut self) {
        self.byte(0x60);
    }

    pub fn jsr_absolute(&mut self, address: u16) {
        self.byte(0x20);
        self.bytes.extend_from_slice(&address.to_le_bytes());
    }

    pub fn tax(&mut self) {
        self.byte(0xaa);
    }

    pub fn txa(&mut self) {
        self.byte(0x8a);
    }

    pub fn tya(&mut self) {
        self.byte(0x98);
    }

    pub fn tay(&mut self) {
        self.byte(0xa8);
    }

    pub fn tsc(&mut self) {
        self.byte(0x3b);
    }

    pub fn inx(&mut self) {
        self.byte(0xe8);
    }

    pub fn dex(&mut self) {
        self.byte(0xca);
    }

    pub fn dey(&mut self) {
        self.byte(0x88);
    }

    pub fn clc(&mut self) {
        self.byte(0x18);
    }

    pub fn sec(&mut self) {
        self.byte(0x38);
    }

    pub fn dec_accumulator(&mut self) {
        self.byte(0x3a);
    }

    pub fn lsr_accumulator(&mut self) {
        self.byte(0x4a);
    }

    pub fn ror_accumulator(&mut self) {
        self.byte(0x6a);
    }

    pub fn asl_accumulator(&mut self) {
        self.byte(0x0a);
    }

    pub fn xba(&mut self) {
        self.byte(0xeb);
    }

    pub fn set_register_width(&mut self, width: RegisterWidth) {
        match width {
            RegisterWidth::Eight => self.bytes.extend_from_slice(&[0xe2, 0x20]),
            RegisterWidth::Sixteen => self.bytes.extend_from_slice(&[0xc2, 0x20]),
        }
    }

    pub fn set_index_width(&mut self, width: IndexWidth) {
        match width {
            IndexWidth::Eight => self.bytes.extend_from_slice(&[0xe2, 0x10]),
            IndexWidth::Sixteen => self.bytes.extend_from_slice(&[0xc2, 0x10]),
        }
    }

    pub fn rep(&mut self, mask: u8) {
        self.bytes.extend_from_slice(&[0xc2, mask]);
    }

    pub fn sep(&mut self, mask: u8) {
        self.bytes.extend_from_slice(&[0xe2, mask]);
    }

    pub fn lda_immediate8(&mut self, value: u8) {
        self.bytes.extend_from_slice(&[0xa9, value]);
    }

    pub fn lda_immediate16(&mut self, value: u16) {
        self.byte(0xa9);
        self.word(value);
    }

    pub fn lda_stack_relative(&mut self, offset: u8) {
        self.bytes.extend_from_slice(&[0xa3, offset]);
    }

    pub fn lda_absolute(&mut self, address: u16) {
        self.byte(0xad);
        self.word(address);
    }

    pub fn lda_absolute_indexed_x(&mut self, address: u16) {
        self.byte(0xbd);
        self.word(address);
    }

    pub fn lda_absolute_indexed_y(&mut self, address: u16) {
        self.byte(0xb9);
        self.word(address);
    }

    pub fn lda_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0xa5, address]);
    }

    pub fn lda_direct_page_indexed_x(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0xb5, address]);
    }

    pub fn lda_direct_page_indirect_long_indexed_y(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0xb7, address]);
    }

    pub fn lda_direct_page_indirect_long(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0xa7, address]);
    }

    pub fn lda_long(&mut self, address: u32) {
        self.byte(0xaf);
        self.long(address);
    }

    pub fn lda_long_indexed_x(&mut self, address: u32) {
        self.byte(0xbf);
        self.long(address);
    }

    pub fn ldx_immediate8(&mut self, value: u8) {
        self.bytes.extend_from_slice(&[0xa2, value]);
    }

    pub fn ldx_immediate16(&mut self, value: u16) {
        self.byte(0xa2);
        self.word(value);
    }

    pub fn ldy_immediate8(&mut self, value: u8) {
        self.bytes.extend_from_slice(&[0xa0, value]);
    }

    pub fn ldy_immediate16(&mut self, value: u16) {
        self.byte(0xa0);
        self.word(value);
    }

    pub fn ldx_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0xa6, address]);
    }

    pub fn ldx_absolute(&mut self, address: u16) {
        self.byte(0xae);
        self.word(address);
    }

    pub fn ldy_absolute(&mut self, address: u16) {
        self.byte(0xac);
        self.word(address);
    }

    pub fn ldy_absolute_indexed_x(&mut self, address: u16) {
        self.byte(0xbc);
        self.word(address);
    }

    pub fn ldy_direct_page_indexed_x(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0xb4, address]);
    }

    pub fn ldy_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0xa4, address]);
    }

    pub fn sta_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0x85, address]);
    }

    pub fn sta_absolute(&mut self, address: u16) {
        self.byte(0x8d);
        self.word(address);
    }

    pub fn sta_absolute_indexed_x(&mut self, address: u16) {
        self.byte(0x9d);
        self.word(address);
    }

    pub fn sta_long(&mut self, address: u32) {
        self.byte(0x8f);
        self.long(address);
    }

    pub fn stx_absolute(&mut self, address: u16) {
        self.byte(0x8e);
        self.word(address);
    }

    pub fn stx_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0x86, address]);
    }

    pub fn sty_absolute(&mut self, address: u16) {
        self.byte(0x8c);
        self.word(address);
    }

    pub fn stz_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0x64, address]);
    }

    pub fn stz_absolute(&mut self, address: u16) {
        self.byte(0x9c);
        self.word(address);
    }

    pub fn and_immediate8(&mut self, value: u8) {
        self.bytes.extend_from_slice(&[0x29, value]);
    }

    pub fn and_immediate16(&mut self, value: u16) {
        self.byte(0x29);
        self.word(value);
    }

    pub fn ora_immediate16(&mut self, value: u16) {
        self.byte(0x09);
        self.word(value);
    }

    pub fn ora_immediate8(&mut self, value: u8) {
        self.bytes.extend_from_slice(&[0x09, value]);
    }

    pub fn eor_immediate16(&mut self, value: u16) {
        self.byte(0x49);
        self.word(value);
    }

    pub fn adc_immediate16(&mut self, value: u16) {
        self.byte(0x69);
        self.word(value);
    }

    pub fn adc_immediate8(&mut self, value: u8) {
        self.bytes.extend_from_slice(&[0x69, value]);
    }

    pub fn adc_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0x65, address]);
    }

    pub fn adc_absolute(&mut self, address: u16) {
        self.byte(0x6d);
        self.word(address);
    }

    pub fn sbc_immediate16(&mut self, value: u16) {
        self.byte(0xe9);
        self.word(value);
    }

    pub fn sbc_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0xe5, address]);
    }

    pub fn cmp_immediate16(&mut self, value: u16) {
        self.byte(0xc9);
        self.word(value);
    }

    pub fn cmp_immediate8(&mut self, value: u8) {
        self.bytes.extend_from_slice(&[0xc9, value]);
    }

    pub fn cmp_absolute(&mut self, address: u16) {
        self.byte(0xcd);
        self.word(address);
    }

    pub fn cpy_immediate16(&mut self, value: u16) {
        self.byte(0xc0);
        self.word(value);
    }

    pub fn cpy_absolute(&mut self, address: u16) {
        self.byte(0xcc);
        self.word(address);
    }

    pub fn cpx_immediate8(&mut self, value: u8) {
        self.bytes.extend_from_slice(&[0xe0, value]);
    }

    pub fn cmp_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0xc5, address]);
    }

    pub fn bit_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0x24, address]);
    }

    pub fn bit_absolute(&mut self, address: u16) {
        self.byte(0x2c);
        self.word(address);
    }

    pub fn tsb_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0x04, address]);
    }

    pub fn tsb_absolute(&mut self, address: u16) {
        self.byte(0x0c);
        self.word(address);
    }

    pub fn ora_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0x05, address]);
    }

    pub fn trb_direct_page(&mut self, address: u8) {
        self.bytes.extend_from_slice(&[0x14, address]);
    }

    pub fn trb_absolute(&mut self, address: u16) {
        self.byte(0x1c);
        self.word(address);
    }

    pub fn branch(&mut self, condition: BranchCondition, target: Label) {
        let instruction_offset = self.bytes.len();
        self.bytes.extend_from_slice(&[condition.opcode(), 0]);
        self.branches.push(PendingBranch {
            instruction_offset,
            operand_offset: instruction_offset + 1,
            target,
        });
    }

    /// Emits a checked unconditional `BRL` to a local label.
    pub fn branch_long(&mut self, target: Label) {
        let instruction_offset = self.bytes.len();
        self.bytes.extend_from_slice(&[0x82, 0, 0]);
        self.long_branches.push(PendingLongBranch {
            instruction_offset,
            operand_offset: instruction_offset + 1,
            target,
        });
    }

    pub fn jsl(&mut self, target: LongAddressTarget) {
        self.long_instruction(0x22, target);
    }

    /// Emits a relocatable `JSL` to a label in an allocated payload.
    pub fn jsl_label(&mut self, target: Label, payload: usize) {
        self.long_label_instruction(0x22, target, payload);
    }

    pub fn jml(&mut self, target: LongAddressTarget) {
        self.long_instruction(0x5c, target);
    }

    /// Emits a relocatable `JML` to a label in an allocated payload.
    pub fn jml_label(&mut self, target: Label, payload: usize) {
        self.long_label_instruction(0x5c, target, payload);
    }

    pub fn jml_absolute(&mut self, address: u32) {
        self.byte(0x5c);
        self.long(address);
    }

    pub fn jsl_absolute(&mut self, address: u32) {
        self.byte(0x22);
        self.long(address);
    }

    /// Resolves local branches and returns deterministic bytes plus external 24-bit fixups.
    ///
    /// # Errors
    ///
    /// Rejects unbound labels, branches outside signed-eight-bit reach, or code offsets exceeding
    /// the host's signed-offset representation.
    pub fn finish(mut self) -> Result<AssembledCode, CodeBuilderError> {
        for branch in &self.branches {
            let target = self
                .labels
                .get(&branch.target)
                .and_then(|value| *value)
                .ok_or(CodeBuilderError::UnboundLabel(branch.target))?;
            let target = isize::try_from(target).map_err(|_| CodeBuilderError::CodeTooLarge)?;
            let next = isize::try_from(branch.instruction_offset + 2)
                .map_err(|_| CodeBuilderError::CodeTooLarge)?;
            let displacement = target - next;
            let encoded = i8::try_from(displacement).map_err(|_| {
                CodeBuilderError::RelativeBranchOutOfRange {
                    label: branch.target,
                    instruction_offset: branch.instruction_offset,
                    displacement,
                }
            })?;
            self.bytes[branch.operand_offset] = encoded.to_le_bytes()[0];
        }
        for branch in &self.long_branches {
            let target = self
                .labels
                .get(&branch.target)
                .and_then(|value| *value)
                .ok_or(CodeBuilderError::UnboundLabel(branch.target))?;
            let target = isize::try_from(target).map_err(|_| CodeBuilderError::CodeTooLarge)?;
            let next = isize::try_from(branch.instruction_offset + 3)
                .map_err(|_| CodeBuilderError::CodeTooLarge)?;
            let displacement = target - next;
            let encoded = i16::try_from(displacement).map_err(|_| {
                CodeBuilderError::RelativeLongBranchOutOfRange {
                    label: branch.target,
                    instruction_offset: branch.instruction_offset,
                    displacement,
                }
            })?;
            self.bytes[branch.operand_offset..branch.operand_offset + 2]
                .copy_from_slice(&encoded.to_le_bytes());
        }
        for fixup in &self.long_label_fixups {
            let addend = self
                .labels
                .get(&fixup.target)
                .and_then(|value| *value)
                .ok_or(CodeBuilderError::UnboundLabel(fixup.target))?;
            self.long_address_fixups.push(LongAddressFixup {
                offset: fixup.operand_offset,
                target: LongAddressTarget {
                    payload: fixup.payload,
                    addend,
                },
            });
        }
        Ok(AssembledCode {
            bytes: self.bytes,
            long_address_fixups: self.long_address_fixups,
        })
    }

    fn long_instruction(&mut self, opcode: u8, target: LongAddressTarget) {
        self.byte(opcode);
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(&[0, 0, 0]);
        self.long_address_fixups
            .push(LongAddressFixup { offset, target });
    }

    fn long_label_instruction(&mut self, opcode: u8, target: Label, payload: usize) {
        self.byte(opcode);
        let operand_offset = self.bytes.len();
        self.bytes.extend_from_slice(&[0, 0, 0]);
        self.long_label_fixups.push(PendingLongLabelFixup {
            operand_offset,
            target,
            payload,
        });
    }

    fn byte(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn word(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn long(&mut self, value: u32) {
        let bytes = value.to_le_bytes();
        self.bytes.extend_from_slice(&bytes[..3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_backward_branches_and_external_fixups_resolve_exactly() {
        let mut code = CodeBuilder::new();
        let loop_start = code.label().unwrap();
        let done = code.label().unwrap();
        code.branch(BranchCondition::Always, done);
        code.bind(loop_start).unwrap();
        code.inx();
        code.branch(BranchCondition::NotEqual, loop_start);
        code.bind(done).unwrap();
        code.jsl(LongAddressTarget {
            payload: 2,
            addend: 0x123,
        });
        code.rtl();
        let assembled = code.finish().unwrap();
        assert_eq!(
            assembled.bytes,
            [0x80, 3, 0xe8, 0xd0, 0xfd, 0x22, 0, 0, 0, 0x6b]
        );
        assert_eq!(
            assembled.long_address_fixups,
            [LongAddressFixup {
                offset: 6,
                target: LongAddressTarget {
                    payload: 2,
                    addend: 0x123
                }
            }]
        );
    }

    #[test]
    fn long_branches_and_relocatable_local_calls_resolve_exactly() {
        let mut code = CodeBuilder::new();
        let helper = code.label().unwrap();
        let done = code.label().unwrap();
        code.jsl_label(helper, 0);
        code.branch_long(done);
        code.bind(helper).unwrap();
        code.rtl();
        code.bind(done).unwrap();
        code.jml_label(helper, 2);
        let assembled = code.finish().unwrap();
        assert_eq!(
            assembled.bytes,
            [0x22, 0, 0, 0, 0x82, 1, 0, 0x6b, 0x5c, 0, 0, 0]
        );
        assert_eq!(
            assembled.long_address_fixups,
            [
                LongAddressFixup {
                    offset: 1,
                    target: LongAddressTarget {
                        payload: 0,
                        addend: 7,
                    },
                },
                LongAddressFixup {
                    offset: 9,
                    target: LongAddressTarget {
                        payload: 2,
                        addend: 7,
                    },
                }
            ]
        );
    }

    #[test]
    fn duplicate_unbound_and_far_labels_are_rejected() {
        let mut duplicate = CodeBuilder::new();
        let label = duplicate.label().unwrap();
        duplicate.bind(label).unwrap();
        assert_eq!(
            duplicate.bind(label),
            Err(CodeBuilderError::LabelAlreadyBound(label))
        );

        let mut unbound = CodeBuilder::new();
        let missing = unbound.label().unwrap();
        unbound.branch(BranchCondition::Equal, missing);
        assert_eq!(
            unbound.finish(),
            Err(CodeBuilderError::UnboundLabel(missing))
        );

        let mut far = CodeBuilder::new();
        let target = far.label().unwrap();
        far.branch(BranchCondition::Always, target);
        for _ in 0..128 {
            far.inx();
        }
        far.bind(target).unwrap();
        assert!(matches!(
            far.finish(),
            Err(CodeBuilderError::RelativeBranchOutOfRange { .. })
        ));

        let mut far_long = CodeBuilder::new();
        let target = far_long.label().unwrap();
        far_long.branch_long(target);
        for _ in 0..32_768 {
            far_long.inx();
        }
        far_long.bind(target).unwrap();
        assert!(matches!(
            far_long.finish(),
            Err(CodeBuilderError::RelativeLongBranchOutOfRange { .. })
        ));
    }

    #[test]
    fn width_and_address_instructions_have_stable_encodings() {
        let mut code = CodeBuilder::new();
        code.php();
        code.set_register_width(RegisterWidth::Sixteen);
        code.set_index_width(IndexWidth::Eight);
        code.lda_immediate16(0x1234);
        code.lda_absolute(0x1be3);
        code.lda_long(0x007f_c01a);
        code.and_immediate16(4);
        code.txa();
        code.asl_accumulator();
        code.xba();
        code.cmp_immediate16(0x4567);
        code.tsb_direct_page(0x40);
        code.ora_direct_page(0x41);
        code.plp();
        code.rts();
        assert_eq!(
            code.finish().unwrap().bytes,
            [
                0x08, 0xc2, 0x20, 0xe2, 0x10, 0xa9, 0x34, 0x12, 0xad, 0xe3, 0x1b, 0xaf, 0x1a, 0xc0,
                0x7f, 0x29, 4, 0, 0x8a, 0x0a, 0xeb, 0xc9, 0x67, 0x45, 0x04, 0x40, 0x05, 0x41, 0x28,
                0x60
            ]
        );
    }

    #[test]
    fn runtime_helper_addressing_modes_have_stable_encodings() {
        let mut code = CodeBuilder::new();
        code.phx();
        code.plx();
        code.pei_direct_page(0x24);
        code.tya();
        code.tsc();
        code.dex();
        code.clc();
        code.sec();
        code.lda_direct_page(0x1a);
        code.lda_long_indexed_x(0x10_9de4);
        code.ldx_immediate8(5);
        code.ldx_absolute(0x145c);
        code.ldy_absolute(0x145f);
        code.stx_absolute(0x145c);
        code.stz_direct_page(0x26);
        code.eor_immediate16(8);
        code.adc_direct_page(0x22);
        code.adc_absolute(0x146a);
        code.sbc_immediate16(0x100);
        code.sbc_direct_page(0x1c);
        code.cmp_direct_page(2);
        code.bit_direct_page(1);
        code.bit_absolute(0x190d);
        assert_eq!(
            code.finish().unwrap().bytes,
            [
                0xda, 0xfa, 0xd4, 0x24, 0x98, 0x3b, 0xca, 0x18, 0x38, 0xa5, 0x1a, 0xbf, 0xe4, 0x9d,
                0x10, 0xa2, 5, 0xae, 0x5c, 0x14, 0xac, 0x5f, 0x14, 0x8e, 0x5c, 0x14, 0x64, 0x26,
                0x49, 8, 0, 0x65, 0x22, 0x6d, 0x6a, 0x14, 0xe9, 0, 1, 0xe5, 0x1c, 0xc5, 2, 0x24, 1,
                0x2c, 0x0d, 0x19,
            ]
        );
    }

    #[test]
    fn dynamic_helper_addressing_modes_have_stable_encodings() {
        let mut code = CodeBuilder::new();
        code.ldx_direct_page(0x9d);
        code.trb_absolute(0x0be6);
        code.cpx_immediate8(0x1d);
        code.tsb_absolute(0x0be6);
        assert_eq!(
            code.finish().unwrap().bytes,
            [0xa6, 0x9d, 0x1c, 0xe6, 0x0b, 0xe0, 0x1d, 0x0c, 0xe6, 0x0b]
        );
    }
}
