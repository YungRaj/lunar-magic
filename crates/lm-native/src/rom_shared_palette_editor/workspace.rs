use lm_graphics::{Bgr555, SmwPaletteBackend, SmwPaletteFile};

#[derive(Clone)]
pub(super) struct Workspace {
    pub revision: u64,
    pub original: SmwPaletteFile,
    pub current: SmwPaletteFile,
}

impl Workspace {
    pub(super) fn replace_file(&mut self, file: SmwPaletteFile) -> Result<(), String> {
        if self.current.backend() == SmwPaletteBackend::Expanded
            && file.backend() == SmwPaletteBackend::Legacy
        {
            return Err(
                "an installed expanded shared-palette runtime cannot be downgraded by import"
                    .into(),
            );
        }
        self.current = file;
        Ok(())
    }

    pub(super) fn replace_color(&mut self, index: usize, color: Bgr555) -> Result<(), String> {
        if color.0 > 0x7fff {
            return Err("SNES BGR555 color must be 0000–7FFF".into());
        }
        let mut bytes = self.current.encode();
        let palette_len = self.current.palette_bytes().len();
        let offset = index
            .checked_mul(2)
            .filter(|offset| offset.saturating_add(2) <= palette_len)
            .ok_or_else(|| "shared-palette color index is out of range".to_owned())?;
        bytes[offset..offset + 2].copy_from_slice(&color.0.to_le_bytes());
        self.current = SmwPaletteFile::decode(&bytes).map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(super) fn replace_auxiliary(&mut self, auxiliary: Vec<u8>) -> Result<(), String> {
        if auxiliary.len() != self.current.auxiliary_bytes().len() {
            return Err(format!(
                "auxiliary region requires exactly {} bytes",
                self.current.auxiliary_bytes().len()
            ));
        }
        self.current = SmwPaletteFile::expanded(self.current.palette_bytes().to_vec(), auxiliary)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(super) fn dirty(&self) -> bool {
        self.current != self.original
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_preserve_backend_shape_and_unselected_bytes() {
        let original = SmwPaletteFile::expanded(vec![0; 0x800], (0_u8..16).collect()).unwrap();
        let mut workspace = Workspace {
            revision: 4,
            original: original.clone(),
            current: original,
        };
        workspace.replace_color(0x123, Bgr555(0x4210)).unwrap();
        assert_eq!(
            &workspace.current.palette_bytes()[0x246..0x248],
            &0x4210_u16.to_le_bytes()
        );
        assert_eq!(
            workspace.current.auxiliary_bytes(),
            &(0_u8..16).collect::<Vec<_>>()
        );
        assert!(workspace.replace_color(0x400, Bgr555(1)).is_err());
        assert!(workspace.replace_auxiliary(vec![0; 15]).is_err());
        assert!(workspace.dirty());
    }

    #[test]
    fn complete_file_replacement_allows_install_upgrade_but_rejects_downgrade_atomically() {
        let legacy = SmwPaletteFile::legacy(vec![0; SmwPaletteFile::LEGACY_PALETTE_LEN]).unwrap();
        let expanded = SmwPaletteFile::expanded(
            vec![0; SmwPaletteFile::EXPANDED_PALETTE_LEN],
            vec![0; SmwPaletteFile::EXPANDED_AUXILIARY_LEN],
        )
        .unwrap();
        let mut workspace = Workspace {
            revision: 0,
            original: legacy.clone(),
            current: legacy,
        };
        workspace.replace_file(expanded.clone()).unwrap();
        assert_eq!(workspace.current, expanded);
        let accepted = workspace.current.clone();
        let legacy = SmwPaletteFile::legacy(vec![0; SmwPaletteFile::LEGACY_PALETTE_LEN]).unwrap();
        assert!(workspace.replace_file(legacy).is_err());
        assert_eq!(workspace.current, accepted);
    }
}
