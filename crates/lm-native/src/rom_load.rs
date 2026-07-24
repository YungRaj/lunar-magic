//! Shared guards for asynchronous ROM-backed editor inputs.

pub(crate) fn ensure_current_revision(
    loaded: u64,
    current: u64,
    subject: &str,
) -> Result<(), String> {
    if loaded != current {
        return Err(format!(
            "the ROM changed while {subject} was loading; reopen the editor"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_rom_input_accepts_only_its_exact_revision() {
        assert!(ensure_current_revision(17, 17, "ownership evidence").is_ok());
        assert!(ensure_current_revision(17, 18, "ownership evidence").is_err());
        assert!(ensure_current_revision(18, 17, "ownership evidence").is_err());
    }
}
