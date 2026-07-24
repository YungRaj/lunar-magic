use lm_oracle::Observation;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReleaseEvidenceError {
    Missing {
        operation: String,
        path: &'static str,
    },
    WrongValue {
        operation: String,
        path: &'static str,
        expected: &'static str,
        actual: String,
    },
    InvalidSha256 {
        path: &'static str,
    },
    InvalidPositiveInteger {
        path: &'static str,
    },
    EmptyValue {
        path: &'static str,
    },
    UnknownOperation(String),
}

impl std::fmt::Display for ReleaseEvidenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid release evidence: {self:?}")
    }
}

impl std::error::Error for ReleaseEvidenceError {}

pub fn validate(operation: &str, after: &Observation) -> Result<(), ReleaseEvidenceError> {
    match operation {
        "open-save" => require_true(
            operation,
            after,
            &[
                "release/open-save/reopened",
                "release/open-save/checksum-valid",
                "release/open-save/unchanged-regions",
            ],
        ),
        "render-level" => {
            let digest = required(operation, after, "release/render-level/png-sha256")?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(ReleaseEvidenceError::InvalidSha256 {
                    path: "release/render-level/png-sha256",
                });
            }
            for path in ["release/render-level/width", "release/render-level/height"] {
                if !matches!(required(operation, after, path)?.parse::<usize>(), Ok(1..)) {
                    return Err(ReleaseEvidenceError::InvalidPositiveInteger { path });
                }
            }
            Ok(())
        }
        "level-edit" => require_true(
            operation,
            after,
            &[
                "release/level-edit/semantic-change",
                "release/level-edit/reopened",
                "release/level-edit/unchanged-regions",
            ],
        ),
        "lunar-magic-reopen" => require_true(
            operation,
            after,
            &[
                "release/lunar-magic-reopen/reopened",
                "release/lunar-magic-reopen/semantic-equal",
            ],
        ),
        "emulator-boot" => {
            require_true(operation, after, &["release/emulator-boot/booted"])?;
            if required(operation, after, "release/emulator-boot/emulator")?.is_empty() {
                return Err(ReleaseEvidenceError::EmptyValue {
                    path: "release/emulator-boot/emulator",
                });
            }
            for path in [
                "release/emulator-boot/rom-sha256",
                "release/emulator-boot/screenshot-sha256",
            ] {
                require_sha256(operation, after, path)?;
            }
            for path in [
                "release/emulator-boot/frames",
                "release/emulator-boot/screenshot-width",
                "release/emulator-boot/screenshot-height",
            ] {
                require_positive_integer(operation, after, path)?;
            }
            Ok(())
        }
        _ => Err(ReleaseEvidenceError::UnknownOperation(operation.into())),
    }
}

fn require_sha256(
    operation: &str,
    observation: &Observation,
    path: &'static str,
) -> Result<(), ReleaseEvidenceError> {
    let digest = required(operation, observation, path)?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ReleaseEvidenceError::InvalidSha256 { path });
    }
    Ok(())
}

fn require_positive_integer(
    operation: &str,
    observation: &Observation,
    path: &'static str,
) -> Result<(), ReleaseEvidenceError> {
    if !matches!(
        required(operation, observation, path)?.parse::<usize>(),
        Ok(1..)
    ) {
        return Err(ReleaseEvidenceError::InvalidPositiveInteger { path });
    }
    Ok(())
}

fn require_true(
    operation: &str,
    observation: &Observation,
    paths: &[&'static str],
) -> Result<(), ReleaseEvidenceError> {
    for path in paths {
        let actual = required(operation, observation, path)?;
        if actual != "true" {
            return Err(ReleaseEvidenceError::WrongValue {
                operation: operation.into(),
                path,
                expected: "true",
                actual: actual.into(),
            });
        }
    }
    Ok(())
}

fn required<'a>(
    operation: &str,
    observation: &'a Observation,
    path: &'static str,
) -> Result<&'a str, ReleaseEvidenceError> {
    observation
        .get(path)
        .ok_or_else(|| ReleaseEvidenceError::Missing {
            operation: operation.into(),
            path,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(observation: &mut Observation, path: &str, value: &str) {
        observation.insert(path, value).unwrap();
    }

    #[test]
    fn empty_or_false_claims_do_not_qualify() {
        assert!(validate("emulator-boot", &Observation::new()).is_err());
        let mut evidence = Observation::new();
        insert(&mut evidence, "release/emulator-boot/booted", "false");
        insert(&mut evidence, "release/emulator-boot/emulator", "Mesen");
        assert!(matches!(
            validate("emulator-boot", &evidence),
            Err(ReleaseEvidenceError::WrongValue { .. })
        ));
    }

    #[test]
    fn render_evidence_requires_a_digest_and_positive_dimensions() {
        let mut evidence = Observation::new();
        insert(
            &mut evidence,
            "release/render-level/png-sha256",
            &"a".repeat(64),
        );
        insert(&mut evidence, "release/render-level/width", "256");
        insert(&mut evidence, "release/render-level/height", "0");
        assert!(matches!(
            validate("render-level", &evidence),
            Err(ReleaseEvidenceError::InvalidPositiveInteger { .. })
        ));
        evidence = Observation::new();
        insert(
            &mut evidence,
            "release/render-level/png-sha256",
            &"a".repeat(64),
        );
        insert(&mut evidence, "release/render-level/width", "256");
        insert(&mut evidence, "release/render-level/height", "224");
        validate("render-level", &evidence).unwrap();

        let mut uppercase = Observation::new();
        insert(
            &mut uppercase,
            "release/render-level/png-sha256",
            &"A".repeat(64),
        );
        insert(&mut uppercase, "release/render-level/width", "256");
        insert(&mut uppercase, "release/render-level/height", "224");
        assert!(matches!(
            validate("render-level", &uppercase),
            Err(ReleaseEvidenceError::InvalidSha256 { .. })
        ));
    }
}
