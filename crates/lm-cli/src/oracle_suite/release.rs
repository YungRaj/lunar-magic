use super::fixtures::{discover, manifest_path, read_regular_bounded};
use super::verify_with_policy;
use crate::oracle_release_policy::{
    RELEASE_ARGUMENTS, RELEASE_COMPATIBILITY_OPERATIONS, RELEASE_OPERATIONS, RELEASE_SUBSYSTEMS,
};
use lm_oracle::{CorpusPolicy, CorpusRequirement, OracleManifest, audit_corpus};
use std::collections::BTreeSet;
use std::path::Path;

pub fn release_gate(
    root: &Path,
    requirements: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    validate_release_requirements(requirements)?;
    verify_with_policy(root, true)?;
    audit_coverage(root, requirements)?;
    audit_release_compatibility_matrix(root)?;
    println!("oracle-release-gate: PASS");
    Ok(())
}

fn audit_release_compatibility_matrix(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let cases = discover(root)?;
    let mut present = BTreeSet::new();
    for directory in cases {
        let manifest = manifest_path(&directory)?
            .ok_or_else(|| format!("oracle fixture has no manifest: {}", directory.display()))?;
        let text = read_regular_bounded(&manifest, OracleManifest::MAX_TEXT_BYTES)?;
        let manifest = OracleManifest::from_text(std::str::from_utf8(&text)?)?;
        let subsystem = manifest
            .operation
            .arguments
            .iter()
            .find_map(|(name, value)| (name == "subsystem").then_some(value.as_str()));
        if let Some(subsystem) = subsystem {
            present.insert((manifest.operation.name, subsystem.to_owned()));
        }
    }
    let mut missing = Vec::new();
    for operation in RELEASE_COMPATIBILITY_OPERATIONS {
        for subsystem in RELEASE_SUBSYSTEMS {
            if !present.contains(&(operation.to_owned(), subsystem.to_owned())) {
                missing.push(format!("{operation}:{subsystem}"));
            }
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "release corpus lacks {} compatibility operation/subsystem pair(s): {}",
            missing.len(),
            missing.join(",")
        )
        .into())
    }
}

fn validate_release_requirements(
    requirements: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = requirements
        .iter()
        .map(|requirement| parse_requirement(requirement))
        .collect::<Result<Vec<_>, _>>()?;
    let has_version = parsed
        .iter()
        .any(|requirement| matches!(requirement, CorpusRequirement::LunarMagicVersion(_)));
    let operations = parsed
        .iter()
        .filter_map(|requirement| match requirement {
            CorpusRequirement::Operation(operation) => Some(operation.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let missing_operations = RELEASE_OPERATIONS
        .into_iter()
        .filter(|operation| !operations.contains(operation))
        .collect::<Vec<_>>();
    let argument_names = parsed
        .iter()
        .filter_map(|requirement| match requirement {
            CorpusRequirement::Argument { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let missing_arguments = RELEASE_ARGUMENTS
        .into_iter()
        .filter(|name| !argument_names.contains(name))
        .collect::<Vec<_>>();
    let subsystem_values = parsed
        .iter()
        .filter_map(|requirement| match requirement {
            CorpusRequirement::Argument { name, value } if name == "subsystem" => {
                Some(value.as_str())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let missing_subsystems = RELEASE_SUBSYSTEMS
        .into_iter()
        .filter(|subsystem| !subsystem_values.contains(subsystem))
        .collect::<Vec<_>>();
    if !has_version
        || !missing_operations.is_empty()
        || !missing_arguments.is_empty()
        || !missing_subsystems.is_empty()
    {
        return Err(format!(
            "release gate requires version, all workflow operations, representative arguments, and every subsystem; missing version={}, operations={}, arguments={}, subsystems={}",
            !has_version,
            missing_operations.join(","),
            missing_arguments.join(","),
            missing_subsystems.join(",")
        )
        .into());
    }
    Ok(())
}

pub fn audit_coverage(
    root: &Path,
    requirements: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let cases = discover(root)?;
    let policy = CorpusPolicy {
        requirements: requirements
            .iter()
            .map(|requirement| parse_requirement(requirement))
            .collect::<Result<BTreeSet<_>, _>>()?,
    };
    let manifests = cases
        .iter()
        .map(|directory| {
            let manifest = manifest_path(directory)?.ok_or_else(|| {
                format!("oracle fixture has no manifest: {}", directory.display())
            })?;
            let text = read_regular_bounded(&manifest, OracleManifest::MAX_TEXT_BYTES)?;
            Ok(OracleManifest::from_text(std::str::from_utf8(&text)?)?)
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let report = audit_corpus(&policy, &manifests);
    println!(
        "oracle-cases: {} ({} valid)",
        report.case_count, report.valid_case_count
    );
    for requirement in &report.missing {
        println!("MISSING {}", format_requirement(requirement));
    }
    for case_id in &report.duplicate_case_ids {
        println!("DUPLICATE case:{case_id}");
    }
    for invalid in &report.invalid_cases {
        println!(
            "INVALID case[{}]:{:?}: {}",
            invalid.index, invalid.case_id, invalid.error
        );
    }
    if report.is_satisfied() {
        Ok(())
    } else {
        Err(format!(
            "oracle corpus lacks {} requirement(s), has {} duplicate case ID(s), and has {} invalid case(s)",
            report.missing.len(),
            report.duplicate_case_ids.len(),
            report.invalid_cases.len()
        )
        .into())
    }
}

fn parse_requirement(value: &str) -> Result<CorpusRequirement, Box<dyn std::error::Error>> {
    if let Some(version) = value
        .strip_prefix("version:")
        .filter(|value| !value.is_empty())
    {
        return Ok(CorpusRequirement::LunarMagicVersion(version.into()));
    }
    if let Some(operation) = value
        .strip_prefix("operation:")
        .filter(|value| !value.is_empty())
    {
        return Ok(CorpusRequirement::Operation(operation.into()));
    }
    if let Some(argument) = value.strip_prefix("argument:")
        && let Some((name, value)) = argument.split_once('=')
        && !name.is_empty()
        && !value.is_empty()
    {
        return Ok(CorpusRequirement::Argument {
            name: name.into(),
            value: value.into(),
        });
    }
    Err(format!(
        "invalid coverage requirement {value:?}; expected version:VALUE, operation:VALUE, or argument:NAME=VALUE"
    )
    .into())
}

fn format_requirement(requirement: &CorpusRequirement) -> String {
    match requirement {
        CorpusRequirement::LunarMagicVersion(version) => format!("version:{version}"),
        CorpusRequirement::Operation(operation) => format!("operation:{operation}"),
        CorpusRequirement::Argument { name, value } => format!("argument:{name}={value}"),
    }
}
