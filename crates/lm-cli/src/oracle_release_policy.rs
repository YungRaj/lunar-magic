use lm_oracle::OracleManifest;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const RELEASE_OPERATIONS: [&str; 5] = [
    "open-save",
    "render-level",
    "level-edit",
    "lunar-magic-reopen",
    "emulator-boot",
];
pub const RELEASE_COMPATIBILITY_OPERATIONS: [&str; 2] = ["lunar-magic-reopen", "emulator-boot"];
pub const RELEASE_ARGUMENTS: [&str; 7] = [
    "mapper",
    "header",
    "region",
    "revision",
    "rom_size",
    "fixture_family",
    "subsystem",
];
pub const RELEASE_SUBSYSTEMS: [&str; 12] = [
    "rom",
    "codecs",
    "rats",
    "levels",
    "map16",
    "sprites",
    "graphics",
    "palettes",
    "exanimation",
    "overworld",
    "rendering",
    "application",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReleaseCaseMetadataError {
    UnsupportedOperation(String),
    MissingArguments {
        operation: String,
        names: Vec<&'static str>,
    },
    EmptyArgument {
        operation: String,
        name: &'static str,
    },
    DuplicateArgument {
        operation: String,
        name: String,
    },
    UnsupportedSubsystem(String),
}

impl fmt::Display for ReleaseCaseMetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid release-case provenance: {self:?}")
    }
}

impl std::error::Error for ReleaseCaseMetadataError {}

/// Requires every release workflow case to identify its complete execution environment.
///
/// # Errors
///
/// Rejects non-release operations and cases missing any nonempty mapper, header, region, revision,
/// ROM-size, fixture-family, or subsystem argument.
pub fn validate_release_case_metadata(
    manifest: &OracleManifest,
) -> Result<(), ReleaseCaseMetadataError> {
    let operation = manifest.operation.name.as_str();
    if !RELEASE_OPERATIONS.contains(&operation) {
        return Err(ReleaseCaseMetadataError::UnsupportedOperation(
            operation.into(),
        ));
    }
    let mut names = BTreeSet::new();
    for (name, _) in &manifest.operation.arguments {
        if !names.insert(name.as_str()) {
            return Err(ReleaseCaseMetadataError::DuplicateArgument {
                operation: operation.into(),
                name: name.clone(),
            });
        }
    }
    let arguments = manifest
        .operation
        .arguments
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<BTreeMap<_, _>>();
    let missing = RELEASE_ARGUMENTS
        .into_iter()
        .filter(|name| !arguments.contains_key(name))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(ReleaseCaseMetadataError::MissingArguments {
            operation: operation.into(),
            names: missing,
        });
    }
    for name in RELEASE_ARGUMENTS {
        if arguments[name].is_empty() {
            return Err(ReleaseCaseMetadataError::EmptyArgument {
                operation: operation.into(),
                name,
            });
        }
    }
    if !RELEASE_SUBSYSTEMS.contains(&arguments["subsystem"]) {
        return Err(ReleaseCaseMetadataError::UnsupportedSubsystem(
            arguments["subsystem"].into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_oracle::{Observation, Operation};

    fn manifest(operation: &str, arguments: &[(&str, &str)]) -> OracleManifest {
        OracleManifest {
            case_id: "case".into(),
            lunar_magic_version: "3.63".into(),
            input_sha256: "a".repeat(64),
            output_sha256: "b".repeat(64),
            operation: Operation {
                name: operation.into(),
                arguments: arguments
                    .iter()
                    .map(|(name, value)| ((*name).into(), (*value).into()))
                    .collect(),
            },
            changed_ranges: vec![],
            decoded_before: Observation::new().to_text(),
            decoded_after: Observation::new().to_text(),
            owned_allocations_before: vec![],
            owned_allocations_after: vec![],
            warnings: vec![],
            errors: vec![],
        }
    }

    #[test]
    fn complete_nonempty_provenance_is_required_per_case() {
        let arguments = [
            ("mapper", "lorom"),
            ("header", "headerless"),
            ("region", "us"),
            ("revision", "smw-us-v1"),
            ("rom_size", "expanded"),
            ("fixture_family", "clean"),
            ("subsystem", "levels"),
        ];
        validate_release_case_metadata(&manifest("open-save", &arguments)).unwrap();

        let missing = &arguments[..6];
        assert!(matches!(
            validate_release_case_metadata(&manifest("open-save", missing)),
            Err(ReleaseCaseMetadataError::MissingArguments { .. })
        ));
        let mut empty = arguments;
        empty[2].1 = "";
        assert!(matches!(
            validate_release_case_metadata(&manifest("open-save", &empty)),
            Err(ReleaseCaseMetadataError::EmptyArgument { name: "region", .. })
        ));
        assert!(matches!(
            validate_release_case_metadata(&manifest("other", &arguments)),
            Err(ReleaseCaseMetadataError::UnsupportedOperation(_))
        ));
        let mut unknown = arguments;
        unknown[6].1 = "audio";
        assert!(matches!(
            validate_release_case_metadata(&manifest("open-save", &unknown)),
            Err(ReleaseCaseMetadataError::UnsupportedSubsystem(value)) if value == "audio"
        ));
        let mut duplicate = arguments.to_vec();
        duplicate.push(("mapper", "exlorom"));
        assert!(matches!(
            validate_release_case_metadata(&manifest("open-save", &duplicate)),
            Err(ReleaseCaseMetadataError::DuplicateArgument { name, .. }) if name == "mapper"
        ));
    }
}
