use super::*;

#[test]
fn manifest_round_trips_delimiters_and_unicode() {
    let manifest = OracleManifest {
        case_id: "level-105-object-move".into(),
        lunar_magic_version: "3.40".into(),
        input_sha256: "aa".repeat(32),
        output_sha256: "bb".repeat(32),
        operation: Operation {
            name: "save\nlevel".into(),
            arguments: vec![("level".into(), "105=雪".into())],
        },
        changed_ranges: vec![0x100..0x108, 0x7fdc..0x7fe0],
        decoded_before: "{\"x\":1}".into(),
        decoded_after: "{\"x\":2}".into(),
        owned_allocations_before: vec![0x200..0x220, 0x400..0x410],
        owned_allocations_after: vec![0x300..0x320, 0x400..0x410],
        warnings: vec!["expanded ROM".into()],
        errors: Vec::new(),
    };
    assert_eq!(
        OracleManifest::from_text(&manifest.to_text()).unwrap(),
        manifest
    );
}

#[test]
fn malformed_manifest_is_rejected() {
    assert_eq!(
        OracleManifest::from_text("wrong\n").unwrap_err(),
        ManifestError::InvalidHeader
    );
}

#[test]
fn duplicate_scalar_fields_are_rejected_instead_of_overwritten() {
    let text = "LMORACLE1\ncase_id=61\ncase_id=62\n";
    assert_eq!(
        OracleManifest::from_text(text),
        Err(ManifestError::DuplicateField("case_id"))
    );
}

#[test]
fn parser_limits_apply_before_unbounded_collection_or_value_growth() {
    let limits = ParseLimits {
        text_bytes: 1024,
        value_bytes: 2,
        records: 2,
    };
    assert_eq!(
        OracleManifest::from_text_with_limits("LMORACLE1\ncase_id=616263\n", limits),
        Err(ManifestError::ValueTooLarge(3))
    );
    assert_eq!(
        OracleManifest::from_text_with_limits("LMORACLE1\nwarning=\nwarning=\nwarning=\n", limits),
        Err(ManifestError::TooManyRecords(3))
    );
    assert_eq!(
        OracleManifest::from_text_with_limits(
            "LMORACLE1\n",
            ParseLimits {
                text_bytes: 4,
                ..limits
            }
        ),
        Err(ManifestError::InputTooLarge(10))
    );
}

#[test]
fn semantic_identity_and_argument_names_are_unambiguous() {
    let valid = OracleManifest {
        case_id: "case".into(),
        lunar_magic_version: "3.40".into(),
        input_sha256: "00".repeat(32),
        output_sha256: "11".repeat(32),
        operation: Operation {
            name: "level-save".into(),
            arguments: vec![("mapper".into(), "lorom".into())],
        },
        changed_ranges: Vec::new(),
        decoded_before: String::new(),
        decoded_after: String::new(),
        owned_allocations_before: Vec::new(),
        owned_allocations_after: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };
    assert!(valid.validate().is_ok());
    for (invalid, expected) in [
        (
            {
                let mut value = valid.clone();
                value.case_id.clear();
                value
            },
            ManifestError::EmptyField("case_id"),
        ),
        (
            {
                let mut value = valid.clone();
                value.input_sha256 = "not-a-hash".into();
                value
            },
            ManifestError::InvalidSha256("input_sha256"),
        ),
        (
            {
                let mut value = valid.clone();
                value.input_sha256 = "AA".repeat(32);
                value
            },
            ManifestError::InvalidSha256("input_sha256"),
        ),
        (
            {
                let mut value = valid.clone();
                value
                    .operation
                    .arguments
                    .push(("mapper".into(), "sa1".into()));
                value
            },
            ManifestError::DuplicateArgumentName("mapper".into()),
        ),
        (
            {
                let mut value = valid.clone();
                value.operation.arguments[0].0.clear();
                value
            },
            ManifestError::EmptyArgumentName(0),
        ),
    ] {
        assert_eq!(invalid.validate(), Err(expected));
        assert_eq!(
            OracleManifest::from_text(&invalid.to_text()),
            Err(invalid.validate().unwrap_err())
        );
    }
}

#[test]
fn range_claims_must_be_nonempty_sorted_and_disjoint() {
    let mut manifest = OracleManifest {
        case_id: "case".into(),
        lunar_magic_version: "3.40".into(),
        input_sha256: "00".repeat(32),
        output_sha256: "11".repeat(32),
        operation: Operation {
            name: "edit".into(),
            arguments: Vec::new(),
        },
        changed_ranges: vec![1..2, 2..4],
        decoded_before: String::new(),
        decoded_after: String::new(),
        owned_allocations_before: std::iter::once(8..12).collect(),
        owned_allocations_after: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };
    assert!(manifest.validate().is_ok());
    manifest.changed_ranges = std::iter::once(2..2).collect();
    assert_eq!(
        manifest.validate(),
        Err(ManifestError::InvalidRange {
            field: "changed_range",
            index: 0,
            start: 2,
            end: 2,
        })
    );
    manifest.changed_ranges = vec![4..8, 2..3];
    assert_eq!(
        manifest.validate(),
        Err(ManifestError::NonCanonicalRanges {
            field: "changed_range",
            index: 1,
        })
    );
    manifest.changed_ranges.clear();
    manifest.owned_allocations_before = vec![8..12, 10..16];
    assert_eq!(
        OracleManifest::from_text(&manifest.to_text()),
        Err(ManifestError::NonCanonicalRanges {
            field: "owned_before",
            index: 1,
        })
    );
}
