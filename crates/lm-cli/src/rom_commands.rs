use crate::atomic_output::write_new;
use crate::oracle_input::{MAX_ROM_BYTES, read_bounded};
use lm_project::Project;
use lm_rom::{
    COPIER_HEADER_LEN, CopierHeader, MAX_IPS_IMAGE_LEN, MAX_IPS_PATCH_LEN, Mapper, RomImage,
    apply_ips, create_ips, detect_identity,
};
use std::path::Path;

pub fn checksum(
    input: &Path,
    output: &Path,
    field_offset: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct_paths(input, output)?;
    let mut rom = RomImage::from_bytes(read_bounded(input, MAX_ROM_BYTES)?)?;
    let checksum = rom.update_snes_checksum(field_offset)?;
    write_new(output, rom.as_file_bytes())?;
    println!("checksum: {:#06x}", checksum.checksum);
    println!("complement: {:#06x}", checksum.complement);
    Ok(())
}

pub fn checksum_auto(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct_paths(input, output)?;
    let mut rom = RomImage::from_bytes(read_bounded(input, MAX_ROM_BYTES)?)?;
    let identity = detect_identity(&rom)?;
    let checksum = rom.update_snes_checksum(identity.internal_header_offset + 0x1c)?;
    write_new(output, rom.as_file_bytes())?;
    println!("mapper: {:?}", identity.mapper);
    println!("checksum: {:#06x}", checksum.checksum);
    println!("complement: {:#06x}", checksum.complement);
    Ok(())
}

pub fn expand(
    input: &Path,
    output: &Path,
    mapper: Mapper,
    target_logical_len: usize,
    fill: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct_paths(input, output)?;
    if target_logical_len > MAX_ROM_BYTES {
        return Err(format!("expanded ROM exceeds the {MAX_ROM_BYTES:#x}-byte file limit").into());
    }
    let image = RomImage::from_bytes(read_bounded(input, MAX_ROM_BYTES)?)?;
    let identity = detect_identity(&image)?;
    if identity.mapper != mapper {
        return Err(format!(
            "requested mapper {mapper:?} does not match detected mapper {:?}",
            identity.mapper
        )
        .into());
    }
    let checksum_field = identity.internal_header_offset + 0x1c;
    let mut project = Project::open_supported(image)?;
    let checksum = project
        .expand_rom(mapper, target_logical_len, fill, checksum_field)?
        .ok_or("ROM expansion target must be larger than the input")?;
    let snapshot = project.save_snapshot();
    let reopened = RomImage::from_bytes(snapshot.clone())?;
    let reopened_identity = detect_identity(&reopened)?;
    if reopened.logical_len() != target_logical_len
        || reopened_identity.mapper != mapper
        || !reopened_identity.checksum_matches()
    {
        return Err("expanded ROM failed semantic reopen verification".into());
    }
    write_new(output, snapshot)?;
    println!("logical-size: {target_logical_len:#x}");
    println!("checksum: {:#06x}", checksum.checksum);
    println!("complement: {:#06x}", checksum.complement);
    Ok(())
}

pub fn patch(
    input: &Path,
    output: &Path,
    offset: usize,
    bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct_paths(input, output)?;
    let mut rom = RomImage::from_bytes(read_bounded(input, MAX_ROM_BYTES)?)?;
    rom.write(offset, bytes)?;
    write_new(output, rom.as_file_bytes())?;
    Ok(())
}

pub fn convert_copier_header(
    input: &Path,
    output: &Path,
    header: CopierHeader,
    fill: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct_paths(input, output)?;
    let mut rom = RomImage::from_bytes(read_bounded(input, MAX_ROM_BYTES)?)?;
    if rom.copier_header() == header {
        return Err("ROM already has the requested copier-header state".into());
    }
    if header == CopierHeader::Present
        && rom
            .as_file_bytes()
            .len()
            .checked_add(COPIER_HEADER_LEN)
            .is_none_or(|length| length > MAX_ROM_BYTES)
    {
        return Err("headered ROM would exceed the bounded ROM file limit".into());
    }
    rom.set_copier_header(header, fill);
    write_new(output, rom.as_file_bytes())?;
    println!("copier-header: {header:?}");
    Ok(())
}

pub fn ips_apply(
    source: &Path,
    patch: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_output_distinct(output, &[source, patch])?;
    let source_bytes = read_bounded(source, MAX_IPS_IMAGE_LEN)?;
    let patch_bytes = read_bounded(patch, MAX_IPS_PATCH_LEN)?;
    let result = apply_ips(&source_bytes, &patch_bytes)?;
    write_new(output, result)?;
    Ok(())
}

pub fn ips_create(
    before: &Path,
    after: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_output_distinct(output, &[before, after])?;
    let before_bytes = read_bounded(before, MAX_IPS_IMAGE_LEN)?;
    let after_bytes = read_bounded(after, MAX_IPS_IMAGE_LEN)?;
    let patch = create_ips(&before_bytes, &after_bytes)?;
    write_new(output, patch)?;
    Ok(())
}

fn require_output_distinct(output: &Path, inputs: &[&Path]) -> Result<(), &'static str> {
    if inputs.contains(&output) {
        Err("IPS output must differ from every input")
    } else {
        Ok(())
    }
}

fn require_distinct_paths(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        return Err("refusing to overwrite the input ROM; choose a different output path".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::compute_snes_checksum;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn supported_rom(headered: bool) -> Vec<u8> {
        let mut logical = vec![0x11; 0x8000];
        logical[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        logical[0x7fd5] = 0x20;
        logical[0x7fd9] = 1;
        logical[0x7fdb] = 0;
        let checksum = compute_snes_checksum(&logical, 0x7fdc).unwrap();
        logical[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        if headered {
            let mut bytes = vec![0x5a; 0x200];
            bytes.extend(logical);
            bytes
        } else {
            logical
        }
    }

    #[test]
    fn identical_paths_are_rejected_before_io() {
        assert!(require_distinct_paths(Path::new("x"), Path::new("x")).is_err());
    }

    #[test]
    fn expansion_preserves_copier_header_and_reopens_with_valid_checksum() {
        let directory = std::env::temp_dir().join(format!(
            "lm-rom-expand-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let input = directory.join("input 日本語.smc");
        let output = directory.join("expanded output.smc");
        let original = supported_rom(true);
        fs::write(&input, &original).unwrap();
        expand(&input, &output, Mapper::LoRom, 0x1_0000, 0xff).unwrap();
        let bytes = fs::read(&output).unwrap();
        assert_eq!(&bytes[..0x200], &original[..0x200]);
        let rom = RomImage::from_bytes(bytes).unwrap();
        assert_eq!(rom.logical_len(), 0x1_0000);
        assert!(detect_identity(&rom).unwrap().checksum_matches());
        assert!(expand(&input, &output, Mapper::LoRom, 0x1_0000, 0xff).is_err());
        assert!(expand(&input, &input, Mapper::LoRom, 0x1_0000, 0xff).is_err());
        assert!(
            expand(
                &input,
                &directory.join("wrong.smc"),
                Mapper::Sa1,
                0x1_0000,
                0xff
            )
            .is_err()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rom_mutation_commands_reject_oversized_input_without_output() {
        let directory = std::env::temp_dir().join(format!(
            "lm-rom-bounds-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let input = directory.join("oversized.smc");
        let output = directory.join("output.smc");
        fs::File::create(&input)
            .unwrap()
            .set_len(u64::try_from(MAX_ROM_BYTES + 1).unwrap())
            .unwrap();
        assert!(checksum(&input, &output, 0x7fdc).is_err());
        assert!(checksum_auto(&input, &output).is_err());
        assert!(patch(&input, &output, 0, &[1]).is_err());
        assert!(!output.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn ips_create_and_apply_round_trip_through_create_new_files() {
        let directory = std::env::temp_dir().join(format!(
            "lm-ips-workflow-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let before = directory.join("before.smc");
        let after = directory.join("after.smc");
        let patch = directory.join("change.ips");
        let output = directory.join("output.smc");
        fs::write(&before, b"0123456789").unwrap();
        fs::write(&after, b"01AAAA6789-extra").unwrap();
        ips_create(&before, &after, &patch).unwrap();
        ips_apply(&before, &patch, &output).unwrap();
        assert_eq!(fs::read(&output).unwrap(), fs::read(&after).unwrap());
        assert!(ips_apply(&before, &patch, &output).is_err());
        assert!(ips_create(&before, &after, &before).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn copier_header_add_remove_preserves_every_logical_byte() {
        let directory = std::env::temp_dir().join(format!(
            "lm-copier-header-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let plain = directory.join("plain.smc");
        let headered = directory.join("headered.smc");
        let restored = directory.join("restored.smc");
        let logical = vec![0x5a; 0x8000];
        fs::write(&plain, &logical).unwrap();
        convert_copier_header(&plain, &headered, CopierHeader::Present, 0xa5).unwrap();
        let headered_image = RomImage::from_bytes(fs::read(&headered).unwrap()).unwrap();
        assert_eq!(headered_image.copier_header(), CopierHeader::Present);
        assert_eq!(headered_image.logical_bytes(), logical);
        assert!(
            headered_image.as_file_bytes()[..COPIER_HEADER_LEN]
                .iter()
                .all(|byte| *byte == 0xa5)
        );
        convert_copier_header(&headered, &restored, CopierHeader::Absent, 0).unwrap();
        assert_eq!(fs::read(&restored).unwrap(), logical);
        assert!(convert_copier_header(&plain, &restored, CopierHeader::Absent, 0).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
