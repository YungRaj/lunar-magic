use crate::{
    EditorOverlay, EditorOverlayError, GridOverlay, Rgba, SelectionOverlay, WorldRect,
    validate_editor_overlays,
};

const MAGIC: &[u8; 8] = b"LMOVLY01";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EditorOverlayFile {
    pub overlays: Vec<EditorOverlay>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorOverlayFileError {
    WrongMagic,
    Truncated,
    TrailingBytes(usize),
    TooManyOverlays(usize),
    UnknownRecord(u8),
    Overlay(EditorOverlayError),
}

impl std::fmt::Display for EditorOverlayFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid editor-overlay file: {self:?}")
    }
}

impl std::error::Error for EditorOverlayFileError {}

impl From<EditorOverlayError> for EditorOverlayFileError {
    fn from(error: EditorOverlayError) -> Self {
        Self::Overlay(error)
    }
}

impl EditorOverlayFile {
    pub const MAX_OVERLAYS: usize = 256;
    pub const MAX_FILE_LEN: usize = 8 + 2 + Self::MAX_OVERLAYS * 49;

    /// Encodes one canonical bounded overlay artifact.
    ///
    /// # Errors
    ///
    /// Rejects excessive or invalid overlay batches.
    pub fn encode(&self) -> Result<Vec<u8>, EditorOverlayFileError> {
        self.validate()?;
        let mut output = Vec::with_capacity(10 + self.overlays.len() * 49);
        output.extend_from_slice(MAGIC);
        let count = u16::try_from(self.overlays.len())
            .map_err(|_| EditorOverlayFileError::TooManyOverlays(self.overlays.len()))?;
        output.extend_from_slice(&count.to_le_bytes());
        for overlay in &self.overlays {
            match overlay {
                EditorOverlay::Grid(grid) => {
                    output.push(0);
                    output.extend_from_slice(&grid.origin_x.to_le_bytes());
                    output.extend_from_slice(&grid.origin_y.to_le_bytes());
                    output.extend_from_slice(&grid.cell_width.to_le_bytes());
                    output.extend_from_slice(&grid.cell_height.to_le_bytes());
                    rgba_bytes(&mut output, grid.color);
                }
                EditorOverlay::Selection(selection) => {
                    output.push(1);
                    output.extend_from_slice(&selection.bounds.left.to_le_bytes());
                    output.extend_from_slice(&selection.bounds.top.to_le_bytes());
                    output.extend_from_slice(&selection.bounds.right.to_le_bytes());
                    output.extend_from_slice(&selection.bounds.bottom.to_le_bytes());
                    rgba_bytes(&mut output, selection.light);
                    rgba_bytes(&mut output, selection.dark);
                    output.extend_from_slice(&selection.dash_length.to_le_bytes());
                    output.extend_from_slice(&selection.phase.to_le_bytes());
                }
            }
        }
        Ok(output)
    }

    /// Decodes one exact bounded overlay artifact.
    ///
    /// # Errors
    ///
    /// Rejects invalid framing, record kinds, limits, geometry, or trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, EditorOverlayFileError> {
        if bytes.len() > Self::MAX_FILE_LEN {
            return Err(EditorOverlayFileError::TooManyOverlays(
                Self::MAX_OVERLAYS + 1,
            ));
        }
        let mut input = Input::new(bytes);
        if input.take(MAGIC.len())? != MAGIC {
            return Err(EditorOverlayFileError::WrongMagic);
        }
        let count = usize::from(input.u16()?);
        if count > Self::MAX_OVERLAYS {
            return Err(EditorOverlayFileError::TooManyOverlays(count));
        }
        let mut overlays = Vec::with_capacity(count);
        for _ in 0..count {
            overlays.push(match input.byte()? {
                0 => EditorOverlay::Grid(GridOverlay {
                    origin_x: input.i64()?,
                    origin_y: input.i64()?,
                    cell_width: input.u32()?,
                    cell_height: input.u32()?,
                    color: input.rgba()?,
                }),
                1 => EditorOverlay::Selection(SelectionOverlay {
                    bounds: WorldRect {
                        left: input.i64()?,
                        top: input.i64()?,
                        right: input.i64()?,
                        bottom: input.i64()?,
                    },
                    light: input.rgba()?,
                    dark: input.rgba()?,
                    dash_length: input.u32()?,
                    phase: input.u32()?,
                }),
                tag => return Err(EditorOverlayFileError::UnknownRecord(tag)),
            });
        }
        if input.remaining() != 0 {
            return Err(EditorOverlayFileError::TrailingBytes(input.remaining()));
        }
        let file = Self { overlays };
        file.validate()?;
        Ok(file)
    }

    fn validate(&self) -> Result<(), EditorOverlayFileError> {
        if self.overlays.len() > Self::MAX_OVERLAYS {
            return Err(EditorOverlayFileError::TooManyOverlays(self.overlays.len()));
        }
        validate_editor_overlays(&self.overlays)?;
        Ok(())
    }
}

fn rgba_bytes(output: &mut Vec<u8>, value: Rgba) {
    output.extend_from_slice(&[value.red, value.green, value.blue, value.alpha]);
}

struct Input<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Input<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], EditorOverlayFileError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(EditorOverlayFileError::Truncated)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(EditorOverlayFileError::Truncated)?;
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, EditorOverlayFileError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, EditorOverlayFileError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("exact slice"),
        ))
    }

    fn u32(&mut self) -> Result<u32, EditorOverlayFileError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("exact slice"),
        ))
    }

    fn i64(&mut self) -> Result<i64, EditorOverlayFileError> {
        Ok(i64::from_le_bytes(
            self.take(8)?.try_into().expect("exact slice"),
        ))
    }

    fn rgba(&mut self) -> Result<Rgba, EditorOverlayFileError> {
        let bytes = self.take(4)?;
        Ok(Rgba {
            red: bytes[0],
            green: bytes[1],
            blue: bytes[2],
            alpha: bytes[3],
        })
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> EditorOverlayFile {
        EditorOverlayFile {
            overlays: vec![
                EditorOverlay::Grid(GridOverlay {
                    origin_x: -16,
                    origin_y: i64::MAX,
                    cell_width: 16,
                    cell_height: 32,
                    color: Rgba {
                        red: 1,
                        green: 2,
                        blue: 3,
                        alpha: 128,
                    },
                }),
                EditorOverlay::Selection(SelectionOverlay {
                    bounds: WorldRect {
                        left: -4,
                        top: 5,
                        right: 20,
                        bottom: 30,
                    },
                    light: Rgba {
                        red: 255,
                        green: 255,
                        blue: 255,
                        alpha: 255,
                    },
                    dark: Rgba {
                        red: 0,
                        green: 0,
                        blue: 0,
                        alpha: 255,
                    },
                    dash_length: 3,
                    phase: u32::MAX,
                }),
            ],
        }
    }

    #[test]
    fn canonical_round_trip_preserves_every_field() {
        let file = fixture();
        let encoded = file.encode().unwrap();
        assert_eq!(EditorOverlayFile::decode(&encoded).unwrap(), file);
        assert_eq!(
            EditorOverlayFile::decode(&encoded)
                .unwrap()
                .encode()
                .unwrap(),
            encoded
        );
    }

    #[test]
    fn framing_record_and_semantic_failures_are_typed() {
        assert_eq!(
            EditorOverlayFile::decode(b"bad"),
            Err(EditorOverlayFileError::Truncated)
        );
        let mut wrong = fixture().encode().unwrap();
        wrong[0] = b'X';
        assert_eq!(
            EditorOverlayFile::decode(&wrong),
            Err(EditorOverlayFileError::WrongMagic)
        );
        let mut unknown = fixture().encode().unwrap();
        unknown[10] = 9;
        assert_eq!(
            EditorOverlayFile::decode(&unknown),
            Err(EditorOverlayFileError::UnknownRecord(9))
        );
        let mut trailing = fixture().encode().unwrap();
        trailing.push(0);
        assert_eq!(
            EditorOverlayFile::decode(&trailing),
            Err(EditorOverlayFileError::TrailingBytes(1))
        );

        let invalid = EditorOverlayFile {
            overlays: vec![EditorOverlay::Grid(GridOverlay {
                origin_x: 0,
                origin_y: 0,
                cell_width: 0,
                cell_height: 1,
                color: Rgba::default(),
            })],
        };
        assert_eq!(
            invalid.encode(),
            Err(EditorOverlayFileError::Overlay(
                EditorOverlayError::ZeroGridSpacing
            ))
        );
    }

    #[test]
    fn count_and_every_truncation_boundary_are_rejected() {
        let encoded = fixture().encode().unwrap();
        for len in 0..encoded.len() {
            assert!(
                EditorOverlayFile::decode(&encoded[..len]).is_err(),
                "accepted prefix {len}"
            );
        }
        let excessive = EditorOverlayFile {
            overlays: vec![fixture().overlays[0]; EditorOverlayFile::MAX_OVERLAYS + 1],
        };
        assert!(matches!(
            excessive.encode(),
            Err(EditorOverlayFileError::TooManyOverlays(_))
        ));
        let mut declared = MAGIC.to_vec();
        let excessive_count = u16::try_from(EditorOverlayFile::MAX_OVERLAYS).unwrap() + 1;
        declared.extend_from_slice(&excessive_count.to_le_bytes());
        assert!(matches!(
            EditorOverlayFile::decode(&declared),
            Err(EditorOverlayFileError::TooManyOverlays(_))
        ));
    }
}
