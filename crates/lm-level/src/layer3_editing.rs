//! Failure-atomic editing for optional per-level Layer 3 state.

use crate::{Layer3Data, Layer3Error, Level};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layer3Edit {
    Enable(Layer3Data),
    Disable,
    SetStartPosition(u8),
    SetTilemapSize(u8),
    SetLiquidType(u8),
    SetFlags(u8),
    SetGraphicsFile { slot: usize, file: u16 },
    SetReserved([u8; 16]),
    ReplaceTilemap(Vec<u8>),
    ReplaceTilemapRange { offset: usize, bytes: Vec<u8> },
    ReplaceRemapCommands(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layer3EditError {
    Missing {
        command: usize,
    },
    AlreadyEnabled {
        command: usize,
    },
    GraphicsSlot {
        command: usize,
        slot: usize,
    },
    TilemapRange {
        command: usize,
        offset: usize,
        len: usize,
    },
    Invalid {
        command: usize,
        error: Layer3Error,
    },
}

impl std::fmt::Display for Layer3EditError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Layer 3 edit error: {self:?}")
    }
}

impl std::error::Error for Layer3EditError {}

impl Level {
    /// Applies an ordered Layer 3 batch to a clone and commits only after final validation.
    ///
    /// # Errors
    ///
    /// Returns [`Layer3EditError`] with the failing command index for invalid state transitions,
    /// indexes, ranges, or recovered native limits. Failure leaves the level unchanged.
    pub fn apply_layer3_edits(&mut self, edits: &[Layer3Edit]) -> Result<(), Layer3EditError> {
        let mut staged = self.layer3.clone();
        for (command, edit) in edits.iter().enumerate() {
            match edit {
                Layer3Edit::Enable(value) => {
                    if staged.is_some() {
                        return Err(Layer3EditError::AlreadyEnabled { command });
                    }
                    value
                        .validate()
                        .map_err(|error| Layer3EditError::Invalid { command, error })?;
                    staged = Some(value.clone());
                }
                Layer3Edit::Disable => {
                    if staged.take().is_none() {
                        return Err(Layer3EditError::Missing { command });
                    }
                }
                Layer3Edit::SetStartPosition(value) => {
                    require(&mut staged, command)?.settings.start_position = *value;
                }
                Layer3Edit::SetTilemapSize(value) => {
                    require(&mut staged, command)?.settings.tilemap_size = *value;
                }
                Layer3Edit::SetLiquidType(value) => {
                    require(&mut staged, command)?.settings.liquid_type = *value;
                }
                Layer3Edit::SetFlags(value) => {
                    require(&mut staged, command)?.settings.flags = *value;
                }
                Layer3Edit::SetGraphicsFile { slot, file } => {
                    let value = require(&mut staged, command)?;
                    let target = value.settings.graphics_files.get_mut(*slot).ok_or(
                        Layer3EditError::GraphicsSlot {
                            command,
                            slot: *slot,
                        },
                    )?;
                    *target = *file;
                }
                Layer3Edit::SetReserved(bytes) => {
                    require(&mut staged, command)?.settings.reserved = *bytes;
                }
                Layer3Edit::ReplaceTilemap(bytes) => {
                    require(&mut staged, command)?.tilemap.clone_from(bytes);
                }
                Layer3Edit::ReplaceTilemapRange { offset, bytes } => {
                    let value = require(&mut staged, command)?;
                    let end =
                        offset
                            .checked_add(bytes.len())
                            .ok_or(Layer3EditError::TilemapRange {
                                command,
                                offset: *offset,
                                len: bytes.len(),
                            })?;
                    let target = value.tilemap.get_mut(*offset..end).ok_or(
                        Layer3EditError::TilemapRange {
                            command,
                            offset: *offset,
                            len: bytes.len(),
                        },
                    )?;
                    target.copy_from_slice(bytes);
                }
                Layer3Edit::ReplaceRemapCommands(bytes) => {
                    require(&mut staged, command)?
                        .remap_commands
                        .clone_from(bytes);
                }
            }
        }
        if let Some(value) = &staged {
            value.validate().map_err(|error| Layer3EditError::Invalid {
                command: edits.len().saturating_sub(1),
                error,
            })?;
        }
        self.layer3 = staged;
        Ok(())
    }
}

fn require(
    value: &mut Option<Layer3Data>,
    command: usize,
) -> Result<&mut Layer3Data, Layer3EditError> {
    value.as_mut().ok_or(Layer3EditError::Missing { command })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Layer3Settings;

    fn layer3() -> Layer3Data {
        Layer3Data {
            settings: Layer3Settings::default(),
            tilemap: vec![0; 16],
            remap_commands: vec![],
        }
    }

    #[test]
    fn mixed_batch_changes_settings_buffers_and_round_trips() {
        let mut level = Level::default();
        level
            .apply_layer3_edits(&[
                Layer3Edit::Enable(layer3()),
                Layer3Edit::SetGraphicsFile {
                    slot: 2,
                    file: 0xabc,
                },
                Layer3Edit::SetFlags(0x80),
                Layer3Edit::ReplaceTilemapRange {
                    offset: 4,
                    bytes: vec![1, 2, 3],
                },
                Layer3Edit::ReplaceRemapCommands(vec![0xfe, 7]),
            ])
            .unwrap();
        let value = level.layer3.as_ref().unwrap();
        assert_eq!(value.settings.graphics_files[2], 0xabc);
        assert_eq!(value.settings.flags, 0x80);
        assert_eq!(&value.tilemap[4..7], [1, 2, 3]);
        assert_eq!(value.remap_commands, [0xfe, 7]);
        let encoded = crate::CompleteLevelFile(level.clone()).encode().unwrap();
        assert_eq!(crate::CompleteLevelFile::decode(&encoded).unwrap().0, level);
    }

    #[test]
    fn late_range_and_limit_failures_are_atomic() {
        let mut level = Level {
            layer3: Some(layer3()),
            ..Level::default()
        };
        let original = level.clone();
        assert!(matches!(
            level.apply_layer3_edits(&[
                Layer3Edit::SetFlags(1),
                Layer3Edit::ReplaceTilemapRange {
                    offset: 15,
                    bytes: vec![1, 2],
                },
            ]),
            Err(Layer3EditError::TilemapRange { command: 1, .. })
        ));
        assert_eq!(level, original);

        assert!(matches!(
            level.apply_layer3_edits(&[Layer3Edit::SetGraphicsFile {
                slot: 0,
                file: 0x1000,
            }]),
            Err(Layer3EditError::Invalid { command: 0, .. })
        ));
        assert_eq!(level, original);
    }
}
