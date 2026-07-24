pub const COPIER_HEADER_LEN: usize = 0x200;
const LOROM_BANK_LEN: usize = 0x8000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CopierHeader {
    Absent,
    Present,
}

impl CopierHeader {
    #[must_use]
    pub(crate) const fn byte_len(self) -> usize {
        match self {
            Self::Absent => 0,
            Self::Present => COPIER_HEADER_LEN,
        }
    }
}

#[must_use]
pub const fn detect_copier_header(file_len: usize) -> CopierHeader {
    if file_len % LOROM_BANK_LEN == COPIER_HEADER_LEN {
        CopierHeader::Present
    } else {
        CopierHeader::Absent
    }
}
