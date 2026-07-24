use std::ops::Range;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ByteDifference {
    pub range: Range<usize>,
}

#[must_use]
pub fn compare_bytes(left: &[u8], right: &[u8]) -> Vec<ByteDifference> {
    let len = left.len().max(right.len());
    let mut result = Vec::new();
    let mut start = None;
    for index in 0..len {
        if left.get(index) != right.get(index) {
            start.get_or_insert(index);
        } else if let Some(begin) = start.take() {
            result.push(ByteDifference {
                range: begin..index,
            });
        }
    }
    if let Some(begin) = start {
        result.push(ByteDifference { range: begin..len });
    }
    result
}

/// Reports changes that fall outside the ranges an operation was expected to own.
#[must_use]
pub fn unexpected_differences(
    left: &[u8],
    right: &[u8],
    allowed: &[Range<usize>],
) -> Vec<ByteDifference> {
    let mut unexpected = Vec::new();
    for difference in compare_bytes(left, right) {
        let mut start = None;
        for index in difference.range.clone() {
            let is_allowed = allowed.iter().any(|range| range.contains(&index));
            if is_allowed {
                if let Some(begin) = start.take() {
                    unexpected.push(ByteDifference {
                        range: begin..index,
                    });
                }
            } else {
                start.get_or_insert(index);
            }
        }
        if let Some(begin) = start {
            unexpected.push(ByteDifference {
                range: begin..difference.range.end,
            });
        }
    }
    unexpected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differences_are_coalesced_and_filterable() {
        let left = [0, 0, 0, 0, 0, 0];
        let right = [0, 1, 2, 0, 4, 0, 5];
        assert_eq!(
            compare_bytes(&left, &right),
            vec![
                ByteDifference { range: 1..3 },
                ByteDifference { range: 4..5 },
                ByteDifference { range: 6..7 },
            ]
        );
        assert_eq!(
            unexpected_differences(&left, &right, &[2..5, 99..100]),
            vec![
                ByteDifference { range: 1..2 },
                ByteDifference { range: 6..7 },
            ]
        );
    }
}
