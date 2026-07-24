use super::*;

#[test]
fn parses_address() {
    assert_eq!(
        parse_from(
            &vec!["address", "sa1", "pc-to-snes", "200000"]
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>()
        )
        .unwrap(),
        Command::Address {
            mapper: Mapper::Sa1,
            direction: Direction::PcToSnes,
            value: 0x0020_0000
        }
    );
}
#[test]
fn parses_terminated_and_sized_rle_workflows() {
    let parse = |values: &[&str]| {
        parse_from(&values.iter().map(OsString::from).collect::<Vec<_>>()).unwrap()
    };
    assert_eq!(
        parse(&["codec", "rle-sized-encode", "raw.bin", "packed.bin"]),
        Command::Codec {
            operation: CodecOperation::RleSizedEncode,
            input: "raw.bin".into(),
            output: "packed.bin".into(),
        }
    );
    assert_eq!(
        parse(&["codec", "rle-sized-decode", "packed.bin", "raw.bin", "1000",]),
        Command::CodecSizedRleDecode {
            input: "packed.bin".into(),
            output: "raw.bin".into(),
            expected_len: 0x1000,
        }
    );
}

#[test]
fn parses_recovered_lz3_workflows() {
    for (name, operation) in [
        ("lz3-decode", CodecOperation::Lz3Decode),
        ("lz3-encode", CodecOperation::Lz3Encode),
    ] {
        assert_eq!(
            parse_from(&[
                "codec".into(),
                name.into(),
                "input.bin".into(),
                "output.bin".into(),
            ])
            .unwrap(),
            Command::Codec {
                operation,
                input: "input.bin".into(),
                output: "output.bin".into(),
            }
        );
    }
}

#[test]
fn parses_semantic_codec_observation() {
    assert_eq!(
        parse_from(&[
            "codec-observe".into(),
            "lz3".into(),
            "packed.bin".into(),
            "10000".into(),
            "decoded.obs".into(),
        ])
        .unwrap(),
        Command::CodecObserve {
            kind: lm_oracle::CodecObservationKind::Lz3,
            input: "packed.bin".into(),
            output_bound: 0x1_0000,
            observation: "decoded.obs".into(),
        }
    );
    assert_eq!(
        parse_from(&[
            "codec-observe".into(),
            "rle-sized".into(),
            "packed.rle".into(),
            "80".into(),
            "decoded.obs".into(),
        ])
        .unwrap(),
        Command::CodecObserve {
            kind: lm_oracle::CodecObservationKind::RleSized,
            input: "packed.rle".into(),
            output_bound: 0x80,
            observation: "decoded.obs".into(),
        }
    );
    assert!(
        parse_from(&[
            "codec-observe".into(),
            "lz4".into(),
            "packed.bin".into(),
            "100".into(),
            "decoded.obs".into(),
        ])
        .is_err()
    );
}

#[test]
fn parses_generic_planar_conversions() {
    assert_eq!(
        parse_from(&[
            "planar".into(),
            "decode".into(),
            "3".into(),
            "tiles.3bpp".into(),
            "pixels.idx".into(),
        ])
        .unwrap(),
        Command::Planar {
            operation: crate::command_types::PlanarOperation::Decode,
            bits_per_pixel: 3,
            input: "tiles.3bpp".into(),
            output: "pixels.idx".into(),
        }
    );
    assert!(
        parse_from(&[
            "planar".into(),
            "convert".into(),
            "3".into(),
            "input.bin".into(),
            "output.bin".into(),
        ])
        .is_err()
    );
}

#[test]
fn parses_copy_on_write_rom_expansion() {
    assert_eq!(
        parse_from(&[
            "rom-expand".into(),
            "input.smc".into(),
            "expanded.smc".into(),
            "lorom".into(),
            "10000".into(),
            "ff".into(),
        ])
        .unwrap(),
        Command::RomExpand {
            input: "input.smc".into(),
            output: "expanded.smc".into(),
            mapper: Mapper::LoRom,
            target_logical_len: 0x1_0000,
            fill: 0xff,
        }
    );
}
#[test]
fn parses_variance_quantization_workflow() {
    assert_eq!(
        parse_from(&[
            "quantize-rgb24".into(),
            "pixels.rgb".into(),
            "10".into(),
            "palette.lmpal".into(),
            "pixels.idx".into(),
        ])
        .unwrap(),
        Command::QuantizeRgb24 {
            input: "pixels.rgb".into(),
            maximum_colors: 0x10,
            palette_output: "palette.lmpal".into(),
            indices_output: "pixels.idx".into(),
        }
    );
}

#[test]
fn parses_indexed_map16_import_workflow() {
    assert_eq!(
        parse_from(&[
            "import-indexed-map16".into(),
            "page.idx".into(),
            "base.lmgfx".into(),
            "base.occ".into(),
            "3".into(),
            "130".into(),
            "20".into(),
            "result.lmgfx".into(),
            "result.occ".into(),
            "result.map16".into(),
        ])
        .unwrap(),
        Command::ImportIndexedMap16 {
            indices: "page.idx".into(),
            graphics: "base.lmgfx".into(),
            occupancy: "base.occ".into(),
            palette_row: 3,
            acts_like: 0x130,
            source_page: 0x20,
            graphics_output: "result.lmgfx".into(),
            occupancy_output: "result.occ".into(),
            page_output: "result.map16".into(),
        }
    );
}

#[test]
fn parses_end_to_end_rgb_map16_import_workflow() {
    let command = parse_from(&[
        "import-rgb-map16".into(),
        "page.rgb".into(),
        "base.lmpal".into(),
        "palette.access".into(),
        "base.lmgfx".into(),
        "base.occ".into(),
        "2".into(),
        "130".into(),
        "20".into(),
        "result.lmpal".into(),
        "result.lmgfx".into(),
        "result.occ".into(),
        "result.map16".into(),
    ])
    .unwrap();
    assert!(matches!(
        command,
        Command::ImportRgbMap16(RgbMap16ImportCommand {
            palette_row: 2,
            acts_like: 0x130,
            source_page: 0x20,
            ..
        })
    ));
}

#[test]
fn parses_end_to_end_rgba_map16_import_workflow() {
    let command = parse_from(&[
        "import-rgba-map16".into(),
        "page.rgba".into(),
        "base.lmpal".into(),
        "palette.access".into(),
        "base.lmgfx".into(),
        "base.occ".into(),
        "2".into(),
        "130".into(),
        "20".into(),
        "result.lmpal".into(),
        "result.lmgfx".into(),
        "result.occ".into(),
        "result.map16".into(),
    ])
    .unwrap();
    assert!(matches!(
        command,
        Command::ImportRgbaMap16(RgbaMap16ImportCommand {
            palette_row: 2,
            acts_like: 0x130,
            source_page: 0x20,
            ..
        })
    ));
}

#[test]
fn parses_end_to_end_png_map16_import_workflow() {
    let command = parse_from(&[
        "import-png-map16".into(),
        "page.png".into(),
        "base.lmpal".into(),
        "palette.access".into(),
        "base.lmgfx".into(),
        "base.occ".into(),
        "2".into(),
        "130".into(),
        "20".into(),
        "result.lmpal".into(),
        "result.lmgfx".into(),
        "result.occ".into(),
        "result.map16".into(),
    ])
    .unwrap();
    assert!(matches!(
        command,
        Command::ImportPngMap16(PngMap16ImportCommand {
            palette_row: 2,
            acts_like: 0x130,
            source_page: 0x20,
            ..
        })
    ));
}

#[test]
fn parses_exanimation_frame_edit_workflow() {
    assert_eq!(
        parse_from(&[
            "exanimation-frames".into(),
            "input.lmexan".into(),
            "modes.bin".into(),
            "40".into(),
            "2".into(),
            "edits.txt".into(),
            "output.lmexan".into(),
        ])
        .unwrap(),
        Command::EditExAnimationFrames {
            input: "input.lmexan".into(),
            size_modes: "modes.bin".into(),
            maximum_records: 0x40,
            record: 2,
            edits: "edits.txt".into(),
            output: "output.lmexan".into(),
        }
    );
}

#[test]
fn parses_safe_patch_workflow() {
    assert_eq!(
        parse_from(
            &vec!["patch", "in.smc", "out.smc", "0x20", "deadBEEF"]
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>()
        )
        .unwrap(),
        Command::Patch {
            input: "in.smc".into(),
            output: "out.smc".into(),
            offset: 0x20,
            bytes: vec![0xde, 0xad, 0xbe, 0xef],
        }
    );
}

#[test]
fn parses_ips_create_and_apply_workflows() {
    assert_eq!(
        parse_from(
            &vec!["ips-create", "before.smc", "after.smc", "change.ips"]
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>()
        )
        .unwrap(),
        Command::IpsCreate {
            before: "before.smc".into(),
            after: "after.smc".into(),
            output: "change.ips".into(),
        }
    );
    assert_eq!(
        parse_from(
            &vec!["ips-apply", "clean.smc", "change.ips", "patched.smc"]
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>()
        )
        .unwrap(),
        Command::IpsApply {
            source: "clean.smc".into(),
            patch: "change.ips".into(),
            output: "patched.smc".into(),
        }
    );
}

#[test]
fn parses_copier_header_conversion_workflows() {
    let parse = |values: Vec<&str>| {
        parse_from(
            &values
                .into_iter()
                .map(Into::into)
                .collect::<Vec<OsString>>(),
        )
        .unwrap()
    };
    assert_eq!(
        parse(vec!["copier-header-add", "plain.smc", "headered.smc", "ff"]),
        Command::CopierHeaderAdd {
            input: "plain.smc".into(),
            output: "headered.smc".into(),
            fill: 0xff,
        }
    );
    assert_eq!(
        parse(vec!["copier-header-remove", "headered.smc", "plain.smc"]),
        Command::CopierHeaderRemove {
            input: "headered.smc".into(),
            output: "plain.smc".into(),
        }
    );
}

#[test]
fn parses_oracle_semantic_observations() {
    assert_eq!(
        parse_from(
            &vec![
                "oracle-verify",
                "case.manifest",
                "before.smc",
                "after.smc",
                "before.obs",
                "after.obs",
            ]
            .into_iter()
            .map(Into::into)
            .collect::<Vec<OsString>>()
        )
        .unwrap(),
        Command::OracleVerify {
            manifest: "case.manifest".into(),
            before: "before.smc".into(),
            after: "after.smc".into(),
            observations: Some(("before.obs".into(), "after.obs".into())),
        }
    );
}

#[test]
fn parses_recursive_oracle_suite_verification() {
    assert_eq!(
        parse_from(&[
            OsString::from("oracle-verify-suite"),
            OsString::from("fixtures")
        ])
        .unwrap(),
        Command::OracleVerifySuite {
            root: "fixtures".into(),
        }
    );
}

#[test]
fn parses_oracle_corpus_coverage_requirements() {
    assert_eq!(
        parse_from(&[
            "oracle-coverage".into(),
            "fixtures".into(),
            "version:3.40".into(),
            "operation:level-save".into(),
            "argument:mapper=sa1".into(),
        ])
        .unwrap(),
        Command::OracleCoverage {
            root: "fixtures".into(),
            requirements: vec![
                "version:3.40".into(),
                "operation:level-save".into(),
                "argument:mapper=sa1".into(),
            ],
        }
    );
}

#[test]
fn parses_combined_oracle_release_gate() {
    assert_eq!(
        parse_from(&[
            "oracle-release-gate".into(),
            "fixtures".into(),
            "version:3.40".into(),
            "operation:level-save".into(),
            "argument:mapper=lorom".into(),
            "argument:header=headerless".into(),
            "argument:fixture_family=clean".into(),
        ])
        .unwrap(),
        Command::OracleReleaseGate {
            root: "fixtures".into(),
            requirements: vec![
                "version:3.40".into(),
                "operation:level-save".into(),
                "argument:mapper=lorom".into(),
                "argument:header=headerless".into(),
                "argument:fixture_family=clean".into(),
            ],
        }
    );
}

#[test]
fn parses_oracle_capture_with_variable_operation_arguments() {
    assert_eq!(
        parse_from(
            &vec![
                "oracle-capture",
                "level-105-move",
                "3.63",
                "move-object",
                "before.smc",
                "after.smc",
                "before.obs",
                "after.obs",
                "changed-rats",
                "case.manifest",
                "level=105",
                "object=7",
            ]
            .into_iter()
            .map(Into::into)
            .collect::<Vec<OsString>>(),
        )
        .unwrap(),
        Command::OracleCapture(OracleCaptureCommand {
            case_id: "level-105-move".into(),
            lunar_magic_version: "3.63".into(),
            operation: "move-object".into(),
            before: "before.smc".into(),
            after: "after.smc".into(),
            decoded_before: "before.obs".into(),
            decoded_after: "after.obs".into(),
            ownership: OracleOwnership::ChangedRats,
            output: "case.manifest".into(),
            arguments: vec![
                ("level".into(), "105".into()),
                ("object".into(), "7".into()),
            ],
        })
    );
    assert!(
        parse_from(
            &vec![
                "oracle-capture",
                "case",
                "3.63",
                "edit",
                "before.smc",
                "after.smc",
                "before.obs",
                "after.obs",
                "automatic",
                "case.manifest",
            ]
            .into_iter()
            .map(Into::into)
            .collect::<Vec<OsString>>()
        )
        .is_err()
    );
}
