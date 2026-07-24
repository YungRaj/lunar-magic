use crate::atomic_output::write_new;
use crate::oracle_input::{MAX_ROM_BYTES, read_bounded};
use lm_rom::RomImage;
use std::path::Path;

pub fn execute(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("RATS observation input and output paths must differ".into());
    }
    let image = RomImage::from_bytes(read_bounded(rom, MAX_ROM_BYTES)?)?;
    let observation = lm_oracle::observe_rats(image.logical_bytes());
    let block_count = observation
        .get("rats/block-count")
        .expect("RATS observations always include a block count");
    write_new(output, observation.to_text())?;
    println!("rats-blocks: {block_count}");
    println!("rats-observation: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_oracle::Observation;
    use lm_rats::make_header;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm-rats-observation-{}-{}-{name}",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn observes_logical_offsets_and_preserves_copier_header_transparency() {
        let input = path("input.smc");
        let output = path("output.obs");
        let mut logical = vec![0xff; 0x8000];
        logical[0x20..0x28].copy_from_slice(&make_header(3).unwrap());
        logical[0x28..0x2b].copy_from_slice(&[1, 2, 3]);
        let mut file = vec![0x55; 0x200];
        file.extend_from_slice(&logical);
        fs::write(&input, file).unwrap();
        execute(&input, &output).unwrap();
        let bytes = fs::read(&output).unwrap();
        let observation = Observation::from_text(std::str::from_utf8(&bytes).unwrap()).unwrap();
        assert_eq!(observation.get("rats/block-count"), Some("1"));
        assert_eq!(
            observation.get("rats/blocks/00000020/payload-start"),
            Some("40")
        );
        assert!(execute(&input, &output).is_err());
        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
    }
}
