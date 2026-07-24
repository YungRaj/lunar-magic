use lm_oracle::{
    AllocationOwnershipPolicy, CaptureMetadata, Observation, Operation, capture_oracle_case,
};
use lm_render::{Canvas, encode_png};
use std::fs;
use std::path::Path;

const SUBSYSTEMS: [&str; 12] = [
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

pub(super) fn write_release_corpus(root: &Path) {
    for (index, (operation, subsystem)) in [
        ("open-save", "rom"),
        ("render-level", "rendering"),
        ("level-edit", "levels"),
    ]
    .into_iter()
    .enumerate()
    {
        write_case(root, operation, operation, subsystem, index);
    }
    let mut index = 3;
    for operation in ["lunar-magic-reopen", "emulator-boot"] {
        for subsystem in SUBSYSTEMS {
            let directory = if subsystem == "rom" {
                operation.to_owned()
            } else {
                format!("{operation}-{subsystem}")
            };
            write_case(root, &directory, operation, subsystem, index);
            index += 1;
        }
    }
}

fn write_case(root: &Path, directory: &str, operation: &str, subsystem: &str, index: usize) {
    let directory_path = root.join(directory);
    fs::create_dir(&directory_path).unwrap();
    let before = [u8::try_from(index).unwrap(), 2];
    let after = [u8::try_from(index).unwrap(), 3];
    let image = matches!(operation, "render-level" | "emulator-boot")
        .then(|| encode_png(&Canvas::try_new(2, 3).unwrap()).unwrap());
    let before_observation = semantic_observation(subsystem, format!("before-{index}"));
    let mut after_observation = release_observation(operation, image.as_deref(), &after);
    insert(
        &mut after_observation,
        &format!("model/{subsystem}/sample"),
        format!("after-{index}"),
    );
    let digest = semantic_digest(&after_observation);
    insert(
        &mut after_observation,
        &format!("release/subsystem/{subsystem}/observation-sha256"),
        digest,
    );
    let manifest = capture_oracle_case(
        CaptureMetadata {
            case_id: directory.into(),
            lunar_magic_version: "3.63".into(),
            operation: Operation {
                name: operation.into(),
                arguments: vec![
                    ("mapper".into(), "lorom".into()),
                    ("header".into(), "headerless".into()),
                    ("region".into(), "us".into()),
                    ("revision".into(), "smw-us-v1".into()),
                    ("rom_size".into(), "expanded".into()),
                    ("fixture_family".into(), "clean".into()),
                    ("subsystem".into(), subsystem.into()),
                ],
            },
            warnings: Vec::new(),
            errors: Vec::new(),
            allocation_ownership: AllocationOwnershipPolicy::None,
        },
        &before,
        &after,
        &before_observation,
        &after_observation,
    )
    .unwrap();
    fs::write(directory_path.join("case.manifest"), manifest.to_text()).unwrap();
    fs::write(directory_path.join("before.smc"), before).unwrap();
    fs::write(directory_path.join("after.smc"), after).unwrap();
    fs::write(
        directory_path.join("before.obs"),
        before_observation.to_text(),
    )
    .unwrap();
    fs::write(
        directory_path.join("after.obs"),
        after_observation.to_text(),
    )
    .unwrap();
    if let Some(image) = image {
        let name = if operation == "render-level" {
            "render.png"
        } else {
            "emulator.png"
        };
        fs::write(directory_path.join(name), image).unwrap();
    }
}

fn semantic_observation(subsystem: &str, value: String) -> Observation {
    let mut observation = Observation::new();
    insert(
        &mut observation,
        &format!("model/{subsystem}/sample"),
        value,
    );
    observation
}

fn release_observation(operation: &str, image: Option<&[u8]>, after: &[u8]) -> Observation {
    let mut observation = Observation::new();
    match operation {
        "open-save" => {
            for path in ["reopened", "checksum-valid", "unchanged-regions"] {
                insert(
                    &mut observation,
                    &format!("release/open-save/{path}"),
                    "true",
                );
            }
        }
        "render-level" => {
            let image = image.unwrap();
            insert(
                &mut observation,
                "release/render-level/png-sha256",
                lm_oracle::sha256_hex(image),
            );
            insert(&mut observation, "release/render-level/width", "2");
            insert(&mut observation, "release/render-level/height", "3");
        }
        "level-edit" => {
            for path in ["semantic-change", "reopened", "unchanged-regions"] {
                insert(
                    &mut observation,
                    &format!("release/level-edit/{path}"),
                    "true",
                );
            }
        }
        "lunar-magic-reopen" => {
            for path in ["reopened", "semantic-equal"] {
                insert(
                    &mut observation,
                    &format!("release/lunar-magic-reopen/{path}"),
                    "true",
                );
            }
        }
        "emulator-boot" => {
            let image = image.unwrap();
            insert(&mut observation, "release/emulator-boot/booted", "true");
            insert(
                &mut observation,
                "release/emulator-boot/emulator",
                "Mesen 2",
            );
            insert(
                &mut observation,
                "release/emulator-boot/rom-sha256",
                lm_oracle::sha256_hex(after),
            );
            insert(&mut observation, "release/emulator-boot/frames", "120");
            insert(
                &mut observation,
                "release/emulator-boot/screenshot-sha256",
                lm_oracle::sha256_hex(image),
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
        _ => unreachable!(),
    }
    observation
}

fn semantic_digest(observation: &Observation) -> String {
    let text = observation
        .entries()
        .filter(|(path, _)| !path.starts_with("release/"))
        .fold(String::new(), |mut text, (path, value)| {
            text.push_str(path);
            text.push('\0');
            text.push_str(value);
            text.push('\n');
            text
        });
    lm_oracle::sha256_hex(text.as_bytes())
}

fn insert(observation: &mut Observation, path: &str, value: impl Into<String>) {
    observation.insert(path, value).unwrap();
}
