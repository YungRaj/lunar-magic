//! Strict object-backed Layer 2 scripts that reuse the authoritative level object grammar.

use crate::level_edit_script;
use lm_app::NativeLevelEdit;
use lm_level::ObjectEdit;
use std::fmt;

pub const MAX_SCRIPT_LEN: usize = level_edit_script::MAX_SCRIPT_LEN;
const MAGIC: &str = "LML2OBJ1";

#[derive(Debug)]
pub enum Layer2ObjectEditScriptError {
    TooLarge,
    MissingMagic,
    UnsupportedVersion(String),
    NonObjectCommand { command: usize },
    Level(level_edit_script::LevelEditScriptError),
}

impl fmt::Display for Layer2ObjectEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid Layer 2 object-edit script: ")?;
        match self {
            Self::TooLarge => formatter.write_str("file exceeds the size limit"),
            Self::MissingMagic => formatter.write_str("missing format header"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported format header {version:?}")
            }
            Self::NonObjectCommand { command } => {
                write!(formatter, "command {} does not edit objects", command + 1)
            }
            Self::Level(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for Layer2ObjectEditScriptError {}

/// Parses only `object ...` commands through the exact `LMLEDIT1` object grammar.
pub fn parse(input: &str) -> Result<Vec<ObjectEdit>, Layer2ObjectEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(Layer2ObjectEditScriptError::TooLarge);
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(Layer2ObjectEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(Layer2ObjectEditScriptError::UnsupportedVersion(
            magic.into(),
        ));
    }
    let mut translated = String::with_capacity(input.len().saturating_add(1));
    translated.push_str("LMLEDIT1\n");
    for line in lines {
        translated.push_str(line);
        translated.push('\n');
    }
    let edits =
        level_edit_script::parse(&translated).map_err(Layer2ObjectEditScriptError::Level)?;
    let mut objects = Vec::new();
    for (command, edit) in edits.into_iter().enumerate() {
        let NativeLevelEdit::Objects(mut edits) = edit else {
            return Err(Layer2ObjectEditScriptError::NonObjectCommand { command });
        };
        objects.append(&mut edits);
    }
    Ok(objects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{NativeObjectRecordFields, ObjectCoordinateNibbles, ObjectRecord};

    #[test]
    fn reuses_every_positioned_object_shape_and_named_field_bound() {
        let edits = parse(
            "LML2OBJ1\n\
             object insert 0 090855\n\
             object place 090855 1f 0c 0b true\n\
             object relocate-position 0 1e 0a 09 false\n\
             object fields 0 22 55 1d 0c 0b true\n",
        )
        .unwrap();
        assert_eq!(edits.len(), 4);
        assert_eq!(
            edits[0],
            ObjectEdit::Insert {
                index: 0,
                record: ObjectRecord::new(vec![0x09, 0x08, 0x55]).unwrap(),
            }
        );
        assert_eq!(
            edits[3],
            ObjectEdit::SetOrdinaryFields {
                index: 0,
                fields: NativeObjectRecordFields {
                    command_id: 0x22,
                    parameter: 0x55,
                    screen: 0x1d,
                    coordinates: ObjectCoordinateNibbles {
                        first: 0x0c,
                        second: 0x0b,
                    },
                    perpendicular_high: true,
                },
            }
        );
        assert!(parse("LML2OBJ1\nobject fields 0 40 00 00 00 00 false\n").is_err());
    }

    #[test]
    fn rejects_other_level_domains_and_bad_framing() {
        for script in [
            "LMLEDIT1\nobject remove 0\n",
            "LML2OBJ1\nheader mode 03\n",
            "LML2OBJ1\nsprite remove 0\n",
        ] {
            assert!(parse(script).is_err(), "accepted {script:?}");
        }
    }
}
