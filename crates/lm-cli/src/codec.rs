use crate::args::CodecOperation;
use crate::atomic_output::write_new;
use crate::oracle_input::read_bounded;
use lm_codec::{
    decode_lz2, decode_lz3, decode_sized_rle, decode_terminated_rle, encode_lz2, encode_lz3,
    encode_sized_rle, encode_terminated_rle,
};
use std::fs;
use std::io;
use std::path::Path;

const MAX_CODEC_BYTES: usize = 16 * 1024 * 1024;

pub fn transform(
    operation: CodecOperation,
    input: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_new_distinct(input, output)?;
    let source = read_bounded(input, MAX_CODEC_BYTES)?;
    let result = match operation {
        CodecOperation::Lz2Decode => decode_lz2(&source, 0x100_0000)?,
        CodecOperation::Lz2Encode => encode_lz2(&source),
        CodecOperation::Lz3Decode => decode_lz3(&source, 0x100_0000)?,
        CodecOperation::Lz3Encode => encode_lz3(&source),
        CodecOperation::RleDecode => decode_terminated_rle(&source, 0x100_0000)?,
        CodecOperation::RleEncode => encode_terminated_rle(&source),
        CodecOperation::RleSizedEncode => encode_sized_rle(&source),
    };
    if result.len() > MAX_CODEC_BYTES {
        return Err("codec output exceeds the bounded file limit".into());
    }
    write_new(output, result)?;
    Ok(())
}

pub fn decode_sized(
    input: &Path,
    output: &Path,
    expected_len: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    require_new_distinct(input, output)?;
    if expected_len > MAX_CODEC_BYTES {
        return Err("expected RLE output exceeds the bounded file limit".into());
    }
    let decoded = decode_sized_rle(&read_bounded(input, MAX_CODEC_BYTES)?, expected_len)?;
    write_new(output, decoded)?;
    Ok(())
}

fn require_new_distinct(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        return Err("codec output must differ from input".into());
    }
    match fs::symlink_metadata(output) {
        Ok(_) => {
            Err(io::Error::new(io::ErrorKind::AlreadyExists, "codec output already exists").into())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-cli-codec-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn transforms_refuse_to_overwrite_input() {
        let path = Path::new("same.bin");
        assert!(transform(CodecOperation::Lz2Encode, path, path).is_err());
        assert!(decode_sized(path, path, 1).is_err());
    }

    #[test]
    fn oversized_inputs_and_declared_outputs_are_rejected_before_publication() {
        let directory = directory();
        let input = directory.join("oversized.bin");
        let output = directory.join("output.bin");
        fs::File::create(&input)
            .unwrap()
            .set_len(u64::try_from(MAX_CODEC_BYTES + 1).unwrap())
            .unwrap();
        assert!(transform(CodecOperation::Lz2Encode, &input, &output).is_err());
        assert!(!output.exists());

        fs::write(&input, [0xff, 0xff]).unwrap();
        assert!(decode_sized(&input, &output, MAX_CODEC_BYTES + 1).is_err());
        assert!(!output.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn existing_output_is_rejected_before_input_is_opened() {
        let directory = directory();
        let missing_input = directory.join("missing.bin");
        let output = directory.join("existing.bin");
        fs::write(&output, [7, 8, 9]).unwrap();
        assert!(transform(CodecOperation::Lz3Encode, &missing_input, &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), [7, 8, 9]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn hard_link_input_alias_is_preserved() {
        let directory = directory();
        let input = directory.join("input.bin");
        let output = directory.join("alias.bin");
        fs::write(&input, [1, 2, 3]).unwrap();
        fs::hard_link(&input, &output).unwrap();
        assert!(transform(CodecOperation::Lz2Encode, &input, &output).is_err());
        assert_eq!(fs::read(&input).unwrap(), [1, 2, 3]);
        assert_eq!(fs::read(&output).unwrap(), [1, 2, 3]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn standalone_decoders_reject_trailing_data_without_publishing_output() {
        let directory = directory();
        for (operation, bytes) in [
            (CodecOperation::Lz2Decode, &[0x00, 7, 0xff, 0xaa][..]),
            (CodecOperation::Lz3Decode, &[0x00, 7, 0xff, 0xaa][..]),
            (CodecOperation::RleDecode, &[0x00, 7, 0xff, 0xff, 0xaa][..]),
        ] {
            let input = directory.join(format!("{operation:?}.in"));
            let output = directory.join(format!("{operation:?}.out"));
            fs::write(&input, bytes).unwrap();
            assert!(transform(operation, &input, &output).is_err());
            assert!(!output.exists());
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn lz3_file_workflow_round_trips_without_replacing_outputs() {
        let directory = directory();
        let raw = directory.join("Raw 日本語.bin");
        let packed = directory.join("Packed data.lz3");
        let decoded = directory.join("Decoded data.bin");
        let mut bytes = vec![0; 96];
        bytes.extend([0x12, 0x34].into_iter().cycle().take(65));
        bytes.extend_from_slice(b"LZ3 literal tail");
        fs::write(&raw, &bytes).unwrap();
        transform(CodecOperation::Lz3Encode, &raw, &packed).unwrap();
        transform(CodecOperation::Lz3Decode, &packed, &decoded).unwrap();
        assert_eq!(fs::read(&decoded).unwrap(), bytes);
        assert!(transform(CodecOperation::Lz3Encode, &raw, &packed).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
