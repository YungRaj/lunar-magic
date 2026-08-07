use crate::{ExAnimationError, ExAnimationRecord};
use std::fmt;

/// One frame's normal source word and, when present, its triggered-bank source word.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExAnimationFrame {
    pub source_words: Vec<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationFrameEdit {
    Insert {
        index: usize,
        frame: ExAnimationFrame,
    },
    Replace {
        index: usize,
        frame: ExAnimationFrame,
    },
    Remove {
        index: usize,
    },
    MoveBefore {
        from: usize,
        before: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationFrameEditError {
    NoFramePayload { kind: u8 },
    WrongWordCount { expected: usize, actual: usize },
    FrameIndexOutOfRange { index: usize, len: usize },
    TooManyFrames { actual: usize, maximum: usize },
    Record(ExAnimationError),
}

impl fmt::Display for ExAnimationFrameEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ExAnimation frame edit failed: {self:?}")
    }
}

impl std::error::Error for ExAnimationFrameEditError {}

/// Decodes the frame payload into explicit little-endian source words.
///
/// # Errors
///
/// Returns [`ExAnimationFrameEditError::NoFramePayload`] for inactive and special transfer kinds
/// whose compact record intentionally carries no ordinary frame array.
pub fn exanimation_frames(
    record: &ExAnimationRecord,
    double_size: bool,
) -> Result<Vec<ExAnimationFrame>, ExAnimationFrameEditError> {
    let words_per_frame = words_per_frame(record, double_size)?;
    let frame_count = usize::from(record.frame_count_minus_one()) + 1;
    let maximum = maximum_frames(words_per_frame);
    if frame_count > maximum {
        return Err(ExAnimationFrameEditError::TooManyFrames {
            actual: frame_count,
            maximum,
        });
    }
    let words = record
        .frame_bytes(double_size)
        .chunks_exact(2)
        .map(|word| u16::from_le_bytes([word[0], word[1]]))
        .collect::<Vec<_>>();
    Ok((0..frame_count)
        .map(|frame| ExAnimationFrame {
            source_words: (0..words_per_frame)
                .map(|bank| words[bank * frame_count + frame])
                .collect(),
        })
        .collect())
}

/// Applies an ordered frame batch to a staged copy of one lossless record.
///
/// Unknown metadata and bytes outside the active payload remain exact. All edits and final compact
/// capacity validation succeed before a changed record is returned.
///
/// # Errors
///
/// Returns [`ExAnimationFrameEditError`] for unsupported payload kinds, invalid indexes or frame
/// width, or exceeding the 8-bit/count and 0x200-byte compact payload limits.
pub fn edit_exanimation_frames(
    record: &ExAnimationRecord,
    double_size: bool,
    edits: &[ExAnimationFrameEdit],
) -> Result<ExAnimationRecord, ExAnimationFrameEditError> {
    let words_per_frame = words_per_frame(record, double_size)?;
    let mut frames = exanimation_frames(record, double_size)?;
    for edit in edits {
        match edit {
            ExAnimationFrameEdit::Insert { index, frame } => {
                validate_frame(frame, words_per_frame)?;
                if *index > frames.len() {
                    return Err(index_error(*index, frames.len()));
                }
                frames.insert(*index, frame.clone());
            }
            ExAnimationFrameEdit::Replace { index, frame } => {
                validate_frame(frame, words_per_frame)?;
                let len = frames.len();
                let target = frames
                    .get_mut(*index)
                    .ok_or_else(|| index_error(*index, len))?;
                *target = frame.clone();
            }
            ExAnimationFrameEdit::Remove { index } => {
                if *index >= frames.len() {
                    return Err(index_error(*index, frames.len()));
                }
                frames.remove(*index);
            }
            ExAnimationFrameEdit::MoveBefore { from, before } => {
                move_before(&mut frames, *from, *before)?;
            }
        }
    }
    if frames.is_empty() {
        return Err(ExAnimationFrameEditError::TooManyFrames {
            actual: 0,
            maximum: maximum_frames(words_per_frame),
        });
    }
    let maximum = maximum_frames(words_per_frame);
    if frames.len() > maximum {
        return Err(ExAnimationFrameEditError::TooManyFrames {
            actual: frames.len(),
            maximum,
        });
    }
    let payload = (0..words_per_frame)
        .flat_map(|bank| {
            frames
                .iter()
                .flat_map(move |frame| frame.source_words[bank].to_le_bytes())
        })
        .collect::<Vec<_>>();
    record
        .with_frame_payload(
            u8::try_from(frames.len() - 1).map_err(|_| {
                ExAnimationFrameEditError::TooManyFrames {
                    actual: frames.len(),
                    maximum,
                }
            })?,
            &payload,
            double_size,
        )
        .map_err(ExAnimationFrameEditError::Record)
}

fn words_per_frame(
    record: &ExAnimationRecord,
    double_size: bool,
) -> Result<usize, ExAnimationFrameEditError> {
    if record.kind() == 0 || (0x18..=0x1b).contains(&record.kind()) {
        return Err(ExAnimationFrameEditError::NoFramePayload {
            kind: record.kind(),
        });
    }
    Ok(if double_size { 2 } else { 1 })
}

fn maximum_frames(words_per_frame: usize) -> usize {
    0x200 / (words_per_frame * 2)
}

fn validate_frame(
    frame: &ExAnimationFrame,
    words_per_frame: usize,
) -> Result<(), ExAnimationFrameEditError> {
    if frame.source_words.len() != words_per_frame {
        return Err(ExAnimationFrameEditError::WrongWordCount {
            expected: words_per_frame,
            actual: frame.source_words.len(),
        });
    }
    Ok(())
}

fn index_error(index: usize, len: usize) -> ExAnimationFrameEditError {
    ExAnimationFrameEditError::FrameIndexOutOfRange { index, len }
}

fn move_before(
    frames: &mut Vec<ExAnimationFrame>,
    from: usize,
    before: usize,
) -> Result<(), ExAnimationFrameEditError> {
    let len = frames.len();
    if from >= len {
        return Err(index_error(from, len));
    }
    if before > len {
        return Err(index_error(before, len));
    }
    let frame = frames.remove(from);
    let destination = if before > from { before - 1 } else { before };
    frames.insert(destination, frame);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(double_size: bool) -> ExAnimationRecord {
        let bytes = &[1, 0, 2, 0, 3, 0, 4, 0];
        ExAnimationRecord::new(
            1,
            if double_size { 1 } else { 3 },
            7,
            0x1234,
            true,
            bytes,
            double_size,
        )
        .unwrap()
    }

    #[test]
    fn single_word_frames_support_ordered_atomic_edits() {
        let mut raw = record(false).encoded().to_vec();
        raw[3] = 0xaa;
        raw[7] = 0xbb;
        raw[520] = 0xcc;
        let original = ExAnimationRecord::decode(&raw).unwrap();
        let edited = edit_exanimation_frames(
            &original,
            false,
            &[
                ExAnimationFrameEdit::Replace {
                    index: 1,
                    frame: ExAnimationFrame {
                        source_words: vec![9],
                    },
                },
                ExAnimationFrameEdit::MoveBefore { from: 3, before: 0 },
                ExAnimationFrameEdit::Remove { index: 2 },
            ],
        )
        .unwrap();
        assert_eq!(
            exanimation_frames(&edited, false)
                .unwrap()
                .into_iter()
                .map(|frame| frame.source_words[0])
                .collect::<Vec<_>>(),
            [4, 1, 3]
        );
        assert_eq!(edited.destination(), original.destination());
        assert!(edited.destination_flag());
        assert_eq!(edited.encoded()[3], 0xaa);
        assert_eq!(edited.encoded()[7], 0xbb);
        assert_eq!(&edited.encoded()[14..16], &[0, 0]);
        assert_eq!(edited.encoded()[520], 0xcc);
    }

    #[test]
    fn double_word_width_and_late_index_fail_without_touching_input() {
        let original = record(true);
        assert_eq!(
            exanimation_frames(&original, true).unwrap(),
            vec![
                ExAnimationFrame {
                    source_words: vec![1, 3],
                },
                ExAnimationFrame {
                    source_words: vec![2, 4],
                },
            ]
        );
        assert!(matches!(
            edit_exanimation_frames(
                &original,
                true,
                &[
                    ExAnimationFrameEdit::Replace {
                        index: 0,
                        frame: ExAnimationFrame {
                            source_words: vec![8, 9]
                        }
                    },
                    ExAnimationFrameEdit::Remove { index: 9 }
                ]
            ),
            Err(ExAnimationFrameEditError::FrameIndexOutOfRange { index: 9, .. })
        ));

        let edited = edit_exanimation_frames(
            &original,
            true,
            &[ExAnimationFrameEdit::Replace {
                index: 0,
                frame: ExAnimationFrame {
                    source_words: vec![8, 9],
                },
            }],
        )
        .unwrap();
        assert_eq!(&edited.encoded()[8..16], &[8, 0, 2, 0, 9, 0, 4, 0]);
        assert_eq!(original, record(true));
        assert!(matches!(
            edit_exanimation_frames(
                &original,
                true,
                &[ExAnimationFrameEdit::Replace {
                    index: 0,
                    frame: ExAnimationFrame {
                        source_words: vec![1]
                    }
                }]
            ),
            Err(ExAnimationFrameEditError::WrongWordCount { .. })
        ));
    }

    #[test]
    fn empty_and_special_payloads_are_rejected() {
        let original = record(false);
        assert!(matches!(
            edit_exanimation_frames(
                &original,
                false,
                &[
                    ExAnimationFrameEdit::Remove { index: 3 },
                    ExAnimationFrameEdit::Remove { index: 2 },
                    ExAnimationFrameEdit::Remove { index: 1 },
                    ExAnimationFrameEdit::Remove { index: 0 }
                ]
            ),
            Err(ExAnimationFrameEditError::TooManyFrames { actual: 0, .. })
        ));
        let special = ExAnimationRecord::new(0x18, 0, 0, 0, false, &[], false).unwrap();
        assert_eq!(
            exanimation_frames(&special, false),
            Err(ExAnimationFrameEditError::NoFramePayload { kind: 0x18 })
        );
    }

    #[test]
    fn compact_payload_capacity_is_enforced_after_staging() {
        let original = ExAnimationRecord::new(1, 255, 0, 0, false, &[0; 0x200], false).unwrap();
        assert!(matches!(
            edit_exanimation_frames(
                &original,
                false,
                &[ExAnimationFrameEdit::Insert {
                    index: 256,
                    frame: ExAnimationFrame {
                        source_words: vec![7]
                    }
                }]
            ),
            Err(ExAnimationFrameEditError::TooManyFrames {
                actual: 257,
                maximum: 256
            })
        ));
        assert_eq!(original.frame_count_minus_one(), 255);
    }
}
