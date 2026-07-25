use super::*;
use crate::oracle_release_policy::{
    RELEASE_COMPATIBILITY_OPERATIONS, RELEASE_OPERATIONS, RELEASE_SUBSYSTEMS,
};
use lm_oracle::{
    AllocationOwnershipPolicy, CaptureMetadata, Observation, Operation, OracleManifest,
    capture_oracle_case,
};
use lm_render::{Canvas, encode_png};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

fn temporary_directory() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "lm-oracle-suite-{}-{nonce}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    directory
}

fn write_case(directory: &Path, before: &[u8], after: &[u8]) {
    fs::create_dir_all(directory).unwrap();
    let manifest = capture_oracle_case(
        CaptureMetadata {
            case_id: directory
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            lunar_magic_version: "test".into(),
            operation: Operation {
                name: "fixture".into(),
                arguments: vec![],
            },
            warnings: vec![],
            errors: vec![],
            allocation_ownership: AllocationOwnershipPolicy::None,
        },
        before,
        after,
        &Observation::new(),
        &Observation::new(),
    )
    .unwrap();
    fs::write(directory.join(MANIFEST), manifest.to_text()).unwrap();
    fs::write(directory.join(BEFORE_ROM), before).unwrap();
    fs::write(directory.join(AFTER_ROM), after).unwrap();
}

fn release_observation(operation: &str, image: Option<&[u8]>, after_rom: &[u8]) -> Observation {
    let mut observation = Observation::new();
    match operation {
        "open-save" => {
            insert(&mut observation, "release/open-save/reopened", "true");
            insert(&mut observation, "release/open-save/checksum-valid", "true");
            insert(
                &mut observation,
                "release/open-save/unchanged-regions",
                "true",
            );
        }
        "render-level" => {
            let render = image.expect("render-level case requires test PNG");
            insert(
                &mut observation,
                "release/render-level/png-sha256",
                lm_oracle::sha256_hex(render),
            );
            insert(&mut observation, "release/render-level/width", "2");
            insert(&mut observation, "release/render-level/height", "3");
        }
        "level-edit" => {
            insert(
                &mut observation,
                "release/level-edit/semantic-change",
                "true",
            );
            insert(&mut observation, "release/level-edit/reopened", "true");
            insert(
                &mut observation,
                "release/level-edit/unchanged-regions",
                "true",
            );
        }
        "lunar-magic-reopen" => {
            insert(
                &mut observation,
                "release/lunar-magic-reopen/reopened",
                "true",
            );
            insert(
                &mut observation,
                "release/lunar-magic-reopen/semantic-equal",
                "true",
            );
        }
        "emulator-boot" => {
            insert(&mut observation, "release/emulator-boot/booted", "true");
            insert(
                &mut observation,
                "release/emulator-boot/emulator",
                "Mesen 2",
            );
            insert(
                &mut observation,
                "release/emulator-boot/rom-sha256",
                lm_oracle::sha256_hex(after_rom),
            );
            insert(&mut observation, "release/emulator-boot/frames", "120");
            let screenshot = image.expect("emulator case requires test screenshot");
            insert(
                &mut observation,
                "release/emulator-boot/screenshot-sha256",
                lm_oracle::sha256_hex(screenshot),
            );
            insert(
                &mut observation,
                "release/emulator-boot/screenshot-width",
                "2",
            );
            insert(
                &mut observation,
                "release/emulator-boot/screenshot-height",
                "3",
            );
        }
        _ => panic!("unknown release operation"),
    }
    observation
}

fn insert(observation: &mut Observation, path: &str, value: impl Into<String>) {
    observation.insert(path, value).unwrap();
}

fn write_release_cases(root: &Path, arguments: &[(String, String)]) {
    let base = [
        ("open-save", "rom"),
        ("render-level", "rendering"),
        ("level-edit", "levels"),
    ];
    let mut index = 0;
    for (operation, subsystem) in base {
        write_release_case(root, operation, subsystem, operation, arguments, index);
        index += 1;
    }
    for operation in RELEASE_COMPATIBILITY_OPERATIONS {
        for (subsystem_index, subsystem) in RELEASE_SUBSYSTEMS.into_iter().enumerate() {
            let directory = if subsystem_index == 0 {
                operation.into()
            } else {
                format!("{operation}-{subsystem}")
            };
            write_release_case(root, &directory, subsystem, operation, arguments, index);
            index += 1;
        }
    }
}

fn write_release_case(
    root: &Path,
    directory_name: &str,
    subsystem: &str,
    operation: &str,
    arguments: &[(String, String)],
    index: usize,
) {
    let directory = root.join(directory_name);
    fs::create_dir(&directory).unwrap();
    let before = [u8::try_from(index).unwrap(), 2];
    let after = [u8::try_from(index).unwrap(), 3];
    let image = matches!(operation, "render-level" | "emulator-boot")
        .then(|| encode_png(&Canvas::try_new(2, 3).unwrap()).unwrap());
    let mut before_observation = Observation::new();
    insert(
        &mut before_observation,
        &format!("model/{subsystem}/sample"),
        format!("baseline-{index}"),
    );
    let mut after_observation = release_observation(operation, image.as_deref(), &after);
    insert(
        &mut after_observation,
        &format!("model/{subsystem}/sample"),
        format!("case-{index}"),
    );
    let semantic_digest =
        crate::release_subsystem_evidence::semantic_observation_digest(&after_observation).unwrap();
    insert(
        &mut after_observation,
        &format!("release/subsystem/{subsystem}/observation-sha256"),
        semantic_digest,
    );
    let mut case_arguments = arguments.to_vec();
    case_arguments.push(("subsystem".into(), subsystem.into()));
    let manifest = capture_oracle_case(
        CaptureMetadata {
            case_id: directory_name.into(),
            lunar_magic_version: "3.40".into(),
            operation: Operation {
                name: operation.into(),
                arguments: case_arguments,
            },
            warnings: vec![],
            errors: vec![],
            allocation_ownership: AllocationOwnershipPolicy::None,
        },
        &before,
        &after,
        &before_observation,
        &after_observation,
    )
    .unwrap();
    fs::write(directory.join(MANIFEST), manifest.to_text()).unwrap();
    fs::write(directory.join(BEFORE_ROM), before).unwrap();
    fs::write(directory.join(AFTER_ROM), after).unwrap();
    fs::write(
        directory.join(BEFORE_OBSERVATION),
        before_observation.to_text(),
    )
    .unwrap();
    fs::write(
        directory.join(AFTER_OBSERVATION),
        after_observation.to_text(),
    )
    .unwrap();
    if let Some(image) = image {
        let name = if operation == "render-level" {
            RENDER_PNG
        } else {
            EMULATOR_PNG
        };
        fs::write(directory.join(name), image).unwrap();
    }
}

fn assert_emulator_evidence_is_artifact_bound(root: &Path, requirements: &[String]) {
    let directory = root.join("emulator-boot");
    let screenshot_path = directory.join(EMULATOR_PNG);
    let screenshot = fs::read(&screenshot_path).unwrap();
    fs::write(&screenshot_path, [0_u8]).unwrap();
    assert!(release_gate(root, requirements).is_err());
    fs::write(&screenshot_path, screenshot).unwrap();
    release_gate(root, requirements).unwrap();

    let observation_path = directory.join(AFTER_OBSERVATION);
    let manifest_path = directory.join(MANIFEST);
    let observation_text = fs::read_to_string(&observation_path).unwrap();
    let manifest_text = fs::read_to_string(&manifest_path).unwrap();
    let observation = Observation::from_text(&observation_text).unwrap();
    let mut wrong_rom = Observation::new();
    for (path, value) in observation.entries() {
        wrong_rom
            .insert(
                path,
                if path == "release/emulator-boot/rom-sha256" {
                    "a".repeat(64)
                } else {
                    value.into()
                },
            )
            .unwrap();
    }
    let mut manifest = OracleManifest::from_text(&manifest_text).unwrap();
    manifest.decoded_after = wrong_rom.to_text();
    fs::write(&observation_path, wrong_rom.to_text()).unwrap();
    fs::write(&manifest_path, manifest.to_text()).unwrap();
    assert!(release_gate(root, requirements).is_err());
    fs::write(observation_path, observation_text).unwrap();
    fs::write(manifest_path, manifest_text).unwrap();
    release_gate(root, requirements).unwrap();
}

#[test]
fn nested_cases_are_sorted_and_verified() {
    let root = temporary_directory();
    write_case(&root.join("b"), &[1, 2], &[1, 3]);
    write_case(&root.join("a/nested"), &[4], &[5]);
    assert_eq!(
        discover(&root).unwrap(),
        [root.join("a/nested"), root.join("b")]
    );
    verify(&root).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_manifests_are_discovered_and_canonical_manifests_take_precedence() {
    let root = temporary_directory();
    let legacy = root.join("legacy");
    write_case(&legacy, &[1, 2], &[1, 3]);
    fs::rename(legacy.join(MANIFEST), legacy.join(LEGACY_MANIFEST)).unwrap();
    assert_eq!(discover(&root).unwrap(), std::slice::from_ref(&legacy));
    verify(&root).unwrap();

    let canonical = root.join("canonical");
    write_case(&canonical, &[4], &[5]);
    fs::write(canonical.join(LEGACY_MANIFEST), b"malformed legacy").unwrap();
    assert_eq!(
        fixtures::manifest_path(&canonical).unwrap().unwrap(),
        canonical.join(MANIFEST)
    );
    verify(&root).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_parent_case_cannot_hide_nested_case_manifests() {
    let root = temporary_directory();
    let parent = root.join("parent");
    let nested = parent.join("nested");
    write_case(&parent, &[1], &[2]);
    write_case(&nested, &[3], &[4]);
    assert_eq!(discover(&root).unwrap(), [parent.clone(), nested.clone()]);
    fs::write(nested.join(AFTER_ROM), [9]).unwrap();
    assert!(verify(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn symlinked_fixture_artifacts_and_suite_roots_are_rejected() {
    use std::os::unix::fs::symlink;

    let root = temporary_directory();
    let case = root.join("case");
    write_case(&case, &[1], &[2]);
    let target = root.join("outside-before.smc");
    fs::write(&target, [1]).unwrap();
    fs::remove_file(case.join(BEFORE_ROM)).unwrap();
    symlink(&target, case.join(BEFORE_ROM)).unwrap();
    assert!(verify(&root).is_err());

    let root_link = root.with_extension("link");
    symlink(&root, &root_link).unwrap();
    assert!(discover(&root_link).is_err());
    fs::remove_file(root_link).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_pairs_and_mismatches_fail_the_suite() {
    let root = temporary_directory();
    write_case(&root.join("case"), &[1], &[2]);
    fs::write(root.join("case").join(BEFORE_OBSERVATION), b"partial").unwrap();
    assert!(verify(&root).is_err());
    fs::remove_file(root.join("case").join(BEFORE_OBSERVATION)).unwrap();
    fs::write(root.join("case").join(AFTER_ROM), [9]).unwrap();
    assert!(verify(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn coverage_audit_reads_manifests_and_rejects_missing_dimensions() {
    let root = temporary_directory();
    write_case(&root.join("case"), &[1], &[2]);
    audit_coverage(&root, &["version:test".into(), "operation:fixture".into()]).unwrap();
    assert!(audit_coverage(&root, &["argument:mapper=sa1".into()]).is_err());
    assert!(audit_coverage(&root, &["argument:broken".into()]).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn release_gate_requires_semantics_replay_and_representative_dimensions() {
    let root = temporary_directory();
    let directory = root.join("release-case");
    fs::create_dir(&directory).unwrap();
    let before = [1, 2];
    let after = [1, 3];
    let observation = Observation::new();
    let manifest = capture_oracle_case(
        CaptureMetadata {
            case_id: "release-case".into(),
            lunar_magic_version: "3.40".into(),
            operation: Operation {
                name: "level-save".into(),
                arguments: vec![
                    ("mapper".into(), "lorom".into()),
                    ("header".into(), "headerless".into()),
                    ("fixture_family".into(), "clean".into()),
                ],
            },
            warnings: vec![],
            errors: vec![],
            allocation_ownership: AllocationOwnershipPolicy::None,
        },
        &before,
        &after,
        &observation,
        &observation,
    )
    .unwrap();
    fs::write(directory.join(MANIFEST), manifest.to_text()).unwrap();
    fs::write(directory.join(BEFORE_ROM), before).unwrap();
    fs::write(directory.join(AFTER_ROM), after).unwrap();
    let requirements = [
        "version:3.40".into(),
        "operation:level-save".into(),
        "argument:mapper=lorom".into(),
        "argument:header=headerless".into(),
        "argument:fixture_family=clean".into(),
    ];

    assert!(release_gate(&root, &requirements).is_err());
    fs::write(directory.join(BEFORE_OBSERVATION), observation.to_text()).unwrap();
    fs::write(directory.join(AFTER_OBSERVATION), observation.to_text()).unwrap();
    assert!(release_gate(&root, &requirements).is_err());
    assert!(release_gate(&root, &requirements[..4]).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn complete_release_workflow_policy_passes_only_with_all_operations_and_dimensions() {
    let root = temporary_directory();
    let arguments = vec![
        ("mapper".into(), "lorom".into()),
        ("header".into(), "headerless".into()),
        ("region".into(), "us".into()),
        ("revision".into(), "smw-us-v1".into()),
        ("rom_size".into(), "expanded".into()),
        ("fixture_family".into(), "clean".into()),
    ];
    write_release_cases(&root, &arguments);
    let mut requirements = vec!["version:3.40".into()];
    requirements.extend(
        RELEASE_OPERATIONS
            .into_iter()
            .map(|operation| format!("operation:{operation}")),
    );
    requirements.extend(
        arguments
            .iter()
            .map(|(name, value)| format!("argument:{name}={value}")),
    );
    requirements.extend(
        RELEASE_SUBSYSTEMS
            .into_iter()
            .map(|subsystem| format!("argument:subsystem={subsystem}")),
    );
    release_gate(&root, &requirements).unwrap();
    let missing_pair = root.join("emulator-boot-application");
    fs::remove_dir_all(&missing_pair).unwrap();
    assert!(release_gate(&root, &requirements).is_err());
    write_release_case(
        &root,
        "emulator-boot-application",
        "application",
        "emulator-boot",
        &arguments,
        200,
    );
    release_gate(&root, &requirements).unwrap();
    let open_save_directory = root.join("open-save");
    let open_save_manifest_path = open_save_directory.join(MANIFEST);
    let open_save_before_path = open_save_directory.join(BEFORE_OBSERVATION);
    let open_save_manifest_text = fs::read_to_string(&open_save_manifest_path).unwrap();
    let open_save_before_text = fs::read_to_string(&open_save_before_path).unwrap();
    let mut empty_before_manifest = OracleManifest::from_text(&open_save_manifest_text).unwrap();
    empty_before_manifest.decoded_before = Observation::new().to_text();
    fs::write(&open_save_manifest_path, empty_before_manifest.to_text()).unwrap();
    fs::write(&open_save_before_path, Observation::new().to_text()).unwrap();
    assert!(release_gate(&root, &requirements).is_err());
    fs::write(&open_save_manifest_path, open_save_manifest_text).unwrap();
    fs::write(&open_save_before_path, open_save_before_text).unwrap();
    release_gate(&root, &requirements).unwrap();
    let without_application = requirements
        .iter()
        .filter(|requirement| requirement.as_str() != "argument:subsystem=application")
        .cloned()
        .collect::<Vec<_>>();
    assert!(release_gate(&root, &without_application).is_err());
    let render_manifest_path = root.join("render-level").join(MANIFEST);
    let render_manifest_text = fs::read_to_string(&render_manifest_path).unwrap();
    let mut incomplete_metadata = OracleManifest::from_text(&render_manifest_text).unwrap();
    incomplete_metadata
        .operation
        .arguments
        .retain(|(name, _)| name != "region");
    fs::write(&render_manifest_path, incomplete_metadata.to_text()).unwrap();
    assert!(release_gate(&root, &requirements).is_err());
    fs::write(&render_manifest_path, render_manifest_text).unwrap();
    release_gate(&root, &requirements).unwrap();
    let render_path = root.join("render-level").join(RENDER_PNG);
    let render = fs::read(&render_path).unwrap();
    fs::write(&render_path, [0_u8]).unwrap();
    assert!(release_gate(&root, &requirements).is_err());
    fs::write(&render_path, render).unwrap();
    release_gate(&root, &requirements).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let render_bytes = fs::read(&render_path).unwrap();
        let linked_render = root.join("linked-render.png");
        fs::write(&linked_render, &render_bytes).unwrap();
        fs::remove_file(&render_path).unwrap();
        symlink(&linked_render, &render_path).unwrap();
        assert!(release_gate(&root, &requirements).is_err());
        fs::remove_file(&render_path).unwrap();
        fs::write(&render_path, render_bytes).unwrap();
        fs::remove_file(linked_render).unwrap();
    }
    assert_emulator_evidence_is_artifact_bound(&root, &requirements);
    requirements.retain(|requirement| requirement != "operation:emulator-boot");
    assert!(release_gate(&root, &requirements).is_err());
    requirements.push("operation:emulator-boot".into());
    let manifest_path = root.join("emulator-boot").join(MANIFEST);
    let mut manifest =
        OracleManifest::from_text(std::str::from_utf8(&fs::read(&manifest_path).unwrap()).unwrap())
            .unwrap();
    manifest.errors.push("emulator rejected ROM".into());
    fs::write(manifest_path, manifest.to_text()).unwrap();
    assert!(release_gate(&root, &requirements).is_err());
    fs::remove_dir_all(root).unwrap();
}
