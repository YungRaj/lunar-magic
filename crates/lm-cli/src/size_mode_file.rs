use crate::oracle_input::read_exact;
use std::io;
use std::path::Path;

pub fn read(path: &Path) -> io::Result<[bool; 256]> {
    let bytes = read_exact(path, 256, "ExAnimation size-mode table")?;
    Ok(std::array::from_fn(|index| bytes[index] != 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn exact_shape_and_nonzero_flags_are_required() {
        let path = std::env::temp_dir().join(format!("lm-size-modes-{}", std::process::id()));
        let mut bytes = [0; 256];
        bytes[7] = 2;
        fs::write(&path, bytes).unwrap();
        let modes = read(&path).unwrap();
        assert!(modes[7]);
        fs::write(&path, [0; 255]).unwrap();
        assert_eq!(read(&path).unwrap_err().kind(), io::ErrorKind::InvalidData);
        fs::File::create(&path).unwrap().set_len(1_000_000).unwrap();
        assert_eq!(read(&path).unwrap_err().kind(), io::ErrorKind::InvalidData);
        fs::remove_file(path).unwrap();
    }
}
