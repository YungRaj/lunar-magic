use lm_oracle::{Observation, sha256_hex};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubsystemEvidenceError {
    MissingSemanticObservation(String),
    MissingDigest {
        subsystem: String,
        path: String,
    },
    InvalidDigest {
        subsystem: String,
        actual: String,
    },
    DigestMismatch {
        subsystem: String,
        expected: String,
        actual: String,
    },
}

impl std::fmt::Display for SubsystemEvidenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid release subsystem evidence: {self:?}")
    }
}

impl std::error::Error for SubsystemEvidenceError {}

/// Hashes the canonical non-release observation entries that carry decoded semantic evidence.
#[must_use]
pub fn semantic_observation_digest(observation: &Observation) -> Option<String> {
    let semantic = observation
        .entries()
        .filter(|(path, _)| !path.starts_with("release/"))
        .fold(String::new(), |mut output, (path, value)| {
            output.push_str(path);
            output.push('\0');
            output.push_str(value);
            output.push('\n');
            output
        });
    (!semantic.is_empty()).then(|| sha256_hex(semantic.as_bytes()))
}

/// Requires a subsystem-specific digest bound to the case's decoded semantic observation.
///
/// # Errors
///
/// Returns [`SubsystemEvidenceError`] when semantic entries or the declared digest are missing,
/// malformed, or inconsistent.
pub fn validate(subsystem: &str, observation: &Observation) -> Result<(), SubsystemEvidenceError> {
    let actual = semantic_observation_digest(observation)
        .ok_or_else(|| SubsystemEvidenceError::MissingSemanticObservation(subsystem.into()))?;
    let path = format!("release/subsystem/{subsystem}/observation-sha256");
    let expected = observation
        .get(&path)
        .ok_or_else(|| SubsystemEvidenceError::MissingDigest {
            subsystem: subsystem.into(),
            path,
        })?;
    if expected.len() != 64
        || !expected
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(SubsystemEvidenceError::InvalidDigest {
            subsystem: subsystem.into(),
            actual: expected.into(),
        });
    }
    if expected != actual {
        return Err(SubsystemEvidenceError::DigestMismatch {
            subsystem: subsystem.into(),
            expected: expected.into(),
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_is_bound_to_non_release_semantics() {
        let mut observation = Observation::new();
        observation.insert("model/levels/count", "512").unwrap();
        let digest = semantic_observation_digest(&observation).unwrap();
        observation
            .insert("release/subsystem/levels/observation-sha256", &digest)
            .unwrap();
        validate("levels", &observation).unwrap();
        observation.insert("model/levels/count", "511").unwrap_err();

        let mut changed = Observation::new();
        changed.insert("model/levels/count", "511").unwrap();
        changed
            .insert("release/subsystem/levels/observation-sha256", digest)
            .unwrap();
        assert!(matches!(
            validate("levels", &changed),
            Err(SubsystemEvidenceError::DigestMismatch { .. })
        ));

        let empty = Observation::new();
        assert!(matches!(
            validate("levels", &empty),
            Err(SubsystemEvidenceError::MissingSemanticObservation(value)) if value == "levels"
        ));
        let mut missing = Observation::new();
        missing.insert("model/levels/count", "512").unwrap();
        assert!(matches!(
            validate("levels", &missing),
            Err(SubsystemEvidenceError::MissingDigest { .. })
        ));
    }
}
