pub(crate) fn decode(bytes: &[u8]) -> Result<[bool; 256], String> {
    if bytes.len() != 256 {
        return Err(format!(
            "ExAnimation size-mode table must contain exactly 256 bytes, got {}",
            bytes.len()
        ));
    }
    Ok(std::array::from_fn(|index| bytes[index] != 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_mode_table_requires_all_entries_and_maps_nonzero() {
        assert!(decode(&[0; 255]).is_err());
        let mut bytes = [0; 256];
        bytes[73] = 2;
        let modes = decode(&bytes).unwrap();
        assert!(modes[73]);
        assert!(!modes[72]);
    }
}
