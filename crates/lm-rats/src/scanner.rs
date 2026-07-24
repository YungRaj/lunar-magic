use crate::{HEADER_LEN, RatsBlock, SIGNATURE, parse_at};

#[must_use]
pub fn scan(bytes: &[u8]) -> Vec<RatsBlock> {
    let mut blocks = Vec::new();
    let mut cursor = 0;
    while cursor + HEADER_LEN <= bytes.len() {
        let Some(relative) = bytes[cursor..]
            .windows(4)
            .position(|window| window == SIGNATURE)
        else {
            break;
        };
        let offset = cursor + relative;
        match parse_at(bytes, offset) {
            Ok(block) => {
                cursor = block.payload.end;
                blocks.push(block);
            }
            Err(_) => cursor = offset + 1,
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::make_header;

    #[test]
    fn ignores_false_signatures() {
        let mut bytes = b"STAR\0\0\0\0padding".to_vec();
        let offset = bytes.len();
        bytes.extend_from_slice(&make_header(1).unwrap());
        bytes.push(7);
        assert_eq!(
            scan(&bytes),
            vec![RatsBlock {
                header_offset: offset,
                payload: offset + 8..offset + 9
            }]
        );
    }
}
