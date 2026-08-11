//! Platform-neutral SNES ROM image primitives.

mod checksum;
mod error;
mod header;
mod identity;
mod image;
mod ips;
mod lunar_magic_metadata;
mod lunar_magic_metadata_file;
mod mapping;
mod pointer;

pub use checksum::{
    SnesChecksum, additive_checksum, checksum_complement, compute_snes_checksum, mirrored_checksum,
};
pub use error::RomError;
pub use header::{COPIER_HEADER_LEN, CopierHeader, detect_copier_header};
pub use identity::{IdentityError, Region, RomIdentity, SupportedGame, detect_identity};
pub use image::{ChangeRange, RomImage};
pub use ips::{IpsError, MAX_IPS_IMAGE_LEN, MAX_IPS_PATCH_LEN, apply_ips, create_ips};
pub use lunar_magic_metadata::{LunarMagicRomMetadata, LunarMagicRomMetadataError};
pub use lunar_magic_metadata_file::LunarMagicRomMetadataFileError;
pub use mapping::{
    LoRomAddressing, MAPPER_BANK_LEN, Mapper, mapper_supports_image_len, pc_to_snes,
    pc_to_snes_with_lorom_addressing, snes_to_pc,
};
pub use pointer::{PointerTable24, SnesPointer24};
