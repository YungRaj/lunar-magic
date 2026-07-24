use crate::oracle_input::read_exact;
use lm_level::SpriteLengthTable;
use std::{io, path::Path};

pub fn read(path: Option<&Path>) -> Result<SpriteLengthTable, io::Error> {
    match path {
        None => Ok(SpriteLengthTable::standard()),
        Some(path) => {
            let bytes = read_exact(path, SpriteLengthTable::ENCODED_LEN, "sprite length table")?;
            SpriteLengthTable::decode(&bytes).map_err(|actual| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "sprite length table must contain exactly {} bytes, got {actual}",
                        SpriteLengthTable::ENCODED_LEN
                    ),
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn standard_requires_no_file_and_custom_tables_are_exactly_sized() {
        assert_eq!(read(None).unwrap(), SpriteLengthTable::standard());
        let path =
            std::env::temp_dir().join(format!("lm-cli-sprite-length-file-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        fs::write(&path, [3; SpriteLengthTable::ENCODED_LEN]).unwrap();
        assert!(read(Some(&path)).is_ok());
        fs::write(&path, [3; 3]).unwrap();
        assert_eq!(
            read(Some(&path)).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        fs::File::create(&path).unwrap().set_len(1_000_000).unwrap();
        assert_eq!(
            read(Some(&path)).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        fs::remove_file(path).unwrap();
    }
}
