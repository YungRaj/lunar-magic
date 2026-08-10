//! Bounded decoding of standard and extended Win32 dialog templates.

const DS_SETFONT: u32 = 0x0000_0040;

/// A text-bearing control recovered from a Win32 dialog template.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OriginalLanguageDialogControl {
    /// Standard templates publish a 16-bit ID; extended templates publish a 32-bit ID.
    pub id: u32,
    /// Numeric predefined class ordinal, or `None` for a named/custom class.
    pub class_ordinal: Option<u16>,
    /// Literal Unicode caption. Ordinal image/resource titles intentionally produce `None`.
    pub text: Option<String>,
}

/// User-visible text recovered from one standard or extended Win32 dialog resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OriginalLanguageDialogTemplate {
    pub extended: bool,
    pub title: Option<String>,
    pub controls: Vec<OriginalLanguageDialogControl>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OriginalLanguageDialogTemplateError {
    Truncated,
    InvalidExtendedVersion(u16),
    InvalidUtf16,
    AlignmentOverflow,
    TrailingData,
}

impl std::fmt::Display for OriginalLanguageDialogTemplateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "original language dialog template error: {self:?}"
        )
    }
}

impl std::error::Error for OriginalLanguageDialogTemplateError {}

/// Decodes one Win32 `DLGTEMPLATE` or `DLGTEMPLATEEX` resource without invoking Win32.
///
/// Only literal dialog/control text is published. Menu names, custom class names, font names,
/// image ordinals, geometry, styles, and creation data are validated and consumed but discarded.
///
/// # Errors
///
/// Returns [`OriginalLanguageDialogTemplateError`] for truncated framing, an invalid extended
/// version, invalid UTF-16, alignment overflow, or non-padding bytes after the declared controls.
pub fn decode_original_language_dialog_template(
    bytes: &[u8],
) -> Result<OriginalLanguageDialogTemplate, OriginalLanguageDialogTemplateError> {
    let extended = bytes.get(2..4) == Some(&0xffff_u16.to_le_bytes());
    if extended {
        decode_extended(bytes)
    } else {
        decode_standard(bytes)
    }
}

fn decode_standard(
    bytes: &[u8],
) -> Result<OriginalLanguageDialogTemplate, OriginalLanguageDialogTemplateError> {
    let mut reader = Reader::new(bytes);
    let style = reader.u32()?;
    reader.skip(4)?; // extended style
    let control_count = reader.u16()?;
    reader.skip(8)?; // x, y, cx, cy
    reader.value()?; // menu
    reader.value()?; // class
    let title = reader.value()?.text();
    if style & DS_SETFONT != 0 {
        reader.skip(2)?; // point size
        reader.value()?; // typeface
    }

    let mut controls = Vec::new();
    for _ in 0..control_count {
        reader.align4()?;
        reader.skip(8)?; // style, extended style
        reader.skip(8)?; // x, y, cx, cy
        let id = u32::from(reader.u16()?);
        let class = reader.value()?;
        let text = reader.value()?.text();
        let creation_bytes = usize::from(reader.u16()?);
        reader.skip(creation_bytes)?;
        controls.push(OriginalLanguageDialogControl {
            id,
            class_ordinal: class.ordinal(),
            text,
        });
    }
    reader.finish_padding()?;
    Ok(OriginalLanguageDialogTemplate {
        extended: false,
        title,
        controls,
    })
}

fn decode_extended(
    bytes: &[u8],
) -> Result<OriginalLanguageDialogTemplate, OriginalLanguageDialogTemplateError> {
    let mut reader = Reader::new(bytes);
    let version = reader.u16()?;
    if version != 1 {
        return Err(OriginalLanguageDialogTemplateError::InvalidExtendedVersion(
            version,
        ));
    }
    if reader.u16()? != 0xffff {
        return Err(OriginalLanguageDialogTemplateError::Truncated);
    }
    reader.skip(8)?; // help ID, extended style
    let style = reader.u32()?;
    let control_count = reader.u16()?;
    reader.skip(8)?; // x, y, cx, cy
    reader.value()?; // menu
    reader.value()?; // class
    let title = reader.value()?.text();
    if style & DS_SETFONT != 0 {
        reader.skip(6)?; // point size, weight, italic, charset
        reader.value()?; // typeface
    }

    let mut controls = Vec::new();
    for _ in 0..control_count {
        reader.align4()?;
        reader.skip(12)?; // help ID, extended style, style
        reader.skip(8)?; // x, y, cx, cy
        let id = reader.u32()?;
        let class = reader.value()?;
        let text = reader.value()?.text();
        let creation_bytes = usize::from(reader.u16()?);
        reader.skip(creation_bytes)?;
        controls.push(OriginalLanguageDialogControl {
            id,
            class_ordinal: class.ordinal(),
            text,
        });
    }
    reader.finish_padding()?;
    Ok(OriginalLanguageDialogTemplate {
        extended: true,
        title,
        controls,
    })
}

enum Value {
    Empty,
    Ordinal(u16),
    Text(String),
}

impl Value {
    fn ordinal(&self) -> Option<u16> {
        match self {
            Self::Ordinal(value) => Some(*value),
            Self::Empty | Self::Text(_) => None,
        }
    }

    fn text(self) -> Option<String> {
        match self {
            Self::Text(value) => Some(value),
            Self::Empty | Self::Ordinal(_) => None,
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn skip(&mut self, length: usize) -> Result<(), OriginalLanguageDialogTemplateError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(OriginalLanguageDialogTemplateError::AlignmentOverflow)?;
        if end > self.bytes.len() {
            return Err(OriginalLanguageDialogTemplateError::Truncated);
        }
        self.offset = end;
        Ok(())
    }

    fn u16(&mut self) -> Result<u16, OriginalLanguageDialogTemplateError> {
        let bytes = self
            .bytes
            .get(self.offset..self.offset + 2)
            .ok_or(OriginalLanguageDialogTemplateError::Truncated)?;
        self.offset += 2;
        Ok(u16::from_le_bytes(
            bytes.try_into().expect("length checked"),
        ))
    }

    fn u32(&mut self) -> Result<u32, OriginalLanguageDialogTemplateError> {
        let bytes = self
            .bytes
            .get(self.offset..self.offset + 4)
            .ok_or(OriginalLanguageDialogTemplateError::Truncated)?;
        self.offset += 4;
        Ok(u32::from_le_bytes(
            bytes.try_into().expect("length checked"),
        ))
    }

    fn align4(&mut self) -> Result<(), OriginalLanguageDialogTemplateError> {
        let aligned = self
            .offset
            .checked_add(3)
            .ok_or(OriginalLanguageDialogTemplateError::AlignmentOverflow)?
            & !3;
        self.skip(aligned - self.offset)
    }

    fn value(&mut self) -> Result<Value, OriginalLanguageDialogTemplateError> {
        let first = self.u16()?;
        if first == 0 {
            return Ok(Value::Empty);
        }
        if first == 0xffff {
            return Ok(Value::Ordinal(self.u16()?));
        }
        let mut words = vec![first];
        loop {
            let word = self.u16()?;
            if word == 0 {
                break;
            }
            words.push(word);
        }
        String::from_utf16(&words)
            .map(Value::Text)
            .map_err(|_| OriginalLanguageDialogTemplateError::InvalidUtf16)
    }

    fn finish_padding(self) -> Result<(), OriginalLanguageDialogTemplateError> {
        let trailing = &self.bytes[self.offset..];
        if trailing.len() <= 3 && trailing.iter().all(|byte| *byte == 0) {
            Ok(())
        } else {
            Err(OriginalLanguageDialogTemplateError::TrailingData)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn dword(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn text(bytes: &mut Vec<u8>, value: &str) {
        for unit in value.encode_utf16() {
            word(bytes, unit);
        }
        word(bytes, 0);
    }

    fn align4(bytes: &mut Vec<u8>) {
        while bytes.len() & 3 != 0 {
            bytes.push(0);
        }
    }

    fn standard_template() -> Vec<u8> {
        let mut bytes = Vec::new();
        dword(&mut bytes, DS_SETFONT);
        dword(&mut bytes, 0);
        word(&mut bytes, 2);
        bytes.extend_from_slice(&[0; 8]);
        word(&mut bytes, 0);
        word(&mut bytes, 0);
        text(&mut bytes, "Palette 🦀");
        word(&mut bytes, 9);
        text(&mut bytes, "Segoe UI");
        align4(&mut bytes);
        bytes.extend_from_slice(&[0; 16]);
        word(&mut bytes, 100);
        word(&mut bytes, 0xffff);
        word(&mut bytes, 0x0080);
        text(&mut bytes, "&Apply");
        word(&mut bytes, 3);
        bytes.extend_from_slice(&[1, 2, 3]);
        align4(&mut bytes);
        bytes.extend_from_slice(&[0; 16]);
        word(&mut bytes, 101);
        text(&mut bytes, "CustomClass");
        word(&mut bytes, 0xffff);
        word(&mut bytes, 7);
        word(&mut bytes, 0);
        bytes
    }

    fn extended_template() -> Vec<u8> {
        let mut bytes = Vec::new();
        word(&mut bytes, 1);
        word(&mut bytes, 0xffff);
        bytes.extend_from_slice(&[0; 8]);
        dword(&mut bytes, DS_SETFONT);
        word(&mut bytes, 1);
        bytes.extend_from_slice(&[0; 8]);
        word(&mut bytes, 0xffff);
        word(&mut bytes, 9);
        word(&mut bytes, 0);
        text(&mut bytes, "Extended");
        word(&mut bytes, 9);
        word(&mut bytes, 400);
        bytes.extend_from_slice(&[0, 1]);
        text(&mut bytes, "MS Shell Dlg");
        align4(&mut bytes);
        bytes.extend_from_slice(&[0; 20]);
        dword(&mut bytes, 0x1_0001);
        word(&mut bytes, 0xffff);
        word(&mut bytes, 0x0082);
        text(&mut bytes, "Localized label");
        word(&mut bytes, 0);
        bytes
    }

    #[test]
    fn standard_template_decodes_unicode_classes_ordinals_and_creation_data() {
        let template = decode_original_language_dialog_template(&standard_template()).unwrap();
        assert!(!template.extended);
        assert_eq!(template.title.as_deref(), Some("Palette 🦀"));
        assert_eq!(template.controls.len(), 2);
        assert_eq!(template.controls[0].id, 100);
        assert_eq!(template.controls[0].class_ordinal, Some(0x0080));
        assert_eq!(template.controls[0].text.as_deref(), Some("&Apply"));
        assert_eq!(template.controls[1].class_ordinal, None);
        assert_eq!(template.controls[1].text, None);
    }

    #[test]
    fn extended_template_decodes_wide_ids_and_font_header() {
        let template = decode_original_language_dialog_template(&extended_template()).unwrap();
        assert!(template.extended);
        assert_eq!(template.title.as_deref(), Some("Extended"));
        assert_eq!(template.controls.len(), 1);
        assert_eq!(template.controls[0].id, 0x1_0001);
        assert_eq!(template.controls[0].class_ordinal, Some(0x0082));
        assert_eq!(
            template.controls[0].text.as_deref(),
            Some("Localized label")
        );
    }

    #[test]
    fn every_truncation_invalid_utf16_version_and_trailing_data_reject() {
        for template in [standard_template(), extended_template()] {
            for end in 0..template.len() {
                assert!(decode_original_language_dialog_template(&template[..end]).is_err());
            }
        }

        let mut invalid_version = extended_template();
        invalid_version[0..2].copy_from_slice(&2_u16.to_le_bytes());
        assert_eq!(
            decode_original_language_dialog_template(&invalid_version),
            Err(OriginalLanguageDialogTemplateError::InvalidExtendedVersion(
                2
            ))
        );

        let mut invalid_utf16 = standard_template();
        invalid_utf16[22..24].copy_from_slice(&0xd800_u16.to_le_bytes());
        assert_eq!(
            decode_original_language_dialog_template(&invalid_utf16),
            Err(OriginalLanguageDialogTemplateError::InvalidUtf16)
        );

        let mut trailing = standard_template();
        trailing.extend_from_slice(&[0, 0, 0, 1]);
        assert_eq!(
            decode_original_language_dialog_template(&trailing),
            Err(OriginalLanguageDialogTemplateError::TrailingData)
        );
    }
}
