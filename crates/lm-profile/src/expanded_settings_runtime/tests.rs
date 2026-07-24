use super::*;
use std::{fs, path::PathBuf};

#[test]
fn fixed_runtime_relocations_are_the_five_recovered_descriptor_sites() {
    for (offset, relocation) in SMW_US_V1_EXPANDED_HEADER_FIXED_RUNTIME_RELOCATIONS
        .iter()
        .enumerate()
    {
        assert_eq!(relocation.site_descriptor_index, 0x35 + offset);
        assert_eq!(relocation.site_addend, 0x6c);
        assert_eq!(
            relocation.target,
            ExpandedSettingsRelocationTarget::DescriptorEntry {
                index: 0x70,
                addend: 0
            }
        );
    }
}

#[test]
fn allocation_targets_resolve_to_recovered_subregions() {
    assert_eq!(
        ExpandedSettingsRelocationTarget::AllocationBase.allocation_addend(),
        Some(0)
    );
    assert_eq!(
        ExpandedSettingsRelocationTarget::RecordTable.allocation_addend(),
        Some(0x2d00)
    );
    assert_eq!(
        ExpandedSettingsRelocationTarget::SpecialRecord.allocation_addend(),
        Some(0x6d00)
    );
    assert_eq!(
        ExpandedSettingsRelocationTarget::DescriptorEntry {
            index: 0x70,
            addend: 0
        }
        .allocation_addend(),
        None
    );
}

#[test]
fn all_embedded_templates_match_retained_wine_runtime_outside_relocations() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
    let installed =
        fs::read(root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"))
            .unwrap();
    // The active descriptor stores physical offsets because this retained ROM has a copier
    // header. These values were read from the live Wine process after the fixture was loaded.
    let physical_destinations = [
        0x7f360, 0x7f980, 0x7f9f0, 0x7fa40, 0x7faa0, 0x7fb00, 0x7fbc0, 0x7fbe0, 0x7fcb0, 0x7fcf0,
        0x7fd20, 0x7ff80,
    ];
    for (block, destination) in SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
        .iter()
        .copied()
        .zip(physical_destinations)
    {
        let source = usize::try_from(block.embedded_template_va - 0x0040_0000).unwrap();
        verify_expanded_settings_runtime_block(
            block,
            &executable[source..source + block.len],
            &installed[destination..destination + block.len],
        )
        .unwrap();
    }
}

#[test]
fn runtime_verifier_rejects_unexplained_changes_and_bad_metadata() {
    let block = runtime_block(0, 0, 4, &[RuntimeMutableSpan { offset: 1, len: 1 }]);
    verify_expanded_settings_runtime_block(block, &[1, 2, 3, 4], &[1, 9, 3, 4]).unwrap();
    assert_eq!(
        verify_expanded_settings_runtime_block(block, &[1, 2, 3, 4], &[0, 9, 3, 4]),
        Err(RuntimeBlockVerificationError::UnexpectedDifference {
            offset: 0,
            template: 1,
            installed: 0
        })
    );
    let invalid = runtime_block(0, 0, 4, &[RuntimeMutableSpan { offset: 3, len: 2 }]);
    assert_eq!(
        verify_expanded_settings_runtime_block(invalid, &[0; 4], &[0; 4]),
        Err(RuntimeBlockVerificationError::MutableSpanOutOfBounds { offset: 3, len: 2 })
    );
}

#[test]
fn generated_index_restore_block_matches_recovered_relocation_free_template() {
    let payload = smw_us_v1_expanded_settings_index_restore_block().unwrap();
    assert_eq!(payload.bytes.len(), 0x20);
    assert_eq!(
        &payload.bytes[..11],
        &[
            0x08, 0x20, 0xf7, 0xf9, 0x28, 0xae, 0xc6, 0x13, 0xa9, 0x18, 0x6b
        ]
    );
    assert!(payload.bytes[11..].iter().all(|byte| *byte == 0xff));
    assert!(payload.fixups.is_empty());
    assert_eq!(
        payload.bytes.len(),
        SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS[6].len
    );
    assert_eq!(
        SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS[6].descriptor_index,
        0x213
    );
}

#[test]
fn generated_state_compare_block_matches_recovered_relocation_free_template() {
    let payload = smw_us_v1_expanded_settings_state_compare_block().unwrap();
    assert_eq!(payload.bytes.len(), 0x30);
    assert_eq!(
        &payload.bytes[..17],
        &[
            0xad, 0x11, 0x1f, 0xcd, 0x12, 0x1f, 0xf0, 0x05, 0xa9, 0x0c, 0x8d, 0x00, 0x01, 0x5c,
            0xf2, 0xdb, 0x05
        ]
    );
    assert!(payload.bytes[17..].iter().all(|byte| *byte == 0xff));
    assert_eq!(
        SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS[9].descriptor_index,
        0x219
    );
}

#[test]
fn generated_allocation_load_and_special_record_blocks_match_both_oracles() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
    let installed =
        fs::read(root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"))
            .unwrap();

    let allocation_template = smw_us_v1_expanded_settings_allocation_load_block(0x00_8000).unwrap();
    let allocation_source = 0x005b_59d8 - 0x0040_0000;
    assert_eq!(
        allocation_template.bytes,
        executable[allocation_source..allocation_source + 0x50]
    );
    let allocation_installed =
        smw_us_v1_expanded_settings_allocation_load_block(0x11_8000).unwrap();
    assert_eq!(allocation_installed.bytes, installed[0x7fa40..0x7fa90]);

    let special_template = smw_us_v1_expanded_settings_special_record_block(0x00_8000).unwrap();
    let special_source = 0x005b_5bf8 - 0x0040_0000;
    assert_eq!(
        special_template.bytes,
        executable[special_source..special_source + 0x40]
    );
    let special_installed = smw_us_v1_expanded_settings_special_record_block(0x11_ed00).unwrap();
    assert_eq!(special_installed.bytes, installed[0x7fcb0..0x7fcf0]);
    assert_eq!(
        smw_us_v1_expanded_settings_special_record_block(0x0100_0000),
        Err(ExpandedSettingsRuntimeBuildError::AddressOutOfRange(
            0x0100_0000
        ))
    );
    for descriptor in SMW_US_V1_GENERATED_EXPANDED_SETTINGS_RUNTIME_BLOCKS {
        assert!(
            SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS
                .iter()
                .any(|block| block.descriptor_index == descriptor)
        );
    }
}

#[test]
fn generated_reset_block_matches_both_relocation_free_oracles() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
    let installed =
        fs::read(root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"))
            .unwrap();
    let payload = smw_us_v1_expanded_settings_reset_block().unwrap();
    let source = 0x005b_5918 - 0x0040_0000;
    assert_eq!(payload.bytes, executable[source..source + 0x60]);
    assert_eq!(payload.bytes, installed[0x7f980..0x7f9e0]);
}

#[test]
fn generated_selector_dispatch_matches_both_relocation_free_oracles() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
    let installed =
        fs::read(root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"))
            .unwrap();
    let payload = smw_us_v1_expanded_settings_selector_dispatch_block().unwrap();
    let source = 0x005b_5880 - 0x0040_0000;
    assert_eq!(payload.bytes, executable[source..source + 0x90]);
    assert_eq!(payload.bytes, installed[0x7f360..0x7f3f0]);
}

#[test]
fn generated_indexed_scratch_block_matches_template_and_resolved_oracle() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
    let installed =
        fs::read(root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"))
            .unwrap();
    let template = smw_us_v1_expanded_settings_indexed_scratch_block(0x0f_f200, 0x00_f900).unwrap();
    let source = 0x005b_5a30 - 0x0040_0000;
    assert_eq!(template.bytes, executable[source..source + 0x50]);
    let resolved = smw_us_v1_expanded_settings_indexed_scratch_block(0x0f_f200, 0x0f_f900).unwrap();
    assert_eq!(resolved.bytes, installed[0x7faa0..0x7faf0]);
}

#[test]
fn generated_record_select_block_matches_template_and_resolved_oracle() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
    let installed =
        fs::read(root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"))
            .unwrap();
    let template = smw_us_v1_expanded_settings_record_select_block(0x00_8000).unwrap();
    let source = 0x005b_5980 - 0x0040_0000;
    assert_eq!(template.bytes, executable[source..source + 0x50]);
    let resolved = smw_us_v1_expanded_settings_record_select_block(0x11_ad00).unwrap();
    assert_eq!(resolved.bytes, installed[0x7f9f0..0x7fa40]);
}

#[test]
fn generated_pointer_dispatch_matches_template_and_resolved_oracle() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
    let installed =
        fs::read(root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"))
            .unwrap();
    let template = smw_us_v1_expanded_settings_pointer_dispatch_block(0x00_8000).unwrap();
    let source = 0x005b_5a88 - 0x0040_0000;
    assert_eq!(template.bytes, executable[source..source + 0x70]);
    let resolved = smw_us_v1_expanded_settings_pointer_dispatch_block(0x11_8000).unwrap();
    assert_eq!(resolved.bytes, installed[0x7fb00..0x7fb70]);
}

#[test]
fn generated_dma_block_matches_configured_template_and_installed_oracles() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
    let installed =
        fs::read(root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"))
            .unwrap();
    let template =
        smw_us_v1_expanded_settings_dma_block(ExpandedSettingsEntryContinuation::Return).unwrap();
    let source = 0x005b_5b20 - 0x0040_0000;
    assert_eq!(template.bytes, executable[source..source + 0xd0]);
    let resolved =
        smw_us_v1_expanded_settings_dma_block(ExpandedSettingsEntryContinuation::Continue).unwrap();
    assert_eq!(resolved.bytes, installed[0x7fbe0..0x7fcb0]);
}

#[test]
fn generated_field_runtime_matches_both_relocation_free_oracles() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
    let installed =
        fs::read(root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"))
            .unwrap();
    let payload = smw_us_v1_expanded_settings_field_runtime_block().unwrap();
    let source = 0x005b_5e98 - 0x0040_0000;
    assert_eq!(payload.bytes, executable[source..source + 0x150]);
    assert_eq!(payload.bytes, installed[0x7ff80..0x800d0]);
}
