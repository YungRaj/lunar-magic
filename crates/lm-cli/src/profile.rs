use crate::oracle_input::read_rom;
use lm_profile::RevisionProfile;
use lm_rom::RomImage;
use std::fs;
use std::path::Path;

pub fn inspect(
    profile_path: &Path,
    rom_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let profile = RevisionProfile::read_from(fs::File::open(profile_path)?)?;
    if let Some(path) = rom_path {
        let image = RomImage::from_bytes(read_rom(path)?)?;
        let audit = profile.audit_rom(&image)?;
        println!("rom-compatible: yes");
        println!("pointer-entries: {}", audit.total_entries);
        for table in audit.tables {
            println!(
                "pointer-table: {} entries={} unique={} range={:#x}..={:#x}",
                table.domain,
                table.entries,
                table.unique_targets,
                table.minimum_target,
                table.maximum_target
            );
        }
        if let Some(table) = audit.expanded_settings {
            println!(
                "direct-table: {} entries={} stride={:#x} span={:#x}..{:#x}",
                table.domain,
                table.entries,
                table.stride,
                table.byte_span.start,
                table.byte_span.end
            );
        }
    }
    println!("format: {}", RevisionProfile::MAGIC);
    println!("name: {}", profile.name);
    println!("game: {:?}", profile.game);
    println!("region: {:?}", profile.region);
    println!("revision: {}", profile.revision);
    println!("mapper: {:?}", profile.mapper);
    println!("level-slots: {}", profile.level.layer1.entries);
    println!("map16-pages: {}", profile.map16.graphics.entries);
    println!("graphics-slots: {}", profile.graphics.pointers.entries);
    println!("palette-slots: {}", profile.palette.pointers.entries);
    println!(
        "exanimation-slots: {}",
        profile.exanimation.pointers.entries
    );
    if let Some(layout) = profile.expanded_settings {
        println!("expanded-settings-slots: {}", layout.entries);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use lm_rom::{Region, RomIdentity, SnesChecksum};

    #[test]
    fn compatibility_validation_uses_shared_profile_identity() {
        let profile = lm_profile::test_support::profile();
        let mut identity = RomIdentity {
            game: profile.game,
            mapper: profile.mapper,
            region: profile.region,
            revision: profile.revision,
            map_mode: 0x20,
            cartridge_type: 2,
            internal_header_offset: 0x7fc0,
            stored_checksum: SnesChecksum {
                complement: 0xffff,
                checksum: 0,
            },
            computed_checksum: SnesChecksum {
                complement: 0xffff,
                checksum: 0,
            },
        };
        profile.ensure_identity(&identity).unwrap();
        identity.region = Region::Japan;
        assert!(profile.ensure_identity(&identity).is_err());
    }
}
