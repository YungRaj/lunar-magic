//! RATS (`STAR`) tagged-allocation parsing and placement.

mod allocator;
mod header;
mod scanner;

pub use allocator::{
    AllocationError, AllocationOutcome, AllocationPolicy, FreeSpaceAllocator, ProtectedRange,
    find_duplicate,
};
pub use header::{HEADER_LEN, HeaderError, RatsBlock, SIGNATURE, make_header, parse_at};
pub use scanner::scan;
