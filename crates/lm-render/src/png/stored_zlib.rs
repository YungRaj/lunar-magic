use super::PngError;

pub(super) fn zlib_stored(data: &[u8]) -> Result<Vec<u8>, PngError> {
    let mut output = Vec::with_capacity(stored_zlib_capacity(data.len())?);
    output.extend_from_slice(&[0x78, 0x01]);
    if data.is_empty() {
        output.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    } else {
        let mut chunks = data.chunks(usize::from(u16::MAX)).peekable();
        while let Some(block) = chunks.next() {
            output.push(u8::from(chunks.peek().is_none()));
            let len = u16::try_from(block.len()).expect("stored block length is capped");
            output.extend_from_slice(&len.to_le_bytes());
            output.extend_from_slice(&(!len).to_le_bytes());
            output.extend_from_slice(block);
        }
    }
    output.extend_from_slice(&adler32(data).to_be_bytes());
    Ok(output)
}

pub(super) fn stored_zlib_capacity(data_len: usize) -> Result<usize, PngError> {
    let block_len = usize::from(u16::MAX);
    let block_count = if data_len == 0 {
        1
    } else {
        data_len / block_len + usize::from(data_len % block_len != 0)
    };
    data_len
        .checked_add(block_count.checked_mul(5).ok_or(PngError::Overflow)?)
        .and_then(|length| length.checked_add(6))
        .ok_or(PngError::Overflow)
}

pub(super) fn adler32(bytes: &[u8]) -> u32 {
    const MODULUS: u32 = 65_521;
    let mut first = 1_u32;
    let mut second = 0_u32;
    for byte in bytes {
        first = (first + u32::from(*byte)) % MODULUS;
        second = (second + first) % MODULUS;
    }
    second << 16 | first
}
