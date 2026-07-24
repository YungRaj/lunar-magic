use crate::args::{OracleCaptureCommand, OracleOwnership};
use crate::atomic_output::write_new;
use crate::oracle_input::{MAX_ROM_BYTES, read_bounded};
use lm_oracle::{
    AllocationOwnershipPolicy, CaptureMetadata, Observation, Operation, OracleManifest,
    capture_oracle_case, verify_oracle_case_with_observations,
};

pub fn capture(command: &OracleCaptureCommand) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct_output(command)?;
    let before = read_bounded(&command.before, MAX_ROM_BYTES)?;
    let after = read_bounded(&command.after, MAX_ROM_BYTES)?;
    let decoded_before = read_bounded(&command.decoded_before, Observation::MAX_TEXT_BYTES)?;
    let decoded_after = read_bounded(&command.decoded_after, Observation::MAX_TEXT_BYTES)?;
    let decoded_before = Observation::from_text(std::str::from_utf8(&decoded_before)?)?;
    let decoded_after = Observation::from_text(std::str::from_utf8(&decoded_after)?)?;
    let manifest = build_manifest(command, &before, &after, &decoded_before, &decoded_after)?;
    write_new(&command.output, manifest.to_text())?;
    println!("case-id: {}", manifest.case_id);
    println!("changed-ranges: {}", manifest.changed_ranges.len());
    println!(
        "owned-tagged-ranges: {} before, {} after",
        manifest.owned_allocations_before.len(),
        manifest.owned_allocations_after.len()
    );
    println!("output: {}", command.output.display());
    Ok(())
}

fn build_manifest(
    command: &OracleCaptureCommand,
    before: &[u8],
    after: &[u8],
    decoded_before: &Observation,
    decoded_after: &Observation,
) -> Result<OracleManifest, Box<dyn std::error::Error>> {
    let allocation_ownership = match command.ownership {
        OracleOwnership::None => AllocationOwnershipPolicy::None,
        OracleOwnership::ChangedRats => AllocationOwnershipPolicy::ChangedTagged,
    };
    let manifest = capture_oracle_case(
        CaptureMetadata {
            case_id: command.case_id.clone(),
            lunar_magic_version: command.lunar_magic_version.clone(),
            operation: Operation {
                name: command.operation.clone(),
                arguments: command.arguments.clone(),
            },
            warnings: Vec::new(),
            errors: Vec::new(),
            allocation_ownership,
        },
        before,
        after,
        decoded_before,
        decoded_after,
    )?;
    let replay = verify_oracle_case_with_observations(
        &manifest,
        before,
        after,
        &[],
        decoded_before,
        decoded_after,
    )?;
    if !replay.is_match() {
        return Err("captured oracle manifest failed immediate replay verification".into());
    }
    Ok(manifest)
}

fn require_distinct_output(
    command: &OracleCaptureCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = command.output.as_path();
    if [
        command.before.as_path(),
        command.after.as_path(),
        command.decoded_before.as_path(),
        command.decoded_after.as_path(),
    ]
    .contains(&output)
    {
        Err("refusing to overwrite an oracle capture input".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::make_header;

    fn command(ownership: OracleOwnership) -> OracleCaptureCommand {
        OracleCaptureCommand {
            case_id: "level-105-edit".into(),
            lunar_magic_version: "3.63".into(),
            operation: "move-object".into(),
            before: "before.smc".into(),
            after: "after.smc".into(),
            decoded_before: "before.obs".into(),
            decoded_after: "after.obs".into(),
            ownership,
            output: "case.manifest".into(),
            arguments: vec![("level".into(), "105".into())],
        }
    }

    fn image(offset: usize) -> Vec<u8> {
        let mut bytes = vec![0xff; 0x100];
        bytes[offset..offset + 8].copy_from_slice(&make_header(3).unwrap());
        bytes[offset + 8..offset + 11].copy_from_slice(&[1, 2, 3]);
        bytes
    }

    #[test]
    fn captured_cli_manifest_immediately_replays_and_retains_arguments() {
        let before = image(0x20);
        let after = image(0x60);
        let mut before_observation = Observation::new();
        before_observation.insert("level/105/x", "1").unwrap();
        let mut after_observation = Observation::new();
        after_observation.insert("level/105/x", "2").unwrap();
        let manifest = build_manifest(
            &command(OracleOwnership::ChangedRats),
            &before,
            &after,
            &before_observation,
            &after_observation,
        )
        .unwrap();
        assert_eq!(
            manifest.operation.arguments,
            [("level".into(), "105".into())]
        );
        assert_eq!(manifest.owned_allocations_before.len(), 1);
        assert_eq!(manifest.owned_allocations_before[0], 0x20..0x2b);
        assert_eq!(manifest.owned_allocations_after.len(), 1);
        assert_eq!(manifest.owned_allocations_after[0], 0x60..0x6b);
        assert_eq!(
            OracleManifest::from_text(&manifest.to_text()).unwrap(),
            manifest
        );
    }

    #[test]
    fn capture_output_cannot_alias_any_input() {
        let mut command = command(OracleOwnership::None);
        command.output = command.decoded_after.clone();
        assert!(require_distinct_output(&command).is_err());
    }
}
