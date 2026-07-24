//! Lunar Magic and SMW compression primitives.

mod error;
mod interleaved_rle;
mod lz2;
mod lz3;
mod rle;

pub use error::CodecError;
pub use interleaved_rle::{
    DecodedInterleavedRle, decode_interleaved_sized_rle_prefix, encode_interleaved_sized_rle,
};
pub use lz2::{DecodedLz2, decode_lz2, decode_lz2_prefix, encode_lz2, encode_lz2_literals};
pub use lz3::{DecodedLz3, decode_lz3, decode_lz3_prefix, encode_lz3};
pub use rle::{
    DecodedRle, decode_sized_rle, decode_sized_rle_prefix, decode_terminated_rle,
    decode_terminated_rle_prefix, encode_sized_rle, encode_terminated_rle,
};

pub(crate) fn ensure_room(
    output: &[u8],
    additional: usize,
    limit: usize,
) -> Result<(), CodecError> {
    if output
        .len()
        .checked_add(additional)
        .is_none_or(|len| len > limit)
    {
        Err(CodecError::OutputLimitExceeded { limit })
    } else {
        Ok(())
    }
}
