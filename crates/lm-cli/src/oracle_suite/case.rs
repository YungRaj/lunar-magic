use super::fixtures::{optional_regular_file, read_regular_bounded};
use super::{
    AFTER_OBSERVATION, AFTER_ROM, BEFORE_OBSERVATION, BEFORE_ROM, EMULATOR_PNG, MANIFEST,
    RENDER_PNG,
};
use crate::oracle_input::MAX_ROM_BYTES;
use lm_oracle::{
    Observation, OracleCaseReport, OracleManifest, verify_oracle_case,
    verify_oracle_case_with_observations,
};
use std::path::Path;

pub(super) fn verify_case(
    directory: &Path,
    require_observations: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_text =
        read_regular_bounded(&directory.join(MANIFEST), OracleManifest::MAX_TEXT_BYTES)?;
    let manifest = OracleManifest::from_text(std::str::from_utf8(&manifest_text)?)?;
    if require_observations && !manifest.errors.is_empty() {
        return Err(format!(
            "release-gate case records {} Lunar Magic error(s)",
            manifest.errors.len()
        )
        .into());
    }
    if require_observations {
        crate::oracle_release_policy::validate_release_case_metadata(&manifest)?;
    }
    let before = read_regular_bounded(&directory.join(BEFORE_ROM), MAX_ROM_BYTES)?;
    let after = read_regular_bounded(&directory.join(AFTER_ROM), MAX_ROM_BYTES)?;
    let before_observation = directory.join(BEFORE_OBSERVATION);
    let after_observation = directory.join(AFTER_OBSERVATION);
    let report = match (
        optional_regular_file(&before_observation)?,
        optional_regular_file(&after_observation)?,
    ) {
        (true, true) => {
            let before_text =
                read_regular_bounded(&before_observation, Observation::MAX_TEXT_BYTES)?;
            let after_text = read_regular_bounded(&after_observation, Observation::MAX_TEXT_BYTES)?;
            let decoded_before = Observation::from_text(std::str::from_utf8(&before_text)?)?;
            let decoded_after = Observation::from_text(std::str::from_utf8(&after_text)?)?;
            if require_observations {
                if crate::release_subsystem_evidence::semantic_observation_digest(&decoded_before)
                    .is_none()
                {
                    return Err(
                        "release-gate before.obs requires non-release semantic evidence".into(),
                    );
                }
                crate::release_evidence::validate(&manifest.operation.name, &decoded_after)?;
                let subsystem = manifest
                    .operation
                    .arguments
                    .iter()
                    .find_map(|(name, value)| (name == "subsystem").then_some(value.as_str()))
                    .ok_or("release case is missing subsystem provenance")?;
                crate::release_subsystem_evidence::validate(subsystem, &decoded_after)?;
                if manifest.operation.name == "render-level" {
                    let render = read_regular_bounded(
                        &directory.join(RENDER_PNG),
                        crate::oracle_render_evidence::MAX_RELEASE_RENDER_BYTES,
                    )?;
                    crate::oracle_render_evidence::validate_release_render(
                        &render,
                        &decoded_after,
                    )?;
                }
                if manifest.operation.name == "emulator-boot" {
                    let observed_rom = decoded_after
                        .get("release/emulator-boot/rom-sha256")
                        .ok_or("emulator evidence is missing its ROM digest")?;
                    let actual_rom = lm_oracle::sha256_hex(&after);
                    if observed_rom != actual_rom {
                        return Err(format!(
                            "emulator evidence ROM digest mismatch: expected {observed_rom}, actual {actual_rom}"
                        )
                        .into());
                    }
                    let screenshot = read_regular_bounded(
                        &directory.join(EMULATOR_PNG),
                        crate::oracle_render_evidence::MAX_RELEASE_RENDER_BYTES,
                    )?;
                    crate::oracle_render_evidence::validate_emulator_screenshot(
                        &screenshot,
                        &decoded_after,
                    )?;
                }
            }
            verify_oracle_case_with_observations(
                &manifest,
                &before,
                &after,
                &[],
                &decoded_before,
                &decoded_after,
            )?
        }
        (false, false) if !require_observations => {
            verify_oracle_case(&manifest, &before, &after, &[])
        }
        (false, false) => {
            return Err("release-gate cases require before.obs and after.obs".into());
        }
        _ => return Err("oracle observations must supply both before.obs and after.obs".into()),
    };
    if report.is_match() {
        Ok(())
    } else {
        Err(summarize(&report).into())
    }
}

fn summarize(report: &OracleCaseReport) -> String {
    let mut failures = Vec::new();
    if !report.manifest_valid {
        failures.push("manifest invariants");
    }
    if !report.input_hash_matches {
        failures.push("input hash");
    }
    if !report.output_hash_matches {
        failures.push("output hash");
    }
    if !report.recorded_errors.is_empty() {
        failures.push("recorded oracle errors");
    }
    if !report.changes.is_match() {
        failures.push("changed ranges");
    }
    if !report.owned_tagged_ranges.is_valid() {
        failures.push("owned RATS ranges");
    }
    if report
        .semantics
        .as_ref()
        .is_some_and(|semantics| !semantics.is_match())
    {
        failures.push("semantic observations");
    }
    format!("mismatched {}", failures.join(", "))
}
