use lm_graphics::{ExAnimationFrame, ExAnimationRecord};

#[derive(Clone, Debug, Default)]
pub(crate) struct GlobalForm {
    pub(crate) setting: String,
    pub(crate) header: String,
}

impl GlobalForm {
    pub(crate) fn load(setting: u8, header: u32) -> Self {
        Self {
            setting: format!("{setting:02X}"),
            header: format!("{header:08X}"),
        }
    }

    pub(crate) fn parse(&self) -> Result<(u8, u32), String> {
        Ok((
            hex_u8(&self.setting, "setting")?,
            hex_u32(&self.header, "header")?,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RecordForm {
    pub(crate) kind: String,
    pub(crate) size_mode: String,
    pub(crate) destination: String,
    pub(crate) destination_flag: bool,
    pub(crate) frames: String,
}

impl Default for RecordForm {
    fn default() -> Self {
        Self {
            kind: "01".into(),
            size_mode: "00".into(),
            destination: "0000".into(),
            destination_flag: false,
            frames: "0000".into(),
        }
    }
}

impl RecordForm {
    pub(crate) fn load(record: &ExAnimationRecord, frames: &[ExAnimationFrame]) -> Self {
        Self {
            kind: format!("{:02X}", record.kind()),
            size_mode: format!("{:02X}", record.size_mode()),
            destination: format!("{:04X}", record.destination()),
            destination_flag: record.destination_flag(),
            frames: format_frames(frames),
        }
    }

    pub(crate) fn parse(&self, modes: &[bool; 256]) -> Result<ExAnimationRecord, String> {
        let kind = hex_u8(&self.kind, "kind")?;
        if kind == 0 {
            return Err("kind 00 is reserved for inactive records".into());
        }
        let size_mode = hex_u8(&self.size_mode, "size mode")?;
        let destination = hex_u16(&self.destination, "destination")?;
        let words_per_frame = if modes[usize::from(size_mode)] { 2 } else { 1 };
        let frames = parse_frames(&self.frames, words_per_frame)?;
        let count =
            u8::try_from(frames.len() - 1).map_err(|_| "frame count exceeds 256".to_string())?;
        let bytes = frames
            .into_iter()
            .flatten()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        ExAnimationRecord::new(
            kind,
            count,
            size_mode,
            destination,
            self.destination_flag,
            &bytes,
            words_per_frame == 2,
        )
        .map_err(|error| error.to_string())
    }
}

fn parse_frames(text: &str, words_per_frame: usize) -> Result<Vec<Vec<u16>>, String> {
    let frames = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split_whitespace()
                .map(|word| hex_u16(word, "frame word"))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    if frames.is_empty() {
        return Err("at least one frame is required".into());
    }
    if let Some(frame) = frames.iter().find(|frame| frame.len() != words_per_frame) {
        return Err(format!(
            "each frame requires {words_per_frame} source word(s), found {}",
            frame.len()
        ));
    }
    Ok(frames)
}

fn format_frames(frames: &[ExAnimationFrame]) -> String {
    frames
        .iter()
        .map(|frame| {
            frame
                .source_words
                .iter()
                .map(|word| format!("{word:04X}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn hex_u8(text: &str, name: &str) -> Result<u8, String> {
    u8::from_str_radix(text.trim(), 16).map_err(|error| format!("invalid {name}: {error}"))
}

pub(crate) fn hex_u16(text: &str, name: &str) -> Result<u16, String> {
    u16::from_str_radix(text.trim(), 16).map_err(|error| format!("invalid {name}: {error}"))
}

fn hex_u32(text: &str, name: &str) -> Result<u32, String> {
    u32::from_str_radix(text.trim(), 16).map_err(|error| format!("invalid {name}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_width_record_round_trips_form_fields() {
        let mut modes = [false; 256];
        modes[3] = true;
        let form = RecordForm {
            kind: "01".into(),
            size_mode: "03".into(),
            destination: "1234".into(),
            destination_flag: true,
            frames: "1111 2222\n3333 4444".into(),
        };
        let record = form.parse(&modes).unwrap();
        assert_eq!(record.frame_count_minus_one(), 1);
        assert_eq!(
            record.frame_bytes(true),
            &[0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44]
        );
        assert_eq!(record.destination(), 0x1234);
        assert!(record.destination_flag());
    }

    #[test]
    fn form_rejects_wrong_width_and_inactive_kind() {
        let modes = [false; 256];
        assert!(
            RecordForm {
                frames: "1 2".into(),
                ..RecordForm::default()
            }
            .parse(&modes)
            .is_err()
        );
        assert!(
            RecordForm {
                kind: "00".into(),
                ..RecordForm::default()
            }
            .parse(&modes)
            .is_err()
        );
    }
}
