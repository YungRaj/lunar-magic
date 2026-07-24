use crate::atomic_output::{write_new, write_new_batch};
use crate::oracle_input::{read_bounded, read_rom};
use lm_oracle::observe_rats_manifest;
use lm_project::{Project, RatsOwnershipManifest, RatsOwnershipManifestFile, RatsReclamationPlan};
use lm_rats::parse_at;
use lm_rom::{RomImage, SnesChecksum, compute_snes_checksum, detect_identity};
use std::path::Path;

#[cfg(test)]
use std::fs;

pub fn inspect_manifest(
    input: &Path,
    normalized_output: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if [normalized_output, observation]
        .into_iter()
        .flatten()
        .any(|path| path == input)
    {
        return Err("RATS manifest outputs must differ from input".into());
    }
    if normalized_output.is_some() && normalized_output == observation {
        return Err("normalized RATS manifest and observation outputs must differ".into());
    }
    let manifest = read_manifest(input)?;
    println!("owned-blocks: {}", manifest.owned.len());
    println!("retained-blocks: {}", manifest.retained.len());
    for block in &manifest.owned {
        let disposition = if manifest.retained.contains(block) {
            "retain"
        } else {
            "reclaim"
        };
        println!(
            "{disposition}: {:#x}..{:#x}",
            block.header_offset, block.payload.end
        );
    }
    let encoded = normalized_output
        .map(|_| RatsOwnershipManifestFile(manifest.clone()).encode())
        .transpose()?;
    let observed = observation.map(|_| observe_rats_manifest(&manifest).to_text());
    let mut outputs = Vec::new();
    if let (Some(path), Some(bytes)) = (normalized_output, encoded.as_deref()) {
        outputs.push((path, bytes));
    }
    if let (Some(path), Some(text)) = (observation, observed.as_deref()) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    Ok(())
}

pub fn plan(
    rom_path: &Path,
    manifest_path: &Path,
    fill: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new(RomImage::from_bytes(read_rom(rom_path)?)?);
    let manifest = read_manifest(manifest_path)?;
    let plan = project.plan_rats_reclamation(&manifest, fill)?;
    print_plan(&plan);
    Ok(())
}

pub fn execute(
    input: &Path,
    output: &Path,
    manifest_path: &Path,
    fill: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        return Err("refusing to overwrite the input ROM; choose a different output path".into());
    }
    let bytes = read_rom(input)?;
    let rom = RomImage::from_bytes(bytes)?;
    let identity = detect_identity(&rom)?;
    let manifest = read_manifest(manifest_path)?;
    let (output_bytes, plan, checksum) =
        prepare(rom, &manifest, fill, identity.internal_header_offset + 0x1c)?;
    verify_output(
        &output_bytes,
        &manifest,
        &plan,
        fill,
        identity.internal_header_offset + 0x1c,
    )?;
    write_new(output, output_bytes)?;
    print_plan(&plan);
    println!("checksum: {:#06x}", checksum.checksum);
    println!("complement: {:#06x}", checksum.complement);
    Ok(())
}

fn prepare(
    rom: RomImage,
    manifest: &RatsOwnershipManifest,
    fill: u8,
    checksum_offset: usize,
) -> Result<(Vec<u8>, RatsReclamationPlan, SnesChecksum), Box<dyn std::error::Error>> {
    let mut project = Project::new(rom);
    let plan = project.plan_rats_reclamation(manifest, fill)?;
    reject_internal_header_overlap(&plan, checksum_offset)?;
    let (_, checksum) = project.reclaim_owned_rats_with_checksum(
        "reclaim exclusively owned RATS blocks",
        manifest,
        fill,
        checksum_offset,
    )?;
    Ok((project.save_snapshot(), plan, checksum))
}

fn reject_internal_header_overlap(
    plan: &RatsReclamationPlan,
    checksum_offset: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let header_start = checksum_offset
        .checked_sub(0x1c)
        .ok_or("invalid SNES checksum field offset")?;
    let header_end = header_start
        .checked_add(0x40)
        .ok_or("SNES internal-header range overflow")?;
    if plan.reclaimed.iter().any(|block| {
        let range = block.full_range();
        range.start < header_end && header_start < range.end
    }) {
        Err("refusing to reclaim a RATS block overlapping the SNES internal header".into())
    } else {
        Ok(())
    }
}

fn verify_output(
    file_bytes: &[u8],
    manifest: &RatsOwnershipManifest,
    plan: &RatsReclamationPlan,
    fill: u8,
    checksum_offset: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let rom = RomImage::from_bytes(file_bytes.to_vec())?;
    for block in &plan.reclaimed {
        if !rom.logical_bytes()[block.full_range()]
            .iter()
            .all(|byte| *byte == fill)
        {
            return Err("reclaimed RATS range failed post-write verification".into());
        }
    }
    for block in &manifest.retained {
        if parse_at(rom.logical_bytes(), block.header_offset).as_ref() != Ok(block) {
            return Err("retained RATS block failed post-write verification".into());
        }
    }
    let expected = compute_snes_checksum(rom.logical_bytes(), checksum_offset)?;
    if SnesChecksum::decode(rom.logical_bytes(), checksum_offset)? != expected {
        return Err("reclaimed ROM checksum failed post-write verification".into());
    }
    Ok(())
}

fn read_manifest(path: &Path) -> Result<RatsOwnershipManifest, Box<dyn std::error::Error>> {
    let maximum = RatsOwnershipManifestFile::MAX_FILE_LEN;
    let bytes = read_bounded(path, maximum)?;
    Ok(RatsOwnershipManifestFile::decode(&bytes)?.0)
}

fn print_plan(plan: &RatsReclamationPlan) {
    for block in &plan.reclaimed {
        println!(
            "reclaim: {:#x}..{:#x}",
            block.header_offset, block.payload.end
        );
    }
    println!("reclaimed-blocks: {}", plan.reclaimed.len());
    println!("reclaimed-bytes: {}", plan.reclaimed_bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::RatsOwnershipManifest;
    use lm_rats::{AllocationPolicy, FreeSpaceAllocator};

    fn fixture() -> (RomImage, RatsOwnershipManifest) {
        let mut bytes = vec![0xff; 0x8000];
        let mut allocator =
            FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x100..0x7000));
        let reclaimed = allocator.allocate(&[1, 2, 3]).unwrap();
        let retained = allocator.allocate(&[4, 5]).unwrap();
        (
            RomImage::from_bytes(bytes).unwrap(),
            RatsOwnershipManifest {
                owned: vec![reclaimed, retained.clone()],
                retained: vec![retained],
            },
        )
    }

    #[test]
    fn preparation_repairs_checksum_and_verifies_both_block_classes() {
        let (rom, manifest) = fixture();
        let (bytes, plan, _) = prepare(rom, &manifest, 0xff, 0x7fdc).unwrap();
        verify_output(&bytes, &manifest, &plan, 0xff, 0x7fdc).unwrap();
        assert_eq!(plan.reclaimed.len(), 1);
    }

    #[test]
    fn failed_proof_preserves_the_input_image() {
        let (rom, mut manifest) = fixture();
        let before = rom.as_file_bytes().to_vec();
        manifest.owned[0].payload.end += 1;
        assert!(prepare(rom, &manifest, 0xff, 0x7fdc).is_err());
        assert_eq!(before.len(), 0x8000);
    }

    #[test]
    fn internal_header_overlap_is_rejected_before_reclamation() {
        let mut bytes = vec![0xff; 0x8000];
        let offset = 0x7fb0;
        bytes[offset..offset + 8].copy_from_slice(&lm_rats::make_header(0x40).unwrap());
        let block = parse_at(&bytes, offset).unwrap();
        let manifest = RatsOwnershipManifest {
            owned: vec![block],
            retained: Vec::new(),
        };
        assert!(
            prepare(
                RomImage::from_bytes(bytes).unwrap(),
                &manifest,
                0xff,
                0x7fdc
            )
            .is_err()
        );
    }

    #[test]
    fn identical_input_and_output_are_rejected_before_io() {
        let path = Path::new("same.smc");
        assert!(execute(path, path, Path::new("owned.lmrats"), 0xff).is_err());
    }

    #[test]
    fn manifest_normalization_and_observation_publish_as_one_distinct_batch() {
        let path = Path::new("same.lmrats");
        assert!(inspect_manifest(path, Some(path), None).is_err());
        assert!(inspect_manifest(path, Some(Path::new("same")), Some(Path::new("same"))).is_err());
        let directory =
            std::env::temp_dir().join(format!("lm-cli-rats-manifest-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("owned.lmrats");
        let normalized = directory.join("normalized.lmrats");
        let observation = directory.join("owned.obs");
        let (_, manifest) = fixture();
        fs::write(
            &input,
            RatsOwnershipManifestFile(manifest.clone())
                .encode()
                .unwrap(),
        )
        .unwrap();
        inspect_manifest(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            RatsOwnershipManifestFile::decode(&fs::read(normalized).unwrap())
                .unwrap()
                .0,
            manifest
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert_eq!(observed.get("rats-manifest/owned-count"), Some("2"));
        fs::remove_dir_all(directory).unwrap();
    }
}
