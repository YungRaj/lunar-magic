use lm_graphics::{Bgr555, Rgb8, SmwPaletteFile};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ColorForm {
    pub word: u16,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl ColorForm {
    pub(super) fn load(file: &SmwPaletteFile, index: usize) -> Result<Self, String> {
        let color = file
            .palette()
            .map_err(|error| error.to_string())?
            .colors
            .get(index)
            .copied()
            .ok_or_else(|| "shared-palette color index is out of range".to_owned())?;
        Ok(Self::from_color(color))
    }

    pub(super) fn from_color(color: Bgr555) -> Self {
        let rgb = color.to_rgb8();
        Self {
            word: color.0,
            red: rgb.red,
            green: rgb.green,
            blue: rgb.blue,
        }
    }

    pub(super) fn use_word(&mut self) -> Result<(), String> {
        if self.word > 0x7fff {
            return Err("SNES BGR555 color must be 0000–7FFF".into());
        }
        *self = Self::from_color(Bgr555(self.word));
        Ok(())
    }

    pub(super) fn rgb_color(self) -> Bgr555 {
        Bgr555::from_rgb8(Rgb8 {
            red: self.red,
            green: self.green,
            blue: self.blue,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_and_rgb_forms_use_canonical_snes_conversion() {
        let mut form = ColorForm {
            word: 0x7c1f,
            ..ColorForm::default()
        };
        form.use_word().unwrap();
        assert_eq!(form.red, 255);
        assert_eq!(form.green, 0);
        assert_eq!(form.blue, 255);
        assert_eq!(form.rgb_color(), Bgr555(0x7c1f));
        form.word = 0x8000;
        assert!(form.use_word().is_err());
    }
}
