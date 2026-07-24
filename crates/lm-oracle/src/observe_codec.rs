use crate::{Observation, ObservationError, sha256_hex};
use lm_codec::{
    CodecError, decode_lz2, decode_lz3, decode_sized_rle, decode_terminated_rle, encode_lz2,
    encode_lz3, encode_sized_rle, encode_terminated_rle,
};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecObservationKind {
    Lz2,
    Lz3,
    RleTerminated,
    RleSized,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodecObservationError {
    Codec(CodecError),
    Observation(ObservationError),
    CanonicalReopenMismatch,
}

impl fmt::Display for CodecObservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "codec observation failed: {self:?}")
    }
}

impl std::error::Error for CodecObservationError {}

impl From<CodecError> for CodecObservationError {
    fn from(value: CodecError) -> Self {
        Self::Codec(value)
    }
}

impl From<ObservationError> for CodecObservationError {
    fn from(value: ObservationError) -> Self {
        Self::Observation(value)
    }
}

/// Observes a complete terminated compressed stream by decoded semantics.
///
/// Physical compressed bytes are deliberately excluded: Lunar Magic and this implementation may
/// choose different valid command sequences. Canonical re-encoding fields remain stable for any
/// two streams with identical decoded bytes and expose deterministic-encoder regressions.
///
/// # Errors
///
/// Returns [`CodecObservationError`] for malformed/trailing streams, output-limit violations,
/// observation construction, or an internal canonical encode/decode disagreement.
pub fn observe_codec(
    kind: CodecObservationKind,
    compressed: &[u8],
    output_bound: usize,
) -> Result<Observation, CodecObservationError> {
    let decoded = match kind {
        CodecObservationKind::Lz2 => decode_lz2(compressed, output_bound)?,
        CodecObservationKind::Lz3 => decode_lz3(compressed, output_bound)?,
        CodecObservationKind::RleTerminated => decode_terminated_rle(compressed, output_bound)?,
        CodecObservationKind::RleSized => decode_sized_rle(compressed, output_bound)?,
    };
    let canonical = match kind {
        CodecObservationKind::Lz2 => encode_lz2(&decoded),
        CodecObservationKind::Lz3 => encode_lz3(&decoded),
        CodecObservationKind::RleTerminated => encode_terminated_rle(&decoded),
        CodecObservationKind::RleSized => encode_sized_rle(&decoded),
    };
    let reopened = match kind {
        CodecObservationKind::Lz2 => decode_lz2(&canonical, output_bound)?,
        CodecObservationKind::Lz3 => decode_lz3(&canonical, output_bound)?,
        CodecObservationKind::RleTerminated => decode_terminated_rle(&canonical, output_bound)?,
        CodecObservationKind::RleSized => decode_sized_rle(&canonical, output_bound)?,
    };
    if reopened != decoded {
        return Err(CodecObservationError::CanonicalReopenMismatch);
    }

    let mut observation = Observation::new();
    observation.insert(
        "codec/kind",
        match kind {
            CodecObservationKind::Lz2 => "lz2",
            CodecObservationKind::Lz3 => "lz3",
            CodecObservationKind::RleTerminated => "rle-terminated",
            CodecObservationKind::RleSized => "rle-sized",
        },
    )?;
    observation.insert("codec/decoded-bytes", decoded.len().to_string())?;
    observation.insert("codec/decoded-sha256", sha256_hex(&decoded))?;
    observation.insert("codec/canonical-encoded-bytes", canonical.len().to_string())?;
    observation.insert("codec/canonical-encoded-sha256", sha256_hex(&canonical))?;
    Ok(observation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physically_distinct_streams_have_one_semantic_observation() {
        let literal = [0x02, b'A', b'A', b'A', 0xff];
        let fill = [0x22, b'A', 0xff];
        assert_ne!(literal.as_slice(), fill.as_slice());
        assert_eq!(
            observe_codec(CodecObservationKind::Lz2, &literal, 3).unwrap(),
            observe_codec(CodecObservationKind::Lz2, &fill, 3).unwrap()
        );
    }

    #[test]
    fn kind_limits_trailing_data_and_malformed_references_are_observed_strictly() {
        assert_ne!(
            observe_codec(CodecObservationKind::Lz2, &[0x00, 7, 0xff], 1).unwrap(),
            observe_codec(CodecObservationKind::Lz3, &[0x00, 7, 0xff], 1).unwrap()
        );
        assert!(observe_codec(CodecObservationKind::Lz3, &[0x01, 1, 2, 0xff], 1).is_err());
        assert!(observe_codec(CodecObservationKind::Lz2, &[0xff, 0], 8).is_err());
        assert!(observe_codec(CodecObservationKind::Lz3, &[0x80, 0, 0xff], 8).is_err());
    }

    #[test]
    fn terminated_and_sized_rle_keep_distinct_container_contracts() {
        let bytes = vec![0xff; 128];
        let terminated = encode_terminated_rle(&bytes);
        let sized = encode_sized_rle(&bytes);
        let terminated_observation = observe_codec(
            CodecObservationKind::RleTerminated,
            &terminated,
            bytes.len(),
        )
        .unwrap();
        let sized_observation =
            observe_codec(CodecObservationKind::RleSized, &sized, bytes.len()).unwrap();
        assert_eq!(
            terminated_observation.get("codec/decoded-sha256"),
            sized_observation.get("codec/decoded-sha256")
        );
        assert_ne!(terminated_observation, sized_observation);
        assert!(
            observe_codec(
                CodecObservationKind::RleTerminated,
                &terminated,
                bytes.len() - 1
            )
            .is_err()
        );
        assert!(observe_codec(CodecObservationKind::RleSized, &sized, bytes.len() - 1).is_err());
        let mut trailing = sized;
        trailing.push(0);
        assert!(observe_codec(CodecObservationKind::RleSized, &trailing, bytes.len()).is_err());
    }
}
