use std::fs;
use std::io::{self, Read};
use std::path::Path;

pub const MAX_ROM_BYTES: usize = 32 * 1024 * 1024;

pub fn read_rom(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    read_bounded(path, MAX_ROM_BYTES)
}

pub fn read_exact(
    path: impl AsRef<Path>,
    expected: usize,
    description: &str,
) -> io::Result<Vec<u8>> {
    let bytes = read_bounded(path, expected)?;
    if bytes.len() != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{description} must contain exactly {expected} bytes, got {}",
                bytes.len()
            ),
        ));
    }
    Ok(bytes)
}

pub fn read_bounded(path: impl AsRef<Path>, maximum: usize) -> io::Result<Vec<u8>> {
    let path = path.as_ref();
    let path_metadata = fs::symlink_metadata(path)?;
    if !path_metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("input {} is not a regular file", path.display()),
        ));
    }
    let limit = u64::try_from(maximum)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "input limit overflow"))?;
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("input {} is not a regular file", path.display()),
        ));
    }
    if metadata.len() > u64::try_from(maximum).unwrap_or(u64::MAX) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("input {} exceeds {maximum} bytes", path.display()),
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("input {} exceeds {maximum} bytes", path.display()),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn bounded_reader_rejects_excess_before_returning_fixture_bytes() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lm-oracle-input-{}-{nonce}.bin",
            std::process::id()
        ));
        fs::write(&path, [1, 2, 3, 4]).unwrap();
        assert_eq!(read_bounded(&path, 4).unwrap(), [1, 2, 3, 4]);
        assert_eq!(
            read_bounded(&path, 3).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn shared_rom_reader_enforces_the_workspace_rom_ceiling() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lm-rom-input-bound-{}-{nonce}.smc",
            std::process::id()
        ));
        fs::File::create(&path)
            .unwrap()
            .set_len(u64::try_from(MAX_ROM_BYTES + 1).unwrap())
            .unwrap();
        assert_eq!(
            read_rom(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn bounded_reader_rejects_non_regular_inputs() {
        let path =
            std::env::temp_dir().join(format!("lm-oracle-input-directory-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        assert_eq!(
            read_bounded(&path, 4).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        fs::remove_dir(path).unwrap();
    }
}
