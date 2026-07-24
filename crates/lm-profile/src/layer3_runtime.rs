//! Independently authored revision-specific Layer 3 runtime fragments.

use crate::{smw_us_v1_layer3_scroll_dispatch_program, smw_us_v1_layer3_scroll_helper_library};
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite};
use lm_snes::{AssembledCode, BranchCondition, CodeBuilder, CodeBuilderError, LongAddressTarget};

/// One verified fragment of the eventual complete Layer 3 runtime template.
///
/// Fragments are deliberately not convertible to [`crate::RevisionPatchTemplate`] on their own:
/// installing an incomplete runtime would make a clean ROM appear supported when it is not.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layer3RuntimeFragment {
    pub name: &'static str,
    pub payload: PatchPayload,
    pub writes: Vec<PatchWrite>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layer3RuntimeMissingComponent {
    MainRuntime,
    ExtendedRuntime,
}

/// Address-independent verified code accumulated toward the complete revision template.
///
/// The explicit nonempty missing-component list prevents this type from masquerading as an
/// installable [`crate::RevisionPatchTemplate`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layer3RuntimeBundle {
    pub payload: PatchPayload,
    pub writes: Vec<PatchWrite>,
    pub missing_components: Vec<Layer3RuntimeMissingComponent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layer3RuntimeBuildError {
    Code(CodeBuilderError),
    ExternalFragmentFixup {
        fragment: usize,
        target_payload: usize,
    },
    Overflow,
}

impl std::fmt::Display for Layer3RuntimeBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Layer 3 runtime construction failed: {self:?}")
    }
}

impl std::error::Error for Layer3RuntimeBuildError {}

impl From<CodeBuilderError> for Layer3RuntimeBuildError {
    fn from(value: CodeBuilderError) -> Self {
        Self::Code(value)
    }
}

/// Builds the verified vanilla-fallback arm used by the SMW US revision-0 main Layer 3 hook.
///
/// The original six-byte sequence is `LDA $1BE3; BEQ +$20; DEC A`. The hook becomes
/// `JSL fragment; BEQ +$1F`; the fragment performs `LDA $1BE3; DEC A; TAX; INX; RTL`.
/// `INX` recreates the original branch's zero condition from the pre-decrement accumulator while
/// leaving the decremented value in A for the fallthrough path.
///
/// This is only one entry arm, not a complete custom Layer 3 runtime.
///
/// # Errors
///
/// Propagates deterministic 65C816 code-construction failures.
pub fn smw_us_v1_layer3_vanilla_fallback_fragment()
-> Result<Layer3RuntimeFragment, Layer3RuntimeBuildError> {
    let mut code = CodeBuilder::new();
    code.lda_absolute(0x1be3);
    code.dec_accumulator();
    code.tax();
    code.inx();
    code.rtl();
    Ok(fragment_from_code(
        "SMW US v1 Layer 3 vanilla mode-dispatch fallback",
        code.finish()?,
        vec![hook(
            0x201f,
            &[0xad, 0xe3, 0x1b, 0xf0, 0x20, 0x3a],
            &[0x22, 0, 0, 0, 0xf0, 0x1f],
        )],
    ))
}

/// Builds the verified first-hook dispatcher and custom-mode setup prefix.
///
/// The generated entry preserves the vanilla fallback and disabled result, then performs every
/// custom-mode state mutation recovered before the payload's first table-driven helper:
/// `$40`, `$0D9D`, `$212C/$212E`, `$146A`, `$146C`, and direct-page `$01`. Its final JML is an
/// intentional cross-payload relocation to payload 1, the still-unimplemented table/helper
/// continuation. Consequently this fragment cannot be merged into the guarded single-payload
/// bundle yet.
///
/// # Errors
///
/// Propagates deterministic 65C816 code-construction failures.
pub fn smw_us_v1_layer3_main_dispatch_setup_fragment()
-> Result<Layer3RuntimeFragment, Layer3RuntimeBuildError> {
    let mut code = CodeBuilder::new();
    let custom = code.label()?;
    let disabled = code.label()?;
    let clear_layer = code.label()?;
    let layer_done = code.label()?;
    let no_screen_update = code.label()?;
    let store_horizontal = code.label()?;

    code.lda_absolute(0x145e);
    code.lsr_accumulator();
    code.branch(BranchCondition::CarrySet, custom);
    code.lda_long(0x007f_c01a);
    code.branch(BranchCondition::Minus, disabled);
    code.lda_absolute(0x1be3);
    code.dec_accumulator();
    code.tax();
    code.inx();
    code.rtl();
    code.bind(disabled)?;
    code.lda_immediate8(0);
    code.rtl();

    code.bind(custom)?;
    code.lda_long(0x007f_c01a);
    code.tax();
    code.and_immediate8(4);
    code.branch(BranchCondition::Equal, clear_layer);
    code.tsb_direct_page(0x40);
    code.branch(BranchCondition::Always, layer_done);
    code.bind(clear_layer)?;
    code.lda_immediate8(4);
    code.trb_direct_page(0x40);
    code.bind(layer_done)?;
    code.txa();
    code.and_immediate8(8);
    code.rep(0x20);
    code.branch(BranchCondition::Equal, no_screen_update);
    code.lda_absolute(0x0d9d);
    code.and_immediate16(0xfffb);
    code.ora_immediate16(0x0400);
    code.sta_absolute(0x0d9d);
    code.sta_absolute(0x212c);
    code.sta_absolute(0x212e);
    code.bind(no_screen_update)?;
    code.txa();
    code.and_immediate16(3);
    code.xba();
    code.lsr_accumulator();
    code.lsr_accumulator();
    code.cmp_immediate16(0x00c0);
    code.branch(BranchCondition::NotEqual, store_horizontal);
    code.lda_immediate16(0x0100);
    code.bind(store_horizontal)?;
    code.sta_absolute(0x146a);
    code.lda_long(0x007f_c01b);
    code.sta_direct_page(1);
    code.sep(0x20);
    code.lda_absolute(0x145e);
    code.and_immediate8(0xf8);
    code.rep(0x20);
    code.asl_accumulator();
    code.asl_accumulator();
    code.cmp_immediate16(0x8000);
    code.ror_accumulator();
    code.sta_absolute(0x146c);
    code.jml(LongAddressTarget {
        payload: 1,
        addend: 0,
    });

    Ok(fragment_from_code(
        "SMW US v1 Layer 3 main dispatch and setup prefix",
        code.finish()?,
        vec![hook(
            0x201f,
            &[0xad, 0xe3, 0x1b, 0xf0, 0x20, 0x3a],
            &[0x22, 0, 0, 0, 0xf0, 0x1f],
        )],
    ))
}

/// Builds the self-contained first-hook runtime from setup, selector dispatch, and helpers.
///
/// The three generated logical payloads are flattened into one allocation. Setup-to-dispatch,
/// dispatch-to-helper, helper-local, and final setup-return relocations are all rebased before the
/// hook can be exposed.
///
/// # Errors
///
/// Rejects code-generation failures, unknown component targets, or arithmetic overflow.
pub fn smw_us_v1_layer3_main_fragment() -> Result<Layer3RuntimeFragment, Layer3RuntimeBuildError> {
    let setup = smw_us_v1_layer3_main_dispatch_setup_fragment()?;
    let helpers = smw_us_v1_layer3_scroll_helper_library()?;
    let dispatch = smw_us_v1_layer3_scroll_dispatch_program(&helpers)?;
    let dispatch_base = setup.payload.bytes.len();
    let helper_base = dispatch_base
        .checked_add(dispatch.payload.bytes.len())
        .ok_or(Layer3RuntimeBuildError::Overflow)?;
    let mut payload = PatchPayload {
        bytes: Vec::new(),
        fixups: Vec::new(),
    };
    append_flattened_component(&mut payload, setup.payload, 0, &[0, dispatch_base], 0)?;
    append_flattened_component(
        &mut payload,
        dispatch.payload,
        dispatch_base,
        &[0, dispatch_base, helper_base],
        1,
    )?;
    append_flattened_component(
        &mut payload,
        helpers.payload,
        helper_base,
        &[helper_base],
        2,
    )?;
    Ok(Layer3RuntimeFragment {
        name: "SMW US v1 Layer 3 self-contained main hook",
        payload,
        writes: setup.writes,
    })
}

/// Builds the complete conditional `$12` initialization dispatch at logical `$002153`.
///
/// Vanilla behavior writes `$06` to direct-page `$12`. A negative expanded runtime state word at
/// `$7FC01A` instead advances the stacked JSL return by three bytes under 16-bit accumulator mode,
/// skipping the vanilla continuation before returning.
///
/// # Errors
///
/// Propagates deterministic 65C816 code-construction failures.
pub fn smw_us_v1_layer3_status_fragment() -> Result<Layer3RuntimeFragment, Layer3RuntimeBuildError>
{
    let mut code = CodeBuilder::new();
    let custom = code.label()?;
    code.lda_long(0x007f_c01a);
    code.branch(BranchCondition::Minus, custom);
    code.lda_immediate8(6);
    code.sta_direct_page(0x12);
    code.rtl();
    code.bind(custom)?;
    code.rep(0x21);
    code.pla();
    code.adc_immediate16(3);
    code.pha();
    code.sep(0x20);
    code.rtl();
    Ok(fragment_from_code(
        "SMW US v1 Layer 3 conditional status initialization",
        code.finish()?,
        vec![hook(0x2153, &[0xa9, 6, 0x85, 0x12], &[0x22, 0, 0, 0])],
    ))
}

/// Builds the complete `$1693/$1694` initialization hook at logical `$0094B6`.
///
/// The routine always clears `$1694`, conditionally selects value `$25` from `$145E` bit 2,
/// stores `$1693`, clears A, and jumps to the revision's injected `RTS` continuation.
///
/// # Errors
///
/// Propagates deterministic 65C816 code-construction failures.
pub fn smw_us_v1_layer3_mode_value_fragment()
-> Result<Layer3RuntimeFragment, Layer3RuntimeBuildError> {
    let mut code = CodeBuilder::new();
    let store = code.label()?;
    code.stz_absolute(0x1694);
    code.lda_absolute(0x145e);
    code.and_immediate8(4);
    code.branch(BranchCondition::Equal, store);
    code.lda_immediate8(0x25);
    code.bind(store)?;
    code.sta_absolute(0x1693);
    code.lda_immediate8(0);
    code.jml_absolute(0x0001_94ba);
    Ok(fragment_from_code(
        "SMW US v1 Layer 3 mode-value initialization",
        code.finish()?,
        vec![hook(
            0x94b6,
            &[0xa9, 0, 0x8d, 0x93, 0x16],
            &[0x5c, 0, 0, 0, 0x60],
        )],
    ))
}

/// Builds the `$1403`/custom-mode dispatcher hooked at logical `$02C40C`.
///
/// Negative `$1931`, or a clear `$145E` bit 0, discards the JSL return and redirects to the
/// revision's `$05C414/$05C494` paths according to `$1403`. A set bit returns normally to the
/// hook's injected RTS.
///
/// # Errors
///
/// Propagates deterministic 65C816 code-construction failures.
pub fn smw_us_v1_layer3_level_dispatch_fragment()
-> Result<Layer3RuntimeFragment, Layer3RuntimeBuildError> {
    let mut code = CodeBuilder::new();
    let redirect = code.label()?;
    let redirect_zero = code.label()?;
    let done = code.label()?;
    code.lda_absolute(0x1931);
    code.branch(BranchCondition::Minus, redirect);
    code.lda_absolute(0x145e);
    code.lsr_accumulator();
    code.branch(BranchCondition::CarrySet, done);
    code.bind(redirect)?;
    code.pla();
    code.pla();
    code.lda_absolute(0x1403);
    code.branch(BranchCondition::Equal, redirect_zero);
    code.jml_absolute(0x0005_c494);
    code.bind(redirect_zero)?;
    code.jml_absolute(0x0005_c414);
    code.bind(done)?;
    code.rtl();
    Ok(fragment_from_code(
        "SMW US v1 Layer 3 level dispatcher",
        code.finish()?,
        vec![hook(
            0x2c40c,
            &[0xad, 0x03, 0x14, 0xf0, 0x03],
            &[0x22, 0, 0, 0, 0x60],
        )],
    ))
}

/// Composes every currently verified SMW US revision-0 fragment into one relocatable payload.
///
/// Hook fixups are rebased to each fragment's generated entry offset. The returned bundle remains
/// explicitly incomplete and has no conversion into an installable revision template.
///
/// # Errors
///
/// Rejects arithmetic overflow or fragment-local fixups that refer to a payload outside the
/// fragment being merged.
pub fn smw_us_v1_verified_layer3_runtime_bundle()
-> Result<Layer3RuntimeBundle, Layer3RuntimeBuildError> {
    compose_fragments(
        vec![
            smw_us_v1_layer3_main_fragment()?,
            smw_us_v1_layer3_status_fragment()?,
            smw_us_v1_layer3_mode_value_fragment()?,
            smw_us_v1_layer3_level_dispatch_fragment()?,
        ],
        vec![
            Layer3RuntimeMissingComponent::MainRuntime,
            Layer3RuntimeMissingComponent::ExtendedRuntime,
        ],
    )
}

fn append_flattened_component(
    output: &mut PatchPayload,
    component: PatchPayload,
    component_base: usize,
    target_bases: &[usize],
    component_index: usize,
) -> Result<(), Layer3RuntimeBuildError> {
    output.bytes.extend_from_slice(&component.bytes);
    for fixup in component.fixups {
        let target_base = target_bases.get(fixup.target_payload).copied().ok_or(
            Layer3RuntimeBuildError::ExternalFragmentFixup {
                fragment: component_index,
                target_payload: fixup.target_payload,
            },
        )?;
        output.fixups.push(PatchFixup {
            offset: component_base
                .checked_add(fixup.offset)
                .ok_or(Layer3RuntimeBuildError::Overflow)?,
            target_payload: 0,
            target_addend: target_base
                .checked_add(fixup.target_addend)
                .ok_or(Layer3RuntimeBuildError::Overflow)?,
            encoding: fixup.encoding,
        });
    }
    Ok(())
}

fn compose_fragments(
    fragments: Vec<Layer3RuntimeFragment>,
    missing_components: Vec<Layer3RuntimeMissingComponent>,
) -> Result<Layer3RuntimeBundle, Layer3RuntimeBuildError> {
    let mut payload = PatchPayload {
        bytes: Vec::new(),
        fixups: Vec::new(),
    };
    let mut writes = Vec::new();
    for (fragment_index, fragment) in fragments.into_iter().enumerate() {
        let base = payload.bytes.len();
        for mut fixup in fragment.payload.fixups {
            if fixup.target_payload != 0 {
                return Err(Layer3RuntimeBuildError::ExternalFragmentFixup {
                    fragment: fragment_index,
                    target_payload: fixup.target_payload,
                });
            }
            fixup.offset = fixup
                .offset
                .checked_add(base)
                .ok_or(Layer3RuntimeBuildError::Overflow)?;
            fixup.target_addend = fixup
                .target_addend
                .checked_add(base)
                .ok_or(Layer3RuntimeBuildError::Overflow)?;
            payload.fixups.push(fixup);
        }
        payload.bytes.extend_from_slice(&fragment.payload.bytes);
        for mut write in fragment.writes {
            for fixup in &mut write.fixups {
                if fixup.target_payload != 0 {
                    return Err(Layer3RuntimeBuildError::ExternalFragmentFixup {
                        fragment: fragment_index,
                        target_payload: fixup.target_payload,
                    });
                }
                fixup.target_addend = fixup
                    .target_addend
                    .checked_add(base)
                    .ok_or(Layer3RuntimeBuildError::Overflow)?;
            }
            writes.push(write);
        }
    }
    Ok(Layer3RuntimeBundle {
        payload,
        writes,
        missing_components,
    })
}

fn fragment_from_code(
    name: &'static str,
    assembled: AssembledCode,
    writes: Vec<PatchWrite>,
) -> Layer3RuntimeFragment {
    let fixups = assembled
        .long_address_fixups
        .into_iter()
        .map(|fixup| PatchFixup {
            offset: fixup.offset,
            target_payload: fixup.target.payload,
            target_addend: fixup.target.addend,
            encoding: PatchFixupEncoding::Long24,
        })
        .collect();
    Layer3RuntimeFragment {
        name,
        payload: PatchPayload {
            bytes: assembled.bytes,
            fixups,
        },
        writes,
    }
}

fn hook(offset: usize, expected: &[u8], replacement: &[u8]) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: vec![PatchFixup {
            offset: 1,
            target_payload: 0,
            target_addend: 0,
            encoding: PatchFixupEncoding::Long24,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn fallback_code_and_hook_contract_match_instruction_level_evidence() {
        let fragment = smw_us_v1_layer3_vanilla_fallback_fragment().unwrap();
        assert_eq!(
            fragment.payload.bytes,
            [0xad, 0xe3, 0x1b, 0x3a, 0xaa, 0xe8, 0x6b]
        );
        assert!(fragment.payload.fixups.is_empty());
        assert_eq!(fragment.writes[0].offset, 0x201f);
        assert_eq!(
            fragment.writes[0].expected,
            [0xad, 0xe3, 0x1b, 0xf0, 0x20, 0x3a]
        );
        assert_eq!(fragment.writes[0].replacement, [0x22, 0, 0, 0, 0xf0, 0x1f]);
        assert_eq!(
            fragment.writes[0].fixups,
            [PatchFixup {
                offset: 1,
                target_payload: 0,
                target_addend: 0,
                encoding: PatchFixupEncoding::Long24,
            }]
        );
    }

    #[test]
    fn retained_wine_rom_proves_hook_and_fallback_sequence() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive");
        let before = fs::read(fixture.join("before.smc")).unwrap();
        let after = fs::read(fixture.join("after.smc")).unwrap();
        let fragment = smw_us_v1_layer3_vanilla_fallback_fragment().unwrap();
        let raw_hook = 0x201f + 0x200;
        assert_eq!(
            &before[raw_hook..raw_hook + fragment.writes[0].expected.len()],
            fragment.writes[0].expected
        );
        assert_eq!(after[raw_hook], 0x22);
        assert_eq!(&after[raw_hook + 4..raw_hook + 6], &[0xf0, 0x1f]);
        let target = u32::from(after[raw_hook + 1])
            | (u32::from(after[raw_hook + 2]) << 8)
            | (u32::from(after[raw_hook + 3]) << 16);
        let target_pc =
            usize::try_from(((target >> 16) & 0x7f) * 0x8000 + (target & 0x7fff)).unwrap();
        let payload = &after[target_pc + 0x200..target_pc + 0x200 + 0x4c0];
        assert_eq!(
            payload
                .windows(fragment.payload.bytes.len())
                .position(|window| window == fragment.payload.bytes),
            Some(12)
        );
    }

    #[test]
    fn generated_main_dispatch_setup_matches_retained_runtime_until_helper_boundary() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let after = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let installed_payload = &after[0x81a0d + 0x200..0x81a0d + 0x200 + 0x4c0];
        let fragment = smw_us_v1_layer3_main_dispatch_setup_fragment().unwrap();

        // The independently generated prefix ends where the installed runtime begins its first
        // table-driven helper. Its JML operand remains an explicit external continuation fixup.
        assert_eq!(&fragment.payload.bytes[..0x6a], &installed_payload[..0x6a]);
        assert_eq!(&fragment.payload.bytes[0x6a..], &[0x5c, 0, 0, 0]);
        assert_eq!(
            fragment.payload.fixups,
            [PatchFixup {
                offset: 0x6b,
                target_payload: 1,
                target_addend: 0,
                encoding: PatchFixupEncoding::Long24,
            }]
        );
        assert_eq!(fragment.writes[0].offset, 0x201f);
    }

    #[test]
    fn complete_small_hook_fragments_match_recovered_runtime_entries() {
        let status = smw_us_v1_layer3_status_fragment().unwrap();
        assert_eq!(
            status.payload.bytes,
            [
                0xaf, 0x1a, 0xc0, 0x7f, 0x30, 5, 0xa9, 6, 0x85, 0x12, 0x6b, 0xc2, 0x21, 0x68, 0x69,
                3, 0, 0x48, 0xe2, 0x20, 0x6b
            ]
        );
        assert!(status.payload.fixups.is_empty());
        assert_eq!(status.writes[0].offset, 0x2153);
        assert_eq!(status.writes[0].expected, [0xa9, 6, 0x85, 0x12]);

        let mode = smw_us_v1_layer3_mode_value_fragment().unwrap();
        assert_eq!(
            mode.payload.bytes,
            [
                0x9c, 0x94, 0x16, 0xad, 0x5e, 0x14, 0x29, 4, 0xf0, 2, 0xa9, 0x25, 0x8d, 0x93, 0x16,
                0xa9, 0, 0x5c, 0xba, 0x94, 1
            ]
        );
        assert_eq!(mode.writes[0].offset, 0x94b6);
        assert_eq!(mode.writes[0].expected, [0xa9, 0, 0x8d, 0x93, 0x16]);
        assert_eq!(mode.writes[0].replacement, [0x5c, 0, 0, 0, 0x60]);

        let dispatch = smw_us_v1_layer3_level_dispatch_fragment().unwrap();
        assert_eq!(dispatch.writes[0].offset, 0x2c40c);
        assert_eq!(dispatch.writes[0].expected, [0xad, 0x03, 0x14, 0xf0, 0x03]);
        assert!(dispatch.payload.fixups.is_empty());
    }

    #[test]
    fn verified_bundle_rebases_all_four_hook_entries_but_stays_incomplete() {
        let bundle = smw_us_v1_verified_layer3_runtime_bundle().unwrap();
        assert_eq!(
            bundle
                .writes
                .iter()
                .map(|write| write.offset)
                .collect::<Vec<_>>(),
            [0x201f, 0x2153, 0x94b6, 0x2c40c]
        );
        let main_len = smw_us_v1_layer3_main_fragment()
            .unwrap()
            .payload
            .bytes
            .len();
        let status_len = smw_us_v1_layer3_status_fragment()
            .unwrap()
            .payload
            .bytes
            .len();
        let mode_len = smw_us_v1_layer3_mode_value_fragment()
            .unwrap()
            .payload
            .bytes
            .len();
        assert_eq!(
            bundle
                .writes
                .iter()
                .map(|write| write.fixups[0].target_addend)
                .collect::<Vec<_>>(),
            [
                0,
                main_len,
                main_len + status_len,
                main_len + status_len + mode_len
            ]
        );
        assert_eq!(
            bundle.missing_components,
            [
                Layer3RuntimeMissingComponent::MainRuntime,
                Layer3RuntimeMissingComponent::ExtendedRuntime,
            ]
        );
        assert!(bundle.payload.fixups.iter().all(|fixup| {
            fixup.target_payload == 0
                && fixup.offset + 3 <= bundle.payload.bytes.len()
                && fixup.target_addend < bundle.payload.bytes.len()
        }));
    }
}
