use crate::oracle_input::{MAX_ROM_BYTES, read_bounded};
use lm_oracle::{
    Observation, OracleManifest, verify_oracle_case, verify_oracle_case_with_observations,
};
use std::path::Path;

pub fn verify(
    manifest: &Path,
    before: &Path,
    after: &Path,
    observations: Option<&(std::path::PathBuf, std::path::PathBuf)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = read_bounded(manifest, OracleManifest::MAX_TEXT_BYTES)?;
    let manifest = OracleManifest::from_text(std::str::from_utf8(&manifest)?)?;
    let before = read_bounded(before, MAX_ROM_BYTES)?;
    let after = read_bounded(after, MAX_ROM_BYTES)?;
    let report = if let Some((decoded_before, decoded_after)) = observations {
        let decoded_before = read_bounded(decoded_before, Observation::MAX_TEXT_BYTES)?;
        let decoded_after = read_bounded(decoded_after, Observation::MAX_TEXT_BYTES)?;
        let decoded_before = Observation::from_text(std::str::from_utf8(&decoded_before)?)?;
        let decoded_after = Observation::from_text(std::str::from_utf8(&decoded_after)?)?;
        verify_oracle_case_with_observations(
            &manifest,
            &before,
            &after,
            &[],
            &decoded_before,
            &decoded_after,
        )?
    } else {
        verify_oracle_case(&manifest, &before, &after, &[])
    };
    println!("manifest-valid: {}", report.manifest_valid);
    println!("input-hash-matches: {}", report.input_hash_matches);
    println!("output-hash-matches: {}", report.output_hash_matches);
    println!("recorded-oracle-errors: {}", report.recorded_errors.len());
    for error in &report.recorded_errors {
        println!("oracle-error: {error}");
    }
    println!(
        "unexpected-ranges: {}",
        report.changes.unexpected_changes.len()
    );
    println!("tagged-added: {}", report.tagged_allocations.added.len());
    println!(
        "tagged-removed: {}",
        report.tagged_allocations.removed.len()
    );
    println!(
        "tagged-relocated: {}",
        report.tagged_allocations.relocated_count()
    );
    println!(
        "invalid-owned-tagged-ranges: {}",
        report.owned_tagged_ranges.missing_before.len()
            + report.owned_tagged_ranges.missing_after.len()
    );
    if let Some(semantics) = &report.semantics {
        println!("semantic-before-differences: {}", semantics.before.len());
        println!("semantic-after-differences: {}", semantics.after.len());
        for difference in semantics.before.iter().chain(&semantics.after) {
            println!("semantic-difference: {}", difference.path);
        }
    } else {
        println!("semantics-checked: false");
    }
    if !report.is_match() {
        return Err("oracle case does not match its manifest".into());
    }
    Ok(())
}
