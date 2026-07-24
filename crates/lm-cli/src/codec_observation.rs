use crate::atomic_output::write_new;
use crate::oracle_input::read_bounded;
use lm_oracle::{CodecObservationKind, observe_codec};
use std::fs;
use std::io;
use std::path::Path;

const MAX_COMPRESSED_BYTES: usize = 16 * 1024 * 1024;
const MAX_DECOMPRESSED_BYTES: usize = 16 * 1024 * 1024;

pub fn execute(
    kind: CodecObservationKind,
    input: &Path,
    output_bound: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if output_bound > MAX_DECOMPRESSED_BYTES {
        return Err(format!(
            "codec observation output bound {output_bound} exceeds {MAX_DECOMPRESSED_BYTES} bytes"
        )
        .into());
    }
    if input == output {
        return Err("codec observation output must differ from its input".into());
    }
    match fs::symlink_metadata(output) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "codec observation output already exists",
            )
            .into());
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let compressed = read_bounded(input, MAX_COMPRESSED_BYTES)?;
    let observation = observe_codec(kind, &compressed, output_bound)?;
    write_new(output, observation.to_text().into_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_oracle::Observation;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-codec-observe-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn publishes_one_bounded_create_new_observation() {
        let directory = directory();
        let input = directory.join("packed 日本語.lz3");
        let output = directory.join("semantic result.obs");
        fs::write(&input, lm_codec::encode_lz3(b"semantic graphics")).unwrap();
        execute(CodecObservationKind::Lz3, &input, 64, &output).unwrap();
        let text = fs::read_to_string(&output).unwrap();
        let observation = Observation::from_text(&text).unwrap();
        assert_eq!(observation.get("codec/kind"), Some("lz3"));
        assert_eq!(observation.get("codec/decoded-bytes"), Some("17"));
        assert!(execute(CodecObservationKind::Lz3, &input, 64, &output).is_err());
        assert!(
            execute(
                CodecObservationKind::Lz3,
                &input,
                16,
                &directory.join("small.obs")
            )
            .is_err()
        );
        assert!(execute(CodecObservationKind::Lz3, &input, 64, &input).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn sized_rle_requires_the_exact_declared_output_length() {
        let directory = directory();
        let input = directory.join("sized.rle");
        fs::write(&input, lm_codec::encode_sized_rle(b"exact length")).unwrap();
        let output = directory.join("sized.obs");
        execute(CodecObservationKind::RleSized, &input, 12, &output).unwrap();
        assert!(
            execute(
                CodecObservationKind::RleSized,
                &input,
                13,
                &directory.join("wrong.obs")
            )
            .is_err()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn oversized_input_and_output_bound_fail_without_publication() {
        let directory = directory();
        let input = directory.join("oversized.lz2");
        let output = directory.join("result.obs");
        fs::File::create(&input)
            .unwrap()
            .set_len(u64::try_from(MAX_COMPRESSED_BYTES + 1).unwrap())
            .unwrap();
        assert!(execute(CodecObservationKind::Lz2, &input, 1, &output).is_err());
        assert!(!output.exists());

        fs::write(&input, [0xff]).unwrap();
        assert!(
            execute(
                CodecObservationKind::Lz2,
                &input,
                MAX_DECOMPRESSED_BYTES + 1,
                &output,
            )
            .is_err()
        );
        assert!(!output.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn existing_output_is_rejected_before_missing_input_is_opened() {
        let directory = directory();
        let input = directory.join("missing.lz3");
        let output = directory.join("existing.obs");
        fs::write(&output, b"preserve").unwrap();
        assert!(execute(CodecObservationKind::Lz3, &input, 16, &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"preserve");
        fs::remove_dir_all(directory).unwrap();
    }
}
