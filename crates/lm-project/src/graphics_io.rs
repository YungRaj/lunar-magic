use crate::{
    LevelLoadError, LevelPointerTable, PayloadLoadError, PayloadPointer, PayloadReadPolicy,
    PayloadSaveError, PayloadSaveRequest, PayloadSaveResult, Project, RatsOwnershipManifest,
};
use lm_codec::{CodecError, decode_lz2_prefix, decode_lz3_prefix, encode_lz2, encode_lz3};
use lm_graphics::{GraphicsFile4bpp, GraphicsFileError};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::{Mapper, RomError, SnesPointer24};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GraphicsCompression {
    #[default]
    Lz2,
    Lz3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphicsRomLayout {
    pub mapper: Mapper,
    pub pointers: LevelPointerTable,
    pub split_pointer_planes: Option<GraphicsPointerPlanes>,
    pub compression: GraphicsCompression,
    pub maximum_compressed_len: usize,
    pub maximum_decompressed_len: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphicsPointerPlanes {
    pub low_offset: usize,
    pub high_offset: usize,
    pub bank_offset: usize,
    pub entries: usize,
    pub stride: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsSaveOptions {
    pub allocation: AllocationPolicy,
    pub previous_block: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Debug)]
pub enum GraphicsIoError {
    Layout(LevelLoadError),
    Load(PayloadLoadError),
    Codec(CodecError),
    Graphics(GraphicsFileError),
    DecompressedLimit { actual: usize, maximum: usize },
    CompressedLimit { actual: usize, maximum: usize },
    ReopenMismatch { slot: usize },
    PointerBounds(RomError),
    Save(PayloadSaveError),
}

impl fmt::Display for GraphicsIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "graphics I/O failed: {self:?}")
    }
}

impl std::error::Error for GraphicsIoError {}

impl From<LevelLoadError> for GraphicsIoError {
    fn from(value: LevelLoadError) -> Self {
        Self::Layout(value)
    }
}
impl From<PayloadLoadError> for GraphicsIoError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}
impl From<CodecError> for GraphicsIoError {
    fn from(value: CodecError) -> Self {
        Self::Codec(value)
    }
}
impl From<GraphicsFileError> for GraphicsIoError {
    fn from(value: GraphicsFileError) -> Self {
        Self::Graphics(value)
    }
}
impl From<PayloadSaveError> for GraphicsIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl From<RomError> for GraphicsIoError {
    fn from(value: RomError) -> Self {
        Self::PointerBounds(value)
    }
}

impl GraphicsRomLayout {
    /// Returns the physical pointer-byte locations for one graphics file.
    ///
    /// # Errors
    ///
    /// Rejects an out-of-range slot, unsafe stride, or address overflow.
    pub fn payload_pointer(self, file_number: usize) -> Result<PayloadPointer, GraphicsIoError> {
        let Some(planes) = self.split_pointer_planes else {
            return Ok(self.pointers.pointer_offset(file_number)?.into());
        };
        if file_number >= planes.entries {
            return Err(LevelLoadError::LevelOutOfRange {
                level: file_number,
                entries: planes.entries,
            }
            .into());
        }
        if planes.stride == 0 {
            return Err(LevelLoadError::InvalidPointerStride(planes.stride).into());
        }
        let displacement = file_number
            .checked_mul(planes.stride)
            .ok_or(LevelLoadError::AddressOverflow)?;
        let add = |base: usize| {
            base.checked_add(displacement)
                .ok_or(LevelLoadError::AddressOverflow)
        };
        Ok(PayloadPointer::SplitBytes {
            low_offset: add(planes.low_offset)?,
            high_offset: add(planes.high_offset)?,
            bank_offset: add(planes.bank_offset)?,
        })
    }

    /// Decodes one pointer from either contiguous or parallel-plane storage.
    ///
    /// # Errors
    ///
    /// Returns pointer-layout, ROM-bounds, or pointer-encoding errors.
    pub fn read_pointer(
        self,
        project: &Project,
        file_number: usize,
    ) -> Result<SnesPointer24, GraphicsIoError> {
        match self.payload_pointer(file_number)? {
            PayloadPointer::Contiguous { offset }
            | PayloadPointer::ContiguousLowBank { offset } => {
                let bytes = project.rom.read(offset, 3)?;
                SnesPointer24::new(
                    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16),
                )
                .map_err(|_| GraphicsIoError::Layout(LevelLoadError::AddressOverflow))
            }
            PayloadPointer::SplitBytes {
                low_offset,
                high_offset,
                bank_offset,
            } => {
                let low = u32::from(project.rom.read(low_offset, 1)?[0]);
                let high = u32::from(project.rom.read(high_offset, 1)?[0]);
                let bank = u32::from(project.rom.read(bank_offset, 1)?[0]);
                SnesPointer24::new(low | (high << 8) | (bank << 16))
                    .map_err(|_| GraphicsIoError::Layout(LevelLoadError::AddressOverflow))
            }
            PayloadPointer::Split { .. } => {
                unreachable!("graphics layouts do not emit split words")
            }
        }
    }
}

impl Project {
    /// Loads, decompresses, and decodes one native 4bpp graphics file.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsIoError`] for invalid table entries, mapping, compression, or tile data.
    pub fn load_graphics_file(
        &self,
        file_number: usize,
        layout: GraphicsRomLayout,
    ) -> Result<GraphicsFile4bpp, GraphicsIoError> {
        let payload = self.load_payload_from_pointer(
            layout.read_pointer(self, file_number)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrBounded {
                maximum_len: layout.maximum_compressed_len,
                // Vanilla SMW contains compressed graphics streams that cross a LoROM bank.
                // The explicit compressed-size ceiling remains the corruption boundary.
                bank_size: None,
            },
        )?;
        let (decoded, consumed) = match layout.compression {
            GraphicsCompression::Lz2 => {
                let value = decode_lz2_prefix(&payload.bytes, layout.maximum_decompressed_len)?;
                (value.bytes, value.consumed)
            }
            GraphicsCompression::Lz3 => {
                let value = decode_lz3_prefix(&payload.bytes, layout.maximum_decompressed_len)?;
                (value.bytes, value.consumed)
            }
        };
        if payload.block.is_some() && consumed != payload.bytes.len() {
            return Err(GraphicsIoError::Codec(CodecError::TrailingCompressedData(
                payload.bytes.len() - consumed,
            )));
        }
        Ok(GraphicsFile4bpp::decode(&decoded)?)
    }

    /// Compresses, allocates, and repoints one native 4bpp graphics file transactionally.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsIoError`] for table bounds, compressed limits, allocation, or mapping.
    pub fn save_graphics_file(
        &mut self,
        file_number: usize,
        graphics: &GraphicsFile4bpp,
        layout: GraphicsRomLayout,
        options: &GraphicsSaveOptions,
    ) -> Result<PayloadSaveResult, GraphicsIoError> {
        Ok(self.save_tagged_payload(&graphics_save_request(
            file_number,
            graphics,
            layout,
            options,
        )?)?)
    }

    /// Saves a graphics file and repairs the SNES checksum in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsIoError`] when encoding, allocation, mapping, or checksum repair fails.
    pub fn save_graphics_file_with_checksum(
        &mut self,
        file_number: usize,
        graphics: &GraphicsFile4bpp,
        layout: GraphicsRomLayout,
        checksum_field: usize,
        options: &GraphicsSaveOptions,
    ) -> Result<PayloadSaveResult, GraphicsIoError> {
        let request = graphics_save_request(file_number, graphics, layout, options)?;
        Ok(self
            .save_tagged_payloads_with_checksum(
                &request.description,
                std::slice::from_ref(&request),
                checksum_field,
            )?
            .remove(0))
    }

    /// Saves, reclaims the exactly owned displaced block, and repairs checksum in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsIoError`] for graphics encoding failures, non-exact or stale ownership,
    /// unsafe reclamation overlap, allocation/mapping failure, or checksum failure. No mutation is
    /// committed unless every stage succeeds.
    pub fn save_graphics_file_with_checksum_and_reclamation(
        &mut self,
        file_number: usize,
        graphics: &GraphicsFile4bpp,
        layout: GraphicsRomLayout,
        checksum_field: usize,
        options: &GraphicsSaveOptions,
        manifest: &RatsOwnershipManifest,
    ) -> Result<PayloadSaveResult, GraphicsIoError> {
        let request = graphics_save_request(file_number, graphics, layout, options)?;
        Ok(self
            .save_tagged_payloads_with_checksum_and_reclamation(
                &request.description,
                std::slice::from_ref(&request),
                checksum_field,
                manifest,
            )?
            .remove(0))
    }
}

fn graphics_save_request(
    file_number: usize,
    graphics: &GraphicsFile4bpp,
    layout: GraphicsRomLayout,
    options: &GraphicsSaveOptions,
) -> Result<PayloadSaveRequest, GraphicsIoError> {
    let decoded = graphics.encode()?;
    if decoded.len() > layout.maximum_decompressed_len {
        return Err(GraphicsIoError::DecompressedLimit {
            actual: decoded.len(),
            maximum: layout.maximum_decompressed_len,
        });
    }
    let payload = match layout.compression {
        GraphicsCompression::Lz2 => encode_lz2(&decoded),
        GraphicsCompression::Lz3 => encode_lz3(&decoded),
    };
    if payload.len() > layout.maximum_compressed_len {
        return Err(GraphicsIoError::CompressedLimit {
            actual: payload.len(),
            maximum: layout.maximum_compressed_len,
        });
    }
    Ok(PayloadSaveRequest {
        description: format!("save graphics file {file_number:02x}"),
        payload,
        pointer: layout.payload_pointer(file_number)?,
        mapper: layout.mapper,
        allocation_policy: options.allocation.clone(),
        previous_block: options.previous_block.clone(),
        reuse_identical: options.reuse_identical,
        maximum_payload_len: layout.maximum_compressed_len,
        erase_fill: options.erase_fill,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::IndexedTile;
    use lm_rats::ProtectedRange;
    use lm_rom::RomImage;

    fn layout() -> GraphicsRomLayout {
        GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x20,
                entries: 4,
                stride: 3,
            },
            split_pointer_planes: None,
            compression: GraphicsCompression::Lz2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x10000,
        }
    }

    fn options() -> GraphicsSaveOptions {
        GraphicsSaveOptions {
            allocation: AllocationPolicy {
                search: 0x100..0x8000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![ProtectedRange(0x20..0x2c)],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    fn graphics() -> GraphicsFile4bpp {
        GraphicsFile4bpp {
            tiles: vec![IndexedTile::new(std::array::from_fn(|index| {
                index.to_le_bytes()[0] & 0x0f
            }))],
        }
    }

    fn split_layout() -> GraphicsRomLayout {
        GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x20,
                entries: 4,
                stride: 1,
            },
            split_pointer_planes: Some(GraphicsPointerPlanes {
                low_offset: 0x20,
                high_offset: 0x24,
                bank_offset: 0x28,
                entries: 4,
                stride: 1,
            }),
            compression: GraphicsCompression::Lz2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x10000,
        }
    }

    #[test]
    fn saves_loads_and_undoes_a_graphics_file() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        project
            .save_graphics_file(2, &graphics(), layout(), &options())
            .unwrap();
        assert_eq!(project.load_graphics_file(2, layout()).unwrap(), graphics());
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn loads_an_untagged_compressed_file() {
        let encoded = encode_lz2(&graphics().encode().unwrap());
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x81, 0x80]);
        bytes[0x100..0x100 + encoded.len()].copy_from_slice(&encoded);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert_eq!(project.load_graphics_file(0, layout()).unwrap(), graphics());
    }

    #[test]
    fn split_pointer_planes_load_save_and_undo_atomically() {
        let encoded = encode_lz2(&graphics().encode().unwrap());
        let mut bytes = vec![0xff; 0x8000];
        bytes[0x20] = 0x00;
        bytes[0x24] = 0x81;
        bytes[0x28] = 0x80;
        bytes[0x100..0x100 + encoded.len()].copy_from_slice(&encoded);
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert_eq!(
            project.load_graphics_file(0, split_layout()).unwrap(),
            graphics()
        );

        let original = project.save_snapshot();
        project
            .save_graphics_file(2, &graphics(), split_layout(), &options())
            .unwrap();
        let pointer = split_layout().read_pointer(&project, 2).unwrap();
        assert_eq!(project.rom.read(0x22, 1).unwrap()[0], pointer.encode()[0]);
        assert_eq!(project.rom.read(0x26, 1).unwrap()[0], pointer.encode()[1]);
        assert_eq!(project.rom.read(0x2a, 1).unwrap()[0], pointer.encode()[2]);
        assert_eq!(
            project.load_graphics_file(2, split_layout()).unwrap(),
            graphics()
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn saves_loads_and_undoes_an_lz3_graphics_file() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let mut lz3 = layout();
        lz3.compression = GraphicsCompression::Lz3;
        project
            .save_graphics_file(2, &graphics(), lz3, &options())
            .unwrap();
        assert_eq!(project.load_graphics_file(2, lz3).unwrap(), graphics());
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn tagged_compressed_file_must_consume_its_complete_owned_payload() {
        let mut payload = encode_lz2(&graphics().encode().unwrap());
        payload.extend_from_slice(&[0xaa, 0xbb]);
        let mut bytes = vec![0xff; 0x8000];
        let block = lm_rats::FreeSpaceAllocator::new(
            &mut bytes,
            AllocationPolicy {
                search: 0x100..0x8000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![ProtectedRange(0x20..0x2c)],
            },
        )
        .allocate(&payload)
        .unwrap();
        let pointer = lm_rom::pc_to_snes(Mapper::LoRom, block.payload.start)
            .unwrap()
            .to_le_bytes();
        bytes[0x20..0x23].copy_from_slice(&pointer[..3]);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert!(matches!(
            project.load_graphics_file(0, layout()),
            Err(GraphicsIoError::Codec(CodecError::TrailingCompressedData(
                2
            )))
        ));
    }

    #[test]
    fn decompressed_limit_failure_is_atomic() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let mut too_small = layout();
        too_small.maximum_decompressed_len = 31;
        assert!(matches!(
            project.save_graphics_file(0, &graphics(), too_small, &options()),
            Err(GraphicsIoError::DecompressedLimit {
                actual: 32,
                maximum: 31
            })
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }

    #[test]
    fn invalid_pixel_failure_is_atomic() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let invalid = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([16; IndexedTile::PIXEL_COUNT])],
        };
        assert!(matches!(
            project.save_graphics_file(0, &invalid, layout(), &options()),
            Err(GraphicsIoError::Graphics(
                GraphicsFileError::PixelOutOfRange {
                    tile: 0,
                    pixel: 0,
                    value: 16,
                }
            ))
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }
}
