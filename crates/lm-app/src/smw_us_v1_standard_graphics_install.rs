use crate::PreparedRomCommit;
use lm_project::{GraphicsSaveOptions, Project, RomMutation};
use lm_rats::ProtectedRange;
use lm_rom::{Mapper, RomImage};

const TARGET_LOGICAL_LEN: usize = 0x10_0000;
const CHECKSUM_FIELD: usize = 0x7fdc;
const ROM_SIZE_FIELD: usize = 0x7fd7;
const FILE_SIZES: [usize; 0x34] = [
    0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000,
    0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000,
    0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000, 0x1000,
    0x1000, 0x1000, 0x1000, 0x0c00, 0x0800, 0x0800, 0x0800, 0x0800, 0x1000, 0x1000, 0x1000, 0x0400,
    0x0800, 0x0800, 0x5d00, 0x3000,
];

struct FixedPatch {
    offset: usize,
    before: &'static [u8],
    after: &'static [u8],
}

const PATCHES: &[FixedPatch] = &[
    FixedPatch {
        offset: 0x0013f7,
        before: &[0xa9, 0x81, 0x8d, 0x00, 0x42],
        after: &[0x22, 0x08, 0x80, 0x10, 0x60],
    },
    FixedPatch {
        offset: 0x0015e9,
        before: &[0x20, 0xda, 0xa9, 0x20, 0xed, 0xab, 0x22, 0x9e],
        after: &[0x5c, 0x50, 0xfc, 0x0e, 0x20, 0xed, 0xab, 0x5c],
    },
    FixedPatch {
        offset: 0x0015f2,
        before: &[0x05, 0x20, 0xf9, 0xa5],
        after: &[0xfc, 0x0e, 0xea, 0xea],
    },
    FixedPatch {
        offset: 0x002439,
        before: &[0xf0],
        after: &[0x80],
    },
    FixedPatch {
        offset: 0x002830,
        before: &[0x28, 0xba, 0x00],
        after: &[0x00, 0xfc, 0x0e],
    },
    FixedPatch {
        offset: 0x002873,
        before: &[0x22, 0x28, 0xba, 0x00],
        after: &[0x80, 0x45, 0xea, 0xea],
    },
    FixedPatch {
        offset: 0x002a06,
        before: &[0xf0, 0x03],
        after: &[0xea, 0xea],
    },
    FixedPatch {
        offset: 0x002a8d,
        before: &[0x08],
        after: &[0x32],
    },
    FixedPatch {
        offset: 0x002a91,
        before: &[0x1e],
        after: &[0x32],
    },
    FixedPatch {
        offset: 0x002ace,
        before: &[0x07],
        after: &[0x10],
    },
    FixedPatch {
        offset: 0x002ad4,
        before: &[
            0xeb, 0x07, 0x00, 0x9d, 0xb2, 0x1b, 0xe6, 0x00, 0xe6, 0x00, 0xca, 0x10, 0xee, 0xa2,
        ],
        after: &[
            0xe6, 0x00, 0xe6, 0x00, 0xea, 0xca, 0xd0, 0xf3, 0x88, 0x10, 0xee, 0xe2, 0x20, 0x60,
        ],
    },
    FixedPatch {
        offset: 0x002b0b,
        before: &[
            0xb0, 0x00, 0xa2, 0x07, 0xa7, 0x00, 0x8d, 0x18, 0x21, 0xeb, 0x07, 0x00, 0x9d, 0xb2,
            0x1b,
        ],
        after: &[
            0xa2, 0x07, 0xa7, 0x00, 0x8d, 0x18, 0x21, 0xeb, 0x07, 0x00, 0x9d, 0xb2, 0x1b, 0xe6,
            0x00,
        ],
    },
    FixedPatch {
        offset: 0x002b1c,
        before: &[
            0xe6, 0x00, 0xca, 0x10, 0xee, 0xa2, 0x07, 0xa7, 0x00, 0x29, 0xff, 0x00, 0x85, 0x0c,
            0xa7, 0x00, 0xeb, 0x1d, 0xb2, 0x1b, 0x25, 0x0a, 0x05, 0x0c, 0x8d, 0x18, 0x21,
        ],
        after: &[
            0xca, 0x10, 0xee, 0xa2, 0x07, 0xa7, 0x00, 0x29, 0xff, 0x00, 0x85, 0x0c, 0xa7, 0x00,
            0xeb, 0x1d, 0xb2, 0x1b, 0x25, 0x0a, 0x05, 0x0c, 0x8d, 0x18, 0x21, 0xe6, 0x00,
        ],
    },
    FixedPatch {
        offset: 0x002b3b,
        before: &[0xe7],
        after: &[0xe5],
    },
    FixedPatch {
        offset: 0x003895,
        before: &[0x20],
        after: &[0x7d],
    },
    FixedPatch {
        offset: 0x00389f,
        before: &[0xa9, 0x7e, 0x85, 0x8f, 0xc2, 0x30, 0xa9, 0xfe, 0xac, 0x85],
        after: &[0xc2, 0x30, 0xa0, 0x00, 0x20, 0x84, 0x00, 0x4c, 0xd7, 0xb8],
    },
    FixedPatch {
        offset: 0x01ddc9,
        before: &[0x28, 0xba, 0x00],
        after: &[0x00, 0xfc, 0x0e],
    },
    FixedPatch {
        offset: 0x020000,
        before: &[0x80, 0xb4, 0x98, 0xb4, 0xb0, 0xb4],
        after: &[0x00, 0xb7, 0x20, 0xb7, 0x40, 0xb7],
    },
    FixedPatch {
        offset: 0x020007,
        before: &[
            0xb3, 0x18, 0xb3, 0x30, 0xb3, 0x48, 0xb3, 0x60, 0xb3, 0x78, 0xb3, 0x90, 0xb3, 0xa8,
            0xb3, 0xc0, 0xb3, 0xd8, 0xb3, 0xf0, 0xb3, 0x08, 0xb4, 0x20, 0xb4, 0x38, 0xb4, 0x50,
            0xb4, 0x68, 0xb4, 0x80, 0xb4, 0x98, 0xb4, 0xb0, 0xb4, 0xc8, 0xb4, 0xe0, 0xb4, 0xf8,
            0xb4, 0x10, 0xb5, 0x28, 0xb5, 0x40, 0xb5, 0x58, 0xb5, 0x70, 0xb5, 0x88, 0xb5, 0xa0,
            0xb5, 0xb8, 0xb5, 0xd0, 0xb5, 0xe8, 0xb5,
        ],
        after: &[
            0xb5, 0x20, 0xb5, 0x40, 0xb5, 0x60, 0xb5, 0x80, 0xb5, 0xa0, 0xb5, 0xc0, 0xb5, 0xe0,
            0xb5, 0x00, 0xb6, 0x20, 0xb6, 0x40, 0xb6, 0x60, 0xb6, 0x80, 0xb6, 0xa0, 0xb6, 0xc0,
            0xb6, 0xe0, 0xb6, 0x00, 0xb7, 0x20, 0xb7, 0x40, 0xb7, 0x60, 0xb7, 0x80, 0xb7, 0xa0,
            0xb7, 0xc0, 0xb7, 0xe0, 0xb7, 0x00, 0xb8, 0x20, 0xb8, 0x40, 0xb8, 0x60, 0xb8, 0x80,
            0xb8, 0xa0, 0xb8, 0xc0, 0xb8, 0xe0, 0xb8,
        ],
    },
    FixedPatch {
        offset: 0x020047,
        before: &[
            0xb6, 0x18, 0xb6, 0x30, 0xb6, 0x48, 0xb6, 0x60, 0xb6, 0x78, 0xb6, 0x90, 0xb6, 0xa8,
            0xb6, 0xc0, 0xb6, 0xd8, 0xb6, 0xf0, 0xb6, 0x08, 0xb7, 0x20, 0xb7, 0x38, 0xb7, 0x50,
            0xb7, 0x68, 0xb7, 0x80, 0xb7, 0x98, 0xb7, 0xb0, 0xb7, 0xc8, 0xb7, 0xe0, 0xb7, 0xf8,
            0xb7, 0x10, 0xb8, 0x28, 0xb8, 0x40, 0xb8, 0x58, 0xb8, 0x70, 0xb8, 0x88, 0xb8, 0xa0,
            0xb8, 0xb8, 0xb8, 0xd0, 0xb8, 0xe8, 0xb8,
        ],
        after: &[
            0xb9, 0x20, 0xb9, 0x40, 0xb9, 0x60, 0xb9, 0x80, 0xb9, 0xa0, 0xb9, 0xc0, 0xb9, 0xe0,
            0xb9, 0x00, 0xba, 0x20, 0xba, 0x40, 0xba, 0x60, 0xba, 0x80, 0xba, 0xa0, 0xba, 0xc0,
            0xba, 0xe0, 0xba, 0x00, 0xbb, 0x20, 0xbb, 0x40, 0xbb, 0x60, 0xbb, 0x80, 0xbb, 0xa0,
            0xbb, 0xc0, 0xbb, 0xe0, 0xbb, 0x00, 0xbc, 0x20, 0xbc, 0x40, 0xbc, 0x60, 0xbc, 0x80,
            0xbc, 0xa0, 0xbc, 0xc0, 0xbc, 0xe0, 0xbc,
        ],
    },
    FixedPatch {
        offset: 0x0200bd,
        before: &[0x08],
        after: &[0x10],
    },
    FixedPatch {
        offset: 0x0200d0,
        before: &[0xb7],
        after: &[0x60],
    },
];

const RAM_REFERENCE_PATCHES: &[(usize, [u8; 2], [u8; 2])] = &[
    (0x272b8, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x272bf, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x272c6, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x272cd, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x272d3, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x272d7, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x272e0, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x272e7, [0xba, 0x7e], [0xc6, 0x7f]),
    (0x272ed, [0xba, 0x7e], [0xc6, 0x7f]),
    (0x2732d, [0xba, 0x7e], [0xc6, 0x7f]),
    (0x2733c, [0xba, 0x7e], [0xc6, 0x7f]),
    (0x27340, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x27345, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x2739f, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x273a8, [0xba, 0x7e], [0xc6, 0x7f]),
    (0x273ac, [0xba, 0x7e], [0xc6, 0x7f]),
    (0x273b0, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x273c1, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x273c5, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x273cb, [0xb9, 0x7e], [0xc5, 0x7f]),
    (0x273cf, [0xb9, 0x7e], [0xc5, 0x7f]),
];

const RUNTIME_A: &[u8] = &[
    0x08, 0xc2, 0x30, 0x48, 0xda, 0x5a, 0xa2, 0x00, 0x00, 0xbf, 0x00, 0xb9, 0x7e, 0x9f, 0x00, 0x20,
    0x7e, 0xe8, 0xe8, 0xe0, 0x00, 0x04, 0xd0, 0xf1, 0x7a, 0xfa, 0x68, 0x28, 0xc2, 0x20, 0xa9, 0xec,
    0x95, 0x48, 0xe2, 0x30, 0x5c, 0xda, 0xa9, 0x00,
];
const RUNTIME_B: &[u8] = &[
    0x08, 0xc2, 0x30, 0x48, 0xda, 0x5a, 0xa2, 0x00, 0x00, 0xbf, 0x00, 0x20, 0x7e, 0x9f, 0x00, 0xb9,
    0x7e, 0xe8, 0xe8, 0xe0, 0x00, 0x04, 0xd0, 0xf1, 0x7a, 0xfa, 0x68, 0x28, 0x22, 0x9e, 0x80, 0x05,
    0x5c, 0x22, 0x0a, 0x9f, 0x59, 0x5c, 0xf9, 0xa5, 0x00,
];
const RUNTIME_C: &[u8] = &[
    0x53, 0x54, 0x41, 0x52, 0x1f, 0x00, 0xe0, 0xff, 0xa9, 0x81, 0x2c, 0x12, 0x42, 0x30, 0xfb, 0x70,
    0xf9, 0x8d, 0x00, 0x42, 0x6b, 0x4c, 0x4d, 0x00, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

/// Installs Lunar Magic's first 4bpp standard-GFX runtime and all 52 editable files into a
/// pristine North-American SMW revision-0 ROM as one revision-bound mutation.
pub fn prepare_smw_us_v1_standard_graphics_install(
    expected_revision: u64,
    image: RomImage,
    files: &[Vec<u8>],
) -> Result<PreparedRomCommit, String> {
    if files.len() != FILE_SIZES.len() {
        return Err(format!(
            "standard GFX installation requires 52 files, got {}",
            files.len()
        ));
    }
    for (number, (bytes, expected)) in files.iter().zip(FILE_SIZES).enumerate() {
        if bytes.len() != expected {
            return Err(format!(
                "GFX{number:02X}: expected {expected} bytes, got {}",
                bytes.len()
            ));
        }
    }
    if image.logical_len() != 0x80_000 {
        return Err(format!(
            "first standard-GFX installation requires a 512 KiB pristine ROM, got {:#X} bytes",
            image.logical_len()
        ));
    }
    authenticate_patches(&image)?;
    let before = image.logical_bytes().to_vec();
    let mut project = Project::new(image);
    project
        .expand_rom(Mapper::LoRom, TARGET_LOGICAL_LEN, 0xff, CHECKSUM_FIELD)
        .map_err(|error| error.to_string())?;
    apply_runtime(&mut project)?;

    let mut options = GraphicsSaveOptions {
        allocation: lm_rats::AllocationPolicy::lorom(0x80_028..TARGET_LOGICAL_LEN),
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    options
        .allocation
        .protected
        .push(ProtectedRange(0x80_000..0x80_028));
    let ordinary = lm_profile::smw_us_v1_vanilla_graphics_layout();
    protect_layout(&mut options, ordinary)?;
    project
        .save_decompressed_graphics_slots_with_checksum(
            &(0..0x32).collect::<Vec<_>>(),
            &files[..0x32],
            ordinary,
            CHECKSUM_FIELD,
            &options,
        )
        .map_err(|error| error.to_string())?;

    let special = crate::graphics_batch_import::prepare_smw_us_v1_special_graphics_import_resized(
        0,
        project.rom.clone(),
        CHECKSUM_FIELD,
        &[files[0x33].clone(), files[0x32].clone()],
        &options,
    )?;
    project
        .apply_mutation(&special.description, &special.mutation)
        .map_err(|error| error.to_string())?;
    project
        .rom
        .write(ROM_SIZE_FIELD, &[0x0a])
        .map_err(|error| error.to_string())?;
    project
        .rom
        .update_snes_checksum(CHECKSUM_FIELD)
        .map_err(|error| error.to_string())?;
    verify_reopen(&project.rom, files)?;
    let mutation = RomMutation::between(Mapper::LoRom, &before, project.rom.logical_bytes())
        .map_err(|error| error.to_string())?;
    Ok(PreparedRomCommit {
        expected_revision,
        description: "Install 4bpp standard GFX system".into(),
        mutation,
    })
}

/// Splits Lunar Magic's canonical joined 52-file image and prepares the same first installation.
pub fn prepare_smw_us_v1_joined_standard_graphics_install(
    expected_revision: u64,
    image: RomImage,
    joined: &[u8],
) -> Result<PreparedRomCommit, String> {
    let expected = FILE_SIZES.iter().sum::<usize>();
    if joined.len() != expected {
        return Err(format!(
            "AllGFX.bin: expected {expected} bytes for pristine SMW-US, got {}",
            joined.len()
        ));
    }
    let mut cursor = 0;
    let files = FILE_SIZES
        .iter()
        .map(|size| {
            let end = cursor + size;
            let file = joined[cursor..end].to_vec();
            cursor = end;
            file
        })
        .collect::<Vec<_>>();
    prepare_smw_us_v1_standard_graphics_install(expected_revision, image, &files)
}

fn authenticate_patches(image: &RomImage) -> Result<(), String> {
    for patch in PATCHES {
        let actual = image
            .read(patch.offset, patch.before.len())
            .map_err(|error| error.to_string())?;
        if actual != patch.before {
            return Err(format!(
                "unsupported standard-GFX runtime bytes at {:#08X}",
                patch.offset
            ));
        }
    }
    for (offset, before, _) in RAM_REFERENCE_PATCHES {
        if image.read(*offset, 2).map_err(|error| error.to_string())? != before {
            return Err(format!(
                "unsupported graphics RAM reference at {offset:#08X}"
            ));
        }
    }
    Ok(())
}

fn apply_runtime(project: &mut Project) -> Result<(), String> {
    for patch in PATCHES {
        project
            .rom
            .write(patch.offset, patch.after)
            .map_err(|error| error.to_string())?;
    }
    for (offset, _, after) in RAM_REFERENCE_PATCHES {
        project
            .rom
            .write(*offset, after)
            .map_err(|error| error.to_string())?;
    }
    for (offset, bytes) in [
        (0x77c50, RUNTIME_A),
        (0x77c80, RUNTIME_B),
        (0x80000, RUNTIME_C),
    ] {
        project
            .rom
            .write(offset, bytes)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn protect_layout(
    options: &mut GraphicsSaveOptions,
    layout: lm_project::GraphicsRomLayout,
) -> Result<(), String> {
    let planes = layout
        .split_pointer_planes
        .ok_or("standard graphics pointer table is not split")?;
    for offset in [planes.low_offset, planes.high_offset, planes.bank_offset] {
        options
            .allocation
            .protected
            .push(ProtectedRange(offset..offset + planes.entries));
    }
    Ok(())
}

fn verify_reopen(image: &RomImage, files: &[Vec<u8>]) -> Result<(), String> {
    if !lm_profile::has_smw_us_v1_4bpp_graphics_prerequisite(image) {
        return Err("installed standard-GFX format markers did not reopen".into());
    }
    let project = Project::new(image.clone());
    let ordinary = lm_profile::smw_us_v1_vanilla_graphics_layout();
    for (slot, expected) in files.iter().take(0x32).enumerate() {
        let actual = project
            .load_decompressed_graphics_file(slot, ordinary)
            .map_err(|error| format!("GFX{slot:02X}: {error}"))?;
        if &actual != expected {
            return Err(format!("GFX{slot:02X}: semantic reopen mismatch"));
        }
    }
    let special = lm_profile::smw_us_v1_special_graphics_layouts(image)
        .map_err(|error| format!("special graphics startup layout: {error}"))?;
    for (number, layout) in [(0x32, special.gfx32), (0x33, special.gfx33)] {
        let actual = project
            .load_decompressed_graphics_file(0, layout)
            .map_err(|error| format!("GFX{number:02X}: {error}"))?;
        if actual != files[number] {
            return Err(format!("GFX{number:02X}: semantic reopen mismatch"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn first_install_rejects_incomplete_input_before_mutation() {
        let image = RomImage::from_bytes(vec![0; 0x80_000]).unwrap();
        assert!(prepare_smw_us_v1_standard_graphics_install(0, image, &[]).is_err());
    }

    #[test]
    fn joined_first_install_rejects_inexact_shape_before_rom_authentication() {
        let image = RomImage::from_bytes(vec![0; 0x80_000]).unwrap();
        assert!(prepare_smw_us_v1_joined_standard_graphics_install(0, image, &[0; 1]).is_err());
    }

    #[test]
    #[ignore = "requires a retained pristine SMW-US ROM and Lunar Magic Graphics directory"]
    fn retained_first_install_reopens_all_52_lunar_magic_files_and_undoes() {
        let rom = std::env::var_os("LM_PRISTINE_GFX_ROM").expect("LM_PRISTINE_GFX_ROM");
        let directory =
            PathBuf::from(std::env::var_os("LM_GFX_EXPORT_DIR").expect("LM_GFX_EXPORT_DIR"));
        let original = RomImage::from_bytes(fs::read(rom).unwrap()).unwrap();
        let files = (0..0x34)
            .map(|number| fs::read(directory.join(format!("GFX{number:02X}.bin"))).unwrap())
            .collect::<Vec<_>>();
        let commit =
            prepare_smw_us_v1_standard_graphics_install(7, original.clone(), &files).unwrap();
        assert_eq!(commit.expected_revision, 7);
        let mut project = Project::new(original.clone());
        project
            .apply_mutation(&commit.description, &commit.mutation)
            .unwrap();
        assert_eq!(project.rom.logical_len(), TARGET_LOGICAL_LEN);
        verify_reopen(&project.rom, &files).unwrap();
        if let Some(output) = std::env::var_os("LM_STANDARD_GFX_OUTPUT_ROM") {
            fs::write(output, project.rom.as_file_bytes()).unwrap();
        }
        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), original.as_file_bytes());

        let joined = files.iter().flatten().copied().collect::<Vec<_>>();
        let joined_commit =
            prepare_smw_us_v1_joined_standard_graphics_install(8, original.clone(), &joined)
                .unwrap();
        let mut joined_project = Project::new(original);
        joined_project
            .apply_mutation(&joined_commit.description, &joined_commit.mutation)
            .unwrap();
        verify_reopen(&joined_project.rom, &files).unwrap();
    }
}
