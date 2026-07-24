use super::PayloadLoadError;

/// Describes how a pointer-targeted ROM payload is delimited.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PayloadReadPolicy {
    Tagged,
    Fixed {
        len: usize,
    },
    Terminated {
        terminator: Vec<u8>,
        maximum_len: usize,
        bank_size: Option<usize>,
    },
    TaggedOrTerminated {
        terminator: Vec<u8>,
        maximum_len: usize,
        bank_size: Option<usize>,
    },
    TaggedOrFixed {
        len: usize,
    },
    Bounded {
        maximum_len: usize,
        bank_size: Option<usize>,
    },
    TaggedOrBounded {
        maximum_len: usize,
        bank_size: Option<usize>,
    },
}

pub(super) fn validate_read_policy(policy: &PayloadReadPolicy) -> Result<(), PayloadLoadError> {
    match policy {
        PayloadReadPolicy::Terminated {
            terminator,
            bank_size,
            ..
        }
        | PayloadReadPolicy::TaggedOrTerminated {
            terminator,
            bank_size,
            ..
        } => {
            if terminator.is_empty() {
                return Err(PayloadLoadError::EmptyTerminator);
            }
            validate_bank_size(*bank_size)?;
        }
        PayloadReadPolicy::Bounded { bank_size, .. }
        | PayloadReadPolicy::TaggedOrBounded { bank_size, .. } => {
            validate_bank_size(*bank_size)?;
        }
        PayloadReadPolicy::Tagged
        | PayloadReadPolicy::Fixed { .. }
        | PayloadReadPolicy::TaggedOrFixed { .. } => {}
    }
    Ok(())
}

pub(super) fn bank_remaining(
    payload_offset: usize,
    bank_size: Option<usize>,
) -> Result<usize, PayloadLoadError> {
    match bank_size {
        None => Ok(usize::MAX),
        Some(0) => Err(PayloadLoadError::InvalidBankSize(0)),
        Some(size) => Ok(size - payload_offset % size),
    }
}

fn validate_bank_size(bank_size: Option<usize>) -> Result<(), PayloadLoadError> {
    if bank_size == Some(0) {
        Err(PayloadLoadError::InvalidBankSize(0))
    } else {
        Ok(())
    }
}
