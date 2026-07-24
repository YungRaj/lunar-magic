#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedTableEncodingError {
    pub records: usize,
    pub record_len: usize,
}

impl std::fmt::Display for FixedTableEncodingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "fixed overworld table size overflow: {self:?}")
    }
}

impl std::error::Error for FixedTableEncodingError {}

pub(crate) fn checked_table_len(
    records: usize,
    record_len: usize,
) -> Result<usize, FixedTableEncodingError> {
    records
        .checked_mul(record_len)
        .ok_or(FixedTableEncodingError {
            records,
            record_len,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_and_message_boundaries_never_saturate() {
        for record_len in [5, 144] {
            let maximum = usize::MAX / record_len;
            assert_eq!(
                checked_table_len(maximum, record_len).unwrap(),
                maximum * record_len
            );
            assert_eq!(
                checked_table_len(maximum + 1, record_len),
                Err(FixedTableEncodingError {
                    records: maximum + 1,
                    record_len,
                })
            );
        }
    }
}
